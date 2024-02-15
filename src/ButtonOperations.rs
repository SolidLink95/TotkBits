use crate::BinTextFile::BymlFile;
use crate::Gui::{ActiveTab, TotkBitsApp};
use crate::Pack::PackFile;
use crate::SarcFileLabel::SarcLabel;
use crate::Settings::{Icons, Pathlib, Settings};
use crate::Tree::{self, tree_node};
use crate::Zstd::{FileType, TotkZstd};
//use crate::SarcFileLabel::ScrollAreaPub;
use eframe::egui::{self, ScrollArea, SelectableLabel, TopBottomPanel};
use egui::{Align, Button, InnerResponse, Label, Layout, Pos2, Rect, Shape};
use rfd::FileDialog;
use roead::aamp::ParameterIO;
//use nfd::Response;
use roead::byml::Byml;
use roead::sarc::File;

use std::error::Error;
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
                        .set_title("Save")
                        .save_file();
                    if let Some(dest_file) = &path {
                        let data: Option<&[u8]> = pack.sarc.get_data(&internal_file.path.full_path);
                        if data.is_some() {
                            let mut f_handle = fs::File::create(dest_file)?;
                            f_handle.write_all(&data.unwrap())?;
                            app.status_text = format!("Saved: {}", dest_file.to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }
        ActiveTab::TextBox => {}
    }
    Ok(())
}

pub fn open_byml_or_sarc(app: &mut TotkBitsApp, _ui: &mut egui::Ui) {
    if app.settings.is_file_loaded {
        return; //stops the app from infinite file loading from disk
    }
    println!("Is {} a sarc?", app.opened_file.full_path.clone());
    match PackFile::new(app.opened_file.full_path.clone(), app.zstd.clone()) {
        Ok(pack) => {
            app.pack = Some(pack);
            app.settings.is_file_loaded = true;
            println!("Sarc  opened!");
            app.active_tab = ActiveTab::DiretoryTree;
            app.settings.is_tree_loaded = false;
            app.root_node = tree_node::new("ROOT".to_string(), "/".to_string());
            return;
        }
        Err(err) => {
            eprintln!(
                "Error creating sarc {}: {}",
                app.opened_file.full_path.clone(),
                err
            );
            app.settings.is_file_loaded = true;
            //return
        }
    }
    println!("Is {} a byml?", app.opened_file.full_path.clone());
    let res_byml = BymlFile::new(app.opened_file.full_path.clone(), app.zstd.clone());
    match res_byml {
        Ok(ref b) => {
            app.text = Byml::to_text(&b.pio);
            app.byml = Some(res_byml.unwrap());
            app.active_tab = ActiveTab::TextBox;
            println!("Byml  opened!");
            app.settings.is_file_loaded = true;
            app.internal_sarc_file = None;
            return;
        }

        Err(_) => {}
    };
    app.settings.is_file_loaded = true;
    app.settings.is_tree_loaded = true;
    app.status_text = format!("Failed to open: {}", app.opened_file.full_path.clone());
}

pub fn edit_click(app: &mut TotkBitsApp, ui: &mut egui::Ui) {
    if let Some(child) = &mut app.internal_sarc_file.clone() {
        SarcLabel::safe_open_file_from_opened_sarc(app, ui, child.path.full_path.clone())
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
            pack.save(internal_file.path.full_path.clone())?;
            app.status_text = format!("Saved: {}", internal_file.path.full_path.clone());
        }
    } else {
        //file is independent byml
        if app.internal_sarc_file.is_none() {
            //external byml file
            if app.text.len() == 0 {
                return Ok(());
            } //nothing to save
            let mut name = String::new();
            if app.opened_file.full_path.len() > 0 {
                name = app.opened_file.name.clone();
            }
            let path = Some(app.opened_file.full_path.clone()); //= FileDialog::new().set_file_name(&name).set_title("Save").save_file();
            if let Some(dest_file) = &path {
                let mut f_handle = fs::File::create(dest_file)?;
                let mut data: Vec<u8> = Vec::new();
                match app.opened_file_type {
                    FileType::Byml => {
                        let byml = BymlFile::from_text(app.text.clone(), app.zstd.clone());
                        if let Ok(b) = byml {
                            b.save(dest_file.to_string());
                            app.status_text = format!("Saved: {}", dest_file);
                        }
                    }
                    FileType::Aamp => {
                        let pio: ParameterIO = ParameterIO::from_text(&app.text)?;
                        data = pio.to_binary();
                        //TODO: aamp class
                    }
                    FileType::Msbt => {
                        //TODO: MSbt class
                    }
                    _ => {}
                }
            }
        } else { //internal byml file
        }

        let byml = app.byml.as_ref().expect("app.byml is None!");
        let p = Pathlib::new(app.opened_file.full_path.clone());
        if Path::new(&app.opened_file.full_path.clone()).exists() {
            let pio: Byml = Byml::from_text(&app.text)?;
            let mut f_handle = fs::File::open(p.full_path)?;
            let data: Vec<u8> = pio.to_binary(byml.endian.unwrap_or(roead::Endian::Little));
            f_handle.write_all(&data)?;
        } else {
            save_as_click(app);
        }
    }
    Ok(())
}

pub fn save_tab_tree(app: &mut TotkBitsApp) {
    if let Some(pack) = &mut app.pack {
        let _ = pack.save_default();
    }
}

pub fn save_as_click(app: &mut TotkBitsApp) -> Result<(), roead::Error> {
    let mut prob_file_name = String::new();
    if app.opened_file.full_path.len() > 0 {
        prob_file_name = Pathlib::new(app.opened_file.full_path.clone()).name;
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
                let byml = app.byml.as_ref().expect("app.byml is None!");
                let pio: Byml = Byml::from_text(&app.text)?;
                let mut f_handle = fs::File::open(dest_file)?;
                let data: Vec<u8> = pio.to_binary(byml.endian.unwrap_or(roead::Endian::Little));
                f_handle.write_all(&data)?;
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
        app.opened_file = Pathlib::new(file_name.clone());
        let mut f_handle = fs::File::open(&file_name)?;
        let mut buffer: Vec<u8> = Vec::new(); //String::new();
        match f_handle.read_to_end(&mut buffer) {
            Ok(_) => {
                app.status_text = format!("Opened file: {}", &app.opened_file.full_path);
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
