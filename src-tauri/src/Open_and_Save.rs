use crate::{
    file_format::{
        Ainb_py::Ainb_py, Asb_py::{Asb_py, ASB_SEPARATOR}, BinTextFile::{is_banc_path, replace_rotate_deg_to_rad, BymlFile, OpenedFile}, Esetb::Esetb, Evfl_cs::Evfl, Msbt::str_endian_to_roead, Pack::{PackComparer, PackFile, SarcPaths}, Rstb::Restbl, TagProduct::TagProduct, Wrapper::PythonWrapper, Xlink::Xlink_rs, SMO::SmoSaveFile::SmoSaveFile
    }, Comparer::DiffComparer, Settings::Pathlib, TotkApp::InternalFile, Zstd::{is_aamp, is_ainb, is_ainb_path, is_asb_path, is_byml, is_byml_path, is_esetb_path, is_evfl_path, is_gamedatalist, is_msbt_path, is_msyt, is_rstb_path, is_tagproduct_path, is_xlink_path, TotkFileType, TotkZstd}
};
use msbt_bindings_rs::MsbtCpp::MsbtCpp;
use rfd::{FileDialog, MessageDialog};
use roead::{aamp::ParameterIO, byml::Byml};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read, Write},
    path::Path,
    sync::Arc,
};

pub fn open_sarc<P: AsRef<Path>>(
    file_name: P,
    zstd: Arc<TotkZstd>,
) -> Option<(PackComparer, SendData)> {
    let mut data = SendData::default();
    let path_ref = file_name.as_ref();
    print!("Is {:?} a sarc? ", &path_ref.display());
    let pathlib_var = Pathlib::new(path_ref); 
    if let Ok(sarc) = PackFile::new(path_ref, zstd.clone()) {
        let e_s = if sarc.endian==roead::Endian::Little {" [LE]"} else {" [BE]"};
        let yaz0_s = if sarc.is_yaz0 {" [Yaz0] "} else {" "};
        if let Some(pack) = PackComparer::from_pack(sarc, zstd.clone()) {
            println!(" yes!");
            data.get_sarc_paths(&pack);
            data.status_text = format!("Opened {}", &path_ref.to_string_lossy().replace("\\","/"));
            data.path = pathlib_var.clone();
            data.tab = "SARC".to_string();
            data.file_label = format!("{}{}[SARC]{}", &pathlib_var.name, yaz0_s, e_s);
            return Some((pack, data));
        }

    }
    println!(" no");

    None
}

pub fn open_msbt<P:AsRef<Path>>(path: P) -> Option<(OpenedFile<'static>, SendData)> {
    let file_name = path.as_ref().to_string_lossy().to_string().replace("\\", "/");
    let mut opened_file = OpenedFile::default();
    let mut data = SendData::default();
    print!("Is {} a msbt?", &file_name);
    opened_file.msyt = MsbtCpp::from_binary_file(&file_name).ok();
    if let Some(m) = &opened_file.msyt {
        // let m = opened_file.msyt.as_ref().unwrap();
        println!(" yes!");
        let endian = str_endian_to_roead(&m.endian.clone().unwrap_or("LE".to_string()));
        opened_file.path = Pathlib::new(&file_name);
        opened_file.endian = Some(endian);
        opened_file.file_type = TotkFileType::Msbt;
        data.status_text = format!("Opened {}", &file_name);
        data.path = Pathlib::new(file_name.clone());
        data.text = m.text.clone();
        data.get_file_label(opened_file.file_type, Some(endian));
        return Some((opened_file, data));
    }
    println!(" no");
    None
}


pub fn open_text<P: AsRef<Path>>(path: P) -> Option<(OpenedFile<'static>, SendData)> {
    let mut opened_file = OpenedFile::default();
    let mut data = SendData::default();
    let path_ref = path.as_ref();
    let pathlib_var = Pathlib::new(path_ref);
    print!("Is {} regular text file? ", &pathlib_var.full_path);
    let mut file = fs::File::open(path_ref).ok()?;
    let mut buffer = Vec::new();
    if let Ok(x) = file.read_to_end(&mut buffer) {
        if let Ok(text) = String::from_utf8(buffer) {
            println!(" yes!");
            opened_file.path = pathlib_var.clone();
            opened_file.file_type = TotkFileType::Text;
            data.status_text = format!("Opened {}", &pathlib_var.full_path);
            data.path = pathlib_var;
            data.text = text;
            data.get_file_label(TotkFileType::Text, None);
            return Some((opened_file, data));
        }
    }
    println!(" no");
    None
}

pub fn open_aamp<P: AsRef<Path>>(path: P) -> Option<(OpenedFile<'static>, SendData)> {
    let mut opened_file = OpenedFile::default();
    let mut data = SendData::default();
    let path_ref = path.as_ref();
    let pathlib_var = Pathlib::new(path_ref);
    print!("Is {} an aamp? ", &pathlib_var.full_path);
    let raw_data = std::fs::read(path_ref).ok()?;
    if is_aamp(&raw_data) {
        let pio = ParameterIO::from_binary(&raw_data).ok()?; // Parse AAMP from binary data
        println!(" yes!");
        opened_file.path = pathlib_var.clone();
        opened_file.file_type = TotkFileType::Aamp;
        data.status_text = format!("Opened {}", &pathlib_var.full_path);
        data.path = pathlib_var;
        data.text = pio.to_text();
        data.get_file_label(TotkFileType::Aamp, None);
        return Some((opened_file, data));
    }
    println!(" no");
    None
}

pub fn get_string_from_data<P: AsRef<Path>>(
    filepath: P,
    data: Vec<u8>,
    zstd: Arc<TotkZstd>,
) -> Option<(InternalFile, String)> {
    let mut internal_file = InternalFile::default();
    if data.is_empty() {
        return None;
    }
    let path = filepath.as_ref().to_string_lossy().into_owned();
    if is_esetb_path(&filepath) {
        if let Ok(esetb) = Esetb::from_binary(&data, zstd.clone()) {
            internal_file.endian = Some(roead::Endian::Little);
            internal_file.path = Pathlib::new(path.clone());
            internal_file.file_type = TotkFileType::Esetb;
            let text = esetb.to_string();
            internal_file.esetb = Some(esetb);
            return Some((internal_file, text));
        }
    }



    if let Ok(asb) = Asb_py::from_binary(&data, zstd.clone(), filepath.as_ref()) {
        if let Ok(text) = asb.binary_to_text() {
            internal_file.endian = Some(roead::Endian::Little);
            internal_file.path = Pathlib::new(path.clone());
            internal_file.file_type = TotkFileType::ASB;
            return Some((internal_file, text));
        }
    }

    if is_ainb(&data) {
        if let Ok(text) = Ainb_py::new().binary_to_text(&data) {
            internal_file.endian = Some(roead::Endian::Little);
            internal_file.path = Pathlib::new(path.clone());
            internal_file.file_type = TotkFileType::AINB;
            return Some((internal_file, text));
        }
    }
    if is_byml(&data) {
        if let Ok(file_data) = BymlFile::byml_data_to_bytes(&data, zstd.clone()) {
            if let Ok(byml_file) = BymlFile::from_binary(file_data, zstd.clone(), path.clone()) {
                // let text = Byml::to_text(&byml_file.pio);
                let text = byml_file.to_string();
                internal_file.byml = Some(byml_file);
                let byml_ref = internal_file.byml.as_ref().unwrap(); // Safe due to the line above
                internal_file.endian = byml_ref.endian.clone();
                internal_file.path = Pathlib::new(path);
                internal_file.file_type = byml_ref.file_data.file_type.clone(); // Set file type
                return Some((internal_file, text));
            }
        }
    }

    if is_aamp(&data) {
        let text = ParameterIO::from_binary(&data).ok()?.to_text();
        internal_file.endian = None;
        internal_file.path = Pathlib::new(path.clone());
        internal_file.file_type = TotkFileType::Aamp;
        return Some((internal_file, text));
    }
    if is_msyt(&data) {
        // let msbt = MsbtFile::from_binary(data, Some(path.clone()))?;
        // internal_file.endian = Some(msbt.endian.clone());
        // internal_file.path = Pathlib::new(path.clone());
        // internal_file.file_type = TotkFileType::Msbt;
        let msbt = MsbtCpp::from_binary(&data).ok()?;

        internal_file.endian = Some(str_endian_to_roead(&msbt.endian.unwrap_or("LE".to_string())));
        internal_file.path = Pathlib::new(path.clone());
        internal_file.file_type = TotkFileType::Msbt;
        return Some((internal_file, msbt.text));
    }
    if let Ok(text) = String::from_utf8(data) {
        internal_file.endian = None;
        internal_file.path = Pathlib::new(path.clone());
        internal_file.file_type = TotkFileType::Text;
        return Some((internal_file, text));
    }

    None
}

#[allow(dead_code)]
fn write_data_to_file<P: AsRef<Path>>(path: P, data: Vec<u8>) -> io::Result<()> {
    let path = path.as_ref();

    // Ensure the parent directory exists.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Open the file in write mode, creating it if it doesn't exist.
    let mut file = File::create(path)?;

    // Write the data to the file.
    file.write_all(&data)?;

    Ok(())
}

#[allow(dead_code)]
pub fn save_file_dialog(file_name: Option<String>) -> String {
    let name = file_name.unwrap_or_default();
    let file = FileDialog::new().set_file_name(name).save_file();
    match file {
        Some(res) => {
            return res.to_string_lossy().into_owned();
        }
        None => {
            return "".to_string();
        }
    }
}

pub fn check_if_save_in_romfs(dest_file: &str, zstd: Arc<TotkZstd>) -> bool {
    if !dest_file.is_empty() {
        //check if file is saved in romfs
        // if dest_file.starts_with(&zstd.totk_config.romfs.to_string_lossy().to_string()) {
        if dest_file.starts_with(&zstd.totk_config.romfs) {
            let m = format!(
                "About to save file:\n{}\nin romfs dump. Continue?",
                &dest_file
            );
            if MessageDialog::new()
                .set_title("Warning")
                .set_description(m)
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                == rfd::MessageDialogResult::Yes
            {
                return true;
            }
        }
    }
    false
}

pub fn get_binary_by_filetype(
    file_type: TotkFileType,
    text: &str,
    endian: roead::Endian,
    zstd: Arc<TotkZstd>,
    file_path: &str,
    opened_file: &mut OpenedFile<'_>,
) -> Option<Vec<u8>> {
    let mut rawdata: Vec<u8> = Vec::new();
    let endian_str = match endian {
        roead::Endian::Big => "BE",
        roead::Endian::Little => "LE",
    };
    let is_zs = file_path.to_lowercase().ends_with(".zs");
    let is_bcett = file_path.to_lowercase().ends_with(".bcett.byml.zs");
    match file_type {
        TotkFileType::Xlink => {
            let xlink = Xlink_rs::new(zstd.clone()).ok()?;
            if let Ok(new_data) = xlink.yaml_to_binary(text) {
                rawdata = new_data;
                if is_zs {
                    rawdata = zstd.compress_zs(&rawdata).ok()?;
                }
            }
        }
        TotkFileType::Evfl => {
          let evfl = Evfl::new(zstd.clone());
          if let Ok(new_data) = evfl.string_to_binary(text) {
            if is_zs {
                if let Ok(compressed_data) = zstd.compress_zs(&new_data) {
                    rawdata = compressed_data;
                }
            } else {
                rawdata = new_data;
            }
              
          }
        }
        TotkFileType::Esetb => {
            if let Some(esetb) = &mut opened_file.esetb {
                esetb.update_from_text(text).ok()?;
                rawdata = esetb.to_binary();
                if file_path.to_lowercase().ends_with(".zs") {
                    rawdata = zstd.compress_zs(&rawdata).ok()?;
                }
            }
        }
        TotkFileType::ASB => {
            //asb is never an internal file, so i can just save baev here
            let asb = Asb_py::new(zstd.clone());
            if let Ok(some_data) = asb.text_to_binary(text) {
                rawdata = some_data;
                let p = Pathlib::new(file_path);
                let name = &p.stem;
                let rawdata_clone = rawdata.clone(); // Clone rawdata to avoid borrowing issues
                let parts: Vec<&[u8]> = if let Some(pos) = rawdata_clone.windows(ASB_SEPARATOR.len()).position(|window| window == ASB_SEPARATOR) {
                    vec![&rawdata_clone[..pos], &rawdata_clone[pos + ASB_SEPARATOR.len()..]]
                } else {
                    vec![&rawdata_clone[..], &[]]
                };

                rawdata = parts.get(0).copied().unwrap_or(&[]).to_vec(); //asb data
                let mut baev_data = parts.get(1).copied().unwrap_or(&[]).to_vec();
                

                if is_zs {
                    rawdata = zstd.compress_zs(&rawdata).ok()?;
                }
                //save baev
                if !baev_data.is_empty() {
                    let baev_path = Path::new(&p.parent).join(format!("{}.root.baev.zs", name));
                    baev_data = zstd.compress_zs(&baev_data).ok()?;
                    if let Ok(mut file) = File::create(&baev_path) {
                        if let Err(e) = file.write_all(&baev_data) {
                            println!("Error writing baev data: {}", e);
                        } else {
                            println!("Baev data saved successfully to {}", baev_path.display());
                        }
                    } else {
                        println!("Error creating baev file");
                    }
                }
            }
        }
        TotkFileType::AINB => {
            if let Ok(some_data) = Ainb_py::new().text_to_binary(text) {
                rawdata = some_data;
            }
        }
        TotkFileType::TagProduct => {
            if let Ok(some_data) = TagProduct::to_binary(text) {
                rawdata = some_data;
                if is_zs {
                    rawdata = zstd.compress_zs(&rawdata).ok()?;
                }
            }
        }
        TotkFileType::Byml => {
            if (is_gamedatalist(file_path)) {
                println!("is_gamedatalist, attempting to use oead python");
                let p_wrap = PythonWrapper::new();
                match p_wrap.text_to_binary(&text.as_bytes().to_vec(), "byml_text_to_binary".to_string()) {
                    Ok(some_data) => {
                        rawdata = some_data;
                        println!("it worked");
                    }
                    Err(e) => {
                        println!("Error: {}", e);
                    }
                }
            }
            if (rawdata.is_empty()) {
                let processed_text = if is_banc_path(&file_path) && zstd.totk_config.rotation_deg {
                    &replace_rotate_deg_to_rad(&text)
                } else {
                    text
                };
                
                let pio = Byml::from_text(processed_text).ok()?;
                rawdata = pio.to_binary(endian);
            }
            if (!rawdata.is_empty()) {
                if is_bcett {
                    rawdata = zstd.compress_bcett(&rawdata).ok()?;
                } else if is_zs {
                    rawdata = zstd.compress_zs(&rawdata).ok()?;
                }
            }
        }
        TotkFileType::Bcett => {
            let processed_text = if zstd.totk_config.rotation_deg {&replace_rotate_deg_to_rad(&text)} else {text};
            let pio = Byml::from_text(processed_text).ok()?;
            rawdata = pio.to_binary(endian);
            if is_zs {
                rawdata = zstd.compress_bcett(&rawdata).ok()?;
            }
        }
        TotkFileType::Msbt => {
            let result = MsbtCpp::from_text(text, endian_str.to_string());
            if let Ok(msbt) = result {
                rawdata = msbt.binary;
            }
        }
        TotkFileType::Aamp => {
            let pio = ParameterIO::from_text(text).ok()?;
            rawdata = pio.to_binary();
        }
        TotkFileType::SmoSaveFile => {
            let mut smo_file = SmoSaveFile::from_string(text, zstd.clone()).ok()?;
            smo_file.endian = endian;
            rawdata = smo_file.to_binary().ok()?;
        }
        TotkFileType::Text => {
            rawdata = text.as_bytes().to_vec();
        }
        _ => {}
    }

    Some(rawdata)
}

pub struct SaveFileDialog<'a> {
    pub tab: String,
    pub pack: &'a Option<PackComparer<'a>>,
    pub opened_file: &'a OpenedFile<'a>,
    pub title: String,
    pub name: Option<String>,
    pub filters: BTreeMap<String, Vec<String>>,
    pub isText: bool,
}
impl SaveFileDialog<'_> {
    pub fn new<'a>(
        tab: String,
        pack: &'a Option<PackComparer<'a>>,
        opened_file: &'a OpenedFile<'a>,
        title: String,
    ) -> SaveFileDialog<'a> {
        SaveFileDialog {
            tab: tab,
            pack: pack,
            opened_file: opened_file,
            title: title,
            name: None,
            filters: Default::default(),
            isText: false,
        }
    }
    pub fn process_name(&mut self) {
        self.name = None;
        match self.tab.as_str() {
            "SARC" => {
                if let Some(pack) = self.pack {
                    if let Some(opened) = &pack.opened {
                        self.name = Some(opened.path.name.clone());
                    }
                }
            }
            "YAML" => {
                self.name = Some(self.opened_file.path.name.clone());
            }
            _ => {}
        }
    }

    pub fn filters_from_path(&mut self, file_path: &str) {
        let path = Pathlib::new(file_path.to_string());
        let x = if path.ext_last.is_empty() {
            vec![path.extension.clone()]
        } else {
            vec![path.extension.clone(), path.ext_last.clone()]
        };
        let y = if path.ext_last.is_empty() {
            path.extension.clone().to_uppercase()
        } else {
            path.ext_last.clone().to_uppercase()
        };

        self.filters.insert(y, x);
    }

    pub fn generate_filters(&mut self) {
        let mut filters: BTreeMap<String, Vec<String>> = BTreeMap::new();
        match self.tab.as_str() {
            "SARC" => {
                filters.insert(
                    "SARC".to_string(),
                    vec![
                        "pack".to_string(),
                        "sarc".to_string(),
                        "pack.zs".to_string(),
                        "sarc.zs".to_string(),
                    ],
                );
            }
            "YAML" => {
                let exts = if self.opened_file.path.ext_last.is_empty() {
                    vec![self.opened_file.path.extension.clone()]
                } else {
                    vec![
                        self.opened_file.path.extension.clone(),
                        self.opened_file.path.ext_last.clone(),
                    ]
                };
                filters.insert(
                    //own extension
                    format!("{:?}", self.opened_file.file_type),
                    exts,
                );
                filters.insert(
                    "Text Files".to_string(),
                    vec![
                        "yaml".to_string(),
                        "json".to_string(),
                        "yml".to_string(),
                        "txt".to_string(),
                    ],
                );
            }
            _ => {} // Add a wildcard pattern to cover all other cases
        }
        // filters.insert("All Files".to_string(), vec!["*".to_string()]);
        self.filters = filters;
    }
    pub fn generate_filters_and_name(&mut self) {
        self.generate_filters();
        self.process_name();
    }

    pub fn show(&mut self) -> String {
        // self.generate_filters();
        // self.process_name();
        let mut result = String::new();
        let mut dialog = FileDialog::new()
            .set_file_name(self.name.clone().unwrap_or_default())
            .set_title(&self.title);
        for (key, value) in &self.filters {
            dialog = dialog.add_filter(key, value);
        }
        let file = dialog
            .add_filter("All files", &vec!["*".to_string()])
            .save_file();
        if let Some(res) = file {
            result = res.to_string_lossy().into_owned();
        }
        self.isText = vec![".txt", ".yaml", ".json", ".yml"].iter().any(|ext| result.to_lowercase().ends_with(ext));
        result.replace("\\", "/")
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SendData {
    pub text: String,
    pub path: Pathlib,
    pub file_label: String,
    pub status_text: String,
    pub tab: String,
    pub rstb_paths: Vec<serde_json::Value>,
    pub sarc_paths: SarcPaths,
    pub lang: String,
    pub compare_data: DiffComparer
}

impl Default for SendData {
    fn default() -> Self {
        Self {
            text: "".to_string(),
            path: Pathlib::default(),
            file_label: "".to_string(),
            status_text: "".to_string(),
            tab: "YAML".to_string(),
            rstb_paths: Vec::default(),
            sarc_paths: SarcPaths::default(),
            lang: "yaml".to_string(),
            compare_data: DiffComparer::default()
        }
    }
}
impl SendData {
    pub fn get_file_label(&mut self, filetype: TotkFileType, endian: Option<roead::Endian>) {
        let mut e = String::new();
        if let Some(endian) = endian {
            e = match endian {
                roead::Endian::Big => "BE".to_string(),
                roead::Endian::Little => "LE".to_string(),
            };
        }
        if !e.is_empty() {
            self.file_label = format!("{} [{:?}] [{}]", self.path.name, filetype, e)
        } else {
            self.file_label = format!("{} [{:?}]", self.path.name, filetype)
        }
    }
    pub fn get_sarc_paths(&mut self, pack: &PackComparer<'_>) {
        if let Some(opened) = &pack.opened {
            for file in opened.sarc.files() {
                if let Some(name) = file.name {
                    self.sarc_paths.paths.push(name.into());
                }
            }
            for (path, _) in pack.added.iter() {
                self.sarc_paths.added_paths.push(path.into());
            }
            for (path, _) in pack.modded.iter() {
                self.sarc_paths.modded_paths.push(path.into());
            }
            self.sarc_paths
                .paths
                .sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

            if self.sarc_paths.paths.len() == self.sarc_paths.added_paths.len() {
                self.sarc_paths.added_paths.clear(); //avoid all files be lit blue as added
                self.sarc_paths.modded_paths.clear(); //redundant
                return; //skip sorting empty lists
            }

            self.sarc_paths
                .added_paths
                .sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
            self.sarc_paths
                .modded_paths
                .sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        }
        //println!("Sarc paths: {:?}", self.sarc_paths);
    }
}


pub fn file_from_disk_to_senddata_determine_ext<P: AsRef<Path>>(path: P, zstd: Arc<TotkZstd>) -> Option<(OpenedFile, SendData)> {
    //Instead of checking each file type, we check the extension and call the appropriate function
    let file_name = path.as_ref();//.to_string_lossy().to_string().replace("\\", "/");
    let mut p = path.as_ref().to_string_lossy().to_string().to_lowercase();
    if p.ends_with(".zs") {
        p = p[0..p.len()-3].to_string();
    }
    let path = Pathlib::new(&p);
    if is_tagproduct_path(&file_name) {
        return TagProduct::open_tag(&file_name, zstd.clone());
    }
    if is_esetb_path(&file_name) {
        return Esetb::open_esetb(&file_name, zstd.clone());
    }
    if is_xlink_path(&p) {
        return Xlink_rs::open_xlink(&file_name, zstd.clone());
    }
    if is_ainb_path(&p) {
        return Ainb_py::open_ainb(&file_name, zstd.clone());
    }
    if is_rstb_path(&p) {
        return Restbl::open_restbl(&file_name, zstd.clone());
    }
    if is_asb_path(&p) {
        return Asb_py::open_asb(&file_name, zstd.clone());
    }
    if is_byml_path(&p) {
        return BymlFile::open_byml(&file_name, zstd.clone());
    }
    if is_msbt_path(&p) {
        return open_msbt(&file_name);
    }
    if is_evfl_path(&p) {
        return Evfl::open_file(&file_name, zstd.clone());
    }
    
    None // if fails, proceed to regular file open
}

pub fn file_from_disk_to_senddata<P: AsRef<Path>>(path: P, zstd: Arc<TotkZstd>) -> Option<(OpenedFile, SendData)> {
    let file_name = path.as_ref();//.to_string_lossy().to_string().replace("\\", "/");
    // let res = open_tag(&file_name, zstd.clone())
    let res = file_from_disk_to_senddata_determine_ext(&file_name, zstd.clone())
                .or_else(|| TagProduct::open_tag(&file_name, zstd.clone()))
                .or_else(|| Esetb::open_esetb(&file_name, zstd.clone()))
                .or_else(|| Xlink_rs::open_xlink(&file_name, zstd.clone()))
                .or_else(|| Restbl::open_restbl(&file_name, zstd.clone()))
                .or_else(|| Asb_py::open_asb(&file_name, zstd.clone()))
                .or_else(|| Ainb_py::open_ainb(&file_name, zstd.clone()))
                .or_else(|| BymlFile::open_byml(&file_name, zstd.clone()))
                .or_else(|| open_msbt(&file_name))
                .or_else(|| open_aamp(&file_name))
                .or_else(|| Evfl::open_file(&file_name, zstd.clone()))
                .or_else(|| SmoSaveFile::open_smo_save_file(&file_name, zstd.clone()))
                .or_else(|| open_text(&file_name))
                .map(|(opened_file, data)| {
                    // self.opened_file = opened_file;
                    // self.internal_file = None;
                    (opened_file, data)
                });
    res
}