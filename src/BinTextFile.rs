use crate::Pack::PackFile;
use crate::Settings::Pathlib;
use crate::Zstd::{is_byml, is_msyt, FileType, TotkZstd};
use bitvec::order::LocalBits;
use byteordered::Endianness;
use egui::epaint::tessellator::path;
use indexmap::IndexMap;
use msbt::builder::MsbtBuilder;
use msbt::section::Atr1;
use msyt::model::Msyt;
use msyt::model::{self, Content, Entry, MsbtInfo};
use roead::byml::Byml;
use serde_json::Value;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Cursor, Read, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};
use bitvec::prelude::*;

#[derive(Debug)]
pub struct FileData {
    pub file_type: FileType,
    pub data: Vec<u8>,
}

impl FileData {
    pub fn new() -> Self {
        Self {
            file_type: FileType::None,
            data: Vec::new(),
        }
    }
    pub fn from(data: Vec<u8>, file_type: FileType) -> Self {
        Self {
            file_type: file_type,
            data: data,
        }
    }
}

pub struct MsytFile {
    /*pub endian: Option<roead::Endian>,
    pub file_data: FileData,
    pub path: Pathlib,
    pub msbt: std::pin::Pin<Box<Msbt>>,
    pub zstd: Arc<TotkZstd<'a>>,*/
}

pub struct BymlFile<'a> {
    pub endian: Option<roead::Endian>,
    pub file_data: FileData,
    pub path: Pathlib,
    pub pio: roead::byml::Byml,
    pub zstd: Arc<TotkZstd<'a>>,
}

impl<'a> BymlFile<'_> {
    pub fn new(path: String, zstd: Arc<TotkZstd<'a>>) -> io::Result<BymlFile<'a>> {
        let data: FileData =
            BymlFile::byml_data_to_bytes(&PathBuf::from(path.clone()), &zstd.clone())?;
        return BymlFile::from_binary(data, zstd, path);
    }

    pub fn save(&self, path: String) -> io::Result<()> {
        //let mut f_handle = OpenOptions::new().write(true).open(&path)?;
        let mut data = self
            .pio
            .to_binary(self.endian.unwrap_or(roead::Endian::Little));
        if path.to_ascii_lowercase().ends_with(".zs") {
            match self.file_data.file_type {
                FileType::Byml => {
                    data = self
                        .zstd
                        .compressor
                        .compress_zs(&data)
                        .expect("Failed to compress with zs");
                }
                FileType::Bcett => {
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
                path: Pathlib::new("".to_string()),
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

    fn byml_data_to_bytes(path: &PathBuf, zstd: &'a TotkZstd) -> Result<FileData, io::Error> {
        let mut f_handle: fs::File = fs::File::open(path)?;
        let mut buffer: Vec<u8> = Vec::new();
        f_handle.read_to_end(&mut buffer)?;
        //let mut returned_result: Vec<u8> = Vec::new();
        let mut data = FileData::new();
        if is_byml(&buffer) {
            //regular byml file,
            data.data = buffer;
            data.file_type = FileType::Byml;
            return Ok(data);
        } else {
            match zstd.decompressor.decompress_zs(&buffer) {
                //regular byml file compressed with zs
                Ok(res) => {
                    if is_byml(&res) {
                        data.data = res;
                        data.file_type = FileType::Byml;
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
                    data.file_type = FileType::Byml;
                    if is_byml(&data.data) {
                        data.file_type = FileType::Bcett;
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
                    data.file_type = FileType::Other;
                }
                Err(err) => {}
            }
        }
        if data.data.starts_with(b"Yaz0") {
            match roead::yaz0::decompress(&data.data) {
                Ok(dec_data) => {
                    data.data = dec_data;
                }
                Err(_) => {}
            }
        }
        if is_byml(&data.data) {
            data.file_type = FileType::Byml;
            return Ok(data);
        }
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Invalid data, not a byml",
        ));
    }
}

pub fn bytes_to_file(data: Vec<u8>, path: &str) -> io::Result<()> {
    let mut f = fs::File::create(&path); //TODO check if the ::create is sufficient
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

pub struct TagProduct<'a> {
    pub byml: BymlFile<'a>,
    pub path_list: Vec<String>,
    pub tag_list: roead::byml::Byml,
    pub rank_table: roead::byml::Byml,
    pub file_name: String,
    pub actor_tag_data: HashMap<String, Vec<String>>,
    pub cached_tag_list: Vec<String>,
    pub cached_rank_table: String,
    pub bit_table_bytes: roead::byml::Byml,
    pub yaml: String,
    pub endian: roead::Endian,
}

impl<'a> TagProduct<'a> {
    pub fn new(path: String, zstd: Arc<TotkZstd<'a>>) -> io::Result<Self> {
        let byml = BymlFile::new(path, zstd)?;
        Ok(Self {
            byml: byml,
            path_list: Vec::new(),
            tag_list: roead::byml::Byml::default(),
            rank_table: roead::byml::Byml::default(),
            file_name: String::new(),
            actor_tag_data: HashMap::default(),
            cached_tag_list: Vec::new(),
            cached_rank_table: String::new(),
            bit_table_bytes: roead::byml::Byml::default(),
            yaml: String::new(),
            endian: roead::Endian::Little,
        })
    }

    pub fn parse(&mut self) -> Result<(), roead::Error> {
        let p = self.byml.pio.as_map();
        if let Ok(pio) = p {
            //Get path list
            println!("Parsing PathList");
            for p in pio["PathList"].as_array().unwrap() {
                self.path_list.push(p.as_string().unwrap().to_string());
            }
            let mut path_list_count = self.path_list.len();
            // Get Tag list
            println!("Parsing tag_list");
            let tag_list = pio["TagList"].as_array();
            match tag_list {
                Ok(tl) => {self.tag_list = tl.into();},
                Err(err) => {
                    eprintln!("ERROR: {:?} line: {}", err, line!());
                    return Err(err);
                }
            }
            let tag_list_count = pio["TagList"].as_array().unwrap().len();

            // Get Bit Table
            let mut bit_table_bytes: Vec<u8> = Vec::new();
            for byte in pio["BitTable"].as_binary_data().unwrap() {
                bit_table_bytes.push(*byte);
            }
            /*match self.bit_table_bytes.as_array() {
                Ok(arr) => {
                    if arr.is_empty() {
                        return Err(roead::Error::InsufficientData(0, 0));
                    }
                }
                Err(err) => {
                    eprintln!("ERROR: {:?} line: {}", err, line!());
                    return Err(err);
                }
            }*/
            // Valid Check
            /*if let Ok(btb) = self.bit_table_bytes.as_array() {
                if btb.is_empty() {
                    return Err(roead::Error::InvalidData("Bittable empty"));
                }
            }*/

            // Get Rank Table
            println!("Parsing RankTable");
            self.rank_table = pio["RankTable"].clone();
            let mut bit_table_bits = bit_table_bytes.view_bits::<Lsb0>().to_bitvec();
            bit_table_bits.reverse();
            let bit_array_count = bit_table_bits.len();
            // Debug
            println!("INFO: Parsed Bits Count: {}", bit_array_count);
            let mut actor_tag_data_map: HashMap<String, Vec<String>> = std::collections::HashMap::new();
            

            // Get Actors and Tags
            for i in 0..(path_list_count / 3) {
                let actor_path = format!("{}|{}|{}", self.path_list[i*3], self.path_list[(i*3)+1],self.path_list[(i*3)+2]);
                let mut actor_tag_list: Vec<String> = Vec::new();
                for k in 0..tag_list_count {
                    if bit_table_bits[i * tag_list_count + k] == true {
                        actor_tag_list.push(self.tag_list[k].as_string().unwrap().to_string());
                    }
                }
                actor_tag_data_map.insert(actor_path, actor_tag_list);
            }
            self.actor_tag_data = actor_tag_data_map;
            
            self.cached_tag_list.extend(tag_list.unwrap_or(&[roead::byml::Byml::default()]).iter().map(|t| t.as_string().unwrap().to_string()));
            for b in self.rank_table.as_binary_data().unwrap() {
                self.cached_rank_table.push_str(&format!("{:02X}", b));
            }
        }
        Ok(())
    }

    pub fn print(&self) {
        //println!("ActorTagData:\n\n{:?}", self.actor_tag_data);
        let json_data = serde_json::to_string_pretty(&self.actor_tag_data).unwrap_or_else(|_| String::from("{}"));
        //println!("ActorTagData:\n\n{:?}", serde_json::to_string_pretty(&self.actor_tag_data).unwrap_or_else(|_| String::from("{}")));
        //println!("\n\nCachedTagList:\n\n{:?}", self.cached_tag_list);
        //println!("\n\nCachedRankTable:\n\n{:?}", self.cached_rank_table);
        let mut f = fs::File::create("log.json").unwrap();
        f.write_all(json_data.as_bytes());

    }

    fn is_bit_table_bytes_empty(&mut self) -> bool {
        if let Ok(btb) = self.bit_table_bytes.as_array() {
            if btb.is_empty() {
                return true;
            }
        }
        return false;
    }

}

pub fn generic_error(text: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, text)
}

pub fn sort_hashmap(h: HashMap<String, Vec<String>>) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    // Extract keys and sort them
    let mut keys: Vec<_> = h.keys().cloned().collect();
    keys.sort();

    // Sort each Vec<String> in the HashMap
    for key in keys.iter() {
        let value = h.get(key).unwrap().to_vec();
        map.insert(key.to_string(), value);
    }
    map

}