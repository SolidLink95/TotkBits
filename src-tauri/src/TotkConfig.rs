#![allow(non_snake_case,non_camel_case_types)]
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;
//use roead::byml::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::file_format::Pack::PackFile;
use crate::Settings::makedirs;
use crate::Settings::read_string_from_file;
use crate::Settings::write_string_to_file;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TotkConfig {
    pub romfs: String,
    pub fontSize: i32,
    pub close_all_prompt: bool,
    pub monaco_theme: String,
    pub monaco_minimap: bool,
    #[serde(skip)]
    pub available_themes: Vec<String>,
    #[serde(skip)]
    pub config_path: String,
}

impl Default for TotkConfig {
    fn default() -> Self {
        Self {
            romfs: String::new(),
            close_all_prompt: true,
            fontSize: 14,
            monaco_theme: "vs-dark".into(),
            monaco_minimap: false,
            available_themes: vec!["vs".into(), "vs-dark".into(), "hc-black".into(), "hc-light".into()],
            config_path: String::new(),
        }
    }
}

impl TotkConfig {
    pub fn safe_new() -> io::Result<TotkConfig> {
        match Self::new() {
            Ok(conf) => Ok(conf),
            Err(err) => {
                rfd::MessageDialog::new()
                    .set_buttons(rfd::MessageButtons::Ok)
                    .set_title("Error")
                    .set_description(&format!("{}", err))
                    .show();
                Err(err)
            }
        }
    }
    #[allow(dead_code)]
    pub fn to_json(&self) -> io::Result<serde_json::Value> {
        Ok(
            json!({
                "romfs": self.romfs,
                "font size": self.fontSize,
                "Text editor theme": self.monaco_theme,
                "Text editor minimap": self.monaco_minimap,
                "Prompt on close all": self.close_all_prompt,
            })
        )
    }

    pub fn new() -> io::Result<TotkConfig> {
        let mut conf = Self::default();
        conf.get_config_path()?;        
        let mut err_str = String::new();

        if let Err(err) = conf.update_default() {
            let e = format!("{:#?}\n", err);
            println!("{}", &e);
            err_str.push_str(&e);
        }
        if !conf.romfs.is_empty() {
            conf.save().map_err(|e| io::Error::new(io::ErrorKind::NotFound, format!("Unable to save config to:\n{}\n{:?}", &conf.config_path, e)))?;
            return Ok(conf)
        }
        
        if let Err(err) = conf.update_from_NX() {
            err_str.push_str(&format!("{:#?}\n", err));
        }
        if !conf.romfs.is_empty() {
            conf.save().map_err(|e| io::Error::new(io::ErrorKind::NotFound, format!("Unable to save config to:\n{}\n{:?}", &conf.config_path, e)))?;
            return Ok(conf)
        }
        
        if let Err(err) = conf.update_from_input() {
            err_str.push_str(&format!("{:#?}\n", err));
        }
        if !conf.romfs.is_empty() {
            conf.save().map_err(|e| io::Error::new(io::ErrorKind::NotFound, format!("Unable to save config to:\n{}\n{:?}", &conf.config_path, e)))?;
            return Ok(conf)
        }
        println!("{}", err_str);
        if conf.romfs.is_empty() {
            let e = format!("Unable to get proper romfs path:\n{}", err_str);
            return Err(io::Error::new(io::ErrorKind::NotFound, e));
        }
        conf.save().map_err(|e| io::Error::new(io::ErrorKind::NotFound, format!("Unable to save config to:\n{}\n{:?}", &conf.config_path, e)))?;
        Ok(conf)
    }

    pub fn get_config_path(&mut self) -> io::Result<()> {
        let appdata = env::var("LOCALAPPDATA").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "Cannot access appdata"))?;
        let mut conf_path = PathBuf::from(&appdata);
        // conf_path.push("Totkbits/config.json");
        conf_path.push("Totkbits/config.toml");
        makedirs(&conf_path)?;
        self.config_path = conf_path.to_string_lossy().to_string().replace("\\", "/");
        println!("config_path {:?}", &self.config_path);

        Ok(())
    }


    pub fn update_default(&mut self) -> io::Result<()> {
        let conf_str = fs::read_to_string(&self.config_path)?;
        println!("conf_str {:?}", &conf_str);
        // let conf: HashMap<String, serde_json::Value> = serde_json::from_str(&conf_str)?;
        let conf: HashMap<String, serde_json::Value> = toml::from_str(&conf_str).map_err(|e| io::Error::new(io::ErrorKind::NotFound, format!("Unable to parse default config\n{:?}", e)))?;
        
        let theme = self.monaco_theme.as_str().into();
        let theme = conf.get("Text editor theme").unwrap_or(&theme).as_str().unwrap_or(&self.monaco_theme).to_string();
        if self.available_themes.contains(&theme) {
            self.monaco_theme = theme;
        }
        self.fontSize = conf.get("font size").unwrap_or(&self.fontSize.into()).as_i64().unwrap_or(self.fontSize as i64) as i32;
        self.close_all_prompt = conf.get("Prompt on close all").unwrap_or(&self.close_all_prompt.into()).as_bool().unwrap_or(self.close_all_prompt);
        self.monaco_minimap = conf.get("Text editor minimap").unwrap_or(&self.monaco_minimap.into()).as_bool().unwrap_or(self.monaco_minimap);
        let binding = "".into();
        let romfs = conf.get("romfs").unwrap_or(&binding).as_str().unwrap_or("");
        
        if Self::check_for_zsdic(romfs) {
            self.romfs = romfs.to_string().replace("\\", "/");
            return Ok(());
        }
        
        return Err(io::Error::new(io::ErrorKind::NotFound, "Unable to parse default config"));
    }

    pub fn update_from_NX(&mut self) -> io::Result<()> {
        let appdata = env::var("LOCALAPPDATA").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "Cannot access appdata"))?;
        let mut nx_conf = PathBuf::from(&appdata);
        nx_conf.push("Totk/config.json");
        let nx_conf_str = fs::read_to_string(&nx_conf.to_string_lossy().to_string())?;
        let nx_conf: HashMap<String, serde_json::Value> = serde_json::from_str(&nx_conf_str)?;
        let binding = "".into();
        let romfs = nx_conf.get("GamePath").unwrap_or(&binding).as_str().unwrap_or("");
        if Self::check_for_zsdic(romfs) {
            self.romfs = romfs.to_string().replace("\\", "/");
            return Ok(());
        }
        return Err(io::Error::new(io::ErrorKind::NotFound, "Unable to parse nx editor config"));
    }
    pub fn update_from_input(&mut self) -> io::Result<()> {
        let mut chosen = rfd::FileDialog::new()
            .set_title("Choose romfs path")
            .pick_folder()
            .unwrap_or_default();
        let res = chosen.to_string_lossy().to_string().replace("\\", "/");
        if !Self::check_for_zsdic(&res) {
            chosen.push("Pack/ZsDic.pack.zs");
            let e = format!("Invalid romfs path! ZsDic.pack.zs not found:\n{}", chosen.to_string_lossy().to_string().replace("\\", "/"));
            rfd::MessageDialog::new()
                .set_buttons(rfd::MessageButtons::Ok)
                .set_title("Invalid romfs path")
                .set_description(&e)
                .show();
            return Err(io::Error::new(io::ErrorKind::NotFound, e));
        }
        self.romfs = res;
        Ok(())
    }

    pub fn check_for_zsdic(romfs: &str) -> bool {
        if romfs.is_empty() {
            return false;
        }
        let mut zsdic = PathBuf::from(romfs);
        zsdic.push("Pack/ZsDic.pack.zs");
        zsdic.exists()
    }

    #[allow(dead_code)]
    pub fn get_pack_path_from_sarc(&self, pack: PackFile) -> Option<PathBuf> {
        self.get_pack_path(&pack.path.name)
    }

    pub fn get_path(&self, pack_local_path: &str) -> Option<PathBuf> {
        //let pack_local_path = format!("Pack/Actor/{}.pack.zs", name);
        let romfs = PathBuf::from(&self.romfs);
        let mut dest_path = romfs.clone();
        dest_path.push(pack_local_path);
        if dest_path.exists() {
            return Some(dest_path);
        }
        None
    }

    pub fn get_pack_path(&self, name: &str) -> Option<PathBuf> {
        self.get_path(&format!("Pack/Actor/{}.pack.zs", name))
    }

    #[allow(dead_code)]
    pub fn get_mals_path(&self, name: &str) -> Option<PathBuf> {
        self.get_path(&format!("Mals/{}", name))
    }

    pub fn save(&self) -> io::Result<()> {
        if self.config_path.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Empty config path"));
        }
        makedirs(&PathBuf::from(&self.config_path))?;   
        // let json_str: String = serde_json::to_string_pretty(self)?;
        let json_data = self.to_json()?;
        let toml_str = toml::to_string_pretty(&json_data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{:#?}",e)))?;
        // write_string_to_file(&self.config_path, &json_str)?;
        let mut res = String::new();
        res.push_str("# Totkbits config\n");
        res.push_str(&format!("# Available text editor themes: {}\n", self.available_themes.join(", ")));
        res.push_str("# \n");
        res.push_str(&toml_str);
        write_string_to_file(&self.config_path, &res)?;
        Ok(())
    }

}



#[derive(Deserialize, Serialize, Debug)]
pub struct TotkConfigOld {
    pub romfs: PathBuf,
    pub bfres: String,
    pub yuzu_mod_path: PathBuf,
    pub config_path: PathBuf,
}

impl Default for TotkConfigOld {
    fn default() -> Self {
        Self {
            romfs: PathBuf::new(),
            bfres: String::new(),
            yuzu_mod_path: Self::get_yuzumodpath().unwrap_or("".into()),
            config_path: Self::get_config_path().unwrap_or("".into()),
        }
    }
}

impl TotkConfigOld {
    pub fn new() -> Option<TotkConfigOld> {
        if let Ok(config) = Self::from_nx_config() {
            return Some(config);
        }

        if let Ok(config) = Self::get_default() {
            return Some(config);
        }
        None
    }
    pub fn get_default() -> io::Result<TotkConfigOld> {
        let config = TotkConfigOld::get_config_from_json()?; //.expect("Unable to get totks paths");
        let yuzu_mod_path = Self::get_yuzumodpath()?; //.expect("Unable to get yuzu totk mod path");
        let romfs = PathBuf::from(config.get("romfs").unwrap_or(&"".to_string()));
        let binding = String::new();
        let bfres = config.get("bfres_raw").unwrap_or(&binding);
        let config_path = PathBuf::from(config.get("config_path").unwrap_or(&"".to_string()));

        Ok(TotkConfigOld {
            romfs: romfs,
            bfres: bfres.to_string(),
            yuzu_mod_path: yuzu_mod_path,
            config_path: config_path,
        })
    }

    pub fn from_nx_config() -> io::Result<Self> {
        match env::var("APPDATA") {
            Ok(appdata) => {
                let config_path = format!("{}/Totkbits/config.json", &appdata);
                let config_str = read_string_from_file(&config_path)?;
                let config: HashMap<String, String> = serde_json::from_str(&config_str)?;
                if let Some(romfs) = config.get("GamePath") {
                    if let Ok(romfs_path) = PathBuf::from_str(&romfs.replace("\\", "/")) {
                        return Ok(Self {
                            romfs: romfs_path,
                            bfres: String::new(),
                            yuzu_mod_path: Self::get_yuzumodpath().unwrap_or_default(),
                            config_path: Self::get_config_path().unwrap_or_default(),
                        });
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

    #[allow(dead_code)]
    pub fn clone(&self) -> TotkConfigOld {
        TotkConfigOld {
            romfs: self.romfs.clone(),
            bfres: self.bfres.clone(),
            yuzu_mod_path: self.yuzu_mod_path.clone(),
            config_path: self.config_path.clone(),
        }
    }

    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn get_pack_path(&self, name: &str) -> Option<PathBuf> {
        self.get_path(&format!("Pack/Actor/{}.pack.zs", name))
    }

    #[allow(dead_code)]
    pub fn get_mals_path(&self, name: &str) -> Option<PathBuf> {
        self.get_path(&format!("Mals/{}", name))
    }

    #[allow(dead_code)]
    pub fn save(&self) -> io::Result<()> {
        if self.config_path.to_string_lossy().is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Empty config path"));
        }
        if let Some(config_dir) = self.config_path.clone().parent() {
            fs::create_dir_all(config_dir)?;
        }
        println!("{:?}", &self.config_path);
        let json_str: String = serde_json::to_string_pretty(self)?;
        write_string_to_file(
            &self.config_path.clone().to_string_lossy().to_string(),
            &json_str,
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    fn get_config_from_json() -> io::Result<HashMap<String, String>> {
        //attempt to import config from json file
        let appdata_str = env::var("APPDATA").expect("Cannot access appdata");
        let mut config_path = PathBuf::from(appdata_str.to_string());
        config_path.push("Totkbits/config.json");
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
        config_path.push("Totkbits/config.json");
        Ok(config_path)
    }

    fn get_yuzumodpath() -> io::Result<PathBuf> {
        let appdata_str: String = env::var("APPDATA").expect("Failed to get Appdata dir");
        let appdata: PathBuf = PathBuf::from(appdata_str.to_string());
        let mut yuzu_mod_path: PathBuf = appdata.clone();
        yuzu_mod_path.push("yuzu/load/0100F2C0115B6000");
        Ok(yuzu_mod_path)
    }

    #[allow(dead_code)]
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

#[allow(dead_code)]
pub fn init() -> bool {
    let c = TotkConfigOld::new();
    if c.is_some() {
        let c = c.unwrap();
        let mut zsdic = c.romfs.clone();
        zsdic.push("Pack/ZsDic.pack.zs");
        if zsdic.exists() {
            return true;
        }
    }
    rfd::MessageDialog::new()
        .set_buttons(rfd::MessageButtons::Ok)
        .set_title("No romfs path found")
        .set_description("Please choose romfs path")
        .show();
    let chosen = rfd::FileDialog::new()
        .set_title("Choose romfs path")
        .pick_folder()
        .unwrap_or_default();
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
    let mut c = TotkConfigOld::default();
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
    c.config_path.push("Totkbits/config.json");
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
    return true;

    // println!("{:?}", c.romfs);
    // if c.romfs.to_string_lossy().is_empty() || !c.romfs.exists() || c.romfs.is_file() {
    //     rfd::MessageDialog::new()
    //         .set_buttons(rfd::MessageButtons::Ok)
    //         .set_title("No romfs path found")
    //         .set_description("Please choose romfs path")
    //         .show();
    //     let chosen = rfd::FileDialog::new()
    //         .set_title("Choose romfs path")
    //         .pick_folder()
    //         .unwrap_or_default();
    //     let dirr = chosen.to_string_lossy().to_string().replace("\\", "/");
    //     if dirr.is_empty() || !chosen.exists() {
    //         return false;
    //     }
    //     let mut zsdic = chosen.clone();
    //     zsdic.push("Pack/ZsDic.pack.zs");
    //     if !zsdic.exists() {
    //         rfd::MessageDialog::new()
    //             .set_buttons(rfd::MessageButtons::Ok)
    //             .set_title("Invalid romfs path")
    //             .set_description(format!(
    //                 "Invalid romfs path! File\n{}\ndoes not exist. Program will now exit",
    //                 &zsdic.to_string_lossy().to_string().replace("\\", "/")
    //             ))
    //             .show();
    //         return false;
    //     }
    //     let appdata_str = env::var("APPDATA").unwrap_or("".to_string());
    //     if appdata_str.is_empty() {
    //         rfd::MessageDialog::new()
    //             .set_buttons(rfd::MessageButtons::Ok)
    //             .set_title("Error")
    //             .set_description("Unable to access appdata, exiting")
    //             .show();
    //         return false;
    //     }
    //     c.config_path = PathBuf::from(appdata_str.to_string());
    //     c.config_path.push("Totkbits/config.json");
    //     println!("{:?}", &c.config_path);
    //     c.romfs = chosen.clone();
    //     if let Err(err) = c.save() {
    //         rfd::MessageDialog::new()
    //             .set_buttons(rfd::MessageButtons::Ok)
    //             .set_title("Error")
    //             .set_description(format!("{:?}", err))
    //             .show();
    //         return false;
    //     }
    // }

    // true
}
