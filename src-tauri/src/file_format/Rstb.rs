#![allow(non_snake_case, non_camel_case_types)]
use std::fs::File;
use std::io::{self, Read, Write};
use std::sync::Arc;

use crate::Settings::Pathlib;
use crate::Zstd::{is_restbl, TotkZstd};
use restbl::bin::ResTblReader;
use restbl::ResourceSizeTable;

use super::RstbData::get_rstb_data;

pub struct Restbl<'a> {
    pub path: Pathlib,
    pub zstd: Arc<TotkZstd<'a>>,
    // buffer: Arc<Vec<u8>>, // Use Arc to share ownership
    pub reader: ResTblReader<'a>,
    pub table: ResourceSizeTable,
    pub hash_table: Vec<String>,
}

impl<'a> Restbl<'_> {
    pub fn from_path(path: String, zstd: Arc<TotkZstd<'_>>) -> Option<Restbl> {
        let mut f_handle = File::open(&path).ok()?;
        let mut buffer = Vec::new();
        f_handle.read_to_end(&mut buffer).ok()?;
        if !is_restbl(&buffer) {
            buffer = zstd.decompressor.decompress_zs(&buffer).ok()?;
        }
        if !is_restbl(&buffer) {
            return None; //invalid rstb
        }

        match ResTblReader::new(buffer) {
            Ok(r) => {
                let t = ResourceSizeTable::from_parser(&r);
                return Some(Restbl {
                    path: Pathlib::new(path),
                    zstd: zstd.clone(),
                    reader: r,
                    table: t,
                    hash_table: get_rstb_data(),
                });
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
            buffer = self.zstd.cpp_compressor.compress_empty(&buffer)?;
        }
        f.write_all(&buffer)?;
        Ok(())
    }

    pub fn to_text(&self) -> String {
        return self.table.to_text();
    }
}
