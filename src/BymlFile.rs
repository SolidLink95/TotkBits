use crate::Pack::Endian;
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
    pub endian: Option<Endian>,
    pub zstd: Arc<totk_zstd<'a>>,
}

impl<'a> byml_file<'_> {
    pub fn new(path: String, zstd: Arc<totk_zstd<'a>>) -> io::Result<byml_file<'a>> {
        let buffer: Vec<u8> = byml_file::byml_data_to_bytes(&PathBuf::from(path.clone()), &zstd.clone())?;
        return byml_file::from_binary(&buffer, zstd, path);
    }

    pub fn from_binary(data: &Vec<u8>, zstd: Arc<totk_zstd<'a>>, full_path: String) -> io::Result<byml_file<'a>> {
        let pio = Byml::from_binary(&data);
        match pio {
            Ok(ok_pio) => Ok(byml_file {
                path: full_path,
                pio: ok_pio,
                endian: byml_file::get_endiannes(data),
                zstd: zstd.clone(),
            }),
            Err(err) => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Error for byml file",
                ));
            }
        }

    }

    
    pub fn get_endiannes(data: &Vec<u8>) -> Option<Endian> {
        if data.starts_with(b"BY") {return Some(Endian::Big);}
        if data.starts_with(b"YB") {return Some(Endian::Little);}
        None
    }

    fn byml_data_to_bytes(path: &PathBuf, zstd: &'a totk_zstd) -> Result<Vec<u8>, io::Error> {
        let mut f_handle: fs::File = fs::File::open(path)?;
        let mut buffer: Vec<u8> = Vec::new();
        f_handle.read_to_end(&mut buffer)?;
        let mut returned_result: Vec<u8> = Vec::new();
        if is_byml(&buffer) {
            return Ok(buffer);
        }
        else {
            match zstd
                .decompressor
                .decompress_zs(&buffer) {
                Ok(res) => {
                    if is_byml(&res) {
                        returned_result = res;
                    }
                },
                Err(err) => {}
            }
        }
        if !is_byml(&returned_result) {
            match zstd.try_decompress(&buffer) {//try decompressing with other dicts
                Ok(res) => {
                    returned_result = res;
                }
                Err(err) => {
                    println!("Error during zstd decompress, {}", line!());
                    return Err(err);
                }
            }
        }
        if is_byml(&returned_result) {
            return Ok(returned_result);
        }
        if returned_result.starts_with(b"Yaz0") {
            match roead::yaz0::decompress(&returned_result) {
                Ok(dec_data) => {return Ok(dec_data);},
                Err(_) => {}
            }
        }
        return Err(io::Error::new(io::ErrorKind::Other, "Invalid data, not a byml"));

    }
}
