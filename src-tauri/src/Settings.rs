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
        Self::new("")
    }
}

impl Pathlib {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path_str = path.as_ref().to_str().unwrap_or_default().to_string();
        Self {
            parent: Pathlib::get_parent(&path),
            name: Pathlib::get_name(&path),
            stem: Pathlib::get_stem(&path),
            extension: Pathlib::get_extension(&path),
            ext_last: Self::get_ext_last(&path),
            full_path: path_str,
        }
    }

    pub fn is_file(&self) -> bool {
        Path::new(&self.full_path).is_file()
    }

    pub fn is_dir(&self) -> bool {
        Path::new(&self.full_path).is_dir()
    }

    pub fn get_ext_last<P: AsRef<Path>>(path: P) -> String {
        let extension = Pathlib::get_extension(&path);
        if !extension.contains('.') {
            return "".to_string();
        }
        extension.split('.').last().unwrap_or_default().to_string()
    }

    pub fn get_parent<P: AsRef<Path>>(path: P) -> String {
        Path::new(path.as_ref())
            .parent()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    pub fn get_name<P: AsRef<Path>>(path: P) -> String {
        Path::new(path.as_ref())
            .file_name()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    pub fn get_stem<P: AsRef<Path>>(path: P) -> String {
        let res = Path::new(path.as_ref())
            .file_stem()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default();
        if res.contains('.') {
            return res.split('.').next().unwrap_or_default().to_string();
        }
        res
    }

    pub fn get_extension<P: AsRef<Path>>(path: P) -> String {
        let path_str = path.as_ref().to_str().unwrap_or_default();
        let dots = path_str.chars().filter(|&x| x == '.').count();
        if dots == 0 {
            return String::new();
        }
        if dots > 1 {
            return path_str.split('.').skip(1).collect::<Vec<&str>>().join(".");
        }
        Path::new(path.as_ref())
            .extension()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default()
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