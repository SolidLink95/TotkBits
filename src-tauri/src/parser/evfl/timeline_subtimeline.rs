use super::radix_tree::read_string;
use crate::parser::binary::BinaryReader;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SubTimeline {
    pub name: String,
}

impl SubTimeline {
    pub fn read(data: &[u8], offset: u64) -> io::Result<Self> {
        let pointer = BinaryReader::new(data)
            .read_u64_at(offset as usize)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "sub-timeline pointer exceeds input",
                )
            })?;
        Ok(Self {
            name: read_string(data, pointer)?,
        })
    }
}
