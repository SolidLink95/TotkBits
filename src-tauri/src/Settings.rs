use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::process::Command;
use regex::Regex;
use rfd::MessageDialog;
use serde::{Deserialize, Serialize};
use serde_json::json;
use updater::TotkbitsVersion::TotkbitsVersion;

use crate::TotkConfig::TotkConfig;

pub const BACKUP_UPDATER_NAME: &str = "backup_updater.exe";


pub const NO_WINDOW_FLAG: u32 = 0x08000000;

#[tauri::command]
pub fn get_startup_data(state: tauri::State<serde_json::Value>) -> Result<serde_json::Value, String> {
    Ok((*state.inner()).clone())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StartupData {
    pub argv1: String,
    pub config: TotkConfig,
    pub zstd_msg: String,
}

impl StartupData {
    pub fn new() -> io::Result<Self> {
        let args: Vec<String> = env::args().collect();
        let argv1 = args.get(1).cloned().unwrap_or_default();
        let config = TotkConfig::safe_new().unwrap_or(TotkConfig::default());
        let zstd_msg = if config.is_valid() {
            ""
        } else {
            "ZSTD disabled"
        };
        Ok(Self { argv1, config, zstd_msg: zstd_msg.to_string() })
    }
    pub fn to_json(&self) -> io::Result<serde_json::Value> {
        let mut res = json!({"argv1": self.argv1,});
        res = update_json(res, self.config.to_react_json()?);
        res = update_json(res, json!({"zstd_msg": self.zstd_msg}));
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

    pub fn is_valid(&self) -> bool {
        self.full_path.len() > 0
    }
    #[inline]
    pub fn is_file(&self) -> bool {
        Path::new(&self.full_path).is_file()
    }
    #[inline]
    pub fn is_dir(&self) -> bool {
        Path::new(&self.full_path).is_dir()
    }
    #[inline]
    pub fn exists(&self) -> bool {
        Path::new(&self.full_path).exists()
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
            .unwrap_or_default().replace("\\", "/")
    }

    pub fn get_name<P: AsRef<Path>>(path: P) -> String {
        Path::new(path.as_ref())
            .file_name()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default().replace("\\", "/")
    }

    pub fn get_stem<P: AsRef<Path>>(path: P) -> String {
        let res = Path::new(path.as_ref())
            .file_stem()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default().replace("\\", "/");
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
            .unwrap_or_default().replace("\\", "/")
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

pub fn list_files_recursively<T: AsRef<Path>>(path: &T) -> Vec<String> {
    let mut files = Vec::new();

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries {
            if let Ok(entry) = entry {
                let entry_path = entry.path();
                if entry_path.is_file() && entry_path.exists() {
                    if let Some(path_str) = entry_path.to_str() {
                        files.push(path_str.to_string().replace("\\", "/"));
                    }
                } else if entry_path.is_dir() {
                    // Recurse into subdirectories
                    files.extend( list_files_recursively(&entry_path));
                }
            }
        }
    }

    files
}

pub fn process_inline_content(mut input: String, inline_count: usize) -> String {
       // Regex to match content between { and }
       let re = Regex::new(r"(\s*)\{(.*?)\}").unwrap();

       // Replace content in the input string
       input = re.replace_all(&input, |caps: &regex::Captures| {
           if let Some(indentation) = caps.get(1) {
               let indent = indentation.as_str();
            //    println!("Indentation: {:?}", indent);
               if let Some(content) = caps.get(2) {
                   let content_str = content.as_str();
                
                   // Split content by commas
                   let items: Vec<&str> = content_str.split(',').collect();
   
                   if items.len() > inline_count {
                       // If more than 9 items, format as multiline with proper indentation
                       let mut multiline = String::new();
                       for item in items {
                           let parts: Vec<&str> = item.splitn(2, ':').map(|s| s.trim()).collect();
                           if parts.len() == 2 {
                                let res = &format!("{}{}: {}\n", indent, parts[0], parts[1]);
                               multiline.push_str(res);
                           }
                       }
                       while (multiline.ends_with("\n") || multiline.ends_with(" ")) && multiline.len() > 1 {
                           multiline.pop();
                       }
                       return format!("{}",  multiline);
                   } else {
                       // Otherwise, keep as single-line content
                       return format!("{}{{{}}}", indent, content_str);
                   }
               }
           }
           String::new()
       }).to_string();
   
       input
}



pub fn spawn_updater(latest_ver: &str) -> io::Result<()> {
    let version = env!("CARGO_PKG_VERSION").to_string();
    if MessageDialog::new()
        .set_title("Update Available")
        .set_description(&format!("Update available: {} -> {}\nTotkBits will be closed, make sure to save all opened files.\nProceed?", version, latest_ver))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        != rfd::MessageDialogResult::Yes
    {
        return Ok(());
    }

    let mut upd_exe = if cfg!(debug_assertions) {
        "../ext_projects/updater/target/debug/updater.exe"
    } else {
        "updater.exe"
    }.to_string();
    let upd_path = fs::canonicalize(&upd_exe)?;
    if !upd_path.exists() {
        println!("[-] Updater executable not found: {}", &upd_exe);
        process::exit(1);
    }
    upd_exe = upd_path.to_string_lossy().to_string().replace("\\\\?\\", "");
    let backup_upd_exe = format!("{}\\{}", Pathlib::new(&upd_exe).parent, BACKUP_UPDATER_NAME).replace("/", "\\");
    if Path::new(&backup_upd_exe).exists() {
        println!("[+] Removing old backup updater: {}", &backup_upd_exe);
        fs::remove_file(&backup_upd_exe)?;
    }
    println!("[+] Backing up: {}", &backup_upd_exe);
    fs::copy(&upd_exe, &backup_upd_exe)?;
    println!("[+] Updater executable found: {}", &backup_upd_exe);
    let p = Command::new("cmd")
        .arg("/c")
        .arg("start")
        .arg(&backup_upd_exe)
        .arg(&version)
        .arg(latest_ver)
        // .arg(process::id().to_string())
        // .arg("no")
        .spawn()?;
    // pipe_worker();
    process::exit(0);
    Ok(())
}