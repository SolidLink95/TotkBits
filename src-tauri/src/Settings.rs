use std::fs;
use std::io;
use std::io::{BufWriter, Error, ErrorKind, Read, Write};
use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pathlib {
    pub parent: String,
    pub name: String,
    pub stem: String,
    pub extension: String,
    pub ext_last: String,
    pub full_path: String,
}

impl Default for Pathlib {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl Pathlib {
    pub fn new(path: String) -> Self {
        let _p = Path::new(&path);
        Self {
            parent: Pathlib::get_parent(&path),
            name: Pathlib::get_name(&path),
            stem: Pathlib::get_stem(&path),
            extension: Pathlib::get_extension(&path),
            ext_last: Self::get_ext_last(&path),
            full_path: path,
        }
    }

    pub fn get_ext_last(path: &str) -> String {
        let extension = Pathlib::get_extension(&path);
        if !extension.contains(".") {
            return "".to_string();
        }
        return extension.split(".").last().unwrap_or("").to_string();
    }
    pub fn get_parent(path: &str) -> String {
        //parent dir
        Path::new(path)
            .parent()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or("".to_string())
    }
    pub fn get_name(path: &str) -> String {
        //file name + extension
        Path::new(path)
            .file_name()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or("".to_string())
    }
    pub fn get_stem(path: &str) -> String {
        //just file name
        let res = Path::new(path)
            .file_stem()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or("".to_string());
        if res.contains(".") {
            return res.split(".").next().unwrap_or("").to_string();
        }
        res
    }
    pub fn get_extension(path: &str) -> String {
        let dots = path.chars().filter(|&x| x == '.').count();
        if dots == 0 {
            return String::new();
        }
        if dots > 1 {
            return path.split('.').skip(1).collect::<Vec<&str>>().join(".");
        }
        Path::new(path)
            .extension()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or("".to_string())
    }
    pub fn path_to_string(path: &Path) -> String {
        path.to_str()
            .map(|s| s.to_string())
            .unwrap_or("".to_string())
    }
}

pub fn write_string_to_file(path: &str, content: &str) -> io::Result<()> {
    let file = fs::File::create(path)?;
    let mut writer = BufWriter::new(file);

    writer.write_all(content.as_bytes())?;

    // The buffer is automatically flushed when writer goes out of scope,
    // but you can manually flush it if needed.
    writer.flush()?;

    Ok(())
}

pub fn read_string_from_file(path: &str) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    Ok(contents)
}

pub fn check_file_exists(path: &PathBuf) -> std::io::Result<()> {
    match fs::metadata(&path) {
        Ok(_) => Ok(()),
        Err(_) => Err(Error::new(
            ErrorKind::NotFound,
            format!("File {:?} does not exist", path),
        )),
    }
}

pub fn makedirs(path: &PathBuf) -> std::io::Result<()> {
    let par = path.parent();
    if let Some(par) = par {
        fs::create_dir_all(par)?;
    }
    Ok(())
}
