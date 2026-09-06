use crate::parser::binary::BinaryReader;
use std::io::{self, ErrorKind};

#[derive(Clone, Debug)]
pub struct BphclHeader {
    pub file_type: u8,
    pub tag_offset: u32,
    pub parameter_offset: u32,
    pub tag_size: u32,
    pub parameter_size: u32,
}
impl BphclHeader {
    pub fn read(data: &[u8]) -> io::Result<Self> {
        if data.len() < 0x30 || &data[..6] != b"Phive\0" {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "missing BPHCL Phive header",
            ));
        }
        let mut r = BinaryReader::new(data);
        r.seek(10)?;
        let file_type = r.read_u8()?;
        r.skip(1)?;
        let tag_offset = r.read_u32()?;
        let parameter_offset = r.read_u32()?;
        r.skip(4)?;
        let tag_size = r.read_u32()?;
        let parameter_size = r.read_u32()?;
        if file_type != 3 {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("Phive type {file_type} is not cloth"),
            ));
        }
        Ok(Self {
            file_type,
            tag_offset,
            parameter_offset,
            tag_size,
            parameter_size,
        })
    }
}
