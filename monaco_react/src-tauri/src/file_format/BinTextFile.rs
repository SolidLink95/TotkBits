use crate::file_format::TagProduct::TagProduct;
use crate::Settings::Pathlib;
use crate::Zstd::{is_byml, TotkFileType, TotkZstd};
use roead::byml::Byml;
use std::any::type_name;

use std::fs::OpenOptions;
use std::io::{BufWriter, Read, Write};
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;
use std::{fs, io, panic};

use super::Msbt::MsbtFile;
use super::Rstb::Restbl;

#[derive(Debug)]
pub struct FileData {
    pub file_type: TotkFileType,
    pub data: Vec<u8>,
}

impl FileData {
    pub fn new() -> Self {
        Self {
            file_type: TotkFileType::None,
            data: Vec::new(),
        }
    }
    pub fn from(data: Vec<u8>, file_type: TotkFileType) -> Self {
        Self {
            file_type: file_type,
            data: data,
        }
    }
}

pub struct BymlFile<'a> {
    pub endian: Option<roead::Endian>,
    pub file_data: FileData,
    pub path: Pathlib,
    pub pio: roead::byml::Byml,
    pub zstd: Arc<TotkZstd<'a>>,
}

impl<'a> BymlFile<'_> {
    pub fn new(path: String, zstd: Arc<TotkZstd<'a>>) -> Option<BymlFile<'a>> {
        fn inner_func(path: String, zstd: Arc<TotkZstd>) -> io::Result<BymlFile> {
            let data: FileData =
                BymlFile::byml_file_to_bytes(&PathBuf::from(path.clone()), zstd.clone())?;
            return BymlFile::from_binary(data, zstd, path);
        }
        if let Ok(byml) = inner_func(path, zstd.clone()) {
            return Some(byml);
        }
        None
    }

    pub fn save(&self, path: String) -> io::Result<()> {
        //let mut f_handle = OpenOptions::new().write(true).open(&path)?;
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            self.pio
                .to_binary(self.endian.unwrap_or(roead::Endian::Little))
        }));
        let mut data: Vec<u8> = Vec::new();
        match result {
            Ok(rawdata) => data = rawdata,
            Err(_) => return Err(io::Error::new(io::ErrorKind::InvalidData, "")),
        }
        if path.to_ascii_lowercase().ends_with(".zs") {
            match self.file_data.file_type {
                TotkFileType::Byml => {
                    data = self
                        .zstd
                        .compressor
                        .compress_zs(&data)
                        .expect("Failed to compress with zs");
                }
                TotkFileType::Bcett => {
                    data = self
                        .zstd
                        .compressor
                        .compress_bcett(&data)
                        .expect("Failed to compress with bcett");
                }
                _ => {
                    data = self
                        .zstd
                        .compressor
                        .compress_zs(&data)
                        .expect("Failed to compress with zs");
                }
            }
        }
        //f_handle.write_all(&data);
        bytes_to_file(data, &path);
        Ok(())
    }

    pub fn from_text(content: &str, zstd: Arc<TotkZstd<'a>>) -> io::Result<BymlFile<'a>> {
        let pio: Result<Byml, roead::Error> = Byml::from_text(&content);
        match pio {
            Ok(ok_pio) => Ok(BymlFile {
                endian: Some(roead::Endian::Little), //TODO: add Big endian support
                file_data: FileData::new(),
                path: Pathlib::default(),
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

    pub fn from_binary(
        data: FileData,
        zstd: Arc<TotkZstd<'a>>,
        full_path: String,
    ) -> io::Result<BymlFile<'a>> {
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

    pub fn get_endiannes_from_self(&self) -> roead::Endian {
        if self.file_data.data.starts_with(b"BY") {
            return roead::Endian::Big;
        } else if self.file_data.data.starts_with(b"YB") {
            return roead::Endian::Little;
        }
        return roead::Endian::Little;
    }

    pub fn get_endiannes(data: &Vec<u8>) -> Option<roead::Endian> {
        if data.starts_with(b"BY") {
            return Some(roead::Endian::Big);
        }
        if data.starts_with(b"YB") {
            return Some(roead::Endian::Little);
        }
        None
    }

    pub fn byml_data_to_bytes(rawdata: &Vec<u8>, zstd: Arc<TotkZstd>) -> Result<FileData, io::Error> {
        let mut buffer = rawdata.clone();
        let mut data = FileData::new();
        if buffer.starts_with(b"Yaz0") {
            if let Ok(dec_data) = roead::yaz0::decompress(&buffer) {
                buffer = dec_data;
            }
        }
        if is_byml(&buffer) {
            //regular byml file,
            data.data = buffer;
            data.file_type = TotkFileType::Byml;
            return Ok(data);
        } else {
            match zstd.decompressor.decompress_zs(&buffer) {
                //regular byml file compressed with zs
                Ok(res) => {
                    if is_byml(&res) {
                        data.data = res;
                        data.file_type = TotkFileType::Byml;
                    }
                }
                Err(_err) => {}
            }
        }
        if !is_byml(&data.data) {
            match zstd.decompressor.decompress_bcett(&buffer) {
                //bcett map file
                Ok(res) => {
                    data.data = res;
                    data.file_type = TotkFileType::Byml;
                    if is_byml(&data.data) {
                        data.file_type = TotkFileType::Bcett;
                    }
                }
                _ => {}
            }
        }

        if !is_byml(&data.data) {
            match zstd.try_decompress(&buffer) {
                //try decompressing with other dicts
                Ok(res) => {
                    data.data = res;
                    data.file_type = TotkFileType::Other;
                }
                Err(_err) => {}
            }
        }
        if is_byml(&data.data) {
            data.file_type = TotkFileType::Byml;
            return Ok(data);
        }
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Invalid data, not a byml",
        ));
    }

    pub fn byml_file_to_bytes(path: &PathBuf, zstd: Arc<TotkZstd>) -> Result<FileData, io::Error> {
        let mut f_handle: fs::File = fs::File::open(path)?;
        let mut buffer: Vec<u8> = Vec::new();
        f_handle.read_to_end(&mut buffer)?;
        Self::byml_data_to_bytes(&buffer, zstd.clone())
    }
}

pub fn bytes_to_file(data: Vec<u8>, path: &str) -> io::Result<()> {
    let f = fs::File::create(&path); //TODO check if the ::create is sufficient
    match f {
        Ok(mut f_handle) => {
            //file does not exist
            f_handle.write_all(&data);
        }
        Err(_) => {
            //file exist, overwrite
            let mut f_handle = OpenOptions::new().write(true).open(&path)?;
            f_handle.write_all(&data);
        }
    }
    Ok(())
}

//#[derive(Serialise, Deserialise)]
pub struct OpenedFile<'a> {
    pub file_type: TotkFileType,
    pub path: Pathlib,
    pub byml: Option<BymlFile<'a>>,
    pub endian: Option<roead::Endian>,
    pub msyt: Option<MsbtFile>,
    pub aamp: Option<()>,
    pub tag: Option<TagProduct<'a>>,
    pub restbl: Option<Restbl<'a>>,
}

impl Default for OpenedFile<'_> {
    fn default() -> Self {
        Self {
            file_type: TotkFileType::None,
            path: Pathlib::default(),
            byml: None,
            endian: None,
            msyt: None,
            aamp: None,
            tag: None,
            restbl: None,
        }
    }
}

impl<'a> OpenedFile<'_> {
    pub fn new(
        path: String,
        file_type: TotkFileType,
        endian: Option<roead::Endian>,
        msyt: Option<MsbtFile>,
    ) -> Self {
        Self {
            file_type: file_type,
            path: Pathlib::new(path),
            byml: None,
            endian: endian,
            msyt: msyt,
            aamp: None,
            tag: None,
            restbl: None,
        }
    }

    pub fn from_path(path: String, file_type: TotkFileType) -> Self {
        Self {
            file_type: file_type,
            path: Pathlib::new(path),
            byml: None,
            endian: None,
            msyt: None,
            aamp: None,
            tag: None,
            restbl: None,
        }
    }

    pub fn reset(&mut self) {
        self.file_type = TotkFileType::None;
        self.path = Pathlib::default();
        self.byml = None;
        self.endian = None;
        self.msyt = None;
        self.aamp = None;
        self.tag = None;
    }

    pub fn get_endian_label(&self) -> String {
        match self.endian {
            Some(endian) => match endian {
                roead::Endian::Big => {
                    return "BE".to_string();
                }
                roead::Endian::Little => {
                    return "LE".to_string();
                }
            },
            None => {
                return "".to_string();
            }
        }
    }

    pub fn open(&mut self, file_path: &str, zstd: Arc<TotkZstd>) -> String {
        let mut res = String::new();
        let path = Pathlib::new(file_path.to_string());
        if self.open_tag(&path, zstd.clone()) {
            if let Some(tag) = &self.tag {
                return tag.text.to_string();
            }
        }
        res
    }

    pub fn open_tag(&mut self, path: &Pathlib, zstd: Arc<TotkZstd>) -> bool {
        if path.name.to_lowercase().starts_with("tag.product") {
            let tag = TagProduct::new(path.full_path.clone(), zstd.clone());
            match tag {
                Some(mut tag) => {
                    match tag.parse() {
                        Ok(_) => {
                            println!("Tag parsed!");
                        }
                        Err(err) => {
                            eprintln!("Error parsing tag! {:?}", err);
                            return false;
                        }
                    }
                    self.reset();
                    //self.tag = Some(tag);
                    self.file_type = TotkFileType::TagProduct;
                    self.path = path.clone();
                    self.endian = Some(roead::Endian::Little);
                    return true;
                }
                None => {
                    return false;
                }
            }
        }
        false
    }
}

fn print_type_of<T>(_: &T) {
    println!("{}", type_name::<T>());
}

pub fn write_string_to_file(path: &str, content: &str) -> io::Result<()> {
    let file = fs::File::create(path)?;
    let mut writer = BufWriter::new(file);

    writer.write_all(content.as_bytes())?;

    // The buffer is automatically flushed when writer goes out of scope,
    // but you can manually flush it if needed.
    writer.flush()?;

    Ok(())
}

pub fn read_string_from_file(path: &str) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    Ok(contents)
}
