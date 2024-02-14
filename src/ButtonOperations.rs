use crate::BymlFile::BymlFile;
use crate::Gui::{ActiveTab, TotkBitsApp};
use crate::Pack::PackFile;
use crate::SarcFileLabel::SarcLabel;
use crate::Settings::{Icons, Pathlib, Settings};
use crate::Tree::{self, tree_node};
use crate::Zstd::totk_zstd;
//use crate::SarcFileLabel::ScrollAreaPub;
use eframe::egui::{self, ScrollArea, SelectableLabel, TopBottomPanel};
use egui::{Align, Button, Label, Layout, Pos2, Rect, Shape};

use nfd::Response;
use roead::byml::Byml;

use std::error::Error;
use std::io::{Read, Write};
use std::path::Path;
use std::{fs, io};

pub fn open_byml_or_sarc(app: &mut TotkBitsApp, _ui: &mut egui::Ui) {
    if app.settings.is_file_loaded {
        return; //stops the app from infinite file loading from disk
    }
    println!("Is {} a sarc?", app.opened_file.clone());
    match PackFile::new(app.opened_file.clone(), app.zstd.clone()) {
        Ok(pack) => {
            app.pack = Some(pack);
            app.settings.is_file_loaded = true;
            println!("Sarc  opened!");
            app.active_tab = ActiveTab::DiretoryTree;
            app.settings.is_tree_loaded = false;
            app.root_node = tree_node::new("ROOT".to_string(), "/".to_string());
            return;
        }
        Err(_) => {}
    }
    println!("Is {} a byml?", app.opened_file.clone());
    let res_byml: Result<BymlFile<'_>, io::Error> =
        BymlFile::new(app.opened_file.clone(), app.zstd.clone());
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
    app.status_text = format!("Failed to open: {}", app.opened_file.clone());
}

pub fn edit_click(app: &mut TotkBitsApp, ui: &mut egui::Ui) {
    if let Some(child) = &mut app.internal_sarc_file.clone() {
        SarcLabel::safe_open_file_from_opened_sarc(
            app,
            ui,
            child.path.full_path.clone(),
        )
    }
    //app.internal_sarc_file = Some(child.clone());
}

pub fn save_click(app: &mut TotkBitsApp) {
    match app.active_tab {
        ActiveTab::DiretoryTree => {
            save_tab_tree(app);
        }
        ActiveTab::TextBox => {
            save_tab_text(app);
        }
    }
}

pub fn save_tab_text(app: &mut TotkBitsApp) -> Result<(), roead::Error> {
    if let Some(internal_file) = &mut app.internal_sarc_file {
        //file is from sarc
        if let Some(pack) = &mut app.pack {
            pack.save(internal_file.path.full_path.clone())?;
        }
    } else {
        //file is independent byml
        let byml = app.byml.as_ref().expect("app.byml is None!");
        let p = Pathlib::new(app.opened_file.clone());
        if Path::new(&app.opened_file.clone()).exists() {
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
    let dest_file = save_file_dialog();
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
    let file_name = open_file_dialog();
    if !file_name.is_empty() {
        println!("Attempting to read {} file", &file_name);
        app.opened_file = file_name.clone();
        let mut f_handle = fs::File::open(&file_name)?;
        let mut buffer: Vec<u8> = Vec::new(); //String::new();
        match f_handle.read_to_end(&mut buffer) {
            Ok(_) => {
                app.status_text = format!("Opened file: {}", &app.opened_file);
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

pub fn save_file_dialog() -> String {
    match nfd::open_save_dialog(None, None) {
        Ok(response) => match response {
            Response::Okay(file_path) => {
                return file_path;
            }
            Response::Cancel => {
                return "".to_string();
            }
            _ => {
                return "".to_string();
            }
        },
        _ => {
            return "".to_string();
        }
    }
}

pub fn open_file_dialog() -> String {
    match nfd::open_file_dialog(None, None) {
        Ok(response) => {
            match response {
                Response::Okay(file_path) => {
                    // `file_path` contains the selected file's path as a `PathBuf`
                    println!("Selected file: {:?}", file_path);
                    return file_path;
                }
                Response::Cancel => {
                    // The user canceled the file selection
                    println!("File selection canceled");
                    return "".to_string();
                }
                _ => {
                    // Some other error occurred
                    //println!("An error occurred");
                    return "".to_string();
                }
            }
        }
        _ => {
            return "".to_string();
        }
    }
}
