//! Item Creator view: catalog lookups and headless mod generation.

use crate::{
    tools::items_creator::{self, catalog},
    TotkConfig::TotkConfig,
    Zstd::{TotkZstd, TOTK_ZSTD_COMPRESSION_LEVEL},
};
use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(super) fn clean_romfs() -> Result<(Arc<TotkConfig>, PathBuf), String> {
    let config = TotkConfig::safe_new(false).map_err(|error| error.to_string())?;
    let romfs = PathBuf::from(&config.romfs);
    if config.romfs.is_empty() || !romfs.is_dir() {
        return Err("a TOTK RomFS is required; set it in Settings first".into());
    }
    Ok((Arc::new(config), romfs))
}

pub(super) fn zstd(config: Arc<TotkConfig>) -> Result<Arc<TotkZstd<'static>>, String> {
    TotkZstd::new(config, TOTK_ZSTD_COMPRESSION_LEVEL)
        .map(Arc::new)
        .map_err(|error| format!("failed to load RomFS zstd dictionaries: {error}"))
}

#[tauri::command]
pub fn item_creator_catalog() -> Result<catalog::Catalog, String> {
    crate::Settings::catch_panic_with(
        move || {
            let (config, romfs) = clean_romfs()?;
            catalog::catalog(&romfs, zstd(config)?).map_err(|error| error.to_string())
        },
        Err,
    )
}

#[tauri::command]
pub fn item_creator_template_info(actor: String) -> Result<catalog::TemplateInfo, String> {
    crate::Settings::catch_panic_with(
        move || {
            let (config, romfs) = clean_romfs()?;
            catalog::template_info(&romfs, &actor, zstd(config)?).map_err(|error| error.to_string())
        },
        Err,
    )
}

#[tauri::command]
pub fn item_creator_icons(names: Vec<String>) -> HashMap<String, String> {
    crate::Settings::catch_panic_with(move || catalog::icons(&names), |_| HashMap::new())
}

/// Icons of Great Fairy ingredients keyed by icon actor (data URLs); made
/// from the RomFS into `.cache/ingredients` the first time they are asked for.
#[tauri::command]
pub fn item_creator_ingredient_icons(
    names: Vec<String>,
) -> Result<HashMap<String, String>, String> {
    crate::Settings::catch_panic_with(
        move || {
            let (config, romfs) = clean_romfs()?;
            Ok(catalog::ingredient_icons(&romfs, zstd(config)?, &names))
        },
        Err,
    )
}

#[tauri::command]
pub fn item_creator_load_specs(path: String) -> Result<serde_json::Value, String> {
    crate::Settings::catch_panic_with(
        move || {
            let text = fs::read_to_string(&path).map_err(|error| format!("{path}: {error}"))?;
            serde_json::from_str(&text).map_err(|error| format!("{path}: {error}"))
        },
        Err,
    )
}

#[tauri::command]
pub fn item_creator_save_specs(path: String, specs: serde_json::Value) -> Result<(), String> {
    crate::Settings::catch_panic_with(
        move || {
            let text = serde_json::to_string_pretty(&specs).map_err(|error| error.to_string())?;
            fs::write(&path, text).map_err(|error| format!("{path}: {error}"))
        },
        Err,
    )
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemCreatorResult {
    pub output_romfs: String,
    pub spec_path: String,
    pub report_path: String,
    pub weapons: Vec<String>,
    pub armors: Vec<String>,
    /// Custom ELink users written with the mod.
    pub elinks: Vec<String>,
    /// `None` when the RSTB pass was skipped.
    pub rstb_entries: Option<usize>,
    pub seconds: f64,
    pub warnings: Vec<String>,
}

/// Writes `<outputDir>/<modName>/items_creator_input.json`, generates the mod
/// into `<outputDir>/<modName>/romfs` and drops the report next to it. Runs
/// on a worker thread; the UI polls its returned promise. `generateRstb`
/// (default true) false skips the ResourceSizeTable pass.
#[tauri::command]
pub async fn item_creator_generate(
    specs: serde_json::Value,
    outputDir: String,
    modName: String,
    zstdLevel: Option<i32>,
    generateRstb: Option<bool>,
) -> Result<ItemCreatorResult, String> {
    let mod_name = modName.trim().to_owned();
    if mod_name.is_empty()
        || mod_name
            .chars()
            .any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        return Err("mod name must be a valid folder name".into());
    }
    let items = specs
        .as_array()
        .map(Vec::len)
        .ok_or_else(|| "the item list must be a JSON array".to_owned())?;
    if items == 0 {
        return Err("add at least one item to the mod first".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let (config, romfs) = clean_romfs()?;
        let zstd = zstd(config)?;
        let mod_dir = Path::new(&outputDir).join(&mod_name);
        fs::create_dir_all(&mod_dir).map_err(|error| format!("{}: {error}", mod_dir.display()))?;
        let spec_path = mod_dir.join("items_creator_input.json");
        let text = serde_json::to_string_pretty(&specs).map_err(|error| error.to_string())?;
        fs::write(&spec_path, text).map_err(|error| format!("{}: {error}", spec_path.display()))?;
        let loaded =
            items_creator::load_item_specs(&spec_path).map_err(|error| error.to_string())?;
        for spec in &loaded {
            spec.validate(&mod_dir)
                .map_err(|error| format!("{}: {error}", spec.actor_name()))?;
        }
        let output_romfs = mod_dir.join("romfs");
        let started = std::time::Instant::now();
        let report = items_creator::generate_item_mod_with_options(
            &loaded,
            &romfs,
            &output_romfs,
            &mod_dir,
            zstd,
            zstdLevel,
            generateRstb.unwrap_or(true),
        )
        .map_err(|error| error.to_string())?;
        let report_path = mod_dir.join("items_creator_report.json");
        let json = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
        fs::write(&report_path, json)
            .map_err(|error| format!("{}: {error}", report_path.display()))?;
        let mut warnings = Vec::new();
        for weapon in &report.weapons {
            for texture in &weapon.ui_textures {
                if let Some(warning) = &texture.warning {
                    warnings.push(format!("{}: {warning}", weapon.actor_name));
                }
            }
        }
        for armor in &report.armors {
            for texture in &armor.ui_textures {
                if let Some(warning) = &texture.warning {
                    warnings.push(format!("{}: {warning}", armor.actor_name));
                }
            }
        }
        for elink in &report.elinks {
            for note in &elink.notes {
                warnings.push(format!("{}: {note}", elink.new_name));
            }
        }
        Ok(ItemCreatorResult {
            output_romfs: output_romfs.to_string_lossy().into_owned(),
            spec_path: spec_path.to_string_lossy().into_owned(),
            report_path: report_path.to_string_lossy().into_owned(),
            weapons: report
                .weapons
                .iter()
                .map(|w| w.actor_name.clone())
                .collect(),
            armors: report.armors.iter().map(|a| a.actor_name.clone()).collect(),
            elinks: report.elinks.iter().map(|e| e.new_name.clone()).collect(),
            rstb_entries: report.rstb.as_ref().map(|rstb| rstb.entries.len()),
            seconds: started.elapsed().as_secs_f64(),
            warnings,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}
