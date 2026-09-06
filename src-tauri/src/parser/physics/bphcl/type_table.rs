use super::{Section, TypeBody, TypeHash, TypeInterface, TypeMember, TypeNamed, TypeTemplate};
use crate::parser::binary::{BinaryReader, BinaryWriter, Endian};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::{self, ErrorKind},
};

#[derive(Clone, Debug)]
pub struct TypeTable {
    pub section: Section,
    pub type_strings: Vec<String>,
    pub field_strings: Vec<String>,
    pub named_types: Vec<TypeNamed>,
    pub bodies: Vec<TypeBody>,
    pub hashes: Vec<TypeHash>,
    original: Vec<u8>,
}

pub struct TypeMerge {
    pub target_to_merged: HashMap<u32, u32>,
    pub source_to_merged: HashMap<u32, u32>,
    pub replacement_section: Option<Vec<u8>>,
}

impl TypeTable {
    pub fn merge_required(
        &self,
        source: &TypeTable,
        required: impl IntoIterator<Item = u32>,
    ) -> io::Result<TypeMerge> {
        let required = source.dependency_closure(required);
        let target_keys = self.definition_keys();
        let source_keys = source.definition_keys();
        let by_key: HashMap<_, _> = target_keys
            .iter()
            .map(|(index, key)| (key.clone(), *index))
            .collect();
        let mut source_to_merged = HashMap::from([(0, 0)]);
        let mut additions = Vec::new();
        for source_index in required {
            if let Some(target_index) = source_keys
                .get(&source_index)
                .and_then(|key| by_key.get(key))
            {
                source_to_merged.insert(source_index, *target_index);
            } else {
                let index = u32::try_from(self.type_count() + additions.len())
                    .map_err(|_| invalid("TYPE index exceeds u32"))?;
                source_to_merged.insert(source_index, index);
                additions.push(source_index);
            }
        }
        if additions.is_empty() {
            return Ok(TypeMerge {
                target_to_merged: self.identity_map(),
                source_to_merged,
                replacement_section: None,
            });
        }
        let mut merged = self.clone();
        let mut type_strings: HashMap<String, u32> = merged
            .type_strings
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, value)| (value, i as u32))
            .collect();
        let mut field_strings: HashMap<String, u32> = merged
            .field_strings
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, value)| (value, i as u32))
            .collect();
        let mut type_string = |index: u32, merged: &mut TypeTable| -> io::Result<u32> {
            let value = source
                .type_strings
                .get(index as usize)
                .ok_or_else(|| invalid("source TYPE string is missing"))?
                .clone();
            Ok(*type_strings.entry(value.clone()).or_insert_with(|| {
                merged.type_strings.push(value);
                (merged.type_strings.len() - 1) as u32
            }))
        };
        for source_index in additions {
            let named = source
                .named_types
                .get(source_index as usize - 1)
                .ok_or_else(|| invalid("source TYPE record is missing"))?;
            let remap = |index: u32| {
                source_to_merged
                    .get(&index)
                    .copied()
                    .ok_or_else(|| invalid("source TYPE dependency is unmapped"))
            };
            let string_index = type_string(named.string_index, &mut merged)?;
            let mut templates = Vec::with_capacity(named.templates.len());
            for template in &named.templates {
                templates.push(TypeTemplate {
                    string_index: type_string(template.string_index, &mut merged)?,
                    type_index: remap(template.type_index)?,
                });
            }
            merged.named_types.push(TypeNamed {
                string_index,
                templates,
            });
            if let Some(body) = source
                .bodies
                .iter()
                .find(|body| body.type_index == source_index)
            {
                let mut body = body.clone();
                body.type_index = remap(body.type_index)?;
                body.parent_type_index = remap(body.parent_type_index)?;
                body.subtype_index = body.subtype_index.map(&remap).transpose()?;
                for member in &mut body.members {
                    let value = source
                        .field_strings
                        .get(member.name_index as usize)
                        .ok_or_else(|| invalid("source field string is missing"))?
                        .clone();
                    member.name_index = *field_strings.entry(value.clone()).or_insert_with(|| {
                        merged.field_strings.push(value);
                        (merged.field_strings.len() - 1) as u32
                    });
                    member.type_index = remap(member.type_index)?;
                }
                for interface in &mut body.interfaces {
                    interface.type_index = remap(interface.type_index)?;
                }
                merged.bodies.push(body);
            }
            if let Some(hash) = source
                .hashes
                .iter()
                .find(|hash| hash.type_index == source_index)
            {
                merged.hashes.push(TypeHash {
                    type_index: remap(hash.type_index)?,
                    hash: hash.hash,
                });
            }
        }
        Ok(TypeMerge {
            target_to_merged: self.identity_map(),
            source_to_merged,
            replacement_section: Some(merged.rebuild()?),
        })
    }
    pub fn parse(tag: &Section, data: &[u8]) -> io::Result<Self> {
        let section = tag
            .find("TYPE")
            .ok_or_else(|| invalid("BPHCL has no TYPE section"))?
            .clone();
        Self::parse_type_section(section, data)
    }

    pub fn type_count(&self) -> usize {
        self.named_types.len() + 1
    }

    pub fn type_name(&self, index: u32) -> Option<&str> {
        if index == 0 {
            return Some("");
        }
        let named = self.named_types.get(index as usize - 1)?;
        self.type_strings
            .get(named.string_index as usize)
            .map(String::as_str)
    }

    pub fn dependency_closure(&self, roots: impl IntoIterator<Item = u32>) -> Vec<u32> {
        let mut found = HashSet::new();
        let mut queue: VecDeque<u32> = roots
            .into_iter()
            .filter(|i| *i > 0 && (*i as usize) < self.type_count())
            .collect();
        while let Some(index) = queue.pop_front() {
            if !found.insert(index) {
                continue;
            }
            let mut push = |value| {
                if value > 0 && (value as usize) < self.type_count() && !found.contains(&value) {
                    queue.push_back(value)
                }
            };
            if let Some(named) = self.named_types.get(index as usize - 1) {
                for template in &named.templates {
                    push(template.type_index);
                }
            }
            if let Some(body) = self.bodies.iter().find(|body| body.type_index == index) {
                push(body.parent_type_index);
                if let Some(value) = body.subtype_index {
                    push(value);
                }
                for member in &body.members {
                    push(member.type_index);
                }
                for interface in &body.interfaces {
                    push(interface.type_index);
                }
            }
        }
        let mut values: Vec<_> = found.into_iter().collect();
        values.sort_unstable();
        values
    }

    pub fn identity_map(&self) -> HashMap<u32, u32> {
        (0..self.type_count() as u32).map(|i| (i, i)).collect()
    }

    pub fn definition_keys(&self) -> HashMap<u32, String> {
        fn describe(
            table: &TypeTable,
            index: u32,
            keys: &mut HashMap<u32, String>,
            active: &mut HashSet<u32>,
        ) -> String {
            if index == 0 {
                return "void".into();
            }
            if let Some(value) = keys.get(&index) {
                return value.clone();
            }
            let Some(named) = table.named_types.get(index as usize - 1) else {
                return format!("external:{index}");
            };
            let name = table
                .type_strings
                .get(named.string_index as usize)
                .cloned()
                .unwrap_or_else(|| format!("invalid-name:{}", named.string_index));
            if !active.insert(index) {
                return format!("cycle:{name}");
            }
            let templates = named
                .templates
                .iter()
                .map(|t| {
                    let parameter = table
                        .type_strings
                        .get(t.string_index as usize)
                        .cloned()
                        .unwrap_or_else(|| format!("invalid-parameter:{}", t.string_index));
                    format!(
                        "{parameter}={}",
                        describe(table, t.type_index, keys, active)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            active.remove(&index);
            let base = if templates.is_empty() {
                name
            } else {
                format!("{name}<{templates}>")
            };
            let Some(body) = table.bodies.iter().find(|b| b.type_index == index) else {
                keys.insert(index, base.clone());
                return base;
            };
            let members = body
                .members
                .iter()
                .map(|m| {
                    let name = table
                        .field_strings
                        .get(m.name_index as usize)
                        .cloned()
                        .unwrap_or_else(|| format!("invalid-field:{}", m.name_index));
                    format!(
                        "{name}@{:X}:{}:{:X}:{:?}",
                        m.offset,
                        describe(table, m.type_index, keys, active),
                        m.flags,
                        m.reserve
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let interfaces = body
                .interfaces
                .iter()
                .map(|i| {
                    format!(
                        "{}:{:X}",
                        describe(table, i.type_index, keys, active),
                        i.flags
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let key = format!("{base}{{parent={};flags={:X};format={:?};subtype={};version={:?};size={:?};alignment={:?};unknown={:?};members=[{}];interfaces=[{}];attribute={:?}}}",
                describe(table, body.parent_type_index, keys, active), body.flags, body.format,
                body.subtype_index.map(|i| describe(table, i, keys, active)).unwrap_or_else(|| "-".into()),
                body.version, body.size, body.alignment, body.unknown_flags, members, interfaces, body.attribute_index);
            keys.insert(index, key.clone());
            key
        }
        let mut keys = HashMap::new();
        for index in 1..self.type_count() as u32 {
            let _ = describe(self, index, &mut keys, &mut HashSet::new());
        }
        keys
    }

    pub fn rebuild(&self) -> io::Result<Vec<u8>> {
        let added = self
            .named_types
            .len()
            .checked_sub(original_named_count(&self.section, &self.original)?)
            .ok_or_else(|| invalid("TYPE named records were removed"))?;
        let mut children = Vec::new();
        for section in &self.section.children {
            let payload = match section.signature.as_str() {
                "TPTR" => {
                    let mut value = payload(section, &self.original)?.to_vec();
                    value.resize(
                        value
                            .len()
                            .checked_add(added * 8)
                            .ok_or_else(|| invalid("TPTR overflow"))?,
                        0,
                    );
                    value
                }
                "TST1" | "TSTR" => write_strings(&self.type_strings),
                "FST1" | "FSTR" => write_strings(&self.field_strings),
                "TNA1" | "TNAM" => write_named(&self.named_types)?,
                "TBDY" | "TBOD" => write_bodies(&self.bodies)?,
                "THSH" => write_hashes(&self.hashes)?,
                _ => payload(section, &self.original)?.to_vec(),
            };
            children.extend(build_section(&section.signature, section.kind, &payload)?);
        }
        build_section(&self.section.signature, self.section.kind, &children)
    }

    pub fn validate_rebuild(&self) -> io::Result<()> {
        let bytes = self.rebuild()?;
        let section = Section::read(&bytes, 0, bytes.len(), Some("TYPE"))?;
        let rebuilt = Self::parse_type_section(section, &bytes)?;
        if self.type_strings != rebuilt.type_strings
            || self.field_strings != rebuilt.field_strings
            || self.named_types != rebuilt.named_types
            || self.bodies != rebuilt.bodies
            || self.hashes != rebuilt.hashes
        {
            return Err(invalid("rebuilt TYPE table is not semantically identical"));
        }
        Ok(())
    }

    fn parse_type_section(section: Section, data: &[u8]) -> io::Result<Self> {
        Ok(Self {
            type_strings: read_strings(child(&section, &["TST1", "TSTR"])?, data)?,
            field_strings: read_strings(child(&section, &["FST1", "FSTR"])?, data)?,
            named_types: read_named(child(&section, &["TNA1", "TNAM"])?, data)?,
            bodies: read_bodies(child(&section, &["TBDY", "TBOD"])?, data)?,
            hashes: section
                .children
                .iter()
                .find(|s| s.signature == "THSH")
                .map(|s| read_hashes(s, data))
                .transpose()?
                .unwrap_or_default(),
            section,
            original: data.to_vec(),
        })
    }
}

fn child<'a>(section: &'a Section, names: &[&str]) -> io::Result<&'a Section> {
    section
        .children
        .iter()
        .find(|s| names.contains(&s.signature.as_str()))
        .ok_or_else(|| invalid(&format!("TYPE missing {}", names[0])))
}
fn payload<'a>(section: &Section, data: &'a [u8]) -> io::Result<&'a [u8]> {
    data.get(section.payload_offset..section.payload_end())
        .ok_or_else(|| invalid("TYPE child exceeds input"))
}
fn read_strings(section: &Section, data: &[u8]) -> io::Result<Vec<String>> {
    let bytes = payload(section, data)?;
    let mut result = Vec::new();
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == 0 {
            result.push(String::from_utf8_lossy(&bytes[start..index]).into_owned());
            start = index + 1;
        }
    }
    if start < bytes.len() {
        result.push(String::from_utf8_lossy(&bytes[start..]).into_owned());
    }
    Ok(result)
}
fn reader<'a>(section: &Section, data: &'a [u8]) -> io::Result<BinaryReader<'a>> {
    let mut r = BinaryReader::new(data);
    r.seek(section.payload_offset)?;
    Ok(r)
}
fn read_var(r: &mut BinaryReader<'_>, end: usize) -> io::Result<u32> {
    let first = r.read_u8()?;
    if first & 0x80 == 0 {
        return Ok(first as u32);
    }
    let marker = first >> 3;
    let n = match marker {
        0x10..=0x17 => 1,
        0x18..=0x1b => 2,
        0x1c => 3,
        0x1d => 4,
        _ => return Err(invalid("unsupported TYPE VarUInt")),
    };
    if r.position() + n > end {
        return Err(invalid("truncated TYPE VarUInt"));
    }
    let mask = match n {
        1 => 0x3f,
        2 => 0x1f,
        _ => 0x07,
    };
    let mut value = (first & mask) as u64;
    for _ in 0..n {
        value = (value << 8) | r.read_u8()? as u64
    }
    u32::try_from(value).map_err(|_| invalid("TYPE VarUInt exceeds u32"))
}
fn read_named(s: &Section, d: &[u8]) -> io::Result<Vec<TypeNamed>> {
    let mut r = reader(s, d)?;
    let count = read_var(&mut r, s.payload_end())?;
    let mut out = Vec::new();
    for _ in 1..count {
        let string_index = read_var(&mut r, s.payload_end())?;
        let n = read_var(&mut r, s.payload_end())?;
        let mut templates = Vec::new();
        for _ in 0..n {
            templates.push(TypeTemplate {
                string_index: read_var(&mut r, s.payload_end())?,
                type_index: read_var(&mut r, s.payload_end())?,
            })
        }
        out.push(TypeNamed {
            string_index,
            templates,
        })
    }
    Ok(out)
}
fn read_bodies(s: &Section, d: &[u8]) -> io::Result<Vec<TypeBody>> {
    let mut r = reader(s, d)?;
    let mut out = Vec::new();
    while r.position() < s.payload_end() {
        let type_index = read_var(&mut r, s.payload_end())?;
        if type_index == 0 {
            out.push(TypeBody {
                type_index: 0,
                parent_type_index: 0,
                flags: 0,
                format: None,
                subtype_index: None,
                version: None,
                size: None,
                alignment: None,
                unknown_flags: None,
                encoded_member_count: 0,
                members: vec![],
                interface_count: None,
                interfaces: vec![],
                attribute_index: None,
            });
            continue;
        }
        let parent_type_index = read_var(&mut r, s.payload_end())?;
        let flags = read_var(&mut r, s.payload_end())?;
        let format = opt(&mut r, s, flags, 1)?;
        let subtype_index = opt(&mut r, s, flags, 2)?;
        let version = opt(&mut r, s, flags, 4)?;
        let (size, alignment) = if flags & 8 != 0 {
            (
                Some(read_var(&mut r, s.payload_end())?),
                Some(read_var(&mut r, s.payload_end())?),
            )
        } else {
            (None, None)
        };
        let unknown_flags = opt(&mut r, s, flags, 0x10)?;
        let encoded_member_count = if flags & 0x20 != 0 {
            read_var(&mut r, s.payload_end())?
        } else {
            0
        };
        let mut members = Vec::new();
        for _ in 0..encoded_member_count & 0xffff {
            let name_index = read_var(&mut r, s.payload_end())?;
            let mf = read_var(&mut r, s.payload_end())?;
            let reserve = if mf & 0x80 != 0 {
                Some(r.read_u8()?)
            } else {
                None
            };
            members.push(TypeMember {
                name_index,
                flags: mf,
                reserve,
                offset: read_var(&mut r, s.payload_end())?,
                type_index: read_var(&mut r, s.payload_end())?,
            })
        }
        let interface_count = if flags & 0x40 != 0 {
            Some(read_var(&mut r, s.payload_end())?)
        } else {
            None
        };
        let mut interfaces = Vec::new();
        for _ in 0..interface_count.unwrap_or(0) {
            interfaces.push(TypeInterface {
                type_index: read_var(&mut r, s.payload_end())?,
                flags: read_var(&mut r, s.payload_end())?,
            })
        }
        let attribute_index = opt(&mut r, s, flags, 0x80)?;
        out.push(TypeBody {
            type_index,
            parent_type_index,
            flags,
            format,
            subtype_index,
            version,
            size,
            alignment,
            unknown_flags,
            encoded_member_count,
            members,
            interface_count,
            interfaces,
            attribute_index,
        })
    }
    Ok(out)
}
fn opt(r: &mut BinaryReader<'_>, s: &Section, flags: u32, bit: u32) -> io::Result<Option<u32>> {
    if flags & bit != 0 {
        Ok(Some(read_var(r, s.payload_end())?))
    } else {
        Ok(None)
    }
}
fn read_hashes(s: &Section, d: &[u8]) -> io::Result<Vec<TypeHash>> {
    let mut r = reader(s, d)?;
    let n = read_var(&mut r, s.payload_end())?;
    let mut out = Vec::new();
    for _ in 0..n {
        out.push(TypeHash {
            type_index: read_var(&mut r, s.payload_end())?,
            hash: r.read_u32()?,
        })
    }
    Ok(out)
}
fn write_var(w: &mut BinaryWriter, v: u32) {
    if v <= 0x7f {
        w.write_u8(v as u8)
    } else if v <= 0x3fff {
        w.write_u8((0x80 | (v >> 8)) as u8);
        w.write_u8(v as u8)
    } else if v <= 0x1f_ffff {
        w.write_u8((0xc0 | (v >> 16)) as u8);
        w.write_u8((v >> 8) as u8);
        w.write_u8(v as u8)
    } else if v <= 0x07ff_ffff {
        w.write_u8((0xe0 | (v >> 24)) as u8);
        w.write_u8((v >> 16) as u8);
        w.write_u8((v >> 8) as u8);
        w.write_u8(v as u8)
    } else {
        w.write_u8(0xe8);
        w.write_u8((v >> 24) as u8);
        w.write_u8((v >> 16) as u8);
        w.write_u8((v >> 8) as u8);
        w.write_u8(v as u8)
    }
}
fn write_strings(v: &[String]) -> Vec<u8> {
    let mut w = BinaryWriter::new();
    for s in v {
        w.write_c_string(s)
    }
    w.into_inner()
}
fn write_named(v: &[TypeNamed]) -> io::Result<Vec<u8>> {
    let mut w = BinaryWriter::new();
    write_var(
        &mut w,
        u32::try_from(v.len() + 1).map_err(|_| invalid("too many named types"))?,
    );
    for n in v {
        write_var(&mut w, n.string_index);
        write_var(&mut w, n.templates.len() as u32);
        for t in &n.templates {
            write_var(&mut w, t.string_index);
            write_var(&mut w, t.type_index)
        }
    }
    Ok(w.into_inner())
}
fn write_bodies(v: &[TypeBody]) -> io::Result<Vec<u8>> {
    let mut w = BinaryWriter::new();
    for b in v {
        write_var(&mut w, b.type_index);
        if b.type_index == 0 {
            continue;
        }
        write_var(&mut w, b.parent_type_index);
        write_var(&mut w, b.flags);
        for (bit, value) in [(1, b.format), (2, b.subtype_index), (4, b.version)] {
            if b.flags & bit != 0 {
                write_var(
                    &mut w,
                    value.ok_or_else(|| invalid("TYPE body flag lacks value"))?,
                )
            }
        }
        if b.flags & 8 != 0 {
            write_var(
                &mut w,
                b.size.ok_or_else(|| invalid("TYPE body lacks size"))?,
            );
            write_var(
                &mut w,
                b.alignment
                    .ok_or_else(|| invalid("TYPE body lacks alignment"))?,
            )
        }
        if b.flags & 0x10 != 0 {
            write_var(
                &mut w,
                b.unknown_flags
                    .ok_or_else(|| invalid("TYPE body lacks unknown flags"))?,
            )
        }
        if b.flags & 0x20 != 0 {
            write_var(&mut w, b.encoded_member_count);
            for m in &b.members {
                write_var(&mut w, m.name_index);
                write_var(&mut w, m.flags);
                if let Some(x) = m.reserve {
                    w.write_u8(x)
                }
                write_var(&mut w, m.offset);
                write_var(&mut w, m.type_index)
            }
        }
        if b.flags & 0x40 != 0 {
            write_var(
                &mut w,
                b.interface_count
                    .ok_or_else(|| invalid("TYPE body lacks interface count"))?,
            );
            for i in &b.interfaces {
                write_var(&mut w, i.type_index);
                write_var(&mut w, i.flags)
            }
        }
        if b.flags & 0x80 != 0 {
            write_var(
                &mut w,
                b.attribute_index
                    .ok_or_else(|| invalid("TYPE body lacks attribute"))?,
            )
        }
    }
    Ok(w.into_inner())
}
fn write_hashes(v: &[TypeHash]) -> io::Result<Vec<u8>> {
    let mut w = BinaryWriter::new();
    write_var(
        &mut w,
        u32::try_from(v.len()).map_err(|_| invalid("too many TYPE hashes"))?,
    );
    for h in v {
        write_var(&mut w, h.type_index);
        w.write_u32(h.hash)
    }
    Ok(w.into_inner())
}
fn build_section(signature: &str, kind: u8, payload: &[u8]) -> io::Result<Vec<u8>> {
    if signature.len() != 4 || kind > 3 {
        return Err(invalid("invalid TYPE section header"));
    }
    let size = payload
        .len()
        .checked_add(8)
        .ok_or_else(|| invalid("TYPE section overflow"))?;
    if size > 0x3fff_ffff {
        return Err(invalid("TYPE section too large"));
    }
    let mut w = BinaryWriter::with_endian(Endian::Big);
    w.write_u32(((kind as u32) << 30) | size as u32);
    w.write_bytes(signature.as_bytes());
    w.write_bytes(payload);
    Ok(w.into_inner())
}
fn original_named_count(section: &Section, data: &[u8]) -> io::Result<usize> {
    Ok(read_named(child(section, &["TNA1", "TNAM"])?, data)?.len())
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_varuint_round_trips_boundaries() {
        for value in [
            0,
            0x7f,
            0x80,
            0x3fff,
            0x4000,
            0x1f_ffff,
            0x20_0000,
            0x07ff_ffff,
            0x0800_0000,
            0x7fff_ffff,
            u32::MAX,
        ] {
            let mut writer = BinaryWriter::new();
            write_var(&mut writer, value);
            let bytes = writer.into_inner();
            let mut reader = BinaryReader::new(&bytes);
            assert_eq!(read_var(&mut reader, bytes.len()).unwrap(), value);
            assert_eq!(reader.position(), bytes.len());
        }
    }
}
