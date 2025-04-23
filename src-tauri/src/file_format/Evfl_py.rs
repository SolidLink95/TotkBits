#![allow(non_snake_case,non_camel_case_types)]
use std::path::Path;
use std::sync::Arc;
use std::io::{self, Read, Write};
use crate::Zstd::{is_evfl, TotkZstd};
use super::Wrapper::PythonWrapper;



pub struct Evfl_py<'a>  {
    pub zstd: Arc<TotkZstd<'a>>,   
    pub py_wrapper: PythonWrapper,
    pub data: Vec<u8>,
}

#[allow(dead_code,unused_variables)]
impl<'a> Evfl_py<'a> {
    pub fn new(zstd: Arc<TotkZstd<'a>>) -> Evfl_py<'a> {
        Self {
            zstd: zstd.clone(),
            py_wrapper: PythonWrapper::default(),
            data: Vec::new(),
        }
    }
    pub fn from_binary_file<P: AsRef<Path>>( file_path: P ,zstd: Arc<TotkZstd<'a>>) -> io::Result<Evfl_py<'a>> {
        let mut f_handle = std::fs::File::open(file_path)?; // Open the file
        let mut buffer = Vec::new(); // Create a buffer to store the data
        f_handle.read_to_end(&mut buffer)?; // Read the file into the buffer
        Self::from_binary(&buffer, zstd.clone())

    }
    pub fn from_binary( data: &Vec<u8>,zstd: Arc<TotkZstd<'a>>) -> io::Result<Evfl_py<'a>> {
        let new_data = if !is_evfl(data) {
            zstd.decompress_zs(data)?
        } else {
            data.to_vec()
        };
        if !is_evfl(&new_data) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Data is not an ASB file.",
            ));
        }
        Ok(
            Self {
                zstd: zstd.clone(),
                py_wrapper: PythonWrapper::default(),
                data: new_data,
            }
        )
    }

    pub fn binary_file_to_text<P: AsRef<Path>>(&mut self, file_path: P) -> io::Result<String> {
        // env::set_var("PATH", self.newpath.clone());
        let path_ref = file_path.as_ref();
        let mut f_handle = std::fs::File::open(path_ref)?; // Open the file
        let mut buffer = Vec::new(); // Create a buffer to store the data
        f_handle.read_to_end(&mut buffer)?; // Read the file into the buffer
        if !is_evfl( &buffer) {
            buffer = self.zstd.decompress_zs(&buffer)?;
        }
        if !is_evfl( &buffer) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "File is not an ASB file.",
            ));
        }
        self.data = buffer;
        let text = self.binary_to_text()?;
        // env::set_var("PATH", self.original_path.clone());
        Ok(text)
    }

    
    pub fn text_file_to_binary<P: AsRef<Path>>(&self, file_path: P) -> io::Result<Vec<u8>> {
        // env::set_var("PATH", self.newpath.clone());
        let path_ref = file_path.as_ref();
        let mut f_handle = std::fs::File::open(path_ref)?; // Open the file
        let mut buffer = Vec::new(); // Create a buffer to store the data
        f_handle.read_to_end(&mut buffer)?; // Read the file into the buffer
        let text = String::from_utf8_lossy(&buffer).into_owned();
        
        self.text_to_binary(&text)
    }

    pub fn text_to_binary_file<P: AsRef<Path>>(&self, text: &str, file_path: P) -> io::Result<()> {
        let path_ref = file_path.as_ref();
        let mut data = self.text_to_binary(&text)?;
        if path_ref.to_string_lossy().to_lowercase().ends_with(".zs") {
            // data = self.zstd.compressor.compress_zs(&data)?;
            data = self.zstd.compress_zs(&data)?;
        }
        let mut f_handle = std::fs::File::create(file_path)?;
        f_handle.write_all(&data)?;
        Ok(())
    }

    pub fn binary_to_text(&self) -> io::Result<String> {
        return self.py_wrapper.binary_to_string(&self.data, "evfl_binary_to_text".to_string());
       
    }

    pub fn text_to_binary(&self, text: &str) -> io::Result<Vec<u8>> {
        return self.py_wrapper.text_to_binary(&self.data, "evfl_text_to_binary".to_string());
    }


    
}