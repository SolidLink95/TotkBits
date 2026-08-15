use crate::{
    file_format::Image::ImageDocument,
    utils::update_json,
    TotkConfig::TotkConfig,
    Zstd::{TotkZstd, TOTK_ZSTD_COMPRESSION_LEVEL},
};
use image::imageops::FilterType;
use serde_json::json;
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
};

const ICON_DIRECTORY: &str = "UI/Tex/Icon";

/// Starts populating the weapon icon cache without delaying application startup.
pub fn launch_weapon_icon_cache(config: &TotkConfig) {
    let romfs = PathBuf::from(&config.romfs);
    let icon_directory = romfs.join(ICON_DIRECTORY);
    if !TotkConfig::check_for_zsdic(&romfs) || !icon_directory.is_dir() {
        return;
    }

    let config = config.clone();
    let cache_directory = cache_directory().join("webp");
    thread::spawn(move || {
        let _ = populate_weapon_icon_cache(&icon_directory, &cache_directory, config);
    });
}

fn populate_weapon_icon_cache(
    icon_directory: &Path,
    cache_directory: &Path,
    config: TotkConfig,
) -> io::Result<()> {
    fs::create_dir_all(cache_directory)?;
    let zstd = TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)?;

    for entry in fs::read_dir(icon_directory)? {
        let Ok(entry) = entry else { continue };
        let source = entry.path();
        let Some(file_name) = source.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(icon_name) = file_name.strip_suffix(".bntx.zs") else {
            continue;
        };
        let is_weapon = icon_name.starts_with("Weapon_") && icon_name.matches('_').count() == 2;
        let is_armor = icon_name.starts_with("Armor_") && icon_name.matches('_').count() == 2;
        if (!is_weapon && !is_armor) || !source.is_file() {
            continue;
        }

        let destination = cache_directory.join(format!("{icon_name}.webp"));
        if destination.exists() {
            continue;
        }
        let _ = convert_icon(&source, &destination, &zstd);
    }
    Ok(())
}

fn convert_icon(source: &Path, destination: &Path, zstd: &TotkZstd<'_>) -> io::Result<()> {
    let compressed = fs::read(source)?;
    let bntx = zstd.decompress_zs(&compressed)?;
    let rendered = ImageDocument::render_bntx_bytes(&bntx, 0)?;
    let encoded = rendered
        .data_url
        .split_once(',')
        .map(|(_, data)| data)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid rendered image"))?;
    use base64::Engine;
    let png = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let image = image::load_from_memory(&png)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        .resize_exact(128, 128, FilterType::Lanczos3);
    image
        .save_with_format(destination, image::ImageFormat::WebP)
        .map_err(io::Error::other)
}

pub(crate) fn cache_directory() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../.cache")
    }
    #[cfg(not(debug_assertions))]
    {
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_default()
            .join(".cache")
    }
}

#[tauri::command]
pub fn get_startup_data(
    state: tauri::State<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    Ok((*state.inner()).clone())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StartupData {
    pub argv1: String,
    pub argv: Vec<String>,
    pub config: TotkConfig,
}

impl StartupData {
    pub fn new() -> io::Result<Self> {
        let argv: Vec<String> = env::args().skip(1).collect();
        let argv1 = argv.first().cloned().unwrap_or_default();
        let config = TotkConfig::safe_new(true)?;
        Ok(Self {
            argv1,
            argv,
            config,
        })
    }

    pub fn to_json(&self) -> io::Result<serde_json::Value> {
        Ok(update_json(
            json!({"argv1": self.argv1, "argv": self.argv}),
            self.config.to_react_json()?,
        ))
    }
}
