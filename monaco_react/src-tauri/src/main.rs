// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use tauri::Window;
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
fn send_text_to_frontend(window: Window, text: String) {
    window.emit("text-from-rust", Some(text)).expect("failed to emit event");
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![receive_text_from_editor, greet, send_text_to_frontend])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
