use std::fs::File;
use std::io::{self, Read};
use std::sync::Arc;

use crate::Settings::Pathlib;
use crate::Zstd::{is_restbl, TotkZstd};
use restbl::bin::ResTblReader;
use restbl::ResourceSizeTable;

pub struct Restbl<'a> {
    pub path: Pathlib,
    pub zstd: Arc<TotkZstd<'a>>,
    // buffer: Arc<Vec<u8>>, // Use Arc to share ownership
    pub reader: ResTblReader<'a>,
    pub table: ResourceSizeTable,
}

impl<'a> Restbl<'_> {
    pub fn from_path(path: String, zstd: Arc<TotkZstd<'_>>) -> Option<Restbl> {
        let mut f_handle = File::open(&path).ok()?;
        let mut buffer = Vec::new();
        f_handle.read_to_end(&mut buffer).ok()?;
        if !is_restbl(&buffer) {
            buffer = zstd.decompressor.decompress_zs(&buffer).ok()?;
        }
        // if let Ok(r) = ResTblReader::new(buffer) {
        //     let t = ResourceSizeTable::from_parser(&r);
        //     // if let Ok(t) = ResourceSizeTable::from_parser(&r) {
        //         return Some(Restbl {
        //             path: Pathlib::new(path),
        //             zstd: zstd.clone(),
        //             reader: r,
        //             table: t,
        //         });
        //     // }
        // }

        match ResTblReader::new(buffer) {
            Ok(r) => {
                let t = ResourceSizeTable::from_parser(&r);
                        return Some(Restbl {
                            path: Pathlib::new(path),
                            zstd: zstd.clone(),
                            reader: r,
                            table: t,
                        });
            }
            Err(err) => {
                eprintln!("{:?}", err);
            }
        }
        // return Err(io::Error::new(io::ErrorKind::InvalidData, ""));
        None
    }

    pub fn to_text(&self) -> String {
        return self.table.to_text();
    }
}
