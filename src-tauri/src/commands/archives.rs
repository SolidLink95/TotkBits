use crate::{DocumentState::DocumentState, Open_and_Save::SendData};
use tauri::Manager;

use super::show_open_error;
#[tauri::command]
pub fn extract_internal_file(
    app_handle: tauri::AppHandle,
    documentId: String,
    internalPath: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || with_document_mut!(app_handle, documentId, app, app.extract_file(internalPath)),
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}
#[tauri::command]
pub fn add_empty_byml_file(
    app_handle: tauri::AppHandle,
    documentId: String,
    path: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || with_document_mut!(app_handle, documentId, app, app.add_empty_byml(path)),
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn edit_internal_file(
    app_handle: tauri::AppHandle,
    documentId: String,
    parentDocumentId: String,
    path: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            let result = app_handle.state::<DocumentState>().open_internal_child(
                &parentDocumentId,
                &documentId,
                None,
                path.clone(),
            );
            let result = result.or_else(|| {
            let mut data = SendData::default();
            data.tab = "ERROR".into();
            data.status_text = format!(
                "Error: unable to open archive entry {path}. The entry is missing, unsupported, or invalid."
            );
            Some(data)
        });
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
pub fn open_bphcl_leaf(
    app_handle: tauri::AppHandle,
    documentId: String,
    parentDocumentId: String,
    path: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle.state::<DocumentState>().open_bphcl_leaf(
                &parentDocumentId,
                &documentId,
                path,
            )
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn expand_nested_sarc(
    app_handle: tauri::AppHandle,
    documentId: String,
    outerPath: String,
) -> SendData {
    crate::Settings::catch_panic_with(
        move || {
            let result = with_document_mut!(
                app_handle,
                documentId,
                app,
                app.expand_nested_sarc(outerPath)
            );
            if result.tab == "ERROR" && result.status_text.contains("WinRAR rar.exe was not found")
            {
                show_open_error(&result);
            }
            result
        },
        crate::Open_and_Save::SendData::panicked,
    )
}

#[tauri::command]
pub fn edit_nested_sarc_file(
    app_handle: tauri::AppHandle,
    documentId: String,
    parentDocumentId: String,
    outerPath: String,
    innerPath: String,
) -> SendData {
    crate::Settings::catch_panic_with(
        move || {
            let display_path = format!("{outerPath}::{innerPath}");
            let result = app_handle
            .state::<DocumentState>()
            .open_internal_child(&parentDocumentId, &documentId, Some(outerPath), innerPath)
            .unwrap_or_else(|| {
                let mut data = SendData::default();
                data.tab = "ERROR".into();
                data.status_text = format!(
                    "Error: unable to open archive entry {display_path}. The entry is missing, unsupported, or invalid."
                );
                data
            });
            if result.tab == "ERROR" {
                show_open_error(&result);
            }
            result
        },
        crate::Open_and_Save::SendData::panicked,
    )
}

#[tauri::command]
pub fn extract_nested_sarc_file(
    app_handle: tauri::AppHandle,
    documentId: String,
    outerPath: String,
    innerPath: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            with_document_mut!(
                app_handle,
                documentId,
                app,
                app.extract_nested_sarc_file(outerPath, innerPath)
            )
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn mutate_nested_archive(
    app_handle: tauri::AppHandle,
    documentId: String,
    chain: String,
    path: String,
    action: String,
    newPath: Option<String>,
    sourcePath: Option<String>,
) -> SendData {
    crate::Settings::catch_panic_with(
        move || {
            with_document_mut!(
                app_handle,
                documentId,
                app,
                app.mutate_nested_archive(chain, path, action, newPath, sourcePath)
            )
        },
        crate::Open_and_Save::SendData::panicked,
    )
}

#[tauri::command]
pub fn extract_opened_sarc(app_handle: tauri::AppHandle, documentId: String) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || with_document!(app_handle, documentId, app, app.extract_opened_sarc()),
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn extract_folder_from_opened_sarc(
    app_handle: tauri::AppHandle,
    documentId: String,
    source_folder: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            with_document_mut!(
                app_handle,
                documentId,
                app,
                app.extract_folder_from_opened_sarc(source_folder)
            )
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}
