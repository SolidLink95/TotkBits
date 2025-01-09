use crate::file_format::BinTextFile::{BymlFile, OpenedFile};
use crate::file_format::Esetb::Esetb;
use crate::file_format::Pack::{PackComparer, SarcPaths};
use crate::Comparer::DiffComparer;
use crate::Open_and_Save::{
    check_if_save_in_romfs, file_from_disk_to_senddata, get_binary_by_filetype, get_string_from_data, open_sarc, SaveFileDialog, SendData
};
use crate::Settings::{get_default_updater, list_files_recursively, write_string_to_file, Pathlib};
use crate::TotkConfig::TotkConfig;
use crate::Zstd::{TotkFileType, TotkZstd};
use rfd::{FileDialog, MessageDialog};
use roead::byml::Byml;
use serde::{Deserialize, Serialize};
use serde_json::json;
use updater::Updater::Updater;
use std::fs;
use std::io::{Read, Write};

use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SaveData {
    pub tab: String,
    pub text: String,
}

pub struct TotkBitsApp<'a> {
    pub opened_file: OpenedFile<'a>, //path to opened file in string
    pub text: String,
    pub status_text: String,
    pub zstd: Arc<TotkZstd<'a>>,
    // pub zstd_cpp: Arc<ZstdCppCompressor>,
    pub pack: Option<PackComparer<'a>>,
    pub internal_file: Option<InternalFile<'a>>,
}

unsafe impl<'a> Send for TotkBitsApp<'a> {}

impl Default for TotkBitsApp<'_> {
    fn default() -> Self {
        match TotkConfig::safe_new() {
            Ok(conf) => {
                let totk_config: Arc<TotkConfig> = Arc::new(conf);
                match TotkZstd::new(totk_config, 16) {
                    Ok(zstd) => {
                        let zstd: Arc<TotkZstd> = Arc::new(zstd);
                        return Self {
                            opened_file: OpenedFile::default(),
                            text: "".to_string(),
                            status_text: "Ready".to_string(),
                            zstd: zstd.clone(),
                            pack: None,
                            internal_file: None,
                        };
                    }
                    Err(_) => {
                        println!("Error while initializing romfs path");
                        std::process::exit(1);
                    }
                }
            }
            Err(_) => {
                println!("Error while initializing romfs path");
                std::process::exit(2);
            }
        }

        // std::process::exit(3); //should never reach here
    }
}

impl<'a> TotkBitsApp<'a> {
    //RSTB
    pub fn get_rstb_entries_by_query(&mut self, entry: String) -> Option<SendData> {
        let mut data = SendData::default();
        let mut isDefaultAdded = false;
        if let Some(rstb) = &mut self.opened_file.restbl {
            data.tab = "RSTB".to_string();

            let entry_low = entry.to_lowercase();
            for elem in rstb.hash_table.iter() {
                if elem.to_lowercase().contains(&entry_low) {
                    if let Some(val) = rstb.table.get(elem.clone()) {
                        if elem == &entry {
                            isDefaultAdded = true;
                        }
                        data.rstb_paths
                            .push(json!({ "path": elem.clone(), "val": val.to_string() }));
                    }
                }
            }
            if !isDefaultAdded {
                if let Some(val) = rstb.table.get(entry.clone()) {
                    //entry exists
                    data.rstb_paths
                        .push(json!({ "path": entry.clone(), "val": val.to_string() }));
                }
            }
            data.status_text = format!("Found entries: {}", data.rstb_paths.len());
        } else {
            data.status_text = "Error: No RSTB opened".to_string();
            data.tab = "ERROR".to_string();
        }

        Some(data)
    }

    pub fn rstb_edit_entry(&mut self, entry: String, val: String) -> Option<SendData> {
        let mut data = SendData::default();
        if let Some(rstb) = &mut self.opened_file.restbl {
            data.tab = "RSTB".to_string();
            if let Some(_) = rstb.table.get(entry.clone()) {
                //entry exists
                data.status_text = format!("Modified: {}", &entry);
            } else {
                data.status_text = format!("Added: {}", &entry);
            }
            rstb.table
                .set(entry, val.parse::<u32>().expect("Failed to parse value"));
        } else {
            data.status_text = "Error: No RSTB opened".to_string();
            data.tab = "ERROR".to_string();
        }

        Some(data)
    }
    pub fn rstb_remove_entry(&mut self, entry: String) -> Option<SendData> {
        let mut data = SendData::default();
        if let Some(rstb) = &mut self.opened_file.restbl {
            data.tab = "RSTB".to_string();
            if let Some(_) = rstb.table.get(entry.clone()) {
                //entry exists
                data.status_text = format!("Removed: {}", &entry);
                rstb.table.remove(entry.clone());
            } else {
                data.status_text = format!("Error: entry absent in RSTB ({})", &entry);
            }
        } else {
            data.status_text = "Error: No RSTB opened".to_string();
            data.tab = "ERROR".to_string();
        }

        Some(data)
    }
    //END RSTB

    pub fn extract_opened_sarc(&self) -> Option<SendData> {
        let mut data = SendData::default();
        let dest_folder = FileDialog::new()
            .set_title("Choose folder to extract to")
            .pick_folder();
        if let Some(dest_folder) = dest_folder {
            if let Some(pack) = &self.pack {
                match pack.extract_all_to_folder(&dest_folder) {
                    Ok(m) => {
                        data.status_text = m;
                    }
                    Err(e) => {
                        data.status_text = "Error: Failed to extract SARC".to_string();
                        MessageDialog::new()
                            .set_title("Error")
                            .set_description(format!("Error: {}", e))
                            .set_buttons(rfd::MessageButtons::Ok)
                            .show();
                    }
                }

                return Some(data);
                // if let Ok(m) = pack.extract_all_to_folder(&dest_folder) {
                //     data.status_text = m;
                // } else {
                //     data.status_text = "Error: Failed to extract SARC".to_string();
                // }
            } else {
                return None; //no sarc opened
            }
        } else {
            return None; //no folder selected
        }
    }

    pub fn remove_internal_elem(&mut self, internal_path: String) -> Option<SendData> {
        let mut data = SendData::default();
        let mut is_reload = false;

        if let Some(pack) = &mut self.pack {
            if let Some(opened) = &mut pack.opened {
                is_reload = true;
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
            if is_reload {
                pack.compare_and_reload();
                data.get_sarc_paths(pack);
            }
            return Some(data);
        }

        None
    }

    pub fn close_all_click(&mut self) -> Option<SendData> {
        if self.zstd.totk_config.close_all_prompt
            && MessageDialog::new()
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
        self.pack = None;
        self.internal_file = None;
        data.status_text = "Closed all opened files".to_string();
        data.sarc_paths = SarcPaths::default();
        Some(data)
    }

    pub fn get_binary_for_opened_file(
        &mut self,
        text: &str,
        zstd: Arc<TotkZstd>,
        dest_file: &str,
    ) -> Option<Vec<u8>> {
        get_binary_by_filetype(
            self.opened_file.file_type,
            text,
            self.opened_file.endian.unwrap_or(roead::Endian::Little),
            zstd.clone(),
            dest_file,
            &mut self.opened_file,
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
        let mut is_reload = false;
        if let Some(pack) = &mut self.pack {
            if let Some(opened) = &mut pack.opened {
                is_reload = true;
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
                            let _result: String =
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
            if is_reload {
                pack.compare_and_reload();
                data.get_sarc_paths(pack);
            }
            return Some(data);
        }
        None
    }

    pub fn add_dir_to_sarc(&mut self, internal_dir: String, path: String) -> Option<SendData> {
        let mut data = SendData::default();
        let mut int_path = Pathlib::new(internal_dir.replace("\\", "/"));
        let path_var = Pathlib::new(path.replace("\\", "/"));
        let dirname = path_var.name.clone();
        let dirname_len = dirname.len();
        let mut path_root_len = path_var.full_path.len() - dirname_len;
        let mut path_root = &path_var.full_path[..path_root_len];
        if path_root.ends_with("/") {
            path_root = &path_root[..path_root.len() - 1];
        }
        path_root_len = path_root.len();
        if int_path.full_path.ends_with("/") {
            int_path.full_path = int_path.full_path[..int_path.full_path.len() - 1].to_string();
        }
        let files = list_files_recursively(&path_var.full_path);
        let files_len = files.len();
        if files_len == 0 {
            data.status_text = format!("No files found in {}", &path_var.full_path);
        } else {
            for file in files {
                let file_path = Pathlib::new(&file);
                let mut file_path_to_add = file_path.full_path[path_root_len..].to_string();
                if file_path_to_add.starts_with("/") {
                    file_path_to_add = file_path_to_add[1..].to_string();
                }
                let new_internal_path = format!("{}/{}", &int_path.full_path, &file_path_to_add);
                let _ = self.add_internal_file_from_path(new_internal_path, file, true);
            }
            data.status_text = format!("Added {} files to {}", files_len, &int_path.full_path);
        }
        if let Some(pack) = &mut self.pack {
            data.get_sarc_paths(pack);
        }
        Some(data)
        // None
    }

    pub fn add_internal_file_to_dir(
        &mut self,
        internal_dir: String,
        path: String,
    ) -> Option<SendData> {
        // let mut data = SendData::default();
        let p1 = Pathlib::new(internal_dir.replace("\\", "/"));
        let p2 = Pathlib::new(path.replace("\\", "/"));
        let internal_path = format!("{}/{}", &p1.full_path, &p2.name);
        return self.add_internal_file_from_path(internal_path, p2.full_path, true);

        // Some(data)
    }

    pub fn add_internal_file_from_path(
        &mut self,
        internal_path: String,
        path: String,
        overwrite: bool,
    ) -> Option<SendData> {
        let mut data = SendData::default();
        println!("trying to add  {} to sarc path {}", &path, &internal_path);
        let mut is_reload = false;
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

                is_reload = true;
                let mut f_handle = fs::File::open(&path).ok()?;
                let mut buffer: Vec<u8> = Vec::new();
                f_handle.read_to_end(&mut buffer).ok()?;
                opened
                    .writer
                    .add_file(&internal_path.replace("\\", "/"), buffer);
                data.status_text = format!("Added/replaced: {}", &internal_path);
            }
            if is_reload {
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
        if save_data.tab == "RSTB" && self.opened_file.restbl.is_some() {
            if let Some(rstb) = &self.opened_file.restbl {
                dialog.name = Some(rstb.path.name.clone());
                dialog.filters_from_path(&rstb.path.full_path);
            }
        }
        if (dialog.name.clone().unwrap_or_default().is_empty() && save_data.tab == "SARC")
            || (save_data.tab == "RSTB" && self.opened_file.restbl.is_none())
        {
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
                        let rawdata = self.get_binary_for_opened_file(
                            &save_data.text,
                            self.zstd.clone(),
                            &dest_file,
                        );
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
                    let mut is_reload = false;
                    if let Some(pack) = &mut self.pack {
                        if let Some(opened) = &mut pack.opened {
                            match opened.save(dest_file.clone()) {
                                Ok(_) => {
                                    is_reload = true;
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
                        if is_reload {
                            pack.compare_and_reload();
                            data.get_sarc_paths(pack);
                        }
                        return Some(data);
                    }
                    return None; //no sarc opened
                }
                "RSTB" => {
                    println!("About to save RSTB");
                    if let Some(rstb) = &mut self.opened_file.restbl {
                        if let Ok(_) = rstb.save(&dest_file) {
                            data.tab = "RSTB".to_string();
                            data.status_text =
                                format!("Saved {}", &self.opened_file.path.full_path);
                        } else {
                            data.status_text = format!(
                                "Error: Failed to save {}",
                                &self.opened_file.path.full_path
                            );
                        }
                    } else {
                        data.status_text = format!("Error: No RSTB opened");
                    }
                }
                _ => {
                    data.status_text = format!("Error: Unsupported tab {}", &save_data.tab);
                    return Some(data);
                }
            }
        }

        None
    }

    pub fn save_tab_yaml(&mut self, save_data: SaveData) -> Option<SendData> {
        let mut data = SendData::default();
        let mut is_reload = false;
        let text = &save_data.text;
        if let Some(internal_file) = &self.internal_file {
            if let Some(pack) = &mut self.pack {
                if let Some(opened) = &mut pack.opened {
                    let path = &internal_file.path.full_path;
                    let rawdata: Vec<u8> = get_binary_by_filetype(
                        internal_file.file_type,
                        text,
                        internal_file.endian.unwrap_or(roead::Endian::Little),
                        self.zstd.clone(),
                        &path,
                        &mut self.opened_file,
                    )?;
                    if rawdata.is_empty() {
                        data.status_text =
                            format!("Error: Failed to save {} for {}", &path, &opened.path.name);
                        data.tab = "ERROR".to_string();
                        println!("{:?}", &data);
                        return Some(data);
                    } else {
                        opened.writer.add_file(path, rawdata);
                        is_reload = true;
                        data.tab = "YAML".to_string();
                        data.status_text = format!(
                            "Saved {} for {}",
                            &internal_file.path.name, &opened.path.name
                        );
                    }
                }
                if is_reload {
                    pack.compare_and_reload();
                    data.get_sarc_paths(pack);
                }
            }
        } else {
            let fullpath = self.opened_file.path.full_path.clone();
            let rawdata: Vec<u8> = get_binary_by_filetype(
                self.opened_file.file_type,
                text,
                self.opened_file.endian.unwrap_or(roead::Endian::Little),
                self.zstd.clone(),
                &fullpath,
                &mut self.opened_file,
            )?;
            if rawdata.is_empty() {
                data.status_text =
                    format!("Error: Failed to save {}", &self.opened_file.path.full_path);
                data.tab = "ERROR".to_string();
                // println!("{:?}", &data);
                return Some(data);
            } else {
                let mut file = fs::File::create(&self.opened_file.path.full_path).ok()?;
                file.write_all(&rawdata).ok()?;
                data.tab = "YAML".to_string();
                data.status_text = format!("Saved {}", &self.opened_file.path.full_path);
                // println!("{:?}", &data);
                return Some(data);
            }
        }

        Some(data)
    }

    // #[allow(unused_variables)]
    pub fn save(&mut self, save_data: SaveData) -> Option<SendData> {
        println!(
            "About to save {} text len {}",
            save_data.tab,
            save_data.text.len()
        );
        let mut data = SendData::default();
        // let mut is_reload = false;
        let _text = &save_data.text;

        match save_data.tab.as_str() {
            "YAML" => {
                return self.save_tab_yaml(save_data);
            }
            "SARC" => {
                if let Some(pack) = &mut self.pack {
                    if let Some(opened) = &mut pack.opened {
                        if !check_if_save_in_romfs(&opened.path.full_path, self.zstd.clone()) {
                            opened.reload();
                            match opened.save_default() {
                                Ok(_) => {
                                    // is_reload = true;
                                    data.tab = "SARC".to_string();
                                    data.status_text =
                                        format!("Saved SARC {}", &opened.path.full_path);
                                }
                                Err(err) => {
                                    data.status_text = format!(
                                        "Error: Failed to save SARC {}",
                                        &opened.path.full_path
                                    );
                                    eprintln!("{:?}", err);
                                }
                            }
                            // } else {
                            //     data.status_text = format!(
                            //         "Error: Failed to save SARC {}",
                            //         &opened.path.full_path
                            //     );
                            // }
                        }
                    }
                }
            }
            "RSTB" => {
                println!("About to save RSTB");
                if let Some(rstb) = &mut self.opened_file.restbl {
                    if let Ok(_) = rstb.save_default() {
                        data.tab = "RSTB".to_string();
                        data.status_text = format!("Saved {}", &self.opened_file.path.full_path);
                    } else {
                        data.status_text =
                            format!("Error: Failed to save {}", &self.opened_file.path.full_path);
                    }
                } else {
                    data.status_text = format!("Error: No RSTB opened");
                }
            }

            _ => {
                // data.status_text = format!("Error: Unsupported tab {}", &save_data.tab);
            }
        }

        Some(data)
    }

    pub fn add_empty_byml(&mut self, folder: String) -> Option<SendData> {
        let mut data = SendData::default();
        let mut is_reload = false;
        if let Some(pack) = &mut self.pack {
            if let Some(opened) = &mut pack.opened {
                let raw_data = Byml::default().to_binary(opened.endian);
                let path = PathBuf::from(&folder);
                let mut i: i32 = 1;
                while i < 9999 {
                    let mut new_path = path.clone();
                    new_path.push(format!("new_{}.byml", i));
                    let dest_file = new_path.to_string_lossy().to_string().replace("\\", "/");
                    if !opened.writer.files.contains_key(&dest_file) {
                        opened.writer.add_file(&dest_file, raw_data.clone());
                        data.status_text = format!("Added {}", &dest_file);
                        is_reload = true;
                        break;
                    }
                    i += 1;
                }
            }
            if is_reload {
                pack.compare_and_reload();
                data.get_sarc_paths(pack);
            }
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

    pub fn search_in_sarc(&mut self, query: String) -> Option<SendData> {
        let mut data = SendData::default();
        let pattern = query.to_lowercase();
        data.tab = "SARC".to_string();
        // data.get_sarc_paths(&self.pack.as_ref().unwrap());
        // data.sarc_paths.paths = Vec::new();
        if let Some(pack) = &mut self.pack {
            data.get_sarc_paths(pack);
            data.sarc_paths.paths = Vec::new();
            if let Some(opened) = &mut pack.opened {
                for file in opened.sarc.files() {
                    if let Some((_, text)) =
                        get_string_from_data("".to_string(), file.data.to_vec(), self.zstd.clone())
                    {
                        let filename = file.name.unwrap_or_default().to_string();
                        if !filename.is_empty() && text.to_lowercase().contains(&pattern) {
                            data.sarc_paths.paths.push(filename);
                        }
                    }
                }
                data.sarc_paths
                    .paths
                    .sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
                data.status_text = format!(
                    "Found {} entries for \"{}\"",
                    data.sarc_paths.paths.len(),
                    &query
                );
                return Some(data);
            }
            data.status_text = format!("Error: No SARC opened for Pack comparer");
        }
        data.status_text = format!("Error: No SARC opened");
        return Some(data);
    }

    pub fn clear_search_in_sarc(&mut self) -> Option<SendData> {
        let mut data = SendData::default();
        data.tab = "SARC".to_string();
        if let Some(pack) = &mut self.pack {
            data.get_sarc_paths(pack);
            data.status_text = "Cleared search results".to_string();
            return Some(data);
        }
        None
    }

    pub fn open_from_path(&mut self, file_name: String) -> Option<SendData> {
        let mut data = SendData::default();
        //let file_name = file.to_string_lossy().to_string().replace("\\", "/");
        if check_if_filepath_valid(&file_name) {
            if let Some((pack, data)) = open_sarc(&file_name, self.zstd.clone()) {
                self.pack = Some(pack);
                self.internal_file = None;
                return Some(data);
            }
            let res = file_from_disk_to_senddata(&file_name, self.zstd.clone());
            if let Some(res) = res {
                self.opened_file = res.0;
                self.internal_file = None;
                return Some(res.1);
            }
            // let res = open_tag(file_name.clone(), self.zstd.clone())
            //     .or_else(|| open_esetb(file_name.clone(), self.zstd.clone()))
            //     .or_else(|| open_restbl(file_name.clone(), self.zstd.clone()))
            //     .or_else(|| open_asb(file_name.clone(), self.zstd.clone()))
            //     .or_else(|| open_ainb(file_name.clone(), self.zstd.clone()))
            //     .or_else(|| open_byml(file_name.clone(), self.zstd.clone()))
            //     .or_else(|| open_msbt(file_name.clone()))
            //     .or_else(|| open_aamp(file_name.clone()))
            //     .or_else(|| open_text(file_name.clone()))
            //     .map(|(opened_file, data)| {
            //         self.opened_file = opened_file;
            //         self.internal_file = None;
            //         data
            //     });
            // if res.is_some() {
            //     return res;
            // }
        } else {
            return None;
        }
        data.tab = "ERROR".to_string();
        data.status_text = format!("Error: Failed to open {}", &file_name);
        return Some(data);
    }

    pub fn compare_files(&self, is_from_disk: bool)-> Option<SendData> {
        // let mut c = DiffComparer::default();
        // c.compare_by_choice(decision, &self.pack, &int_or_regular_path, self.zstd.clone(), is_from_disk)
        DiffComparer::files_from_disk(self.zstd.clone(), is_from_disk)
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

    pub fn compare_opened_file_with_original_monaco(&mut self) -> Option<SendData> {
        let mut data = SendData::default();
        if self.opened_file.path.full_path.is_empty() {
            data.status_text = "Error: No file from disk nor SARC opened".to_string();
            return Some(data);
        }
        let mut van_path = String::new();
        match self.zstd.clone().totk_config.find_vanila_file_in_romfs(&self.opened_file.path.full_path) {
            Ok(p) => {
                van_path = p;
            },
            Err(e) => {
                eprintln!("{:?}", e);
                data.status_text = format!("Error: {} not found", &self.opened_file.path.name);
                return Some(data);
            }
        }
        println!("Comparing {} with original {}", &self.opened_file.path.full_path, &van_path);
        let mut text2 = String::new();
        if let Some((_, t)) = file_from_disk_to_senddata(&van_path, self.zstd.clone()) {
            text2 = t.text;
        } else {
            data.status_text = format!("Error: Failed to parse {}", &self.opened_file.path.full_path);
            return Some(data);
        }
        data.compare_data.file1.label = self.opened_file.path.full_path.clone();
        data.compare_data.file1.path = self.opened_file.path.clone();

        data.compare_data.file2.label = van_path.clone();
        data.compare_data.file2.path = Pathlib::new(van_path.clone());
        data.compare_data.file2.text = text2;
        data.tab = "COMPARE".to_string();
        data.status_text = format!("Compared {} with original", &self.opened_file.path.full_path);
        


        //TODO: finish it 
        //compare internal file 
        // let mut data = SendData::default();
        // let vanila_sarc_path = self.zstd.clone().find_vanila_internal_file_path_in_romfs(&path, self.zstd.clone());

        Some(data)
    }
    
    
    pub fn compare_internal_file_with_original(&mut self, path: String, is_from_sarc: bool) -> Option<SendData> {
        let mut data = SendData::default();
        let mut path = path.clone();
        let mut text1 = String::new();
        let mut text2 = String::new();
        let is_from_monaco = !is_from_sarc;
        if is_from_monaco { //from monaco
            if let Some(internal_file) = &self.internal_file {
                path = internal_file.path.full_path.clone();
                data.compare_data.file1.label = format!("{} (from YAML Editor)", &path);
            } else {
                // data.status_text = "Error: No SARC internal file opened".to_string();
                // return Some(data);
                // Perhaps the file is from disk, check self.opened_file
                //simple fallback
                return self.compare_opened_file_with_original_monaco();
            }
        }
        println!("Comparing {} with original", &path);
    
        if let Some(pack) = &self.pack {
            if let Some(opened) = &pack.opened {
                if is_from_sarc {
                if let Some(rawdata1) = opened.sarc.get_data(&path) {
                    text1 = get_string_from_data(&path, rawdata1.to_vec(), self.zstd.clone())
                        .map(|(_, t)| t)
                        .unwrap_or_default();
                }}
    
                    if let Some(vanila) = &pack.vanila {
                        if let Some(rawdata2) = vanila.sarc.get_data(&path) {
                            text2 = get_string_from_data(&path, rawdata2.to_vec(), self.zstd.clone())
                                .map(|(_, t)| t)
                                .unwrap_or_default();
                        }
                    }
                    // println!("{} {}", text1.len(), text2.len());
    
                    if text2.is_empty() && (!text1.is_empty() || is_from_sarc) {
                        text2 = self
                            .zstd
                            .clone()
                            .find_vanila_internal_file_data_in_romfs(&path, self.zstd.clone())
                            .unwrap_or_else(|err| {
                                data.status_text = format!("ERROR: {:?}", &err);
                                String::new()
                            });
                    }
            }
        }
    
        // println!("{} {}", text1.len(), text2.len());
    
        if (!text1.is_empty() || is_from_monaco) && !text2.is_empty() {
            data.tab = "COMPARE".to_string();
            data.compare_data.file1.text = text1;
            data.compare_data.file2.text = text2;
            data.status_text = format!("Compared {}", &path);
            if data.compare_data.file1.label.is_empty() {
                data.compare_data.file1.label = format!("{} (from SARC)", &path);
            }
            data.compare_data.file2.label = "Original".to_string();
        } else {
            data.status_text = format!("Error: Failed to compare {}", &path);
        }
    
        Some(data)
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
    pub esetb: Option<Esetb<'a>>,
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
            esetb: None,
        }
    }
}

impl InternalFile<'_> {
    #[allow(dead_code)]
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
            esetb: None,
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
