// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use tauri::Window;
mod file_format;
mod App;
mod Zstd;
mod Settings;
mod TotkConfig;


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
    french-hens: 3"#.to_string()
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![receive_text_from_editor, send_text_to_frontend])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
