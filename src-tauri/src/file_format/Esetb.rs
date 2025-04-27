#![allow(non_snake_case,non_camel_case_types)]
use std::{io, path::Path, sync::Arc};
use roead::byml::Byml;
use crate::{Open_and_Save::SendData, Settings::Pathlib, Zstd::{is_esetb_path, TotkFileType, TotkZstd}};
use super::{BinTextFile::{BymlFile, FileData, OpenedFile}, Wrapper::PythonWrapper};

const PTCL_JSON_KEY: &str = "PTCL_JSON";
const PTCL_BIN_KEY: &str = "PtclBin";


pub struct Esetb<'a> {
    pub byml: BymlFile<'a>,
    pub ptcl: Vec<u8>,
}


#[allow(dead_code)]
impl<'a> Esetb<'a> {
    pub fn from_binary(data: &Vec<u8>, zstd: Arc<TotkZstd<'a>>) -> io::Result<Esetb<'a>> {
        let file_data = FileData {file_type: TotkFileType::Esetb, data: data.to_vec()};
        let pio = Byml::from_binary(data).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let mut byml = BymlFile {
            endian: BymlFile::get_endiannes(&file_data.data),
            file_data: file_data,
            path: Pathlib::default(),
            pio: pio,
            zstd: zstd.clone(),
            file_type: TotkFileType::Byml,
        };
        let ptcl = Self::process_ptcl_binary(&mut byml.pio)?;
        Ok(Esetb { byml: byml, ptcl: ptcl })
    }

    pub fn to_binary(&mut self) -> Vec<u8> {
        self.byml.pio.to_binary(roead::Endian::Little)
    }

    pub fn process_ptcl_binary(pio: &mut Byml) -> io::Result<Vec<u8>> {
        // let endian = roead::Endian::Little;
        let mut result: Vec<u8> = Vec::new();
        let py_wrap = PythonWrapper::default();
        let pio_map = pio.as_mut_map().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        if !pio_map.contains_key(PTCL_BIN_KEY)  {
            return Err(io::Error::new(io::ErrorKind::Other, "BYML file does not contain PtclBin key"));
        }
        match pio_map[PTCL_BIN_KEY] {
            Byml::FileData(ref data) => {
                result = data.clone();
                match py_wrap.binary_to_string(&data, "ptcl_binary_to_text".to_string()) {
                    Ok(res) => {
                        pio_map.insert("PTCL_JSON".into(), Byml::from_text(&res).unwrap_or(Byml::Null));
                    },
                    Err(e) => println!("Error while converting PtclBin to text: {}", e),
                }
            },
            _ => {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "PtclBin key is not FileData"));
            },
        }
        let ptcl_bin: Byml =  pio_map.get(PTCL_BIN_KEY).ok_or(io::Error::new(io::ErrorKind::Other, "Error while reading PtclBin key"))?.clone();
        match ptcl_bin.clone() {
            Byml::FileData(data) => {
                match py_wrap.binary_to_string(&data, "ptcl_binary_to_text".to_string()) {
                    Ok(res) => {
                        pio_map.insert("PTCL_JSON".into(), Byml::from_text(&res).unwrap_or(Byml::Null));
                    },
                    Err(e) => println!("Error while converting PtclBin to text: {}", e),
                }
            }
            _ => {},
        }

        Ok(result)
    }

    pub fn from_file<P:AsRef<Path>>(file: P, zstd: Arc<TotkZstd<'a>>) -> io::Result<Esetb<'a>> {
        if let Some(byml) = BymlFile::new(file.as_ref(), zstd.clone()) {
            let mut esetb = Esetb { byml: byml, ptcl: Vec::new() };
            match Self::process_ptcl_binary(&mut esetb.byml.pio) {
                Ok(ptcl) => esetb.ptcl = ptcl,
                Err(e) => {
                    println!("Error while reading PtclBin key: {}", e);
                    return Err(e)
                },
            }
            // esetb.ptcl = Self::process_ptcl_binary(&esetb.byml.pio)?;
            
            esetb.remove_ptclbin_entry()?;
            
            return Ok(esetb);
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "Error while reading BYML file"))
        }
        // let mut byml = BymlFile::new(file.to_string(), zstd.clone()).ok_or(io::Error::new(io::ErrorKind::Other, "Error while reading BYML file"))?;
    }
    pub fn remove_ptclbin_entry(&mut self) -> io::Result<()> {
        if let Ok(pio_map) = self.byml.pio.as_mut_map() {
            if pio_map.contains_key(PTCL_BIN_KEY) {
                pio_map.remove(PTCL_BIN_KEY);
            }
        }
        Ok(())
    }

    pub fn to_string(&self) -> String {
        self.byml.pio.to_text()
    }

    pub fn update_from_text(&mut self, text: &str) -> io::Result<()> {
        let py_wrap = PythonWrapper::default();
        self.byml.pio = Byml::from_text(text).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        self.remove_ptclbin_entry()?;
        if let Ok(pio_map) = self.byml.pio.as_mut_map() {
            let mut is_ptcl_json_succ = false;
            if pio_map.contains_key(PTCL_JSON_KEY) {
                let ptcl_json = pio_map[PTCL_JSON_KEY].clone();
                let ptcl_json_str = Byml::to_text(&ptcl_json);
                let ptcl_json_bytes = ptcl_json_str.as_bytes().to_vec();
                let mut data_to_send = self.ptcl.clone();
                data_to_send.extend_from_slice(b"%PTCL_JSON%");
                data_to_send.extend_from_slice(&ptcl_json_bytes);
                match py_wrap.text_to_binary(&data_to_send, "ptcl_text_to_binary".to_string()) {
                    Ok(new_ptcl_data) => {
                        if new_ptcl_data.is_empty() {
                            println!("Error while converting Ptcl JSON to binary: Empty data");
                        }
                        pio_map.insert(PTCL_BIN_KEY.into(), Byml::FileData(new_ptcl_data));
                        is_ptcl_json_succ = true;
                    },
                    Err(e) => println!("Error while converting Ptcl JSON to binary: {}", e),
                }
               
                pio_map.remove(PTCL_JSON_KEY);
            } 
            if !is_ptcl_json_succ {
                println!("Ptcl JSON key not found or conversion failed. Using backup original PtclBin key");
                let new_node = Byml::FileData(self.ptcl.clone());
                pio_map.insert(PTCL_BIN_KEY.into(), new_node);

            }
            // let local_pio = Byml::from_binary(&self.ptcl).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            // if let Ok(local_pio_map) = local_pio.as_map() {
            //     if let Some(ptcl_bin) = local_pio_map.get(PTCL_BIN_KEY) {
            //         pio_map.insert(PTCL_BIN_KEY.into(), ptcl_bin.clone());
            //     }
            // }
            // pio_map.insert(PTCL_BIN_KEY.into(), Byml::FileData(self.ptcl.clone()));
        } else {
            return Err(io::Error::new(io::ErrorKind::Other, "Error while converting Ptcl byml as mut map"));
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

    pub fn open_esetb<P:AsRef<Path>>(path: P, zstd: Arc<TotkZstd>) -> Option<(OpenedFile, SendData)> {
        let mut opened_file = OpenedFile::default();
        let path_ref = path.as_ref();
        let mut data = SendData::default();
        print!("Is {:?} a esetb? ", &path_ref);
        if is_esetb_path(&path) {
            opened_file.esetb = Esetb::from_file(path_ref, zstd.clone()).ok();
            if let Some(esetb) = &opened_file.esetb {
                println!(" yes!");
                data.tab = "YAML".to_string();
                opened_file.path = Pathlib::new(path_ref);
                opened_file.endian = esetb.byml.endian;
                opened_file.file_type = TotkFileType::Esetb;
                data.status_text = format!("Opened {}", path_ref.display());
                data.path = Pathlib::new(path_ref);
                data.text = esetb.to_string();
                data.get_file_label(TotkFileType::Esetb, esetb.byml.endian);
                return Some((opened_file, data));
            }
        }
        println!("no");
    
        None
    }

}