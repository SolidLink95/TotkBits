use crate::file_format::BinTextFile::{BymlFile, OpenedFile};
use crate::file_format::Pack::{PackComparer, PackFile, SarcPaths};
use crate::Open_and_Save::{
    check_if_save_in_romfs, get_binary_by_filetype, get_string_from_data, open_aamp, open_byml,
    open_msbt, open_sarc, open_tag, open_text, save_file_dialog, SaveFileDialog, SendData,
};
use crate::Settings::{write_string_to_file, Pathlib};
use crate::TotkConfig::TotkConfig;
use crate::Zstd::{TotkFileType, TotkZstd};
use msyt::converter::MsytFile;
use rfd::{FileDialog, MessageDialog};
use roead::aamp::ParameterIO;
use roead::byml::Byml;
use serde::{de, Deserialize, Serialize};
use std::io::{Read, Write};
use std::os::raw;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{env, fs, io};

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
    pub fn process_argv(&mut self) -> Option<SendData> {
        let mut data = SendData::default();
        let args: Vec<String> = env::args().collect();
        if args.len() > 1 {
            let res = self.open_from_path(args[1].clone());
            if let Some(r) = &res {
                if data.tab.as_str() != "ERROR" {
                    return res;
                }
            }
        }
        None
    }

    pub fn remove_internal_elem(&mut self, internal_path: String) -> Option<SendData> {
        let mut data = SendData::default();
        let mut isReload = false;

        if let Some(pack) = &mut self.pack {
            if let Some(opened) = &mut pack.opened {
                isReload = true;
                if let Some(_) = opened.writer.get_file(&internal_path) {
                    //its a file
                    if MessageDialog::new()
                        .set_title("Remove file")
                        .set_description(format!("The file:\n{}\nwill be removed. This operation cannot be reverted! Proceed?", &internal_path))
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show()
                        == rfd::MessageDialogResult::No
                    {
                        return None;
                    }
                    opened.writer.remove_file(&internal_path);
                    data.status_text = format!("Removed {}", &internal_path);
                } else {
                    //its a directory
                    if MessageDialog::new()
                    .set_title("Remove directory")
                    .set_description(format!("All files from directory:\n{}\nwill be removed. This operation cannot be reverted! Proceed?", &internal_path))
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    == rfd::MessageDialogResult::No
                {
                    return None;
                }
                    let mut to_remove: Vec<String> = Vec::new();
                    for file in opened.writer.files.keys() {
                        if file.starts_with(&internal_path) {
                            to_remove.push(file.clone());
                        }
                    }
                    let mut i: usize = 0;
                    for file in to_remove {
                        opened.writer.remove_file(&file);
                        i += 1;
                    }
                    data.status_text = format!("Removed {} files from {}", i, &internal_path);
                }
            }
            if isReload {
                pack.compare_and_reload();
                data.get_sarc_paths(pack);
            }
            return Some(data);
        }

        None
    }

    pub fn close_all_click(&mut self) -> Option<SendData> {
        if MessageDialog::new()
            .set_title("Close all")
            .set_description("All currently opened files will be closed. Proceed?")
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            == rfd::MessageDialogResult::No
        {
            return None;
        }
        let mut data = SendData::default();
        self.opened_file = OpenedFile::default();
        self.text = String::new();
        self.status_text = "Ready".to_string();
        self.active_tab = ActiveTab::DiretoryTree;
        self.pack = None;
        self.internal_file = None;
        data.status_text = "Closed all opened files".to_string();
        data.sarc_paths = SarcPaths::default();
        Some(data)
    }

    pub fn send_status_text(&self) -> String {
        self.status_text.to_string()
    }

    pub fn get_binary_for_opened_file(&self, text: &str) -> Option<Vec<u8>> {
        get_binary_by_filetype(
            self.opened_file.file_type,
            text,
            self.opened_file.endian.unwrap_or(roead::Endian::Little),
        )
    }

    pub fn extract_file(&mut self, internal_path: String) -> Option<SendData> {
        let mut dialog = SaveFileDialog::new(
            "YAML".to_string(),
            &None,
            &self.opened_file,
            "Choose save location".to_string(),
        );
        dialog.name = Some(Pathlib::new(internal_path.clone()).name);
        dialog.filters_from_path(&internal_path);
        let path = dialog.show();
        if path.is_empty() {
            return None;
        }

        let mut data = SendData::default();
        if let Some(pack) = &mut self.pack {
            if let Some(opened) = &mut pack.opened {
                if let Some(rawdata) = opened.writer.get_file(&internal_path) {
                    let mut file = fs::File::create(&path).ok()?;
                    file.write_all(&rawdata).ok()?;
                    data.status_text = format!("Extracted {} to {}", &internal_path, &path);
                    return Some(data);
                }
                data.status_text = format!(
                    "Error: {} not found in {}",
                    &internal_path, &opened.path.name
                );
                return Some(data);
            }
        }
        None
    }

    pub fn rename_internal_file_from_path(
        &mut self,
        internal_path: String,
        new_internal_path: String,
    ) -> Option<SendData> {
        let mut data = SendData::default();
        let p1 = Pathlib::new(internal_path.clone());
        let p2 = Pathlib::new(new_internal_path.clone());
        println!(
            "trying to rename  {} to sarc path {}",
            &new_internal_path, &internal_path
        );
        let mut isReload = false;
        if let Some(pack) = &mut self.pack {
            if let Some(opened) = &mut pack.opened {
                isReload = true;
                //file is in sarc
                if let Some(rawdata) = opened.writer.get_file(&internal_path) {
                    let rawdata_backup = rawdata.clone();
                    let new_path = format!("{}/{}", &p1.parent, &p2.name);
                    opened.writer.remove_file(&internal_path);
                    opened.writer.add_file(&new_path, rawdata_backup);
                    data.status_text = format!("Renamed {} to {}", &p1.name, &p2.name);
                } else {
                    //assuming the node is a directory
                    let mut to_remove: Vec<String> = Vec::new();
                    for file in opened.writer.files.keys() {
                        if file.starts_with(&internal_path) {
                            to_remove.push(file.clone());
                        }
                    }
                    println!("{:?}", to_remove);
                    let mut i: usize = 0;
                    for file in to_remove {
                        if let Some(rawdata) = opened.writer.get_file(&file) {
                            let input = "asdf.qwre.zxcv";
                            let result: String =
                                input.split('.').skip(1).collect::<Vec<&str>>().join(".");

                            let rawdata_backup = rawdata.clone();
                            let new_path = format!("{}/{}", &p1.parent, &p2.name);
                            // let new_file_path = file.replace(&internal_path, &new_path);
                            let tmp = file
                                .split(&internal_path)
                                .skip(1)
                                .collect::<Vec<&str>>()
                                .join(&internal_path);
                            let mut new_file_path = format!("{}/{}", &new_path, &tmp);
                            if new_file_path.starts_with("/") {
                                new_file_path = new_file_path[1..].to_string();
                            }
                            new_file_path = new_file_path.replace("//", "/");
                            println!("{} -> {}", &file, &new_file_path);
                            opened.writer.remove_file(&file);
                            opened.writer.add_file(&new_file_path, rawdata_backup);
                            i += 1;
                        }
                    }
                    data.status_text = format!(
                        "Renamed {} to {} ({} files affected)",
                        &p1.name, &p2.name, i
                    );
                }
            }
            if isReload {
                pack.compare_and_reload();
                data.get_sarc_paths(pack);
            }
            return Some(data);
        }
        None
    }

    pub fn add_internal_file_from_path(
        &mut self,
        internal_path: String,
        path: String,
        overwrite: bool,
    ) -> Option<SendData> {
        let mut data = SendData::default();
        println!("trying to add  {} to sarc path {}", &path, &internal_path);
        let mut isReload = false;
        if let Some(pack) = &mut self.pack {
            if let Some(opened) = &mut pack.opened {
                if let Some(_) = &opened.writer.get_file(&internal_path) {
                    if !overwrite {
                        let m = format!(
                            "{}\nalready exists in {}. Proceed?",
                            &internal_path, &opened.path.name
                        );
                        if MessageDialog::new()
                            .set_title("File already exists")
                            .set_description(m)
                            .set_buttons(rfd::MessageButtons::YesNo)
                            .show()
                            == rfd::MessageDialogResult::No
                        {
                            return None;
                        }
                    }
                }

                isReload = true;
                let mut f_handle = fs::File::open(&path).ok()?;
                let mut buffer: Vec<u8> = Vec::new();
                f_handle.read_to_end(&mut buffer).ok()?;
                opened
                    .writer
                    .add_file(&internal_path.replace("\\", "/"), buffer);
                data.status_text = if overwrite {
                    format!(
                        "Replaced {} with {}",
                        &internal_path,
                        &Pathlib::new(path).name
                    )
                } else {
                    format!("Added {} to {}", &internal_path, &opened.path.name)
                };
            }
            if isReload {
                pack.compare_and_reload();
                data.get_sarc_paths(pack);
            }
            return Some(data);
        }
        println!(
            "ERROR: unable to add {} to sarc path {}",
            &path, &internal_path
        );
        // println!("{:?}", data);
        None
    }

    pub fn save_as(&mut self, save_data: SaveData) -> Option<SendData> {
        //TODO: FINISH!
        let mut data = SendData::default();
        let mut dialog = SaveFileDialog::new(
            save_data.tab.to_string(),
            &self.pack,
            &self.opened_file,
            //"Save file as".to_string(),
            format!("Save {} as", save_data.tab),
        );
        dialog.generate_filters_and_name();
        if self.opened_file.path.full_path.is_empty() {
            if let Some(internal_file) = &self.internal_file {
                dialog.name = Some(internal_file.path.name.clone());
                dialog.filters_from_path(&internal_file.path.full_path);
            }
        }

        if dialog.name.clone().unwrap_or_default().is_empty() && save_data.tab == "SARC" {
            println!("Nothing is opened, nothing to save");
            return None;
        }
        let dest_file = dialog.show();
        if !dest_file.is_empty() && !check_if_save_in_romfs(&dest_file, self.zstd.clone()) {
            match save_data.tab.as_str() {
                "YAML" => {
                    if dialog.isText {
                        write_string_to_file(&dest_file, &save_data.text).ok()?;
                        data.tab = "YAML".to_string();
                        data.status_text = format!("Saved {}", &dest_file);
                        data.path = Pathlib::new(dest_file.clone());
                        self.opened_file.path = Pathlib::new(dest_file);
                    } else {
                        let rawdata = self.get_binary_for_opened_file(&save_data.text);
                        if let Some(rawdata) = rawdata {
                            let mut file = fs::File::create(&dest_file).ok()?;
                            file.write_all(&rawdata).ok()?;
                            data.tab = "YAML".to_string();
                            data.status_text = format!("Saved {}", &dest_file);
                            data.path = Pathlib::new(dest_file.clone());
                            self.opened_file.path = Pathlib::new(dest_file);
                        } else {
                            data.status_text = format!(
                                "Error: Failed to save [{:?}] {}",
                                self.opened_file.file_type, &dest_file
                            );
                        }
                    }

                    return Some(data);
                }
                "SARC" => {
                    let mut isReload = false;
                    if let Some(pack) = &mut self.pack {
                        if let Some(opened) = &mut pack.opened {
                            match opened.save(dest_file.clone()) {
                                Ok(_) => {
                                    isReload = true;
                                    println!("Saved SARC {}", &dest_file);
                                    data.tab = "SARC".to_string();
                                    data.status_text = format!("Saved SARC {}", &dest_file);
                                    data.path = Pathlib::new(dest_file.clone());
                                    self.opened_file.path = Pathlib::new(dest_file);
                                }
                                Err(_) => {
                                    println!("ERROR SAVING SARC {}", &dest_file);
                                    data.status_text =
                                        format!("Error: Failed to save SARC {}", &dest_file);
                                }
                            }
                        }
                        if isReload {
                            pack.compare_and_reload();
                            data.get_sarc_paths(pack);
                        }
                        return Some(data);
                    }
                    return None; //no sarc opened
                }
                _ => {
                    data.status_text = format!("Error: Unsupported tab {}", &save_data.tab);
                    return Some(data);
                }
            }
        }

        None
    }

    pub fn save(&mut self, save_data: SaveData) -> Option<SendData> {
        println!(
            "About to save {} text len {}",
            save_data.tab,
            save_data.text.len()
        );
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
                                println!("{:?}", &data);
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
                        } else {
                            let rawdata: Vec<u8> = get_binary_by_filetype(
                                self.opened_file.file_type,
                                text,
                                self.opened_file.endian.unwrap_or(roead::Endian::Little),
                            )?;
                            if rawdata.is_empty() {
                                data.status_text = format!(
                                    "Error: Failed to save {}",
                                    &self.opened_file.path.full_path
                                );
                                data.tab = "ERROR".to_string();
                                println!("{:?}", &data);
                                return Some(data);
                            } else {
                                let mut file =
                                    fs::File::create(&self.opened_file.path.full_path).ok()?;
                                file.write_all(&rawdata).ok()?;
                                data.tab = "YAML".to_string();
                                data.status_text =
                                    format!("Saved {}", &self.opened_file.path.full_path);
                                    println!("{:?}", &data);
                                return Some(data);
                            }
                        }
                    }
                    "SARC" => {
                        if check_if_save_in_romfs(&opened.path.full_path, self.zstd.clone()) {
                            return None;
                        }
                        opened.reload();
                        opened.save_default();
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
            println!("{:?}", &data);
            return Some(data);
        }
        

        Some(data)
    }

    pub fn edit_internal_file(&mut self, path: String) -> Option<SendData> {
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
                        self.opened_file = OpenedFile::default();
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

    pub fn open_from_path(&mut self, file_name: String) -> Option<SendData> {
        let mut data = SendData::default();
        //let file_name = file.to_string_lossy().to_string().replace("\\", "/");
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

    pub fn open(&mut self) -> Option<SendData> {
        if let Some(file) = FileDialog::new()
            .set_title("Choose file to open")
            .pick_file()
        {
            return self.open_from_path(file.to_string_lossy().to_string().replace("\\", "/"));
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
