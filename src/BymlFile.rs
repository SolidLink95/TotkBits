use crate::Zstd::is_byml;
use crate::Zstd::totk_zstd;
use core::fmt::Error as Generic_Error;
use roead::byml::Byml;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::{fs, io};

pub struct byml_file<'a> {
    pub path: String,
    pub pio: roead::byml::Byml,
    pub zstd: Arc<totk_zstd<'a>>,
}

impl<'a> byml_file<'_> {
    pub fn new(path: String, zstd: Arc<totk_zstd<'a>>) -> Result<byml_file<'a>, roead::Error> {
        let mut f_handle = fs::File::open(path.clone())?;
        let mut buffer: Vec<u8> = Vec::new();
        f_handle.read_to_end(&mut buffer)?;
        if !is_byml(&buffer) {
            return Err(roead::Error::InvalidData("File is not byml"));
        }
        let pio = Byml::from_binary(buffer);
        match pio {
            Ok(ok_pio) => Ok(byml_file {
                path: path,
                pio: ok_pio,
                zstd: zstd.clone(),
            }),
            Err(err) => {
                return Err(roead::Error::InvalidData("Failed to parse byml"));
            }
        }
    }

}
