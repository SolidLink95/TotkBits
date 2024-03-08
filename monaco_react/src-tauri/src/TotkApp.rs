use std::io::Read;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::{fs, io, panic};

use msbt::Msbt;
use msyt::converter::MsytFile;
use rfd::FileDialog;
use roead::byml::Byml;
use serde::{Deserialize, Serialize};
use tauri::{window, Manager};

use crate::file_format::BinTextFile::{BymlFile, OpenedFile};
use crate::file_format::Msbt::MsbtFile;
use crate::file_format::Pack::{self, PackComparer, PackFile, SarcPaths};
use crate::file_format::TagProduct::TagProduct;
use crate::Settings::{read_string_from_file, Pathlib};
use crate::TotkConfig::TotkConfig;
use crate::Zstd::{TotkFileType, TotkZstd};

#[derive(PartialEq, Clone, Copy)]
pub enum ActiveTab {
    DiretoryTree,
    TextBox,
    Settings,
}

pub struct TotkBitsApp<'a> {
    pub opened_file: OpenedFile<'a>, //path to opened file in string
    pub text: String,
    pub status_text: String,
    pub active_tab: ActiveTab, //active tab, either sarc file or text editor
    pub zstd: Arc<TotkZstd<'a>>,
    pub pack: Option<PackComparer<'a>>,
    pub internal_file: Option<InternalFile<'a>>,
}

impl Default for TotkBitsApp<'_> {
    fn default() -> Self {
        let totk_config: Arc<TotkConfig> = Arc::new(TotkConfig::new());
        let zstd: Arc<TotkZstd<'_>> = Arc::new(TotkZstd::new(totk_config, 16).unwrap());
        Self {
            opened_file: OpenedFile::default(),
            text: "".to_string(),
            status_text: "Ready".to_string(),
            active_tab: ActiveTab::DiretoryTree,
            zstd: zstd.clone(),
            pack: None,
            internal_file: None,
        }
    }
}

impl<'a> TotkBitsApp<'a> {
    pub fn send_status_text(&self) -> String {
        self.status_text.to_string()
    }

    pub fn open(&mut self) -> Option<SendData> {
        let mut data = SendData::default();
        if let Some(file) = FileDialog::new()
            .set_title("Choose file to open")
            .pick_file()
        {
            let file_name = file.to_string_lossy().to_string().replace("\\", "/");
            if check_if_filepath_valid(&file_name) {
                println!("Is {} a sarc?", &file_name);
                let sarc = PackFile::new(file_name.clone(), self.zstd.clone());
                if sarc.is_ok() {
                    println!("{} is a sarc", &file_name);
                    let s = sarc.as_ref().unwrap();
                    let endian = s.endian.clone();
                    self.pack = PackComparer::from_pack(sarc.unwrap(), self.zstd.clone());
                    data.get_sarc_paths(self.pack.as_ref().unwrap());
                    data.status_text = format!("Opened {}", &file_name);
                    self.internal_file = None;
                    data.path = Pathlib::new(file_name.clone());
                    data.text = "SARC".to_string();
                    data.tab = "SARC".to_string();
                    data.get_file_label(TotkFileType::Sarc, Some(endian));
                    return Some(data);
                }
                println!("Is {} a tag?", &file_name);
                if Pathlib::new(file_name.clone())
                    .name
                    .to_lowercase()
                    .starts_with("tag.product")
                {
                    println!("{} is a tag", &file_name);
                    self.opened_file.tag = TagProduct::new(file_name.clone(), self.zstd.clone());
                    if let Some(tag) = &mut self.opened_file.tag {
                        self.internal_file = None; //opened tag occupies yaml editor, thus pushes back the internal file if it exists
                        self.opened_file.path = Pathlib::new(file_name.clone());
                        self.opened_file.endian = Some(roead::Endian::Little);
                        self.opened_file.file_type = TotkFileType::TagProduct;
                        //set others to None, just to be sure
                        self.opened_file.msyt = None;
                        self.opened_file.byml = None;
                        self.opened_file.aamp = None;
                        data.status_text = format!("Opened {}", &file_name);
                        data.path = Pathlib::new(file_name.clone());
                        data.text = tag.to_text();
                        data.get_file_label(TotkFileType::TagProduct, Some(roead::Endian::Little));
                        return Some(data);
                    }
                }

                println!("Is {} a byml?", &file_name);
                self.opened_file.byml = BymlFile::new(file_name.clone(), self.zstd.clone());
                if self.opened_file.byml.is_some() {
                    let b = self.opened_file.byml.as_ref().unwrap();
                    println!("{} is a byml", &file_name);
                    self.internal_file = None; //opened byml occupies yaml editor, thus pushes back the internal file if it exists
                    self.opened_file.path = Pathlib::new(file_name.clone());
                    self.opened_file.endian = self.opened_file.byml.as_ref().unwrap().endian;
                    self.opened_file.file_type = b.file_data.file_type.clone();
                    //set others to None, just to be sure
                    self.opened_file.msyt = None;
                    self.opened_file.tag = None;
                    self.opened_file.aamp = None;
                    data.status_text = format!("Opened {}", &file_name);
                    data.path = Pathlib::new(file_name.clone());
                    data.text = Byml::to_text(&b.pio);
                    data.get_file_label(
                        b.file_data.file_type,
                        b.endian,
                    );
                    return Some(data);
                }
                println!("Is {} a msbt?", &file_name);
                self.opened_file.msyt = MsbtFile::from_filepath(&file_name);
                if self.opened_file.msyt.is_some() {
                    let m = self.opened_file.msyt.as_ref().unwrap();
                    println!("{} is a msbt", &file_name);
                    self.internal_file = None; //opened byml occupies yaml editor, thus pushes back the internal file if it exists
                                               //opened file
                    self.opened_file.path = Pathlib::new(file_name.clone());
                    self.opened_file.endian = Some(m.endian);
                    self.opened_file.file_type = TotkFileType::Msbt;
                    //set others to None, just to be sure
                    self.opened_file.byml = None;
                    self.opened_file.tag = None;
                    self.opened_file.aamp = None;
                    data.status_text = format!("Opened {}", &file_name);
                    data.path = Pathlib::new(file_name.clone());
                    data.text = m.text.clone();
                    data.get_file_label(self.opened_file.file_type, Some(m.endian));
                    return Some(data);
                }
                println!("Is {} regular text file?", &file_name);
                let mut file = fs::File::open(&file_name).ok()?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer).ok()?;
                if let Ok(text) = String::from_utf8(buffer) {
                    println!("{} is a text file", &file_name);
                    self.internal_file = None; //opened text file occupies yaml editor, thus pushes back the internal file if it exists
                    data.status_text = format!("Opened {}", &file_name);
                    data.path = Pathlib::new(file_name.clone());
                    data.text = text;
                    data.get_file_label(TotkFileType::Text, None);
                    return Some(data);
                }
                println!("Is {} an aamp?", &file_name);
                //placeholder for aamp
            }
            data.status_text = format!("Error: Failed to open {}", &file_name);
        }

        None
    }
}

pub struct InternalFile<'a> {
    pub path: Pathlib,
    pub endian: roead::Endian,
    pub byml: Option<BymlFile<'a>>,
    pub msyt: Option<String>,
    pub aamp: Option<()>,
}

impl Default for InternalFile<'_> {
    fn default() -> Self {
        Self {
            path: Pathlib::default(),
            endian: roead::Endian::Little,
            byml: None,
            msyt: None,
            aamp: None,
        }
    }
}

impl InternalFile<'_> {
    pub fn new(path: String) -> Self {
        let path = Pathlib::new(path);
        Self {
            path,
            endian: roead::Endian::Little,
            byml: None,
            msyt: None,
            aamp: None,
        }
    }
}

pub fn check_if_filepath_valid(path: &str) -> bool {
    fn inner_func(path: &str) -> io::Result<()> {
        let path = Path::new(path);
        if !(path.exists() && path.is_file()) {
            return io::Result::Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("File {:?} does not exist", path),
            ));
        }
        let mut f_handle = fs::File::open(path)?;
        let mut buffer: Vec<u8> = Vec::new(); //String::new();
        if let Err(_err) = &f_handle.read_to_end(&mut buffer) {
            return io::Result::Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unable to open {:?}", path),
            ));
        }
        Ok(())
    }
    if path.is_empty() {
        return false;
    }
    return inner_func(path).is_ok();
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SendData {
    pub text: String,
    pub path: Pathlib,
    pub file_label: String,
    pub status_text: String,
    pub tab: String,
    pub sarc_paths: SarcPaths,
}

impl Default for SendData {
    fn default() -> Self {
        Self {
            text: "".to_string(),
            path: Pathlib::default(),
            file_label: "".to_string(),
            status_text: "".to_string(),
            tab: "YAML".to_string(),
            sarc_paths: SarcPaths::default(),
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
            self.sarc_paths
                .paths
                .sort_by_key(|s| (s.to_lowercase(), s.clone()));
            for (path, _) in pack.added.iter() {
                self.sarc_paths.added_paths.push(path.into());
            }
            for (path, _) in pack.modded.iter() {
                self.sarc_paths.modded_paths.push(path.into());
            }
        }
        //println!("Sarc paths: {:?}", self.sarc_paths);
    }
}

//tauri commands

#[tauri::command]
pub fn get_status_text(app: tauri::State<'_, TotkBitsApp>) -> String {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        app.inner().send_status_text();
    }));
    if result.is_err() {
        return "Error".to_string();
    }
    app.status_text.clone()
}

#[tauri::command]
pub fn open_file(app_handle: tauri::AppHandle) -> Result<Option<String>, String> {
    // Lock the mutex to get mutable access to your state
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");

    match app.open() {
        Some(result) => Ok(Some(result.text)), // Safely return the result if present
        None => Ok(None),                      // Return None if no result
    }
}

#[tauri::command]
pub fn open_file_struct(app_handle: tauri::AppHandle, window: tauri::Window) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.open() {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}
