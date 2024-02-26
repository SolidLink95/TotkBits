use crate::file_format::Pack::{PackComparer, PackFile};
use crate::Open_Save::write_string_to_file;
use crate::Settings::Pathlib;
use crate::Zstd::{is_byml, is_msyt, TotkFileType, TotkZstd};
use bitvec::order::LocalBits;
//use byteordered::Endianness;
//use indexmap::IndexMap;
use bitvec::prelude::*;
use msyt::model::{self, Content, Entry, MsbtInfo};
use roead::byml::Byml;
use serde::{Deserialize, Serialize};
use serde_json;
use serde_yaml;
use std::any::type_name;
use std::collections::{BTreeMap, HashMap};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Cursor, Read, Write};
use std::ops::Deref;
use std::os::raw;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io, panic};

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
    pub fn new(path: String, zstd: Arc<TotkZstd<'a>>) -> io::Result<BymlFile<'a>> {
        let data: FileData =
            BymlFile::byml_data_to_bytes(&PathBuf::from(path.clone()), &zstd.clone())?;
        return BymlFile::from_binary(data, zstd, path);
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
            data.file_type = TotkFileType::Byml;
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

#[derive(Serialize, Deserialize)]
struct TagJsonData {
    PathList: BTreeMap<String, Vec<String>>,
    TagList: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct YamlData {
    PathList: Vec<String>,
    BitTable: Vec<u8>, // Assuming this is binary data
    RankTable: String,
    TagList: Vec<String>,
}
pub struct TagProduct<'a> {
    pub byml: BymlFile<'a>,
    pub path_list: Vec<String>,
    pub tag_list: Vec<String>,
    pub rank_table: roead::byml::Byml,
    pub file_name: String,
    pub actor_tag_data: BTreeMap<String, Vec<String>>,
    pub cached_tag_list: Vec<String>,
    pub cached_rank_table: String,
    pub bit_table_bytes: roead::byml::Byml,
    pub text: String,
    pub endian: roead::Endian,
}

impl<'a> TagProduct<'a> {
    pub fn new(path: String, zstd: Arc<TotkZstd<'a>>) -> io::Result<Self> {
        let byml = BymlFile::new(path, zstd)?;
        Ok(Self {
            byml: byml,
            path_list: Vec::new(),
            tag_list: Vec::new(),
            rank_table: roead::byml::Byml::default(),
            file_name: String::new(),
            actor_tag_data: BTreeMap::default(),
            cached_tag_list: Vec::new(),
            cached_rank_table: String::new(),
            bit_table_bytes: roead::byml::Byml::default(),
            text: String::new(),
            endian: roead::Endian::Little,
        })
    }

    pub fn save_default(&mut self, text: &str) -> io::Result<()> {
        let path = self.byml.path.full_path.clone();
        self.save(path, text)
    }

    pub fn save(&mut self, path: String, text: &str) -> io::Result<()> {
        //let mut f_handle = OpenOptions::new().write(true).open(&path)?;
        let mut data: Vec<u8> = Vec::new();
        match self.to_binary(text) {
            Ok(rawdata) => data = rawdata,
            Err(err) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Bytes from tag invalid",
                ));
            }
        }

        if path.to_ascii_lowercase().ends_with(".zs") {
            data = self.byml
                .zstd
                .compressor
                .compress_zs(&data)
                .expect("Failed to compress with zs");
        }
        //f_handle.write_all(&data);
        bytes_to_file(data, &path);
        Ok(())
    }

    pub fn to_binary(&mut self, text: &str) -> Result<Vec<u8>, serde_json::Error> {
        //let data: Config = serde_yaml::from_str(text)?;
        //Header
        print_type_of(&self.byml.pio);
        let mut res : Byml = Byml::from_text("{}").unwrap();
        let mut path_list: Vec<String> = Default::default();
        let mut tag_list: Vec<Byml> = Default::default();
        let json_data: TagJsonData = serde_json::from_str(text)?;
        //PathList
        println!("Work/Actor/|Animal_Boar_A|.engine__actor__ActorParam.gyml {:?}", &json_data.PathList.contains_key("Work/Actor/|Animal_Boar_A|.engine__actor__ActorParam.gyml"));
        println!("Work/Actor/|Animal_Boar_C|.engine__actor__ActorParam.gyml {:?}", &json_data.PathList.contains_key("Work/Actor/|Animal_Boar_C|.engine__actor__ActorParam.gyml"));
        for (path, plist) in &json_data.PathList {
            if path.contains("|") {
                for slice in path.split("|") {
                    //let entry = roead::byml::Byml::String(slice.to_string().into());
                    path_list.push(slice.to_string());
                }
            }
        }
        //Bittable
        let mut bit_table_bits = Vec::new();

        for (actor_tag, tag_entries) in &json_data.PathList  {
            for tag in &self.cached_tag_list {
                let bit = if tag_entries.contains(tag) {
                    true
                } else {
                    false
                };
                bit_table_bits.push(bit);
            }
        }
        // Convert Vec<u8> to BitVec
        let mut bit_table_bit_vec = BitVec::<u8, Lsb0>::with_capacity(bit_table_bits.len());
        bit_table_bit_vec.extend(bit_table_bits.iter().map(|t| t));
        // Reverse the bit order
        //bit_table_bit_vec.reverse();
        // Convert BitVec to bytes
        let bit_table_bytes = bit_table_bit_vec.into_vec();

        //Tag list
        tag_list.extend(
            self.cached_tag_list
                .iter()
                .map(|t| roead::byml::Byml::String(t.to_string().into())),
        );

        
        let yml_data = YamlData {
            PathList: path_list.clone(),
            BitTable: bit_table_bytes, // Example binary data
            RankTable: "".to_string(),
            TagList: self.cached_tag_list.clone(),
        };
        let mut yml: Vec<String> = Vec::new();
        yml.push("PathList:".to_string());
        for p in &yml_data.PathList {
            yml.push(format!("  - {}",p));
        }
        yml.push(format!("BitTable: !!binary {}", base64::encode(yml_data.BitTable)));
        yml.push("RankTable: \"\"".to_string());
        yml.push("TagList:".to_string());

        for tag in yml_data.TagList {
            yml.push(format!("  - {}",tag));
        }

        let yml_string = yml.join("\n");
        write_string_to_file("tmp.txt", &yml_string);
        let raw_data = Byml::from_text(yml_string).unwrap().to_binary(self.endian);

        //res[roead::byml::BymlIndex("PathList")] = roead::byml::Byml::Array(path_list);
        /*res[roead::byml::Byml::String("PathList".to_string().into()).as_u32().unwrap()] = roead::byml::Byml::Array(path_list);
        res["BitTable"] = roead::byml::Byml::BinaryData(bit_table_bytes);
        res["RankTable"] = roead::byml::Byml::String("".to_string().into());
        res["TagList"] = roead::byml::Byml::Array(tag_list);*/

        //TagList
        //let raw_data = res.to_binary(roead::Endian::Little);
        Ok(raw_data)
    }

    pub fn to_text(&mut self)  {
        let actor_tag_data = &self.actor_tag_data;
        let json_data = TagJsonData {
            PathList: self.actor_tag_data.clone(),
            TagList: self.tag_list.clone(),
        };
        /* increase indent
        let mut buf = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        json_data.serialize(&mut ser).unwrap();*/
        //let text = String::from_utf8(buf).unwrap_or("{}".to_string());
        self.text = serde_json::to_string_pretty(&json_data).unwrap_or_else(|_| String::from("{}"));

        
    }

    pub fn parse(&mut self) -> Result<(), roead::Error> {
        let p = self.byml.pio.as_map();
        if let Ok(pio) = p {
            //Get path list
            println!("Parsing PathList");
            /*for p in pio["PathList"].as_array().unwrap() {
                self.path_list.push(p.as_string().unwrap().to_string());
            }*/
            self.path_list.extend(
                pio["PathList"]
                    .as_array()
                    .unwrap_or(&[roead::byml::Byml::default()])
                    .iter()
                    //.map(|t| t.as_string().unwrap().to_string())
                    .map(|t| match t.as_string() {
                        Ok(p) => p.to_string(),
                        _ => "".to_string(),
                    }),
            );
            let mut path_list_count = self.path_list.len();
            // Get Tag list
            println!("Parsing tag_list");
            self.tag_list.extend(
                pio["TagList"]
                    .as_array()
                    .unwrap_or(&[roead::byml::Byml::default()])
                    .iter()
                    .map(|t| match t.as_string() {
                        Ok(p) => p.to_string(),
                        _ => "".to_string(),
                    }),
            );

            /*let tag_list = pio["TagList"].as_array();
            match tag_list {
                Ok(tl) => {
                    for tag in tl {
                        self.tag_list.push(tag.as_string().unwrap().to_string());
                    }
                    //self.tag_list = tl.into();
                }
                Err(err) => {
                    eprintln!("ERROR: {:?} line: {}", err, line!());
                    return Err(err);
                }
            }*/
            let tag_list_count = pio["TagList"]
                .as_array()
                .unwrap_or(&[roead::byml::Byml::default()])
                .len();

            // Get Bit Table
            let mut bit_table_bytes: Vec<u8> = Vec::new();
            for byte in pio["BitTable"].as_binary_data().unwrap() {
                bit_table_bytes.push(*byte);
            }

            // Get Rank Table
            println!("Parsing RankTable");
            self.rank_table = pio["RankTable"].clone();
            let mut bit_table_bits = bit_table_bytes.view_bits::<Lsb0>().to_bitvec();
            //bit_table_bits.reverse();
            let bit_array_count = bit_table_bits.len();
            // Debug
            println!("INFO: Parsed Bits Count: {}", bit_array_count);
            let mut actor_tag_data_map: BTreeMap<String, Vec<String>> =
                std::collections::BTreeMap::new();

            // Get Actors and Tags
            for i in 0..(path_list_count / 3) {
                let actor_path = format!(
                    "{}|{}|{}",
                    self.path_list[i * 3],
                    self.path_list[(i * 3) + 1],
                    self.path_list[(i * 3) + 2]
                );
                let mut actor_tag_list: Vec<String> = Vec::new();
                for k in 0..tag_list_count {
                    if bit_table_bits[i * tag_list_count + k] == true {
                        actor_tag_list.push(self.tag_list[k].clone());
                    }
                }
                actor_tag_data_map.insert(actor_path, actor_tag_list.clone());
            }
            self.actor_tag_data = actor_tag_data_map;
            //self.actor_tag_data = sort_hashmap(&self.actor_tag_data);

            self.cached_tag_list.extend(
                pio["TagList"]
                    .as_array()
                    .unwrap_or(&[roead::byml::Byml::default()])
                    .iter()
                    .map(|t| t.as_string().unwrap().to_string()),
            );

            let rank_table_result =
                panic::catch_unwind(AssertUnwindSafe(|| self.rank_table.as_binary_data()));
            if let Ok(unwrapped_rank_table) = &rank_table_result {
                if let Ok(rank_table) = &unwrapped_rank_table {
                    for b in rank_table.to_vec() {
                        self.cached_rank_table.push_str(&format!("{:02X}", b));
                    }
                }
            }
            /*for b in self.rank_table.as_binary_data().unwrap() {
                self.cached_rank_table.push_str(&format!("{:02X}", b));
            }*/
            self.to_text();
        }
        Ok(())
    }

    pub fn print(&self) {
        //println!("ActorTagData:\n\n{:?}", self.actor_tag_data);
        let json_data = serde_json::to_string_pretty(&self.actor_tag_data)
            .unwrap_or_else(|_| String::from("{}"));
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

pub fn sort_hashmap(h: &HashMap<String, Vec<String>>) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    // Extract keys and sort them
    let mut keys: Vec<_> = h.keys().cloned().collect();
    keys.sort_by_key(|s| s.to_lowercase());

    println!("{} {} {} {}", keys[0], keys[1], keys[15], keys[100]);

    // Sort each Vec<String> in the HashMap
    for key in keys.iter() {
        let value = h.get(key).unwrap().to_vec();
        map.insert(key.to_string(), value);
    }
    map
}

pub struct OpenedFile<'a> {
    pub file_type: TotkFileType,
    pub path: Pathlib,
    pub byml: Option<BymlFile<'a>>,
    pub endian: Option<roead::Endian>,
    pub msyt: Option<String>,
    pub aamp: Option<()>,
    pub tag: Option<TagProduct<'a>>,
}

impl Default for OpenedFile<'_> {
    fn default() -> Self {
        Self {
            file_type: TotkFileType::None,
            path: Pathlib::new("".to_string()),
            byml: None,
            endian: None,
            msyt: None,
            aamp: None,
            tag: None,
        }
    }
}

impl<'a> OpenedFile<'_> {
    pub fn new(
        path: String,
        file_type: TotkFileType,
        endian: Option<roead::Endian>,
        msyt: Option<String>,
    ) -> Self {
        Self {
            file_type: file_type,
            path: Pathlib::new(path),
            byml: None,
            endian: endian,
            msyt: msyt,
            aamp: None,
            tag: None,
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
        }
    }
}

impl<'a> OpenedFile<'_> {
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
}


fn print_type_of<T>(_: &T) {
    println!("{}", type_name::<T>());
}