//! Physics donors of an items-creator armor piece.
//!
//! A single donor is copied verbatim (its `Phive/*` and `Component/Physics/*`
//! entries replace the template's, `PhysicsRef` re-pointed at the donor's
//! PhysicsParam). Two or more donors are merged into one bundle named after
//! the generated actor: every cloth (with its paired skeleton) and every
//! collidable of the donors' BPHCL files is merged into
//! `Phive/Cloth/<actor>.bphcl`, their ClothParams into
//! `Phive/ClothParam/<actor>.phive__ClothParam.bgyml`, their
//! ControllerSetParams into `Phive/ControllerSetParam/<actor>…` (single
//! `ClothList` / `Cloth` entry, concatenated `ClothReaction` and
//! `HelperBoneList`), and `Component/Physics/<actor>…` binds it all.

use super::actor_pack::{self, InjectedPackEntry};
use crate::{
    file_format::BinTextFile::BymlFile, parser::physics::bphcl::BphclDocument, Zstd::TotkZstd,
};
use roead::byml::{Byml, Map};
use std::{collections::BTreeSet, io, path::Path, sync::Arc};

/// A donor's physics bundle with the documents the merge needs parsed.
struct DonorPhysics<'a> {
    actor: String,
    entries: Vec<InjectedPackEntry>,
    /// `Component/Physics/<x>.engine__component__PhysicsParam.bgyml`.
    physics_path: String,
    physics_param: BymlFile<'a>,
    /// `Phive/ControllerSetParam/<x>.phive__ControllerSetParam.bgyml`.
    controller_path: String,
    controller: BymlFile<'a>,
    /// `Phive/Cloth/<x>.bphcl`, in `ClothList` order.
    cloth_paths: Vec<String>,
    /// `Phive/ClothParam/<x>.phive__ClothParam.bgyml`, in `Cloth` order.
    cloth_param_paths: Vec<String>,
}

/// Resolves the donor list of an armor spec. Blank, malformed, duplicate or
/// missing donors are skipped (the template's own physics files stay when
/// none is usable); one usable donor is copied as-is; two or more are merged
/// under `actor_name`. A failed merge is an error rather than a silent
/// fallback because the caller asked for the combination explicitly.
pub(super) fn merged_physics_entries<'a>(
    clean_romfs: &Path,
    donors: &[String],
    actor_name: &str,
    zstd: Arc<TotkZstd<'a>>,
) -> io::Result<Option<(String, Vec<InjectedPackEntry>)>> {
    let mut bundles = Vec::new();
    let mut seen = BTreeSet::new();
    for donor in donors {
        let Some(actor) = actor_pack::resolve_physics_donor(clean_romfs, Some(donor)) else {
            if !donor.trim().is_empty() {
                eprintln!(
                    "[items creator] physics donor {} has no vanilla actor pack; skipping it",
                    donor.trim()
                );
            }
            continue;
        };
        if !seen.insert(actor.clone()) {
            continue;
        }
        match actor_pack::prepare_physics_entries(clean_romfs, &actor, zstd.clone()) {
            Ok(bundle) => bundles.push((actor, bundle)),
            Err(error) => eprintln!(
                "[items creator] physics donor {actor} is unusable ({error}); skipping it"
            ),
        }
    }
    match bundles.len() {
        0 => Ok(None),
        1 => Ok(bundles.pop().map(|(_, bundle)| bundle)),
        _ => {
            let donors = bundles
                .into_iter()
                .map(|(actor, (physics_ref, entries))| {
                    DonorPhysics::parse(actor, &physics_ref, entries, zstd.clone())
                })
                .collect::<io::Result<Vec<_>>>()?;
            merge_donors(&donors, actor_name, zstd).map(Some)
        }
    }
}

impl<'a> DonorPhysics<'a> {
    fn parse(
        actor: String,
        physics_ref: &str,
        entries: Vec<InjectedPackEntry>,
        zstd: Arc<TotkZstd<'a>>,
    ) -> io::Result<Self> {
        let physics_path = actor_pack::reference_to_internal(physics_ref);
        let physics_param = byml_entry(&entries, &physics_path, &actor, zstd.clone())?;
        let controller_work_path =
            map_string(&physics_param.pio, "ControllerSetPath").ok_or_else(|| {
                invalid(format!(
                    "physics donor {actor}: {physics_path} has no ControllerSetPath"
                ))
            })?;
        let controller_path = actor_pack::work_path_to_internal(&controller_work_path);
        let controller = byml_entry(&entries, &controller_path, &actor, zstd)?;
        let cloth_paths = list_paths(&controller.pio, "ClothList", "Path")
            .into_iter()
            .map(|path| actor_pack::work_path_to_internal(&path).replace(".phcl", ".bphcl"))
            .collect::<Vec<_>>();
        if cloth_paths.is_empty() {
            return Err(invalid(format!(
                "physics donor {actor}: {controller_path} lists no cloth file"
            )));
        }
        let cloth_param_paths = list_paths(&controller.pio, "Cloth", "FilePath")
            .into_iter()
            .map(|path| actor_pack::work_path_to_internal(&path))
            .collect::<Vec<_>>();
        if cloth_param_paths.is_empty() {
            return Err(invalid(format!(
                "physics donor {actor}: {controller_path} lists no cloth parameter file"
            )));
        }
        for path in cloth_paths.iter().chain(&cloth_param_paths) {
            if entry_data(&entries, path).is_none() {
                return Err(invalid(format!(
                    "physics donor {actor}: {controller_path} references a missing entry {path}"
                )));
            }
        }
        Ok(Self {
            actor,
            entries,
            physics_path,
            physics_param,
            controller_path,
            controller,
            cloth_paths,
            cloth_param_paths,
        })
    }

    /// Entries that the merged files replace.
    fn replaced_paths(&self) -> BTreeSet<&str> {
        std::iter::once(self.physics_path.as_str())
            .chain(std::iter::once(self.controller_path.as_str()))
            .chain(self.cloth_paths.iter().map(String::as_str))
            .chain(self.cloth_param_paths.iter().map(String::as_str))
            .collect()
    }
}

fn merge_donors<'a>(
    donors: &[DonorPhysics<'a>],
    actor_name: &str,
    zstd: Arc<TotkZstd<'a>>,
) -> io::Result<(String, Vec<InjectedPackEntry>)> {
    let base = &donors[0];
    let bphcl_path = format!("Phive/Cloth/{actor_name}.bphcl");
    let cloth_param_path = format!("Phive/ClothParam/{actor_name}.phive__ClothParam.bgyml");
    let controller_path =
        format!("Phive/ControllerSetParam/{actor_name}.phive__ControllerSetParam.bgyml");
    let physics_path =
        format!("Component/Physics/{actor_name}.engine__component__PhysicsParam.bgyml");

    // BPHCL: the first cloth file is the target, everything else is merged in
    // cloth by cloth (each with its paired skeleton and reachable graph) and
    // then collidable by collidable, so standalone colliders come along too.
    let mut merged: Option<BphclDocument> = None;
    for donor in donors {
        for path in &donor.cloth_paths {
            let data = entry_data(&donor.entries, path)
                .ok_or_else(|| invalid(format!("{}: missing {path}", donor.actor)))?;
            let source = BphclDocument::parse(data).map_err(|error| {
                invalid(format!(
                    "{}: {path} is not a valid BPHCL ({error})",
                    donor.actor
                ))
            })?;
            let Some(target) = merged.take() else {
                merged = Some(source);
                continue;
            };
            merged = Some(merge_bphcl(target, &source).map_err(|error| {
                invalid(format!(
                    "merging {path} of {} into the physics of {}: {error}",
                    donor.actor, base.actor
                ))
            })?);
        }
    }
    let merged = merged.ok_or_else(|| invalid("no physics donor supplied a cloth file"))?;

    // ClothParam: union of every donor's collidable and simulation-mesh lists
    // on top of the first donor's parameters.
    let mut cloth_param = byml_entry(
        &base.entries,
        &base.cloth_param_paths[0],
        &base.actor,
        zstd.clone(),
    )?;
    {
        let map = as_map_mut(&mut cloth_param.pio, "ClothParam")?;
        map.insert(
            "HktPath".into(),
            byml_string(format!("Work/Phive/Cloth/{actor_name}.hkt")),
        );
        for key in ["CollidableList", "MeshList"] {
            let mut union = Vec::new();
            for donor in donors {
                for path in &donor.cloth_param_paths {
                    let document = byml_entry(&donor.entries, path, &donor.actor, zstd.clone())?;
                    union.extend(list_entries(&document.pio, key));
                }
            }
            map.insert(key.into(), Byml::Array(dedupe_by(union, "Name")));
        }
    }

    // ControllerSetParam: one cloth file, one cloth parameter set, every
    // reaction and helper-bone file of every donor.
    let mut controller = byml_entry(
        &base.entries,
        &base.controller_path,
        &base.actor,
        zstd.clone(),
    )?;
    {
        let mut cloth_list = list_entries(&base.controller.pio, "ClothList");
        cloth_list.truncate(1);
        set_entry_string(
            &mut cloth_list,
            "Path",
            format!("Work/Phive/Cloth/{actor_name}.phcl"),
        );
        let mut cloth = list_entries(&base.controller.pio, "Cloth");
        cloth.truncate(1);
        set_entry_string(
            &mut cloth,
            "FilePath",
            format!("Work/Phive/ClothParam/{actor_name}.phive__ClothParam.gyml"),
        );
        let reactions = donors
            .iter()
            .flat_map(|donor| list_entries(&donor.controller.pio, "ClothReaction"))
            .collect();
        let helpers = donors
            .iter()
            .flat_map(|donor| list_entries(&donor.controller.pio, "HelperBoneList"))
            .collect();
        let map = as_map_mut(&mut controller.pio, "ControllerSetParam")?;
        map.insert("ClothList".into(), Byml::Array(cloth_list));
        map.insert("Cloth".into(), Byml::Array(cloth));
        map.insert(
            "ClothReaction".into(),
            Byml::Array(dedupe_by(reactions, "Name")),
        );
        map.insert(
            "HelperBoneList".into(),
            Byml::Array(unique_names(dedupe_by(helpers, "FilePath"))),
        );
    }

    // Component/Physics: the first donor's parameters bound to the merged set.
    let mut physics_param = byml_entry(&base.entries, &base.physics_path, &base.actor, zstd)?;
    as_map_mut(&mut physics_param.pio, "PhysicsParam")?.insert(
        "ControllerSetPath".into(),
        byml_string(format!(
            "Work/Phive/ControllerSetParam/{actor_name}.phive__ControllerSetParam.gyml"
        )),
    );

    let mut entries = vec![
        InjectedPackEntry {
            path: bphcl_path,
            data: merged.raw.clone(),
        },
        InjectedPackEntry {
            path: cloth_param_path,
            data: cloth_param.to_binary_preserving_header()?,
        },
        InjectedPackEntry {
            path: controller_path,
            data: controller.to_binary_preserving_header()?,
        },
        InjectedPackEntry {
            path: physics_path.clone(),
            data: physics_param.to_binary_preserving_header()?,
        },
    ];
    let mut taken: BTreeSet<String> = entries.iter().map(|entry| entry.path.clone()).collect();
    for donor in donors {
        let replaced = donor.replaced_paths();
        for entry in &donor.entries {
            if replaced.contains(entry.path.as_str()) || !taken.insert(entry.path.clone()) {
                continue;
            }
            entries.push(entry.clone());
        }
    }
    Ok((format!("?{physics_path}"), entries))
}

/// Imports every cloth (with its skeleton) and every collidable of `source`.
fn merge_bphcl(mut target: BphclDocument, source: &BphclDocument) -> io::Result<BphclDocument> {
    for position in 0..source.cloth.len() {
        let bytes = target.merge_complete_cloth(source, position)?;
        if bytes != target.raw {
            target = BphclDocument::parse(&bytes)?;
        }
    }
    for position in 0..source.collidables.len() {
        let bytes = target.merge_collidable(source, position)?;
        if bytes != target.raw {
            target = BphclDocument::parse(&bytes)?;
        }
    }
    target.validate()?;
    Ok(target)
}

fn entry_data<'e>(entries: &'e [InjectedPackEntry], path: &str) -> Option<&'e [u8]> {
    entries
        .iter()
        .find(|entry| entry.path == path)
        .map(|entry| entry.data.as_slice())
}

fn byml_entry<'a>(
    entries: &[InjectedPackEntry],
    path: &str,
    actor: &str,
    zstd: Arc<TotkZstd<'a>>,
) -> io::Result<BymlFile<'a>> {
    let data = entry_data(entries, path)
        .ok_or_else(|| invalid(format!("physics donor {actor}: bundle has no {path}")))?;
    BymlFile::from_binary(data, zstd, path)
        .map_err(|error| invalid(format!("physics donor {actor}: {path}: {error}")))
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

/// The map items of the `key` array (non-map items are dropped).
fn list_entries(value: &Byml, key: &str) -> Vec<Byml> {
    value
        .as_map()
        .ok()
        .and_then(|map| map.get(key))
        .and_then(|list| list.as_array().ok())
        .map(|list| {
            list.iter()
                .filter(|item| item.as_map().is_ok())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn list_paths(value: &Byml, key: &str, field: &str) -> Vec<String> {
    list_entries(value, key)
        .iter()
        .filter_map(|entry| map_string(entry, field))
        .collect()
}

fn set_entry_string(entries: &mut [Byml], key: &str, text: String) {
    if let Some(Byml::Map(map)) = entries.first_mut() {
        map.insert(key.into(), byml_string(text));
    }
}

/// Keeps the first entry per `field` value; entries without it are kept.
fn dedupe_by(entries: Vec<Byml>, field: &str) -> Vec<Byml> {
    let mut seen = BTreeSet::new();
    entries
        .into_iter()
        .filter(|entry| match map_string(entry, field) {
            Some(value) => seen.insert(value),
            None => true,
        })
        .collect()
}

/// Suffixes colliding `Name` values (`Default`, `Default_2`, ...).
fn unique_names(entries: Vec<Byml>) -> Vec<Byml> {
    let mut seen = BTreeSet::new();
    entries
        .into_iter()
        .map(|mut entry| {
            let Some(name) = map_string(&entry, "Name") else {
                return entry;
            };
            let mut candidate = name.clone();
            let mut counter = 2;
            while !seen.insert(candidate.clone()) {
                candidate = format!("{name}_{counter}");
                counter += 1;
            }
            if candidate != name {
                if let Byml::Map(map) = &mut entry {
                    map.insert("Name".into(), byml_string(candidate));
                }
            }
            entry
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};

    fn romfs_zstd() -> Option<(&'static Path, Arc<TotkZstd<'static>>)> {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        if !clean_romfs.is_dir() {
            return None;
        }
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).ok()?);
        Some((clean_romfs, zstd))
    }

    fn donor_bphcl(romfs: &Path, actor: &str, zstd: Arc<TotkZstd<'static>>) -> BphclDocument {
        let (_, entries) = actor_pack::prepare_physics_entries(romfs, actor, zstd).unwrap();
        let entry = entries
            .iter()
            .find(|entry| entry.path.starts_with("Phive/Cloth/") && entry.path.ends_with(".bphcl"))
            .unwrap();
        BphclDocument::parse(&entry.data).unwrap()
    }

    #[test]
    fn blank_and_unknown_donors_are_skipped_and_one_donor_is_copied_verbatim() {
        let Some((romfs, zstd)) = romfs_zstd() else {
            return;
        };
        let donors = [
            String::new(),
            "Armor_Does_Not_Exist".into(),
            "Armor_005_Head".into(),
            " Armor_005_Head ".into(),
        ];
        let (reference, entries) = merged_physics_entries(romfs, &donors, "Armor_900_Head", zstd)
            .unwrap()
            .unwrap();
        assert_eq!(
            reference,
            "?Component/Physics/Armor_005_Head.engine__component__PhysicsParam.bgyml"
        );
        assert!(entries
            .iter()
            .any(|entry| entry.path == "Phive/Cloth/Armor_005_Head.bphcl"));
        assert!(
            merged_physics_entries(romfs, &[String::new()], "Armor_900_Head", zstd_for(romfs))
                .unwrap()
                .is_none()
        );
    }

    fn zstd_for(_romfs: &Path) -> Arc<TotkZstd<'static>> {
        romfs_zstd().unwrap().1
    }

    #[test]
    fn two_donors_merge_cloths_skeletons_collidables_and_parameters() {
        let Some((romfs, zstd)) = romfs_zstd() else {
            return;
        };
        let donors = ["Armor_005_Head".to_string(), "Armor_180_Upper".to_string()];
        let (reference, entries) =
            merged_physics_entries(romfs, &donors, "Armor_900_Head", zstd.clone())
                .unwrap()
                .unwrap();
        assert_eq!(
            reference,
            "?Component/Physics/Armor_900_Head.engine__component__PhysicsParam.bgyml"
        );
        let entry = |path: &str| entry_data(&entries, path);
        let merged =
            BphclDocument::parse(entry("Phive/Cloth/Armor_900_Head.bphcl").unwrap()).unwrap();
        let head = donor_bphcl(romfs, "Armor_005_Head", zstd.clone());
        let upper = donor_bphcl(romfs, "Armor_180_Upper", zstd.clone());
        assert_eq!(merged.cloth.len(), head.cloth.len() + upper.cloth.len());
        assert_eq!(
            merged.skeletons.len(),
            head.skeletons.len() + upper.skeletons.len()
        );
        let names: BTreeSet<_> = merged
            .collidables
            .iter()
            .map(|collidable| collidable.name.clone())
            .collect();
        for collidable in head.collidables.iter().chain(&upper.collidables) {
            assert!(names.contains(&collidable.name), "{}", collidable.name);
        }
        for replaced in [
            "Phive/Cloth/Armor_005_Head.bphcl",
            "Phive/Cloth/Armor_180_RaulSkin_Upper.bphcl",
            "Phive/ControllerSetParam/Armor_005_Head.phive__ControllerSetParam.bgyml",
            "Phive/ControllerSetParam/Armor_180_Upper.phive__ControllerSetParam.bgyml",
            "Phive/ClothParam/Armor_005_Head.phive__ClothParam.bgyml",
            "Component/Physics/Armor_180_Upper.engine__component__PhysicsParam.bgyml",
        ] {
            assert!(entry(replaced).is_none(), "{replaced}");
        }
        for kept in [
            "Phive/HelperBone/Armor_005_Head.bphhb",
            "Phive/HelperBone/Armor_180_RaulSkin_Upper.bphhb",
            "Phive/ClothReaction/Mant_Large.phive__ClothReaction.bgyml",
            "Phive/ClothReaction/Linl_Hat.phive__ClothReaction.bgyml",
        ] {
            assert!(entry(kept).is_some(), "{kept}");
        }
        let controller = BymlFile::from_binary(
            entry("Phive/ControllerSetParam/Armor_900_Head.phive__ControllerSetParam.bgyml")
                .unwrap(),
            zstd.clone(),
            "controller",
        )
        .unwrap();
        assert_eq!(
            list_paths(&controller.pio, "ClothList", "Path"),
            ["Work/Phive/Cloth/Armor_900_Head.phcl"]
        );
        assert_eq!(
            list_paths(&controller.pio, "Cloth", "FilePath"),
            ["Work/Phive/ClothParam/Armor_900_Head.phive__ClothParam.gyml"]
        );
        assert_eq!(
            list_paths(&controller.pio, "HelperBoneList", "Name"),
            ["Default", "Default_2"]
        );
        assert_eq!(list_entries(&controller.pio, "ClothReaction").len(), 9);
        let cloth_param = BymlFile::from_binary(
            entry("Phive/ClothParam/Armor_900_Head.phive__ClothParam.bgyml").unwrap(),
            zstd.clone(),
            "cloth-param",
        )
        .unwrap();
        assert_eq!(
            list_entries(&cloth_param.pio, "MeshList").len(),
            merged.cloth.len()
        );
        assert_eq!(
            map_string(&cloth_param.pio, "HktPath").as_deref(),
            Some("Work/Phive/Cloth/Armor_900_Head.hkt")
        );
        let physics = BymlFile::from_binary(
            entry("Component/Physics/Armor_900_Head.engine__component__PhysicsParam.bgyml")
                .unwrap(),
            zstd,
            "physics",
        )
        .unwrap();
        assert_eq!(
            map_string(&physics.pio, "ControllerSetPath").as_deref(),
            Some("Work/Phive/ControllerSetParam/Armor_900_Head.phive__ControllerSetParam.gyml")
        );
    }
}
