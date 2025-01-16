use std::{io::{self, Read}, path::Path, sync::Arc};

use crate::{Open_and_Save::SendData, Settings::Pathlib, Zstd::{is_evfl, is_little_endian, TotkFileType, TotkZstd}};

use super::{BinTextFile::OpenedFile, Wrapper::ExeWrapper};




pub struct Evfl<'a> {
    pub zstd: Arc<TotkZstd<'a>>,
    pub wrapper: ExeWrapper,
    pub data: Vec<u8>,
}

impl<'a> Evfl<'a> {
    pub fn new(zstd: Arc<TotkZstd<'a>>) -> Evfl<'a> {
        Self {
            zstd: zstd.clone(),
            wrapper: ExeWrapper::dotnet_new(),
            data: Vec::new(),
        }
    }
    pub fn binary_to_string(&self, data: &Vec<u8>) -> io::Result<String> {
        let new_data = if !is_evfl(data) {
            self.zstd.decompressor.decompress_zs(data)?
        } else {
            data.to_vec()
        };
        // println!("Data is evfl: {} LE: {}", is_evfl(&new_data), is_little_endian(&new_data));
        if !is_evfl(&new_data) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Data is not an Evfl file.",
            ));
        }
        self.wrapper.binary_to_string(&new_data, "EvflBinaryToText".to_string())
    }
    pub fn string_to_binary(&self, text_data: &str) -> io::Result<Vec<u8>> {
        self.wrapper.string_to_binary(text_data, "EvflTextToBinary".to_string())
    }

    pub fn binary_file_to_string<P: AsRef<std::path::Path>>(&self,
        file_path: P,
    ) -> io::Result<String> {
        let mut f_handle = std::fs::File::open(file_path)?; // Open the file
        let mut buffer = Vec::new(); // Create a buffer to store the data
        f_handle.read_to_end(&mut buffer)?; // Read the file into the buffer
        if let Err(e) = self.binary_to_string(&buffer) {
            println!("Error while converting EvflBin to text: {}", e);
            return Err(e);
        }
        self.binary_to_string(&buffer)
    }

    pub fn open_file<P:AsRef<Path>>(path: P, zstd: Arc<TotkZstd>) -> Option<(OpenedFile, SendData)> {
        let path_ref = path.as_ref();
        print!("Is {:?} a evfl? ", &path_ref);
        let evfl = Evfl::new(zstd.clone());
        if let Ok(text) = evfl.binary_file_to_string( path_ref) {
            let mut opened_file = OpenedFile::default();
            let mut data = SendData::default();
            let pathlib_var = Pathlib::new(path_ref);
            data.lang = "json".to_string();
            println!(" yes!");
            data.tab = "YAML".to_string();
            opened_file.path = pathlib_var.clone();
            opened_file.endian = Some(roead::Endian::Little);
            opened_file.file_type = TotkFileType::Evfl;
            data.status_text = format!("Opened {}", path_ref.display());
            data.path = pathlib_var;
            data.text = text;  
            data.get_file_label(TotkFileType::Evfl, Some(roead::Endian::Little));
            return Some((opened_file, data));
        }
        println!("no");
    
        None
    }
}