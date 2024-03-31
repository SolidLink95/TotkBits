use std::{
    env, io::{self, Read, Write}, os::windows::process::CommandExt, process::{Command, Stdio}
};

use eframe::egui::text;

use crate::Zstd::is_ainb;

pub struct Ainb_py {
    pub python_exe: String,
    pub python_script: String,
    pub current_dir: String,
    pub CREATE_NO_WINDOW: u32,
    // pub newpath: String,
    // pub original_path: String,
}
//C:\Users\Luiza\AppData\Local\Programs\Python\Python37\Scripts\
//C:\Users\Luiza\AppData\Local\Programs\Python\Python37\

impl Default for Ainb_py {
    fn default() -> Self {
        Self {
            python_exe: "bin/winpython/python-3.11.8.amd64/python.exe".to_string(),
            python_script: "bin/ainb/ainb/ainb_rs.py".to_string(),
            current_dir: "bin/ainb/ainb".to_string(),
            CREATE_NO_WINDOW: 0x08000000,
            // newpath: "../bin/winpython/python-3.11.8.amd64:../bin/winpython/python-3.11.8.amd64/Scripts".to_string(),
            // original_path: env::var("PATH").unwrap_or("".to_string()),
        }
    }
}
impl Ainb_py {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn binary_file_to_text(&self, file_path: &str) -> io::Result<String> {
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
        if data.starts_with(b"Error") {
            return Err(io::Error::new(io::ErrorKind::Other, String::from_utf8_lossy(&data).into_owned()));
        }
        // env::set_var("PATH", self.original_path.clone());
        Ok(data)
    }

    pub fn binary_to_text(&self, data: &Vec<u8>) -> io::Result<String> {
        // env::set_var("PATH", self.newpath.clone());
        let mut child = Command::new(&self.python_exe)
            // .current_dir(&self.current_dir)
            .creation_flags(self.CREATE_NO_WINDOW)
            .arg(&self.python_script)
            .arg("binary_to_text")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        if let Some(ref mut stdin) = child.stdin.take() {
            stdin.write_all(data)?;
            // For binary data, ensure you're handling errors and using `write_all` to guarantee all data is written.
        } // Dropping `stdin` here closes the pipe.

        let output = child.wait_with_output()?;
        if output.status.success() {
            println!("Script executed successfully.");
        } else {
            eprintln!("Script execution failed.");
        }
        // env::set_var("PATH", self.original_path.clone());
        let text = String::from_utf8_lossy(&output.stdout);
        if text.starts_with("Error") {
            return Err(io::Error::new(io::ErrorKind::Other, text.into_owned()));
        }
        Ok(text.into_owned())
    }

    pub fn text_to_binary(&self, text: &str) -> io::Result<Vec<u8>> {
        let mut child = Command::new(&self.python_exe)
            // .current_dir(&self.current_dir)
            .creation_flags(self.CREATE_NO_WINDOW)
            .arg(&self.python_script)
            .arg("text_to_binary")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        if let Some(ref mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        } // Dropping `stdin` here closes the pipe.

        let output = child.wait_with_output()?;
        if output.status.success() {
            println!("Script executed successfully.");
            return Ok(output.stdout);
        } else {
            eprintln!("Script execution failed.");
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Script execution failed.",
            ));
        }
    }
    pub fn test_winpython(&self) -> io::Result<()> {
        // env::set_var("PATH", self.newpath.clone());
        let output = Command::new(&self.python_exe)
            .arg(&self.python_script)
            .creation_flags(self.CREATE_NO_WINDOW)
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
