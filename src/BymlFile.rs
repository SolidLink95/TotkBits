use crate::Settings::Pathlib;
use crate::Zstd::{is_byml, totk_zstd, FileType};
use roead::byml::Byml;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::{fs, io};

pub struct FileData {
    pub file_type: FileType,
    pub data: Vec<u8>
}

impl FileData {
    pub fn new() -> Self {
        Self {
            file_type: FileType::None,
            data: Vec::new()
        }
    }

} 


pub struct BymlFile<'a> {
    pub endian: Option<roead::Endian>,
    pub file_data: FileData,
    pub path: Pathlib,
    pub pio: roead::byml::Byml,
    pub zstd: Arc<totk_zstd<'a>>,
}

impl<'a> BymlFile<'_> {
    pub fn new(path: String, zstd: Arc<totk_zstd<'a>>) -> io::Result<BymlFile<'a>> {
        let data: FileData = BymlFile::byml_data_to_bytes(&PathBuf::from(path.clone()), &zstd.clone())?;
        return BymlFile::from_binary(data, zstd, path);
    }

    pub fn from_binary(data: FileData, zstd: Arc<totk_zstd<'a>>, full_path: String) -> io::Result<BymlFile<'a>> {
        let pio = Byml::from_binary(&data.data);
        match pio {
            Ok(ok_pio) => Ok(BymlFile {
                endian: BymlFile::get_endiannes(&data.data.clone()),
                file_data: data,
                path: Pathlib::new(full_path),
                pio: ok_pio,
                zstd: zstd.clone(),
            }),
            Err(_err) => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Error for byml file",
                ));
            }
        }

    }

    
    pub fn get_endiannes(data: &Vec<u8>) -> Option<roead::Endian> {
        if data.starts_with(b"BY") {return Some(roead::Endian::Big);}
        if data.starts_with(b"YB") {return Some(roead::Endian::Little);}
        None
    }

    fn byml_data_to_bytes(path: &PathBuf, zstd: &'a totk_zstd) -> Result<FileData, io::Error> {
        let mut f_handle: fs::File = fs::File::open(path)?;
        let mut buffer: Vec<u8> = Vec::new();
        f_handle.read_to_end(&mut buffer)?;
        //let mut returned_result: Vec<u8> = Vec::new();
        let mut data  = FileData::new();
        if is_byml(&buffer) { //regular byml file, 
            data.data = buffer;
            data.file_type = FileType::Byml;
            return Ok(data);
        }
        else {
            match zstd
                .decompressor
                .decompress_zs(&buffer) { //regular byml file compressed with zs
                Ok(res) => {
                    if is_byml(&res) {
                        data.data = res;
                        data.file_type = FileType::Byml;
                    }
                },
                Err(_err) => {}
            }
        }
        if !is_byml(&data.data) {
            match zstd.decompressor.decompress_bcett(&buffer) { //bcett map file
                Ok(res) => {
                    data.data = res;
                    data.file_type = FileType::Byml;
                },
                _ => {}
            }
        
        }
        

        if !is_byml(&data.data) {
            match zstd.try_decompress(&buffer) {//try decompressing with other dicts
                Ok(res) => {
                    data.data = res;
                    data.file_type = FileType::Other;
                }
                Err(err) => {
                    println!("Error during zstd decompress, {}", line!());
                    return Err(err);
                }
            }
        }
        if data.data.starts_with(b"Yaz0") {
            match roead::yaz0::decompress(&data.data) {
                Ok(dec_data) => {
                    data.data = dec_data;
                },
                Err(_) => {}
            }
        }
        if is_byml(&data.data) {
            return Ok(data);
        }
        return Err(io::Error::new(io::ErrorKind::Other, "Invalid data, not a byml"));

    }
}
