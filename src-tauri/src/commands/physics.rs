use crate::DocumentState::DocumentState;
use rfd::MessageDialog;
use tauri::Manager;

#[tauri::command]
pub fn list_open_bphcl_documents(
    app_handle: tauri::AppHandle,
) -> Vec<crate::DocumentState::OpenBphclDocument> {
    app_handle.state::<DocumentState>().open_bphcl_documents()
}

#[tauri::command]
pub fn list_bphcl_selectable_nodes(
    app_handle: tauri::AppHandle,
    documentId: String,
) -> Result<Vec<crate::DocumentState::BphclSelectableNode>, String> {
    app_handle
        .state::<DocumentState>()
        .bphcl_selectable_nodes(&documentId)
}

#[tauri::command]
pub fn list_open_hkcl_documents(
    app_handle: tauri::AppHandle,
) -> Vec<crate::DocumentState::OpenHkclDocument> {
    app_handle.state::<DocumentState>().open_hkcl_documents()
}

#[tauri::command]
pub fn list_open_bphhb_documents(
    app_handle: tauri::AppHandle,
) -> Vec<crate::DocumentState::OpenBphhbDocument> {
    app_handle.state::<DocumentState>().open_bphhb_documents()
}

#[tauri::command]
pub fn list_hkcl_selectable_nodes(
    app_handle: tauri::AppHandle,
    documentId: String,
) -> Result<Vec<crate::DocumentState::HkclSelectableNode>, String> {
    app_handle
        .state::<DocumentState>()
        .hkcl_selectable_nodes(&documentId)
}

#[tauri::command]
pub fn validate_physics_merge_request(
    app_handle: tauri::AppHandle,
    request: crate::DocumentState::PhysicsMergeRequest,
) -> crate::DocumentState::PhysicsMergeValidation {
    app_handle
        .state::<DocumentState>()
        .validate_physics_merge_request(&request)
}

#[tauri::command]
pub fn build_physics_merge_graph(
    app_handle: tauri::AppHandle,
    request: crate::DocumentState::PhysicsMergeRequest,
) -> Result<crate::DocumentState::PhysicsGraphMergeResult, String> {
    app_handle
        .state::<DocumentState>()
        .build_physics_merge_graph(&request)
}

#[tauri::command]
pub fn merge_hkcl_nodes_into_bphcl(
    app_handle: tauri::AppHandle,
    request: crate::DocumentState::PhysicsMergeRequest,
) -> Result<crate::DocumentState::BphclMergeResult, String> {
    app_handle
        .state::<DocumentState>()
        .merge_hkcl_nodes_into_bphcl(&request)
}

#[tauri::command]
pub fn commit_rebuilt_physics_document(
    app_handle: tauri::AppHandle,
    request: crate::DocumentState::RebuiltPhysicsDocument,
) -> Result<crate::DocumentState::PhysicsDocumentUpdateResult, String> {
    app_handle
        .state::<DocumentState>()
        .commit_rebuilt_physics_document(request)
}

#[tauri::command]
pub fn remove_bphcl_node(
    app_handle: tauri::AppHandle,
    documentId: String,
    path: String,
) -> Result<crate::DocumentState::BphclMutationResult, String> {
    app_handle
        .state::<DocumentState>()
        .remove_bphcl_node(&documentId, &path)
}

#[tauri::command]
pub fn validate_bphcl_merge_documents(app_handle: tauri::AppHandle) -> bool {
    if app_handle
        .state::<DocumentState>()
        .open_bphcl_documents()
        .len()
        >= 2
    {
        return true;
    }

    MessageDialog::new()
        .set_title("TotkBits - Physics Merge")
        .set_description("Open at least two BPHCL documents before using Physics Merge.")
        .set_level(rfd::MessageLevel::Warning)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    false
}

#[tauri::command]
pub fn merge_bphcl_nodes(
    app_handle: tauri::AppHandle,
    targetDocumentId: String,
    sourceDocumentId: String,
    nodeIds: Vec<String>,
) -> Result<crate::DocumentState::BphclMergeResult, String> {
    let result = app_handle.state::<DocumentState>().merge_bphcl_nodes(
        &targetDocumentId,
        &sourceDocumentId,
        &nodeIds,
    )?;
    let imported = if result.imported.is_empty() {
        "  (none)".into()
    } else {
        result
            .imported
            .iter()
            .map(|name| format!("  - {name}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let skipped = if result.skipped.is_empty() {
        "  (none)".into()
    } else {
        result
            .skipped
            .iter()
            .map(|item| format!("  - {} — {}", item.name, item.reason))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let text = format!(
        "Physics merge completed.\n\nNodes added: {} ({} cloth, {} collidables)\nSelections merged: {}\n{}\n\nSkipped selections: {}\n{}",
        result.imported_count,
        result.added_cloth_count,
        result.added_collidable_count,
        result.imported_selection_count,
        imported,
        result.skipped_count,
        skipped
    );
    MessageDialog::new()
        .set_title("TotkBits - Physics Merge")
        .set_description(text)
        .set_level(rfd::MessageLevel::Info)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    Ok(result)
}

#[tauri::command]
pub fn list_open_sidecar_documents(
    app_handle: tauri::AppHandle,
) -> Vec<crate::DocumentState::OpenSidecarDocument> {
    app_handle.state::<DocumentState>().open_sidecar_documents()
}

#[tauri::command]
pub fn list_sidecar_driver_groups(
    app_handle: tauri::AppHandle,
    documentId: String,
) -> Result<Vec<crate::parser::physics::SidecarDriverGroup>, String> {
    app_handle
        .state::<DocumentState>()
        .sidecar_driver_groups(&documentId)
}

#[tauri::command]
pub fn merge_sidecar_driver_group(
    app_handle: tauri::AppHandle,
    targetDocumentId: String,
    sourceDocumentId: String,
    groupIndex: usize,
) -> Result<crate::DocumentState::SidecarMergeResult, String> {
    app_handle
        .state::<DocumentState>()
        .merge_sidecar_driver_group(&targetDocumentId, &sourceDocumentId, groupIndex)
}

#[tauri::command]
pub fn mirror_sidecar_driver_group(
    app_handle: tauri::AppHandle,
    documentId: String,
    groupIndex: usize,
) -> Result<crate::DocumentState::SidecarMergeResult, String> {
    app_handle
        .state::<DocumentState>()
        .mirror_sidecar_driver_group(&documentId, groupIndex)
}

#[tauri::command]
pub fn compact_bphcl_document(
    app_handle: tauri::AppHandle,
    documentId: String,
) -> Result<crate::DocumentState::BphclMutationResult, String> {
    app_handle
        .state::<DocumentState>()
        .compact_bphcl_document(&documentId)
}
