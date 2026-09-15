//! HKCL (BOTW) → BPHCL (TOTK) conversion and its injection into an actor
//! pack (Tools ▸ HKCL to BPHCL, `--cli hkcl_to_bphcl`).
//!
//! The pack side mirrors what a vanilla armor pack registers for its cloth:
//! `Phive/ControllerSetParam` names the cloth file (`ClothList`), the cloth
//! parameters (`Cloth`) and one wind reaction per cloth (`ClothReaction`);
//! `Phive/ClothParam` lists every cloth (`MeshList`, with the bone it hangs
//! from and a material preset) and every collidable. The converted BPHCL
//! replaces the pack's cloth file and both parameter files are regenerated
//! from it; existing entries keep their tuning when the cloth name survives.
//! `HelperBoneList` entries reference bones of the replaced cloth skeleton,
//! so they are dropped and reported.

use crate::{
    file_format::{BinTextFile::BymlFile, Pack::PackFile},
    parser::physics::{
        bphcl::{convert_hkcl_to_bphcl, type_donor, BphclDocument, ConvertOptions, ConvertReport},
        hkcl::HkclDocument,
    },
    utils::LookupData,
    Zstd::TotkZstd,
};
use roead::byml::{Byml, Map};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    fs, io,
    path::Path,
    sync::Arc,
};

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HkclConversionSummary {
    pub output_path: String,
    /// Internal pack path of the written cloth file (empty for a bare BPHCL).
    pub bphcl_entry: String,
    pub cloths: Vec<String>,
    pub collidables: Vec<String>,
    /// `(cloth, BaseBone, Preset)` as registered in the ClothParam.
    pub registrations: Vec<(String, String, String)>,
    /// `(cloth, reaction file)` as registered in the ControllerSetParam.
    pub reactions: Vec<(String, String)>,
    pub removed_helper_bones: Vec<String>,
    pub scale: f32,
    pub warnings: Vec<String>,
}

/// The TOTK TYPE section every converted file is built from.
pub fn type_section() -> io::Result<Vec<u8>> {
    LookupData::read_support_bytes("bphcl_types.bin")
}

/// Bones of the TOTK player skeleton (`Link:` prefix rule).
pub fn link_bones() -> HashSet<String> {
    LookupData::read_support_text("link_bones.txt", "")
        .lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Converts `hkcl_path` into BPHCL bytes, optionally rescaled.
pub fn convert_hkcl_file(hkcl_path: &Path, scale: f32) -> io::Result<(Vec<u8>, ConvertReport)> {
    let source = fs::read(hkcl_path)?;
    let hkcl = HkclDocument::parse(&source).map_err(|error| {
        invalid(format!(
            "{} is not a readable HKCL: {error}",
            hkcl_path.display()
        ))
    })?;
    let types_bytes = type_section()?;
    let types = type_donor(&types_bytes)?;
    let options = ConvertOptions {
        link_bones: link_bones(),
        ..Default::default()
    };
    let (mut bytes, mut report) = convert_hkcl_to_bphcl(&hkcl, &types, &types_bytes, &options)?;
    if (scale - 1.0).abs() > 1e-6 {
        let document = BphclDocument::parse(&bytes)?;
        let (rescaled, rescale) = document.rescale_geometry(scale)?;
        bytes = rescaled;
        report.warnings.push(format!(
            "geometry scaled by {:.3} ({} values)",
            rescale.scale,
            rescale.edits.iter().map(|(_, count)| count).sum::<usize>()
        ));
    }
    Ok((bytes, report))
}

/// Converts `hkcl_path` and writes the bare BPHCL to `output_path`.
pub fn convert_hkcl_to_file(
    hkcl_path: &Path,
    output_path: &Path,
    scale: f32,
) -> io::Result<HkclConversionSummary> {
    let (bytes, report) = convert_hkcl_file(hkcl_path, scale)?;
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(output_path, &bytes)?;
    Ok(HkclConversionSummary {
        output_path: output_path.to_string_lossy().replace('\\', "/"),
        bphcl_entry: String::new(),
        cloths: report.cloths,
        collidables: report.collidables,
        registrations: report.registrations,
        reactions: Vec::new(),
        removed_helper_bones: Vec::new(),
        scale,
        warnings: report.warnings,
    })
}

/// Converts `hkcl_path` and installs the result into the actor pack at
/// `pack_path`, saving the pack to `output_path` (the same path overwrites).
pub fn convert_hkcl_into_pack<'a>(
    hkcl_path: &Path,
    pack_path: &Path,
    output_path: &Path,
    scale: f32,
    zstd: Arc<TotkZstd<'a>>,
) -> io::Result<HkclConversionSummary> {
    let (bytes, report) = convert_hkcl_file(hkcl_path, scale)?;
    let pack = PackFile::new(pack_path, zstd.clone()).map_err(|error| {
        invalid(format!(
            "{} is not a readable actor pack: {error}",
            pack_path.display()
        ))
    })?;
    let stem = pack_stem(pack_path);
    let entry_names: Vec<String> = pack
        .sarc
        .files()
        .filter_map(|file| file.name().map(str::to_owned))
        .collect();
    let mut warnings = report.warnings.clone();

    // ControllerSetParam: the registry that names the other files.
    let controller_path = entry_names
        .iter()
        .find(|name| name.starts_with("Phive/ControllerSetParam/") && name.ends_with(".bgyml"))
        .cloned()
        .ok_or_else(|| {
            invalid(format!(
                "{} has no Phive/ControllerSetParam entry; pick a physics-enabled actor pack",
                pack_path.display()
            ))
        })?;
    let mut controller = pack.byml_file(&controller_path)?;
    let controller_map = as_map_mut(&mut controller.pio, "ControllerSetParam")?;

    let mut cloth_list = list_entries(controller_map, "ClothList");
    let bphcl_entry = match cloth_list
        .first()
        .and_then(|entry| map_string(entry, "Path"))
    {
        Some(work_path) => work_to_internal(&work_path).replace(".phcl", ".bphcl"),
        None => format!("Phive/Cloth/{stem}.bphcl"),
    };
    if cloth_list.is_empty() {
        cloth_list.push(entry(&[
            ("Name", "Default"),
            ("Path", &format!("Work/Phive/Cloth/{stem}.phcl")),
        ]));
    }
    cloth_list.truncate(1);
    controller_map.insert("ClothList".into(), Byml::Array(cloth_list));

    let mut cloth_entries = list_entries(controller_map, "Cloth");
    let cloth_param_path = match cloth_entries
        .first()
        .and_then(|entry| map_string(entry, "FilePath"))
    {
        Some(work_path) => work_to_internal(&work_path),
        None => format!("Phive/ClothParam/{stem}.phive__ClothParam.bgyml"),
    };
    if cloth_entries.is_empty() {
        cloth_entries.push(entry(&[
            ("Name", "Default"),
            ("FilePath", &internal_to_work(&cloth_param_path)),
        ]));
    }
    cloth_entries.truncate(1);
    controller_map.insert("Cloth".into(), Byml::Array(cloth_entries));

    // Wind reactions: keep the pack's own entry per surviving cloth name,
    // give new cloths the closest existing reaction.
    let existing_reactions: Vec<(String, String)> = list_entries(controller_map, "ClothReaction")
        .iter()
        .filter_map(|entry| Some((map_string(entry, "Name")?, map_string(entry, "FilePath")?)))
        .collect();
    let pack_reactions: Vec<String> = entry_names
        .iter()
        .filter(|name| name.starts_with("Phive/ClothReaction/") && name.ends_with(".bgyml"))
        .map(|name| internal_to_work(name))
        .collect();
    let mut reactions = Vec::new();
    let mut reaction_entries = Vec::new();
    for cloth in &report.cloths {
        let lower = cloth.to_ascii_lowercase();
        let hair = lower.contains("hair");
        let file = existing_reactions
            .iter()
            .find(|(name, _)| name == cloth)
            .or_else(|| {
                existing_reactions
                    .iter()
                    .find(|(name, _)| name.to_ascii_lowercase().contains("hair") == hair)
            })
            .or_else(|| existing_reactions.first())
            .map(|(_, file)| file.clone())
            .or_else(|| {
                pack_reactions
                    .iter()
                    .find(|file| file.to_ascii_lowercase().contains("hair") == hair)
                    .or_else(|| pack_reactions.first())
                    .cloned()
            });
        match file {
            Some(file) => {
                reaction_entries.push(entry(&[("FilePath", &file), ("Name", cloth)]));
                reactions.push((cloth.clone(), file));
            }
            None => warnings.push(format!(
                "no ClothReaction file in the pack for {cloth}; it will not react to wind"
            )),
        }
    }
    controller_map.insert("ClothReaction".into(), Byml::Array(reaction_entries));

    let removed_helper_bones: Vec<String> = list_entries(controller_map, "HelperBoneList")
        .iter()
        .filter_map(|entry| map_string(entry, "FilePath"))
        .collect();
    if !removed_helper_bones.is_empty() {
        controller_map.insert("HelperBoneList".into(), Byml::Array(Vec::new()));
        warnings.push(format!(
            "removed {} HelperBoneList entries (they reference bones of the replaced cloth skeleton)",
            removed_helper_bones.len()
        ));
    }
    let controller_bytes = controller.to_binary_preserving_header()?;

    // ClothParam: cloth registrations and collidables of the new file.
    let mut cloth_param = match pack.byml_file(&cloth_param_path) {
        Ok(file) => file,
        Err(_) => {
            warnings.push(format!(
                "{cloth_param_path} was missing and has been created"
            ));
            let mut map = Map::default();
            map.insert(
                "HktPath".into(),
                byml_string(format!("Work/Phive/Cloth/{stem}.hkt")),
            );
            let bytes = Byml::Map(map).to_binary_with_version(roead::Endian::Little, 7);
            BymlFile::from_binary(&bytes, zstd.clone(), &cloth_param_path)?
        }
    };
    let param_map = as_map_mut(&mut cloth_param.pio, "ClothParam")?;
    let previous: BTreeMap<String, Byml> = list_entries(param_map, "MeshList")
        .into_iter()
        .filter_map(|entry| Some((map_string(&entry, "Name")?, entry)))
        .collect();
    let mut registrations = Vec::new();
    let mut mesh_list = Vec::new();
    for (cloth, base_bone, preset) in &report.registrations {
        if let Some(kept) = previous.get(cloth) {
            mesh_list.push(kept.clone());
            registrations.push((
                cloth.clone(),
                map_string(kept, "BaseBone").unwrap_or_else(|| base_bone.clone()),
                map_string(kept, "Preset").unwrap_or_else(|| preset.clone()),
            ));
            continue;
        }
        let mut map = Map::default();
        map.insert("BaseBone".into(), byml_string(base_bone.clone()));
        map.insert("BoneCorrection".into(), Byml::Bool(false));
        map.insert("BoneCorrectionAxisOrder".into(), byml_string("XYZ".into()));
        map.insert("KeepBoneLength".into(), Byml::Bool(false));
        map.insert("Name".into(), byml_string(cloth.clone()));
        map.insert("Preset".into(), byml_string(preset.clone()));
        map.insert("Twist".into(), Byml::Bool(false));
        map.insert("TwistAngleCoef".into(), Byml::Float(0.0));
        map.insert("TwistMaxAngle".into(), Byml::Float(180.0));
        map.insert("TwistSwingAxis".into(), byml_string("Y_Plus".into()));
        mesh_list.push(Byml::Map(map));
        registrations.push((cloth.clone(), base_bone.clone(), preset.clone()));
    }
    param_map.insert("MeshList".into(), Byml::Array(mesh_list));
    let collidable_list = report
        .collidables
        .iter()
        .map(|name| {
            let mut map = Map::default();
            map.insert("ForReplace".into(), Byml::Bool(false));
            map.insert("Name".into(), byml_string(name.clone()));
            Byml::Map(map)
        })
        .collect();
    param_map.insert("CollidableList".into(), Byml::Array(collidable_list));
    let cloth_param_bytes = cloth_param.to_binary_preserving_header()?;

    let rebuilt = pack.rebuild_replacing_entries([
        (bphcl_entry.clone(), bytes),
        (controller_path, controller_bytes),
        (cloth_param_path, cloth_param_bytes),
    ])?;
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(output_path, rebuilt)?;
    Ok(HkclConversionSummary {
        output_path: output_path.to_string_lossy().replace('\\', "/"),
        bphcl_entry,
        cloths: report.cloths,
        collidables: report.collidables,
        registrations,
        reactions,
        removed_helper_bones,
        scale,
        warnings,
    })
}

/// `Armor_900_Head.pack.zs` → `Armor_900_Head`.
fn pack_stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.trim_end_matches(".zs")
        .trim_end_matches(".pack")
        .trim_end_matches(".sarc")
        .to_owned()
}

/// `Work/Phive/Cloth/X.phcl` → `Phive/Cloth/X.phcl`; `.gyml` → `.bgyml`.
fn work_to_internal(path: &str) -> String {
    let stripped = path.strip_prefix("Work/").unwrap_or(path);
    if stripped.ends_with(".gyml") {
        format!("{}.bgyml", stripped.trim_end_matches(".gyml"))
    } else {
        stripped.to_owned()
    }
}

fn internal_to_work(path: &str) -> String {
    let path = path.strip_prefix("Work/").unwrap_or(path);
    if path.ends_with(".bgyml") {
        format!("Work/{}.gyml", path.trim_end_matches(".bgyml"))
    } else {
        format!("Work/{path}")
    }
}

fn entry(fields: &[(&str, &str)]) -> Byml {
    let mut map = Map::default();
    for (key, value) in fields {
        map.insert((*key).into(), byml_string((*value).to_owned()));
    }
    Byml::Map(map)
}

fn map_string(value: &Byml, key: &str) -> Option<String> {
    value
        .as_map()
        .ok()?
        .get(key)?
        .as_string()
        .ok()
        .map(|text| text.to_string())
}

/// The map items of the `key` array.
fn list_entries(map: &Map, key: &str) -> Vec<Byml> {
    map.get(key)
        .and_then(|list| list.as_array().ok())
        .map(|list| {
            list.iter()
                .filter(|item| item.as_map().is_ok())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn as_map_mut<'m>(value: &'m mut Byml, what: &str) -> io::Result<&'m mut Map> {
    value
        .as_mut_map()
        .map_err(|_| invalid(format!("{what} root is not a map")))
}

fn byml_string(value: String) -> Byml {
    Byml::String(value.as_str().into())
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
