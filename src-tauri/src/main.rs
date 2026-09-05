// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// #![windows_subsystem = "windows"]
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(non_snake_case, non_camel_case_types)]
use std::{env, io};
use tauri::Manager;
use Zstd::get_executable_dir;
mod Comparer;
mod DocumentState;
mod InternalFile;
mod TotkFile;
// mod InternalFile_EX;
mod NestedSarc;
mod Open_and_Save;
// mod Open_and_Save_EX;
pub mod utils;
pub use utils as Settings;
mod TauriCommands;
mod TotkApp;
mod TotkConfig;
mod Zstd;
mod compression;
mod file_format;
mod parser;
pub mod tools;
use crate::tools::mii::{download_mii_glb, export_loaded_glb, read_glb_preview, read_mii_name};
use crate::DocumentState::DocumentState as Documents;
use crate::Settings::{get_startup_data, StartupData};
use crate::TauriCommands::{
    add_archive_bytes, add_click, add_empty_byml_file, add_files_from_dir_recursively,
    add_to_dir_click, build_physics_merge_graph, check_if_update_needed, clear_rfl_miis,
    clear_search_in_sarc, close_all_opened_files, close_document, commit_rebuilt_physics_document,
    compact_bphcl_document, compare_files, compare_internal_file_with_vanila, edit_config,
    edit_internal_file, edit_nested_sarc_file, exit_app, expand_nested_sarc, export_bfwav_node,
    export_g1m_fbx, export_g1m_glb, export_image_png, export_lm3_fbx, export_lm3_glb,
    export_viewport_png, extract_folder_from_opened_sarc, extract_internal_file,
    extract_nested_sarc_file, extract_opened_sarc, get_aoc_model_catalog, get_lm3_slot_catalog,
    get_recent_files, get_toml_config, get_viewport_brightness, inspect_3d_model,
    inspect_batch_g1m, inspect_bfres, inspect_g1a_animation, list_batch_render_files,
    list_bphcl_selectable_nodes, list_g1a_animations, list_hkcl_selectable_nodes,
    list_open_bphcl_documents, list_open_bphhb_documents, list_open_hkcl_documents,
    list_open_sidecar_documents, list_sidecar_driver_groups, load_tomodachi_texture_set,
    merge_bphcl_nodes, merge_hkcl_nodes_into_bphcl, merge_sidecar_driver_group,
    mirror_sidecar_driver_group, mutate_nested_archive, open_amta_node, open_audio_file_dialog,
    open_bfwav_node, open_bphcl_leaf, open_dir_dialog, open_file_dialog, open_file_from_path,
    open_file_struct, open_folder_struct, preview_aoc_model, read_file_base64, remove_bphcl_node,
    remove_internal_sarc_file, rename_bntx_texture, rename_internal_sarc_file, render_image,
    replace_bars_audio_from_folder, replace_bfwav_node, replace_bntx_image, replace_dds_image,
    replace_g1m_meshes, restart_app, rstb_edit_entry, rstb_get_entries, rstb_remove_entry,
    save_as_click, save_file_struct, search_in_sarc, set_viewport_brightness, update_toml_config,
    validate_bphcl_merge_documents, validate_physics_merge_request,
};

fn main() -> io::Result<()> {
    // texture2ddecoder panics on malformed game textures; those panics are
    // caught and reported as skipped textures, so the default hook's trace
    // for them is pure console noise. Every other panic keeps its trace.
    let default_panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let from_texture_decoder = info
            .location()
            .is_some_and(|location| location.file().contains("texture2ddecoder"));
        if !from_texture_decoder {
            default_panic_hook(info);
        }
    }));
    let cli = tools::Cli::CliCommand::from_env();
    if let Some(command) = cli {
        return command.execute().map_err(std::io::Error::other);
    }
    main_initialization()?;
    // test_case()?;
    // return Ok(());
    let startup = StartupData::new()?;
    Settings::launch_weapon_icon_cache(&startup.config);
    let startup_data = startup.to_json()?;
    // println!("{:?}", startup_data);
    let documents = Documents::default();
    if let Err(err) = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app_setup| {
            app_setup.manage(startup_data);

            Ok(())
        })
        .manage(documents)
        .invoke_handler(tauri::generate_handler![
            inspect_bfres,
            inspect_3d_model,
            list_g1a_animations,
            load_tomodachi_texture_set,
            inspect_g1a_animation,
            inspect_batch_g1m,
            export_g1m_fbx,
            replace_g1m_meshes,
            export_g1m_glb,
            export_lm3_fbx,
            export_lm3_glb,
            download_mii_glb,
            export_loaded_glb,
            read_glb_preview,
            read_mii_name,
            export_viewport_png,
            list_batch_render_files,
            render_image,
            rename_bntx_texture,
            replace_bntx_image,
            export_image_png,
            replace_dds_image,
            open_bfwav_node,
            replace_bfwav_node,
            replace_bars_audio_from_folder,
            open_amta_node,
            export_bfwav_node,
            open_audio_file_dialog,
            add_empty_byml_file,
            extract_opened_sarc,
            extract_folder_from_opened_sarc,
            get_toml_config,
            get_aoc_model_catalog,
            get_lm3_slot_catalog,
            preview_aoc_model,
            get_viewport_brightness,
            set_viewport_brightness,
            get_recent_files,
            update_toml_config,
            restart_app,
            edit_config,
            get_startup_data,
            open_file_struct,
            open_folder_struct,
            open_file_from_path,
            edit_internal_file,
            open_bphcl_leaf,
            list_open_bphcl_documents,
            list_bphcl_selectable_nodes,
            list_open_hkcl_documents,
            list_open_bphhb_documents,
            list_hkcl_selectable_nodes,
            validate_bphcl_merge_documents,
            merge_bphcl_nodes,
            merge_hkcl_nodes_into_bphcl,
            validate_physics_merge_request,
            build_physics_merge_graph,
            commit_rebuilt_physics_document,
            remove_bphcl_node,
            list_open_sidecar_documents,
            list_sidecar_driver_groups,
            merge_sidecar_driver_group,
            mirror_sidecar_driver_group,
            compact_bphcl_document,
            expand_nested_sarc,
            edit_nested_sarc_file,
            extract_nested_sarc_file,
            mutate_nested_archive,
            save_file_struct,
            save_as_click,
            add_click,
            add_to_dir_click,
            add_archive_bytes,
            read_file_base64,
            extract_internal_file,
            rename_internal_sarc_file,
            close_all_opened_files,
            close_document,
            remove_internal_sarc_file,
            clear_rfl_miis,
            exit_app,
            open_file_dialog,
            rstb_get_entries,
            rstb_edit_entry,
            rstb_remove_entry,
            search_in_sarc,
            clear_search_in_sarc,
            open_dir_dialog,
            add_files_from_dir_recursively,
            //COMPARER
            compare_files,
            compare_internal_file_with_vanila,
            check_if_update_needed
        ])
        .run(tauri::generate_context!())
    {
        rfd::MessageDialog::new()
            .set_buttons(rfd::MessageButtons::Ok)
            .set_title("Error while running tauri application")
            .set_description(format!("{:?}", err))
            .show();
    }
    Ok(())
}

fn main_initialization() -> io::Result<()> {
    #[allow(unused_variables)]
    let exe_cwd = get_executable_dir();
    if exe_cwd.len() > 0 {
        env::set_current_dir(&exe_cwd)?;
    }
    let version = env!("CARGO_PKG_VERSION").to_string();
    println!("[+] Totkbits version: {}", &version);
    println!("[+] Current directory: {:?}", exe_cwd);
    Ok(())
}
