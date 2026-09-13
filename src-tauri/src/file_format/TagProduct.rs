#![allow(non_snake_case, non_camel_case_types)]
use crate::file_format::BinTextFile::OpenedFile;
use crate::file_format::BinTextFile::{bytes_to_file, BymlFile};
use crate::Open_and_Save::SendData;
use crate::Settings::Pathlib;
use crate::Zstd::{is_tagproduct, TotkFileType, TotkZstd};
//use byteordered::Endianness;
//use indexmap::IndexMap;
use bitvec::prelude::*;

use roead::byml::{self, Byml};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

use std::io::{self, Cursor};
use std::path::Path;
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
struct TagJsonData {
    PathList: BTreeMap<String, Vec<String>>,
    TagList: Vec<String>,
    #[serde(default)]
    RankTable: String,
}

#[derive(Serialize, Deserialize)]
struct YamlData {
    PathList: Vec<String>,
    BitTable: Vec<u8>, // Assuming this is binary data
    RankTable: String,
    TagList: Vec<String>,
}

struct AlphabeticalPathList<'a>(&'a BTreeMap<String, Vec<String>>);

impl Serialize for AlphabeticalPathList<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut entries: Vec<_> = self.0.iter().collect();
        entries.sort_by(|(left, _), (right, _)| {
            left.split('|')
                .collect::<String>()
                .cmp(&right.split('|').collect::<String>())
        });
        let mut map = serializer.serialize_map(Some(entries.len()))?;
        for (path, tags) in entries {
            map.serialize_entry(path, tags)?;
        }
        map.end()
    }
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct TagJsonOutput<'a> {
    PathList: AlphabeticalPathList<'a>,
    TagList: &'a [String],
    RankTable: String,
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
    pub fn from_binary<P: AsRef<Path>>(
        data: &Vec<u8>,
        path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<Self> {
        let file_data = BymlFile::byml_data_to_bytes(data, zstd.clone()).ok()?;
        let mut byml = BymlFile::from_binary(
            &file_data.data,
            zstd,
            path.as_ref().to_string_lossy().into_owned(),
        )
        .ok()?;
        byml.file_type = file_data.file_type;
        byml.file_data = file_data;
        let mut tag_product = TagProduct {
            byml,
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
        tag_product.parse().ok()?;
        Some(tag_product)
    }

    pub fn open_tag<P: AsRef<Path>>(
        path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(
        super::BinTextFile::OpenedFile<'a>,
        crate::Open_and_Save::SendData,
    )> {
        let mut opened_file = OpenedFile::default();
        let mut data = SendData::default();
        let path_ref = path.as_ref();
        let pathlib_var = Pathlib::new(path_ref);
        print!("Is {} a tag? ", &pathlib_var.full_path);
        if is_tagproduct(path_ref) {
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

    pub fn open_tag_binary<P: AsRef<Path>>(
        binary: &[u8],
        display_path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(OpenedFile<'a>, SendData)> {
        let path = display_path.as_ref();
        let mut tag = Self::from_binary(&binary.to_vec(), path, zstd)?;
        let mut opened = OpenedFile::default();
        opened.path = Pathlib::new(path);
        opened.endian = Some(roead::Endian::Little);
        opened.file_type = TotkFileType::TagProduct;
        let text = tag.to_text();
        opened.tag = Some(tag);
        let mut data = SendData::default();
        data.path = Pathlib::new(path);
        data.text = text;
        data.lang = "json".into();
        data.status_text = format!("Opened {}", path.display());
        data.get_file_label(TotkFileType::TagProduct, Some(roead::Endian::Little));
        Some((opened, data))
    }
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
        let mut data: Vec<u8> = Self::to_binary(text, self.rank_table_bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        if let Some(compression) = self.byml.file_data.compression {
            data = if compression == crate::Zstd::ZstdDictionary::Yaz0 {
                crate::Zstd::TotkZstd::compress_yaz0_with_alignment(
                    &data,
                    self.byml.file_data.yaz0_alignment,
                )?
            } else {
                self.byml
                    .zstd
                    .compress_with_dictionary(&data, compression)?
            };
        }
        //f_handle.write_all(&data);
        bytes_to_file(data, &path)?;
        Ok(())
    }

    pub fn to_binary(text: &str, rank_table: &[u8]) -> io::Result<Vec<u8>> {
        //let data: Config = serde_yaml::from_str(text)?;
        //Header
        // let _res : Byml = Byml::from_text("{}").unwrap();
        let mut path_list: Vec<Byml> = Default::default();
        let mut tag_list: Vec<Byml> = Default::default();
        let json_data: TagJsonData = serde_json::from_str(text)?;
        let mut cached_tag_list = json_data.TagList;
        cached_tag_list.sort();
        if cached_tag_list
            .windows(2)
            .any(|pair| matches!(pair, [left, right] if left == right))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "TagList contains duplicate tags",
            ));
        }
        //PathList
        let mut sorted_paths: Vec<_> = json_data.PathList.iter().collect();
        sorted_paths.sort_by(|(left, _), (right, _)| {
            let left_parts: String = left.split('|').collect();
            let right_parts: String = right.split('|').collect();
            left_parts.cmp(&right_parts)
        });
        for (path, _plist) in &sorted_paths {
            let parts: Vec<_> = path.split('|').collect();
            if parts.len() != 3 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("TagProduct path must have exactly three pipe-delimited parts: {path}"),
                ));
            }
            for slice in parts {
                path_list.push(roead::byml::Byml::String(slice.into()));
            }
        }
        //Bittable
        let mut bit_table_bits = Vec::new();

        for (_actor_tag, tag_entries) in &sorted_paths {
            for tag in *tag_entries {
                if !cached_tag_list.contains(tag) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Entry references tag that is absent from TagList: {tag}"),
                    ));
                }
            }
            for tag in &cached_tag_list {
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
                    Byml::BinaryData(rank_table.to_vec()),
                );
                x.insert("TagList".to_string().into(), Byml::Array(tag_list));
            }
            // TotK Tag.Product files use BYML version 7. Roead's writer supports
            // the required node layout but currently restricts its public version
            // argument to 2-4, so write version 4 and update the header version.
            let mut binary = Vec::new();
            res.write(&mut Cursor::new(&mut binary), roead::Endian::Little, 4)
                .map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("TagProduct serialization failed: {error}"),
                    )
                })?;
            crate::parser::binary::BinaryPatcher::new(&mut binary)
                .write_u16_at(2, 7)
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "serialized TagProduct header is truncated",
                    )
                })?;
            return Ok(binary);
        }

        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Failed to convert to binary",
        ))
    }

    pub fn to_text(&mut self) -> String {
        let json_data = TagJsonOutput {
            PathList: AlphabeticalPathList(&self.actor_tag_data),
            TagList: &self.tag_list,
            RankTable: rank_table_sha256(self.rank_table_bytes()),
        };
        serde_json::to_string_pretty(&json_data).unwrap_or(String::from("{}"))
    }

    pub fn rank_table_bytes(&self) -> &[u8] {
        self.rank_table.as_binary_data().unwrap_or_default()
    }

    pub fn parse(&mut self) -> Result<(), roead::Error> {
        self.path_list.clear();
        self.tag_list.clear();
        self.actor_tag_data.clear();
        self.cached_tag_list.clear();
        self.cached_rank_table.clear();
        let pio = self.byml.pio.as_map()?;
        //Get path list
        println!("Parsing PathList");
        let path_list = pio
            .get("PathList")
            .ok_or_else(|| roead::Error::Any("TagProduct has no PathList".into()))?;
        self.path_list.extend(
            path_list
                .as_array()?
                .iter()
                //.map(|t| t.as_string().unwrap().to_string())
                .map(|t| t.as_string().map(ToString::to_string))
                .collect::<Result<Vec<_>, _>>()?,
        );
        let path_list_count = self.path_list.len();
        if path_list_count % 3 != 0 {
            return Err(roead::Error::Any(
                "TagProduct PathList length is not divisible by three".into(),
            ));
        }
        // Get Tag list
        println!("Parsing tag_list");
        let tag_list = pio
            .get("TagList")
            .ok_or_else(|| roead::Error::Any("TagProduct has no TagList".into()))?;
        self.tag_list.extend(
            tag_list
                .as_array()?
                .iter()
                .map(|t| t.as_string().map(ToString::to_string))
                .collect::<Result<Vec<_>, _>>()?,
        );

        let tag_list_count = tag_list.as_array()?.len();
        let required_bits = (path_list_count / 3)
            .checked_mul(tag_list_count)
            .ok_or_else(|| roead::Error::Any("TagProduct dimensions overflow".into()))?;

        // Get Bit Table
        let mut bit_table_bytes: Vec<u8> = Vec::new();
        let bit_table = pio
            .get("BitTable")
            .ok_or_else(|| roead::Error::Any("TagProduct has no BitTable".into()))?;
        for byte in bit_table.as_binary_data()? {
            bit_table_bytes.push(*byte);
        }

        // Get Rank Table
        println!("Parsing RankTable");
        self.rank_table = normalize_rank_table(pio.get("RankTable"));
        let rank_table = self.rank_table.as_binary_data()?;
        let bit_table_bits = bit_table_bytes.view_bits::<Lsb0>().to_bitvec();
        //bit_table_bits.reverse();
        let bit_array_count = bit_table_bits.len();
        if bit_array_count < required_bits {
            return Err(roead::Error::Any(
                "TagProduct BitTable is shorter than PathList x TagList".into(),
            ));
        }
        // Debug
        println!("INFO: Parsed Bits Count: {}", bit_array_count);
        let mut actor_tag_data_map: BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();

        // Get Actors and Tags
        for i in 0..(path_list_count / 3) {
            let start = i
                .checked_mul(3)
                .ok_or_else(|| roead::Error::Any("TagProduct path index overflow".into()))?;
            let parts = self
                .path_list
                .get(start..start + 3)
                .ok_or_else(|| roead::Error::Any("TagProduct path entry is truncated".into()))?;
            let [prefix, actor, suffix] = parts else {
                return Err(roead::Error::Any(
                    "TagProduct path entry must contain three strings".into(),
                ));
            };
            let actor_path = format!("{prefix}|{actor}|{suffix}");
            let mut actor_tag_list: Vec<String> = Vec::new();
            for k in 0..tag_list_count {
                let bit_index = i
                    .checked_mul(tag_list_count)
                    .and_then(|value| value.checked_add(k))
                    .ok_or_else(|| roead::Error::Any("TagProduct bit index overflow".into()))?;
                let enabled = bit_table_bits
                    .get(bit_index)
                    .ok_or_else(|| roead::Error::Any("TagProduct BitTable is truncated".into()))?;
                if *enabled {
                    let tag = self.tag_list.get(k).ok_or_else(|| {
                        roead::Error::Any("TagProduct TagList is truncated".into())
                    })?;
                    actor_tag_list.push(tag.clone());
                }
            }
            actor_tag_data_map.insert(actor_path, actor_tag_list.clone());
        }
        self.actor_tag_data = actor_tag_data_map;
        //self.actor_tag_data = sort_hashmap(&self.actor_tag_data);

        self.cached_tag_list.extend(
            tag_list
                .as_array()?
                .iter()
                .filter_map(|t| t.as_string().ok().map(|value| value.to_string())),
        );

        for b in rank_table {
            self.cached_rank_table.push_str(&format!("{:02X}", b));
        }
        /*for b in self.rank_table.as_binary_data().unwrap() {
            self.cached_rank_table.push_str(&format!("{:02X}", b));
        }*/
        //self.to_text();
        Ok(())
    }
}

fn rank_table_sha256(rank_table: &[u8]) -> String {
    format!("{:x}", Sha256::digest(rank_table))
}

fn normalize_rank_table(rank_table: Option<&Byml>) -> Byml {
    match rank_table {
        Some(Byml::BinaryData(bytes)) => Byml::BinaryData(bytes.clone()),
        _ => Byml::BinaryData(Vec::new()),
    }
}

#[allow(dead_code)]
pub fn sort_hashmap(h: &HashMap<String, Vec<String>>) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    // Extract keys and sort them
    let mut keys: Vec<_> = h.keys().cloned().collect();
    keys.sort_by_key(|s| s.to_lowercase());

    // Sort each Vec<String> in the HashMap
    for key in keys.iter() {
        if let Some(value) = h.get(key) {
            map.insert(key.to_string(), value.to_vec());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_writer_uses_backend_rank_table_and_preserves_tag_matrix() {
        let text = r#"{
            "PathList": {
                "Work/Actor/Enemy|Enemy_Bokoblin|.engine__actor__ActorParam.gyml": ["ZTag"],
                "Work/Actor/Enemy|Enemy_Bokoblin_Junior|.engine__actor__ActorParam.gyml": ["ATag", "ZTag"]
            },
            "TagList": ["ZTag", "ATag"],
            "RankTable": "this editor value is read-only and ignored"
        }"#;

        let binary =
            TagProduct::to_binary(text, &[0, 1, 2, 255]).expect("TagProduct should serialize");
        assert_eq!(&binary[..4], &[b'Y', b'B', 7, 0]);

        let root = Byml::from_binary(&binary).expect("written BYML should parse");
        let map = root.as_map().expect("root should be a map");
        assert_eq!(map["RankTable"].as_binary_data().unwrap(), &[0, 1, 2, 255]);
        assert_eq!(
            map["TagList"]
                .as_array()
                .unwrap()
                .iter()
                .map(|node| node.as_string().unwrap().as_str())
                .collect::<Vec<_>>(),
            ["ATag", "ZTag"]
        );
        // Two entries by two tags, packed continuously and LSB-first:
        // first row 10, second row 11 => 0b1110.
        assert_eq!(map["BitTable"].as_binary_data().unwrap(), &[0b1110]);
        assert_eq!(
            rank_table_sha256(&[0, 1, 2, 255]),
            "3d1f57c984978ef98a18378c8166c1cb8ede02c03eeb6aee7e2f121dfeee3e56"
        );
    }

    #[test]
    fn text_writer_rejects_invalid_paths_and_unknown_tags() {
        let invalid_path = r#"{"PathList":{"only|two":[]},"TagList":[],"RankTable":""}"#;
        assert!(TagProduct::to_binary(invalid_path, &[]).is_err());

        let unknown_tag = r#"{"PathList":{"a|b|c":["missing"]},"TagList":[],"RankTable":""}"#;
        assert!(TagProduct::to_binary(unknown_tag, &[]).is_err());
    }

    #[test]
    fn invalid_or_missing_rank_table_is_treated_as_empty_binary_data() {
        assert_eq!(
            normalize_rank_table(Some(&Byml::BinaryData(vec![1, 2, 3]))),
            Byml::BinaryData(vec![1, 2, 3])
        );
        assert_eq!(
            normalize_rank_table(Some(&Byml::String(String::new().into()))),
            Byml::BinaryData(Vec::new())
        );
        assert_eq!(
            normalize_rank_table(Some(&Byml::I32(42))),
            Byml::BinaryData(Vec::new())
        );
        assert_eq!(normalize_rank_table(None), Byml::BinaryData(Vec::new()));
    }
}
