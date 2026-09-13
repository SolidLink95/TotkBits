use crate::parser::binary::{BinaryReader, BinaryWriter, Endian};
use std::io;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericLabelSection(pub Vec<(u32, u32)>);

impl NumericLabelSection {
    pub fn read(data: &[u8], endian: Endian) -> io::Result<Self> {
        let mut reader = BinaryReader::with_endian(data, endian);
        let count = reader.read_u32()? as usize;
        let mut entries = Vec::new();
        for _ in 0..count {
            entries.push((reader.read_u32()?, reader.read_u32()?));
        }
        Ok(Self(entries))
    }
    pub fn write(&self, endian: Endian) -> Vec<u8> {
        let mut writer = BinaryWriter::with_endian(endian);
        writer.write_u32(self.0.len() as u32);
        for &(id, index) in &self.0 {
            writer.write_u32(id);
            writer.write_u32(index);
        }
        writer.into_inner()
    }
}
