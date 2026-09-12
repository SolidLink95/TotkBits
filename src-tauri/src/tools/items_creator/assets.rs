//! BFRES and TexToGo asset generation for custom weapons.

use crate::{
    compression::meshcodec::MeshCodec,
    file_format::{
        BinTextFile::BymlFile,
        Image::{BntxReplacementReport, ImageDocument},
        Model3D::bfres::{
            toolbox::{ExternalStrings, ResFile},
            BfresFile,
        },
    },
    Zstd::TotkZstd,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WeaponModelAssetsRequest {
    /// Substring used by the vanilla model and its texture names.
    #[serde(alias = "base")]
    pub base_name: String,
    /// New model/texture substring and BFRES model name.
    #[serde(alias = "name")]
    pub new_name: String,
    /// Optional BFRES or BFRES.MC source. When omitted, the source is resolved from
    /// the vanilla ActorInfo row for `base_name`.
    #[serde(default)]
    pub model_source: Option<PathBuf>,
    /// Destination relative to the mod ROMFS. Defaults to the source path with
    /// every occurrence of `base_name` replaced by `new_name`.
    #[serde(default)]
    pub model_destination: Option<PathBuf>,
    /// Optional FBX whose polygon meshes replace BFRES geometry. FBX materials
    /// and non-mesh objects are ignored; the BFRES first material is retained.
    #[serde(default, alias = "fbx")]
    pub fbx_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WeaponBntxAssetRequest {
    /// BNTX or BNTX.ZS source, relative to clean ROMFS unless absolute.
    pub texture_source: PathBuf,
    /// Optional custom PNG supplied by the user. When omitted, the source image
    /// payload is preserved and only the texture name/container path changes.
    #[serde(default)]
    pub png_source: Option<PathBuf>,
    /// New internal name for the sole BNTX texture.
    #[serde(alias = "name")]
    pub new_name: String,
    /// BNTX destination relative to the mod ROMFS.
    pub texture_destination: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GeneratedWeaponAssets {
    pub model: PathBuf,
    pub textures: Vec<PathBuf>,
    /// Deduplicated final texture names referenced by all BFRES material slots.
    pub texture_names: Vec<String>,
}

impl WeaponModelAssetsRequest {
    pub fn from_json(text: &str) -> io::Result<Self> {
        serde_json::from_str(text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn generate(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<GeneratedWeaponAssets> {
        validate_name(&self.base_name, "base_name")?;
        validate_name(&self.new_name, "new_name")?;
        ensure_output_outside_romfs(clean_romfs, output_romfs)?;
        // Vanilla actors may render a model project named after another actor
        // (Weapon_Spear_001 uses Weapon_Spear_106's model). The model's own
        // project name is what its shape and texture names are built from, so
        // it is the substring to replace when the model comes from ActorInfo.
        let (source, base_name) = match &self.model_source {
            Some(source) => (resolve_source(clean_romfs, source)?, self.base_name.clone()),
            None => {
                let (source, model_project_name) =
                    resolve_vanilla_model_source(clean_romfs, &self.base_name, zstd.clone())?;
                let base_name = if model_project_name.is_empty() {
                    self.base_name.clone()
                } else {
                    model_project_name
                };
                (source, base_name)
            }
        };
        let base_name = base_name.as_str();
        let source_bytes = fs::read(&source)?;
        let raw = if crate::Settings::Magic::is_bfres(&source_bytes) {
            source_bytes
        } else if crate::Settings::Magic::is_mcpk(&source_bytes) {
            zstd.decompress_mcpk(&source_bytes).map_err(|error| {
                invalid_data(format!(
                    "failed to MCPK-decompress base BFRES {}: {error}",
                    source.display()
                ))
            })?
        } else {
            zstd.try_decompress_for_path(&source, &source_bytes)
                .map(|(data, _)| data)
                .map_err(|error| {
                    invalid_data(format!(
                        "failed to decompress base BFRES {}: {error}",
                        source.display()
                    ))
                })?
        };
        let base_bfres = BfresFile::from_bytes(&raw)
            .map_err(|error| invalid_data(format!("failed to parse base BFRES: {error}")))?;
        let major_version = base_bfres
            .header
            .version
            .get(2)
            .copied()
            .ok_or_else(|| invalid_data("BFRES version header is truncated"))?;
        if major_version != 10 {
            return Err(invalid(
                "custom weapon MCPK output requires BFRES version 10",
            ));
        }
        drop(base_bfres);

        let mut customized = raw;
        if let Some(fbx_path) = &self.fbx_path {
            let fbx_path = if fbx_path.is_absolute() {
                fbx_path.clone()
            } else {
                std::env::current_dir()?.join(fbx_path)
            };
            if !fbx_path.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("custom FBX is missing: {}", fbx_path.display()),
                ));
            }
            customized = BfresFile::replace_geometry_from_fbx(&customized, &fs::read(fbx_path)?)
                .map_err(|error| {
                    invalid_data(format!("failed to replace BFRES geometry: {error}"))
                })?;
        }

        // From here on the model is edited through the Switch Toolbox compatible
        // object model so renames rebuild the string pool and dictionaries
        // instead of patching bytes in place.
        let external = external_strings_for(clean_romfs, &customized)?;
        let mut file = ResFile::load(&customized, &external)
            .map_err(|error| invalid_data(format!("failed to load base BFRES: {error}")))?;
        if file.model_count() == 0 {
            return Err(invalid_data("base BFRES contains no model"));
        }
        let texture_names = model_texture_names(&file);
        let texture_sources = index_textures(&clean_romfs.join("TexToGo"))?;
        let texture_output = output_romfs.join("TexToGo");
        fs::create_dir_all(&texture_output)?;
        let mut copied = Vec::with_capacity(texture_names.len());
        for old_name in &texture_names {
            let logical = old_name
                .strip_suffix(".txtg")
                .unwrap_or(old_name)
                .to_ascii_lowercase();
            let Some(source_texture) = texture_sources.get(&logical) else {
                continue;
            };
            let file_name = source_texture
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| invalid("texture filename is not UTF-8"))?;
            let destination_name = replace_file_stem(file_name, base_name, &self.new_name)?;
            if destination_name == file_name {
                // Shared textures (CmnTex_*) keep pointing at the vanilla file.
                continue;
            }
            let destination = texture_output.join(destination_name);
            fs::copy(source_texture, &destination)?;
            copied.push(destination);
        }

        let container_name = format!("{}.{}", self.new_name, self.new_name);
        file.set_internal_name(&container_name);
        file.rename_first_model(&self.new_name)
            .map_err(|error| invalid_data(format!("failed to rename BFRES model: {error}")))?;
        file.rename_texture_slots(base_name, &self.new_name);
        // Actor templates also embed their model name in shape names, so every
        // remaining model-level string is rewritten as well.
        file.rename_model_strings(base_name, &self.new_name);
        let leftovers: Vec<String> = file
            .strings_containing(base_name)
            .into_iter()
            .filter(|(field, _)| field != "original_strings")
            .map(|(field, value)| format!("{field}={value}"))
            .collect();
        if !leftovers.is_empty() {
            return Err(invalid_data(format!(
                "generated BFRES still references the base name: {}",
                leftovers.join(", ")
            )));
        }
        let required_texture_names = model_texture_names(&file);
        let renamed = file
            .save_like_toolbox()
            .map_err(|error| invalid_data(format!("failed to serialize BFRES: {error}")))?;
        let verified = BfresFile::from_bytes(&renamed)
            .map_err(|error| invalid_data(format!("failed to reopen renamed BFRES: {error}")))?;
        validate_bfres_geometry(&verified)?;
        if verified.name.as_deref() != Some(container_name.as_str()) {
            return Err(invalid_data(format!(
                "renamed BFRES container is {:?}, expected {container_name}",
                verified.name
            )));
        }
        let placeholder = placeholder_texture_path()?;
        ensure_material_textures(
            &texture_output,
            &required_texture_names,
            &placeholder,
            &mut copied,
            &texture_sources,
        )?;

        let relative_destination =
            weapon_model_destination(self.model_destination.as_deref(), &source, &self.new_name);
        validate_relative_path(&relative_destination)?;
        let destination = output_romfs.join(relative_destination);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        // Custom weapon models are always emitted as Toolbox-style MCPK.
        let compressed = MeshCodec::compress(&renamed)
            .map_err(|error| invalid_data(format!("failed to MCPK-compress BFRES: {error}")))?;
        let roundtrip = MeshCodec::decompress(&compressed).map_err(|error| {
            invalid_data(format!("generated MCPK does not decompress: {error}"))
        })?;
        let roundtrip_bfres = BfresFile::from_bytes(&roundtrip).map_err(|error| {
            invalid_data(format!("round-tripped BFRES cannot be parsed: {error}"))
        })?;
        validate_bfres_geometry(&roundtrip_bfres)?;
        fs::write(&destination, compressed)?;
        Ok(GeneratedWeaponAssets {
            model: destination,
            textures: copied,
            texture_names: required_texture_names.into_iter().collect(),
        })
    }
}

/// Vanilla TOTK models keep their names in `Shader/ExternalBinaryString.bfres.mc`;
/// models that carry their own string pool (Toolbox or TotkBits output) do not need it.
pub(super) fn external_strings_for(clean_romfs: &Path, raw: &[u8]) -> io::Result<ExternalStrings> {
    let needs_external = raw.get(0xEE).is_some_and(|flags| flags & 0x02 != 0);
    if !needs_external {
        return Ok(ExternalStrings::empty());
    }
    ExternalStrings::from_romfs(clean_romfs).map_err(|error| {
        invalid_data(format!(
            "failed to load TOTK external BFRES strings from {}: {error}",
            clean_romfs.display()
        ))
    })
}

/// Deduplicated texture names referenced by every material of every model.
pub(super) fn model_texture_names(file: &ResFile) -> BTreeSet<String> {
    file.models
        .iter()
        .flat_map(|model| &model.materials)
        .flat_map(|material| &material.texture_refs)
        .cloned()
        .collect()
}

/// The bundled TexToGo stand-in used for material textures the base actor does not ship.
pub(super) fn placeholder_texture_path() -> io::Result<PathBuf> {
    let relative = Path::new("misc/placeholder_tex.txtg");
    let mut candidates = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)];
    if let Ok(exe_dir) = crate::utils::running_exe_dir() {
        candidates.push(exe_dir.join(relative));
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "placeholder TexToGo texture is missing (expected misc/placeholder_tex.txtg)",
            )
        })
}

/// Resolves `Model/<ModelProjectName>.<FmdbName>.bfres.mc` from the vanilla
/// ActorInfo row and returns it together with the model project name.
pub(super) fn resolve_vanilla_model_source(
    clean_romfs: &Path,
    actor_name: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<(PathBuf, String)> {
    let (_, actor_info_path) = super::version::discover_product_file(
        &clean_romfs.join("RSDB"),
        "ActorInfo.Product.",
        ".rstbl.byml.zs",
    )?;
    let actor_info = BymlFile::new(&actor_info_path, zstd).ok_or_else(|| {
        invalid_data(format!(
            "failed to parse vanilla ActorInfo: {}",
            actor_info_path.display()
        ))
    })?;
    let rows = actor_info
        .pio
        .as_array()
        .map_err(|_| invalid_data("vanilla ActorInfo root is not an array"))?;
    let row = rows
        .iter()
        .find_map(|row| {
            let map = row.as_map().ok()?;
            (map.get("__RowId")?.as_string().ok()?.as_str() == actor_name).then_some(map)
        })
        .ok_or_else(|| invalid_data(format!("ActorInfo row is missing: {actor_name}")))?;
    let string_field = |name: &str| {
        row.get(name)
            .ok_or_else(|| invalid_data(format!("ActorInfo {actor_name} has no {name}")))?
            .as_string()
            .map(|value| value.to_string())
            .map_err(|_| invalid_data(format!("ActorInfo {actor_name}.{name} is not a string")))
    };
    let model_project_name = string_field("ModelProjectName")?;
    let fmdb_name = string_field("FmdbName")?;
    let source = resolve_source(
        clean_romfs,
        &Path::new("Model").join(format!("{model_project_name}.{fmdb_name}.bfres.mc")),
    )?;
    Ok((source, model_project_name))
}

pub(super) fn validate_bfres_geometry(file: &BfresFile) -> io::Result<()> {
    if file.render.meshes.is_empty() {
        return Err(invalid_data("generated BFRES contains no meshes"));
    }
    for mesh in &file.render.meshes {
        let vertex_count = mesh.positions.len();
        if vertex_count == 0 || mesh.indices.is_empty() || mesh.indices.len() % 3 != 0 {
            return Err(invalid_data(format!(
                "generated BFRES mesh {} has invalid triangles",
                mesh.name
            )));
        }
        if mesh.normals.len() != vertex_count
            || mesh
                .uv_maps
                .first()
                .is_none_or(|uv| uv.len() != vertex_count)
        {
            return Err(invalid_data(format!(
                "generated BFRES mesh {} has incomplete vertex attributes",
                mesh.name
            )));
        }
        if mesh
            .indices
            .iter()
            .any(|&index| index as usize >= vertex_count)
        {
            return Err(invalid_data(format!(
                "generated BFRES mesh {} has an out-of-range index",
                mesh.name
            )));
        }
        let finite = mesh
            .positions
            .iter()
            .flatten()
            .chain(mesh.normals.iter().flatten())
            .chain(mesh.uv_maps.iter().flatten().flatten())
            .chain(mesh.bone_weights.iter().flatten())
            .all(|value| value.is_finite());
        if !finite {
            return Err(invalid_data(format!(
                "generated BFRES mesh {} contains non-finite geometry",
                mesh.name
            )));
        }
        if usize::from(mesh.material_index) >= file.materials.len()
            || usize::from(mesh.bone_index) >= file.render.bones.len()
            || mesh
                .skin_bones
                .iter()
                .any(|&bone| usize::from(bone) >= file.render.bones.len())
        {
            return Err(invalid_data(format!(
                "generated BFRES mesh {} has an invalid material or bone reference",
                mesh.name
            )));
        }
    }
    Ok(())
}

impl WeaponBntxAssetRequest {
    pub fn from_json(text: &str) -> io::Result<Self> {
        serde_json::from_str(text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn generate(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<BntxReplacementReport> {
        validate_name(&self.new_name, "new_name")?;
        validate_relative_path(&self.texture_destination)?;
        ensure_output_outside_romfs(clean_romfs, output_romfs)?;

        let source = resolve_source(clean_romfs, &self.texture_source)?;
        let destination = output_romfs.join(&self.texture_destination);
        match &self.png_source {
            Some(png) => {
                if !png.is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("custom PNG is missing: {}", png.display()),
                    ));
                }
                ImageDocument::replace_single_bntx_from_png(
                    source,
                    destination,
                    png,
                    &self.new_name,
                    &zstd,
                )
            }
            None => ImageDocument::clone_single_bntx_with_name(
                source,
                destination,
                &self.new_name,
                &zstd,
            ),
        }
    }
}

pub(super) fn index_textures(root: &Path) -> io::Result<BTreeMap<String, PathBuf>> {
    let mut textures = BTreeMap::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let lowercase = name.to_ascii_lowercase();
        let logical = lowercase
            .strip_suffix(".txtg.zs")
            .or_else(|| lowercase.strip_suffix(".txtg"));
        if let Some(logical) = logical {
            textures.insert(logical.to_owned(), path);
        }
    }
    Ok(textures)
}

/// Copies the placeholder for every material texture that exists neither in the
/// mod's TexToGo nor in `clean_textures` (the vanilla TexToGo index).
pub(super) fn ensure_material_textures(
    texture_output: &Path,
    required_texture_names: &BTreeSet<String>,
    placeholder: &Path,
    copied: &mut Vec<PathBuf>,
    clean_textures: &BTreeMap<String, PathBuf>,
) -> io::Result<()> {
    let mut available = index_textures(texture_output)?;
    for texture_name in required_texture_names {
        validate_name(texture_name, "BFRES material texture name")?;
        let stem = texture_name.strip_suffix(".txtg").unwrap_or(texture_name);
        let logical = stem.to_ascii_lowercase();
        if available.contains_key(&logical) || clean_textures.contains_key(&logical) {
            continue;
        }
        let destination = texture_output.join(format!("{stem}.txtg"));
        fs::copy(placeholder, &destination)?;
        available.insert(logical, destination.clone());
        copied.push(destination);
    }
    Ok(())
}

fn replace_file_stem(file_name: &str, base: &str, new_name: &str) -> io::Result<String> {
    let (stem, suffix) = if let Some(stem) = file_name.strip_suffix(".txtg.zs") {
        (stem, ".txtg.zs")
    } else if let Some(stem) = file_name.strip_suffix(".txtg") {
        (stem, ".txtg")
    } else {
        return Err(invalid(format!(
            "unsupported TexToGo filename: {file_name}"
        )));
    };
    Ok(format!("{}{suffix}", stem.replace(base, new_name)))
}

fn weapon_model_destination(requested: Option<&Path>, source: &Path, new_name: &str) -> PathBuf {
    let parent = requested
        .and_then(Path::parent)
        .or_else(|| (!source.is_absolute()).then(|| source.parent()).flatten())
        .unwrap_or_else(|| Path::new("Model"));
    parent.join(format!("{new_name}.{new_name}.bfres.mc"))
}

fn resolve_source(clean_romfs: &Path, source: &Path) -> io::Result<PathBuf> {
    let resolved = if source.is_absolute() {
        source.to_path_buf()
    } else {
        clean_romfs.join(source)
    };
    if !resolved.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("source asset is missing: {}", resolved.display()),
        ));
    }
    Ok(resolved)
}

fn validate_relative_path(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(invalid("asset destination must be a safe relative path"));
    }
    Ok(())
}

fn validate_name(value: &str, field: &str) -> io::Result<()> {
    if value.is_empty() || value.contains(['/', '\\', '\0']) {
        return Err(invalid(format!("invalid {field}")));
    }
    Ok(())
}

pub(super) fn ensure_output_outside_romfs(clean_romfs: &Path, output: &Path) -> io::Result<()> {
    let clean = clean_romfs.canonicalize()?;
    let absolute = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()?.join(output)
    };
    let output = resolve_with_existing_ancestor(&absolute)?;
    if output == clean || output.starts_with(&clean) {
        return Err(invalid("output must be outside clean ROMFS"));
    }
    Ok(())
}

/// Canonicalize the closest existing ancestor, then restore the not-yet-created suffix.
/// This catches Windows junctions/symlinks without requiring the output to exist already.
pub(super) fn resolve_with_existing_ancestor(path: &Path) -> io::Result<PathBuf> {
    let mut ancestor = path;
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        let name = ancestor
            .file_name()
            .ok_or_else(|| invalid("output path has no existing ancestor"))?;
        suffix.push(name.to_owned());
        ancestor = ancestor
            .parent()
            .ok_or_else(|| invalid("output path has no existing ancestor"))?;
    }
    let mut resolved = ancestor.canonicalize()?;
    for component in suffix.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};

    #[test]
    fn fills_each_missing_material_texture_once() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/assets_placeholder_safeguard_test");
        if root.is_dir() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Existing_Alb.txtg"), b"existing").unwrap();
        let required = BTreeSet::from([
            "Existing_Alb".to_owned(),
            "Missing_Nrm".to_owned(),
            "Missing_Spm".to_owned(),
        ]);
        let placeholder = Path::new(env!("CARGO_MANIFEST_DIR")).join("misc/placeholder_tex.txtg");
        let mut copied = Vec::new();
        let clean = BTreeMap::from([("missing_spm".to_owned(), PathBuf::from("vanilla"))]);
        ensure_material_textures(&root, &required, &placeholder, &mut copied, &clean).unwrap();

        // Missing_Spm exists in vanilla, so only Missing_Nrm needs the placeholder.
        assert_eq!(copied.len(), 1);
        assert_eq!(
            fs::read(root.join("Existing_Alb.txtg")).unwrap(),
            b"existing"
        );
        assert_eq!(
            fs::read(root.join("Missing_Nrm.txtg")).unwrap(),
            fs::read(&placeholder).unwrap()
        );
        assert!(!root.join("Missing_Spm.txtg").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replaces_only_the_texture_file_stem() {
        assert_eq!(
            replace_file_stem(
                "Weapon_Sword_019_Alb.txtg.zs",
                "Weapon_Sword_019",
                "Weapon_Sword_900"
            )
            .unwrap(),
            "Weapon_Sword_900_Alb.txtg.zs"
        );
    }

    #[test]
    fn weapon_bfres_destination_is_always_actor_dot_actor() {
        assert_eq!(
            weapon_model_destination(
                Some(Path::new("Model/ignored_name.bfres.mc")),
                Path::new("Model/Base.Base.bfres.mc"),
                "Weapon_Sword_900",
            ),
            Path::new("Model/Weapon_Sword_900.Weapon_Sword_900.bfres.mc")
        );
        assert_eq!(
            weapon_model_destination(
                None,
                Path::new("Model/Base.Base.bfres.mc"),
                "Weapon_Sword_900",
            ),
            Path::new("Model/Weapon_Sword_900.Weapon_Sword_900.bfres.mc")
        );
    }

    #[test]
    fn parses_bntx_asset_request_from_json() {
        let request = WeaponBntxAssetRequest::from_json(
            r#"{
                "texture_source": "UI/Tex/Icon/Weapon_Sword_019.bntx.zs",
                "png_source": "custom.png",
                "name": "Weapon_Sword_900",
                "texture_destination": "UI/Tex/Icon/Weapon_Sword_900.bntx.zs"
            }"#,
        )
        .unwrap();
        assert_eq!(request.new_name, "Weapon_Sword_900");
        assert_eq!(request.png_source, Some(PathBuf::from("custom.png")));
        assert!(request.texture_destination.is_relative());

        let without_png = WeaponBntxAssetRequest::from_json(
            r#"{
                "texture_source": "UI/Tex/Icon/Weapon_Sword_019.bntx.zs",
                "name": "Weapon_Sword_900",
                "texture_destination": "UI/Tex/Icon/Weapon_Sword_900.bntx.zs"
            }"#,
        )
        .unwrap();
        assert_eq!(without_png.png_source, None);
    }

    #[test]
    #[ignore = "requires a configured clean ROMFS"]
    fn copies_textures_rewrites_slots_and_emits_mcpk() {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let model = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/bfres/Weapon_Sword_019.Weapon_Sword_019.bfres");
        if !model.is_file() {
            return;
        }
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/assets_generated_romfs");
        let request = WeaponModelAssetsRequest {
            base_name: "Weapon_Sword_019".into(),
            new_name: "Weapon_Sword_900".into(),
            model_source: Some(model),
            model_destination: Some("Model/Weapon_Sword_900.Weapon_Sword_900.bfres.mc".into()),
            fbx_path: None,
        };
        let generated = request
            .generate(clean_romfs, &output, zstd.clone())
            .unwrap();
        assert!(!generated.textures.is_empty());
        let compressed = fs::read(&generated.model).unwrap();
        assert!(crate::Settings::Magic::is_mcpk(&compressed));
        let raw = zstd.decompress_mcpk(&compressed).unwrap();
        let bfres = BfresFile::from_bytes(&raw).unwrap();
        assert_eq!(
            bfres.name.as_deref(),
            Some("Weapon_Sword_900.Weapon_Sword_900")
        );
        assert!(bfres.materials.iter().all(|material| material
            .texture_slots
            .iter()
            .all(|slot| !slot.name.contains("Weapon_Sword_019"))));
        fs::remove_dir_all(output).unwrap();
    }

    #[test]
    #[ignore = "requires the configured clean ROMFS and supplied weapon FBX"]
    fn saves_supplied_fbx_as_custom_mcpk_bfres() {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let model = root.join("bfres/Weapon_Sword_022.Weapon_Sword_022.bfres");
        let fbx = root.join("Weapon_Sword_022.fbx");
        if !clean_romfs.is_dir() || !model.is_file() || !fbx.is_file() {
            return;
        }
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        let output = root.join("test_sic/romfs");
        let request = WeaponModelAssetsRequest {
            base_name: "Weapon_Sword_022".into(),
            new_name: "Weapon_Lsword_005".into(),
            model_source: Some(model),
            model_destination: Some("Model/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc".into()),
            fbx_path: Some(fbx.clone()),
        };
        let imported =
            crate::parser::fbx::import::import_for_bfres(&fs::read(fbx).unwrap()).unwrap();
        let generated = request
            .generate(clean_romfs, &output, zstd.clone())
            .unwrap();
        let compressed = fs::read(&generated.model).unwrap();
        assert!(crate::Settings::Magic::is_mcpk(&compressed));
        let raw = zstd.decompress_mcpk(&compressed).unwrap();
        let source_flags = fs::read(request.model_source.as_ref().unwrap()).unwrap();
        assert_eq!(&raw[0xEE..0xF0], &source_flags[0xEE..0xF0]);
        let saved = BfresFile::from_bytes(&raw).unwrap();
        assert_eq!(
            saved.name.as_deref(),
            Some("Weapon_Lsword_005.Weapon_Lsword_005")
        );
        assert_eq!(
            saved
                .sections_with_signature(b"FMDL")
                .next()
                .and_then(|model| model.name.as_deref()),
            Some("Weapon_Lsword_005")
        );
        assert_eq!(
            saved
                .render
                .meshes
                .iter()
                .map(|mesh| mesh.positions.len())
                .sum::<usize>(),
            imported
                .meshes
                .iter()
                .map(|mesh| mesh.positions.len())
                .sum::<usize>()
        );
        let blade = saved
            .render
            .meshes
            .iter()
            .find(|mesh| mesh.name.to_ascii_lowercase().contains("blade_hide"))
            .expect("saved blade-hide mesh");
        assert!(blade
            .positions
            .iter()
            .flatten()
            .all(|value| value.is_finite()));
        assert_eq!(
            saved
                .render
                .meshes
                .iter()
                .map(|mesh| mesh.indices.len())
                .sum::<usize>(),
            imported
                .meshes
                .iter()
                .map(|mesh| mesh.indices.len())
                .sum::<usize>()
        );
        let blade_material = saved
            .materials
            .iter()
            .position(|material| material.name.to_ascii_lowercase().contains("blade_hide"))
            .map(|index| index as u16)
            .unwrap_or(0);
        let default_material = u16::from(saved.materials.len() > 1);
        let mut expected_materials: Vec<_> = imported
            .meshes
            .iter()
            .map(|mesh| {
                if mesh.name.to_ascii_lowercase().contains("blade_hide") {
                    blade_material
                } else {
                    default_material
                }
            })
            .collect();
        let mut actual_materials: Vec<_> = saved
            .render
            .meshes
            .iter()
            .map(|mesh| mesh.material_index)
            .collect();
        expected_materials.sort_unstable();
        actual_materials.sort_unstable();
        assert_eq!(actual_materials, expected_materials);
    }
}
