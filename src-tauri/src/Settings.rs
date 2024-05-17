use std::env;
use std::fs;
use std::io;
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::TotkConfig::TotkConfig;


#[tauri::command]
pub fn get_startup_data(state: tauri::State<serde_json::Value>) -> Result<serde_json::Value, String> {
    Ok((*state.inner()).clone())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StartupData {
    pub argv1: String,
    pub config: TotkConfig,
}

impl StartupData {
    pub fn new() -> io::Result<Self> {
        let args: Vec<String> = env::args().collect();
        let argv1 = args.get(1).cloned().unwrap_or_default();
        let config = TotkConfig::safe_new()?;
        Ok(Self { argv1, config })
    }
    pub fn to_json(&self) -> io::Result<serde_json::Value> {
        let mut res = json!({"argv1": self.argv1,});
        res = update_json(res, self.config.to_react_json()?);
        Ok(res)
    }
}


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
    // pub fn path_to_string(path: &Path) -> String {
    //     path.to_str()
    //         .map(|s| s.to_string())
    //         .unwrap_or("".to_string())
    // }
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

#[allow(dead_code)]
pub fn read_string_from_file(path: &str) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    Ok(contents)
}


pub fn makedirs(path: &PathBuf) -> std::io::Result<()> {
    let par = path.parent();
    if let Some(par) = par {
        fs::create_dir_all(par)?;
    }
    Ok(())
}



pub fn update_json(mut base: serde_json::Value, update: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = base.as_object_mut() {
        if let Some(upd_obj) = update.as_object() {
            for (key, value) in upd_obj {
                obj.insert(key.to_string(), value.clone());
            }
        }
    }
    base
}