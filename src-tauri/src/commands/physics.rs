use crate::DocumentState::DocumentState;
use rfd::MessageDialog;
use tauri::Manager;

#[tauri::command]
pub fn list_open_bphcl_documents(
    app_handle: tauri::AppHandle,
) -> Vec<crate::DocumentState::OpenBphclDocument> {
    crate::Settings::catch_panic_with(
        move || app_handle.state::<DocumentState>().open_bphcl_documents(),
        |_| Vec::new(),
    )
}

#[tauri::command]
pub fn list_bphcl_selectable_nodes(
    app_handle: tauri::AppHandle,
    documentId: String,
) -> Result<Vec<crate::DocumentState::BphclSelectableNode>, String> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .bphcl_selectable_nodes(&documentId)
        },
        Err,
    )
}

#[tauri::command]
pub fn list_open_hkcl_documents(
    app_handle: tauri::AppHandle,
) -> Vec<crate::DocumentState::OpenHkclDocument> {
    crate::Settings::catch_panic_with(
        move || app_handle.state::<DocumentState>().open_hkcl_documents(),
        |_| Vec::new(),
    )
}

#[tauri::command]
pub fn list_open_bphhb_documents(
    app_handle: tauri::AppHandle,
) -> Vec<crate::DocumentState::OpenBphhbDocument> {
    crate::Settings::catch_panic_with(
        move || app_handle.state::<DocumentState>().open_bphhb_documents(),
        |_| Vec::new(),
    )
}

#[tauri::command]
pub fn list_hkcl_selectable_nodes(
    app_handle: tauri::AppHandle,
    documentId: String,
) -> Result<Vec<crate::DocumentState::HkclSelectableNode>, String> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .hkcl_selectable_nodes(&documentId)
        },
        Err,
    )
}

#[tauri::command]
pub fn validate_physics_merge_request(
    app_handle: tauri::AppHandle,
    request: crate::DocumentState::PhysicsMergeRequest,
) -> crate::DocumentState::PhysicsMergeValidation {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .validate_physics_merge_request(&request)
        },
        |message| crate::DocumentState::PhysicsMergeValidation {
            valid: false,
            issues: vec![message],
            requires_template: false,
            supports_collidables: false,
            helper_used: false,
        },
    )
}

#[tauri::command]
pub fn build_physics_merge_graph(
    app_handle: tauri::AppHandle,
    request: crate::DocumentState::PhysicsMergeRequest,
) -> Result<crate::DocumentState::PhysicsGraphMergeResult, String> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .build_physics_merge_graph(&request)
        },
        Err,
    )
}

#[tauri::command]
pub fn merge_hkcl_nodes_into_bphcl(
    app_handle: tauri::AppHandle,
    request: crate::DocumentState::PhysicsMergeRequest,
) -> Result<crate::DocumentState::BphclMergeResult, String> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .merge_hkcl_nodes_into_bphcl(&request)
        },
        Err,
    )
}

#[tauri::command]
pub fn commit_rebuilt_physics_document(
    app_handle: tauri::AppHandle,
    request: crate::DocumentState::RebuiltPhysicsDocument,
) -> Result<crate::DocumentState::PhysicsDocumentUpdateResult, String> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .commit_rebuilt_physics_document(request)
        },
        Err,
    )
}

#[tauri::command]
pub fn remove_bphcl_node(
    app_handle: tauri::AppHandle,
    documentId: String,
    path: String,
) -> Result<crate::DocumentState::BphclMutationResult, String> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .remove_bphcl_node(&documentId, &path)
        },
        Err,
    )
}

#[tauri::command]
pub fn validate_bphcl_merge_documents(app_handle: tauri::AppHandle) -> bool {
    crate::Settings::catch_panic_with(
        move || {
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
        },
        |_| false,
    )
}

#[tauri::command]
pub fn merge_bphcl_nodes(
    app_handle: tauri::AppHandle,
    targetDocumentId: String,
    sourceDocumentId: String,
    nodeIds: Vec<String>,
) -> Result<crate::DocumentState::BphclMergeResult, String> {
    crate::Settings::catch_panic_with(
        move || {
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
        },
        Err,
    )
}

#[tauri::command]
pub fn list_open_sidecar_documents(
    app_handle: tauri::AppHandle,
) -> Vec<crate::DocumentState::OpenSidecarDocument> {
    crate::Settings::catch_panic_with(
        move || app_handle.state::<DocumentState>().open_sidecar_documents(),
        |_| Vec::new(),
    )
}

#[tauri::command]
pub fn list_sidecar_driver_groups(
    app_handle: tauri::AppHandle,
    documentId: String,
) -> Result<Vec<crate::parser::physics::SidecarDriverGroup>, String> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .sidecar_driver_groups(&documentId)
        },
        Err,
    )
}

#[tauri::command]
pub fn merge_sidecar_driver_group(
    app_handle: tauri::AppHandle,
    targetDocumentId: String,
    sourceDocumentId: String,
    groupIndex: usize,
) -> Result<crate::DocumentState::SidecarMergeResult, String> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .merge_sidecar_driver_group(&targetDocumentId, &sourceDocumentId, groupIndex)
        },
        Err,
    )
}

#[tauri::command]
pub fn mirror_sidecar_driver_group(
    app_handle: tauri::AppHandle,
    documentId: String,
    groupIndex: usize,
) -> Result<crate::DocumentState::SidecarMergeResult, String> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .mirror_sidecar_driver_group(&documentId, groupIndex)
        },
        Err,
    )
}

#[tauri::command]
pub fn compact_bphcl_document(
    app_handle: tauri::AppHandle,
    documentId: String,
) -> Result<crate::DocumentState::BphclMutationResult, String> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .compact_bphcl_document(&documentId)
        },
        Err,
    )
}

/// Converts an HKCL into a BPHCL and installs it into the actor pack at
/// `packPath` (overwritten in place); returns what was registered.
#[tauri::command]
pub fn convert_hkcl_into_pack(
    hkclPath: String,
    packPath: String,
    scale: f32,
) -> Result<crate::tools::hkcl_convert::HkclConversionSummary, String> {
    crate::Settings::catch_panic_with(
        move || {
            let config = std::sync::Arc::new(
                crate::TotkConfig::TotkConfig::safe_new(false)
                    .map_err(|error| error.to_string())?,
            );
            let zstd = crate::Zstd::TotkZstd::new(
                config.clone(),
                crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
            )
            .unwrap_or_else(|_| {
                crate::Zstd::TotkZstd::dictionaryless(
                    config,
                    crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
                )
            });
            let pack = std::path::PathBuf::from(&packPath);
            crate::tools::hkcl_convert::convert_hkcl_into_pack(
                std::path::Path::new(&hkclPath),
                &pack,
                &pack,
                scale,
                std::sync::Arc::new(zstd),
            )
            .map_err(|error| format!("HKCL to BPHCL failed: {error}"))
        },
        Err,
    )
}

#[tauri::command]
pub fn rescale_bphcl_document(
    app_handle: tauri::AppHandle,
    documentId: String,
    scale: f32,
) -> Result<crate::DocumentState::BphclMutationResult, String> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle
                .state::<DocumentState>()
                .rescale_bphcl_document(&documentId, scale)
        },
        Err,
    )
}
