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
use tauri::api::version;
use updater::TotkbitsVersion::TotkbitsVersion;

use crate::file_format::Pack::PackFile;
use crate::Settings::makedirs;
use crate::Settings::write_string_to_file;
use crate::Settings::Pathlib;

const MAX_INLINE_BYML_ITEMS: usize = 10;
const MIN_INLINE_BYML_ITEMS: usize = 1;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TotkConfig {
    pub romfs: String,
    pub font_size: i32,
    pub context_menu_font_size: i32,
    pub yaml_max_inl: usize,
    pub lower_float_prec: bool,
    pub close_all_prompt: bool,
    pub monaco_theme: String,
    pub monaco_minimap: bool,
    pub rotation_deg: bool,
    #[serde(skip)]
    pub game_version:String,
    #[serde(skip)]
    pub game_versions: Vec<String>,
    #[serde(skip)]
    pub available_themes: Vec<String>,
    #[serde(skip)]
    pub config_path: String,
    pub botw_romfs_path: String,
}

impl Default for TotkConfig {
    
    fn default() -> Self {
        Self {
            romfs: String::new(),
            close_all_prompt: true,
            font_size: 14,
            yaml_max_inl: 4,
            lower_float_prec: true,
            context_menu_font_size: 14,
            monaco_theme: "vs-dark".into(),
            monaco_minimap: false,
            rotation_deg: false,
            game_version: String::new(),
            game_versions: (100..130).rev().map(|e| e.to_string()).collect(),
            available_themes: vec!["vs".into(), "vs-dark".into(), "hc-black".into(), "hc-light".into()],
            config_path: String::new(),
            botw_romfs_path: String::new(),
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
                // let e = format!("Unable to create config file. ZSTD disabled and comparison suppport limited:\n{}", err);
                // rfd::MessageDialog::new()
                //     .set_buttons(rfd::MessageButtons::Ok)
                //     .set_title("Error")
                //     .set_description(&e)
                //     .show();
                Ok(Self::default())
            }
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.romfs.is_empty() && Self::check_for_zsdic(&self.romfs)
    }

    pub fn from_toml() -> io::Result<TotkConfig> {
        let mut conf = Self::default();
        conf.get_game_version().unwrap_or_default();//no point in handling error here
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
        Ok(conf)
    }

    pub fn new() -> io::Result<TotkConfig> {
        let mut conf = Self::default();
        if let Ok(_conf) = Self::from_toml() {
            if _conf.is_valid() {
                return Ok(_conf);
            }
            conf = _conf;
        }


        conf.get_game_version().unwrap_or_default();//no point in handling error here
        
        if !conf.is_valid() {
            //unable to find romfs path, get it from NX editor or user input
            conf.update_romfs_path()?;//throws error if not found
        }

        conf.save()?;
        Ok(conf)
    }




    //UPDATE INTERNAL INFO
    pub fn update_from_json_data(&mut self, json_data: HashMap<String, serde_json::Value>) {
        if json_data.is_empty() {
            println!("Empty json data");
            return;
        }
    
        fn get_string(data: &HashMap<String, serde_json::Value>, key: &str) -> String {
            data.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
        }
    
        fn get_i64(data: &HashMap<String, serde_json::Value>, key: &str, default: i64) -> i64 {
            if let Some(value) = data.get(key).and_then(|v| v.as_i64()) {
                return value;
            }
            match data.get(key).and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()) {
                Some(num) => num,
                None => default,
            }
        }
    
        fn get_bool(data: &HashMap<String, serde_json::Value>, key: &str, default: bool) -> bool {
            data.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
        }
    
        let theme = get_string(&json_data, "Text editor theme");
        if self.available_themes.contains(&theme) {
            self.monaco_theme = theme;
        }
    
        self.font_size = get_i64(&json_data, "font size", self.font_size as i64) as i32;
        self.yaml_max_inl = get_i64(&json_data, "Byml inline container max count", self.yaml_max_inl as i64) as usize;
        self.context_menu_font_size = get_i64(&json_data, "Context menu font size", self.context_menu_font_size as i64) as i32;
        self.close_all_prompt = get_bool(&json_data, "Prompt on close all", self.close_all_prompt);
        self.monaco_minimap = get_bool(&json_data, "Text editor minimap", self.monaco_minimap);
        self.lower_float_prec = get_bool(&json_data, "Lower float precision", self.lower_float_prec);
        self.rotation_deg = get_bool(&json_data, "Rotation in degrees", self.rotation_deg);
        self.romfs = get_string(&json_data, "romfs");
        self.botw_romfs_path = get_string(&json_data, "BOTW WIIU path (optional)");
    
        self.yaml_max_inl = self.yaml_max_inl.max(MIN_INLINE_BYML_ITEMS).min(MAX_INLINE_BYML_ITEMS);
        // println!("Updated config from json data {:?}", self);
    }
    

    // JSON <-> STRUCT
    #[allow(dead_code)]
    pub fn to_json(&self) -> io::Result<serde_json::Value> {
        Ok(
            json!({
                "romfs": self.romfs,
                "font size": self.font_size,
                "Byml inline container max count": self.yaml_max_inl,
                "Lower float precision": self.lower_float_prec,
                "Context menu font size": self.context_menu_font_size,
                "Text editor theme": self.monaco_theme,
                "Text editor minimap": self.monaco_minimap,
                "Prompt on close all": self.close_all_prompt,
                "Rotation in degrees": self.rotation_deg,
                "BOTW WIIU path (optional)": self.botw_romfs_path,
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
    pub fn to_react_options_json(&self) -> io::Result<serde_json::Value> {
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

    pub fn get_config_path(&mut self) -> io::Result<()> {
        let appdata = Self::get_config_root_path();
        let conf_path = Pathlib::new(Path::new(&appdata).join("Totkbits/config.toml"));
        if !Path::new(&conf_path.parent).exists() {
            fs::create_dir_all(&conf_path.parent)?;
        }
        self.config_path = conf_path.full_path;
        Ok(())
    }


    pub fn save(&mut self) -> io::Result<()> {
        if self.config_path.is_empty() {
            self.get_config_path()?;
        }
        if self.game_version.is_empty() {
            self.get_game_version().unwrap_or_default();//no point in handling error here
        }
        if self.config_path.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Empty config path"));
        }
        let json_data =   self.to_json()?;
        let toml_str = toml::to_string_pretty(&json_data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{:#?}",e)))?;
        let mut res = String::new();
        let version = TotkbitsVersion::from_str(env!("CARGO_PKG_VERSION"));
        res.push_str(&format!("# TotkBits v{} config\n#\n", version.as_str()));
        res.push_str(&format!("# Available text editor themes: {}\n", self.available_themes.join(", ")));
        if !self.game_version.is_empty() {
            res.push_str(&format!("# Detected game version: {}\n", self.game_version));
        }
        res.push_str(&format!("# Byml inline container max count must be between {} and {}\n#\n", MIN_INLINE_BYML_ITEMS, MAX_INLINE_BYML_ITEMS));
        if let Ok(exe_path) = env::current_exe() {
            if let Some(cwd_path) = exe_path.parent() {
                res.push_str(&format!("# Current working directory: {}\n", cwd_path.to_string_lossy().to_string().replace("\\", "/")));
            } else {
                res.push_str(&format!("# Executable path: {}\n", exe_path.to_string_lossy().to_string().replace("\\", "/")));
            }
        }
        res.push_str("# \n");
        res.push_str(&toml_str);
        write_string_to_file(&self.config_path, &res)?;
        Ok(())
    }

    pub fn update_romfs_path(&mut self) -> io::Result<()> {
        if self.update_romfs_from_NX().is_err() && self.update_romfs_from_input().is_err() {
            Err(io::Error::new(io::ErrorKind::NotFound, "Unable to get romfs path from NX editor or user input"))
        } else {
            Ok(())
        }
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
            .set_title("Choose Tears of The Kingdom path to dumped romfs")
            .pick_folder()
            .unwrap_or_default();
        let res = chosen.to_string_lossy().to_string().replace("\\", "/");
        if !Self::check_for_zsdic(&res) {
            chosen.push("Pack/ZsDic.pack.zs");
            let t = if !res.is_empty() {"Invalid romfs path".to_string()} else {"No romfs path selected".to_string()};
            // let e = if !res.is_empty() {format!("Invalid romfs path! ZsDic.pack.zs not found:\n{}", chosen.to_string_lossy().to_string().replace("\\", "/")) } else {"No romfs path selected. ZSTD disabled and comparison support limited".to_string()};
            let e = if !res.is_empty() {format!("Invalid romfs path! ZsDic.pack.zs not found:\n{}", chosen.to_string_lossy().to_string().replace("\\", "/")) } else {"".to_string()};
            if !e.is_empty() {
                rfd::MessageDialog::new()
                    .set_buttons(rfd::MessageButtons::Ok)
                    .set_title(&t)
                    .set_description(&e)
                    .show();
            }
            return Err(io::Error::new(io::ErrorKind::NotFound, e));
        }
        self.romfs = res;
        Ok(())
    }
    pub fn get_game_version(&mut self) -> io::Result<()> {
        let region_lang_path = PathBuf::from(&self.romfs).join("System/RegionLangMask.txt");
        // println!("{:?}", &region_lang_path);
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

    
    pub fn get_config_root_path() -> String {
        //save config in localappdata, if not possible save in appdata, if not possible save in exe path
        if let Ok(appdata) = env::var("LOCALAPPDATA") {
            return appdata;
        }
        if let Ok(appdata) = env::var("APPDATA") {
            return appdata;
        }
        if let Ok(exe_path) = env::current_exe() {
            if let Some(cwd_path) = exe_path.parent() {
                return cwd_path.to_string_lossy().to_string().replace("\\", "/");
            }
        }
        String::new()
    }
}

