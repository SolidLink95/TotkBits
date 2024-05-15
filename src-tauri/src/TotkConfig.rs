#![allow(non_snake_case,non_camel_case_types)]
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
//use roead::byml::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::file_format::Pack::PackFile;
use crate::Settings::makedirs;
use crate::Settings::write_string_to_file;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TotkConfig {
    pub romfs: String,
    pub font_size: i32,
    pub context_menu_font_size: i32,
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
            font_size: 14,
            context_menu_font_size: 14,
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
                "font size": self.font_size,
                "context menu font size": self.context_menu_font_size,
                "Text editor theme": self.monaco_theme,
                "Text editor minimap": self.monaco_minimap,
                "Prompt on close all": self.close_all_prompt,
            })
        )
    }

    pub fn to_react_json(&self) -> io::Result<serde_json::Value> {
        Ok(
            json!({
                "romfs": self.romfs,
                "fontSize": self.font_size,
                "contextMenuFontSize": self.context_menu_font_size,
                "theme": self.monaco_theme,
                "minimap": self.monaco_minimap,
                // "Prompt on close all": self.close_all_prompt, unused in UI
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
        self.font_size = conf.get("font size").unwrap_or(&self.font_size.into()).as_i64().unwrap_or(self.font_size as i64) as i32;
        self.context_menu_font_size = conf.get("context menu font size").unwrap_or(&self.context_menu_font_size.into()).as_i64().unwrap_or(self.context_menu_font_size as i64) as i32;
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
        let json_data =   self.to_json()?;
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

