use crate::parser::binary::BinaryReader;
use std::io::{self, ErrorKind};

pub fn read_string(data: &[u8], offset: u64) -> io::Result<String> {
    let offset = usize::try_from(offset)
        .map_err(|_| io::Error::new(ErrorKind::InvalidData, "string offset overflow"))?;
    let mut reader = BinaryReader::new(data);
    reader.seek(offset)?;
    let size = reader.read_u16()? as usize;
    String::from_utf8(reader.read_bytes(size)?.to_vec())
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

pub fn read_string_ptr(reader: &mut BinaryReader<'_>, data: &[u8]) -> io::Result<String> {
    read_string(data, reader.read_u64()?)
}

pub fn read_keys(data: &[u8], offset: u64) -> io::Result<Vec<String>> {
    let mut reader = BinaryReader::new(data);
    reader.seek(offset as usize)?;
    if reader.read_bytes(4)? != b"DIC " {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "invalid EVFL dictionary",
        ));
    }
    let count = reader.read_u32()? as usize;
    reader.skip(16)?;
    let mut keys = Vec::new();
    for _ in 0..count {
        reader.skip(8)?;
        keys.push(read_string_ptr(&mut reader, data)?);
    }
    Ok(keys)
}

pub fn read_offset_array(data: &[u8], offset: u64, count: usize) -> io::Result<Vec<u64>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut reader = BinaryReader::new(data);
    reader.seek(offset as usize)?;
    (0..count).map(|_| reader.read_u64()).collect()
}
