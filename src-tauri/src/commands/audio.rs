use crate::{DocumentState::DocumentState, Open_and_Save::SendData};
use serde::Serialize;
use std::{fs, path::Path};
use tauri::Manager;

#[derive(Serialize)]
pub struct BfwavPreview {
    pub path: String,
    pub data_url: String,
    pub size: usize,
    pub sample_rate: u32,
    pub channels: usize,
    pub samples: usize,
    pub looping: bool,
}

#[derive(Serialize)]
pub struct BfwavReplacement {
    pub old_size: usize,
    pub new_size: usize,
    pub increased: bool,
    pub compressed: bool,
    pub sample_rate: u32,
}

#[derive(Serialize)]
pub struct BarsFolderReplacement {
    pub replaced: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
    pub oversized: Vec<String>,
}

#[tauri::command]
pub fn open_bfwav_node(
    app_handle: tauri::AppHandle,
    documentId: String,
    path: String,
) -> Result<BfwavPreview, String> {
    crate::Settings::catch_panic_with(
        move || with_document!(app_handle, documentId, app, app.open_bfwav_node(&path)),
        Err,
    )
}

#[tauri::command]
pub fn replace_bfwav_node(
    app_handle: tauri::AppHandle,
    documentId: String,
    path: String,
    sourcePath: String,
) -> Result<BfwavReplacement, String> {
    crate::Settings::catch_panic_with(
        move || {
            with_document_mut!(
                app_handle,
                documentId,
                app,
                app.replace_bfwav_node(&path, Path::new(&sourcePath), false, None, false)
            )
        },
        Err,
    )
}

#[tauri::command]
pub fn replace_bars_audio_from_folder(
    app_handle: tauri::AppHandle,
    documentId: String,
    folderPath: String,
) -> Result<BarsFolderReplacement, String> {
    crate::Settings::catch_panic_with(
        move || {
            with_document_mut!(app_handle, documentId, app, {
                app.replace_bars_audio_from_folder(Path::new(&folderPath), false, false)
            })
        },
        Err,
    )
}

#[tauri::command]
pub fn open_amta_node(
    app_handle: tauri::AppHandle,
    documentId: String,
    parentDocumentId: String,
    path: String,
) -> Result<SendData, String> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .open_amta_node(&parentDocumentId, &documentId, path)
        },
        Err,
    )
}

#[tauri::command]
pub fn open_audio_file_dialog() -> Option<String> {
    crate::Settings::catch_panic_with(
        move || {
            rfd::FileDialog::new()
                .add_filter("Audio", &["wav", "mp3"])
                .pick_file()
                .map(|path| path.to_string_lossy().into_owned())
        },
        |_| None,
    )
}

#[tauri::command]
pub fn export_bfwav_node(
    app_handle: tauri::AppHandle,
    documentId: String,
    path: String,
    format: String,
) -> Result<Option<String>, String> {
    crate::Settings::catch_panic_with(
        move || {
            let format = format.to_ascii_lowercase();
            if format != "wav" && format != "mp3" {
                return Err("audio export format must be WAV or MP3".into());
            }
            let encoded = with_document!(app_handle, documentId, app, {
                let bytes = app
                    .archive
                    .as_ref()
                    .and_then(|archive| archive.get(&path))
                    .ok_or_else(|| format!("archive entry not found: {path}"))?;
                if format == "wav" {
                    crate::file_format::Audio::to_wav(bytes)
                } else {
                    crate::file_format::Audio::to_mp3(bytes)
                }
            })?;
            let stem = Path::new(&path)
                .file_stem()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .unwrap_or("audio");
            let Some(output) = rfd::FileDialog::new()
                .add_filter(format.to_ascii_uppercase(), &[format.as_str()])
                .set_file_name(format!("{stem}.{format}"))
                .save_file()
            else {
                return Ok(None);
            };
            fs::write(&output, encoded)
                .map_err(|error| format!("failed to export audio: {error}"))?;
            Ok(Some(output.to_string_lossy().into_owned()))
        },
        Err,
    )
}
