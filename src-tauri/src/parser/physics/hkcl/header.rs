use crate::parser::binary::{BinaryReader, Endian};
use std::io::{self, ErrorKind};

pub(crate) const PACKFILE_MAGIC: [u8; 8] = [0x57, 0xe0, 0xe0, 0x57, 0x10, 0xc0, 0xc0, 0x10];
pub const HEADER_SIZE: usize = 0x40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HkclLayoutRules {
    pub pointer_size: u8,
    pub endian: Endian,
    pub reuse_padding_optimization: bool,
    pub empty_base_class_optimization: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HkclHeader {
    pub user_tag: u32,
    pub file_version: u32,
    pub layout: HkclLayoutRules,
    pub section_count: usize,
    pub contents_section_index: Option<usize>,
    pub contents_section_offset: u32,
    pub contents_class_name_section_index: Option<usize>,
    pub contents_class_name_section_offset: u32,
    pub contents_version: String,
    pub flags: u32,
    pub max_predicate: u16,
    pub predicate_array_size: u16,
}

impl HkclHeader {
    pub fn read(data: &[u8]) -> io::Result<Self> {
        if data.len() < HEADER_SIZE || data[..8] != PACKFILE_MAGIC {
            return Err(invalid("missing HKCL Havok packfile header"));
        }
        let pointer_size = data[0x10];
        if !matches!(pointer_size, 4 | 8) {
            return Err(invalid("HKCL pointer size must be 4 or 8"));
        }
        let endian = match data[0x11] {
            0 => Endian::Big,
            1 => Endian::Little,
            _ => return Err(invalid("invalid HKCL endian layout rule")),
        };
        let layout = HkclLayoutRules {
            pointer_size,
            endian,
            reuse_padding_optimization: data[0x12] != 0,
            empty_base_class_optimization: data[0x13] != 0,
        };
        let mut reader = BinaryReader::with_endian(data, endian);
        reader.seek(8)?;
        let user_tag = reader.read_u32()?;
        let file_version = reader.read_u32()?;
        reader.seek(0x14)?;
        let section_count = non_negative(reader.read_i32()?, "section count")?;
        if section_count > (data.len() - HEADER_SIZE) / super::section::SECTION_HEADER_SIZE {
            return Err(invalid("HKCL section table exceeds input"));
        }
        let contents_section_index = section_index(reader.read_i32()?, section_count, "contents")?;
        let contents_section_offset = reader.read_u32()?;
        let contents_class_name_section_index =
            section_index(reader.read_i32()?, section_count, "contents class name")?;
        let contents_class_name_section_offset = reader.read_u32()?;
        let version_bytes = reader.read_bytes(16)?;
        let version_end = version_bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(16);
        let contents_version = String::from_utf8(version_bytes[..version_end].to_vec())
            .map_err(|_| invalid("HKCL contents version is not UTF-8"))?;
        let flags = reader.read_u32()?;
        let max_predicate = reader.read_u16()?;
        let predicate_array_size = reader.read_u16()?;
        Ok(Self {
            user_tag,
            file_version,
            layout,
            section_count,
            contents_section_index,
            contents_section_offset,
            contents_class_name_section_index,
            contents_class_name_section_offset,
            contents_version,
            flags,
            max_predicate,
            predicate_array_size,
        })
    }
}

fn non_negative(value: i32, name: &str) -> io::Result<usize> {
    usize::try_from(value).map_err(|_| invalid(&format!("HKCL {name} is negative")))
}

fn section_index(value: i32, count: usize, name: &str) -> io::Result<Option<usize>> {
    if value == -1 {
        return Ok(None);
    }
    let value = non_negative(value, &format!("{name} section index"))?;
    if value >= count {
        return Err(invalid(&format!(
            "HKCL {name} section index is out of range"
        )));
    }
    Ok(Some(value))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}
