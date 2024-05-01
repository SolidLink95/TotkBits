// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// #![windows_subsystem = "windows"]
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(non_snake_case, non_camel_case_types)]
use std::{env, io};

use std::sync::Mutex;

use serde_json::json;
use tauri::Manager;
// use TotkConfig::{init, TotkConfig};
mod Open_and_Save;
mod Settings;
mod TauriCommands;
mod TotkApp;
mod TotkConfig;
mod Zstd;
mod file_format;
use crate::TauriCommands::{
    add_click, add_to_dir_click, clear_search_in_sarc, close_all_opened_files, edit_internal_file,
    exit_app, extract_internal_file, open_file_dialog, open_file_from_path, open_file_struct,
    remove_internal_sarc_file, rename_internal_sarc_file, rstb_edit_entry, rstb_get_entries,
    rstb_remove_entry, save_as_click, save_file_struct, search_in_sarc,edit_config, restart_app
};
use crate::TotkApp::TotkBitsApp;

#[tauri::command]
fn get_startup_data(state: tauri::State<serde_json::Value>) -> Result<serde_json::Value, String> {
    Ok((*state.inner()).clone())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StartupData {
    pub argv1: String,
    pub config: TotkConfig::TotkConfig,
}

impl StartupData {
    fn new() -> io::Result<Self> {
        let args: Vec<String> = env::args().collect();
        let argv1 = args.get(1).cloned().unwrap_or_default();
        let config = TotkConfig::TotkConfig::safe_new()?;
        Ok(Self { argv1, config })
    }
    fn to_json(&self) -> io::Result<serde_json::Value> {
        Ok(json!({
            "argv1": self.argv1,
            "fontSize": self.config.fontSize,

        }))
    }
}

fn main() -> io::Result<()> {
    #[allow(unused_variables)]
    let startup_data = StartupData::new()?.to_json()?;
    println!("{:?}", startup_data);
    let app = Mutex::<TotkBitsApp>::default();
    if let Err(err) = tauri::Builder::default()
        .setup(|app_setup| {
            app_setup.manage(startup_data);
            Ok(())
        })
        .manage(app)
        .invoke_handler(tauri::generate_handler![
            restart_app,
            edit_config,
            get_startup_data,
            open_file_struct,
            open_file_from_path,
            edit_internal_file,
            save_file_struct,
            save_as_click,
            add_click,
            add_to_dir_click,
            extract_internal_file,
            rename_internal_sarc_file,
            close_all_opened_files,
            remove_internal_sarc_file,
            exit_app,
            open_file_dialog,
            rstb_get_entries,
            rstb_edit_entry,
            rstb_remove_entry,
            search_in_sarc,
            clear_search_in_sarc,
        ])
        .run(tauri::generate_context!())
    {
        rfd::MessageDialog::new()
            .set_buttons(rfd::MessageButtons::Ok)
            .set_title("Error while running tauri application")
            .set_description(format!("{:?}", err))
            .show();
    }
    Ok(())
}
