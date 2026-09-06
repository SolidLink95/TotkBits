use crate::parser::binary::{BinaryReader, Endian};
use std::{
    io::{self, ErrorKind},
    ops::Range,
};

pub const SECTION_HEADER_SIZE: usize = 0x40;
const SECTION_TAG_SIZE: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HkclSection {
    pub tag: String,
    pub absolute_data_start: usize,
    pub local_fixups: Range<usize>,
    pub global_fixups: Range<usize>,
    pub virtual_fixups: Range<usize>,
    pub exports: Range<usize>,
    pub imports: Range<usize>,
    pub end: usize,
}

impl HkclSection {
    pub fn read(data: &[u8], header_offset: usize, endian: Endian) -> io::Result<Self> {
        if header_offset
            .checked_add(SECTION_HEADER_SIZE)
            .is_none_or(|end| end > data.len())
        {
            return Err(invalid("truncated HKCL section header"));
        }
        let mut reader = BinaryReader::with_endian(data, endian);
        reader.seek(header_offset)?;
        let tag_bytes = reader.read_bytes(SECTION_TAG_SIZE)?;
        let tag_end = tag_bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(SECTION_TAG_SIZE);
        let tag = String::from_utf8(tag_bytes[..tag_end].to_vec())
            .map_err(|_| invalid("HKCL section tag is not UTF-8"))?;
        if tag.is_empty() {
            return Err(invalid("HKCL section tag is empty"));
        }
        // TotK HKCL stores an additional 32-bit header field before the section
        // payload offset.
        reader.skip(4)?;
        let absolute_data_start = reader.read_u32()? as usize;
        let offsets = [
            reader.read_u32()? as usize,
            reader.read_u32()? as usize,
            reader.read_u32()? as usize,
            reader.read_u32()? as usize,
            reader.read_u32()? as usize,
            reader.read_u32()? as usize,
        ];
        if offsets.windows(2).any(|pair| pair[0] > pair[1]) {
            return Err(invalid("HKCL section ranges are not monotonic"));
        }
        let absolute_end = absolute_data_start
            .checked_add(offsets[5])
            .ok_or_else(|| invalid("HKCL section end overflows"))?;
        if absolute_data_start > data.len() || absolute_end > data.len() {
            return Err(invalid("HKCL section data exceeds input"));
        }
        let absolute = |offset: usize| absolute_data_start + offset;
        Ok(Self {
            tag,
            absolute_data_start,
            local_fixups: absolute(offsets[0])..absolute(offsets[1]),
            global_fixups: absolute(offsets[1])..absolute(offsets[2]),
            virtual_fixups: absolute(offsets[2])..absolute(offsets[3]),
            exports: absolute(offsets[3])..absolute(offsets[4]),
            imports: absolute(offsets[4])..absolute(offsets[5]),
            end: absolute_end,
        })
    }

    pub fn data(&self) -> Range<usize> {
        self.absolute_data_start..self.local_fixups.start
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}
