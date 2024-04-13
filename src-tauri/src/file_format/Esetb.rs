use std::{io, ops::Deref, sync::Arc};
use roead::byml::{self, Byml};
use tauri::api::file;

use crate::{Settings::Pathlib, Zstd::{TotkFileType, TotkZstd}};
use super::BinTextFile::{BymlFile, FileData};



pub struct Esetb<'a> {
    pub byml: BymlFile<'a>,
    pub ptcl: Vec<u8>,
}


impl<'a> Esetb<'a> {
    pub fn from_binary(data: &Vec<u8>, zstd: Arc<TotkZstd<'a>>) -> io::Result<Esetb<'a>> {
        let file_data = FileData {file_type: TotkFileType::Esetb, data: data.to_vec()};
        let pio = Byml::from_binary(data).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let byml = BymlFile {
            endian: BymlFile::get_endiannes(&file_data.data),
            file_data: file_data,
            path: Pathlib::default(),
            pio: pio,
            zstd: zstd.clone(),
            file_type: TotkFileType::Byml,
        };
        let ptcl = Self::get_ptcl_binary(&byml.pio)?;
        Ok(Esetb { byml: byml, ptcl: ptcl })
    }

    pub fn to_binary(&mut self) -> Vec<u8> {
        self.byml.pio.to_binary(roead::Endian::Little)
    }

    pub fn get_ptcl_binary(pio: &Byml) -> io::Result<Vec<u8>> {
        let endian = roead::Endian::Little;
        let pio_map = pio.as_map().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        if !pio_map.contains_key("PtclBin")  {
            return Err(io::Error::new(io::ErrorKind::Other, "BYML file does not contain PtclBin key"));
        }
        let ptcl_bin =  pio_map.get("PtclBin").ok_or(io::Error::new(io::ErrorKind::Other, "Error while reading PtclBin key"))?;
        // let x = ptcl_bin.to_binary(roead::Endian::Little);
        // let ptcl = ptcl_bin.as_binary_data().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("not binary data {}",e)))?; // TODO: Check if this is correct (as_binary_data() returns Result<Vec<u8>, String>)
        let mut q = byml::Map::default();
        q.insert("PtclBin".into(), ptcl_bin.clone());

        
        let x = byml::Byml::Map(q);
        let ptcl = x.to_binary(endian);
        Ok(ptcl)
    }

    pub fn from_file(file: String, zstd: Arc<TotkZstd<'a>>) -> io::Result<Esetb<'a>> {
        if let Some(byml) = BymlFile::new(file.to_string(), zstd.clone()) {
            let mut esetb = Esetb { byml: byml, ptcl: Vec::new() };
            esetb.ptcl = Self::get_ptcl_binary(&esetb.byml.pio)?;
            
            esetb.remove_ptclbin_entry()?;
            
            return Ok(esetb);
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "Error while reading BYML file"))
        }
        // let mut byml = BymlFile::new(file.to_string(), zstd.clone()).ok_or(io::Error::new(io::ErrorKind::Other, "Error while reading BYML file"))?;
    }
    pub fn remove_ptclbin_entry(&mut self) -> io::Result<()> {
        if let Ok(pio_map) = self.byml.pio.as_mut_map() {
            if pio_map.contains_key("PtclBin") {
                pio_map.remove("PtclBin");
            }
        }
        Ok(())
    }

    pub fn to_text(&self) -> String {
        self.byml.pio.to_text()
    }

    pub fn update_from_text(&mut self, text: &str) -> io::Result<()> {
        self.byml.pio = Byml::from_text(text).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        self.remove_ptclbin_entry()?;
        if let Ok(pio_map) = self.byml.pio.as_mut_map() {
            let local_pio = Byml::from_binary(&self.ptcl).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            if let Ok(local_pio_map) = local_pio.as_map() {
                if let Some(ptcl_bin) = local_pio_map.get("PtclBin") {
                    pio_map.insert("PtclBin".into(), ptcl_bin.clone());
                }
            }
            // pio_map.insert("PtclBin".into(), Byml::FileData(self.ptcl.clone()));
        }


        Ok(())
    }

    pub fn text_to_binary(&mut self, text: &str) -> io::Result<Vec<u8>> {
        self.update_from_text(text)?;
        Ok(self.byml.pio.to_binary(self.byml.endian.unwrap_or(roead::Endian::Little)))
    }

    pub fn save_from_text(&mut self, path: &str, text: &str) -> io::Result<()> {
        self.update_from_text(text)?;
        self.byml.save(path.to_string())?;

        Ok(())
    }

}