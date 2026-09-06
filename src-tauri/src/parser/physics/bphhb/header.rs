use crate::parser::binary::BinaryReader;
use std::{io, io::ErrorKind};

pub const BPHHB_HEADER_SIZE: usize = 0x30;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BphhbHeader {
    pub archive_version: u32,
    pub flags: u32,
    pub file_size: u32,
    pub parameter_io_version: u32,
    pub parameter_io_offset: u32,
    pub list_count: u32,
    pub object_count: u32,
    pub parameter_count: u32,
    pub data_size: u32,
    pub string_pool_size: u32,
    pub unknown: u32,
    pub data_type: String,
}

impl BphhbHeader {
    pub fn read(data: &[u8]) -> io::Result<Self> {
        if data.len() < BPHHB_HEADER_SIZE {
            return Err(invalid("BPHHB is shorter than an AAMP header"));
        }
        if &data[..4] != b"AAMP" {
            return Err(invalid("BPHHB must begin with the AAMP signature"));
        }
        let reader = BinaryReader::new(data);
        let value = |offset| reader.read_u32_at(offset);
        let parameter_io_offset = value(0x14)?;
        let type_end = BPHHB_HEADER_SIZE
            .checked_add(parameter_io_offset as usize)
            .ok_or_else(|| invalid("BPHHB parameter root offset overflows"))?;
        if type_end >= data.len() {
            return Err(invalid("BPHHB parameter root offset lies outside the file"));
        }
        let type_bytes = &data[BPHHB_HEADER_SIZE..type_end];
        let type_len = type_bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(type_bytes.len());
        let data_type = std::str::from_utf8(&type_bytes[..type_len])
            .map_err(|_| invalid("BPHHB data type is not UTF-8"))?
            .to_owned();
        if !data_type.eq_ignore_ascii_case("phhb") {
            return Err(invalid(&format!(
                "expected BPHHB AAMP type phhb, found '{data_type}'"
            )));
        }
        Ok(Self {
            archive_version: value(0x04)?,
            flags: value(0x08)?,
            file_size: value(0x0c)?,
            parameter_io_version: value(0x10)?,
            parameter_io_offset,
            list_count: value(0x18)?,
            object_count: value(0x1c)?,
            parameter_count: value(0x20)?,
            data_size: value(0x24)?,
            string_pool_size: value(0x28)?,
            unknown: value(0x2c)?,
            data_type,
        })
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}
