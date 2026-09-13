use crate::{DocumentState::DocumentState, Open_and_Save::SendData};
use tauri::Manager;

#[tauri::command]
pub fn rstb_get_entries(
    app_handle: tauri::AppHandle,
    documentId: String,
    entry: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            with_document_mut!(
                app_handle,
                documentId,
                app,
                app.get_rstb_entries_by_query(entry)
            )
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn rstb_edit_entry(
    app_handle: tauri::AppHandle,
    documentId: String,
    entry: String,
    val: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || with_document_mut!(app_handle, documentId, app, app.rstb_edit_entry(entry, val)),
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn rstb_remove_entry(
    app_handle: tauri::AppHandle,
    documentId: String,
    entry: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || with_document_mut!(app_handle, documentId, app, app.rstb_remove_entry(entry)),
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}
