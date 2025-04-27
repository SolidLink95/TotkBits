#![allow(non_snake_case, non_camel_case_types)]
use crate::file_format::BinTextFile::{bytes_to_file, BymlFile};
use crate::Open_and_Save::SendData;
use crate::Settings::Pathlib;
use crate::Zstd::{is_tagproduct_path, TotkFileType, TotkZstd};
//use byteordered::Endianness;
//use indexmap::IndexMap;
use bitvec::prelude::*;

use roead::byml::{self, Byml};
use serde::{Deserialize, Serialize};
use serde_json::{self, json};
use std::collections::{BTreeMap, HashMap};

use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::Arc;
use std::{io, panic};

use super::BinTextFile::OpenedFile;

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
#[allow(dead_code)]
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
    pub fn new<P: AsRef<Path>>(path: P, zstd: Arc<TotkZstd<'a>>) -> Option<Self> {
        if let Some(byml) = BymlFile::new(path.as_ref(), zstd.clone()) {
            let mut tag_product = TagProduct {
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
            };
            if tag_product.parse().is_ok() {
                return Some(tag_product);
            }
        }
        None
    }

    #[allow(dead_code)]
    pub fn save_default(&mut self, text: &str) -> io::Result<()> {
        let path = self.byml.path.full_path.clone();
        self.save(path, text)
    }

    #[allow(dead_code)]
    pub fn save(&mut self, path: String, text: &str) -> io::Result<()> {
        //let mut f_handle = OpenOptions::new().write(true).open(&path)?;
        let mut data: Vec<u8> =
            Self::to_binary(text).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        if path.to_ascii_lowercase().ends_with(".zs") {
            data = self
                .byml
                .zstd
                .compress_zs(&data)
                .expect("Failed to compress with zs");
        }
        //f_handle.write_all(&data);
        bytes_to_file(data, &path)?;
        Ok(())
    }

    pub fn to_binary(text: &str) -> io::Result<Vec<u8>> {
        //let data: Config = serde_yaml::from_str(text)?;
        //Header
        // let _res : Byml = Byml::from_text("{}").unwrap();
        let mut path_list: Vec<Byml> = Default::default();
        let mut tag_list: Vec<Byml> = Default::default();
        let json_data: TagJsonData = serde_json::from_str(text)?;
        let cached_tag_list = &json_data.TagList;
        //PathList

        let mut path_vec: Vec<(String, Vec<String>)> = json_data.PathList.clone().into_iter().collect();
        let sorted_map: BTreeMap<String, Vec<String>> = json_data.PathList.into_iter().collect();

        // Custom sort based on the pipe-delimited value first
        path_vec.clone().sort_by(|a, b| {
            let extract = |s: &str| {
                s.split('|')
                    .nth(1) // get the string between the first pair of '|'
                    .map(|part| part.to_string())
                    .unwrap_or_else(|| s.to_string())
            };
            extract(&a.0).cmp(&extract(&b.0)).then_with(|| a.0.cmp(&b.0)) // fallback to full key
        });

        // Then push entries
        for (path, _plist) in &path_vec {
            if path.contains('|') {
                for slice in path.split('|') {
                    let entry = roead::byml::Byml::String(slice.into());
                    path_list.push(entry);
                }
            }
        }

        // for (path, _plist) in &sorted_map {
        //     if path.contains("|") {
        //         for slice in path.split("|") {
        //             let entry = roead::byml::Byml::String(slice.into());
        //             path_list.push(entry);
        //         }
        //     }
        // }
        //Bittable
        let mut bit_table_bits = Vec::new();

        for (_actor_tag, tag_entries) in &sorted_map {
            for tag in cached_tag_list {
                let bit = tag_entries.contains(tag);
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
            cached_tag_list
                .iter()
                .map(|t| roead::byml::Byml::String(t.to_string().into())),
        );

        let mut res = byml::Byml::from_text("{}");
        if let Ok(res) = &mut res {
            if let Ok(x) = res.as_mut_map() {
                x.insert("PathList".to_string().into(), Byml::Array(path_list));
                x.insert(
                    "BitTable".to_string().into(),
                    Byml::BinaryData(bit_table_bytes),
                );
                x.insert(
                    "RankTable".to_string().into(),
                    Byml::String("".to_string().into()),
                );
                x.insert("TagList".to_string().into(), Byml::Array(tag_list));
            }
            return Ok(res.to_binary(roead::Endian::Little));
        }

        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Failed to convert to binary",
        ))
    }

    pub fn to_text(&mut self) -> String {
        let _actor_tag_data = &self.actor_tag_data;
        // let json_data = TagJsonData {
        //     PathList: self.actor_tag_data.clone(),
        //     TagList: self.tag_list.clone(),
        // };
        let json_data = json!({
            "PathList": self.actor_tag_data,
            "TagList": self.tag_list
        });
        serde_json::to_string_pretty(&json_data).unwrap_or(String::from("{}"))
    }

    pub fn parse(&mut self) -> Result<(), roead::Error> {
        let p = self.byml.pio.as_map();
        if let Ok(pio) = p {
            //Get path list
            println!("Parsing PathList");
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
            let path_list_count = self.path_list.len();
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
            let bit_table_bits = bit_table_bytes.view_bits::<Lsb0>().to_bitvec();
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
            //self.to_text();
        }
        Ok(())
    }

    pub fn open_tag<P:AsRef<Path>>(path: P, zstd: Arc<TotkZstd>) -> Option<(OpenedFile, SendData)> {
        let mut opened_file = OpenedFile::default();
        let mut data = SendData::default();
        let path_ref = path.as_ref();
        let pathlib_var = Pathlib::new(path_ref);
        print!("Is {} a tag? ", &pathlib_var.full_path);
        if is_tagproduct_path(path_ref)
        {
            opened_file.tag = TagProduct::new(path_ref, zstd.clone());
            if let Some(tag) = &mut opened_file.tag {
                println!(" yes!");
                opened_file.path = pathlib_var.clone();
                opened_file.endian = Some(roead::Endian::Little);
                opened_file.file_type = TotkFileType::TagProduct;
                data.status_text = format!("Opened {}", &pathlib_var.full_path);
                data.path = pathlib_var;
                data.text = tag.to_text();
                data.lang = "json".to_string();
                data.get_file_label(TotkFileType::TagProduct, Some(roead::Endian::Little));
                return Some((opened_file, data));
            }
        }
        println!(" no");
        None
    }
}

#[allow(dead_code)]
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
