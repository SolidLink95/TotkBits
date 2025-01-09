//tauri commands
use crate::{
    Open_and_Save::SendData, Settings::{check_if_update_needed_standalone, spawn_updater}, TotkApp::{SaveData, TotkBitsApp}
};
use rfd::MessageDialog;
use serde::Deserialize;
use updater::TotkbitsVersion::TotkbitsVersion;
use std::{
    env, error::Error, os::windows::process::CommandExt, path::Path, process::{self, Command}, sync::Mutex
};
use tauri::Manager;
use reqwest::blocking::{get, Client};

#[tauri::command]
pub fn restart_app() -> Option<()> {
    let totkbits_exe = env::current_exe().ok()?;
    let no_window_flag = 0x08000000;
    if let rfd::MessageDialogResult::No = MessageDialog::new()
        .set_title("Warning")
        .set_description("Totkbits will be restarted, all unsaved progress will be lost. Proceed?")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
    {
        return Some(());
    }
    // let _ = Command::new(totkbits_exe)
    let p = Command::new("cmd")
        .creation_flags(no_window_flag)
        .args([
            "/C",
            "start",
            "",
            &totkbits_exe.to_string_lossy().into_owned(),
        ])
        .spawn();
        // .map(|_| ())?;
        // .ok()?;
    match p {
        Ok(_) => process::exit(0),
        Err(_) => {return None;},
    };
    // process::exit(0);
    // #[allow(unreachable_code)]
    Some(())
}

#[tauri::command]
pub fn edit_config(app_handle: tauri::AppHandle) -> Option<()> {
    let no_window_flag = 0x08000000;
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let app = binding.lock().expect("Failed to lock state");
    let file_path = app.zstd.clone().totk_config.config_path.clone();
    let os_type = env::consts::OS;

    let result = match os_type {
        "windows" => Command::new("cmd")
            .creation_flags(no_window_flag)
            .args(["/C", "start", "", &file_path])
            .status(),
        "macos" => Command::new("open")
            .creation_flags(no_window_flag)
            .arg(file_path)
            .status(),
        "linux" => Command::new("xdg-open")
            .creation_flags(no_window_flag)
            .arg(file_path)
            .status(),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Unsupported OS",
        )),
    };

    let _ = result.map(|exit_status| {
        if exit_status.success() {
            return Some(());
        } else {
            return None;
        }
    });
    None
}

#[tauri::command]
pub fn extract_internal_file(
    app_handle: tauri::AppHandle,
    internalPath: String,
) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");

    match app.extract_file(internalPath) {
        Some(result) => Some(result), // Safely return the result if present
        None => None,                 // Return None if no result
    }
}

#[tauri::command]
pub fn add_empty_byml_file(app_handle: tauri::AppHandle, path: String) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");

    match app.add_empty_byml(path) {
        Some(result) => Some(result), // Safely return the result if present
        None => None,                 // Return None if no result
    }
}

#[tauri::command]
pub fn edit_internal_file(app_handle: tauri::AppHandle, path: String) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");

    match app.edit_internal_file(path) {
        Some(result) => Some(result), // Safely return the result if present
        None => None,                 // Return None if no result
    }
}

#[tauri::command]
pub fn extract_opened_sarc(app_handle: tauri::AppHandle) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let app = binding.lock().expect("Failed to lock state");

    match app.extract_opened_sarc() {
        Some(result) => Some(result), // Safely return the result if present
        None => None,                 // Return None if no result
    }
}

#[tauri::command]
pub fn save_as_click(app_handle: tauri::AppHandle, save_data: SaveData) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");

    match app.save_as(save_data) {
        Some(result) => Some(result), // Safely return the result if present
        None => None,                 // Return None if no result
    }
}

#[tauri::command]
pub fn add_click(
    app_handle: tauri::AppHandle,
    internalPath: String,
    path: String,
    overwrite: bool,
) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    println!("internal_path: {}", internalPath);
    match app.add_internal_file_from_path(internalPath, path, overwrite) {
        Some(result) => Some(result), // Safely return the result if present
        None => None,                 // Return None if no result
    }
}

#[tauri::command]
pub fn add_files_from_dir_recursively(
    app_handle: tauri::AppHandle,
    internalPath: String,
    path: String,
) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    println!("internal_path: {}", internalPath);
    // if path_
    match app.add_dir_to_sarc(internalPath, path) {
        Some(result) => Some(result), // Safely return the result if present
        None => None,                 // Return None if no result
    }
}

#[tauri::command]
pub fn add_to_dir_click(
    app_handle: tauri::AppHandle,
    internalPath: String,
    path: String,
) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    println!("internal_path: {}", internalPath);
    // if path_
    match app.add_internal_file_to_dir(internalPath, path) {
        Some(result) => Some(result), // Safely return the result if present
        None => None,                 // Return None if no result
    }
}

// #[tauri::command]
// pub fn get_status_text(app: tauri::State<'_, TotkBitsApp>) -> String {
//     let result = panic::catch_unwind(AssertUnwindSafe(|| {
//         app.inner().send_status_text();
//     }));
//     if result.is_err() {
//         return "Error".to_string();
//     }
//     app.status_text.clone()
// }

#[tauri::command]
pub fn open_file_struct(app_handle: tauri::AppHandle, _window: tauri::Window) -> Option<SendData> {
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
pub fn open_file_from_path(app_handle: tauri::AppHandle, path: String) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.open_from_path(path.replace("\\", "/")) {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}

#[tauri::command]
pub fn remove_internal_sarc_file(
    app_handle: tauri::AppHandle,
    internalPath: String,
) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.remove_internal_elem(internalPath) {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}

#[tauri::command]
pub fn save_file_struct(app_handle: tauri::AppHandle, save_data: SaveData) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.save(save_data) {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}
#[tauri::command]
pub fn rename_internal_sarc_file(
    app_handle: tauri::AppHandle,
    internalPath: String,
    newInternalPath: String,
) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.rename_internal_file_from_path(internalPath, newInternalPath) {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}

#[tauri::command]
pub fn close_all_opened_files(app_handle: tauri::AppHandle) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.close_all_click() {
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

#[tauri::command]
pub fn open_file_dialog() -> Option<String> {
    match rfd::FileDialog::new().pick_file() {
        Some(path) => Some(path.to_string_lossy().to_string().replace("\\", "/")),
        None => None,
    }
}

#[tauri::command]
pub fn open_dir_dialog() -> Option<String> {
    match rfd::FileDialog::new().pick_folder() {
        Some(path) => Some(path.to_string_lossy().to_string().replace("\\", "/")),
        None => None,
    }
}

#[tauri::command]
pub fn rstb_get_entries(app_handle: tauri::AppHandle, entry: String) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.get_rstb_entries_by_query(entry) {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}

#[tauri::command]
pub fn rstb_edit_entry(
    app_handle: tauri::AppHandle,
    entry: String,
    val: String,
) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.rstb_edit_entry(entry, val) {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}

#[tauri::command]
pub fn rstb_remove_entry(app_handle: tauri::AppHandle, entry: String) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.rstb_remove_entry(entry) {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}

#[tauri::command]
pub fn search_in_sarc(app_handle: tauri::AppHandle, query: String) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.search_in_sarc(query) {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}

#[tauri::command]
pub fn clear_search_in_sarc(app_handle: tauri::AppHandle) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.clear_search_in_sarc() {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}

//COMPARE stuff
#[tauri::command]
pub fn compare_files(app_handle: tauri::AppHandle, isFromDisk: bool) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let app = binding.lock().expect("Failed to lock state");
    match app.compare_files(isFromDisk) {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}

#[tauri::command]
pub fn compare_internal_file_with_vanila(app_handle: tauri::AppHandle,  internal_path: String, is_from_sarc: bool) -> Option<SendData> {
    let binding = app_handle.state::<Mutex<TotkBitsApp>>();
    let mut app = binding.lock().expect("Failed to lock state");
    match app.compare_internal_file_with_original(internal_path, is_from_sarc) {
        Some(result) => {
            return Some(result);
        } // Safely return the result if present
        None => {} // Return None if no result
    }
    None
}


#[tauri::command]
pub fn check_if_update_needed() -> String {
    let repo_owner = "SolidLink95".to_string();
    let repo_name = "TotkBits".to_string();
    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        repo_owner, repo_name
    );
    println!("Checking for updates...");
    let client = Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "MyAppName")
        .send();

        if let Ok(response) = response {
            // println!("Response: {:?}", response);
    
            if let Ok(json_value) = response.json::<serde_json::Value>() {
                // println!("\n\nJson value: {:?}", json_value);
                if let Some(release_info) = json_value["tag_name"].as_str() {
                    // println!("\n\nRelease info: {}", release_info);
                    let installed_ver = TotkbitsVersion::from_str(env!("CARGO_PKG_VERSION"));
                    let latest_ver = TotkbitsVersion::from_str(release_info);
                    if latest_ver > installed_ver {
                        return release_info.to_string();
                    }
                }
            }
        }
    String::new()
}

#[tauri::command]
pub fn update_app(latestVer: String) -> String {
    
    if let Err(e) = spawn_updater(latestVer.as_str()) {
        return format!("Error spawning updater: {:?}", e);
    }
    // process::exit(1);
    String::new()
}