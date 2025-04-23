use std::io::{self, Write};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use crate::Settings::NO_WINDOW_FLAG;

pub struct ExeWrapper {
    pub exe: String,
    pub args: Vec<String>,
}

impl ExeWrapper {
    pub fn new(exe: String, args: Vec<String>) -> Self {
        Self { exe, args }
    }
    pub fn dotnet_new() -> Self {
        // let exe = PathBuf::from(get_cwd_dir().unwrap_or_default()).join("bin/DotNetWrapper.exe").to_string_lossy().to_string();

        Self {
            exe: "bin/cs/DotNetWrapper.exe".to_string(),
            args: vec![],
        }
    }

    pub fn binary_to_string(&self, data: &Vec<u8>, fname: String) -> io::Result<String> {
        // if Path::new(&self.exe).exists() == false {
        //     println!("ExeWrapper: {} does not exist.", &self.exe);
        // } else {
        //     println!("ExeWrapper: {} exists.", &self.exe);
        // }
        let mut child = Command::new(&self.exe)
            .creation_flags(NO_WINDOW_FLAG)
            .args(&self.args)
            .arg(&fname)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(ref mut stdin) = child.stdin.take() {
            stdin.write_all(data)?;
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
            // println!("Script executed successfully: {}.", &fname);
        } else {
            eprintln!("Script execution failed.");
            println!("{}", &stderr);
            return Err(io::Error::new(io::ErrorKind::Other, "Script execution failed."));
        }
        Ok(stdout)
    }

    pub fn string_to_binary(&self, text_data: &str, fname: String) -> io::Result<Vec<u8>> {
        let mut child = Command::new(&self.exe)
            .creation_flags(NO_WINDOW_FLAG)
            .args(&self.args)
            .arg(&fname)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(ref mut stdin) = child.stdin.take() {
            stdin.write_all(text_data.as_bytes())?;
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
            // println!("Script executed successfully.");
        } else {
            eprintln!("Script execution failed.");
            return Err(io::Error::new(io::ErrorKind::Other, "Script execution failed."));
        }
        Ok(output.stdout)
    }
}


pub struct PythonWrapper {
    pub python_exe: String,
    pub python_script: String,
    pub create_no_window: u32,
}

impl Default for PythonWrapper {
    fn default() -> Self {
        Self {
            python_exe: "bin/winpython/python-3.11.8.amd64/python.exe".to_string(),
            python_script: "totkbits.py".to_string(),
            create_no_window: NO_WINDOW_FLAG,
        }
    }
}

#[allow(dead_code)]
impl PythonWrapper {
    pub fn new() -> Self {
        Self::default()
    }
    

    pub fn binary_to_string(&self, data: &Vec<u8>, fname: String) -> io::Result<String> {
        // env::set_var("PATH", self.newpath.clone());
        let mut child = Command::new(&self.python_exe)
            // .current_dir(&self.current_dir)
            .creation_flags(self.create_no_window)
            .arg(&self.python_script)
            .arg(&fname)
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
            println!("Script executed successfully: {}.", &fname);
        } else {
            // eprintln!("Script execution failed.");
            eprintln!("Script execution failed. {:#?}\n", output.status);
            return Err(io::Error::new(io::ErrorKind::Other, "Script execution failed."));
        }
        Ok(stdout)
    }

    pub fn text_to_binary_mult_args(&self, args: &Vec<&Vec<u8>>, fname: String) -> io::Result<Vec<u8>> {
        // println!("Text to binary: spawning child process");
        let mut child = Command::new(&self.python_exe)
            // .current_dir(&self.current_dir)
            .creation_flags(self.create_no_window)
            .arg(&self.python_script)
            .arg(&fname)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // println!("Text to binary: writing to stdin");
        for arg in args {
            if let Some(ref mut stdin) = child.stdin.take() {
                stdin.write_all(&arg)?;
            } // Dropping `stdin` here closes the pipe.
        }

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
            let e = format!("Script execution failed. Unable to convert ainb text to binary. \n{:#?}\n", output.status);
            return Err(io::Error::new(
                io::ErrorKind::Other,
                e,
            ));
        }
        Ok(output.stdout)
    }


    pub fn text_to_binary(&self, text_data: &Vec<u8>, fname: String) -> io::Result<Vec<u8>> {
        // println!("Text to binary: spawning child process");
        let mut child = Command::new(&self.python_exe)
            // .current_dir(&self.current_dir)
            .creation_flags(self.create_no_window)
            .arg(&self.python_script)
            .arg(&fname)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // println!("Text to binary: writing to stdin");
        if let Some(ref mut stdin) = child.stdin.take() {
            stdin.write_all(text_data)?;
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
            let e = format!("Script execution failed. Unable to convert ainb text to binary. \n{:#?}\n", output.status);
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
            eprintln!("Script execution failed. {:#?}\n", output.status);
        }
        let text = String::from_utf8_lossy(&output.stdout);
        // env::set_var("PATH", self.original_path.clone());
        println!("Test response from winpython: {}", text);
        Ok(())
    }
}