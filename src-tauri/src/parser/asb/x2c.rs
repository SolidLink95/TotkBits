use super::parameter::{read_parameter, ParameterType};
use crate::parser::binary::BinaryReader;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::io;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct X2cEntry {
    #[serde(rename = "Source Node")]
    pub source_node: u16,
    #[serde(rename = "Target Node")]
    pub target_node: u16,
    #[serde(rename = "Unknown 1")]
    pub unknown_1: u32,
    #[serde(rename = "Unknown 2")]
    pub unknown_2: u32,
    #[serde(rename = "Unknown 3")]
    pub unknown_3: u32,
    #[serde(rename = "Entries")]
    pub entries: Vec<X2cSubEntry>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct X2cSubEntry {
    #[serde(rename = "Entry Type")]
    pub entry_type: u16,
    #[serde(rename = "Unknown Type")]
    pub unknown_type: u16,
    #[serde(rename = "Unknown 1", skip_serializing_if = "Option::is_none")]
    pub unknown_1: Option<Value>,
    #[serde(rename = "Unknown 2", skip_serializing_if = "Option::is_none")]
    pub unknown_2: Option<Value>,
}
pub fn read_x2c(
    r: &mut BinaryReader<'_>,
    p: &BinaryReader<'_>,
    offset: u32,
) -> io::Result<Vec<X2cEntry>> {
    r.seek(offset as usize)?;
    let count = r.read_u32()?;
    let mut out = Vec::new();
    for _ in 0..count {
        let source_node = r.read_u16()?;
        let target_node = r.read_u16()?;
        let unknown_1 = r.read_u32()?;
        let unknown_2 = r.read_u32()?;
        let unknown_3 = r.read_u32()?;
        let mut entries = Vec::new();
        for _ in 0..4 {
            entries.push(read_sub(r, p)?)
        }
        out.push(X2cEntry {
            source_node,
            target_node,
            unknown_1,
            unknown_2,
            unknown_3,
            entries,
        })
    }
    Ok(out)
}
fn read_sub(r: &mut BinaryReader<'_>, p: &BinaryReader<'_>) -> io::Result<X2cSubEntry> {
    let entry_type = r.read_u16()?;
    let unknown_type = r.read_u16()?;
    if entry_type == 0 {
        r.skip(16)?;
        return Ok(X2cSubEntry {
            entry_type,
            unknown_type,
            unknown_1: None,
            unknown_2: None,
        });
    }
    let kind = match entry_type {
        1 => ParameterType::Float,
        2 => ParameterType::Int,
        3 => ParameterType::Bool,
        _ => ParameterType::String,
    };
    Ok(X2cSubEntry {
        entry_type,
        unknown_type,
        unknown_1: Some(read_parameter(r, p, kind)?),
        unknown_2: Some(read_parameter(r, p, kind)?),
    })
}
