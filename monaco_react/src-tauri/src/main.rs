// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::sync::Mutex;

use crate::TotkApp::{get_status_text, open_file, TotkBitsApp,open_file_struct};
use tauri::{App, Window};
mod Settings;
mod TotkApp;
mod TotkConfig;
mod Zstd;
mod file_format;

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn receive_text_from_editor(text: String) {
    println!("Received text: {}", text);
    // You can process the text here
}

#[tauri::command]
fn send_text_to_frontend() -> String {
    r#"---
    doe: "a deer, a female deer"
    ray: "a drop of golden sun"
    pi: 3.14159
    xmas: true
    french-hens: 3"#
        .to_string()
}

fn main() {
    let mut app = Mutex::<TotkBitsApp>::default();
    tauri::Builder::default()
        .manage(app)
        .invoke_handler(tauri::generate_handler![
            receive_text_from_editor,
            send_text_to_frontend,
            get_status_text,
            open_file,
            open_file_struct,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
