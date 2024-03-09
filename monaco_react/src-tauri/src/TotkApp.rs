use crate::file_format::BinTextFile::{BymlFile, OpenedFile};
use crate::file_format::Pack::{PackComparer, PackFile, SarcPaths};
use crate::Open_and_Save::{
    check_if_save_in_romfs, get_binary_by_filetype, get_string_from_data, open_aamp, open_byml,
    open_msbt, open_sarc, open_tag, open_text, save_file_dialog,
};
use crate::Settings::Pathlib;
use crate::TotkConfig::TotkConfig;
use crate::Zstd::{TotkFileType, TotkZstd};
use msyt::converter::MsytFile;
use rfd::{FileDialog, MessageDialog};
use roead::aamp::ParameterIO;
use roead::byml::Byml;
use serde::{de, Deserialize, Serialize};
use std::io::{Read, Write};
use std::os::raw;
use std::path::Path;
use std::sync::Arc;
use std::{fs, io};

#[derive(PartialEq, Clone, Copy)]
pub enum ActiveTab {
    DiretoryTree,
    TextBox,
    Settings,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SaveData {
    pub tab: String,
    pub text: String,
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

    pub fn save_as(&mut self) -> Option<SendData> {
        let mut data = SendData::default();
        let mut prob_file_name = String::new();
        if self.opened_file.path.full_path.len() > 0 {
            prob_file_name = self.opened_file.path.name.clone();
        }
        let dest_file = save_file_dialog(Some(prob_file_name));
        if !dest_file.is_empty() {
            //check if file is saved in romfs
            if check_if_save_in_romfs(&dest_file, self.zstd.clone()) {
                return None;
            }
        }

        None
    }

    pub fn save(&mut self, save_data: SaveData) -> Option<SendData> {
        println!("About to save {} text len {}", save_data.tab, save_data.text.len());
        let mut data = SendData::default();
        let mut isReload = false;
        let text = &save_data.text;

        if let Some(pack) = &mut self.pack {
            if let Some(opened) = &mut pack.opened {
                match save_data.tab.as_str() {
                    "YAML" => {
                        if let Some(internal_file) = &self.internal_file {
                            let path = &internal_file.path.full_path;
                            let rawdata: Vec<u8> = get_binary_by_filetype(
                                internal_file.file_type,
                                text,
                                internal_file.endian.unwrap_or(roead::Endian::Little),
                            )?;
                            if rawdata.is_empty() {
                                data.status_text = format!(
                                    "Error: Failed to save {} for {}",
                                    &path, &opened.path.name
                                );
                                data.tab = "ERROR".to_string();
                                return Some(data);
                            } else {
                                opened.writer.add_file(path, rawdata);
                                isReload = true;
                                data.tab = "YAML".to_string();
                                data.status_text = format!(
                                    "Saved {} for {}",
                                    &internal_file.path.name, &opened.path.name
                                );
                            }
                        }
                    }
                    "SARC" => {
                        if check_if_save_in_romfs(&opened.path.full_path, self.zstd.clone()) {
                            return None;
                        }
                        opened.reload();
                        if let Ok(res) = opened.save_default() {
                            println!("Saved SARC {}", &opened.path.full_path);
                        } else {
                            println!("ERROR SAVING SARC {}", &opened.path.full_path);

                        }
                        isReload = true;
                        data.tab = "SARC".to_string();
                        data.status_text = format!("Saved SARC {}", &opened.path.full_path);
                    }
                    _ => {
                        data.status_text = format!("Error: Unsupported tab {}", &save_data.tab);
                    }
                }
            }
            if isReload {
                pack.compare_and_reload();
                data.get_sarc_paths(pack);
            }
            return Some(data);
        }
        if save_data.tab.as_str() == "YAML" {
            let rawdata: Vec<u8> = get_binary_by_filetype(
                self.opened_file.file_type,
                text,
                self.opened_file.endian.unwrap_or(roead::Endian::Little),
            )?;
            if rawdata.is_empty() {
                data.status_text = format!("Error: Failed to save {}", &self.opened_file.path.full_path);
                data.tab = "ERROR".to_string();
                return Some(data);
            } else {
                let mut file = fs::File::create(&self.opened_file.path.full_path).ok()?;
                file.write_all(&rawdata).ok()?;
                data.tab = "YAML".to_string();
                data.status_text = format!("Saved {}", &self.opened_file.path.full_path);
                return Some(data);
            }
        }

        None
    }

    pub fn open_internal_file(&mut self, path: String) -> Option<SendData> {
        if path.is_empty() || !path.contains(".") {
            return None;
        }
        let mut data = SendData::default();
        if let Some(pack) = &mut self.pack {
            if let Some(opened) = &mut pack.opened {
                let raw_data = opened.sarc.get_data(&path);
                if let Some(raw_data) = raw_data {
                    //if let Some(res) = get_string_from_data(path, raw_data.to_vec(), self.zstd.clone()) {
                    if let Some((intern, text)) =
                        get_string_from_data(path.clone(), raw_data.to_vec(), self.zstd.clone())
                    {
                        self.internal_file = Some(intern);
                        let i = self.internal_file.as_ref().unwrap();
                        data.text = text;
                        data.path = i.path.clone();
                        data.status_text = format!(
                            "Opened {} [{:?}] from {}",
                            &i.path.name, &i.file_type, &opened.path.name
                        );
                        data.tab = "YAML".to_string();
                        data.get_file_label(i.file_type, i.endian);
                        return Some(data);
                    } else {
                        data.status_text =
                            format!("Error: unsupported data type for {}", &path.clone());
                    }
                } else {
                    data.status_text = format!("Error: {} absent in {}", &path, &opened.path.name);
                }
                data.tab = "ERROR".to_string();
                return Some(data);
            }
        }

        None
    }

    pub fn open(&mut self) -> Option<SendData> {
        let mut data = SendData::default();
        if let Some(file) = FileDialog::new()
            .set_title("Choose file to open")
            .pick_file()
        {
            let file_name = file.to_string_lossy().to_string().replace("\\", "/");
            if check_if_filepath_valid(&file_name) {
                if let Some((pack, data)) = open_sarc(file_name.clone(), self.zstd.clone()) {
                    self.pack = Some(pack);
                    self.internal_file = None;
                    return Some(data);
                }
                let res = open_tag(file_name.clone(), self.zstd.clone())
                    .or_else(|| open_byml(file_name.clone(), self.zstd.clone()))
                    .or_else(|| open_msbt(file_name.clone()))
                    .or_else(|| open_aamp(file_name.clone()))
                    .or_else(|| open_text(file_name.clone()))
                    .map(|(opened_file, data)| {
                        self.opened_file = opened_file;
                        self.internal_file = None;
                        data
                    });
                if res.is_some() {
                    return res;
                }
            } else {
                return None;
            }
            data.tab = "ERROR".to_string();
            data.status_text = format!("Error: Failed to open {}", &file_name);
            return Some(data);
        }
        None
    }
}

pub struct InternalFile<'a> {
    pub path: Pathlib,
    pub file_type: TotkFileType,
    pub endian: Option<roead::Endian>,
    pub byml: Option<BymlFile<'a>>,
    pub msyt: Option<String>,
    pub text: Option<String>,
    pub aamp: Option<String>,
}

impl Default for InternalFile<'_> {
    fn default() -> Self {
        Self {
            path: Pathlib::default(),
            file_type: TotkFileType::None,
            endian: None,
            byml: None,
            msyt: None,
            text: None,
            aamp: None,
        }
    }
}

impl InternalFile<'_> {
    pub fn new(path: String) -> Self {
        let path = Pathlib::new(path);
        Self {
            path,
            file_type: TotkFileType::None,
            endian: None,
            byml: None,
            msyt: None,
            text: None,
            aamp: None,
        }
    }
}

pub fn check_if_filepath_valid(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    let path = Path::new(path);
    path.exists() && path.is_file()
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
                .sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
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
