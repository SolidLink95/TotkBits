#![allow(non_snake_case, non_camel_case_types)]
// use std::any;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::file_format::BinTextFile::OpenedFile;
use crate::parser::rstb::ResourceSizeTable;
use crate::Open_and_Save::SendData;
use crate::Settings::Magic;
use crate::Settings::{list_files_recursively, Pathlib};
use crate::Zstd::{TotkFileType, TotkZstd, ZstdDictionary};
// use serde_json::to_string_pretty;

use super::Pack::PackFile;

fn should_scan_local_rstb_paths(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("Resource"))
}

fn comparable_windows_path(path: &Path) -> Option<String> {
    if path.as_os_str().is_empty() {
        return None;
    }
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|current| current.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    });
    let mut normalized = resolved.to_string_lossy().replace('\\', "/");
    if normalized
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("//?/UNC/"))
    {
        normalized.replace_range(..8, "//");
    } else if normalized.starts_with("//?/") {
        normalized.replace_range(..4, "");
    }
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }
    Some(normalized.to_lowercase())
}

fn same_windows_path(left: &Path, right: &Path) -> bool {
    match (
        comparable_windows_path(left),
        comparable_windows_path(right),
    ) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn table_to_json(table: &ResourceSizeTable, known_paths: &[String]) -> io::Result<String> {
    let mut paths_by_hash = BTreeMap::<u32, &str>::new();
    for path in known_paths {
        let hash = crate::parser::rstb::crc32(path);
        if table.hash_table.contains_key(&hash) {
            paths_by_hash.entry(hash).or_insert(path);
        }
    }

    let mut named = BTreeMap::<String, u32>::new();
    let mut numeric = BTreeMap::<u32, u32>::new();
    for (&hash, &value) in &table.hash_table {
        if let Some(path) = paths_by_hash.get(&hash) {
            named.insert((*path).to_owned(), value);
        } else {
            numeric.insert(hash, value);
        }
    }
    named.extend(
        table
            .overflow_table
            .iter()
            .map(|(path, value)| (path.clone(), *value)),
    );
    let entries: Vec<_> = named
        .into_iter()
        .chain(
            numeric
                .into_iter()
                .map(|(hash, value)| (hash.to_string(), value)),
        )
        .collect();
    let mut json = String::from("{\n");
    for (index, (key, value)) in entries.iter().enumerate() {
        let key = serde_json::to_string(key).map_err(io::Error::other)?;
        let comma = if index + 1 == entries.len() { "" } else { "," };
        json.push_str(&format!("  {key}: {value}{comma}\n"));
    }
    json.push_str("}\n");
    Ok(json)
}

fn json_to_entries(
    json: &str,
    existing_overflow: &BTreeMap<String, u32>,
) -> io::Result<(BTreeMap<u32, u32>, BTreeMap<String, u32>)> {
    let mapping = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(json)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut hashes = BTreeMap::<u32, u32>::new();
    let mut overflow = BTreeMap::<String, u32>::new();
    for (key, value) in mapping {
        let value = value.as_u64().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "RSTB values must be integers")
        })?;
        let value = u32::try_from(value)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "RSTB value exceeds u32"))?;
        if let Some(hash) = key
            .strip_prefix("0x")
            .or_else(|| key.strip_prefix("0X"))
            .and_then(|hash| u32::from_str_radix(hash, 16).ok())
            .or_else(|| key.parse::<u32>().ok())
        {
            hashes.insert(hash, value);
        } else if existing_overflow.contains_key(&key) {
            overflow.insert(key, value);
        } else {
            hashes.insert(crate::parser::rstb::crc32(key), value);
        }
    }
    Ok((hashes, overflow))
}

// use super::RstbData::get_rstb_data;

#[allow(dead_code)]
pub struct Restbl<'a> {
    pub path: Pathlib,
    pub zstd: Arc<TotkZstd<'a>>,
    // buffer: Arc<Vec<u8>>, // Use Arc to share ownership
    pub table: ResourceSizeTable,
    pub hash_table: Arc<Vec<String>>,
    pub compression: Option<ZstdDictionary>,
}

impl<'a> Restbl<'_> {
    pub(crate) fn to_json(&self) -> io::Result<String> {
        table_to_json(&self.table, &self.hash_table)
    }

    pub fn apply_json(&mut self, json: &str) -> io::Result<()> {
        let (hashes, overflow) = json_to_entries(json, &self.table.overflow_table)?;
        self.table.replace_entries(hashes, overflow);
        Ok(())
    }

    pub fn cached_path(&self, path: &str) -> Option<&str> {
        self.hash_table
            .iter()
            .find(|candidate| candidate.eq_ignore_ascii_case(path))
            .map(String::as_str)
    }

    pub fn open_restbl<P: AsRef<Path>>(
        path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(
        super::BinTextFile::OpenedFile<'a>,
        crate::Open_and_Save::SendData,
    )> {
        let mut opened_file = OpenedFile::default();
        let path_ref = path.as_ref();
        let mut data = SendData::default();
        println!("[RSTB] route: considering {}", path_ref.display());
        let pathlib_var = Pathlib::new(path_ref);
        let recognized_name = pathlib_var
            .name
            .to_lowercase()
            .starts_with("resourcesizetable.product");
        println!(
            "[RSTB] route: file name {:?}, ResourceSizeTable.Product prefix match={}",
            pathlib_var.name, recognized_name
        );
        if recognized_name {
            println!("[RSTB] route: dispatching to Restbl::from_path");
            opened_file.restbl = Restbl::from_path(path_ref, zstd.clone());
            if let Some(restbl) = &mut opened_file.restbl {
                println!("[RSTB] route: RSTB opened successfully");
                data.tab = "RSTB".to_string();
                opened_file.path = pathlib_var.clone();
                opened_file.endian = Some(roead::Endian::Little);
                opened_file.file_type = TotkFileType::Restbl;
                data.status_text = format!("Opened {}", &pathlib_var.full_path);
                data.path = pathlib_var;
                // data.text = restbl.to_text();
                data.get_file_label(TotkFileType::Restbl, Some(roead::Endian::Little));
                if zstd.totk_config.rstb_view == "json" {
                    match restbl.to_json() {
                        Ok(json) => {
                            data.tab = "YAML".to_string();
                            data.lang = "json".to_string();
                            data.text = json;
                            data.read_only = false;
                        }
                        Err(error) => {
                            data.tab = "ERROR".to_string();
                            data.status_text = format!("Unable to display RSTB as JSON: {error}");
                        }
                    }
                }
                return Some((opened_file, data));
            }
            println!("[RSTB] route: Restbl::from_path rejected the file");
        }
        println!("[RSTB] route: not handled as RSTB");
        None
    }

    pub fn open_restbl_binary<P: AsRef<Path>>(
        binary: &[u8],
        display_path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(OpenedFile<'a>, SendData)> {
        let path = display_path.as_ref();
        let mut restbl = Self::from_binary(binary, zstd.clone(), path)?;
        let mut opened = OpenedFile::default();
        opened.path = Pathlib::new(path);
        opened.endian = Some(roead::Endian::Little);
        opened.file_type = TotkFileType::Restbl;
        let mut data = SendData::default();
        data.tab = "RSTB".into();
        data.path = Pathlib::new(path);
        data.status_text = format!("Opened {}", path.display());
        if zstd.totk_config.rstb_view == "json" {
            data.tab = "YAML".into();
            data.lang = "json".into();
            data.text = restbl.to_json().ok()?;
        }
        data.get_file_label(TotkFileType::Restbl, Some(roead::Endian::Little));
        opened.restbl = Some(restbl);
        Some((opened, data))
    }

    pub fn get_restb_entries<P: AsRef<Path>>(&mut self, path: P) -> io::Result<Arc<Vec<String>>> {
        let mut res = crate::utils::LookupData::rstb_paths();
        if !should_scan_local_rstb_paths(path.as_ref()) {
            return Ok(res);
        }
        let mut p = PathBuf::from(path.as_ref());
        for _ in 0..3 {
            if !p.pop() {
                return Ok(res); //unable to go to mod romfs path
            }
        }
        //No point in updating from romfs dump
        if same_windows_path(&p, Path::new(&self.zstd.totk_config.romfs)) {
            return Ok(res);
        }
        let mod_romfs_path = p.to_string_lossy().to_string().replace("\\", "/");
        let mod_romfs_path_len = mod_romfs_path.len();
        //limit to map files and actors
        let valid_paths = vec!["Pack/Actor", "AI", "AS"];
        for entry in valid_paths.iter() {
            //no point in calling res.contains() since the check costs more time than
            //adding redundant local path
            let valid_path = PathBuf::from(&mod_romfs_path).join(entry);
            if !valid_path.exists() {
                continue;
            }
            for file in list_files_recursively(&valid_path) {
                // println!("  {}", &file);
                let mut local_path = file.replace("\\", "/")[mod_romfs_path_len..].to_string();
                if local_path.starts_with("/") {
                    local_path = local_path[1..].to_string()
                }
                let local_path_lower = local_path.to_ascii_lowercase();
                if local_path.to_ascii_lowercase().ends_with(".zs") {
                    local_path = local_path[..(local_path.len() - 3)].to_string()
                }
                // if !res.contains(&local_path) {
                // println!("Adding custom rstb path: {}", &local_path);
                // }
                if entry == &"Pack/Actor"
                    && (local_path_lower.ends_with(".pack") || local_path_lower.ends_with(".sarc"))
                {
                    if let Ok(pack) = PackFile::new(&file, self.zstd.clone()) {
                        for entry in pack.sarc.files() {
                            let entry_path = entry.name.unwrap_or_default().to_string();
                            // if !entry_path.is_empty() && !res.contains(&entry_path) {
                            if !entry_path.is_empty() {
                                // println!("Adding custom rstb sarc path: {}", &entry_path);
                                Arc::make_mut(&mut res).push(entry_path);
                            }
                        }
                    }
                } else {
                    Arc::make_mut(&mut res).push(local_path);
                }
            }
        }

        Ok(res)
    }

    pub fn from_binary<P: AsRef<Path>>(
        data: &[u8],
        zstd: Arc<TotkZstd<'a>>,
        path: P,
    ) -> Option<Restbl<'a>> {
        let path = path.as_ref();
        let (buffer, compression) = zstd.try_decompress_all_ordered_safe(data, path);
        if !Magic::is_restbl(&buffer) {
            return None; //invalid rstb
        }

        println!("[RSTB] native parser: parsing {} bytes", buffer.len());
        match ResourceSizeTable::from_bytes(&buffer) {
            Ok(t) => {
                println!("[RSTB] native parser: parse succeeded");
                // let hash_table = get_rstb_data().unwrap_or_default();

                let mut new_restbl = Restbl {
                    path: Pathlib::new(path),
                    zstd: zstd.clone(),
                    table: t,
                    hash_table: Default::default(),
                    compression: (compression != ZstdDictionary::None).then_some(compression),
                };
                //TODO: check if self function works
                new_restbl.hash_table = new_restbl.get_restb_entries(path).unwrap_or_default();
                return Some(new_restbl);
            }
            Err(err) => {
                println!("[RSTB] native parser: parse failed: {err}");
                eprintln!("{:?}", err);
            }
        }
        // return Err(io::Error::new(io::ErrorKind::InvalidData, ""));
        None
    }

    pub fn from_path<P: AsRef<Path>>(path: P, zstd: Arc<TotkZstd<'a>>) -> Option<Restbl<'a>> {
        let path = path.as_ref();
        let buffer = fs::read(path).ok()?;
        Self::from_binary(&buffer, zstd, path)
    }

    pub fn save_default(&mut self) -> io::Result<()> {
        self.save(&self.path.full_path.clone())
    }

    pub fn save(&mut self, path: &str) -> io::Result<()> {
        let mut buffer = self
            .table
            .to_bytes()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if let Some(compression) = self.compression {
            buffer = self.zstd.compress_with_dictionary(&buffer, compression)?;
        }
        if buffer.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "refusing to save an empty RSTB",
            ));
        }
        let mut f = File::create(&path)?;
        f.write_all(&buffer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::binary::BinaryWriter;

    #[test]
    fn json_resolves_known_paths_and_preserves_unknown_hashes() {
        let known = "System/Known.product.byml".to_string();
        let known_hash = crate::parser::rstb::crc32(&known);
        let unknown_hash = 0x1234_5678;
        let mut writer = BinaryWriter::new();
        writer.write_bytes(b"RSTB");
        writer.write_u32(2);
        writer.write_u32(0);
        writer.write_u32(known_hash);
        writer.write_u32(100);
        writer.write_u32(unknown_hash);
        writer.write_u32(200);
        let table = ResourceSizeTable::from_bytes(&writer.into_inner()).unwrap();
        let json = table_to_json(&table, &[known]).unwrap();
        assert_eq!(
            json,
            "{\n  \"System/Known.product.byml\": 100,\n  \"305419896\": 200\n}\n"
        );
        let (hashes, overflow) = json_to_entries(&json, &BTreeMap::new()).unwrap();
        assert_eq!(hashes[&known_hash], 100);
        assert_eq!(hashes[&unknown_hash], 200);
        assert!(overflow.is_empty());
    }

    #[test]
    fn local_path_expansion_requires_resource_parent() {
        assert!(should_scan_local_rstb_paths(Path::new(
            "Mod/System/Resource/ResourceSizeTable.Product.121.rsizetable"
        )));
        assert!(should_scan_local_rstb_paths(Path::new(
            "Mod/System/resource/ResourceSizeTable.Product.121.rsizetable"
        )));
        assert!(!should_scan_local_rstb_paths(Path::new(
            "Mod/System/Other/ResourceSizeTable.Product.121.rsizetable"
        )));
        assert!(!should_scan_local_rstb_paths(Path::new(
            "ResourceSizeTable.Product.121.rsizetable"
        )));
    }

    #[test]
    fn romfs_comparison_normalizes_windows_paths() {
        assert!(same_windows_path(
            Path::new("W:\\Games\\TotK\\romfs\\"),
            Path::new("w:/games/totk/ROMFS")
        ));
        assert!(!same_windows_path(Path::new(""), Path::new("")));
        assert!(!same_windows_path(
            Path::new("W:/Games/TotK/romfs"),
            Path::new("W:/Games/TotK/mod")
        ));
    }
}
