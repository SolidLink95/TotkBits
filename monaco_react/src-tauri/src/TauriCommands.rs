//tauri commands
use std::{panic::{self, AssertUnwindSafe}, process, sync::Mutex};
use rfd::MessageDialog;
use tauri::Manager;
use crate::TotkApp::{SendData, TotkBitsApp};


#[tauri::command]
pub fn open_internal_file(app_handle: tauri::AppHandle, path: String) -> Option<SendData>{
   // pub fn open_file(app_handle: tauri::AppHandle) -> Result<Option<String>, String> {
        // Lock the mutex to get mutable access to your state
        let binding = app_handle.state::<Mutex<TotkBitsApp>>();
        let mut app = binding.lock().expect("Failed to lock state");
    
        match app.open_internal_file(path) {
            Some(result) => Some(result), // Safely return the result if present
            None => None,                      // Return None if no result
        }
}

#[tauri::command]
pub fn get_status_text(app: tauri::State<'_, TotkBitsApp>) -> String {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        app.inner().send_status_text();
    }));
    if result.is_err() {
        return "Error".to_string();
    }
    app.status_text.clone()
}

#[tauri::command]
pub fn open_file(app_handle: tauri::AppHandle) -> Result<Option<String>, String> {
    // Lock the mutex to get mutable access to your state
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");

    match app.open() {
        Some(result) => Ok(Some(result.text)), // Safely return the result if present
        None => Ok(None),                      // Return None if no result
    }
}

#[tauri::command]
pub fn open_file_struct(app_handle: tauri::AppHandle, window: tauri::Window) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.open() {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}

#[tauri::command]
pub fn exit_app() {
    if MessageDialog::new()
        .set_title("Warning")
        .set_description("The program will be closed, all unsaved progress will be lost. Proceed?")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
    {
        process::exit(0); // Replace 0 with the desired exit code
    }
}