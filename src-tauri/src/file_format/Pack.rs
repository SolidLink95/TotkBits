use roead;
use roead::sarc::{Sarc, SarcWriter};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{PathBuf};
use std::sync::Arc;

//mod Zstd;

use crate::file_format::BinTextFile::FileData;
use crate::Settings::{makedirs, Pathlib};
use crate::TotkConfig::TotkConfig;
use crate::Zstd::{is_sarc, sha256, TotkFileType, TotkZstd};

use super::SarcEntriesData::get_sarc_entries_data;

pub struct PackComparer<'a> {
    pub opened: Option<PackFile<'a>>,
    pub vanila: Option<PackFile<'a>>,
    pub totk_config: Arc<TotkConfig>,
    pub zstd: Arc<TotkZstd<'a>>,
    pub added: HashMap<String, String>,
    pub modded: HashMap<String, String>,
    pub global_sarc_data: HashMap<String, String>,
}

impl<'a> PackComparer<'a> {
    pub fn from_pack(pack: PackFile<'a>, zstd: Arc<TotkZstd<'a>>) -> Option<Self> {
        let config = zstd.clone().totk_config.clone();
        // let vanila = PackComparer::get_vanila_sarc(&pack.path, zstd.clone());
        //Set to None - rely only on global_sarc_data (except for mals files)
        let vanila = PackComparer::get_vanila_mals(&pack.path, zstd.clone());
        //let vanila_path = config.get_pack_path_from_sarc(pack);
        let mut pack = Self {
            opened: Some(pack),
            vanila: vanila,
            totk_config: config,
            zstd: zstd.clone(),
            added: HashMap::default(),
            modded: HashMap::default(),
            global_sarc_data: HashMap::default(),
        };
        pack.compare_and_reload();
        Some(pack)
    }

    pub fn get_sarc_paths(&self) -> SarcPaths {
        let mut paths = SarcPaths::default();
        if let Some(opened) = &self.opened {
            for file in opened.sarc.files() {
                if let Some(name) = file.name {
                    paths.paths.push(name.to_string());
                }
            }
            for (path, _) in self.added.iter() {
                paths.added_paths.push(path.to_string());
            }
            for (path, _) in self.modded.iter() {
                paths.modded_paths.push(path.to_string());
            }
        }
        paths
    }

    pub fn compare_and_reload(&mut self) {
        if let Some(opened) = &mut self.opened {
            opened.reload();
            opened.self_populate_hashes();
        }

        self.compare();
    }

    pub fn compare(&mut self) {
        if let Some(opened) = &self.opened {
            if let Some(vanila) = &self.vanila { //unreachable
                let mut added: HashMap<String, String> = HashMap::default();
                let mut modded: HashMap<String, String> = HashMap::default();
                for (file, hash) in opened.hashes.iter() {
                    let van_hash = vanila.hashes.get(file);
                    match van_hash {
                        Some(h) => {
                            if h != hash {
                                modded.insert(file.to_string(), hash.to_string());
                            }
                        }
                        None => {
                            added.insert(file.to_string(), hash.to_string());
                        }
                    }
                }
                self.added = added;
                self.modded = modded;
                // println!("Added {:?}\nModded {:?}", self.added.keys(), self.modded.keys());
            } else {
                //custom actor
                if self.global_sarc_data.is_empty() {
                    self.global_sarc_data = get_sarc_entries_data();
                }
                let mut added: HashMap<String, String> = HashMap::default();
                let mut modded: HashMap<String, String> = HashMap::default();
                for (file, hash) in opened.hashes.iter() {
                    let van_hash = self.global_sarc_data.get(file);
                    match van_hash {
                        Some(h) => {
                            if h != hash {
                                modded.insert(file.to_string(), hash.to_string());
                            }
                        }
                        None => {
                            added.insert(file.to_string(), hash.to_string());
                        }
                    }
                }
                self.added = added;
                self.modded = modded;
                // println!("Added {:?}\nModded {:?}", self.added, self.modded);
            }
        }
    }

    pub fn get_vanila_mals(path: &Pathlib, zstd: Arc<TotkZstd<'a>>) -> Option<PackFile<'a>> {
        println!("Getting the mals: {}", &path.stem);
        let versions: Vec<usize> = (100..130).collect();//130, in case new updates are issued in the future for TOTK
        let romfs = &zstd.clone().totk_config.romfs;
        for version in versions {
            let mut prob_mals_path = PathBuf::from(romfs);
            prob_mals_path.push(format!("Mals/{}.Product.{}.sarc.zs", &path.stem, version));
            if prob_mals_path.exists() {
                if let Ok(pack) = PackFile::new(prob_mals_path.to_string_lossy().to_string(), zstd.clone()) {
                    println!("Got the mals! Version: {}", version);
                    return Some(pack);
                }   
            }
        }
        
        
        // let path = zstd.clone().totk_config.clone().get_mals_path(&path.name);
        // if let Some(path) = &path {
        //     match PackFile::new(path.to_string_lossy().to_string(), zstd.clone()) {
        //         Ok(pack) => {
        //             println!("Got the mals!");
        //             return Some(pack);
        //         }
        //         _ => {
        //             return None;
        //         }
        //     }
        // }
        None
    }
    #[allow(dead_code)]
    pub fn get_vanila_sarc(path: &Pathlib, zstd: Arc<TotkZstd<'a>>) -> Option<PackFile<'a>> {
        let pack = PackComparer::get_vanila_pack(path, zstd.clone());
        if pack.is_some() {
            return pack;
        }
        let mals = PackComparer::get_vanila_mals(path, zstd.clone());
        if mals.is_some() {
            return mals;
        }
        None
    }
    #[allow(dead_code)]
    pub fn get_vanila_pack(path: &Pathlib, zstd: Arc<TotkZstd<'a>>) -> Option<PackFile<'a>> {
        println!("Getting the pack: {}", &path.name);
        let path = zstd.clone().totk_config.clone().get_pack_path(&path.stem);
        if let Some(path) = &path {
            match PackFile::new(path.to_string_lossy().to_string(), zstd.clone()) {
                Ok(pack) => {
                    return Some(pack);
                }
                _ => {
                    return None;
                }
            }
        }
        None
    }
}

pub struct PackFile<'a> {
    pub path: Pathlib,
    pub totk_config: Arc<TotkConfig>,
    zstd: Arc<TotkZstd<'a>>,
    pub data: Vec<u8>,
    pub file_type: TotkFileType,
    pub endian: roead::Endian,
    pub writer: SarcWriter,
    pub hashes: HashMap<String, String>,
    pub sarc: Sarc<'a>,
    pub is_yaz0: bool,
}

impl<'a> PackFile<'_> {
    pub fn default(zstd: Arc<TotkZstd<'a>>) -> io::Result<PackFile<'a>> {
        let arr: Vec<u8> = vec![
            0x53, 0x41, 0x52, 0x43, 0x14, 0x00, 0xFF, 0xFE, 
            0x28, 0x00, 0x00, 0x00, 0x28, 0x00, 0x00, 0x00, 
            0x00, 0x01, 0x00, 0x00, 0x53, 0x46, 0x41, 0x54, 
            0x0C, 0x00, 0x00, 0x00, 0x65, 0x00, 0x00, 0x00, 
            0x53, 0x46, 0x4E, 0x54, 0x08, 0x00, 0x00, 0x00,
        ];
        let sarc = Sarc::new(arr).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let writer = SarcWriter::from_sarc(&sarc);
        Ok(PackFile {
            path: Pathlib::default(),
            totk_config: zstd.totk_config.clone(),
            zstd: zstd.clone(),
            data: Default::default(),
            file_type: TotkFileType::Sarc,
            endian: roead::Endian::Little,
            writer: writer,
            hashes: HashMap::default(),
            sarc: sarc,
            is_yaz0: false,
        })
    }

    pub fn new(
        path: String,
        //totk_config: Arc<TotkConfig>,
        zstd: Arc<TotkZstd<'a>>,
        //decompressor: &'a ZstdDecompressor,
        //compressor: &'a ZstdCompressor
    ) -> io::Result<PackFile<'a>> {
        let mut pack = Self::default(zstd.clone())?;
        pack.sarc_file_to_bytes(path.as_str())?;
        // pack.sarc = Sarc::new(&pack.data).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        pack.writer = SarcWriter::from_sarc(&pack.sarc);
        pack.path = Pathlib::new(path.clone());
        Ok(pack)



        // let (is_yaz0, file_data) = PackFile::sarc_file_to_bytes(&PathBuf::from(path.clone()), &zstd.clone())?;
        // println!("asdf");
        // let sarc: Sarc = Sarc::new(file_data.data.clone()).expect("Failed");
        // let writer: SarcWriter = SarcWriter::from_sarc(&sarc);
        // Ok(PackFile {
        //     path: Pathlib::new(path),
        //     totk_config: zstd.totk_config.clone(),
        //     zstd: zstd.clone(),
        //     data: file_data,
        //     endian: sarc.endian(),
        //     writer: writer,
        //     hashes: PackFile::populate_hashes(&sarc),
        //     sarc: sarc,
        //     is_yaz0: is_yaz0
        // })
    }

    pub fn rename(&mut self, old_name: &str, new_name: &str) -> io::Result<()> {
        let some_data = self.writer.get_file(old_name);
        match some_data {
            Some(data) => {
                let d = data.clone();
                self.writer.add_file(new_name, d.to_vec());
                self.writer.remove_file(old_name);
                self.reload();
                self.self_populate_hashes();
            }
            None => {
                let e = format!("File {} absent in sarc {}", &old_name, &self.path.full_path);
                return Err(io::Error::new(io::ErrorKind::InvalidInput, e));
            }
        }
        Ok(())
    }

    pub fn reload(&mut self) {
        let data: Vec<u8> = self.writer.to_binary();
        self.sarc = Sarc::new(data).expect("Failed");
    }

    pub fn self_populate_hashes(&mut self) {
        self.hashes = PackFile::populate_hashes(&self.sarc);
    }

    pub fn populate_hashes(sarc: &Sarc) -> HashMap<String, String> {
        let mut hashes: HashMap<String, String> = HashMap::default();
        for file in sarc.files() {
            let file_name = file.name.unwrap_or("");
            if !file_name.is_empty() {
                let hash = sha256(file.data().to_vec());
                hashes.insert(file_name.to_string(), hash);
            }
        }
        hashes
    }

    //Save the sarc file, compress if file ends with .zs, create directory if needed
    pub fn save_default(&mut self) -> io::Result<()> {
        let dest_file = self.path.full_path.clone();
        self.save(dest_file)
    }

    fn compress(&self, data: &Vec<u8>) -> io::Result<Vec<u8>> {
        match self.file_type {
            TotkFileType::Sarc => {
                println!("Compressing SARC");
                return self.zstd.compressor.compress_pack(data);
            }
            TotkFileType::MalsSarc => {
                println!("Compressing MALS SARC");
                return self.zstd.compressor.compress_zs(data);
            }
            _ => {
                return Ok(data.to_vec());
            }
        }
        
    }

    pub fn save(&mut self, dest_file: String) -> io::Result<()> {
        makedirs(&PathBuf::from(&dest_file))?;
        let mut data: Vec<u8> = self.writer.to_binary();
        if dest_file.to_lowercase().ends_with(".zs") {
            data = self.compress(&data)?;
        } else if self.is_yaz0 {
            data = roead::yaz0::compress(&data);
        }
        let mut file_handle: fs::File = fs::File::create(dest_file)?;
        file_handle.write_all(&data)?;
        Ok(())
    }
    pub fn sarc_file_to_bytes(&mut self, path: &str) ->io::Result<()> {
        let mut f_handle: fs::File = fs::File::open(path)?;
        let mut buffer: Vec<u8> = Vec::new();
        f_handle.read_to_end(&mut buffer)?;
        if buffer.starts_with(b"Yaz0") {
            if let Ok(dec_data) = roead::yaz0::decompress(&buffer) {
                buffer = dec_data;
                self.is_yaz0 = true;
            }
            if is_sarc(&buffer) {
                // self.data = buffer;
                self.sarc = Sarc::new(buffer).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                return Ok(());
            }
        }
        if path.to_lowercase().ends_with(".zs") {
            if let Ok(dec_data) = self.zstd.decompressor.decompress_pack(&buffer) {
                if is_sarc(&dec_data) {
                    // self.data = dec_data;
                    self.sarc = Sarc::new(dec_data).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                    self.file_type = TotkFileType::Sarc;
                    return Ok(());
                }
            }
            if let Ok(dec_data) = self.zstd.decompressor.decompress_zs(&buffer) {
                if is_sarc(&dec_data) {
                    // self.data = dec_data;
                    self.sarc = Sarc::new(dec_data).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                    self.file_type = TotkFileType::MalsSarc;
                    return Ok(());
                }
            }
        }
        if is_sarc(&buffer) {
            // self.data = buffer;
            self.sarc = Sarc::new(buffer).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            return Ok(());
        }
        
        Err(io::Error::new(
            io::ErrorKind::Other,
            "Invalid data, not a sarc",
        ))
    }
    //Read sarc file's bytes, decompress if needed
    // fn sarc_file_to_bytes1(path: &PathBuf, zstd: &'a TotkZstd) -> Result<(bool, FileData), io::Error> {
    //     let mut is_yaz0 = false;
    //     let mut f_handle: fs::File = fs::File::open(path)?;
    //     let mut buffer: Vec<u8> = Vec::new();
    //     //let mut returned_result: Vec<u8> = Vec::new();
    //     let mut file_data: FileData = FileData::new();
    //     file_data.file_type = TotkFileType::Sarc;
    //     f_handle.read_to_end(&mut buffer)?;

    //     if buffer.starts_with(b"Yaz0") {
    //         if let Ok(dec_data) = roead::yaz0::decompress(&buffer) {
    //             buffer = dec_data;
    //             is_yaz0 = true;
    //         }
    //     }
    //     if is_sarc(&buffer) {
    //         //buffer.as_slice().starts_with(b"SARC") {
    //         file_data.data = buffer;
    //         return Ok((is_yaz0, file_data));
    //     }
    //     match zstd.decompressor.decompress_pack(&buffer) {
    //         Ok(res) => {
    //             if is_sarc(&res) {
    //                 file_data.data = res;
    //             }
    //         }
    //         Err(_) => {
    //             match zstd.decompressor.decompress_zs(&buffer) {
    //                 //try decompressing with other dicts
    //                 Ok(res) => {
    //                     if is_sarc(&res) {
    //                         file_data.data = res;
    //                         file_data.file_type = TotkFileType::MalsSarc;
    //                     }
    //                 }
    //                 Err(err) => {
    //                     eprintln!(
    //                         "Error during zstd decompress {}",
    //                         &path.to_str().unwrap_or("UNKNOWN")
    //                     );
    //                     return Err(err);
    //                 }
    //             }
    //         }
    //     }
    //     if is_sarc(&file_data.data) {
    //         return Ok((is_yaz0, file_data));
    //     }
    //     return Err(io::Error::new(
    //         io::ErrorKind::Other,
    //         "Invalid data, not a sarc",
    //     ));
    // }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SarcPaths {
    pub paths: Vec<String>,
    pub added_paths: Vec<String>,
    pub modded_paths: Vec<String>,
}
impl Default for SarcPaths {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            added_paths: Vec::new(),
            modded_paths: Vec::new(),
        }
    }
}
