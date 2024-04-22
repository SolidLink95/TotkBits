#![allow(non_snake_case, non_camel_case_types)]
use crate::file_format::BinTextFile::{bytes_to_file, BymlFile};
use crate::Zstd::TotkZstd;
//use byteordered::Endianness;
//use indexmap::IndexMap;
use bitvec::prelude::*;

use roead::byml::{self, Byml};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::{BTreeMap, HashMap};

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::{io, panic};

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
    pub fn new(path: String, zstd: Arc<TotkZstd<'a>>) -> Option<Self> {
        if let Some(byml) = BymlFile::new(path.clone(), zstd.clone()) {
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
        let mut data: Vec<u8> = Self::to_binary(text).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        if path.to_ascii_lowercase().ends_with(".zs") {
            data = self
                .byml
                .zstd
                // .compressor
                .cpp_compressor
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
        for (path, _plist) in &json_data.PathList {
            if path.contains("|") {
                for slice in path.split("|") {
                    let entry = roead::byml::Byml::String(slice.into());
                    path_list.push(entry);
                }
            }
        }
        //Bittable
        let mut bit_table_bits = Vec::new();

        for (_actor_tag, tag_entries) in &json_data.PathList {
            for tag in cached_tag_list {
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
        let json_data = TagJsonData {
            PathList: self.actor_tag_data.clone(),
            TagList: self.tag_list.clone(),
        };
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
