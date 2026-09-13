use super::baev_array::BaevArray;
use crate::parser::binary::BinaryReader;
use std::io;

#[derive(Clone, Debug)]
pub struct BaevSectionHeader {
    pub magic: String,
    pub section_offset: u32,
    pub section_size: u32,
    pub alignment: u32,
    pub base_offset: u64,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct BaevFileHeader {
    pub magic: String,
    pub section_offset: u32,
    pub file_size: u32,
    pub alignment: u32,
    pub sections: Vec<BaevSectionHeader>,
    pub container_offset: u64,
    pub resource_name: String,
}

impl BaevFileHeader {
    pub fn read(reader: &mut BinaryReader<'_>) -> io::Result<Self> {
        let magic = read_fixed(reader, 4)?;
        if magic != "BFFH" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid BAEV magic {magic:?}"),
            ));
        }
        let section_offset = reader.read_u32()?;
        let file_size = reader.read_u32()?;
        let alignment = reader.read_u32()?;
        let array = BaevArray::read(reader)?;
        let container_offset = reader.read_u64()?;
        let resource_name = read_fixed(reader, 0x80)?;
        let return_position = reader.position();
        reader.seek(array.offset()?)?;
        let mut sections = Vec::new();
        for _ in 0..array.count {
            sections.push(BaevSectionHeader::read(reader)?);
        }
        reader.seek(return_position)?;
        Ok(Self {
            magic,
            section_offset,
            file_size,
            alignment,
            sections,
            container_offset,
            resource_name,
        })
    }
}

impl BaevSectionHeader {
    fn read(reader: &mut BinaryReader<'_>) -> io::Result<Self> {
        Ok(Self {
            magic: read_fixed(reader, 4)?,
            section_offset: reader.read_u32()?,
            section_size: reader.read_u32()?,
            alignment: reader.read_u32()?,
            base_offset: reader.read_u64()?,
            name: read_fixed(reader, 0x10)?,
        })
    }
}

fn read_fixed(reader: &mut BinaryReader<'_>, size: usize) -> io::Result<String> {
    let bytes = reader.read_bytes(size)?;
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8(bytes[..end].to_vec())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
