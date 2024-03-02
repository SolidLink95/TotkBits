//#![windows_subsystem = "windows"]
//use std::fs::File;

use std::{env, path::{Path, PathBuf}};

//mod TestCases;
mod ButtonOperations;
mod FileReader;
mod Gui;
mod GuiMenuBar;
mod GuiScroll;
mod Open_Save;
mod SarcFileLabel;
mod Settings;
mod TotkConfig;
mod Tree;
mod Zstd;
mod file_format;
mod misc;
mod ui_elements;
mod widgets;

fn init() -> bool {
    let mut c = TotkConfig::TotkConfig::new();
    println!("{:?}", c.romfs);
    if c.romfs.to_string_lossy().is_empty() || !c.romfs.exists() || c.romfs.is_file() {
        rfd::MessageDialog::new()
            .set_buttons(rfd::MessageButtons::Ok)
            .set_title("No romfs path found")
            .set_description("Please choose romfs path")
            .show();
        let mut chosen = rfd::FileDialog::new()
            .set_title("Choose romfs path")
            .pick_folder()
            .unwrap_or_default();
        let dirr = chosen.to_string_lossy().to_string().replace("\\", "/");
        if dirr.is_empty() || !chosen.exists() {
            return false;
        }
        let mut zsdic = chosen.clone();
        zsdic.push("Pack/ZsDic.pack.zs");
        if !zsdic.exists() {
            rfd::MessageDialog::new()
                .set_buttons(rfd::MessageButtons::Ok)
                .set_title("Invalid romfs path")
                .set_description(format!(
                    "Invalid romfs path! File\n{}\ndoes not exist. Program will now exit",
                    &zsdic.to_string_lossy().to_string().replace("\\", "/")
                )).show();
            return false;
        }
        let appdata_str = env::var("APPDATA").unwrap_or("".to_string());
        if appdata_str.is_empty() {
            rfd::MessageDialog::new()
            .set_buttons(rfd::MessageButtons::Ok)
            .set_title("Error")
            .set_description("Unable to access appdata, exiting").show();
            return false;
        }
        c.config_path = PathBuf::from(appdata_str.to_string());
        c.config_path.push("Totk/config.json");
        println!("{:?}", &c.config_path);
        c.romfs = chosen.clone();
         if let Err(err) = c.save(){
            rfd::MessageDialog::new()
            .set_buttons(rfd::MessageButtons::Ok)
            .set_title("Error")
            .set_description(format!("{:?}", err)).show();
            return false;
         }
    }

    true
}
//use msyt;

fn main() {
    if init() {
        Gui::run();
    }
}
