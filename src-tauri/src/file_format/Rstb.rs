#![allow(non_snake_case, non_camel_case_types)]
// use std::any;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::Open_and_Save::SendData;
use crate::Settings::{list_files_recursively, Pathlib};
use crate::Zstd::{is_restbl, TotkFileType, TotkZstd};
use flate2::read::ZlibDecoder;
use restbl::bin::ResTblReader;
use restbl::ResourceSizeTable;
// use serde_json::to_string_pretty;

use super::BinTextFile::OpenedFile;
use super::Pack::PackFile;

// use super::RstbData::get_rstb_data;

#[allow(dead_code)]
fn get_rstb_data() -> io::Result<Vec<String>> {
    let json_zlibdata = fs::read("bin/totk_rstb_paths.bin")?;
    let mut decoder = ZlibDecoder::new(&json_zlibdata[..]);
    let mut json_str = String::new();
    decoder.read_to_string(&mut json_str)?;
    let res: Vec<String> = serde_json::from_str(&json_str)?;
    Ok(res)
}

#[allow(dead_code)]
pub struct Restbl<'a> {
    pub path: Pathlib,
    pub zstd: Arc<TotkZstd<'a>>,
    // buffer: Arc<Vec<u8>>, // Use Arc to share ownership
    pub reader: ResTblReader<'a>,
    pub table: ResourceSizeTable,
    pub hash_table: Vec<String>,
}

impl<'a> Restbl<'_> {

    pub fn open_restbl<P: AsRef<Path>>(path: P, zstd: Arc<TotkZstd>) -> Option<(OpenedFile, SendData)> {
        let mut opened_file = OpenedFile::default();
        let path_ref = path.as_ref();
        let mut data = SendData::default();
        print!("Is {} a restbl? ", &path_ref.display());
        let pathlib_var = Pathlib::new(path_ref);
        if pathlib_var
            .name
            .to_lowercase()
            .starts_with("resourcesizetable.product")
        {
            println!(" yes!");
            opened_file.restbl = Restbl::from_path(path_ref, zstd.clone());
            if let Some(_restbl) = &mut opened_file.restbl {
                data.tab = "RSTB".to_string();
                opened_file.path = pathlib_var.clone();
                opened_file.endian = Some(roead::Endian::Little);
                opened_file.file_type = TotkFileType::Restbl;
                data.status_text = format!("Opened {}", &pathlib_var.full_path);
                data.path = pathlib_var;
                // data.text = restbl.to_text();
                data.get_file_label(TotkFileType::Restbl, Some(roead::Endian::Little));
                return Some((opened_file, data));
            }
        }
        println!(" no");
        None
    }

    pub fn get_restb_entries<P: AsRef<Path>>(&mut self, path: P) -> io::Result<Vec<String>> {
        //read from zlib json
        let json_zlibdata = fs::read("bin/totk_rstb_paths.bin")?;
        let mut decoder = ZlibDecoder::new(&json_zlibdata[..]);
        let mut json_str = String::new();
        decoder.read_to_string(&mut json_str)?;
        let mut res: Vec<String> = serde_json::from_str(&json_str)?;
        let mut p = PathBuf::from(path.as_ref());
        for _ in 0..3 {
            if !p.pop() {
                return Ok(res); //unable to go to mod romfs path
            }
        }
        let mod_romfs_path = p.to_string_lossy().to_string().replace("\\", "/");
        //No point in updating from romfs dump
        if mod_romfs_path == self.zstd.totk_config.romfs {return Ok(res);}
        let mod_romfs_path_len = mod_romfs_path.len();
        //limit to map files and actors
        let valid_paths = vec!["Pack/Actor",  "AI", "AS"];
        for entry in valid_paths.iter() {
            //no point in calling res.contains() since the check costs more time than
            //adding redundant local path
            let valid_path = PathBuf::from(&mod_romfs_path).join(entry);
            if !valid_path.exists() {continue;}
            for file in list_files_recursively(&valid_path) {
                // println!("  {}", &file);
                let mut local_path = file.replace("\\","/")[mod_romfs_path_len..].to_string();
                if local_path.starts_with("/") {local_path = local_path[1..].to_string()}
                let local_path_lower = local_path.to_ascii_lowercase();
                if local_path.to_ascii_lowercase().ends_with(".zs") {local_path = local_path[..(local_path.len()-3)].to_string()}
                // if !res.contains(&local_path) {
                    // println!("Adding custom rstb path: {}", &local_path);
                // }
                if entry == &"Pack/Actor" && (local_path_lower.ends_with(".pack") || local_path_lower.ends_with(".sarc")) {
                    if let Ok(pack) = PackFile::new(&file, self.zstd.clone()) {
                        for entry in pack.sarc.files() {
                            let entry_path = entry.name.unwrap_or_default().to_string();
                            // if !entry_path.is_empty() && !res.contains(&entry_path) {
                            if !entry_path.is_empty()  {
                                // println!("Adding custom rstb sarc path: {}", &entry_path);
                                res.push(entry_path);
                            }
                        }
                    }
                } else {
                    
                    res.push(local_path);
                }
                
            }
        }

        Ok(res)
    }

    pub fn from_path<P: AsRef<Path>>(path: P, zstd: Arc<TotkZstd<'_>>) -> Option<Restbl> {
        let mut f_handle = File::open(&path).ok()?;
        let mut buffer = Vec::new();
        f_handle.read_to_end(&mut buffer).ok()?;
        if !is_restbl(&buffer) {
            buffer = zstd.decompress_zs(&buffer).ok()?;
        }
        if !is_restbl(&buffer) {
            return None; //invalid rstb
        }

        match ResTblReader::new(buffer) {
            Ok(r) => {
                let t: ResourceSizeTable = ResourceSizeTable::from_parser(&r);
                // let hash_table = get_rstb_data().unwrap_or_default();
                
                let mut new_restbl = Restbl {
                    path: Pathlib::new(&path),
                    zstd: zstd.clone(),
                    reader: r,
                    table: t,
                    hash_table: Default::default(),
                };
                //TODO: check if self function works
                new_restbl.hash_table = new_restbl.get_restb_entries(&path).unwrap_or_default();
                return Some(new_restbl);
            }
            Err(err) => {
                eprintln!("{:?}", err);
            }
        }
        // return Err(io::Error::new(io::ErrorKind::InvalidData, ""));
        None
    }

    pub fn save_default(&mut self) -> io::Result<()> {
        self.save(&self.path.full_path.clone())
    }

    pub fn save(&mut self, path: &str) -> io::Result<()> {
        let mut buffer = self.table.to_binary();
        let mut f = File::create(&path)?;
        if path.to_lowercase().ends_with(".zs") {
            // buffer = self.zstd.compressor.compress_empty(&buffer)?;
            buffer = self.zstd.compress_empty(&buffer)?;
        }
        f.write_all(&buffer)?;
        Ok(())
    }

}
