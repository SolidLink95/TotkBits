use super::{
    fixup::{GlobalFixup, LocalFixup, VirtualFixup},
    header::HEADER_SIZE,
    item::Item,
    patch::Patch,
    physics::{parse_physics_graph, PhysicsGraph},
    section::SECTION_HEADER_SIZE,
    HkclHeader, HkclSection,
};
use crate::parser::binary::{BinaryReader, Endian};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io::{self, ErrorKind};
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct HkclDocument {
    pub raw: Vec<u8>,
    pub header: HkclHeader,
    pub sections: Vec<HkclSection>,
    pub contents_offset: Option<usize>,
    pub contents_class_name_offset: Option<usize>,
    pub contents_class_name: Option<String>,
    pub items: Vec<Item>,
    pub patches: Vec<Patch>,
    pub type_names: Vec<String>,
    pub local_fixups: Vec<LocalFixup>,
    pub global_fixups: Vec<GlobalFixup>,
    pub virtual_fixups: Vec<VirtualFixup>,
    pub physics: PhysicsGraph,
}

#[derive(Clone, Debug, Serialize)]
pub struct HkclLeaf {
    pub path: String,
    pub yaml: String,
    pub viewer_type: String,
    pub read_only: bool,
}

#[derive(Serialize)]
struct HkclYaml<'a> {
    format: &'static str,
    contents_version: &'a str,
    endian: &'static str,
    pointer_size: u8,
    contents_class_name: &'a Option<String>,
    physics: &'a PhysicsGraph,
}

impl HkclDocument {
    pub fn neutral_physics_graph(
        &self,
    ) -> crate::parser::physics::physics_graph::FormatNeutralPhysicsGraph {
        (&self.physics).into()
    }

    pub fn parse(data: &[u8]) -> io::Result<Self> {
        let header = HkclHeader::read(data)?;
        let sections = (0..header.section_count)
            .map(|index| {
                HkclSection::read(
                    data,
                    HEADER_SIZE + index * SECTION_HEADER_SIZE,
                    header.layout.endian,
                )
            })
            .collect::<io::Result<Vec<_>>>()?;
        let contents_offset = resolve_offset(
            &sections,
            header.contents_section_index,
            header.contents_section_offset,
            "contents",
        )?;
        let contents_class_name_offset = resolve_offset(
            &sections,
            header.contents_class_name_section_index,
            header.contents_class_name_section_offset,
            "contents class name",
        )?;
        let contents_class_name = contents_class_name_offset
            .map(|offset| BinaryReader::new(data).read_c_string_at(offset))
            .transpose()?;
        let discovered = discover_graph_sections(data, &sections)?;
        let mut items = parse_items(data, header.layout.endian, &discovered)?;
        let patches = parse_patches(data, header.layout.endian, &discovered)?;
        let mut type_names = parse_type_names(data, &discovered)?;
        let (local_fixups, global_fixups, virtual_fixups) =
            parse_packfile_fixups(data, header.layout.endian, &sections)?;
        if items.is_empty() && !virtual_fixups.is_empty() {
            let parsed = packfile_items(data, &sections, &virtual_fixups)?;
            items = parsed.0;
            type_names = parsed.1;
        }
        let physics = parse_physics_graph(
            data,
            &header,
            &sections,
            &items,
            &type_names,
            &local_fixups,
            &global_fixups,
        )?;
        Ok(Self {
            raw: data.to_vec(),
            header,
            sections,
            contents_offset,
            contents_class_name_offset,
            contents_class_name,
            items,
            patches,
            type_names,
            local_fixups,
            global_fixups,
            virtual_fixups,
            physics,
        })
    }

    pub fn section(&self, tag: &str) -> Option<&HkclSection> {
        self.sections.iter().find(|section| section.tag == tag)
    }

    pub fn validate(&self) -> io::Result<()> {
        self.validate_item_graph()?;
        let mut objects = HashSet::new();
        for item in &self.items {
            if item.data_section_index >= self.sections.len() {
                return Err(invalid("HKCL ITEM references a missing section"));
            }
            if item.type_index as usize >= self.type_names.len() {
                return Err(invalid("HKCL ITEM references a missing type"));
            }
            let key = super::ObjectKey {
                section_index: item.data_section_index,
                offset: item.data_offset,
            };
            if !objects.insert(key) {
                return Err(invalid(
                    "HKCL ITEM graph contains duplicate object locations",
                ));
            }
        }
        self.physics.validate(&objects)
    }

    pub fn to_yaml(&self) -> io::Result<String> {
        self.validate()?;
        serde_yaml::to_string(&HkclYaml {
            format: "HKCL",
            contents_version: &self.header.contents_version,
            endian: match self.header.layout.endian {
                Endian::Little => "little",
                Endian::Big => "big",
            },
            pointer_size: self.header.layout.pointer_size,
            contents_class_name: &self.contents_class_name,
            physics: &self.physics,
        })
        .map_err(io::Error::other)
    }

    pub fn leaves(&self) -> io::Result<Vec<HkclLeaf>> {
        self.validate()?;
        let mut leaves = Vec::new();
        for (index, skeleton) in self.physics.skeletons.iter().enumerate() {
            leaves.push(yaml_leaf(
                format!(
                    "Skeletons/{index:03} {}.bin",
                    leaf_name(skeleton.name.as_deref(), "Skeleton")
                ),
                "Skeleton",
                skeleton,
            )?);
        }
        for (cloth_index, cloth) in self.physics.cloths.iter().enumerate() {
            let cloth_name = leaf_name(cloth.name.as_deref(), "Cloth");
            leaves.push(yaml_leaf(
                format!("Cloths/{cloth_index:03} {cloth_name}.bin"),
                "Cloth",
                cloth,
            )?);
            for (simulation_index, simulation) in cloth.simulations.iter().enumerate() {
                leaves.push(yaml_leaf(
                    format!(
                        "Cloths/{cloth_index:03} {cloth_name}/Simulations/{simulation_index:03} {}.bin",
                        leaf_name(simulation.name.as_deref(), "Simulation")
                    ),
                    "Simulation",
                    simulation,
                )?);
            }
        }
        for (index, constraint) in self.physics.constraints.iter().enumerate() {
            leaves.push(yaml_leaf(
                format!(
                    "Constraints/{index:03} {}.bin",
                    leaf_name(constraint.name.as_deref(), &constraint.class_name)
                ),
                "Constraint",
                constraint,
            )?);
        }
        for (index, collidable) in self.physics.collidables.iter().enumerate() {
            leaves.push(yaml_leaf(
                format!(
                    "Collidables/{index:03} {}.bin",
                    leaf_name(collidable.name.as_deref(), "Collidable")
                ),
                "Collidable",
                collidable,
            )?);
        }
        Ok(leaves)
    }

    pub(super) fn data_payload(&self) -> io::Result<Option<Range<usize>>> {
        Ok(
            find_section_payload(&discover_graph_sections(&self.raw, &self.sections)?, "DATA")
                .or_else(|| self.section("__data__").map(HkclSection::data)),
        )
    }

    pub(super) fn items_payload(&self) -> io::Result<Option<Range<usize>>> {
        Ok(find_section_payload(
            &discover_graph_sections(&self.raw, &self.sections)?,
            "ITEM",
        ))
    }

    pub(super) fn patches_payload(&self) -> io::Result<Option<Range<usize>>> {
        Ok(find_section_payload(
            &discover_graph_sections(&self.raw, &self.sections)?,
            "PTCH",
        ))
    }
}

fn yaml_leaf<T: Serialize>(path: String, viewer_type: &str, value: &T) -> io::Result<HkclLeaf> {
    Ok(HkclLeaf {
        path,
        yaml: serde_yaml::to_string(value).map_err(io::Error::other)?,
        viewer_type: viewer_type.into(),
        read_only: true,
    })
}

fn leaf_name(name: Option<&str>, fallback: &str) -> String {
    let name = name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(fallback);
    name.chars()
        .map(|value| match value {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => value,
        })
        .collect()
}

fn parse_items(
    data: &[u8],
    endian: Endian,
    sections: &[DiscoveredSection],
) -> io::Result<Vec<Item>> {
    let Some(payload) = find_section_payload(sections, "ITEM") else {
        return Ok(vec![]);
    };
    if payload.len() % 12 != 0 {
        return Err(invalid("HKCL ITEM payload is not aligned"));
    }

    let mut reader = BinaryReader::with_endian(data, endian);
    reader.seek(payload.start)?;
    let mut out = Vec::with_capacity(payload.len() / 12);
    while reader.position() < payload.end {
        let flags = reader.read_u32()?;
        out.push(Item {
            flags,
            type_index: flags & 0x00ff_ffff,
            data_section_index: sections
                .iter()
                .position(|section| section.tag == "DATA")
                .unwrap_or(0),
            data_offset: reader.read_u32()?,
            count: reader.read_u32()?,
        });
    }
    Ok(out)
}

fn parse_packfile_fixups(
    data: &[u8],
    endian: Endian,
    sections: &[HkclSection],
) -> io::Result<(Vec<LocalFixup>, Vec<GlobalFixup>, Vec<VirtualFixup>)> {
    let mut local = Vec::new();
    let mut global = Vec::new();
    let mut virtuals = Vec::new();
    for (section_index, section) in sections.iter().enumerate() {
        let local_words = read_fixup_words(data, endian, section.local_fixups.clone(), 2)?;
        for words in local_words {
            local.push(LocalFixup {
                section_index,
                source_offset: words[0],
                destination_offset: words[1],
            });
        }
        let global_words = read_fixup_words(data, endian, section.global_fixups.clone(), 3)?;
        for words in global_words {
            let destination_section_index = usize::try_from(words[1])
                .map_err(|_| invalid("HKCL global fixup section index overflows"))?;
            if destination_section_index >= sections.len() {
                return Err(invalid("HKCL global fixup section index is out of range"));
            }
            global.push(GlobalFixup {
                section_index,
                source_offset: words[0],
                destination_section_index,
                destination_offset: words[2],
            });
        }
        let virtual_words = read_fixup_words(data, endian, section.virtual_fixups.clone(), 3)?;
        for words in virtual_words {
            let class_name_section_index = usize::try_from(words[1])
                .map_err(|_| invalid("HKCL virtual fixup section index overflows"))?;
            if class_name_section_index >= sections.len() {
                return Err(invalid("HKCL virtual fixup section index is out of range"));
            }
            virtuals.push(VirtualFixup {
                section_index,
                source_offset: words[0],
                class_name_section_index,
                class_name_offset: words[2],
            });
        }
    }
    Ok((local, global, virtuals))
}

fn read_fixup_words(
    data: &[u8],
    endian: Endian,
    range: Range<usize>,
    width: usize,
) -> io::Result<Vec<Vec<u32>>> {
    let mut reader = BinaryReader::with_endian(data, endian);
    reader.seek(range.start)?;
    let mut records = Vec::new();
    while reader.position() + width * 4 <= range.end {
        let words = (0..width)
            .map(|_| reader.read_u32())
            .collect::<io::Result<Vec<_>>>()?;
        if words[0] == u32::MAX {
            break;
        }
        records.push(words);
    }
    if data[reader.position()..range.end]
        .iter()
        .any(|byte| *byte != 0xff)
    {
        return Err(invalid("HKCL fixup section has non-padding trailing bytes"));
    }
    Ok(records)
}

fn packfile_items(
    data: &[u8],
    sections: &[HkclSection],
    virtual_fixups: &[VirtualFixup],
) -> io::Result<(Vec<Item>, Vec<String>)> {
    let mut type_names = vec![String::new()];
    let mut type_indices = HashMap::new();
    let mut items = Vec::with_capacity(virtual_fixups.len());
    for fixup in virtual_fixups {
        let class_section = &sections[fixup.class_name_section_index];
        let class_name_offset = class_section
            .absolute_data_start
            .checked_add(fixup.class_name_offset as usize)
            .ok_or_else(|| invalid("HKCL class name offset overflows"))?;
        if class_name_offset >= class_section.local_fixups.start {
            return Err(invalid("HKCL class name offset exceeds section data"));
        }
        let class_name = BinaryReader::new(data).read_c_string_at(class_name_offset)?;
        let type_index = if let Some(index) = type_indices.get(&class_name) {
            *index
        } else {
            let index = u32::try_from(type_names.len())
                .map_err(|_| invalid("HKCL type table exceeds u32"))?;
            type_names.push(class_name.clone());
            type_indices.insert(class_name, index);
            index
        };
        items.push(Item {
            flags: type_index,
            type_index,
            data_section_index: fixup.section_index,
            data_offset: fixup.source_offset,
            count: 1,
        });
    }
    Ok((items, type_names))
}

fn parse_patches(
    data: &[u8],
    endian: Endian,
    sections: &[DiscoveredSection],
) -> io::Result<Vec<Patch>> {
    let Some(payload) = find_section_payload(sections, "PTCH") else {
        return Ok(vec![]);
    };
    let mut reader = BinaryReader::with_endian(data, endian);
    reader.seek(payload.start)?;
    let end = payload.end;
    let mut out = Vec::new();
    while reader.position() + 4 <= end {
        let type_index = reader.read_u32()?;
        if type_index == 0 {
            break;
        }
        let count = reader.read_u32()? as usize;
        if count > (end - reader.position()) / 4 {
            return Err(invalid("HKCL PTCH exceeds section"));
        }
        let mut offsets = Vec::with_capacity(count);
        for _ in 0..count {
            offsets.push(reader.read_u32()?);
        }
        out.push(Patch {
            type_index,
            offsets,
        });
    }
    Ok(out)
}

fn parse_type_names(data: &[u8], sections: &[DiscoveredSection]) -> io::Result<Vec<String>> {
    let Some(type_payload) = find_section_payload(sections, "TYPE") else {
        return Ok(vec![]);
    };
    let nested = discover_graph_sections_in_range(data, type_payload.clone())?;
    let type_string_section = nested
        .iter()
        .find(|section| section.tag == "TST1" || section.tag == "TSTR")
        .map(|section| section.payload.clone());
    let type_name_section = nested
        .iter()
        .find(|section| section.tag == "TNA1" || section.tag == "TNAM")
        .map(|section| section.payload.clone());

    let (Some(type_strings), Some(type_names)) = (type_string_section, type_name_section) else {
        return Ok(vec![]);
    };
    let strings = read_null_terminated_strings(data, type_strings);
    let mut cursor = type_names.start;
    let mut type_count = read_varuint(data, &mut cursor, type_names.end)? as usize;
    if type_count == 0 {
        type_count = 1;
    }
    let mut out = Vec::with_capacity(type_count);
    out.push(String::new());
    for _ in 1..type_count {
        let string_index = read_varuint(data, &mut cursor, type_names.end)? as usize;
        let template_count = read_varuint(data, &mut cursor, type_names.end)?;
        out.push(
            strings
                .get(string_index)
                .cloned()
                .unwrap_or_else(|| format!("type-string-{string_index}")),
        );
        for _ in 0..template_count {
            let _ = read_varuint(data, &mut cursor, type_names.end)?;
            let _ = read_varuint(data, &mut cursor, type_names.end)?;
        }
    }
    Ok(out)
}

fn resolve_offset(
    sections: &[HkclSection],
    section_index: Option<usize>,
    relative_offset: u32,
    name: &str,
) -> io::Result<Option<usize>> {
    let Some(section_index) = section_index else {
        return Ok(None);
    };
    let section = &sections[section_index];
    let absolute = section
        .absolute_data_start
        .checked_add(relative_offset as usize)
        .ok_or_else(|| invalid(&format!("HKCL {name} offset overflows")))?;
    if absolute >= section.end {
        return Err(invalid(&format!("HKCL {name} offset exceeds its section")));
    }
    Ok(Some(absolute))
}

#[derive(Clone, Debug)]
struct DiscoveredSection {
    tag: String,
    payload: Range<usize>,
}

fn discover_graph_sections(
    data: &[u8],
    sections: &[HkclSection],
) -> io::Result<Vec<DiscoveredSection>> {
    let mut out = Vec::new();
    for section in sections {
        out.push(DiscoveredSection {
            tag: section.tag.clone(),
            payload: section.data(),
        });
        if matches!(section.tag.as_str(), "TAG0" | "TYPE" | "INDX") {
            out.extend(discover_graph_sections_in_range(data, section.data())?);
        }
    }
    Ok(out)
}

fn discover_graph_sections_in_range(
    data: &[u8],
    payload: Range<usize>,
) -> io::Result<Vec<DiscoveredSection>> {
    let mut sections = Vec::new();
    let mut cursor = payload.start;
    while cursor < payload.end {
        if cursor + 8 > payload.end {
            return Err(invalid("HKCL nested section is truncated"));
        }
        let size_word = BinaryReader::with_endian(data, Endian::Big)
            .read_u32_at(cursor)
            .map_err(|_| invalid("HKCL nested section is truncated"))?;
        let size = (size_word & 0x3fff_ffff) as usize;
        let signature = std::str::from_utf8(
            data.get(cursor + 4..cursor + 8)
                .ok_or_else(|| invalid("HKCL nested section is truncated"))?,
        )
        .map_err(|_| invalid("HKCL nested section tag is not UTF-8"))?
        .to_string();
        let section_end = cursor
            .checked_add(size)
            .ok_or_else(|| invalid("HKCL nested section end overflows"))?;
        if size < 8 || section_end > payload.end {
            return Err(invalid("HKCL nested section is invalid"));
        }
        let child_payload = (cursor + 8)..section_end;
        let entry = DiscoveredSection {
            tag: signature,
            payload: child_payload.clone(),
        };
        cursor = section_end;
        if matches!(entry.tag.as_str(), "TAG0" | "TYPE" | "INDX") {
            sections.push(entry.clone());
            sections.extend(discover_graph_sections_in_range(data, child_payload)?);
        } else {
            sections.push(entry);
        }
    }
    if cursor != payload.end {
        return Err(invalid("HKCL nested sections do not end on boundaries"));
    }
    Ok(sections)
}

fn find_section_payload(sections: &[DiscoveredSection], tag: &str) -> Option<Range<usize>> {
    sections
        .iter()
        .find(|section| section.tag == tag)
        .map(|section| section.payload.clone())
}

fn read_null_terminated_strings(data: &[u8], payload: Range<usize>) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = payload.start;
    while cursor <= payload.end {
        if cursor == payload.end {
            break;
        }
        let next = data[cursor..payload.end]
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(payload.end - cursor);
        out.push(String::from_utf8_lossy(&data[cursor..cursor + next]).into_owned());
        cursor += next + 1;
    }
    out
}

fn read_varuint(data: &[u8], cursor: &mut usize, end: usize) -> io::Result<u64> {
    if *cursor >= end || *cursor >= data.len() {
        return Err(invalid("truncated HKCL VarUInt"));
    }
    let first = data[*cursor];
    *cursor += 1;
    if first & 0x80 == 0 {
        return Ok(first as u64);
    }
    let marker = first >> 3;
    let extra = match marker {
        0x10..=0x17 => 1,
        0x18..=0x1b => 2,
        0x1c => 3,
        0x1d => 4,
        0x1e => 7,
        _ => return Err(invalid("unsupported HKCL VarUInt")),
    };
    let mut value = (first
        & match marker {
            0x10..=0x17 => 0x3f,
            0x18..=0x1b => 0x1f,
            _ => 0x07,
        }) as u64;
    if *cursor > end.saturating_sub(extra) || *cursor + extra > end {
        return Err(invalid("truncated HKCL VarUInt"));
    }
    for _ in 0..extra {
        let byte = data[*cursor];
        value = (value << 8) | byte as u64;
        *cursor += 1;
    }
    Ok(value)
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::binary::BinaryWriter;
    use roead::{
        sarc::{Sarc, SarcWriter},
        Endian as SarcEndian,
    };
    use std::{fs, path::PathBuf};

    fn mk_nested_section_header(tag: &str, payload_size: u32) -> [u8; 8] {
        let mut out = [0u8; 8];
        let size = payload_size + 8;
        out[0..4].copy_from_slice(&(size | 0x8000_0000).to_be_bytes());
        let signature = tag.as_bytes();
        let mut signature_bytes = [0u8; 4];
        let name_len = signature.len().min(4);
        signature_bytes[..name_len].copy_from_slice(&signature[..name_len]);
        out[4..8].copy_from_slice(&signature_bytes);
        out
    }

    fn packfile_with_sections(endian: Endian, sections: &[(String, Vec<u8>)]) -> Vec<u8> {
        let mut writer = BinaryWriter::with_endian(endian);
        writer.write_bytes(&[0x57, 0xe0, 0xe0, 0x57, 0x10, 0xc0, 0xc0, 0x10]);
        writer.write_u32(0);
        writer.write_u32(11);
        writer.write_bytes(&[4, if endian == Endian::Little { 1 } else { 0 }, 1, 1]);
        writer.write_i32(sections.len() as i32);
        writer.write_i32(-1);
        writer.write_u32(0);
        writer.write_i32(-1);
        writer.write_u32(0);
        let mut version = [0u8; 16];
        let version_text = b"hk_2014.1.0-r1";
        version[..version_text.len()].copy_from_slice(version_text);
        writer.write_bytes(&version);
        writer.write_u32(0);
        writer.write_u16(0);
        writer.write_u16(0);

        let section_count = sections.len();
        let mut section_meta = Vec::new();
        let start = HEADER_SIZE + section_count * SECTION_HEADER_SIZE;
        let mut cursor = start;
        let write_u32 = |value: u32| endian.u32_to_bytes(value);
        for (tag, payload) in sections {
            let mut tag_bytes = [0u8; 16];
            let signature = tag.as_bytes();
            let name_len = signature.len().min(16);
            tag_bytes[..name_len].copy_from_slice(&signature[..name_len]);
            section_meta.extend_from_slice(&tag_bytes);
            section_meta.extend_from_slice(&write_u32(0));
            section_meta.extend_from_slice(&write_u32(cursor as u32));
            let payload_len = payload.len() as u32;
            section_meta.extend_from_slice(&write_u32(payload_len));
            section_meta.extend_from_slice(&write_u32(payload_len));
            section_meta.extend_from_slice(&write_u32(payload_len));
            section_meta.extend_from_slice(&write_u32(payload_len));
            section_meta.extend_from_slice(&write_u32(payload_len));
            section_meta.extend_from_slice(&write_u32(payload_len));
            section_meta.extend_from_slice(&[0; 16]);
            cursor += payload.len();
        }
        assert_eq!(section_meta.len(), section_count * SECTION_HEADER_SIZE);
        writer.write_bytes(&section_meta);
        for (_, payload) in sections {
            writer.write_bytes(payload);
        }
        writer.into_inner()
    }

    fn build_nested_section(tag: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let header = mk_nested_section_header(tag, payload.len() as u32);
        out.extend_from_slice(&header[0..8]);
        out.extend_from_slice(payload);
        out
    }

    fn encode_words(endian: Endian, words: &[u32]) -> Vec<u8> {
        let mut writer = BinaryWriter::with_endian(endian);
        for word in words {
            writer.write_u32(*word);
        }
        writer.into_inner()
    }

    fn packfile_with_classic_graph(endian: Endian) -> Vec<u8> {
        let mut writer = BinaryWriter::with_endian(endian);
        writer.write_bytes(&[0x57, 0xe0, 0xe0, 0x57, 0x10, 0xc0, 0xc0, 0x10]);
        writer.write_u32(0);
        writer.write_u32(11);
        writer.write_bytes(&[4, if endian == Endian::Little { 1 } else { 0 }, 1, 1]);
        writer.write_i32(2);
        writer.write_i32(-1);
        writer.write_u32(0);
        writer.write_i32(-1);
        writer.write_u32(0);
        let mut version = [0u8; 16];
        let version_text = b"hk_2014.1.0-r1";
        version[..version_text.len()].copy_from_slice(version_text);
        writer.write_bytes(&version);
        writer.write_u32(0);
        writer.write_u16(0);
        writer.write_u16(0);

        let class_names = b"hkRoot\0";
        let class_start = HEADER_SIZE + 2 * SECTION_HEADER_SIZE;
        let data_start = class_start + class_names.len();
        let mut write_section = |tag: &str, start: usize, offsets: [u32; 6]| {
            let mut tag_bytes = [0u8; 16];
            tag_bytes[..tag.len()].copy_from_slice(tag.as_bytes());
            writer.write_bytes(&tag_bytes);
            writer.write_u32(0);
            writer.write_u32(start as u32);
            for offset in offsets {
                writer.write_u32(offset);
            }
            writer.write_bytes(&[0xff; 16]);
        };
        write_section("__classnames__", class_start, [7; 6]);
        write_section("__data__", data_start, [8, 16, 28, 40, 40, 40]);

        writer.write_bytes(class_names);
        writer.write_bytes(&[0; 8]);
        for word in [0, 4, 4, 1, 0, 0, 0, 0] {
            writer.write_u32(word);
        }
        writer.into_inner()
    }

    #[test]
    fn parses_little_and_big_endian_packfile_headers() {
        let section = b"ROOT\0hkRoot\0\0\0\0\0".to_vec();
        for endian in [Endian::Little, Endian::Big] {
            let bytes = packfile_with_sections(endian, &[("DATA".to_owned(), section.clone())]);
            let document = HkclDocument::parse(&bytes).unwrap();
            assert_eq!(document.header.layout.endian, endian);
            assert_eq!(document.header.file_version, 11);
            assert_eq!(document.header.contents_version, "hk_2014.1.0-r1");
            assert_eq!(document.contents_offset, None);
            assert_eq!(document.contents_class_name, None);
            assert_eq!(document.sections[0].data(), 0x80..0x90);
        }
    }

    #[test]
    fn parses_item_ptch_and_type_graph_sections() {
        let mut strings = b"hclCloth\0hclMesh\0".to_vec();
        if !strings.ends_with(&[0]) {
            strings.push(0);
        }
        let type_payloads = [
            build_nested_section("TST1", &strings),
            build_nested_section("TNAM", &[3, 0, 0, 1, 0]),
        ];
        let mut type_payload = Vec::new();
        for payload in &type_payloads {
            type_payload.extend_from_slice(payload);
        }
        for endian in [Endian::Little, Endian::Big] {
            let bytes = packfile_with_sections(
                endian,
                &[
                    (
                        "ITEM".to_owned(),
                        encode_words(endian, &[0, 0, 0, 1, 0x10, 1]),
                    ),
                    ("PTCH".to_owned(), encode_words(endian, &[1, 1, 0])),
                    ("TYPE".to_owned(), type_payload.clone()),
                    ("DATA".to_owned(), encode_words(endian, &[1, 0, 0, 4])),
                ],
            );
            let document = HkclDocument::parse(&bytes).unwrap();
            assert_eq!(document.header.layout.endian, endian);
            assert_eq!(document.items.len(), 2);
            assert_eq!(document.items[1].data_offset, 0x10);
            assert_eq!(document.patches.len(), 1);
            assert_eq!(document.patches[0].offsets, vec![0]);
            assert_eq!(document.type_names.len(), 3);
            assert_eq!(document.type_names[1], "hclCloth");
            assert_eq!(document.type_names[2], "hclMesh");
            assert!(document.data_payload().unwrap().is_some());
        }
    }

    #[test]
    fn parses_classic_packfile_graph_in_both_endians() {
        for endian in [Endian::Little, Endian::Big] {
            let document = HkclDocument::parse(&packfile_with_classic_graph(endian)).unwrap();
            assert_eq!(document.header.layout.endian, endian);
            assert_eq!(document.type_names, vec!["", "hkRoot"]);
            assert_eq!(document.items.len(), 1);
            assert_eq!(document.items[0].data_section_index, 1);
            assert_eq!(document.items[0].data_offset, 0);
            assert_eq!(document.local_fixups.len(), 1);
            assert_eq!(document.local_fixups[0].destination_offset, 4);
            assert_eq!(document.global_fixups.len(), 1);
            assert_eq!(document.global_fixups[0].destination_section_index, 1);
            assert_eq!(document.virtual_fixups.len(), 1);
            assert_eq!(document.virtual_fixups[0].class_name_section_index, 0);
            assert_eq!(document.data_range().unwrap().len(), 8);
        }
    }

    #[test]
    fn rejects_non_monotonic_section_ranges() {
        let mut bytes = [0u8; HEADER_SIZE + SECTION_HEADER_SIZE];
        bytes[0x58..0x5c].copy_from_slice(&0x18u32.to_le_bytes());
        assert!(HkclDocument::parse(&bytes).is_err());
    }

    #[test]
    fn rejects_truncated_tnam_varuint() {
        let type_payload = [
            build_nested_section("TST1", &[0]),
            build_nested_section("TNAM", &[0x80]),
        ]
        .into_iter()
        .collect::<Vec<_>>()
        .concat();
        let bytes = packfile_with_sections(Endian::Little, &[("TYPE".to_owned(), type_payload)]);
        assert!(HkclDocument::parse(&bytes).is_err());
    }

    #[test]
    fn rejects_truncated_ptch_offsets() {
        let item_payload = [
            0u8, 0, 0, 0, // flags
            0, 0, 0, 0, // data offset
            0, 0, 0, 0, // count
        ]
        .to_vec();
        let ptch_payload = [
            1, 0, 0, 0, // type index
            2, 0, 0, 0, // count
            0, 0, 0, 0, // one offset only
        ]
        .to_vec();
        let bytes = packfile_with_sections(
            Endian::Little,
            &[
                ("ITEM".to_owned(), item_payload),
                ("PTCH".to_owned(), ptch_payload),
            ],
        );
        assert!(HkclDocument::parse(&bytes).is_err());
    }

    fn hkcl_corpus() -> Vec<(String, HkclDocument)> {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/hkcl");
        let mut paths: Vec<_> = fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
            .map(|entry| entry.expect("failed to read corpus entry").path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "hkcl")
            })
            .collect();
        paths.sort();
        assert!(!paths.is_empty(), "HKCL corpus is empty");
        paths
            .into_iter()
            .map(|path| {
                let name = path
                    .file_name()
                    .expect("corpus file has no name")
                    .to_string_lossy()
                    .into_owned();
                let bytes = fs::read(&path)
                    .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
                let document = HkclDocument::parse(&bytes)
                    .unwrap_or_else(|error| panic!("failed to parse {name}: {error}"));
                document
                    .validate()
                    .unwrap_or_else(|error| panic!("failed to validate {name}: {error}"));
                (name, document)
            })
            .collect()
    }

    #[test]
    fn parses_and_validates_hkcl_corpus_graphs() {
        for (name, document) in hkcl_corpus() {
            if document.section("__data__").is_some() {
                assert!(
                    document.type_names.len() > 1,
                    "HKCL virtual TYPE graph in {name} did not parse any type names"
                );
                assert!(
                    !document.items.is_empty(),
                    "HKCL virtual ITEM graph in {name} is empty"
                );
                assert!(
                    !document.virtual_fixups.is_empty(),
                    "HKCL virtual fixup graph in {name} is empty"
                );
                assert!(
                    !document.local_fixups.is_empty() || !document.global_fixups.is_empty(),
                    "HKCL pointer fixup graph in {name} is empty"
                );
                assert!(
                    document.data_range().is_ok(),
                    "HKCL DATA is missing in {name}"
                );
                let type_count = |class_name: &str| {
                    document
                        .items
                        .iter()
                        .filter(|item| {
                            document
                                .type_names
                                .get(item.type_index as usize)
                                .is_some_and(|name| name == class_name)
                        })
                        .count()
                };
                assert_eq!(
                    document.physics.skeletons.len(),
                    type_count("hkaSkeleton"),
                    "HKCL skeleton count mismatch in {name}"
                );
                assert_eq!(
                    document.physics.cloths.len(),
                    type_count("hclClothData"),
                    "HKCL cloth count mismatch in {name}"
                );
                assert_eq!(
                    document.physics.collidables.len(),
                    type_count("hclCollidable"),
                    "HKCL collidable count mismatch in {name}"
                );
                assert!(
                    document.physics.skeletons.iter().all(|skeleton| {
                        !skeleton.bones.is_empty()
                            && skeleton.bones.iter().all(|bone| bone.name.is_some())
                    }),
                    "HKCL skeleton bones did not parse in {name}"
                );
                assert!(
                    document.physics.cloths.iter().all(|cloth| {
                        cloth.name.is_some()
                            && !cloth.simulations.is_empty()
                            && cloth.simulations.iter().all(|simulation| {
                                !simulation.particles.is_empty()
                                    && !simulation.constraints.is_empty()
                                    && simulation
                                        .particles
                                        .iter()
                                        .all(|particle| particle.position.is_some())
                            })
                    }),
                    "HKCL cloth/particle graph did not parse in {name}"
                );
                assert!(
                    !document.physics.constraints.is_empty()
                        && document
                            .physics
                            .constraints
                            .iter()
                            .all(|constraint| constraint.name.is_some()),
                    "HKCL constraints did not parse in {name}"
                );
                assert!(
                    document
                        .physics
                        .collidables
                        .iter()
                        .all(|collidable| collidable.name.is_some()),
                    "HKCL collidables did not parse in {name}"
                );
                continue;
            }

            assert!(
                document.section("ITEM").is_some() || document.items.is_empty(),
                "HKCL ITEM section metadata is inconsistent in {name}"
            );
            assert!(
                document.section("PTCH").is_some() || document.patches.is_empty(),
                "HKCL PTCH section metadata is inconsistent in {name}"
            );
            assert!(
                document.section("DATA").is_some() || document.items.is_empty(),
                "HKCL DATA section metadata is inconsistent in {name}"
            );

            if let Ok(Some(_)) = document.patches_payload() {
                document
                    .validate_item_graph()
                    .unwrap_or_else(|error| panic!("invalid ITEM/PTCH graph in {name}: {error}"));
                let roots: Vec<usize> = (0..document.items.len()).collect();
                document
                    .collect_item_closure(roots.clone())
                    .unwrap_or_else(|error| {
                        panic!("failed collecting ITEM closure in {name}: {error}")
                    });
                for item in &document.patches {
                    for offset in &item.offsets {
                        document
                            .resolve_patched_item(*offset)
                            .unwrap_or_else(|error| {
                                panic!("failed resolving PTCH offset in {name} at {offset:#x}: {error}")
                            });
                    }
                }
            }
        }
    }

    #[test]
    fn validation_rejects_broken_corpus_physics_references() {
        let (_name, mut document) = hkcl_corpus()
            .into_iter()
            .find(|(_, document)| {
                document.physics.cloths.iter().any(|cloth| {
                    cloth
                        .simulations
                        .iter()
                        .any(|simulation| !simulation.constraints.is_empty())
                })
            })
            .expect("HKCL corpus has no simulation constraint references");
        document.physics.constraints.clear();
        let error = document.validate().unwrap_err();
        assert!(error.to_string().contains("missing constraint"));
    }

    #[test]
    fn validation_rejects_broken_corpus_skeleton_topology() {
        let (_name, mut document) = hkcl_corpus()
            .into_iter()
            .find(|(_, document)| {
                document
                    .physics
                    .skeletons
                    .iter()
                    .any(|skeleton| !skeleton.bones.is_empty())
            })
            .expect("HKCL corpus has no skeleton bones");
        let skeleton = document
            .physics
            .skeletons
            .iter_mut()
            .find(|skeleton| !skeleton.bones.is_empty())
            .unwrap();
        skeleton.bones[0].parent_index = 0;
        let error = document.validate().unwrap_err();
        assert!(error.to_string().contains("invalid parent"));
    }

    #[test]
    fn serializes_read_only_yaml_leaves_for_hkcl_corpus() {
        for (name, document) in hkcl_corpus() {
            let yaml = document
                .to_yaml()
                .unwrap_or_else(|error| panic!("failed to serialize {name}: {error}"));
            let root: serde_yaml::Value = serde_yaml::from_str(&yaml)
                .unwrap_or_else(|error| panic!("invalid document YAML for {name}: {error}"));
            assert_eq!(root["format"].as_str(), Some("HKCL"));

            let leaves = document
                .leaves()
                .unwrap_or_else(|error| panic!("failed to build leaves for {name}: {error}"));
            let simulation_count: usize = document
                .physics
                .cloths
                .iter()
                .map(|cloth| cloth.simulations.len())
                .sum();
            assert_eq!(
                leaves.len(),
                document.physics.skeletons.len()
                    + document.physics.cloths.len()
                    + simulation_count
                    + document.physics.constraints.len()
                    + document.physics.collidables.len(),
                "leaf count mismatch in {name}"
            );
            let mut paths = HashSet::new();
            for leaf in leaves {
                assert!(leaf.read_only, "writable HKCL leaf in {name}");
                assert!(paths.insert(leaf.path.clone()), "duplicate leaf in {name}");
                serde_yaml::from_str::<serde_yaml::Value>(&leaf.yaml).unwrap_or_else(|error| {
                    panic!("invalid YAML in leaf {} from {name}: {error}", leaf.path)
                });
            }
            assert_eq!(yaml, document.to_yaml().unwrap(), "unstable YAML in {name}");
        }
    }

    #[test]
    fn hkcl_corpus_preserves_bytes_and_survives_root_and_nested_sarc_reopen() {
        let corpus = hkcl_corpus();
        for (name, document) in &corpus {
            assert_eq!(
                HkclDocument::parse(&document.raw).unwrap().raw,
                document.raw,
                "raw roundtrip changed {name}"
            );
            document
                .validate_item_graph()
                .unwrap_or_else(|error| panic!("pointer validation failed for {name}: {error}"));
        }
        let (name, document) = &corpus[0];
        let mut root = SarcWriter::new(SarcEndian::Little);
        root.add_file(&format!("Physics/{name}"), document.raw.clone());
        let reopened = Sarc::new(root.to_binary()).unwrap();
        let root_bytes = reopened.get_data(&format!("Physics/{name}")).unwrap();
        HkclDocument::parse(root_bytes).unwrap().validate().unwrap();

        let mut inner = SarcWriter::new(SarcEndian::Little);
        inner.add_file(&format!("Physics/{name}"), document.raw.clone());
        let mut outer = SarcWriter::new(SarcEndian::Little);
        outer.add_file("Nested/physics.pack", inner.to_binary());
        let outer = Sarc::new(outer.to_binary()).unwrap();
        let inner = Sarc::new(outer.get_data("Nested/physics.pack").unwrap().to_vec()).unwrap();
        HkclDocument::parse(inner.get_data(&format!("Physics/{name}")).unwrap())
            .unwrap()
            .validate()
            .unwrap();
    }
}
