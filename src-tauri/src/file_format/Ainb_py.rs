#![allow(non_snake_case,non_camel_case_types)]
use std::{
    io::{self, Read, Write}, os::windows::process::CommandExt, path::Path, process::{Command, Stdio}
};


use crate::Zstd::is_ainb;

pub struct Ainb_py {
    pub python_exe: String,
    pub python_script: String,
    pub create_no_window: u32,
}

impl Default for Ainb_py {
    fn default() -> Self {
        Self {
            python_exe: "bin/winpython/python-3.11.8.amd64/python.exe".to_string(),
            python_script: "src/totkbits.py".to_string(),
            create_no_window: 0x08000000,
        }
    }
}

#[allow(dead_code)]
impl Ainb_py {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn binary_file_to_text<P:AsRef<Path>>(&self, file_path: P) -> io::Result<String> {
        // env::set_var("PATH", self.newpath.clone());
        let mut f_handle = std::fs::File::open(file_path)?; // Open the file
        let mut buffer = Vec::new(); // Create a buffer to store the data
        f_handle.read_to_end(&mut buffer)?; // Read the file into the buffer
        if !is_ainb( &buffer) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "File is not an Ainb file.",
            ));
        }
        let text = self.binary_to_text(&buffer)?;
        // env::set_var("PATH", self.original_path.clone());
        Ok(text)
    }

    
    pub fn text_file_to_binary(&self, file_path: &str) -> io::Result<Vec<u8>> {
        // env::set_var("PATH", self.newpath.clone());
        let mut f_handle = std::fs::File::open(file_path)?; // Open the file
        let mut buffer = Vec::new(); // Create a buffer to store the data
        f_handle.read_to_end(&mut buffer)?; // Read the file into the buffer
        let text = String::from_utf8_lossy(&buffer).into_owned();
        let data = self.text_to_binary(&text)?;
        // env::set_var("PATH", self.original_path.clone());
        Ok(data)
    }

    pub fn binary_to_text(&self, data: &Vec<u8>) -> io::Result<String> {
        // env::set_var("PATH", self.newpath.clone());
        let mut child = Command::new(&self.python_exe)
            // .current_dir(&self.current_dir)
            .creation_flags(self.create_no_window)
            .arg(&self.python_script)
            .arg("ainb_binary_to_text")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(ref mut stdin) = child.stdin.take() {
            stdin.write_all(data)?;
            // For binary data, ensure you're handling errors and using `write_all` to guarantee all data is written.
        } // Dropping `stdin` here closes the pipe.

        let output = child.wait_with_output()?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if stdout.to_lowercase().starts_with("error") {
            return Err(io::Error::new(io::ErrorKind::Other, stdout));
        }
        if stderr.to_lowercase().starts_with("error") {
            return Err(io::Error::new(io::ErrorKind::Other, stderr));
        }

        if output.status.success() {
            println!("Script executed successfully.");
        } else {
            // eprintln!("Script execution failed.");
            eprintln!("Script execution failed. {:#?}\n{}", output.status, &stderr);
            eprintln!("Data: {:?}", &stdout);
            return Err(io::Error::new(io::ErrorKind::Other, "Script execution failed."));
        }
        Ok(stdout)
    }

    pub fn text_to_binary(&self, text: &str) -> io::Result<Vec<u8>> {
        let mut child = Command::new(&self.python_exe)
            // .current_dir(&self.current_dir)
            .creation_flags(self.create_no_window)
            .arg(&self.python_script)
            .arg("ainb_text_to_binary")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(ref mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        } // Dropping `stdin` here closes the pipe.

        let output = child.wait_with_output()?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if output.stdout.starts_with(b"Error") {
            return Err(io::Error::new(io::ErrorKind::Other, String::from_utf8_lossy(&output.stdout).into_owned()));
        }
        if stderr.to_lowercase().starts_with("error") {
            return Err(io::Error::new(io::ErrorKind::Other, stderr));
        }

        if output.status.success() {
            println!("Script executed successfully.");
        } else {
            eprintln!("Script execution failed.");
            let e = format!("Script execution failed. Unable to convert ainb text to binary. \n{:#?}\n{}", output.status, &stderr);
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
