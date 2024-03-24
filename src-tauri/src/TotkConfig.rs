use std::collections::HashMap;



use std::env;
use std::fs;
use std::io;

use std::path::PathBuf;
use std::str::FromStr;
//use roead::byml::HashMap;
use serde::{Deserialize, Serialize};

use crate::file_format::Pack::PackFile;
use crate::Settings::read_string_from_file;
use crate::Settings::write_string_to_file;

#[derive(Deserialize, Serialize, Debug)]
pub struct TotkConfig {
    pub romfs: PathBuf,
    pub bfres: String,
    pub yuzu_mod_path: PathBuf,
    pub config_path: PathBuf,
}

impl Default for TotkConfig {
    fn default() -> Self {
        Self {
            romfs: PathBuf::new(),
            bfres: String::new(),
            yuzu_mod_path: Self::get_yuzumodpath().unwrap_or("".into()),
            config_path: Self::get_config_path().unwrap_or("".into()),

        }
    }
}

impl TotkConfig {
    pub fn new() -> TotkConfig {
        if let Ok(config) = Self::get_default() {
            return config;
        }
        Self::from_nx_config().unwrap()
     }
    pub fn get_default() -> io::Result<TotkConfig> {
        let config = TotkConfig::get_config_from_json()?; //.expect("Unable to get totks paths");
        let yuzu_mod_path = Self::get_yuzumodpath()?; //.expect("Unable to get yuzu totk mod path");
        let romfs = PathBuf::from(config.get("romfs").unwrap_or(&"".to_string()));
        let binding = String::new();
        let bfres = config.get("bfres_raw").unwrap_or(&binding);
        let config_path = PathBuf::from(config.get("config_path").unwrap_or(&"".to_string()));
        
        Ok(TotkConfig {
            romfs: romfs,
            bfres: bfres.to_string(),
            yuzu_mod_path: yuzu_mod_path,
            config_path: config_path,
        })
    }

    pub fn from_nx_config() -> io::Result<Self> {
        match env::var("APPDATA") {
            Ok(appdata) => {
                let config_path = format!("{}/Totk/config.json", &appdata);
                let config_str = read_string_from_file(&config_path)?;
                let config: HashMap<String, String> = serde_json::from_str(&config_str)?;
                if let Some(romfs) = config.get("GamePath") {
                    if let Ok(romfs_path) = PathBuf::from_str(&romfs.replace("\\", "/")) {
                        return Ok(
                            Self {
                                romfs: romfs_path,
                                bfres: String::new(),
                                yuzu_mod_path: Self::get_yuzumodpath()?,
                                config_path: Self::get_config_path().unwrap_or("".into()),
                            }
                        );
                    }
                }
            }
            Err(_err) => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Cannot access localappdata",
                ));
            }
        }
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Unable to parse nx editor config",
        ));
    }

    pub fn clone(&self) -> TotkConfig {
        TotkConfig {
            romfs: self.romfs.clone(),
            bfres: self.bfres.clone(),
            yuzu_mod_path: self.yuzu_mod_path.clone(),
            config_path: self.config_path.clone(),
        }
    }

    pub fn get_pack_path_from_sarc(&self, pack: PackFile) -> Option<PathBuf> {
        self.get_pack_path(&pack.path.name)
    }

    pub fn get_path(&self, pack_local_path: &str) -> Option<PathBuf> {
        //let pack_local_path = format!("Pack/Actor/{}.pack.zs", name);
        let mut dest_path = self.romfs.clone();
        dest_path.push(pack_local_path);
        if dest_path.exists() {
            return Some(dest_path);
        }
        None
    }

    pub fn get_pack_path(&self, name: &str) -> Option<PathBuf> {
        self.get_path(&format!("Pack/Actor/{}.pack.zs", name))
    }

    pub fn get_mals_path(&self, name: &str) -> Option<PathBuf> {
        self.get_path(&format!("Mals/{}", name))
    }

    pub fn save(&self) -> io::Result<()> {
        if self.config_path.to_string_lossy().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Empty config path",
            ));
        }
        if let Some(config_dir) = self.config_path.clone().parent() {
            fs::create_dir_all(config_dir);
        }
        println!("{:?}", &self.config_path);
        let json_str: String = serde_json::to_string_pretty(self)?;
        write_string_to_file(&self.config_path.clone().to_string_lossy().to_string(), &json_str)?;
        Ok(())
    }

    fn get_config_from_json() -> io::Result<HashMap<String, String>> {
        //attempt to import config from json file
        let appdata_str = env::var("APPDATA").expect("Cannot access appdata");
        let mut config_path = PathBuf::from(appdata_str.to_string());
        config_path.push("Totk/config.json");
        if !config_path.exists() {
            let config: HashMap<String, String> = Default::default();
            return Ok(config);
        }
        let config_str = read_string_from_file(&config_path.to_string_lossy())?;
        let mut config: HashMap<String, String> = serde_json::from_str(&config_str)?;
        if !config.contains_key("config_path") {
            config.insert(
                "config_path".to_string(),
                config_path.to_string_lossy().to_string(),
            );
        }
        Ok(config)
    }

    fn get_config_path() -> io::Result<PathBuf> {
        let appdata_str = env::var("APPDATA").expect("Cannot access appdata");
        let mut config_path = PathBuf::from(appdata_str.to_string());
        config_path.push("Totk/config.json");
        Ok(config_path)
     }
     
    fn get_yuzumodpath() -> io::Result<PathBuf> {
        let appdata_str: String = env::var("APPDATA").expect("Failed to get Appdata dir");
        let appdata: PathBuf = PathBuf::from(appdata_str.to_string());
        let mut yuzu_mod_path: PathBuf = appdata.clone();
        yuzu_mod_path.push("yuzu/load/0100F2C0115B6000");
        Ok(yuzu_mod_path)
    }

    pub fn print(&self) -> io::Result<()> {
        println!(
            "Romfs: {}\nBfres: {}\n Yuzu mod path: {} {:?}",
            self.romfs.to_string_lossy(),
            self.bfres,
            self.yuzu_mod_path.to_string_lossy(),
            self.yuzu_mod_path.exists()
        );
        Ok(())
    }
}



pub fn init() -> bool {
    let mut c = TotkConfig::new();
    println!("{:?}", c.romfs);
    if c.romfs.to_string_lossy().is_empty() || !c.romfs.exists() || c.romfs.is_file() {
        rfd::MessageDialog::new()
            .set_buttons(rfd::MessageButtons::Ok)
            .set_title("No romfs path found")
            .set_description("Please choose romfs path")
            .show();
        let chosen = rfd::FileDialog::new()
            .set_title("Choose romfs path")
            .pick_folder()
            .unwrap_or_default();
        let dirr = chosen.to_string_lossy().to_string().replace("\\", "/");
        if dirr.is_empty() || !chosen.exists() {
            return false;
        }
        let mut zsdic = chosen.clone();
        zsdic.push("Pack/ZsDic.pack.zs");
        if !zsdic.exists() {
            rfd::MessageDialog::new()
                .set_buttons(rfd::MessageButtons::Ok)
                .set_title("Invalid romfs path")
                .set_description(format!(
                    "Invalid romfs path! File\n{}\ndoes not exist. Program will now exit",
                    &zsdic.to_string_lossy().to_string().replace("\\", "/")
                ))
                .show();
            return false;
        }
        let appdata_str = env::var("APPDATA").unwrap_or("".to_string());
        if appdata_str.is_empty() {
            rfd::MessageDialog::new()
                .set_buttons(rfd::MessageButtons::Ok)
                .set_title("Error")
                .set_description("Unable to access appdata, exiting")
                .show();
            return false;
        }
        c.config_path = PathBuf::from(appdata_str.to_string());
        c.config_path.push("Totk/config.json");
        println!("{:?}", &c.config_path);
        c.romfs = chosen.clone();
        if let Err(err) = c.save() {
            rfd::MessageDialog::new()
                .set_buttons(rfd::MessageButtons::Ok)
                .set_title("Error")
                .set_description(format!("{:?}", err))
                .show();
            return false;
        }
    }

    true
}
