use super::parameter::{read_parameter, ParameterType};
use crate::parser::binary::BinaryReader;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::io;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transition {
    #[serde(rename = "Unknown")]
    pub unknown: i32,
    #[serde(rename = "Transitions")]
    pub entries: Vec<TransitionEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransitionEntry {
    #[serde(rename = "Command 1")]
    pub command_1: String,
    #[serde(rename = "Command 2")]
    pub command_2: String,
    #[serde(rename = "Parameter Type")]
    pub parameter_type: String,
    #[serde(rename = "Allow Multiple Matches")]
    pub allow_multiple_matches: bool,
    #[serde(rename = "Parameter")]
    pub parameter: String,
    #[serde(rename = "Value")]
    pub value: Value,
    #[serde(rename = "Command Group", skip_serializing_if = "Option::is_none")]
    pub command_group: Option<Vec<String>>,
}

pub fn read_command_groups(
    reader: &mut BinaryReader<'_>,
    pool: &BinaryReader<'_>,
    offset: u32,
) -> io::Result<Vec<Vec<String>>> {
    if offset == 0 {
        return Ok(Vec::new());
    }
    reader.seek(offset as usize)?;
    let count = reader.read_u32()?;
    let mut groups = Vec::new();
    for _ in 0..count {
        let values_offset = reader.read_u32()? as usize;
        let return_position = reader.position();
        reader.seek(values_offset)?;
        let value_count = reader.read_u32()?;
        let mut values = Vec::new();
        for _ in 0..value_count {
            values.push(pool.read_c_string_at(reader.read_u32()? as usize)?);
        }
        reader.seek(return_position)?;
        groups.push(values);
    }
    Ok(groups)
}

pub fn read_transitions(
    reader: &mut BinaryReader<'_>,
    pool: &BinaryReader<'_>,
    offset: u32,
    groups: &[Vec<String>],
) -> io::Result<Vec<Transition>> {
    reader.seek(offset as usize)?;
    let count = reader.read_u32()?;
    reader.read_u32()?;
    let mut result = Vec::new();
    for _ in 0..count {
        let entry_count = reader.read_u32()?;
        let unknown = reader.read_i32()?;
        let entries_offset = reader.read_u32()? as usize;
        let return_position = reader.position();
        reader.seek(entries_offset)?;
        let mut entries = Vec::new();
        for _ in 0..entry_count {
            entries.push(read_entry(reader, pool, groups)?);
        }
        reader.seek(return_position)?;
        result.push(Transition { unknown, entries });
    }
    Ok(result)
}

fn read_entry(
    reader: &mut BinaryReader<'_>,
    pool: &BinaryReader<'_>,
    groups: &[Vec<String>],
) -> io::Result<TransitionEntry> {
    let command_1 = pool.read_c_string_at(reader.read_u32()? as usize)?;
    let command_2 = pool.read_c_string_at(reader.read_u32()? as usize)?;
    let (parameter_type, kind) = match reader.read_u8()? {
        0 => ("int", ParameterType::Int),
        1 => ("string", ParameterType::String),
        2 => ("float", ParameterType::Float),
        3 => ("bool", ParameterType::Bool),
        _ => ("vec3f", ParameterType::Vec3f),
    };
    let allow_multiple_matches = reader.read_u8()? != 0;
    let group_index = i32::from(reader.read_u16()?) - 1;
    let parameter = pool.read_c_string_at(reader.read_u32()? as usize)?;
    let value = read_parameter(reader, pool, kind)?;
    if parameter_type != "vec3f" {
        reader.skip(8)?;
    }
    let command_group = if group_index >= 0 {
        Some(
            groups
                .get(group_index as usize)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "ASB command group index exceeds table",
                    )
                })?
                .clone(),
        )
    } else {
        None
    };
    Ok(TransitionEntry {
        command_1,
        command_2,
        parameter_type: parameter_type.into(),
        allow_multiple_matches,
        parameter,
        value,
        command_group,
    })
}
