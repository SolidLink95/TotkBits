use std::collections::HashMap;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::io;
use std::fs;
use std::env;
//use roead::byml::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct TotkPath {
    pub romfs: PathBuf,
    pub bfres: PathBuf,
    pub totk_decoded: PathBuf,
    //appdata: PathBuf,
    pub yuzu_mod_path: PathBuf,
    pub config_path: PathBuf,
}

impl TotkPath {
    pub fn new() -> TotkPath {
        let config = TotkPath::get_config().expect("Unable to get totks paths");
        let config_path = PathBuf::from(config.get("config_path").unwrap().to_string());
        let yuzu_mod_path = TotkPath::get_yuzumodpath().expect("Unable to get yuzu totk mod path");
        let romfs = PathBuf::from(config.get("romfs").unwrap_or(&"".to_string()));
        let bfres = PathBuf::from(config.get("bfres_raw").unwrap_or(&"".to_string()));
        let totk_decoded = PathBuf::from(config.get("TOTK_decoded").unwrap_or(&r"W:/TOTK_modding/0100F2C0115B6000/Bfres_1.1.2".to_string()));
        

        TotkPath {
            romfs: romfs, 
            bfres: bfres,
            totk_decoded: totk_decoded,
            //appdata: appdata,
            yuzu_mod_path: yuzu_mod_path,
            config_path: config_path,
        }
    }

    pub fn get_pack_path(&self, name: &str) -> io::Result<PathBuf> {
        let pack_local_path = format!("Pack/Actor/{}.pack.zs", name);
        let mut pack_path = self.romfs.clone();
        pack_path.push(pack_local_path);
        if pack_path.exists() {
            return Ok(pack_path);
        }
        else {
            let m: String = format!("Pack with name {:?} does not exist (full path: {:?})", name, pack_path);
            //println!("Pack with name {:?} does not exist (full path: {:?})", name, pack_path);
            return Err(io::Error::new(io::ErrorKind::NotFound, m));
        }
    }

    pub fn save(&self) -> io::Result<()> {
        let mut file = fs::File::create(self.config_path.clone().into_os_string().into_string().unwrap())?;
        //let mut res: HashMap<String, String> = Default::default();
        //res.insert("romfs".to_string(), self.romfs.clone().into_os_string().into_string().unwrap());
        //res.insert("bfres_raw".to_string(), self.bfres.clone().into_os_string().into_string().unwrap());
        //res.insert("TOTK_decoded".to_string(), self.totk_decoded.clone().into_os_string().into_string().unwrap());
        let json_str: String = serde_json::to_string_pretty(self)?;
        file.write_all(json_str.as_bytes()).expect("Failed to save totk config file");
        Ok(())

    }

    fn get_config() -> io::Result<HashMap<String, String>> {
        //attempt to import config from json file
        let appdata_str = env::var("APPDATA").expect("Cannot access appdata");
        let mut config_path = PathBuf::from(appdata_str.to_string());
        config_path.push("Totk/config.json");
        if !config_path.exists() {
            let config: HashMap<String, String> = Default::default();
            return Ok(config);
        }
        let mut file = fs::File::open(config_path.clone()).expect(&format!("Cannot open file {:?}", config_path));
        let mut config_str = String::new();
        file.read_to_string(&mut config_str).expect(&format!("Cannot read file {:?}", config_path));
        let mut config: HashMap<String, String> = serde_json::from_str(&config_str)
                    .expect(&format!("Cannot convert file {:?} to HashMap", config_path));
        if !config.contains_key("config_path") {
            config.insert("config_path".to_string(), config_path.into_os_string().into_string().unwrap());
        }
        Ok(config)
    }

    fn get_yuzumodpath() -> io::Result<PathBuf> {
        let appdata_str: String = env::var("APPDATA").expect("Failed to get Appdata dir");
        let appdata: PathBuf = PathBuf::from(appdata_str.to_string());
        let mut yuzu_mod_path: PathBuf = appdata.clone();
        yuzu_mod_path.push("yuzu/load/0100F2C0115B6000");
        Ok(yuzu_mod_path)
    }

    pub fn print(&self) -> io::Result<()> {
        println!("Romfs: {}\nBfres: {}\n Yuzu mod path: {} {:?}", 
            self.romfs.to_string_lossy(), 
            self.bfres.to_string_lossy(),
            self.yuzu_mod_path.to_string_lossy(),
            self.yuzu_mod_path.exists()
        );
        Ok(())
    }
}

