use crate::utils::Pathlib;
use crate::{DocumentState::DocumentState, Open_and_Save::SendData, Settings::NO_WINDOW_FLAG};
use rfd::MessageDialog;
use serde_json::Value;
use std::collections::HashMap;
use std::{fs, os::windows::process::CommandExt, path::Path, process::Command};
use tauri::Manager;

#[tauri::command]
pub fn get_toml_config(app_handle: tauri::AppHandle, documentId: String) -> Result<Value, String> {
    with_document!(
        app_handle,
        documentId,
        app,
        app.zstd.totk_config.to_json().map_err(|e| e.to_string())
    )
}

#[tauri::command]
pub fn get_recent_files() -> Result<Vec<String>, String> {
    crate::TotkConfig::TotkConfig::safe_new(false)
        .map(|config| config.recent_files)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_aoc_model_catalog(
) -> Result<Option<HashMap<String, crate::parser::AOC::g1m::AocModelEntry>>, String> {
    let config =
        crate::TotkConfig::TotkConfig::safe_new(false).map_err(|error| error.to_string())?;
    let dump_path = Path::new(&config.aoc_path);
    if config.aoc_path.is_empty()
        || !dump_path
            .join("KIDSSystemResource")
            .join("kidsobjdb")
            .join("CharacterEditor.kidsobjdb")
            .is_file()
    {
        return Ok(None);
    }

    let mut catalog = crate::parser::AOC::g1m::aoc_names().clone();
    if catalog.is_empty() {
        return Ok(None);
    }
    for hash in crate::parser::AOC::g1m::model_texture_pairs().keys() {
        catalog.entry(hash.clone()).or_default();
    }
    Ok(Some(catalog))
}

#[tauri::command]
pub fn preview_aoc_model(hash: String) -> Result<Option<String>, String> {
    if hash.len() != 8 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid AOC model hash".into());
    }
    let config =
        crate::TotkConfig::TotkConfig::safe_new(false).map_err(|error| error.to_string())?;
    let dump_path = Path::new(&config.aoc_path);
    let filename = format!("{}.g1m", hash.to_ascii_lowercase());
    let model_path = [
        dump_path.join(&filename),
        dump_path
            .join("CharacterEditor")
            .join("g1m")
            .join(&filename),
        dump_path.join("MaterialEditor").join("g1m").join(&filename),
        dump_path.join("FieldEditor4").join("g1m").join(&filename),
        dump_path
            .join("CharacterEditor")
            .join("g1m_merged")
            .join(hash.to_ascii_lowercase())
            .join(&filename),
        dump_path.join("g1m").join(&filename),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .or_else(|| {
        fs::read_dir(dump_path)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("g1m").join(&filename))
            .find(|path| path.is_file())
    });
    let Some(model_path) = model_path else {
        MessageDialog::new()
            .set_title("AOC model not found")
            .set_description(format!(
                "The model {filename} does not exist in the configured AOC dump path."
            ))
            .set_level(rfd::MessageLevel::Error)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
        return Ok(None);
    };
    Ok(Some(model_path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn get_viewport_brightness() -> Result<f64, String> {
    crate::TotkConfig::TotkConfig::safe_new(false)
        .map(|config| config.viewport_brightness)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_viewport_brightness(brightness: f64) -> Result<(), String> {
    if !brightness.is_finite() {
        return Err("invalid 3D viewport brightness".into());
    }
    let mut config =
        crate::TotkConfig::TotkConfig::safe_new(false).map_err(|error| error.to_string())?;
    config.viewport_brightness = brightness.clamp(0.3, 3.0);
    config.save().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_toml_config(
    app_handle: tauri::AppHandle,
    documentId: String,
    new_config: HashMap<String, Value>,
) -> Result<SendData, String> {
    let mut config = with_document!(app_handle, documentId, app, (*app.zstd.totk_config).clone());
    config.update_from_json_data(new_config);
    config.save().map_err(|e| e.to_string())?;
    let status_text = app_handle
        .state::<DocumentState>()
        .update_runtime_config(config);
    let mut data = SendData::default();
    data.status_text = status_text;
    Ok(data)
}

#[tauri::command]
pub fn restart_app(app_handle: tauri::AppHandle) -> Option<()> {
    if let rfd::MessageDialogResult::No = MessageDialog::new()
        .set_title("Warning")
        .set_description("Totkbits will be restarted, all unsaved progress will be lost. Proceed?")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
    {
        return Some(());
    }
    app_handle.request_restart();
    Some(())
}

#[tauri::command]
pub fn edit_config(app_handle: tauri::AppHandle, documentId: String) -> Result<(), String> {
    let file_path = with_document!(
        app_handle,
        documentId,
        app,
        app.zstd.totk_config.config_path.clone()
    );
    // Command::new("explorer.exe")
    let dir_path = Pathlib::new(&file_path).parent.replace("/", "\\");
    // let explorer_arg = format!("/e,\"{}\"", &dir_path);
    println!("{}", &dir_path);
    // Command::new("explorer.exe")
    // Command::new("explorer.exe")
    //     .creation_flags(NO_WINDOW_FLAG)
    //     .arg(&explorer_arg)
    //     .spawn()
    //     .map(|_| ())
    //     .map_err(|error| format!("failed to launch Explorer for {explorer_arg}: {error}"))
    Command::new("cmd.exe")
        // .creation_flags(NO_WINDOW_FLAG)
        .args(["/c", "start", "", &dir_path])
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("failed to open folder: {error}"))
}
