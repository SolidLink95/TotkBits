use roead;
use roead::sarc::{Sarc, SarcWriter};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::sleep;
//mod Zstd;

use crate::BinTextFile::FileData;
use crate::Settings::Pathlib;
use crate::TotkConfig::TotkConfig;
use crate::Zstd::{is_sarc, FileType, TotkZstd, SHA256};

pub struct PackComparer<'a> {
    pub opened: Option<PackFile<'a>>,
    pub vanila: Option<PackFile<'a>>,
    pub totk_config: Arc<TotkConfig>,
    pub zstd: Arc<TotkZstd<'a>>,
    pub added: HashMap<String,String>,
    pub modded: HashMap<String,String>,
}

impl<'a> PackComparer<'a> {
    pub fn from_pack(pack: PackFile<'a>, zstd: Arc<TotkZstd<'a>>) -> Self {
        let config = zstd.clone().totk_config.clone();
        let vanila = PackComparer::get_vanila_pack(pack.path.name.clone(), zstd.clone());
        //let vanila_path = config.get_pack_path_from_sarc(pack);
        Self {
            opened: Some(pack),
            vanila: vanila,
            totk_config: config,
            zstd: zstd.clone(),
            added: HashMap::default(),
            modded: HashMap::default(),
        }
        
    }
    pub fn compare(&mut self) {
        if let Some(vanila) = &self.vanila {
            if let Some(opened) = &self.opened {
                let mut added: HashMap<String,String> = HashMap::default();
                let mut modded: HashMap<String,String> = HashMap::default();
                for (file, Hash) in opened.hashes.iter() {
                    let van_hash = vanila.hashes.get(file);
                    match van_hash {
                        Some(h) => {
                            if h != Hash {
                                modded.insert(file.to_string(), Hash.to_string());
                            }
                        },
                        None => {
                            added.insert(file.to_string(), Hash.to_string());
                        }
                    }
    
                }
                self.added = added;
                self.modded = modded;
            }

        } else { //custom actor
            self.added.clear();
            self.modded.clear();
        }
    }

    pub fn get_vanila_pack(name: String, zstd: Arc<TotkZstd<'a>>) -> Option<PackFile> {
        let path = zstd.clone().totk_config.clone().get_pack_path(&name);
        if let Some(path) = &path {
            match PackFile::new(path.to_string_lossy().to_string(), zstd.clone()) {
                Ok(pack) => {return  Some(pack);},
                _ => {return None;}
            }
        }
        None


    }
}


pub struct PackFile<'a> {
    pub path: Pathlib,
    pub totk_config: Arc<TotkConfig>,
    zstd: Arc<TotkZstd<'a>>,
    pub data: FileData,
    pub endian: roead::Endian,
    pub writer: SarcWriter,
    pub hashes: HashMap<String, String>,
    pub sarc: Sarc<'a>,
}

impl<'a> PackFile<'_> {
    pub fn new(
        path: String,
        //totk_config: Arc<TotkConfig>,
        zstd: Arc<TotkZstd<'a>>,
        //decompressor: &'a ZstdDecompressor,
        //compressor: &'a ZstdCompressor
    ) -> io::Result<PackFile<'a>> {
        let file_data = PackFile::sarc_file_to_bytes(&PathBuf::from(path.clone()), &zstd.clone())?;
        println!("asdf");
        let sarc: Sarc = Sarc::new(file_data.data.clone()).expect("Failed");
        let writer: SarcWriter = SarcWriter::from_sarc(&sarc);
        Ok(PackFile {
            path: Pathlib::new(path),
            totk_config: zstd.totk_config.clone(),
            zstd: zstd.clone(),
            data: file_data,
            endian: sarc.endian(),
            writer: writer,
            hashes: PackFile::populate_hashes(&sarc),
            sarc: sarc,
        })
    }

    pub fn reload(&mut self) {
        let data: Vec<u8> = self.writer.to_binary();
        self.sarc = Sarc::new(data).expect("Failed");
    }
    
    pub fn populate_hashes(sarc: &Sarc) -> HashMap<String, String> {
        let mut hashes: HashMap<String, String> = HashMap::default();
        for file in sarc.files() {
            let file_name = file.name.unwrap_or("");
            if !file_name.is_empty() {
                let Hash = SHA256(file.data().to_vec());
                hashes.insert(file_name.to_string(), Hash);
            }
        }
        hashes
    }

    //Save the sarc file, compress if file ends with .zs, create directory if needed
    pub fn save_default(&mut self)-> io::Result<()> {
        let dest_file = self.path.full_path.clone();
        self.save(dest_file)
    }

    fn compress(&self) -> Vec<u8> {
        match self.data.file_type {
            FileType::Sarc => {
                return  self.zstd.compressor.compress_pack(&self.data.data).unwrap();
            },
            FileType::MalsSarc => {
                return  self.zstd.compressor.compress_zs(&self.data.data).unwrap();
            },
            _ => {
                return  self.zstd.compressor.compress_zs(&self.data.data).unwrap();
            }
        }
    }

    pub fn save(&mut self, dest_file: String) -> io::Result<()> {
        let file_path: &Path = Path::new(&dest_file);
        let directory: &Path = file_path.parent().expect("Cannot get parent of the file");
        fs::create_dir_all(directory)?;
        let mut data: Vec<u8> = self.writer.to_binary();
        if dest_file.to_lowercase().ends_with(".zs") {
            data = self.compress();
        }
        let mut file_handle: fs::File = fs::File::create(file_path)?;
        file_handle.write_all(&data)?;
        Ok(())
    }

    //Read sarc file's bytes, decompress if needed
    fn sarc_file_to_bytes(path: &PathBuf, zstd: &'a TotkZstd) -> Result<FileData, io::Error> {
        let mut f_handle: fs::File = fs::File::open(path)?;
        let mut buffer: Vec<u8> = Vec::new();
        //let mut returned_result: Vec<u8> = Vec::new();
        let mut file_data: FileData = FileData::new();
        file_data.file_type = FileType::Sarc;
        f_handle.read_to_end(&mut buffer)?;
        if is_sarc(&buffer) { //buffer.as_slice().starts_with(b"SARC") {
            file_data.data = buffer;
            return Ok(file_data);
        } 
        match zstd.decompressor.decompress_pack(&buffer) {
            Ok(res) => {
                if is_sarc(&res) {file_data.data = res;}
            },
            Err(_) => {
                match zstd.decompressor.decompress_zs(&buffer) {//try decompressing with other dicts
                    Ok(res) => {
                        if is_sarc(&res) {
                            file_data.data = res;
                            file_data.file_type = FileType::MalsSarc;
                        }
                    }
                    Err(err) => {
                        eprintln!("Error during zstd decompress {}", &path.to_str().unwrap_or("UNKNOWN"));
                        return Err(err);
                    }
                }
            }
            
        
        }
        if  file_data.data.starts_with(b"Yaz0") {
            match roead::yaz0::decompress(&file_data.data) {
                Ok(dec_data) => {
                    file_data.data = dec_data;
                    return Ok(file_data);
                },
                Err(_) => {}
            }
        }
        if is_sarc(&file_data.data) {
            return Ok(file_data);
        }
        return Err(io::Error::new(io::ErrorKind::Other, "Invalid data, not a sarc"));
    }
}

struct opened_file {
    file_path: String,
}
