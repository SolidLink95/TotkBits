use crate::parser::binary::BinaryReader;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DdsHeader {
    pub width: u32,
    pub height: u32,
    pub mipmap_count: u32,
}

impl DdsHeader {
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < 128 || !crate::Settings::Magic::is_dds(data) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid DDS header",
            ));
        }
        let reader = BinaryReader::new(data);
        let read = |offset: usize| {
            reader
                .read_u32_at(offset)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid DDS header"))
        };
        Ok(Self {
            height: read(12)?,
            width: read(16)?,
            mipmap_count: read(28)?.max(1),
        })
    }
}
