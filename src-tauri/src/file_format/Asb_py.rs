#![allow(non_snake_case,non_camel_case_types)]
use std::path::Path;
use std::sync::Arc;
use std::{
    io::{self, Read, Write}, os::windows::process::CommandExt, process::{Command, Stdio}
};
use crate::Settings::NO_WINDOW_FLAG;
use crate::Zstd::{is_asb, TotkZstd};
use super::BinTextFile::write_string_to_file;



pub struct Asb_py<'a>  {
    pub zstd: Arc<TotkZstd<'a>>,
    pub python_exe: String,
    pub python_script: String,
    pub create_no_window: u32,
    pub data: Vec<u8>,
}

#[allow(dead_code,unused_variables)]
impl<'a> Asb_py<'a> {
    pub fn new(zstd: Arc<TotkZstd<'a>>) -> Asb_py<'a> {
        Self {
            zstd: zstd.clone(),
            python_exe: "bin/winpython/python-3.11.8.amd64/python.exe".to_string(),
            python_script: "totkbits.py".to_string(),
            create_no_window: NO_WINDOW_FLAG,
            data: Vec::new(),
        }
    }
    pub fn from_binary_file<P: AsRef<Path>>( file_path: P ,zstd: Arc<TotkZstd<'a>>) -> io::Result<Asb_py<'a>> {
        let mut f_handle = std::fs::File::open(file_path)?; // Open the file
        let mut buffer = Vec::new(); // Create a buffer to store the data
        f_handle.read_to_end(&mut buffer)?; // Read the file into the buffer
        Self::from_binary(&buffer, zstd.clone())

    }
    pub fn from_binary( data: &Vec<u8>,zstd: Arc<TotkZstd<'a>>) -> io::Result<Asb_py<'a>> {
        let new_data = if !is_asb(data) {
            zstd.decompress_zs(data)?
        } else {
            data.to_vec()
        };
        if !is_asb(&new_data) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Data is not an ASB file.",
            ));
        }
        Ok(
            Self {
                zstd: zstd.clone(),
                python_exe: "bin/winpython/python-3.11.8.amd64/python.exe".to_string(),
                python_script: "totkbits.py".to_string(),
                create_no_window: 0x08000000,
                data: new_data,
            }
        )
    }

    pub fn binary_file_to_text(&mut self, file_path: &str) -> io::Result<String> {
        // env::set_var("PATH", self.newpath.clone());
        let mut f_handle = std::fs::File::open(file_path)?; // Open the file
        let mut buffer = Vec::new(); // Create a buffer to store the data
        f_handle.read_to_end(&mut buffer)?; // Read the file into the buffer
        if !is_asb( &buffer) {
            buffer = self.zstd.decompress_zs(&buffer)?;
        }
        if !is_asb( &buffer) {
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

    
    pub fn text_file_to_binary(&self, file_path: &str) -> io::Result<Vec<u8>> {
        // env::set_var("PATH", self.newpath.clone());
        let mut f_handle = std::fs::File::open(file_path)?; // Open the file
        let mut buffer = Vec::new(); // Create a buffer to store the data
        f_handle.read_to_end(&mut buffer)?; // Read the file into the buffer
        let text = String::from_utf8_lossy(&buffer).into_owned();
        
        self.text_to_binary(&text)
    }

    pub fn text_to_binary_file(&self, text: &str, file_path: &str) -> io::Result<()> {
        let mut data = self.text_to_binary(&text)?;
        if file_path.to_lowercase().ends_with(".zs") {
            // data = self.zstd.compressor.compress_zs(&data)?;
            data = self.zstd.compress_zs(&data)?;
        }
        let mut f_handle = std::fs::File::create(file_path)?;
        f_handle.write_all(&data)?;
        Ok(())
    }

    pub fn binary_to_text(&self) -> io::Result<String> {

        let mut child = Command::new(&self.python_exe)
            .creation_flags(self.create_no_window)
            .arg(&self.python_script)
            .arg("asb_binary_to_text")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(ref mut stdin) = child.stdin.take() {
            stdin.write_all(&self.data)?;
        } // Dropping `stdin` here closes the pipe.

        let output = child.wait_with_output()?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        // write_string_to_file("stderr.log", &stderr)?;
        // write_string_to_file("stdout.log", &stdout)?;
        if stdout.to_lowercase().starts_with("error") {
            return Err(io::Error::new(io::ErrorKind::Other, stdout));
        }
        if stderr.to_lowercase().starts_with("error") {
            return Err(io::Error::new(io::ErrorKind::Other, stderr));
        }

        if output.status.success() {
            // println!("Script executed successfully.");
            eprintln!("Script execution successfully. {:#?}\n{}", output.status, String::from_utf8_lossy(&output.stderr).into_owned());
        } else {
            eprintln!("Script execution failed. {:#?}\n{}", output.status, &stderr);
            // eprintln!("Data: {:?}", &stdout);
            let e = format!("Script execution failed. Unable to convert asb binary to text. \n{:#?}\n{}", output.status, &stderr);
            return Err(io::Error::new(
                io::ErrorKind::Other,
                e,
            ));
        }
        Ok(stdout)
    }

    pub fn text_to_binary(&self, text: &str) -> io::Result<Vec<u8>> {
        let mut child = Command::new(&self.python_exe)
            .creation_flags(self.create_no_window)
            .arg(&self.python_script)
            .arg("asb_text_to_binary")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(ref mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        } // Dropping `stdin` here closes the pipe.

        let output = child.wait_with_output()?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        // write_string_to_file("stderr.log", &stderr)?;
        
        if output.stdout.starts_with(b"Error") {
            return Err(io::Error::new(io::ErrorKind::Other, String::from_utf8_lossy(&output.stdout).into_owned()));
        }
        if stderr.to_lowercase().starts_with("error") {
            return Err(io::Error::new(io::ErrorKind::Other, stderr));
        }


        // write_string_to_file("stdout.log", &stdout)?;
        if output.status.success() {
            println!("Script executed successfully.");
        } else {
            eprintln!("Script execution failed.");
            let e = format!("Script execution failed. Unable to convert asb text to binary. \n{:#?}\n{}", output.status, &stderr);
            return Err(io::Error::new(
                io::ErrorKind::Other,
                e,
            ));
        }
        Ok(output.stdout)
    }


    pub fn test_winpython(&self) -> io::Result<()> {
        // env::set_var("PATH", self.newpath.clone());
        let output = Command::new(&self.python_exe)
            .arg(&self.python_script)
            .creation_flags(self.create_no_window)
            // .arg("-V")
            .output()?;
        if output.status.success() {
            println!("Script executed successfully.");
        } else {
            eprintln!("Script execution failed. {:#?}\n{}", output.status, String::from_utf8_lossy(&output.stderr).into_owned());
        }
        let text = String::from_utf8_lossy(&output.stdout);
        // env::set_var("PATH", self.original_path.clone());
        println!("Test response from winpython: {}", text);
        Ok(())
    }
    
}