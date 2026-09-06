//! A small AAMP (version 2) tree reader and writer for the helper-bone and
//! support-bone sidecars. Parameters keep their raw payload and type code, so
//! hashes the editors do not understand survive a rebuild untouched.

use crate::parser::binary::{BinaryPatcher, BinaryReader, BinaryWriter};
use std::io::{self, ErrorKind};

pub const KIND_BOOL: u8 = 0;
pub const KIND_F32: u8 = 1;
pub const KIND_I32: u8 = 2;
pub const KIND_VEC2: u8 = 3;
pub const KIND_VEC3: u8 = 4;
pub const KIND_VEC4: u8 = 5;
pub const KIND_COLOR: u8 = 6;
pub const KIND_STRING32: u8 = 7;
pub const KIND_STRING64: u8 = 8;
pub const KIND_STRING256: u8 = 15;
pub const KIND_QUAT: u8 = 16;
pub const KIND_U32: u8 = 17;
pub const KIND_STRING_REF: u8 = 20;

const HEADER_SIZE: usize = 0x30;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AampParameter {
    pub hash: u32,
    pub kind: u8,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AampObject {
    pub hash: u32,
    pub parameters: Vec<AampParameter>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AampList {
    pub hash: u32,
    pub children: Vec<AampList>,
    pub objects: Vec<AampObject>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AampTree {
    pub archive_type: String,
    pub archive_version: u32,
    pub format_version: u32,
    pub parameter_io_version: u32,
    pub root: AampList,
}

/// The CRC-32 Nintendo hashes AAMP names with.
pub fn crc32(value: &str) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in value.bytes() {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 0 { 0 } else { 0xedb8_8320 };
        }
    }
    !crc
}

impl AampParameter {
    pub fn raw(hash: u32, kind: u8, bytes: Vec<u8>) -> Self {
        Self { hash, kind, bytes }
    }
    pub fn int(hash: u32, value: i32) -> Self {
        Self::raw(hash, KIND_I32, encode_i32(value))
    }
    pub fn float(hash: u32, value: f32) -> Self {
        Self::raw(hash, KIND_F32, encode_floats(&[value]))
    }
    pub fn boolean(hash: u32, value: bool) -> Self {
        Self::raw(hash, KIND_BOOL, encode_i32(i32::from(value)))
    }
    pub fn vec3(hash: u32, value: [f32; 3]) -> Self {
        Self::raw(hash, KIND_VEC3, encode_floats(&value))
    }
    pub fn vec4(hash: u32, value: [f32; 4]) -> Self {
        Self::raw(hash, KIND_VEC4, encode_floats(&value))
    }
    pub fn string(hash: u32, kind: u8, value: &str) -> Self {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        Self::raw(hash, kind, bytes)
    }
    pub fn is_string(&self) -> bool {
        matches!(
            self.kind,
            KIND_STRING32 | KIND_STRING64 | KIND_STRING256 | KIND_STRING_REF
        )
    }
    pub fn as_i32(&self) -> Option<i32> {
        BinaryReader::new(&self.bytes).read_i32_at(0).ok()
    }
    pub fn as_f32(&self) -> Option<f32> {
        self.as_i32().map(|value| f32::from_bits(value as u32))
    }
    pub fn as_vec3(&self) -> Option<[f32; 3]> {
        let values = decode_floats(&self.bytes, 3)?;
        Some([values[0], values[1], values[2]])
    }
    pub fn as_vec4(&self) -> Option<[f32; 4]> {
        let values = decode_floats(&self.bytes, 4)?;
        Some([values[0], values[1], values[2], values[3]])
    }
    pub fn as_string(&self) -> String {
        let end = self
            .bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(self.bytes.len());
        String::from_utf8_lossy(&self.bytes[..end]).into_owned()
    }
}

impl AampObject {
    pub fn new(hash: u32) -> Self {
        Self {
            hash,
            parameters: Vec::new(),
        }
    }
    pub fn named(name: &str) -> Self {
        Self::new(crc32(name))
    }
    pub fn find(&self, hash: u32) -> Option<&AampParameter> {
        self.parameters
            .iter()
            .find(|parameter| parameter.hash == hash)
    }
    fn position(&self, hash: u32) -> Option<usize> {
        self.parameters
            .iter()
            .position(|parameter| parameter.hash == hash)
    }
    /// Integer parameter, or -1 when absent: the sentinel every index field
    /// in these sidecars uses for "not connected".
    pub fn int(&self, hash: u32) -> i32 {
        self.find(hash)
            .and_then(AampParameter::as_i32)
            .unwrap_or(-1)
    }
    /// Integer parameter only when it is actually stored as an integer.
    pub fn typed_int(&self, hash: u32) -> Option<i32> {
        self.find(hash)
            .filter(|parameter| parameter.kind == KIND_I32)
            .and_then(AampParameter::as_i32)
    }
    pub fn string(&self, hash: u32) -> Option<String> {
        self.find(hash).map(AampParameter::as_string)
    }
    pub fn vec3(&self, hash: u32) -> Option<[f32; 3]> {
        self.find(hash).and_then(AampParameter::as_vec3)
    }
    pub fn vec4(&self, hash: u32) -> Option<[f32; 4]> {
        self.find(hash).and_then(AampParameter::as_vec4)
    }
    /// Replaces the parameter's payload, keeping its stored type when it
    /// already exists and adopting `fallback_kind` otherwise.
    pub fn set_raw(&mut self, hash: u32, fallback_kind: u8, bytes: Vec<u8>) {
        match self.position(hash) {
            Some(index) => {
                let kind = self.parameters[index].kind;
                self.parameters[index] = AampParameter::raw(hash, kind, bytes);
            }
            None => self
                .parameters
                .push(AampParameter::raw(hash, fallback_kind, bytes)),
        }
    }
    pub fn set_int(&mut self, hash: u32, value: i32) {
        self.set_raw(hash, KIND_I32, encode_i32(value));
    }
    pub fn set_existing_int(&mut self, hash: u32, value: i32) {
        if self.position(hash).is_some() {
            self.set_int(hash, value);
        }
    }
    pub fn set_string(&mut self, hash: u32, value: &str) {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        self.set_raw(hash, KIND_STRING256, bytes);
    }
    pub fn set_vec3(&mut self, hash: u32, value: [f32; 3]) {
        self.set_raw(hash, KIND_VEC3, encode_floats(&value));
    }
    pub fn set_vec4(&mut self, hash: u32, value: [f32; 4]) {
        self.set_raw(hash, KIND_VEC4, encode_floats(&value));
    }
    pub fn remove(&mut self, hash: u32) {
        self.parameters.retain(|parameter| parameter.hash != hash);
    }
    pub fn push(&mut self, parameter: AampParameter) {
        self.parameters.push(parameter);
    }
    /// A copy of this object under a new name, keeping every parameter.
    pub fn clone_as(&self, name: &str) -> Self {
        Self {
            hash: crc32(name),
            parameters: self.parameters.clone(),
        }
    }
    /// Copies one parameter from another object, optionally under a new hash.
    pub fn copy_from(
        &mut self,
        source: &AampObject,
        source_hash: u32,
        target_hash: Option<u32>,
    ) -> io::Result<()> {
        let parameter = source.find(source_hash).ok_or_else(|| {
            invalid(&format!(
                "AAMP record is missing parameter 0x{source_hash:08X}"
            ))
        })?;
        self.push(AampParameter::raw(
            target_hash.unwrap_or(parameter.hash),
            parameter.kind,
            parameter.bytes.clone(),
        ));
        Ok(())
    }
}

impl AampList {
    pub fn new(hash: u32) -> Self {
        Self {
            hash,
            children: Vec::new(),
            objects: Vec::new(),
        }
    }
    pub fn named(name: &str) -> Self {
        Self::new(crc32(name))
    }
    pub fn find_child(&self, hash: u32) -> Option<&AampList> {
        self.children.iter().find(|child| child.hash == hash)
    }
    pub fn find_child_mut(&mut self, hash: u32) -> Option<&mut AampList> {
        self.children.iter_mut().find(|child| child.hash == hash)
    }
    pub fn child_index(&self, hash: u32) -> Option<usize> {
        self.children.iter().position(|child| child.hash == hash)
    }
    pub fn require_child(&self, hash: u32, name: &str) -> io::Result<&AampList> {
        self.find_child(hash)
            .ok_or_else(|| invalid(&format!("the AAMP graph does not contain '{name}'")))
    }
    pub fn require_child_mut(&mut self, hash: u32, name: &str) -> io::Result<&mut AampList> {
        self.find_child_mut(hash)
            .ok_or_else(|| invalid(&format!("the AAMP graph does not contain '{name}'")))
    }
    /// Renames every object to `prefix_<index>` after a reorder, which is how
    /// vanilla files label their positional records.
    pub fn normalize_object_names(&mut self, prefix: &str) {
        for (index, object) in self.objects.iter_mut().enumerate() {
            object.hash = crc32(&format!("{prefix}_{index}"));
        }
    }
    fn count(&self) -> (u32, u32, u32) {
        let mut lists = 1;
        let mut objects = self.objects.len() as u32;
        let mut parameters: u32 = self
            .objects
            .iter()
            .map(|object| object.parameters.len() as u32)
            .sum();
        for child in &self.children {
            let (l, o, p) = child.count();
            lists += l;
            objects += o;
            parameters += p;
        }
        (lists, objects, parameters)
    }
}

impl AampTree {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 0x40 || &bytes[..4] != b"AAMP" {
            return Err(invalid("the file is not a complete AAMP archive"));
        }
        let declared = read_u32(bytes, 0x0c)? as usize;
        if declared != bytes.len() {
            return Err(invalid(&format!(
                "AAMP declares {declared} bytes but the file contains {}",
                bytes.len()
            )));
        }
        let archive_type = read_c_string(bytes, HEADER_SIZE)?;
        let root_offset = HEADER_SIZE
            .checked_add(read_u32(bytes, 0x14)? as usize)
            .ok_or_else(|| invalid("AAMP root offset overflows"))?;
        let mut visited = Vec::new();
        let root = read_list(bytes, root_offset, &mut visited)?;
        Ok(Self {
            archive_type,
            archive_version: read_u32(bytes, 0x04)?,
            format_version: read_u32(bytes, 0x08)?,
            parameter_io_version: read_u32(bytes, 0x10)?,
            root,
        })
    }

    /// Serializes the tree with the same sequential layout PhysicsTool
    /// writes: lists, then objects, then parameters, then scalar payloads,
    /// then the string pool.
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        let mut lists = Vec::new();
        collect_lists(&self.root, &mut lists);
        let objects: Vec<&AampObject> = lists.iter().flat_map(|list| &list.objects).collect();
        let parameters: Vec<&AampParameter> = objects
            .iter()
            .flat_map(|object| &object.parameters)
            .collect();

        let mut bytes = vec![0u8; HEADER_SIZE];
        bytes.extend_from_slice(self.archive_type.as_bytes());
        bytes.push(0);

        let mut list_offsets = Vec::with_capacity(lists.len());
        for _ in &lists {
            align(&mut bytes, 4);
            list_offsets.push(bytes.len());
            bytes.extend_from_slice(&[0; 12]);
        }
        let mut object_offsets = Vec::with_capacity(objects.len());
        for _ in &objects {
            align(&mut bytes, 4);
            object_offsets.push(bytes.len());
            bytes.extend_from_slice(&[0; 8]);
        }
        let mut parameter_offsets = Vec::with_capacity(parameters.len());
        for _ in &parameters {
            align(&mut bytes, 4);
            parameter_offsets.push(bytes.len());
            bytes.extend_from_slice(&[0; 8]);
        }

        let data_start = bytes.len();
        let mut value_offsets = vec![0usize; parameters.len()];
        for (index, parameter) in parameters.iter().enumerate() {
            if parameter.is_string() {
                continue;
            }
            align(&mut bytes, 4);
            value_offsets[index] = bytes.len();
            bytes.extend_from_slice(&parameter.bytes);
        }
        let data_size = bytes.len() - data_start;
        for (index, parameter) in parameters.iter().enumerate() {
            if !parameter.is_string() {
                continue;
            }
            align(&mut bytes, 4);
            value_offsets[index] = bytes.len();
            bytes.extend_from_slice(&parameter.bytes);
        }
        let string_pool_size = bytes.len() - data_start - data_size;

        bytes[..4].copy_from_slice(b"AAMP");
        write_u32(&mut bytes, 0x04, self.archive_version);
        write_u32(&mut bytes, 0x08, self.format_version);
        let total = to_u32(bytes.len())?;
        write_u32(&mut bytes, 0x0c, total);
        write_u32(&mut bytes, 0x10, self.parameter_io_version);
        write_u32(&mut bytes, 0x14, to_u32(list_offsets[0] - HEADER_SIZE)?);
        write_u32(&mut bytes, 0x18, to_u32(lists.len())?);
        write_u32(&mut bytes, 0x1c, to_u32(objects.len())?);
        write_u32(&mut bytes, 0x20, to_u32(parameters.len())?);
        write_u32(&mut bytes, 0x24, to_u32(data_size)?);
        write_u32(&mut bytes, 0x28, to_u32(string_pool_size)?);
        write_u32(&mut bytes, 0x2c, 0);

        // Lists index their children and objects by position in the flat
        // tables, so map each node back to the offset it was given.
        let mut list_cursor = 0usize;
        let mut object_cursor = 0usize;
        let mut parameter_cursor = 0usize;
        for (list_index, list) in lists.iter().enumerate() {
            let offset = list_offsets[list_index];
            write_u32(&mut bytes, offset, list.hash);
            // Children of this list sit contiguously after the ones already
            // handed out because the flat order is a pre-order walk in which
            // every list's own children come before any grandchildren.
            let child_start = list_cursor + 1;
            let first_child = list_offsets.get(child_start).copied();
            write_relative(
                &mut bytes,
                offset + 4,
                offset,
                first_child,
                list.children.len(),
            )?;
            list_cursor += list.children.len();
            let first_object = object_offsets.get(object_cursor).copied();
            write_relative(
                &mut bytes,
                offset + 8,
                offset,
                first_object,
                list.objects.len(),
            )?;
            for object in &list.objects {
                let object_offset = object_offsets[object_cursor];
                write_u32(&mut bytes, object_offset, object.hash);
                let first_parameter = parameter_offsets.get(parameter_cursor).copied();
                write_relative(
                    &mut bytes,
                    object_offset + 4,
                    object_offset,
                    first_parameter,
                    object.parameters.len(),
                )?;
                for parameter in &object.parameters {
                    let parameter_offset = parameter_offsets[parameter_cursor];
                    write_u32(&mut bytes, parameter_offset, parameter.hash);
                    let words = (value_offsets[parameter_cursor] - parameter_offset) / 4;
                    if words > 0x00ff_ffff {
                        return Err(invalid(
                            "AAMP parameter payload lies too far from its record",
                        ));
                    }
                    write_u32(
                        &mut bytes,
                        parameter_offset + 4,
                        (u32::from(parameter.kind) << 24) | words as u32,
                    );
                    parameter_cursor += 1;
                }
                object_cursor += 1;
            }
        }
        Ok(bytes)
    }

    /// Total list, object and parameter counts, the way the AAMP header
    /// reports them (the root list included).
    pub fn counts(&self) -> (u32, u32, u32) {
        self.root.count()
    }
}

fn collect_lists<'a>(list: &'a AampList, out: &mut Vec<&'a AampList>) {
    // Breadth-first per level keeps each list's children contiguous, which
    // the relative child table requires.
    out.push(list);
    let mut queue = vec![list];
    let mut index = 0;
    while index < queue.len() {
        let current = queue[index];
        index += 1;
        for child in &current.children {
            out.push(child);
            queue.push(child);
        }
    }
}

fn read_list(bytes: &[u8], offset: usize, visited: &mut Vec<usize>) -> io::Result<AampList> {
    ensure(bytes, offset, 12)?;
    if visited.contains(&offset) {
        return Err(invalid("AAMP list graph contains a cycle"));
    }
    visited.push(offset);
    let hash = read_u32(bytes, offset)?;
    let child_flags = read_u32(bytes, offset + 4)?;
    let object_flags = read_u32(bytes, offset + 8)?;
    let mut list = AampList::new(hash);
    for child_offset in relative_offsets(bytes, offset, child_flags, 12)? {
        list.children.push(read_list(bytes, child_offset, visited)?);
    }
    for object_offset in relative_offsets(bytes, offset, object_flags, 8)? {
        list.objects.push(read_object(bytes, object_offset)?);
    }
    visited.pop();
    Ok(list)
}

fn read_object(bytes: &[u8], offset: usize) -> io::Result<AampObject> {
    ensure(bytes, offset, 8)?;
    let hash = read_u32(bytes, offset)?;
    let flags = read_u32(bytes, offset + 4)?;
    let mut object = AampObject::new(hash);
    for parameter_offset in relative_offsets(bytes, offset, flags, 8)? {
        object
            .parameters
            .push(read_parameter(bytes, parameter_offset)?);
    }
    Ok(object)
}

fn read_parameter(bytes: &[u8], offset: usize) -> io::Result<AampParameter> {
    ensure(bytes, offset, 8)?;
    let hash = read_u32(bytes, offset)?;
    let flags = read_u32(bytes, offset + 4)?;
    let kind = (flags >> 24) as u8;
    let value_offset = offset
        .checked_add((flags & 0x00ff_ffff) as usize * 4)
        .ok_or_else(|| invalid("AAMP parameter offset overflows"))?;
    let size = value_size(bytes, value_offset, kind)?;
    ensure(bytes, value_offset, size)?;
    Ok(AampParameter::raw(
        hash,
        kind,
        bytes[value_offset..value_offset + size].to_vec(),
    ))
}

fn value_size(bytes: &[u8], offset: usize, kind: u8) -> io::Result<usize> {
    Ok(match kind {
        KIND_BOOL | KIND_F32 | KIND_I32 | KIND_U32 => 4,
        KIND_VEC2 => 8,
        KIND_VEC3 => 12,
        KIND_VEC4 | KIND_COLOR | KIND_QUAT => 16,
        KIND_STRING32 | KIND_STRING64 | KIND_STRING256 | KIND_STRING_REF => {
            let end = bytes[offset.min(bytes.len())..]
                .iter()
                .position(|byte| *byte == 0)
                .ok_or_else(|| invalid("AAMP string parameter is not null terminated"))?;
            end + 1
        }
        other => {
            return Err(invalid(&format!(
                "AAMP parameter type {other} is not supported by the sidecar reader"
            )))
        }
    })
}

fn relative_offsets(
    bytes: &[u8],
    source: usize,
    flags: u32,
    stride: usize,
) -> io::Result<Vec<usize>> {
    let count = (flags >> 16) as usize;
    if count == 0 {
        return Ok(Vec::new());
    }
    let offset = source
        .checked_add((flags & 0xffff) as usize * 4)
        .ok_or_else(|| invalid("AAMP table offset overflows"))?;
    ensure(bytes, offset, count * stride)?;
    Ok((0..count).map(|index| offset + index * stride).collect())
}

fn write_relative(
    bytes: &mut [u8],
    flags_offset: usize,
    source: usize,
    first_target: Option<usize>,
    count: usize,
) -> io::Result<()> {
    if count == 0 {
        write_u32(bytes, flags_offset, 0);
        return Ok(());
    }
    let first = first_target.ok_or_else(|| invalid("AAMP table is missing its entries"))?;
    let words = (first - source) / 4;
    if words > 0xffff || count > 0xffff {
        return Err(invalid("AAMP collection exceeds the 16-bit layout range"));
    }
    write_u32(bytes, flags_offset, ((count as u32) << 16) | words as u32);
    Ok(())
}

fn encode_i32(value: i32) -> Vec<u8> {
    let mut writer = BinaryWriter::new();
    writer.write_i32(value);
    writer.into_inner()
}

fn encode_floats(values: &[f32]) -> Vec<u8> {
    let mut writer = BinaryWriter::new();
    for value in values {
        writer.write_f32(*value);
    }
    writer.into_inner()
}

fn decode_floats(bytes: &[u8], count: usize) -> Option<Vec<f32>> {
    let reader = BinaryReader::new(bytes);
    (0..count)
        .map(|index| reader.read_f32_at(index * 4).ok())
        .collect()
}

fn align(bytes: &mut Vec<u8>, alignment: usize) {
    while bytes.len() % alignment != 0 {
        bytes.push(0);
    }
}

fn ensure(bytes: &[u8], offset: usize, length: usize) -> io::Result<()> {
    if offset
        .checked_add(length)
        .is_none_or(|end| end > bytes.len())
    {
        return Err(invalid("an AAMP offset lies outside the file"));
    }
    Ok(())
}

fn read_u32(bytes: &[u8], offset: usize) -> io::Result<u32> {
    ensure(bytes, offset, 4)?;
    BinaryReader::new(bytes).read_u32_at(offset)
}

/// The archive is sized before any field is written, so a miss here is a
/// layout bug rather than a malformed input.
fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    BinaryPatcher::new(bytes)
        .write_u32_at(offset, value)
        .expect("AAMP field lies inside the sized archive");
}

fn read_c_string(bytes: &[u8], offset: usize) -> io::Result<String> {
    let slice = bytes
        .get(offset..)
        .ok_or_else(|| invalid("AAMP type string lies outside the file"))?;
    let end = slice
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(slice.len());
    Ok(String::from_utf8_lossy(&slice[..end]).into_owned())
}

fn to_u32(value: usize) -> io::Result<u32> {
    u32::try_from(value).map_err(|_| invalid("AAMP size exceeds u32"))
}

pub(crate) fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_known_helper_bone_hashes() {
        assert_eq!(crc32("bone_list"), 3_240_679_892);
        assert_eq!(crc32("driver_bone_list"), 3_993_686_733);
        assert_eq!(crc32("name"), 1_579_384_326);
    }

    #[test]
    fn nested_tree_round_trips_through_bytes() {
        let mut root = AampList::named("param_root");
        let mut header = AampObject::named("helper_bone_header");
        header.push(AampParameter::float(crc32("step"), 0.1));
        root.objects.push(header);
        let mut data = AampList::named("helper_bone_data");
        let mut bones = AampList::named("bone_list");
        let mut bone = AampObject::named("bone_0");
        bone.push(AampParameter::string(
            crc32("name"),
            KIND_STRING256,
            "Hair_1",
        ));
        bones.objects.push(bone);
        let mut drivers = AampList::named("driver_bone_list");
        let mut driver = AampObject::named("driver_bone_0");
        driver.push(AampParameter::int(crc32("bone_id"), 0));
        driver.push(AampParameter::vec3(
            crc32("base_translate"),
            [1.0, 2.0, 3.0],
        ));
        driver.push(AampParameter::vec4(
            crc32("base_rotate"),
            [0.0, 0.0, 0.0, 1.0],
        ));
        drivers.objects.push(driver);
        data.children.push(bones);
        data.children.push(drivers);
        root.children.push(data);
        let tree = AampTree {
            archive_type: "phhb".into(),
            archive_version: 2,
            format_version: 3,
            parameter_io_version: 0,
            root,
        };
        let bytes = tree.to_bytes().unwrap();
        let parsed = AampTree::parse(&bytes).unwrap();
        assert_eq!(parsed, tree);
        assert_eq!(parsed.counts(), (4, 3, 5));
        let driver = &parsed.root.children[0].children[1].objects[0];
        assert_eq!(driver.int(crc32("bone_id")), 0);
        assert_eq!(driver.vec3(crc32("base_translate")), Some([1.0, 2.0, 3.0]));
        assert_eq!(
            parsed.root.children[0].children[0].objects[0].string(crc32("name")),
            Some("Hair_1".into())
        );
    }
}
