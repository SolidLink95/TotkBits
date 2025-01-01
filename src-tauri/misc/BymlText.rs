use base64::Engine;
use ryml::{NodeRef, Tree};

use super::*;
use crate::{yaml::*, Error, Result};

impl Byml {
    /// Parse BYML document from YAML text.
    pub fn from_text(text: impl AsRef<str>) -> Result<Byml> {
        Parser::new(text.as_ref())?.parse()
    }

    /// Serialize the document to YAML. This can only be done for Null, Array,
    /// or Hash nodes.
    pub fn to_text(&self) -> std::string::String {
        Emitter::new(self)
            .emit()
            .expect("BYML must be container or null to serialize")
    }
}

#[inline]
fn is_binary_tag(tag: &str) -> bool {
    tag == "tag:yaml.org,2002:binary" || tag == "!!binary"
}

#[inline]
fn recognize_tag(tag: &str) -> Option<TagBasedType> {
    match tag {
        "!f64" => Some(TagBasedType::Float),
        "!u" | "!l" | "!ul" => Some(TagBasedType::Int),
        "tag:yaml.org,2002:binary" | "!!binary" | "!!file" => Some(TagBasedType::Str),
        _ => None,
    }
}

struct Parser<'a>(Tree<'a>);

impl<'a> Parser<'a> {
    fn new(text: &str) -> Result<Self> {
        Ok(Self(Tree::parse(text)?))
    }

    fn parse_node(node: NodeRef<'a, '_, '_, &Tree<'a>>) -> Result<Byml> {
        if node.is_map()? {
            match node.val_tag().unwrap_or("") {
                "!h" => {
                    Ok(Byml::HashMap(
                        node.iter()?
                            .map(|child| {
                                let key = child.key()?.parse().map_err(|_| {
                                    Error::Any("Expected integer hash key".to_owned())
                                })?;
                                let value = Self::parse_node(child.clone())?;
                                Ok((key, value))
                            })
                            .collect::<Result<_>>()?,
                    ))
                }
                "!vh" => {
                    Ok(Byml::ValueHashMap(
                        node.iter()?
                            .map(|child| {
                                let key = child.key()?.parse().map_err(|_| {
                                    Error::Any("Expected integer hash key".to_owned())
                                })?;
                                let value = Self::parse_node(child.clone())?;
                                Ok((key, (value, 0)))
                            })
                            .collect::<Result<_>>()?,
                    ))
                }
                _ => {
                    Ok(Byml::Map(
                        node.iter()?
                            .map(|child| {
                                let key = child.key()?;
                                let value = Self::parse_node(child.clone())?;
                                Ok((key.into(), value))
                            })
                            .collect::<Result<_>>()?,
                    ))
                }
            }
        } else if node.is_seq()? {
            Ok(Byml::Array(
                node.iter()?
                    .map(|child| Self::parse_node(child.clone()))
                    .collect::<Result<_>>()?,
            ))
        } else {
            let tag = node.val_tag().unwrap_or("");
            let tag_type = get_tag_based_type(tag).or_else(|| recognize_tag(tag));
            let scalar = parse_scalar(tag_type, node.val()?, node.is_quoted()?)?;
            match scalar {
                Scalar::Bool(b) => Ok(Byml::Bool(b)),
                Scalar::Float(f) => {
                    match tag {
                        "!f64" => Ok(Byml::Double(f)),
                        _ => Ok(Byml::Float(f as f32)),
                    }
                }
                Scalar::Int(i) => {
                    match tag {
                        "!u" => Ok(Byml::U32(i as u32)),
                        "!ul" => Ok(Byml::U64(i as u64)),
                        "!l" => Ok(Byml::I64(i as i64)),
                        _ => Ok(Byml::I32(i as i32)),
                    }
                }
                Scalar::Null => Ok(Byml::Null),
                Scalar::String(s) => {
                    if is_binary_tag(tag) {
                        Ok(Byml::BinaryData(
                            base64::engine::general_purpose::STANDARD.decode(s)?,
                        ))
                    } else if tag == "!!file" {
                        Ok(Byml::FileData(
                            base64::engine::general_purpose::STANDARD.decode(s)?,
                        ))
                    } else {
                        Ok(Byml::String(s))
                    }
                }
            }
        }
    }

    fn parse(self) -> Result<Byml> {
        let root = self.0.root_ref()?;
        Self::parse_node(root)
    }
}

#[inline(always)]
fn should_use_inline(byml: &Byml) -> bool {
    let is_simple = |by: &Byml| !matches!(by, Byml::Array(_) | Byml::Map(_));
    match byml {
        Byml::Array(arr) => arr.len() < 10 && arr.iter().all(is_simple),
        Byml::Map(hash) => hash.len() < 10 && hash.iter().all(|(_, v)| is_simple(v)),
        _ => false,
    }
}

struct Emitter<'a, 'b>(&'a Byml, Tree<'b>);

impl<'a, 'b> Emitter<'a, 'b> {
    fn new(byml: &'a Byml) -> Self {
        let mut tree = Tree::default();
        tree.reserve(20000);
        Self(byml, tree)
    }

    fn build_node<'e>(
        byml: &Byml,
        mut dest_node: NodeRef<'b, 'e, '_, &'e mut Tree<'b>>,
    ) -> Result<()> {
        match byml {
            Byml::Array(array) => {
                if should_use_inline(byml) {
                    dest_node.change_type(ryml::NodeType::Seq | ryml::NodeType::WipStyleFlowSl)?;
                } else {
                    dest_node.change_type(ryml::NodeType::Seq)?;
                }
                for item in array {
                    let node = dest_node.append_child()?;
                    Self::build_node(item, node)?;
                }
            }
            Byml::Map(hash) => {
                if should_use_inline(byml) {
                    dest_node.change_type(ryml::NodeType::Map | ryml::NodeType::WipStyleFlowSl)?;
                } else {
                    dest_node.change_type(ryml::NodeType::Map)?;
                }
                let mut map_items = hash.iter().collect::<Vec<_>>();
                map_items.sort_by(|a, b| a.0.cmp(b.0));
                for (key, value) in map_items {
                    let mut node = dest_node.append_child()?;
                    node.set_key(key)?;
                    if string_needs_quotes(key) {
                        let flags = node.node_type()?;
                        node.set_type_flags(flags | ryml::NodeType::WipKeySquo)?;
                    }
                    Self::build_node(value, node)?;
                }
            }
            Byml::HashMap(hash) => {
                if should_use_inline(byml) {
                    dest_node.change_type(ryml::NodeType::Map | ryml::NodeType::WipStyleFlowSl)?;
                } else {
                    dest_node.change_type(ryml::NodeType::Map)?;
                }
                let mut map_items = hash.iter().collect::<Vec<_>>();
                map_items.sort_by(|a, b| a.0.cmp(b.0));
                for (key, value) in map_items {
                    let mut node = dest_node.append_child()?;
                    node.set_key(&key.to_string())?;
                    Self::build_node(value, node)?;
                }
                dest_node.set_val_tag("!h")?;
            }
            Byml::ValueHashMap(hash) => {
                if should_use_inline(byml) {
                    dest_node.change_type(ryml::NodeType::Map | ryml::NodeType::WipStyleFlowSl)?;
                } else {
                    dest_node.change_type(ryml::NodeType::Map)?;
                }
                let mut map_items = hash.iter().collect::<Vec<_>>();
                map_items.sort_by(|a, b| a.0.cmp(b.0));
                for (key, (value, _)) in map_items {
                    let mut node = dest_node.append_child()?;
                    node.set_key(&key.to_string())?;
                    Self::build_node(value, node)?;
                }
                dest_node.set_val_tag("!vh")?;
            }
            scalar => {
                match scalar {
                    Byml::String(s) => {
                        dest_node.set_val(s)?;
                        if string_needs_quotes(s) {
                            let flags = dest_node.node_type()?;
                            dest_node.set_type_flags(flags | ryml::NodeType::WipValDquo)?;
                        }
                    }
                    Byml::Bool(b) => dest_node.set_val(if *b { "true" } else { "false" })?,
                    Byml::Float(f) => dest_node.set_val(&write_float(*f as f64)?)?,
                    Byml::Double(d) => {
                        dest_node.set_val(&write_float(*d)?)?;
                        dest_node.set_val_tag("!f64")?;
                    }
                    Byml::I32(i) => dest_node.set_val(&lexical::to_string(*i))?,
                    Byml::I64(i) => {
                        dest_node.set_val(&lexical::to_string(*i))?;
                        dest_node.set_val_tag("!l")?;
                    }
                    Byml::U32(u) => {
                        dest_node.set_val(&format_hex!(u))?;
                        dest_node.set_val_tag("!u")?;
                    }
                    Byml::U64(u) => {
                        dest_node.set_val(&format_hex!(u))?;
                        dest_node.set_val_tag("!ul")?;
                    }
                    Byml::Null => dest_node.set_val("null")?,
                    Byml::BinaryData(data) => {
                        let arena = dest_node.tree().arena_capacity();
                        dest_node.tree_mut().reserve_arena(arena + data.len());
                        dest_node
                            .set_val(&base64::engine::general_purpose::STANDARD.encode(data))?;
                        dest_node.set_val_tag("!!binary")?;
                    }
                    Byml::FileData(data) => {
                        let arena = dest_node.tree().arena_capacity();
                        dest_node.tree_mut().reserve_arena(arena + data.len());
                        dest_node
                            .set_val(&base64::engine::general_purpose::STANDARD.encode(data))?;
                        dest_node.set_val_tag("!!file")?;
                    }
                    _ => unsafe { std::hint::unreachable_unchecked() },
                }
            }
        }
        Ok(())
    }

    fn emit(self) -> Result<std::string::String> {
        let Self(byml, mut tree) = self;
        match byml {
            Byml::Map(_) | Byml::HashMap(_) | Byml::ValueHashMap(_) => tree.to_map(0)?,
            Byml::Array(_) => tree.to_seq(0)?,
            Byml::Null => return Ok("null".to_string()),
            _ => {
                return Err(Error::Any(
                    "Can only serialize Hash, Array, or Null nodes to YAML".into(),
                ));
            }
        };
        Self::build_node(byml, tree.root_ref_mut()?)?;
        Ok(tree.emit()?)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn from_text() {
        for file in crate::byml::FILES {
            println!("{}", file);
            let text = std::fs::read_to_string(
                std::path::Path::new("test/byml").join([file, ".yml"].join("")),
            )
            .unwrap();
            let byml = Byml::from_text(text).unwrap();
            let bytes =
                std::fs::read(std::path::Path::new("test/byml").join([file, ".byml"].join("")))
                    .unwrap();
            let binary_byml = Byml::from_binary(bytes).unwrap();
            assert_eq!(byml, binary_byml);
        }
    }

    #[test]
    fn text_roundtrip() {
        for file in crate::byml::FILES {
            println!("{}", file);
            let text = std::fs::read_to_string(
                std::path::Path::new("test/byml").join([file, ".yml"].join("")),
            )
            .unwrap();
            let byml = Byml::from_text(text).unwrap();
            let text = byml.to_text();
            println!("{}", &text);
            let byml = Byml::from_text(text).unwrap();
            assert_eq!(byml, byml);
        }
    }

    #[test]
    fn negative_zero() {
        let text = "Test: [0.0, -0.0, 1.2]";
        let byml = Byml::from_text(text).unwrap();
        let text2 = byml.to_text();
        assert!(text2.contains("0.0") && text2.contains("-0.0"));
    }
}
