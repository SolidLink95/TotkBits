#![allow(non_snake_case,non_camel_case_types)]
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use flate2::read::ZlibDecoder;
//use roead::byml::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::file_format::Pack::PackFile;
use crate::Settings::makedirs;
use crate::Settings::write_string_to_file;
use crate::Settings::Pathlib;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TotkConfigOld {
    pub romfs: String,
    pub font_size: i32,
    pub context_menu_font_size: i32,
    pub close_all_prompt: bool,
    pub monaco_theme: String,
    pub monaco_minimap: bool,
    #[serde(skip)]
    pub game_version:String,
    #[serde(skip)]
    pub game_versions: Vec<String>,
    #[serde(skip)]
    pub available_themes: Vec<String>,
    #[serde(skip)]
    pub config_path: String,
}

impl Default for TotkConfigOld {
    fn default() -> Self {
        Self {
            romfs: String::new(),
            close_all_prompt: true,
            font_size: 14,
            context_menu_font_size: 14,
            monaco_theme: "vs-dark".into(),
            monaco_minimap: false,
            game_version: String::new(),
            game_versions: (100..130).rev().map(|e| e.to_string()).collect(),
            available_themes: vec!["vs".into(), "vs-dark".into(), "hc-black".into(), "hc-light".into()],
            config_path: String::new(),
        }
    }
}

impl TotkConfigOld {
    pub fn safe_new() -> io::Result<TotkConfigOld> {
        match Self::new() {
            Ok(mut conf) => {
                conf.get_game_version()?;
                Ok(conf)
            },
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

    pub fn new() -> io::Result<TotkConfigOld> {
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
        // println!("conf_str {:?}", &conf_str);
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
    pub fn get_pack_path_from_sarc(&self, pack: &PackFile) -> Option<PathBuf> {
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

    pub fn get_game_version(&mut self) -> io::Result<()> {
        let region_lang_path = PathBuf::from(&self.romfs).join("System/RegionLangMask.txt");
        println!("{:?}", &region_lang_path);
        let file = File::open(region_lang_path)?;
        let reader = BufReader::new(file);

        // Read lines into an iterator, skip the first two, and process the third line
        let third_line = reader
            .lines()
            .skip(2) // Skip the first two lines (0-indexed)
            .next() // Take the third line
            .transpose()?; // Handle the Option<Result<String>>

        // Remove whitespace from the line
        self.game_version = third_line
            .unwrap_or_default() // Handle None case with default empty string
            .chars()
            .filter(|c| !c.is_whitespace()) // Filter out whitespace
            .collect();
        Ok(())
    }

    pub fn find_vanila_file_in_romfs<P: AsRef<Path>>(&self, path: P) -> io::Result<String> {
        //parse json
        let json_zlibdata = fs::read("bin/totk_filename_to_localpath.bin")?;
        let mut decoder = ZlibDecoder::new(&json_zlibdata[..]);
        let mut json_str = String::new();
        decoder.read_to_string(&mut json_str)?;
        let res: HashMap<String, String> = serde_json::from_str(&json_str)?;
        //get filename (key)
        let filename = Pathlib::new(&path).name;
        let mut filenames: Vec<String> = vec![];
        let mut tmp_vec: Vec<String> = vec![];
        //Assume path can be .zs or without .zs
        filenames.push(filename.to_string());
        if filename.to_ascii_lowercase().ends_with(".zs") {
            filenames.push(filename.clone()[..filename.len()-3].to_string());
        } else {
            filenames.push(filename.clone().to_string() + ".zs");
        }
        //process Mals or other stuff RSDB
        let mut filename_gamever = String::new();
        for game_ver in self.game_versions.iter() {
            if filename.contains(game_ver) {
                filename_gamever = game_ver.clone();
                break;
            }
        }
        if !self.game_version.is_empty() && !filename_gamever.is_empty() && self.game_versions.iter().any(|e| filename.contains(e)) {
            for fname in filenames.iter() {
                for game_ver in self.game_versions.iter() {
                    let new_elem = fname.clone().replace(&filename_gamever, game_ver);
                    tmp_vec.push(new_elem);
                }
            }
        }
        filenames.extend(tmp_vec);
        // println!("{:?}", &self.game_versions);
        // println!("{:?}", &filenames);
        // println!("{:?}", &self.game_version);

        // Convert the Vec<String> to a HashSet to remove duplicates
        let set: HashSet<_> = filenames.drain(..).collect();

        // Convert the HashSet back to a Vec<String>
        filenames = set.into_iter().collect();

        for filename in filenames {
            // get local romfs path
            if let Some(file_in_romfs_path) = res.get(&filename) {
                //join with romfs path
                let result = PathBuf::from(&self.romfs).join(file_in_romfs_path);
                if result.exists() {
                    return Ok(result.to_string_lossy().to_string().replace("\\", "/"));
                }
            }
        }
       
        Err(io::Error::new(io::ErrorKind::NotFound, "File not found"))
        
    }

}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TotkConfig {
    pub romfs: String,
    pub font_size: i32,
    pub context_menu_font_size: i32,
    pub close_all_prompt: bool,
    pub monaco_theme: String,
    pub monaco_minimap: bool,
    #[serde(skip)]
    pub game_version:String,
    #[serde(skip)]
    pub game_versions: Vec<String>,
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
            game_version: String::new(),
            game_versions: (100..130).rev().map(|e| e.to_string()).collect(),
            available_themes: vec!["vs".into(), "vs-dark".into(), "hc-black".into(), "hc-light".into()],
            config_path: String::new(),
        }
    }
}

impl TotkConfig {
    
    pub fn safe_new() -> io::Result<TotkConfig> {
        match Self::new() {
            Ok(conf) => {
                Ok(conf)
            },
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

    pub fn new() -> io::Result<TotkConfig> {
        let mut conf = Self::default();
        //get config path
        conf.get_config_path()?;
        //try to update from toml config
        let mut conf_json: HashMap<String, serde_json::Value> = Default::default();
        if Path::new(&conf.config_path).exists() {
            let mut conf_str = String::new();
            match fs::read_to_string(&conf.config_path) {
                Ok(s) => conf_str = s,
                Err(e) => {
                    let e = format!("Unable to read config file:\n{}", e);
                    rfd::MessageDialog::new()
                        .set_buttons(rfd::MessageButtons::Ok)
                        .set_title("Error")
                        .set_description(&e)
                        .show();
                    // return Err(io::Error::new(io::ErrorKind::NotFound, e));
                }
            }
            conf_json = toml::from_str(&conf_str).unwrap_or_default();
        } 
        conf.update_from_json_data(conf_json);
        
        if !Self::check_for_zsdic(&conf.romfs) {
            //unable to find romfs path, get it from NX editor or user input
            conf.update_romfs_path()?;//throws error if not found
        }
        conf.get_game_version().unwrap_or_default();//no point in handling error here



        

        conf.save()?;
        Ok(conf)
    }




    //UPDATE INTERNAL INFO
    pub fn update_from_json_data(&mut self, json_data: HashMap<String, serde_json::Value>) {
        if json_data.is_empty() {
            return;
        }
        let binding = "".into();
        let theme = json_data.get("Text editor theme").unwrap_or(&binding).as_str().unwrap_or("").to_string();
        if self.available_themes.contains(&theme) {
            self.monaco_theme = theme;
        }
        self.font_size = json_data.get("font size").unwrap_or(&self.font_size.into()).as_i64().unwrap_or(self.font_size as i64) as i32;
        self.context_menu_font_size = json_data.get("context menu font size").unwrap_or(&self.context_menu_font_size.into()).as_i64().unwrap_or(self.context_menu_font_size as i64) as i32;
        self.close_all_prompt = json_data.get("Prompt on close all").unwrap_or(&self.close_all_prompt.into()).as_bool().unwrap_or(self.close_all_prompt);
        self.monaco_minimap = json_data.get("Text editor minimap").unwrap_or(&self.monaco_minimap.into()).as_bool().unwrap_or(self.monaco_minimap);
        self.romfs = json_data.get("romfs").unwrap_or(&binding).as_str().unwrap_or("").to_string();
    }

    pub fn get_config_path(&mut self) -> io::Result<()> {
        let appdata = env::var("LOCALAPPDATA").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "Cannot access appdata"))?;
        let conf_path = Pathlib::new(Path::new(&appdata).join("Totkbits/config.toml"));
        if !Path::new(&conf_path.parent).exists() {
            fs::create_dir_all(&conf_path.parent)?;
        }
        self.config_path = conf_path.full_path;
        Ok(())
    }
    pub fn save(&self) -> io::Result<()> {
        if self.config_path.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Empty config path"));
        }
        let json_data =   self.to_json()?;
        let toml_str = toml::to_string_pretty(&json_data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{:#?}",e)))?;
        let mut res = String::new();
        res.push_str("# Totkbits config\n");
        res.push_str(&format!("# Available text editor themes: {}\n", self.available_themes.join(", ")));
        res.push_str("# \n");
        res.push_str(&toml_str);
        write_string_to_file(&self.config_path, &res)?;
        Ok(())
    }

    pub fn update_romfs_path(&mut self) -> io::Result<()> {
        if let Err(_) = self.update_romfs_from_NX() {
            return self.update_romfs_from_input();
        }
        Err(io::Error::new(io::ErrorKind::NotFound, "Unable to get romfs path"))
    }

    pub fn check_for_zsdic<P:AsRef<Path>>(romfs_path: P) -> bool {
         romfs_path.as_ref().join("Pack/ZsDic.pack.zs").exists()
    }

    pub fn update_romfs_from_NX(&mut self) -> io::Result<()> {
        let appdata = env::var("LOCALAPPDATA").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "Cannot access appdata"))?;
        let nx_conf = PathBuf::from(&appdata).join("Totk/config.json");
        let nx_conf_str = fs::read_to_string(&nx_conf)?;
        let nx_conf: HashMap<String, serde_json::Value> = serde_json::from_str(&nx_conf_str)?;
        let binding = "".into();
        let romfs = nx_conf.get("GamePath").unwrap_or(&binding).as_str().unwrap_or("");
        if Self::check_for_zsdic(&romfs) {
            self.romfs = romfs.to_string().replace("\\", "/");
            return Ok(());
        }
        return Err(io::Error::new(io::ErrorKind::NotFound, "Unable to parse nx editor config"));
    }
    pub fn update_romfs_from_input(&mut self) -> io::Result<()> {
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
    pub fn get_game_version(&mut self) -> io::Result<()> {
        let region_lang_path = PathBuf::from(&self.romfs).join("System/RegionLangMask.txt");
        println!("{:?}", &region_lang_path);
        let file = File::open(region_lang_path)?;
        let reader = BufReader::new(file);

        // Read lines into an iterator, skip the first two, and process the third line
        let third_line = reader
            .lines()
            .skip(2) // Skip the first two lines (0-indexed)
            .next() // Take the third line
            .transpose()?; // Handle the Option<Result<String>>

        // Remove whitespace from the line
        self.game_version = third_line
            .unwrap_or_default() // Handle None case with default empty string
            .chars()
            .filter(|c| !c.is_whitespace()) // Filter out whitespace
            .collect();
        Ok(())
    }


    //FIND VANLA FILE IN ROMFS
    pub fn find_vanila_file_in_romfs<P: AsRef<Path>>(&self, path: P) -> io::Result<String> {
        //parse json
        let json_zlibdata = fs::read("bin/totk_filename_to_localpath.bin")?;
        let mut decoder = ZlibDecoder::new(&json_zlibdata[..]);
        let mut json_str = String::new();
        decoder.read_to_string(&mut json_str)?;
        let res: HashMap<String, String> = serde_json::from_str(&json_str)?;
        //get filename (key)
        let filename = Pathlib::new(&path).name;
        let mut filenames: Vec<String> = vec![];
        let mut tmp_vec: Vec<String> = vec![];
        //Assume path can be .zs or without .zs
        filenames.push(filename.to_string());
        if filename.to_ascii_lowercase().ends_with(".zs") {
            filenames.push(filename.clone()[..filename.len()-3].to_string());
        } else {
            filenames.push(filename.clone().to_string() + ".zs");
        }
        //process Mals or other stuff RSDB
        let mut filename_gamever = String::new();
        for game_ver in self.game_versions.iter() {
            if filename.contains(game_ver) {
                filename_gamever = game_ver.clone();
                break;
            }
        }
        //filename contains game version - process all of them
        if !self.game_version.is_empty() && !filename_gamever.is_empty() && self.game_versions.iter().any(|e| filename.contains(e)) {
            for fname in filenames.iter() {
                for game_ver in self.game_versions.iter() {
                    let new_elem = fname.clone().replace(&filename_gamever, game_ver);
                    tmp_vec.push(new_elem);
                }
            }
        }
        filenames.extend(tmp_vec);

        // Convert the Vec<String> to a HashSet to remove duplicates
        let set: HashSet<_> = filenames.drain(..).collect();

        // Convert the HashSet back to a Vec<String>
        filenames = set.into_iter().collect();

        for filename in filenames {
            // get local romfs path
            if let Some(file_in_romfs_path) = res.get(&filename) {
                //join with romfs path
                let result = PathBuf::from(&self.romfs).join(file_in_romfs_path);
                if result.exists() {
                    //return first existing file
                    return Ok(result.to_string_lossy().to_string().replace("\\", "/"));
                }
            }
        }
       
        Err(io::Error::new(io::ErrorKind::NotFound, "File not found"))
        
    }

    #[allow(dead_code)]
    pub fn get_pack_path_from_sarc(&self, pack: &PackFile) -> Option<PathBuf> {
        self.get_pack_path(&pack.path.name)
    }

    pub fn get_path(&self, pack_local_path: &str) -> Option<PathBuf> {
        //let pack_local_path = format!("Pack/Actor/{}.pack.zs", name);
        let dest_path = PathBuf::from(&self.romfs).join(pack_local_path);
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

    // JSON <-> STRUCT
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
}