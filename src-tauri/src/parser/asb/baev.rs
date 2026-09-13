use super::{
    baev_array::BaevArray,
    baev_header::BaevFileHeader,
    baev_node::{BaevEventInfo, BaevNode},
};
use crate::parser::binary::BinaryReader;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Baev {
    pub events: BTreeMap<String, Vec<BaevNode>>,
}

impl Baev {
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        let mut reader = BinaryReader::new(data);
        let header = BaevFileHeader::read(&mut reader)?;
        let container_offset = header
            .sections
            .first()
            .map(|section| section.base_offset)
            .unwrap_or(header.container_offset);
        reader.seek(usize::try_from(container_offset).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "BAEV container offset exceeds usize",
            )
        })?)?;
        reader.read_u64()?;
        reader.read_u8()?;
        reader.read_u8()?;
        reader.read_u16()?;
        reader.read_u32()?;
        reader.read_u64()?;
        let event_info_array = BaevArray::read(&mut reader)?;
        let node_array = BaevArray::read(&mut reader)?;

        let return_position = reader.position();
        reader.seek(event_info_array.offset()?)?;
        let mut event_info = Vec::new();
        for _ in 0..event_info_array.count {
            event_info.push(BaevEventInfo::read(&mut reader)?);
        }
        reader.seek(node_array.offset()?)?;
        let mut nodes = Vec::new();
        for _ in 0..node_array.count {
            nodes.push(BaevNode::read(&mut reader)?);
        }
        reader.seek(return_position)?;

        let mut events = BTreeMap::new();
        for info in event_info {
            let mut group = Vec::with_capacity(info.node_indices.len());
            for index in info.node_indices {
                group.push(
                    nodes
                        .get(index as usize)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("BAEV node index {index} exceeds table"),
                            )
                        })?
                        .clone(),
                );
            }
            events.insert(info.hash, group);
        }
        Ok(Self { events })
    }

    pub fn from_yaml(text: &str) -> io::Result<Self> {
        serde_yaml::from_str(text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn to_yaml(&self) -> io::Result<String> {
        serde_yaml::to_string(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }
}

pub fn calc_hash(value: &str, seed: &str) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in seed.as_bytes().iter().chain(value.as_bytes()) {
        hash = (hash ^ u32::from(*byte)).wrapping_mul(0x01000193);
    }
    hash
}
