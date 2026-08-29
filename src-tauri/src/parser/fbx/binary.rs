//! Minimal binary FBX 7.4 serializer.
//!
//! Every attribute is written with an explicit type tag so the output never
//! depends on a text round-trip guessing whether `1` was meant as an int, a
//! double or a bool. The record layout follows the FBX SDK: a 13-byte node
//! header, the property list, nested records, and a 13-byte null sentinel
//! after the children (or in place of them when a node has no properties).

use std::io::{self, Write};

pub const VERSION_7400: u32 = 7400;

/// `FileId`, `CreationTime` and the first footer block are cryptographically
/// tied together by the FBX SDK. These are the values Blender's exporter ships
/// with, which every SDK based importer accepts.
pub const FILE_ID: [u8; 16] = [
    0x28, 0xb3, 0x2a, 0xeb, 0xb6, 0x24, 0xcc, 0xc2, 0xbf, 0xc8, 0xb0, 0x2a, 0xa9, 0x2b, 0xfc, 0xf1,
];
pub const CREATION_TIME: &str = "1970-01-01 10:00:00:000";
const FOOTER_ID: [u8; 16] = [
    0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e,
];
const FOOTER_MAGIC: [u8; 16] = [
    0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29, 0x0b,
];
const HEAD_MAGIC: &[u8] = b"Kaydara FBX Binary  \x00\x1a\x00";
const SENTINEL: [u8; 13] = [0; 13];

#[derive(Debug, Clone)]
pub enum Attr {
    Bool(bool),
    I32(i32),
    I64(i64),
    F64(f64),
    Str(String),
    Raw(Vec<u8>),
    ArrI32(Vec<i32>),
    ArrF64(Vec<f64>),
}

impl From<bool> for Attr {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}
impl From<i32> for Attr {
    fn from(value: i32) -> Self {
        Self::I32(value)
    }
}
impl From<i64> for Attr {
    fn from(value: i64) -> Self {
        Self::I64(value)
    }
}
impl From<f64> for Attr {
    fn from(value: f64) -> Self {
        Self::F64(value)
    }
}
impl From<&str> for Attr {
    fn from(value: &str) -> Self {
        Self::Str(value.to_owned())
    }
}
impl From<String> for Attr {
    fn from(value: String) -> Self {
        Self::Str(value)
    }
}
impl From<Vec<i32>> for Attr {
    fn from(value: Vec<i32>) -> Self {
        Self::ArrI32(value)
    }
}
impl From<Vec<f64>> for Attr {
    fn from(value: Vec<f64>) -> Self {
        Self::ArrF64(value)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<Attr>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ..Self::default()
        }
    }

    pub fn leaf(name: &str, value: impl Into<Attr>) -> Self {
        Self::new(name).attr(value)
    }

    pub fn attr(mut self, value: impl Into<Attr>) -> Self {
        self.attrs.push(value.into());
        self
    }

    pub fn child(mut self, node: Node) -> Self {
        self.children.push(node);
        self
    }

    pub fn push(&mut self, node: Node) -> &mut Self {
        self.children.push(node);
        self
    }

    pub fn extend(&mut self, nodes: impl IntoIterator<Item = Node>) -> &mut Self {
        self.children.extend(nodes);
        self
    }

    /// Object header attributes: `id, "Name\0\x01Class", "SubClass"`.
    pub fn object(kind: &str, id: i64, name: &str, class: &str, subclass: &str) -> Self {
        Self::new(kind)
            .attr(id)
            .attr(format!("{name}\0\u{1}{class}"))
            .attr(subclass)
    }
}

/// Serializes top-level nodes into a complete FBX binary file.
pub fn write_document(nodes: &[Node]) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    out.write_all(HEAD_MAGIC)?;
    out.write_all(&VERSION_7400.to_le_bytes())?;
    for node in nodes {
        write_node(&mut out, node)?;
    }
    out.write_all(&SENTINEL)?;
    out.write_all(&FOOTER_ID)?;
    out.write_all(&[0; 4])?;
    let mut pad = (16 - out.len() % 16) % 16;
    if pad == 0 {
        pad = 16;
    }
    out.write_all(&vec![0; pad])?;
    out.write_all(&VERSION_7400.to_le_bytes())?;
    out.write_all(&[0; 120])?;
    out.write_all(&FOOTER_MAGIC)?;
    Ok(out)
}

fn write_node(out: &mut Vec<u8>, node: &Node) -> io::Result<()> {
    if node.name.len() > u8::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("FBX node name too long: {}", node.name),
        ));
    }
    let start = out.len();
    out.write_all(&[0; 12])?;
    out.push(node.name.len() as u8);
    out.write_all(node.name.as_bytes())?;
    let props_start = out.len();
    for attr in &node.attrs {
        write_attr(out, attr)?;
    }
    let props_len = out.len() - props_start;
    for child in &node.children {
        write_node(out, child)?;
    }
    if !node.children.is_empty() || node.attrs.is_empty() {
        out.write_all(&SENTINEL)?;
    }
    let end = out.len();
    let header = [
        u32::try_from(end).map_err(|_| io::Error::other("FBX file exceeds 4 GiB"))?,
        node.attrs.len() as u32,
        props_len as u32,
    ];
    for (index, value) in header.iter().enumerate() {
        out[start + index * 4..start + index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn write_attr(out: &mut Vec<u8>, attr: &Attr) -> io::Result<()> {
    match attr {
        Attr::Bool(value) => {
            out.push(b'C');
            out.push(u8::from(*value));
        }
        Attr::I32(value) => {
            out.push(b'I');
            out.write_all(&value.to_le_bytes())?;
        }
        Attr::I64(value) => {
            out.push(b'L');
            out.write_all(&value.to_le_bytes())?;
        }
        Attr::F64(value) => {
            out.push(b'D');
            out.write_all(&value.to_le_bytes())?;
        }
        Attr::Str(value) => {
            out.push(b'S');
            out.write_all(&(value.len() as u32).to_le_bytes())?;
            out.write_all(value.as_bytes())?;
        }
        Attr::Raw(value) => {
            out.push(b'R');
            out.write_all(&(value.len() as u32).to_le_bytes())?;
            out.write_all(value)?;
        }
        Attr::ArrI32(values) => {
            out.push(b'i');
            write_array_header(out, values.len(), 4)?;
            for value in values {
                out.write_all(&value.to_le_bytes())?;
            }
        }
        Attr::ArrF64(values) => {
            out.push(b'd');
            write_array_header(out, values.len(), 8)?;
            for value in values {
                out.write_all(&value.to_le_bytes())?;
            }
        }
    }
    Ok(())
}

fn write_array_header(out: &mut Vec<u8>, count: usize, element_size: usize) -> io::Result<()> {
    out.write_all(&(count as u32).to_le_bytes())?;
    out.write_all(&0u32.to_le_bytes())?;
    out.write_all(&((count * element_size) as u32).to_le_bytes())?;
    Ok(())
}

// ---- Properties70 helpers -------------------------------------------------

fn property(name: &str, kind: &str, label: &str, flags: &str) -> Node {
    Node::new("P").attr(name).attr(kind).attr(label).attr(flags)
}

pub fn p_int(name: &str, value: i32) -> Node {
    property(name, "int", "Integer", "").attr(value)
}
pub fn p_enum(name: &str, value: i32) -> Node {
    property(name, "enum", "", "").attr(value)
}
pub fn p_bool(name: &str, value: bool) -> Node {
    property(name, "bool", "", "").attr(i32::from(value))
}
pub fn p_double(name: &str, value: f64) -> Node {
    property(name, "double", "Number", "").attr(value)
}
pub fn p_number(name: &str, value: f64) -> Node {
    property(name, "Number", "", "A").attr(value)
}
pub fn p_string(name: &str, value: &str) -> Node {
    property(name, "KString", "", "").attr(value)
}
pub fn p_url(name: &str, value: &str) -> Node {
    property(name, "KString", "Url", "").attr(value)
}
pub fn p_xref_url(name: &str, value: &str) -> Node {
    property(name, "KString", "XRefUrl", "").attr(value)
}
pub fn p_datetime(name: &str, value: &str) -> Node {
    property(name, "DateTime", "", "").attr(value)
}
pub fn p_compound(name: &str) -> Node {
    property(name, "Compound", "", "")
}
pub fn p_object(name: &str) -> Node {
    property(name, "object", "", "")
}
pub fn p_ktime(name: &str, value: i64) -> Node {
    property(name, "KTime", "Time", "").attr(value)
}
pub fn p_color(name: &str, value: [f64; 3]) -> Node {
    property(name, "Color", "", "A")
        .attr(value[0])
        .attr(value[1])
        .attr(value[2])
}
pub fn p_color_rgb(name: &str, value: [f64; 3]) -> Node {
    property(name, "ColorRGB", "Color", "")
        .attr(value[0])
        .attr(value[1])
        .attr(value[2])
}
pub fn p_vector(name: &str, value: [f64; 3]) -> Node {
    property(name, "Vector3D", "Vector", "")
        .attr(value[0])
        .attr(value[1])
        .attr(value[2])
}
/// `Lcl Translation` / `Lcl Rotation` / `Lcl Scaling`: the type name equals the
/// property name and the property is animatable.
pub fn p_lcl(name: &str, value: [f64; 3]) -> Node {
    property(name, name, "", "A")
        .attr(value[0])
        .attr(value[1])
        .attr(value[2])
}
pub fn p_visibility(value: f64) -> Node {
    property("Visibility", "Visibility", "", "A").attr(value)
}
pub fn p_visibility_inheritance(value: i32) -> Node {
    property("Visibility Inheritance", "Visibility Inheritance", "", "").attr(value)
}

pub fn properties70(nodes: impl IntoIterator<Item = Node>) -> Node {
    let mut node = Node::new("Properties70");
    node.extend(nodes);
    node
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn writes_sdk_record_layout() {
        let doc = write_document(&[
            Node::new("Empty"),
            Node::leaf("Leaf", 7i32),
            Node::new("Parent").child(Node::leaf("Child", "x")),
        ])
        .unwrap();
        assert!(doc.starts_with(HEAD_MAGIC));
        assert_eq!(read_u32(&doc, 23), VERSION_7400);
        // Empty: header(13) + name(5) + sentinel(13)
        let empty_end = read_u32(&doc, 27) as usize;
        assert_eq!(empty_end, 27 + 13 + 5 + 13);
        assert_eq!(&doc[empty_end - 13..empty_end], &SENTINEL);
        // Leaf: header(13) + name(4) + 'I' + 4 bytes, no sentinel
        let leaf_end = read_u32(&doc, empty_end) as usize;
        assert_eq!(leaf_end, empty_end + 13 + 4 + 5);
        assert_eq!(read_u32(&doc, empty_end + 4), 1);
        assert_eq!(read_u32(&doc, empty_end + 8), 5);
        assert_eq!(doc[empty_end + 13 + 4], b'I');
        // Parent has a trailing sentinel after its child.
        let parent_end = read_u32(&doc, leaf_end) as usize;
        assert_eq!(&doc[parent_end - 13..parent_end], &SENTINEL);
        // File level sentinel, then the footer ends with the version + magic.
        assert_eq!(&doc[parent_end..parent_end + 13], &SENTINEL);
        assert_eq!(&doc[doc.len() - 16..], &FOOTER_MAGIC);
        assert_eq!(read_u32(&doc, doc.len() - 16 - 120 - 4), VERSION_7400);
        let footer_start = parent_end + 13;
        assert_eq!(&doc[footer_start..footer_start + 16], &FOOTER_ID);
        // The version block is 16-byte aligned like the SDK requires.
        assert_eq!((doc.len() - 16 - 120 - 4) % 16, 0);
    }

    #[test]
    fn encodes_arrays_and_object_names() {
        let doc = write_document(&[Node::object("Model", 5, "Bone", "Model", "LimbNode")
            .child(Node::leaf("Indexes", vec![1i32, 2]))
            .child(Node::leaf("Weights", vec![0.5f64]))])
        .unwrap();
        let body = &doc[27 + 13 + 5..];
        assert_eq!(body[0], b'L');
        assert_eq!(&body[9..14], b"S\x0b\x00\x00\x00");
        assert_eq!(&body[14..25], b"Bone\x00\x01Model");
        let indexes = doc.windows(8).position(|w| w == b"Indexesi").unwrap() + 8;
        assert_eq!(read_u32(&doc, indexes), 2);
        assert_eq!(read_u32(&doc, indexes + 4), 0);
        assert_eq!(read_u32(&doc, indexes + 8), 8);
    }
}
