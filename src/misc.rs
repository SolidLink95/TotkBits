
use std::{env};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::{self, Error, ErrorKind, Read, Write};
extern crate nfd;
use nfd::Response;
//use std::io;

pub fn get_example_yaml() -> String{
    let mut r = String::new();
    for i in 1..10000 {
        r += &format!("{:?}\n", i);
    }
    return r;
}

pub fn get_other_yaml() -> String{
    let mut r = String::new();
    for i in 1..10000 {
        r += &format!("{:?}\n", -i);
    }
    return r; 
}

pub fn save_file_dialog() -> String {
    match nfd::open_save_dialog(None, None) {
        Ok(response) => {
            match response {
                Response::Okay(file_path) => {
                    return file_path;
                },
                Response::Cancel => {return "".to_string();},
                _ => {return "".to_string();}
            }
        },
        _ => {return "".to_string();}
    }
}

pub fn open_file_dialog() -> String {
    match nfd::open_file_dialog(None, None) {
        Ok(response) => {
            match response {
                Response::Okay(file_path) => {
                    // `file_path` contains the selected file's path as a `PathBuf`
                    println!("Selected file: {:?}", file_path);
                    return file_path;
                }
                Response::Cancel => {
                    // The user canceled the file selection
                    println!("File selection canceled");
                    return "".to_string();
                }
                _ => {
                    // Some other error occurred
                    //println!("An error occurred");
                    return "".to_string();
                }
            }
        },
        _ => {return "".to_string();}
    }
}



pub fn create_directory(directory: &str) -> io::Result<()> {
    let p = Path::new(directory);
    fs::create_dir_all(p)?;
    Ok(())
}

pub fn save_bytes_to_file(file_path: &str, data: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create(file_path)?;
    file.write_all(data)?;
    Ok(())
}

pub fn check_file_exists(path: &PathBuf) -> std::io::Result<()> {
    match fs::metadata(&path) {
        Ok(_) => Ok(()),
        Err(_) => Err(Error::new(ErrorKind::NotFound, "File does not exist")),
    }
}


fn print_bytes() -> io::Result<()> {
    //let mut file = File::open("res/asdf.txt");
    let mut file =  fs::File::open("res/asdf.txt")?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    println!("{:?}", buffer);
    print_as_hex(&buffer);
    Ok(())
}

fn get_cwd() -> String {
    match env::current_dir() {
        Ok(path) => path_to_string(path),
        Err(_) => String::new(),
    }
}


fn path_to_string(path: PathBuf) -> String {
    path.into_os_string().into_string().unwrap_or_else(|_| String::new())
}

fn print_cwd() {
    let cwd = get_cwd();
    println!("CWD: {}", cwd);
}

pub fn print_as_hex(buffer : &[u8]) {
    for &byte in &buffer[..buffer.len().min(5)] {
        print!("\\x{:02x}", byte)
    }
    println!();
}