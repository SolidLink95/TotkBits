//! "Add ELink" view: ELink user listing, asset-call inspection and mod generation.

use super::items_creator::{clean_romfs, zstd};
use crate::tools::elink_creator::{self, AssetEntry, ElinkRequest, ParamSpec};
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElinkUser {
    pub name: String,
    /// The user has `Effect/<name>.Nin_NX_NVN.esetb.byml.zs` of its own.
    pub has_esetb: bool,
    pub assets: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElinkUsers {
    pub romfs: String,
    pub source: String,
    pub users: Vec<ElinkUser>,
    pub params: Vec<ParamSpec>,
}

#[tauri::command]
pub async fn elink_users() -> Result<ElinkUsers, String> {
    tauri::async_runtime::spawn_blocking(|| {
        crate::Settings::catch_panic_with(
            move || {
                let (config, romfs) = clean_romfs()?;
                let zstd = zstd(config)?;
                let relative = elink_creator::elink_relative_path(&romfs)
                    .map_err(|error| error.to_string())?;
                let text =
                    elink_creator::vanilla_text(&romfs, zstd).map_err(|error| error.to_string())?;
                let counts =
                    elink_creator::asset_counts(&text).map_err(|error| error.to_string())?;
                let users = elink_creator::list_users(&text)
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .map(|name| ElinkUser {
                        has_esetb: elink_creator::esetb_path(&romfs, &name).is_file(),
                        assets: counts.get(&name).copied().unwrap_or(0),
                        name,
                    })
                    .collect();
                Ok(ElinkUsers {
                    romfs: romfs.to_string_lossy().into_owned(),
                    source: relative,
                    users,
                    params: elink_creator::EDITABLE_PARAMS.to_vec(),
                })
            },
            Err,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn elink_user_assets(user: String) -> Result<Vec<AssetEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::Settings::catch_panic_with(
            move || {
                let (config, romfs) = clean_romfs()?;
                let text = elink_creator::vanilla_text(&romfs, zstd(config)?)
                    .map_err(|error| error.to_string())?;
                elink_creator::user_assets(&text, user.trim()).map_err(|error| error.to_string())
            },
            Err,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElinkResult {
    pub output_romfs: String,
    pub elink: String,
    pub esetb: Option<String>,
    pub rstb_entries: Option<usize>,
    pub edited_entries: usize,
    pub notes: Vec<String>,
    pub seconds: f64,
}

/// Generates the custom effect into `<outputDir>/<modName>/romfs` and keeps
/// the request as `<outputDir>/<modName>/add_elink_<newName>.json`.
#[tauri::command]
pub async fn elink_generate(
    request: ElinkRequest,
    outputDir: String,
    modName: String,
) -> Result<ElinkResult, String> {
    let mod_name = modName.trim().to_owned();
    if mod_name.is_empty()
        || mod_name
            .chars()
            .any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        return Err("mod name must be a valid folder name".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::Settings::catch_panic_with(
            move || {
                let (config, romfs) = clean_romfs()?;
                let zstd = zstd(config)?;
                let mod_dir = Path::new(&outputDir).join(&mod_name);
                fs::create_dir_all(&mod_dir)
                    .map_err(|error| format!("{}: {error}", mod_dir.display()))?;
                let output_romfs = mod_dir.join("romfs");
                let started = std::time::Instant::now();
                let report = elink_creator::generate(&request, &romfs, &output_romfs, zstd)
                    .map_err(|error| error.to_string())?;
                let spec_path = mod_dir.join(format!("add_elink_{}.json", request.new_name.trim()));
                if let Ok(json) = serde_json::to_string_pretty(&request) {
                    let _ = fs::write(&spec_path, json);
                }
                Ok(ElinkResult {
                    output_romfs: report.output_romfs.to_string_lossy().into_owned(),
                    elink: report.elink.to_string_lossy().into_owned(),
                    esetb: report.esetb.map(|path| path.to_string_lossy().into_owned()),
                    rstb_entries: report.rstb_entries,
                    edited_entries: report.edited_entries,
                    notes: report.notes,
                    seconds: started.elapsed().as_secs_f64(),
                })
            },
            Err,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}
