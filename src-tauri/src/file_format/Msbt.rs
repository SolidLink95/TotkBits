#![allow(non_snake_case,non_camel_case_types)]
use std::{fs, io::Read};
use msyt::converter::MsytFile;
use crate::{Settings::Pathlib, Zstd::TotkFileType};

//assuming msbt is never compressed
pub struct MsbtFile {
    pub path: Pathlib,
    pub endian: roead::Endian,
    pub file_type: TotkFileType,
    pub text: String,
    //pub data: Vec<u8>,
}

#[allow(dead_code)]
impl MsbtFile {
    pub fn from_filepath(path: &str) -> Option<Self> {
        let mut f_handle = fs::File::open(path).ok()?;
        let mut data: Vec<u8> = Vec::new();
        f_handle.read_to_end(&mut data).ok()?;
        let endian = MsbtFile::check_endianness(&data)?;



        
        let text = MsytFile::binary_to_text_safe(data).ok()?;
        Some(Self {
            path: Pathlib::new(path.to_string()),
            endian,
            file_type: TotkFileType::Msbt,
            text,
            //data,
        })
    }

    pub fn from_binary(data: Vec<u8>, path: Option<String>) -> Option<Self> {
        let endian = MsbtFile::check_endianness(&data)?;
        let text = MsytFile::binary_to_text_safe(data).ok()?;
        Some(Self {
            path: Pathlib::new(path.unwrap_or_default()) ,
            endian,
            file_type: TotkFileType::Msbt,
            text,
            //data,
        })
    }

    fn check_endianness(bytes: &Vec<u8>) -> Option<roead::Endian> {
        if bytes.len() >= 10 {
            // Ensure there are at least 10 bytes to check
            match bytes[8..10] {
                [0xFE, 0xFF] => Some(roead::Endian::Big),    // Big Endian
                [0xFF, 0xFE] => Some(roead::Endian::Little), // Little Endian
                _ => None,                                   // Not matching either pattern
            }
        } else {
            None // Not enough data to determine endianness
        }
    }
}

pub fn str_endian_to_roead(endian: &str) -> roead::Endian {
    match endian {
        "BE" => roead::Endian::Big,
        "LE" => roead::Endian::Little,
        _ => roead::Endian::Little,
    }
}