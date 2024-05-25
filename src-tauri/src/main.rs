// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// #![windows_subsystem = "windows"]
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(non_snake_case, non_camel_case_types)]
use std::{env, fs, io};
use std::sync::{Arc, Mutex};
use file_format::BinTextFile::BymlFile;
use roead::byml::Byml;
use tauri::Manager;
use Zstd::TotkZstd;
use std::time::SystemTime;
mod Open_and_Save;
mod Settings;
mod TauriCommands;
mod TotkApp;
mod TotkConfig;
mod Zstd;
mod file_format;
use crate::Settings::{get_startup_data, StartupData};
use crate::TauriCommands::{
    add_click, add_empty_byml_file, add_to_dir_click, clear_search_in_sarc, close_all_opened_files,
    edit_config, edit_internal_file, exit_app, extract_internal_file, extract_opened_sarc,
    open_file_dialog, open_file_from_path, open_file_struct, remove_internal_sarc_file,
    rename_internal_sarc_file, restart_app, rstb_edit_entry, rstb_get_entries, rstb_remove_entry,
    save_as_click, save_file_struct, search_in_sarc,
};
use crate::TotkApp::TotkBitsApp;

#[allow(dead_code)]
fn test_case() ->io::Result<()> {
    let start = SystemTime::now();
    let p = "D:/coding/TotkBits/tmp/GameDataList.Product.110.byml.zs";
    println!("1 {}", SystemTime::now().duration_since(start).expect("").as_secs());
    let zstd = TotkZstd::new(Arc::new(TotkConfig::TotkConfig::safe_new()?), 14)?;
    let b = BymlFile::new(p.to_string(), Arc::new(zstd)).unwrap();
    println!("2 {}", SystemTime::now().duration_since(start).expect("").as_secs());
    let e = b.endian.unwrap().clone();
    let text = b.pio.to_text();
    println!("3 {}", SystemTime::now().duration_since(start).expect("").as_secs());
    fs::write("D:/coding/TotkBits/tmp/GameDataList.yml", &text)?;
    println!("4 {}", SystemTime::now().duration_since(start).expect("").as_secs());
    let x = Byml::from_text(text).map_err(|_| io::Error::new(io::ErrorKind::Other, "asdasf"))?;
    println!("5 {}", SystemTime::now().duration_since(start).expect("").as_secs());
    let rawdata = x.to_binary(e);
    println!("6 {}", SystemTime::now().duration_since(start).expect("").as_secs());
    fs::write("D:/coding/TotkBits/tmp/GameDataList.byml", rawdata)?;
    println!("7 {}", SystemTime::now().duration_since(start).expect("").as_secs());



    Ok(())
}

fn main() -> io::Result<()> {
    #[allow(unused_variables)]
    // test_case()?;
    // return Ok(());
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
