use super::{
    blackboard::{read_blackboard, Blackboard},
    command::AinbCommand,
    expression::ExpressionModule,
    header::{AinbHeader, HEADER_SIZE},
    model::{Action, Attachment, Module, Replacement, ReplacementType, UnknownSection58},
    node::{AinbNode, NodeTables},
    parameter::{read_multi_sources, read_param_set, read_property_set},
    plug::Transition,
    plug::{plug_size, PLUG_NAMES},
    section::AinbSection,
    writer::write_document,
};
use crate::parser::binary::BinaryReader;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::BTreeMap, io};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AinbDocument {
    #[serde(rename = "Version")]
    pub version: u32,
    #[serde(rename = "Filename")]
    pub filename: String,
    #[serde(rename = "Category")]
    pub category: String,
    #[serde(rename = "Blackboard ID")]
    pub blackboard_id: u32,
    #[serde(rename = "Parent Blackboard ID")]
    pub parent_blackboard_id: u32,
    #[serde(skip)]
    pub header: AinbHeader,
    #[serde(rename = "Commands")]
    pub commands: Vec<AinbCommand>,
    #[serde(rename = "Nodes")]
    pub nodes: Vec<AinbNode>,
    #[serde(rename = "Blackboard")]
    pub blackboard: Blackboard,
    #[serde(
        rename = "Expressions",
        serialize_with = "serialize_optional_mapping",
        deserialize_with = "deserialize_optional_mapping"
    )]
    pub expressions: Option<ExpressionModule>,
    #[serde(rename = "Replacement Table")]
    pub replacement_table: Vec<Replacement>,
    #[serde(rename = "Modules")]
    pub modules: Vec<Module>,
    #[serde(
        rename = "Unknown Section 0x58",
        serialize_with = "serialize_optional_mapping",
        deserialize_with = "deserialize_optional_mapping"
    )]
    pub unknown_section_0x58: Option<UnknownSection58>,
    #[serde(rename = "Has Section 0x6C")]
    pub has_section_0x6c: bool,
    #[serde(rename = "Serialization Metadata", default)]
    pub serialization_metadata: SerializationMetadata,
    #[serde(skip)]
    pub sections: Vec<AinbSection>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SerializationMetadata {
    #[serde(rename = "String Pool", default)]
    pub string_pool: Vec<String>,
    #[serde(rename = "Node Query Base Indices", default)]
    pub node_query_base_indices: Vec<u16>,
    #[serde(rename = "Query Table", default)]
    pub query_table: Vec<u32>,
    #[serde(rename = "Node Multi Counts", default)]
    pub node_multi_counts: Vec<u16>,
    #[serde(rename = "Computed Node Multi Counts", default)]
    pub computed_node_multi_counts: Vec<u16>,
    #[serde(rename = "Node Parameter Sizes", default)]
    pub node_parameter_sizes: Vec<u32>,
    #[serde(rename = "Node Parameter Tail Bytes", default)]
    pub node_parameter_tail_bytes: Vec<String>,
    #[serde(rename = "Output Count", default)]
    pub output_count: Option<u32>,
    #[serde(rename = "Replacement New Indices", default)]
    pub replacement_new_indices: Vec<i16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_ainb_data() {
        let error = AinbDocument::from_bytes(b"not an ainb").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}

impl AinbDocument {
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        let mut reader = BinaryReader::new(data);
        let header = AinbHeader::read(&mut reader)?;
        let pool = header.string_pool_offset as usize;
        let mut string_pool = data
            .get(pool..)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "string pool exceeds input"))?
            .split(|byte| *byte == 0)
            .map(|value| {
                String::from_utf8(value.to_vec())
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
            })
            .collect::<io::Result<Vec<_>>>()?;
        if string_pool.last().is_some_and(String::is_empty) {
            string_pool.pop();
        }
        let filename = reader.read_c_string_at(pool + header.filename_offset as usize)?;
        let category = reader.read_c_string_at(pool + header.category_name_offset as usize)?;
        reader.seek(HEADER_SIZE)?;
        let commands = (0..header.command_count)
            .map(|_| AinbCommand::read(&mut reader, pool))
            .collect::<io::Result<Vec<_>>>()?;
        let node_offset = reader.position();
        let blackboard = read_blackboard(
            &mut reader,
            header.blackboard_offset as usize,
            pool,
            header.version,
        )?;
        reader.seek(header.module_offset as usize)?;
        let module_count = reader.read_u32()?;
        let modules = (0..module_count)
            .map(|_| {
                Ok(Module {
                    path: read_string_offset(&mut reader, pool)?,
                    category: read_string_offset(&mut reader, pool)?,
                    instance_count: reader.read_u32()?,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        reader.seek(header.property_offset as usize)?;
        let properties = read_property_set(&mut reader, pool, header.io_param_offset as usize)?;
        reader.seek(header.attachment_offset as usize)?;
        let attachments = (0..header.attachment_count)
            .map(|_| Attachment::read(&mut reader, pool, header.version, &properties))
            .collect::<io::Result<Vec<_>>>()?;
        reader.seek(header.attachment_index_offset as usize)?;
        let attachment_indices = (0..(header.attachment_offset - header.attachment_index_offset)
            / 4)
            .map(|_| reader.read_u32())
            .collect::<io::Result<Vec<_>>>()?;
        let multi_sources = read_multi_sources(
            &mut reader,
            header.multi_param_offset as usize,
            header.transition_offset as usize,
        )?;
        reader.seek(header.io_param_offset as usize)?;
        let parameters = read_param_set(
            &mut reader,
            pool,
            header.multi_param_offset as usize,
            &multi_sources,
        )?;
        let transitions = read_transitions(
            &mut reader,
            pool,
            header.transition_offset as usize,
            header.query_offset as usize,
        )?;
        let query_end = if header.expression_offset != 0 {
            header.expression_offset
        } else {
            header.module_offset
        };
        reader.seek(header.query_offset as usize)?;
        let queries = (0..(query_end - header.query_offset) / 4)
            .map(|_| {
                let value = reader.read_u16()? as u32;
                reader.read_u16()?;
                Ok(value)
            })
            .collect::<io::Result<Vec<_>>>()?;
        reader.seek(header.action_offset as usize)?;
        let action_count = reader.read_u32()?;
        let mut actions = BTreeMap::<i32, Vec<Action>>::new();
        for _ in 0..action_count {
            let index = reader.read_i32()?;
            actions.entry(index).or_default().push(Action {
                action_slot: read_string_offset(&mut reader, pool)?,
                action: read_string_offset(&mut reader, pool)?,
            });
        }
        let (replacement_table, replacement_new_indices) = if header.version >= 0x407 {
            reader.seek(header.replacement_offset as usize)?;
            reader.read_u8()?;
            reader.read_u8()?;
            let count = reader.read_u16()?;
            reader.read_i16()?;
            reader.read_i16()?;
            let mut raw_new_indices = Vec::new();
            let replacements = (0..count)
                .map(|_| {
                    let kind = ReplacementType::from_raw(reader.read_u8()?)?;
                    reader.read_u8()?;
                    let node_index = reader.read_i16()?;
                    let replace_index = reader.read_i16()?;
                    let new_index = reader.read_i16()?;
                    raw_new_indices.push(new_index);
                    Ok(Replacement {
                        replacement_type: format!("{kind:?}"),
                        node_index,
                        child_plug_index: (kind != ReplacementType::RemoveAttachment)
                            .then_some(replace_index),
                        attachment_index: (kind == ReplacementType::RemoveAttachment)
                            .then_some(replace_index),
                        replacement_node_index: (kind == ReplacementType::ReplaceChild)
                            .then_some(new_index),
                    })
                })
                .collect::<io::Result<Vec<_>>>()?;
            (replacements, raw_new_indices)
        } else {
            (Vec::new(), Vec::new())
        };
        let unknown_section_0x58 = if header.section_0x58_offset != 0 {
            reader.seek(header.section_0x58_offset as usize)?;
            Some(UnknownSection58 {
                description: read_string_offset(&mut reader, pool)?,
                unknown04: reader.read_u32()?,
                unknown08: reader.read_u32()?,
                unknown0c: reader.read_u32()?,
            })
        } else {
            None
        };
        let expressions = if header.expression_offset != 0 {
            Some(ExpressionModule::from_bytes(reader.slice(
                header.expression_offset as usize,
                header.module_offset as usize,
            )?)?)
        } else {
            None
        };
        reader.seek(node_offset)?;
        let tables = NodeTables {
            attachments: &attachments,
            attachment_indices: &attachment_indices,
            properties: &properties,
            parameters: &parameters,
            transitions: &transitions,
            queries: &queries,
            actions: &actions,
        };
        let mut nodes = (0..header.node_count)
            .map(|_| AinbNode::read(&mut reader, header.version, pool, &tables))
            .collect::<io::Result<Vec<_>>>()?;
        let query_nodes = nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.raw_flags & 1 != 0)
            .map(|(index, _)| index as u32)
            .collect::<Vec<_>>();
        for node in &mut nodes {
            node.queries = node
                .queries
                .iter()
                .map(|index| {
                    query_nodes.get(*index as usize).copied().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "query index exceeds query-node table",
                        )
                    })
                })
                .collect::<io::Result<Vec<_>>>()?;
        }
        reader.seek(header.blackboard_id_offset as usize)?;
        let blackboard_id = reader.read_u32()?;
        let parent_blackboard_id = reader.read_u32()?;
        let has_section_0x6c = header.section_0x6c_offset != 0;
        let mut offsets = BTreeMap::new();
        for (name, offset) in header.named_offsets() {
            if offset > 0 && (offset as usize) < data.len() {
                offsets.entry(offset as usize).or_insert(name);
            }
        }
        offsets.entry(pool).or_insert("string_pool");
        let entries: Vec<_> = offsets.into_iter().collect();
        let sections = entries
            .iter()
            .enumerate()
            .map(|(index, (offset, name))| {
                let end = entries
                    .get(index + 1)
                    .map(|entry| entry.0)
                    .unwrap_or(data.len());
                AinbSection::new(name, *offset, &data[*offset..end])
            })
            .collect();
        let node_query_base_indices = nodes.iter().map(|node| node.raw_query_base).collect();
        let node_multi_counts = nodes.iter().map(|node| node.raw_multi_count).collect();
        let computed_node_multi_counts = nodes.iter().map(computed_multi_count).collect();
        let mut node_parameter_sizes = Vec::with_capacity(nodes.len());
        let mut node_parameter_tail_bytes = Vec::with_capacity(nodes.len());
        for (index, node) in nodes.iter().enumerate() {
            let start = node.raw_parameter_offset as usize;
            let end = nodes
                .get(index + 1)
                .map(|next| next.raw_parameter_offset as usize)
                .unwrap_or(header.attachment_index_offset as usize);
            let canonical_size = 0xa4
                + PLUG_NAMES
                    .into_iter()
                    .enumerate()
                    .map(|(plug_type, name)| {
                        node.plugs
                            .get(name)
                            .into_iter()
                            .flatten()
                            .map(|plug| {
                                plug_size(plug, &node.node_type, &node.name, plug_type)
                                    .map(|size| size + 4)
                            })
                            .sum::<io::Result<usize>>()
                    })
                    .sum::<io::Result<usize>>()?;
            let actual_size = end.saturating_sub(start);
            node_parameter_sizes.push(actual_size as u32);
            let tail_size = actual_size.saturating_sub(canonical_size);
            node_parameter_tail_bytes.push(if tail_size == 0 {
                String::new()
            } else {
                data[end - tail_size..end]
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect()
            });
        }
        let output_count = header.output_count;
        Ok(Self {
            version: header.version,
            filename,
            category,
            blackboard_id,
            parent_blackboard_id,
            header,
            commands,
            nodes,
            blackboard,
            expressions,
            replacement_table,
            modules,
            unknown_section_0x58,
            has_section_0x6c,
            serialization_metadata: SerializationMetadata {
                string_pool,
                node_query_base_indices,
                query_table: queries.clone(),
                node_multi_counts,
                computed_node_multi_counts,
                node_parameter_sizes,
                node_parameter_tail_bytes,
                output_count: Some(output_count),
                replacement_new_indices,
            },
            sections,
        })
    }

    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        write_document(self)
    }

    pub fn to_yaml(&self) -> io::Result<String> {
        serde_yaml::to_string(self).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
    pub fn from_yaml(text: &str) -> io::Result<Self> {
        serde_yaml::from_str(text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

pub(super) fn computed_multi_count(node: &AinbNode) -> u16 {
    node.parameters
        .inputs
        .values()
        .flatten()
        .map(|input| input.sources.as_ref().map_or(0, Vec::len) as u16)
        .sum()
}

fn read_string_offset(reader: &mut BinaryReader<'_>, pool: usize) -> io::Result<String> {
    let offset = reader.read_u32()? as usize;
    reader.read_c_string_at(pool + offset)
}

fn read_transitions(
    reader: &mut BinaryReader<'_>,
    pool: usize,
    offset: usize,
    end: usize,
) -> io::Result<Vec<Transition>> {
    if offset >= end {
        return Ok(Vec::new());
    }
    reader.seek(offset)?;
    let first = reader.read_u32()? as usize;
    if first < offset + 4 || first > end {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid transition offset table",
        ));
    }
    let count = (first - offset) / 4;
    reader.seek(offset)?;
    let offsets = (0..count)
        .map(|_| reader.read_u32().map(|value| value as usize))
        .collect::<io::Result<Vec<_>>>()?;
    offsets
        .into_iter()
        .map(|offset| {
            reader.seek(offset)?;
            let flags = reader.read_u32()?;
            Ok(Transition {
                transition_type: flags & 0xff,
                update_post_calc: flags >> 31 != 0,
                command_name: if flags & 0xff == 0 {
                    read_string_offset(reader, pool)?
                } else {
                    String::new()
                },
            })
        })
        .collect()
}

fn serialize_optional_mapping<T, S>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: Serializer,
{
    match value {
        Some(value) => value.serialize(serializer),
        None => serde_yaml::Mapping::new().serialize(serializer),
    }
}

fn deserialize_optional_mapping<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: DeserializeOwned,
    D: Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    if value.as_mapping().is_some_and(|mapping| mapping.is_empty()) || value.is_null() {
        Ok(None)
    } else {
        serde_yaml::from_value(value)
            .map(Some)
            .map_err(serde::de::Error::custom)
    }
}
