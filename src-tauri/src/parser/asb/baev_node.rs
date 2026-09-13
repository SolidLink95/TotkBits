use super::{baev_array::BaevArray, baev_event::Event};
use crate::parser::binary::BinaryReader;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BaevNode {
    #[serde(rename = "Hash")]
    pub hash: String,
    #[serde(rename = "Unknown")]
    pub unknown: u32,
    #[serde(rename = "Event")]
    pub events: BTreeMap<String, Event>,
}

impl BaevNode {
    pub fn read(reader: &mut BinaryReader<'_>) -> io::Result<Self> {
        let event_array = BaevArray::read(reader)?;
        let hash = format!("0x{:08x}", reader.read_u32()?);
        let unknown = reader.read_u32()?;
        let return_position = reader.position();
        reader.seek(event_array.offset()?)?;
        let mut events = BTreeMap::new();
        for _ in 0..event_array.count {
            let (name, event) = Event::read(reader)?;
            events.insert(name, event);
        }
        reader.seek(return_position)?;
        Ok(Self {
            hash,
            unknown,
            events,
        })
    }
}

#[derive(Clone, Debug)]
pub struct BaevEventInfo {
    pub hash: String,
    pub node_indices: Vec<u32>,
}

impl BaevEventInfo {
    pub fn read(reader: &mut BinaryReader<'_>) -> io::Result<Self> {
        let hash = format!("0x{:08x}", reader.read_u32()?);
        reader.read_u32()?;
        let array = BaevArray::read(reader)?;
        let return_position = reader.position();
        reader.seek(array.offset()?)?;
        let mut node_indices = Vec::new();
        for _ in 0..array.count {
            node_indices.push(reader.read_u32()?);
        }
        reader.seek(return_position)?;
        Ok(Self { hash, node_indices })
    }
}
