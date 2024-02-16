use crate::BinTextFile::{bytes_to_file, BymlFile, MsytFile};
use crate::Gui::{ActiveTab, OpenedFile, TotkBitsApp};
use crate::Pack::PackFile;
use crate::SarcFileLabel::SarcLabel;
use crate::Settings::{Icons, Pathlib, Settings};
use crate::Tree::{self, TreeNode};
use crate::Zstd::{is_msyt, FileType, TotkZstd};
//use crate::SarcFileLabel::ScrollAreaPub;
use eframe::egui::{self, ScrollArea, SelectableLabel, TopBottomPanel};
use egui::{Align, Button, InnerResponse, Label, Layout, Pos2, Rect, Shape};
use rfd::FileDialog;
use roead::aamp::ParameterIO;
//use nfd::Response;
use roead::byml::Byml;
use roead::sarc::File;

use std::error::Error;
use std::fmt::format;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;
use std::{fs, io};

pub fn extract_click(app: &mut TotkBitsApp) -> io::Result<()> {
    match app.active_tab {
        ActiveTab::DiretoryTree => {
            if let Some(internal_file) = &mut app.internal_sarc_file {
                if let Some(pack) = &mut app.pack {
                    let path = FileDialog::new()
                        .set_file_name(&internal_file.path.name)
                        .set_title("Extract")
                        .save_file();
                    println!("{}", &path.clone().unwrap().to_string_lossy().into_owned());
                    if let Some(dest_file) = &path {
                        if let Some(data) = pack.sarc.get_data(&internal_file.path.full_path) {
                            match bytes_to_file(data.to_vec(), &dest_file.to_string_lossy()) {
                                Ok(_) => {app.status_text = format!("Saved: {}", dest_file.to_string_lossy().into_owned());},
                                Err(err) => {app.status_text = format!("Error extracting: {}", dest_file.to_string_lossy().into_owned());}
                            }
                            
                        } else {
                            app.status_text = format!(
                                "Error extracting: {} to {}",
                                &internal_file.path.name,
                                dest_file.to_string_lossy().into_owned()
                            );
                        }
                    }
                }
            }
        }
        ActiveTab::TextBox => {}
    }
    Ok(())
}

pub fn open_file_safe(app: &mut TotkBitsApp, _ui: &mut egui::Ui) {
    match open_byml_or_sarc(app, _ui) {
        Some(_) => {
            app.status_text = format!("Opened: {}", app.opened_file.path.full_path);
        }
        None => {
            app.status_text = format!("Failed to open: {}", app.opened_file.path.full_path);
        }
    }
}

pub fn open_byml_or_sarc(app: &mut TotkBitsApp, _ui: &mut egui::Ui) -> Option<io::Result<()>> {
    if app.settings.is_file_loaded {
        return None; //stops the app from infinite file loading from disk
    }
    app.settings.is_file_loaded = true;
    let path = app.opened_file.path.full_path.clone();
    println!("Is {} a msyt?", &path.clone());
    match MsytFile::file_to_text(path.clone()) {
        Ok(text) => {
            app.text = text;
            app.opened_file = OpenedFile::from_path(path.clone(), FileType::Msbt);
            app.opened_file.endian = Some(roead::Endian::Little);
            app.opened_file.msyt = None;
            app.opened_file.byml = None;
            app.internal_sarc_file = None;
            app.active_tab = ActiveTab::TextBox;
            app.status_text = format!("Opened: {}", app.opened_file.path.full_path);
            return Some(Ok(()));
        }
        Err(err) => {}
    }
    println!("Is {} a sarc?", path.clone());
    match PackFile::new(path.clone(), app.zstd.clone()) {
        Ok(pack) => {
            app.pack = Some(pack);
            app.settings.is_file_loaded = true;
            println!("Sarc  opened!");
            app.active_tab = ActiveTab::DiretoryTree;
            app.settings.is_tree_loaded = false;
            app.root_node = TreeNode::new("ROOT".to_string(), "/".to_string());
            app.status_text = format!("Opened: {}", app.opened_file.path.full_path);
            return Some(Ok(()));
        }
        Err(err) => {}
    }
    println!("Is {} a byml?", path.clone());
    let res_byml = BymlFile::new(path.clone(), app.zstd.clone());
    match res_byml {
        Ok(b) => {
            app.text = Byml::to_text(&b.pio);
            app.opened_file = OpenedFile::new(
                path,
                FileType::Byml,
                BymlFile::get_endiannes(&b.file_data.data),
                None,
            );
            app.opened_file.byml = Some(b);
            app.active_tab = ActiveTab::TextBox;
            println!("Byml  opened!");
            app.internal_sarc_file = None;
            app.status_text = format!("Opened: {}", app.opened_file.path.full_path);
            return Some(Ok(()));
        }

        Err(_) => {}
    };
    app.settings.is_tree_loaded = true;
    app.status_text = format!("Failed to open: {}", app.opened_file.path.full_path.clone());
    return None;
}

pub fn edit_click(app: &mut TotkBitsApp, ui: &mut egui::Ui) {
    if let Some(child) = &mut app.internal_sarc_file.clone() {
        SarcLabel::safe_open_file_from_opened_sarc(app, ui, child)
    }
    //app.internal_sarc_file = Some(child.clone());
}

pub fn save_click(app: &mut TotkBitsApp) {
    match app.active_tab {
        ActiveTab::DiretoryTree => {
            save_tab_tree(app);
        }
        ActiveTab::TextBox => {
            let _ = save_tab_text(app);
        }
    }
}

pub fn save_tab_text(app: &mut TotkBitsApp) -> Result<(), roead::Error> {
    if let Some(internal_file) = &mut app.internal_sarc_file {
        //file is from sarc
        if let Some(pack) = &mut app.pack {
            //pack.save(internal_file.path.full_path.clone())?;
            //app.status_text = format!("Saved: {}", internal_file.path.full_path.clone());
        }
    } else {
        //file is independent byml/msyt/aamp
        if app.text.len() > 0 {
            let dest_file = app.opened_file.path.full_path.clone();
            save_text_file_by_filetype(app, &dest_file);
            return Ok(());
        } //nothing to save
    }
    Ok(())
}

pub fn save_text_file_by_filetype(app: &mut TotkBitsApp, dest_file: &str) {
    //Save the content of the text editor. Check if app.text is empty beforehand!
    match app.opened_file.file_type {
        FileType::Bcett => {
            let mut byml = BymlFile::from_text(&app.text, app.zstd.clone());
            if let Ok(b) = &mut byml {
                b.file_data.file_type = FileType::Bcett;
                b.save(dest_file.to_string())
                    .expect(&format!("Failed to save bcett byml {}", dest_file));
                app.status_text = format!("Saved: {}", dest_file);
            }
        }
        FileType::Byml => {
            let mut byml = BymlFile::from_text(&app.text, app.zstd.clone());
            if let Ok(b) = &mut byml {
                b.file_data.file_type = FileType::Byml;
                b.save(dest_file.to_string())
                    .expect(&format!("Failed to save  byml {}", dest_file));
                app.status_text = format!("Saved: {}", dest_file);
            }
        }
        FileType::Msbt => {
            //Only Little endian supported!
            match MsytFile::text_to_binary_file(&app.text, dest_file, roead::Endian::Little) {
                Ok(_) => {
                    app.status_text = format!("Saved: {}", dest_file);
                }
                Err(_) => {
                    app.status_text = format!("Error saving: {}", dest_file);
                }
            }
        }
        FileType::None => {
            app.status_text = "No file opened to save!".to_string();
        }
        _ => {}
    }
}

pub fn save_tab_tree(app: &mut TotkBitsApp) {
    //save sarc to default path, if any opened
    if let Some(pack) = &mut app.pack {
        let _ = pack.save_default();
    }
}

pub fn save_as_click(app: &mut TotkBitsApp) -> Result<(), roead::Error> {
    let mut prob_file_name = String::new();
    if app.opened_file.path.full_path.len() > 0 {
        prob_file_name = Pathlib::new(app.opened_file.path.full_path.clone()).name;
    }
    let dest_file = save_file_dialog(Some(prob_file_name));
    if dest_file.len() > 0 {
        match app.active_tab {
            ActiveTab::DiretoryTree => {
                if let Some(pack) = &mut app.pack {
                    pack.save(dest_file)?;
                    return Ok(());
                }
            }
            ActiveTab::TextBox => {
                save_text_file_by_filetype(app, &dest_file);
            }
        }
    }

    Ok(())
}

pub fn open_file_button_click(app: &mut TotkBitsApp) -> io::Result<()> {
    // Logic for opening a file
    let file_name = open_file_dialog(None);
    if !file_name.is_empty() {
        println!("Attempting to read {} file", &file_name);
        app.opened_file = OpenedFile::from_path(file_name.clone(), FileType::Other);
        app.opened_file.endian = None;
        app.opened_file.msyt = None;
        let mut f_handle = fs::File::open(&file_name)?;
        let mut buffer: Vec<u8> = Vec::new(); //String::new();
        match f_handle.read_to_end(&mut buffer) {
            Ok(_) => {
                app.status_text = format!("Opened file: {}", &app.opened_file.path.full_path);
                app.settings.is_file_loaded = false;
                return Ok(());
            }
            Err(_err) => {
                app.status_text = format!("Error reading file: {}", file_name);
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    app.status_text.clone(),
                ));
            }
        }
        //self.text = buffer;
    } else {
        app.status_text = "No file selected".to_owned();
        return Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "No file selected",
        ));
    }
}

pub fn close_all_click(app: &mut TotkBitsApp) {
    app.opened_file = OpenedFile::default();
    app.pack = None;
    app.root_node = TreeNode::new("ROOT".to_string(), "/".to_string());
    app.text = String::new();
    app.settings.is_file_loaded = true;
    app.settings.is_tree_loaded = true;
}

pub fn save_file_dialog(file_name: Option<String>) -> String {
    let name = file_name.unwrap_or("".to_string());
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

pub fn open_file_dialog(file_name: Option<String>) -> String {
    let name = file_name.unwrap_or("".to_string());
    let file = FileDialog::new().set_file_name(name).pick_file();
    match file {
        Some(res) => {
            return res.to_string_lossy().into_owned();
        }
        None => {
            return "".to_string();
        }
    }
}
