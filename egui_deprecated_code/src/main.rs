//#![windows_subsystem = "windows"]
//use std::fs::File;

use std::{
    env,
    path::{Path, PathBuf},
};

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
use TotkConfig::init;

fn main() {
    if init() {
        if let Err(err) = Gui::run() {
            rfd::MessageDialog::new()
                .set_buttons(rfd::MessageButtons::Ok)
                .set_title("Critical error")
                .set_description(format!("Critical error occured:\n{:?}", err))
                .show();
        }
    }
}
//use msyt;
