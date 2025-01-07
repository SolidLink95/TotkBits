// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// #![windows_subsystem = "windows"]
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(non_snake_case, non_camel_case_types)]
use miow::pipe::NamedPipeBuilder;
use std::io::{BufRead, BufReader};
use std::{fs, process};
use std::process::Command;
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Duration;
use std::{env, io, thread};
use tauri::Manager;
use Zstd::get_executable_dir;
mod Comparer;
mod Open_and_Save;
mod Settings;
mod TauriCommands;
mod TotkApp;
mod TotkConfig;
mod Zstd;
mod file_format;
use crate::Settings::{get_startup_data, StartupData};
use crate::TauriCommands::{
    add_click, add_empty_byml_file, add_files_from_dir_recursively, add_to_dir_click,
    clear_search_in_sarc, close_all_opened_files, compare_files, compare_internal_file_with_vanila,
    edit_config, edit_internal_file, exit_app, extract_internal_file, extract_opened_sarc,
    open_dir_dialog, open_file_dialog, open_file_from_path, open_file_struct,
    remove_internal_sarc_file, rename_internal_sarc_file, restart_app, rstb_edit_entry,
    rstb_get_entries, rstb_remove_entry, save_as_click, save_file_struct, search_in_sarc,check_if_update_needed
};
use crate::TotkApp::TotkBitsApp;
use updater::TotkbitsVersion::TotkbitsVersion;





fn main() -> io::Result<()> {
    main_initialization()?;
    // test_case()?;
    // return Ok(());
    let startup_data = StartupData::new()?.to_json()?;
    // println!("{:?}", startup_data);
    let app = Mutex::<TotkBitsApp>::default();
    if let Err(err) = tauri::Builder::default()
        .setup(|app_setup| {
            app_setup.manage(startup_data);

            Ok(())
        })
        .manage(app)
        .invoke_handler(tauri::generate_handler![
            add_empty_byml_file,
            extract_opened_sarc,
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
            open_dir_dialog,
            add_files_from_dir_recursively,
            //COMPARER
            compare_files,
            compare_internal_file_with_vanila,
            check_if_update_needed
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

fn pipe_worker() {
    thread::spawn(|| {
        let pipe_name = r"//./pipe/tauri_pipe";

        // Create the named pipe
        if let Ok(pipe) = NamedPipeBuilder::new(pipe_name).first(true).create() {
            println!("[+] Named pipe created: {}", pipe_name);
            if let Ok(_) = pipe.connect() {
                println!("[+] Connected to named pipe");
                let reader = BufReader::new(pipe);
                for line in reader.lines() {
                    match line {
                        Ok(msg) => {
                            let size = msg.len();
                            if size >= 3 && &msg[0..3] == "END" {
                                println!("Received end message, closing pipe...");
                                break;
                            }
                            if size >= 4 && &msg[0..4] == "KILL" {
                                println!("Received kill message, ending program...");
                                process::exit(0);
                            }
                            println!("Received message: {}", msg);
                        }
                        Err(e) => {
                            eprintln!("Error reading from pipe: {}", e);
                            // break;
                            sleep(Duration::from_secs(1));

                        }
                    }
                }
            } else {
                println!("[-] Error connecting to named pipe: {}", pipe_name);
            }
        } else {
            println!("[-] Error creating named pipe: {}", pipe_name);
        }
        
    });
}

fn main_initialization() -> io::Result<()> {
    #[allow(unused_variables)]
    let exe_cwd = get_executable_dir();
    if exe_cwd.len() > 0 {
        env::set_current_dir(&exe_cwd)?;
    }
    let version = env!("CARGO_PKG_VERSION").to_string();
    println!("[+] Totkbits version: {}", &version);
    println!("[+] Current directory: {:?}", exe_cwd);
    // let installed_ver = TotkbitsVersion::from_str(&version);
    Ok(())
}

