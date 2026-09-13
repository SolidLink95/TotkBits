use crate::parser::binary::BinaryReader;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnimationSlot {
    #[serde(rename = "Unknown")]
    pub unknown: u16,
    #[serde(rename = "Partial 1")]
    pub partial_1: String,
    #[serde(rename = "Partial 2")]
    pub partial_2: String,
    #[serde(rename = "Entries")]
    pub entries: Vec<AnimationSlotEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnimationSlotEntry {
    #[serde(rename = "Bone")]
    pub bone: String,
    #[serde(rename = "Unknown 1")]
    pub unknown_1: u16,
    #[serde(rename = "Unknown 2")]
    pub unknown_2: u16,
}

impl AnimationSlot {
    pub fn read(reader: &mut BinaryReader<'_>, pool: &BinaryReader<'_>) -> io::Result<Self> {
        let count = reader.read_u16()?;
        let unknown = reader.read_u16()?;
        let partial_1 = pool.read_c_string_at(reader.read_u32()? as usize)?;
        let partial_2 = pool.read_c_string_at(reader.read_u32()? as usize)?;
        let mut entries = Vec::new();
        for _ in 0..count {
            entries.push(AnimationSlotEntry {
                bone: pool.read_c_string_at(reader.read_u32()? as usize)?,
                unknown_1: reader.read_u16()?,
                unknown_2: reader.read_u16()?,
            });
        }
        Ok(Self {
            unknown,
            partial_1,
            partial_2,
            entries,
        })
    }
}
