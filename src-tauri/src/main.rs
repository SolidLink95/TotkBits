// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![windows_subsystem = "windows"]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::env;
use std::sync::Mutex;

use tauri::{App, Manager, State, Window};
use TotkConfig::init;
mod Open_and_Save;
mod Settings;
mod TauriCommands;
mod TotkApp;
mod TotkConfig;
mod Zstd;
mod file_format;
use crate::TauriCommands::{
    add_click, close_all_opened_files, edit_internal_file, exit_app, extract_internal_file,
    get_status_text, open_file_dialog, open_file_from_path, open_file_struct,
    remove_internal_sarc_file, rename_internal_sarc_file, rstb_edit_entry, rstb_get_entries,
    rstb_remove_entry, save_as_click, save_file_struct,add_to_dir_click
};
use crate::TotkApp::TotkBitsApp;

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command

struct CommandLineArg(String);

#[tauri::command]
fn get_command_line_arg(state: State<CommandLineArg>) -> String {
    state.0.clone()
}

fn main() {
    if !init() {
        println!("Error while initializing romfs path");
        return;
    }

    let mut app = Mutex::<TotkBitsApp>::default();
    tauri::Builder::default()
        .setup(|app1| {
            // Access command-line arguments
            let args: Vec<String> = env::args().collect();
            if args.len() > 1 {
                app1.manage(CommandLineArg(args[1].clone()));
            } else {
                app1.manage(CommandLineArg("".to_string()));
            }

            Ok(())
        })
        .manage(app)
        .invoke_handler(tauri::generate_handler![
            get_command_line_arg,
            get_status_text,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
