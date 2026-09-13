use crate::{DocumentState::DocumentState, Open_and_Save::SendData, TotkApp::SaveData};
use base64::Engine;
use rfd::MessageDialog;
use tauri::Manager;

use super::{report_monaco_save_error, show_open_error};
#[tauri::command]
pub fn save_as_click(
    app_handle: tauri::AppHandle,
    documentId: String,
    save_data: SaveData,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            let tab = save_data.tab.clone();
            let result = with_document_mut!(app_handle, documentId, app, app.save_as(save_data));
            // `None` also represents the user cancelling the Save As dialog.
            report_monaco_save_error(&tab, &result, false);
            result
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn add_click(
    app_handle: tauri::AppHandle,
    documentId: String,
    internalPath: String,
    path: String,
    overwrite: bool,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            println!("internal_path: {}", internalPath);
            with_document_mut!(
                app_handle,
                documentId,
                app,
                app.add_internal_file_from_path(internalPath, path, overwrite)
            )
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn add_files_from_dir_recursively(
    app_handle: tauri::AppHandle,
    documentId: String,
    internalPath: String,
    path: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            println!("internal_path: {}", internalPath);
            // if path_
            with_document_mut!(
                app_handle,
                documentId,
                app,
                app.add_dir_to_sarc(internalPath, path)
            )
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn add_to_dir_click(
    app_handle: tauri::AppHandle,
    documentId: String,
    internalPath: String,
    path: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            println!("internal_path: {}", internalPath);
            // if path_
            with_document_mut!(
                app_handle,
                documentId,
                app,
                app.add_internal_file_to_dir(internalPath, path)
            )
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn read_file_base64(path: String) -> Result<String, String> {
    crate::Settings::catch_panic_with(
        move || {
            let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
            Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
        },
        Err,
    )
}

#[tauri::command]
pub fn add_archive_bytes(
    app_handle: tauri::AppHandle,
    documentId: String,
    internalPath: String,
    data: String,
    overwrite: bool,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            let bytes = match base64::engine::general_purpose::STANDARD.decode(data) {
                Ok(bytes) => bytes,
                Err(error) => {
                    let mut result = SendData::default();
                    result.tab = "ERROR".into();
                    result.status_text = format!("Error: invalid Mii data: {error}");
                    return Some(result);
                }
            };
            with_document_mut!(
                app_handle,
                documentId,
                app,
                app.add_internal_file_bytes(internalPath, bytes, overwrite)
            )
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
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
pub fn open_file_struct(
    app_handle: tauri::AppHandle,
    documentId: String,
    _window: tauri::Window,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            let result = with_document_mut!(app_handle, documentId, app, app.open());
            if let Some(data) = &result {
                if data.tab == "ERROR" {
                    show_open_error(data);
                } else if !data.path.full_path.is_empty() {
                    let _ =
                        crate::TotkConfig::TotkConfig::remember_recent_file(&data.path.full_path);
                }
            }
            result
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn open_folder_struct(app_handle: tauri::AppHandle, documentId: String) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            let result = with_document_mut!(app_handle, documentId, app, app.open_folder());
            if let Some(data) = &result {
                if data.tab == "ERROR" {
                    show_open_error(data);
                }
            }
            result
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn open_file_from_path(
    app_handle: tauri::AppHandle,
    documentId: String,
    path: String,
    suppressErrorDialog: bool,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            let result = with_document_mut!(
                app_handle,
                documentId,
                app,
                app.open_from_path(path.replace("\\", "/"))
            );
            if let Some(data) = &result {
                if data.tab == "ERROR" && !suppressErrorDialog {
                    show_open_error(data);
                } else if data.tab != "ERROR" && !data.path.full_path.is_empty() {
                    let _ =
                        crate::TotkConfig::TotkConfig::remember_recent_file(&data.path.full_path);
                }
            }
            result
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn remove_internal_sarc_file(
    app_handle: tauri::AppHandle,
    documentId: String,
    internalPath: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            with_document_mut!(
                app_handle,
                documentId,
                app,
                app.remove_internal_elem(internalPath)
            )
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn clear_rfl_miis(app_handle: tauri::AppHandle, documentId: String) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            let confirmed = MessageDialog::new()
                .set_title("TotkBits - Clear RFL_DB.dat")
                .set_description("Remove all Miis from this RFL_DB.dat?")
                .set_level(rfd::MessageLevel::Warning)
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                == rfd::MessageDialogResult::Yes;
            if !confirmed {
                return None;
            }
            with_document_mut!(
                app_handle,
                documentId,
                app,
                app.remove_internal_elem("Miis".into())
            )
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn save_file_struct(
    app_handle: tauri::AppHandle,
    documentId: String,
    save_data: SaveData,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            let tab = save_data.tab.clone();
            let result = app_handle
                .state::<DocumentState>()
                .save_document(&documentId, save_data);
            report_monaco_save_error(&tab, &result, true);
            result
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}
#[tauri::command]
pub fn rename_internal_sarc_file(
    app_handle: tauri::AppHandle,
    documentId: String,
    internalPath: String,
    newInternalPath: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            with_document_mut!(
                app_handle,
                documentId,
                app,
                app.rename_internal_file_from_path(internalPath, newInternalPath)
            )
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn close_all_opened_files(
    app_handle: tauri::AppHandle,
    documentId: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            let documents = app_handle.state::<DocumentState>();
            let response = documents.with_mut(&documentId, |app| app.close_all_click());
            documents.close_all();
            response
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn close_document(app_handle: tauri::AppHandle, documentId: String) -> bool {
    crate::Settings::catch_panic_with(
        move || app_handle.state::<DocumentState>().close(&documentId),
        |_| false,
    )
}

#[tauri::command]
pub fn exit_app(app_handle: tauri::AppHandle) {
    if MessageDialog::new()
        .set_title("Warning")
        .set_description("The program will be closed, all unsaved progress will be lost. Proceed?")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
    {
        app_handle.exit(0);
    }
}

#[tauri::command]
pub fn open_file_dialog() -> Option<String> {
    crate::Settings::catch_panic_with(
        move || match rfd::FileDialog::new().pick_file() {
            Some(path) => Some(path.to_string_lossy().to_string().replace("\\", "/")),
            None => None,
        },
        |_| None,
    )
}

#[tauri::command]
pub fn open_dir_dialog(title: Option<String>) -> Option<String> {
    crate::Settings::catch_panic_with(
        move || {
            let mut dialog = rfd::FileDialog::new();
            if let Some(title) = title {
                dialog = dialog.set_title(title);
            }
            match dialog.pick_folder() {
                Some(path) => Some(path.to_string_lossy().to_string().replace("\\", "/")),
                None => None,
            }
        },
        |_| None,
    )
}
