use crate::DocumentState::DocumentState;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::{fs, path::Path};
use tauri::Manager;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BfresResolvedTexture {
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    aliases: Vec<String>,
    path: String,
    source: String,
    data_url: String,
    width: u32,
    height: u32,
}

fn tomodachi_textures(
    bfres: &crate::file_format::Model3D::bfres::BfresFile,
    source: &Path,
    set_name: &str,
    variant: u8,
    fallback: Option<&Path>,
    zstd: Option<&crate::Zstd::TotkZstd<'_>>,
) -> Vec<BfresResolvedTexture> {
    crate::file_format::Model3D::bfres::tomodachi::resolve_texture_set(
        source, set_name, variant, fallback, zstd,
    )
    .into_iter()
    .map(|texture| BfresResolvedTexture {
        aliases: tomodachi_texture_aliases(bfres, &texture.path),
        name: texture.name,
        path: texture.path.to_string_lossy().into_owned(),
        source: "tomodachi".into(),
        data_url: texture.data_url,
        width: texture.width,
        height: texture.height,
    })
    .collect()
}

fn tomodachi_texture_aliases(
    bfres: &crate::file_format::Model3D::bfres::BfresFile,
    path: &Path,
) -> Vec<String> {
    let stem = crate::Settings::Pathlib::new(path)
        .stem
        .to_ascii_lowercase();
    let types: &[&str] = if stem.contains("_alb") {
        &["Base color"]
    } else if stem.contains("_nrm") {
        &["Normal"]
    } else if stem.contains("_emm") {
        &["Emission"]
    } else if stem.contains("_mic") {
        &["Roughness", "Specular", "Metalness"]
    } else {
        &["Texture"]
    };
    let mut aliases: Vec<_> = bfres
        .materials
        .iter()
        .flat_map(|material| &material.texture_slots)
        .filter(|slot| types.contains(&slot.texture_type.as_str()))
        .map(|slot| slot.name.clone())
        .collect();
    aliases.sort();
    aliases.dedup();
    aliases
}

#[tauri::command]
pub fn load_tomodachi_texture_set(
    app_handle: tauri::AppHandle,
    documentId: String,
    path: String,
    textureSet: String,
    textureVariant: u8,
) -> Result<Vec<BfresResolvedTexture>, String> {
    require_experimental_visuals()?;
    let documents = app_handle.state::<DocumentState>();
    documents.with(&documentId, |app| {
        let bfres = crate::file_format::Model3D::bfres::BfresFile::from_path(&path)
            .map_err(|error| error.to_string())?;
        Ok(tomodachi_textures(
            &bfres,
            Path::new(&path),
            &textureSet,
            textureVariant,
            Some(Path::new(&app.zstd.totk_config.tomodachi_path)),
            Some(&app.zstd),
        ))
    })
}

#[tauri::command]
pub fn inspect_bfres(
    path: String,
) -> Result<crate::file_format::Model3D::bfres::BfresFile, String> {
    require_experimental_visuals()?;
    crate::file_format::Model3D::bfres::BfresFile::from_path(path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_g1a_animations(
    modelHash: String,
) -> Result<Vec<crate::file_format::Animation::g1a::AvailableG1aAnimation>, String> {
    require_experimental_visuals()?;
    let config =
        crate::TotkConfig::TotkConfig::safe_new(false).map_err(|error| error.to_string())?;
    if config.aoc_path.is_empty() {
        return Ok(Vec::new());
    }
    Ok(crate::file_format::Animation::g1a::available_animations(
        &modelHash,
        Path::new(&config.aoc_path),
    ))
}

#[tauri::command]
pub fn inspect_g1a_animation(
    path: String,
) -> Result<crate::file_format::Animation::g1a::G1aFile, String> {
    require_experimental_visuals()?;
    crate::file_format::Animation::g1a::G1aFile::from_path(path).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn inspect_3d_model(
    app_handle: tauri::AppHandle,
    documentId: String,
    path: String,
) -> Result<serde_json::Value, String> {
    require_experimental_visuals()?;
    let documents = app_handle.state::<DocumentState>();
    let (internal_bfres, internal_bfres_data, visual_data, romfs, aoc_path, tomodachi_path) =
        documents.with(&documentId, |app| {
            (
                app.opened_file.bfres.clone(),
                app.opened_file.bfres_data.clone(),
                app.opened_file.visual_data.clone(),
                app.zstd.totk_config.romfs.clone(),
                app.zstd.totk_config.aoc_path.clone(),
                app.zstd.totk_config.tomodachi_path.clone(),
            )
        });
    if let Some(bfres) = internal_bfres {
        let textures = documents.with(&documentId, |app| {
            resolve_bfres_textures(
                &bfres,
                Path::new(&path),
                internal_bfres_data.as_deref(),
                Path::new(&romfs),
                Path::new(&tomodachi_path),
                Some(&app.zstd),
            )
        });
        let mut value = serde_json::to_value(bfres).map_err(|error| error.to_string())?;
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "resolvedTextures".into(),
                serde_json::to_value(textures).map_err(|error| error.to_string())?,
            );
            insert_tomodachi_texture_sets(object, Path::new(&path), Path::new(&tomodachi_path))?;
        }
        return Ok(value);
    }
    let source_data = match visual_data {
        Some(data) => data,
        None => std::fs::read(&path).map_err(|error| error.to_string())?,
    };
    let data = documents.with(&documentId, |app| {
        app.zstd.try_decompress_safe(&source_data)
    });
    if crate::Settings::Magic::is_fbx(&data) {
        serde_json::to_value(
            crate::parser::fbx::FbxFile::parse(
                &data,
                std::path::Path::new(&path)
                    .file_stem()
                    .and_then(|v| v.to_str())
                    .unwrap_or("FBX"),
            )
            .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    } else if crate::Settings::Magic::is_g1m(&data) {
        let (g1m, texture_resolution) = crate::parser::AOC::g1m::G1mFile::parse_with_textures(
            &data,
            Path::new(&path)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("G1M"),
            Path::new(&path),
            Path::new(&aoc_path),
        )
        .map_err(|error| error.to_string())?;
        let mut value = serde_json::to_value(g1m).map_err(|error| error.to_string())?;
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "resolvedTextures".into(),
                serde_json::to_value(texture_resolution.textures)
                    .map_err(|error| error.to_string())?,
            );
            object.insert(
                "textureStats".into(),
                serde_json::json!({
                    "total": texture_resolution.total,
                    "skipped": texture_resolution.skipped,
                }),
            );
        }
        Ok(value)
    } else {
        let bfres = crate::file_format::Model3D::bfres::BfresFile::from_bytes(&data)
            .map_err(|error| error.to_string())?;
        let textures = documents.with(&documentId, |app| {
            resolve_bfres_textures(
                &bfres,
                Path::new(&path),
                None,
                Path::new(&romfs),
                Path::new(&tomodachi_path),
                Some(&app.zstd),
            )
        });
        let mut value = serde_json::to_value(bfres).map_err(|error| error.to_string())?;
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "resolvedTextures".into(),
                serde_json::to_value(textures).map_err(|error| error.to_string())?,
            );
            insert_tomodachi_texture_sets(object, Path::new(&path), Path::new(&tomodachi_path))?;
        }
        Ok(value)
    }
}

#[tauri::command]
pub fn export_g1m_fbx(
    app_handle: tauri::AppHandle,
    documentId: String,
    source_paths: Vec<String>,
    output: String,
    texture_format: String,
) -> Result<String, String> {
    require_experimental_visuals()?;
    if source_paths.is_empty() {
        return Err("no G1M source paths were supplied".into());
    }
    let documents = app_handle.state::<DocumentState>();
    let aoc_path = documents.with(&documentId, |app| app.zstd.totk_config.aoc_path.clone());
    let mut parsed = Vec::with_capacity(source_paths.len());
    for source in &source_paths {
        let path = Path::new(source);
        let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let model = crate::parser::AOC::g1m::G1mFile::parse_for_export(
            &bytes,
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("G1M"),
        )
        .map_err(|error| format!("{}: {error}", path.display()))?;
        let textures = model
            .resolve_textures_for_export(path, Path::new(&aoc_path))
            .textures;
        let prefix = if source_paths.len() > 1 {
            format!(
                "{}: ",
                path.file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("model")
            )
        } else {
            String::new()
        };
        parsed.push((model, textures, prefix));
    }
    let borrowed: Vec<_> = parsed
        .iter()
        .map(|(model, textures, prefix)| (model, textures.as_slice(), prefix.clone()))
        .collect();
    let armature_name = Path::new(&source_paths[0])
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("G1M");
    crate::parser::fbx::export_g1m(
        &borrowed,
        Path::new(&output),
        crate::parser::fbx::TextureExportFormat::parse(&texture_format)
            .map_err(|error| error.to_string())?,
        armature_name,
    )
    .map_err(|error| error.to_string())?;
    Ok(output)
}

#[tauri::command]
pub fn replace_g1m_meshes(
    app_handle: tauri::AppHandle,
    documentId: String,
    fbx: String,
) -> Result<serde_json::Value, String> {
    require_experimental_visuals()?;
    let documents = app_handle.state::<DocumentState>();
    let (source, aoc_path) = documents.with(&documentId, |app| {
        (
            app.opened_file.path.full_path.clone(),
            app.zstd.totk_config.aoc_path.clone(),
        )
    });
    let source_path = Path::new(&source);
    let fbx_path = Path::new(&fbx);
    let source_data =
        fs::read(source_path).map_err(|error| format!("{}: {error}", source_path.display()))?;
    let fbx_data =
        fs::read(fbx_path).map_err(|error| format!("{}: {error}", fbx_path.display()))?;
    let name = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("G1M");
    let rebuilt =
        crate::parser::AOC::g1m_replace::replace_meshes_from_fbx(&source_data, &fbx_data, name)
            .map_err(|error| error.to_string())?;
    let (model, texture_resolution) = crate::parser::AOC::g1m::G1mFile::parse_with_textures(
        &rebuilt,
        name,
        source_path,
        Path::new(&aoc_path),
    )
    .map_err(|error| error.to_string())?;
    let mut value = serde_json::to_value(model).map_err(|error| error.to_string())?;
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "resolvedTextures".into(),
            serde_json::to_value(texture_resolution.textures).map_err(|error| error.to_string())?,
        );
        object.insert(
            "textureStats".into(),
            serde_json::json!({
                "total": texture_resolution.total,
                "skipped": texture_resolution.skipped,
            }),
        );
    }
    documents.with_mut(&documentId, |app| {
        app.opened_file.custom_g1m = Some(rebuilt)
    });
    Ok(value)
}

#[tauri::command]
pub fn export_g1m_glb(
    app_handle: tauri::AppHandle,
    documentId: String,
    source_paths: Vec<String>,
    output: String,
) -> Result<String, String> {
    require_experimental_visuals()?;
    if source_paths.is_empty() {
        return Err("no G1M source paths were supplied".into());
    }
    let documents = app_handle.state::<DocumentState>();
    let aoc_path = documents.with(&documentId, |app| app.zstd.totk_config.aoc_path.clone());
    let mut parsed = Vec::with_capacity(source_paths.len());
    for source in &source_paths {
        let path = Path::new(source);
        let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let model = crate::parser::AOC::g1m::G1mFile::parse_for_export(
            &bytes,
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("G1M"),
        )
        .map_err(|error| format!("{}: {error}", path.display()))?;
        let textures = model
            .resolve_textures_for_export(path, Path::new(&aoc_path))
            .textures;
        let prefix = if source_paths.len() > 1 {
            format!(
                "{}: ",
                path.file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("model")
            )
        } else {
            String::new()
        };
        parsed.push((model, textures, prefix));
    }
    let borrowed: Vec<_> = parsed
        .iter()
        .map(|(model, textures, prefix)| (model, textures.as_slice(), prefix.clone()))
        .collect();
    crate::parser::glb::export_g1m(&borrowed, Path::new(&output))
        .map_err(|error| error.to_string())?;
    Ok(output)
}

#[tauri::command]
pub fn export_viewport_png(output: String, data_url: String) -> Result<String, String> {
    use base64::Engine;

    let encoded = data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or_else(|| "invalid viewport PNG data URL".to_string())?;
    let png = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| error.to_string())?;
    if !crate::Settings::Magic::is_png(&png) {
        return Err("viewport render did not produce a PNG".into());
    }
    if let Some(parent) = Path::new(&output).parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&output, png).map_err(|error| error.to_string())?;
    Ok(output)
}

#[tauri::command]
pub async fn inspect_batch_g1m(
    path: String,
) -> crate::file_format::Model3D::BatchRender::BatchG1mInspection {
    crate::file_format::Model3D::BatchRender::inspect_batch_g1m(path).await
}

#[tauri::command]
pub fn list_batch_render_files(
    app_handle: tauri::AppHandle,
    documentId: String,
    source_root: String,
    output_root: String,
    existing_png: String,
    model_kind: String,
) -> Result<Vec<crate::file_format::Model3D::BatchRender::BatchRenderFile>, String> {
    crate::file_format::Model3D::BatchRender::list_batch_render_files(
        app_handle,
        documentId,
        source_root,
        output_root,
        existing_png,
        model_kind,
    )
}

pub(crate) fn resolve_bfres_textures(
    bfres: &crate::file_format::Model3D::bfres::BfresFile,
    source: &Path,
    source_data: Option<&[u8]>,
    romfs: &Path,
    tomodachi_root: &Path,
    zstd: Option<&crate::Zstd::TotkZstd<'_>>,
) -> Vec<BfresResolvedTexture> {
    let names: HashSet<&str> = bfres
        .materials
        .iter()
        .flat_map(|material| material.texture_slots.iter().map(|slot| slot.name.as_str()))
        .collect();
    let mut textures = resolve_embedded_bntx_textures(source, source_data, &names);
    let mut resolved_names: HashSet<String> = textures
        .iter()
        .map(|texture| texture.name.to_ascii_lowercase())
        .collect();

    for root in bfres_textogo_roots(source, romfs) {
        let files = index_textogo_files(&root);
        for name in &names {
            let lowercase_name = name.to_ascii_lowercase();
            if resolved_names.contains(&lowercase_name) {
                continue;
            }
            let logical_name = lowercase_name
                .strip_suffix(".txtg")
                .unwrap_or(&lowercase_name);
            let Some(path) = files.get(logical_name) else {
                continue;
            };
            let Ok(rendered) =
                crate::file_format::Image::ImageDocument::render_path_selection_with_zstd(
                    path, 0, 0, 0, zstd,
                )
            else {
                continue;
            };
            textures.push(BfresResolvedTexture {
                name: (*name).to_owned(),
                aliases: Vec::new(),
                path: path.to_string_lossy().into_owned(),
                source: "textogo".into(),
                data_url: rendered.data_url,
                width: rendered.width,
                height: rendered.height,
            });
            resolved_names.insert(lowercase_name);
        }
    }
    if let Some(set) = crate::file_format::Model3D::bfres::tomodachi::available_texture_sets(
        source,
        Some(tomodachi_root),
    )
    .first()
    {
        let selected = tomodachi_textures(
            bfres,
            source,
            &set.name,
            set.variants[0],
            Some(tomodachi_root),
            zstd,
        );
        let selected_names: HashSet<_> = selected
            .iter()
            .map(|texture| texture.name.to_ascii_lowercase())
            .collect();
        textures.retain(|texture| !selected_names.contains(&texture.name.to_ascii_lowercase()));
        textures.extend(selected);
    }
    textures.sort_by(|left, right| left.name.cmp(&right.name));
    textures
}

fn insert_tomodachi_texture_sets(
    object: &mut serde_json::Map<String, Value>,
    source: &Path,
    tomodachi_root: &Path,
) -> Result<(), String> {
    let sets = crate::file_format::Model3D::bfres::tomodachi::available_texture_sets(
        source,
        Some(tomodachi_root),
    );
    if sets.is_empty() {
        return Ok(());
    }
    object.insert(
        "tomodachiTextureSet".into(),
        Value::String(sets[0].name.clone()),
    );
    object.insert(
        "tomodachiTextureVariant".into(),
        Value::from(sets[0].variants[0]),
    );
    object.insert(
        "tomodachiTextureSets".into(),
        serde_json::to_value(sets).map_err(|error| error.to_string())?,
    );
    Ok(())
}

fn bfres_textogo_roots(source: &Path, romfs: &Path) -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Some(mod_romfs) = source.parent().and_then(Path::parent) {
        let adjacent = mod_romfs.join("TexToGo");
        if adjacent.is_dir() {
            roots.push(adjacent);
        }
    }

    let fallback = romfs.join("TexToGo");
    if fallback.is_dir() && !roots.contains(&fallback) {
        roots.push(fallback);
    }
    roots
}

fn index_textogo_files(root: &Path) -> HashMap<String, std::path::PathBuf> {
    std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let file_name = path.file_name()?.to_str()?.to_ascii_lowercase();
            let logical_name = file_name
                .strip_suffix(".txtg.zs")
                .or_else(|| file_name.strip_suffix(".txtg"))?;
            Some((logical_name.to_owned(), path))
        })
        .collect()
}

fn resolve_embedded_bntx_textures(
    source: &Path,
    source_data: Option<&[u8]>,
    referenced_names: &HashSet<&str>,
) -> Vec<BfresResolvedTexture> {
    let disk_data;
    let source_data = match source_data {
        Some(data) => data,
        None => {
            let Ok(data) = std::fs::read(source) else {
                return Vec::new();
            };
            disk_data = data;
            &disk_data
        }
    };
    let Some(offset) = source_data.windows(4).position(|bytes| bytes == b"BNTX") else {
        return Vec::new();
    };
    let data = &source_data[offset..];
    let Ok(bntx) = crate::parser::bntx::BntxFile::parse(data) else {
        return Vec::new();
    };
    let referenced: HashSet<String> = referenced_names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    bntx.textures
        .iter()
        .enumerate()
        .filter(|(_, texture)| referenced.contains(&texture.name.to_ascii_lowercase()))
        .filter_map(|(index, texture)| {
            let rendered =
                crate::file_format::Image::ImageDocument::render_bntx_bytes(data, index).ok()?;
            Some(BfresResolvedTexture {
                name: texture.name.clone(),
                aliases: Vec::new(),
                path: format!("{}#{}", source.display(), texture.name),
                source: "embedded".into(),
                data_url: rendered.data_url,
                width: rendered.width,
                height: rendered.height,
            })
        })
        .collect()
}

#[tauri::command]
pub fn render_image(
    app_handle: tauri::AppHandle,
    documentId: String,
    path: String,
    texture_index: Option<usize>,
    array_index: Option<u32>,
    mip_index: Option<u32>,
) -> Result<crate::file_format::Image::RenderedImage, String> {
    require_experimental_visuals()?;
    let documents = app_handle.state::<DocumentState>();
    documents.with(&documentId, |app| {
        let texture_index = texture_index.unwrap_or(0);
        let array_index = array_index.unwrap_or(0);
        let mip_index = mip_index.unwrap_or(0);
        match app.opened_file.visual_data.as_deref() {
            Some(data) => {
                crate::file_format::Image::ImageDocument::render_bytes_selection_with_zstd(
                    data,
                    &path,
                    texture_index,
                    array_index,
                    mip_index,
                    Some(&app.zstd),
                )
            }
            None => crate::file_format::Image::ImageDocument::render_path_selection_with_zstd(
                path,
                texture_index,
                array_index,
                mip_index,
                Some(&app.zstd),
            ),
        }
        .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn export_image_png(
    app_handle: tauri::AppHandle,
    documentId: String,
    source: String,
    output: String,
    texture_index: Option<usize>,
    array_index: Option<u32>,
    mip_index: Option<u32>,
) -> Result<(), String> {
    require_experimental_visuals()?;
    let documents = app_handle.state::<DocumentState>();
    documents.with(&documentId, |app| {
        let rendered = crate::file_format::Image::ImageDocument::render_path_selection_with_zstd(
            source,
            texture_index.unwrap_or(0),
            array_index.unwrap_or(0),
            mip_index.unwrap_or(0),
            Some(&app.zstd),
        )
        .map_err(|error| error.to_string())?;
        crate::file_format::Image::ImageDocument::export_rendered_png(&rendered, output)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn replace_dds_image(
    target: String,
    png: String,
    ddsType: String,
    mipCount: u32,
    texture_index: Option<usize>,
    array_index: Option<u32>,
    mip_index: Option<u32>,
    replacement_format: Option<String>,
) -> Result<(), String> {
    require_experimental_visuals()?;
    let _ = (ddsType, mipCount);
    let source = std::fs::read(&target).map_err(|error| error.to_string())?;
    if crate::Settings::Magic::is_g1t(&source) {
        return crate::file_format::Image::ImageDocument::replace_g1t_surface(
            target,
            png,
            texture_index.unwrap_or(0),
            array_index.unwrap_or(0),
            mip_index.unwrap_or(0),
        )
        .map_err(|error| error.to_string());
    }
    crate::file_format::Image::ImageDocument::replace_dds_surface(
        target,
        png,
        array_index.unwrap_or(0),
        mip_index.unwrap_or(0),
        replacement_format.as_deref(),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn rename_bntx_texture(
    app_handle: tauri::AppHandle,
    documentId: String,
    path: String,
    texture_index: usize,
    new_name: String,
) -> Result<(), String> {
    require_experimental_visuals()?;
    let documents = app_handle.state::<DocumentState>();
    documents.with(&documentId, |app| {
        crate::file_format::Image::ImageDocument::rename_bntx_texture(
            path,
            texture_index,
            &new_name,
            &app.zstd,
        )
        .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn replace_bntx_image(
    app_handle: tauri::AppHandle,
    documentId: String,
    target: String,
    png: String,
    texture_index: usize,
    array_index: u32,
    mip_index: u32,
    replacement_format: Option<String>,
) -> Result<(), String> {
    require_experimental_visuals()?;
    let documents = app_handle.state::<DocumentState>();
    documents.with(&documentId, |app| {
        crate::file_format::Image::ImageDocument::replace_bntx_surface(
            target,
            png,
            texture_index,
            array_index,
            mip_index,
            replacement_format.as_deref(),
            &app.zstd,
        )
        .map_err(|error| error.to_string())
    })
}

fn require_experimental_visuals() -> Result<(), String> {
    return Ok(());
    if cfg!(debug_assertions) {
        Ok(())
    } else {
        Err("Experimental 3D and image features are disabled in release builds".into())
    }
}
