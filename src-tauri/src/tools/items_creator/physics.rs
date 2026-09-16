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
//! `ClothAdvandecOption` lists), and `Component/Physics/<actor>…` binds it
//! all. Helper bones drive bones of one specific model, so only the first
//! donor's `Phive/HelperBone/*` files stay, renamed
//! `Phive/HelperBone/<actor>.bphhb` (`<actor>_2`, …); the other donors'
//! helper-bone files and `HelperBoneList` entries are left behind, the way a
//! hand-merged pack is built (vanilla `Armor_999_Head` + `Armor_225_Head`
//! keeps the hair piece's empty list and drops the hat's `Hat_1_Armor` helper
//! bone, which the merged actor's model does not have). The `helper_bone`
//! spec field chooses a different set explicitly.
//!
//! [`transfer_helper_bones`] then runs on the assembled pack when a
//! helper-bone donor is chosen: its `Phive/HelperBone/*` files replace
//! whatever the physics step left, and the ControllerSetParam, PhysicsParam
//! and ActorParam `PhysicsRef` are renamed after the actor.

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

/// Resolves the donor list of an armor spec. A donor is either a vanilla
/// actor name (its pack is read from the RomFS) or the path of an actor pack
/// on disk (`.pack.zs` / `.pack`, see [`is_pack_path`]; a relative path
/// resolves against `asset_root` like the other spec assets). Blank,
/// malformed, duplicate or missing donors are skipped (the template's own
/// physics files stay when none is usable); one usable donor is copied
/// as-is; two or more are merged under `actor_name`. A failed merge is an
/// error rather than a silent fallback because the caller asked for the
/// combination explicitly.
pub(super) fn merged_physics_entries<'a>(
    clean_romfs: &Path,
    asset_root: &Path,
    donors: &[String],
    actor_name: &str,
    zstd: Arc<TotkZstd<'a>>,
) -> io::Result<Option<(String, Vec<InjectedPackEntry>)>> {
    let mut bundles = Vec::new();
    let mut seen = BTreeSet::new();
    for donor in donors {
        let donor = donor.trim();
        let loaded = if is_pack_path(donor) {
            let pack_path = super::resolve_asset(asset_root, Path::new(donor));
            actor_pack::prepare_physics_entries_from_file(&pack_path, zstd.clone())
        } else {
            let Some(actor) = actor_pack::resolve_physics_donor(clean_romfs, Some(donor)) else {
                if !donor.is_empty() {
                    eprintln!(
                        "[items creator] physics donor {donor} has no vanilla actor pack; skipping it"
                    );
                }
                continue;
            };
            actor_pack::prepare_physics_entries(clean_romfs, &actor, zstd.clone())
                .map(|bundle| (actor, bundle))
        };
        match loaded {
            Ok((actor, bundle)) => {
                if seen.insert(actor.clone()) {
                    bundles.push((actor, bundle));
                }
            }
            Err(error) => eprintln!(
                "[items creator] physics donor {donor} is unusable ({error}); skipping it"
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

/// Whether a donor names an actor pack file rather than a vanilla actor:
/// `Armor_005_Head` is an actor, `C:\mods\Cape\Armor_950_Head.pack.zs` (or
/// `.pack`) a file. Actor names never contain dots, so the two cannot be
/// confused.
pub(super) fn is_pack_path(donor: &str) -> bool {
    let lower = donor.to_ascii_lowercase();
    lower.ends_with(".pack.zs") || lower.ends_with(".pack")
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
    // reaction and advanced option of every donor, the first donor's helper
    // bones renamed after the actor.
    let helpers = renamed_helper_bones(base, actor_name);
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
        let advanced_options: Vec<Byml> = donors
            .iter()
            .flat_map(|donor| list_entries(&donor.controller.pio, "ClothAdvandecOption"))
            .collect();
        let helper_list = helpers.iter().map(|(entry, _)| entry.clone()).collect();
        let map = as_map_mut(&mut controller.pio, "ControllerSetParam")?;
        map.insert("ClothList".into(), Byml::Array(cloth_list));
        map.insert("Cloth".into(), Byml::Array(cloth));
        map.insert(
            "ClothReaction".into(),
            Byml::Array(dedupe_by(reactions, "Name")),
        );
        if !advanced_options.is_empty() || map.contains_key("ClothAdvandecOption") {
            map.insert(
                "ClothAdvandecOption".into(),
                Byml::Array(dedupe_by(advanced_options, "Name")),
            );
        }
        map.insert("HelperBoneList".into(), Byml::Array(helper_list));
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
    entries.extend(helpers.into_iter().map(|(_, entry)| entry));
    let mut taken: BTreeSet<String> = entries.iter().map(|entry| entry.path.clone()).collect();
    for donor in donors {
        let replaced = donor.replaced_paths();
        for entry in &donor.entries {
            if entry.path.starts_with("Phive/HelperBone/")
                || replaced.contains(entry.path.as_str())
                || !taken.insert(entry.path.clone())
            {
                continue;
            }
            entries.push(entry.clone());
        }
    }
    Ok((format!("?{physics_path}"), entries))
}

/// The helper-bone files of `donor` renamed after the actor
/// (`Phive/HelperBone/<actor>.bphhb`, `<actor>_2`, ...), each paired with
/// its rewritten `HelperBoneList` entry, in list order. Helper bones drive
/// bones of the donor's own model, so a merge takes them from the first donor
/// only. An entry whose file is missing from the donor pack is dropped with a
/// warning rather than failing the merge.
fn renamed_helper_bones(
    donor: &DonorPhysics<'_>,
    actor_name: &str,
) -> Vec<(Byml, InjectedPackEntry)> {
    let mut list = Vec::new();
    let mut files = Vec::new();
    for mut entry in list_entries(&donor.controller.pio, "HelperBoneList") {
        let Some(work_path) = map_string(&entry, "FilePath") else {
            continue;
        };
        let internal = helper_bone_internal_path(&work_path);
        let Some(data) = entry_data(&donor.entries, &internal) else {
            eprintln!(
                "[items creator] physics donor {}: {} references a missing {internal}; dropping that helper-bone entry",
                donor.actor, donor.controller_path
            );
            continue;
        };
        let stem = helper_bone_stem(actor_name, files.len());
        if let Byml::Map(map) = &mut entry {
            map.insert(
                "FilePath".into(),
                byml_string(format!("Work/Phive/HelperBone/{stem}.phhb")),
            );
        }
        list.push(entry);
        files.push(InjectedPackEntry {
            path: format!("Phive/HelperBone/{stem}.bphhb"),
            data: data.to_vec(),
        });
    }
    unique_names(list).into_iter().zip(files).collect()
}

/// `<actor>` for the first helper-bone file, `<actor>_2`, `<actor>_3`, ...
/// for the following ones.
fn helper_bone_stem(actor_name: &str, index: usize) -> String {
    if index == 0 {
        actor_name.to_owned()
    } else {
        format!("{actor_name}_{}", index + 1)
    }
}

/// Transfers the `Phive/HelperBone/*` files of `donor` into the assembled
/// pack `entries` of `actor_name`, after the physics step: whatever physics
/// the clone ended up with (the template's own, one donor's or a merged
/// bundle) keeps its cloth, its `HelperBoneList` is replaced by the donor's
/// helper-bone files (renamed `Phive/HelperBone/<actor>.bphhb`, `<actor>_2`,
/// ...), and the ControllerSetParam, `Component/Physics` and ActorParam
/// `PhysicsRef` are renamed after the actor. A clone without physics of its
/// own (a template pointing at the shared `Dummy` PhysicsParam) receives a
/// helper-bone-only controller set exactly like vanilla pieces such as
/// `Armor_008_Upper`. The donor is the user's explicit choice, so a donor
/// without usable helper bones is an error rather than a silent skip.
pub(super) fn transfer_helper_bones<'a>(
    clean_romfs: &Path,
    donor: &str,
    actor_name: &str,
    actor_file: &str,
    entries: &mut Vec<(String, Vec<u8>)>,
    zstd: Arc<TotkZstd<'a>>,
) -> io::Result<()> {
    let donor = donor.trim();
    let donor_error = |message: String| invalid(format!("helper-bone donor {donor}: {message}"));

    // The donor's helper-bone files, in HelperBoneList order.
    let (donor_ref, donor_entries) =
        actor_pack::prepare_physics_entries(clean_romfs, donor, zstd.clone())
            .map_err(|error| donor_error(error.to_string()))?;
    let donor_physics = byml_entry(
        &donor_entries,
        &actor_pack::reference_to_internal(&donor_ref),
        donor,
        zstd.clone(),
    )?;
    let donor_controller_path = map_string(&donor_physics.pio, "ControllerSetPath")
        .map(|path| actor_pack::work_path_to_internal(&path))
        .ok_or_else(|| donor_error("its PhysicsParam has no ControllerSetPath".into()))?;
    let donor_controller = byml_entry(&donor_entries, &donor_controller_path, donor, zstd.clone())?;
    let helper_entries = list_entries(&donor_controller.pio, "HelperBoneList");
    let mut helpers: Vec<(Byml, Vec<u8>)> = Vec::with_capacity(helper_entries.len());
    for entry in helper_entries {
        let Some(path) = map_string(&entry, "FilePath") else {
            continue;
        };
        let internal = helper_bone_internal_path(&path);
        let data = entry_data(&donor_entries, &internal).ok_or_else(|| {
            donor_error(format!(
                "{donor_controller_path} references a missing {internal}"
            ))
        })?;
        helpers.push((entry, data.to_vec()));
    }
    if helpers.is_empty() {
        return Err(donor_error(format!(
            "{donor_controller_path} lists no helper-bone file"
        )));
    }

    // The clone's current physics binding: ActorParam -> PhysicsParam ->
    // ControllerSetParam, each only when the pack itself carries the file.
    let find = |entries: &[(String, Vec<u8>)], path: &str| -> Option<Vec<u8>> {
        entries
            .iter()
            .find(|(name, _)| name == path)
            .map(|(_, data)| data.clone())
    };
    let actor_data = find(entries, actor_file)
        .ok_or_else(|| invalid(format!("assembled pack has no {actor_file}")))?;
    let mut actor = BymlFile::from_binary(&actor_data, zstd.clone(), actor_file)?;
    let current_physics_path = actor
        .pio
        .as_map()
        .ok()
        .and_then(|map| map.get("Components"))
        .and_then(|components| map_string(components, "PhysicsRef"))
        .map(|reference| actor_pack::reference_to_internal(&reference))
        .filter(|path| find(entries, path).is_some());
    let mut physics_param = match &current_physics_path {
        Some(path) => {
            BymlFile::from_binary(&find(entries, path).unwrap_or_default(), zstd.clone(), path)?
        }
        None => donor_physics,
    };
    let current_controller_path = current_physics_path
        .as_ref()
        .and_then(|_| map_string(&physics_param.pio, "ControllerSetPath"))
        .map(|path| actor_pack::work_path_to_internal(&path))
        .filter(|path| find(entries, path).is_some());
    let mut controller = match &current_controller_path {
        Some(path) => {
            BymlFile::from_binary(&find(entries, path).unwrap_or_default(), zstd.clone(), path)?
        }
        None => {
            // A helper-bone-only controller set, as vanilla ships for
            // pieces without cloth (ConstraintControllerPath + HelperBoneList).
            let mut map = Map::default();
            map.insert(
                "ConstraintControllerPath".into(),
                byml_string(
                    map_string(&donor_controller.pio, "ConstraintControllerPath")
                        .unwrap_or_default(),
                ),
            );
            let mut fresh = donor_controller;
            fresh.pio = Byml::Map(map);
            fresh
        }
    };

    // Old helper-bone files and the three renamed documents leave the pack.
    let controller_path =
        format!("Phive/ControllerSetParam/{actor_name}.phive__ControllerSetParam.bgyml");
    let physics_path =
        format!("Component/Physics/{actor_name}.engine__component__PhysicsParam.bgyml");
    let old_helpers: BTreeSet<String> = list_paths(&controller.pio, "HelperBoneList", "FilePath")
        .iter()
        .map(|path| helper_bone_internal_path(path))
        .collect();
    entries.retain(|(name, _)| {
        !(name.starts_with("Phive/HelperBone/")
            || old_helpers.contains(name)
            || Some(name) == current_physics_path.as_ref()
            || Some(name) == current_controller_path.as_ref()
            || *name == controller_path
            || *name == physics_path)
    });

    let mut helper_list = Vec::with_capacity(helpers.len());
    for (index, (mut entry, data)) in helpers.into_iter().enumerate() {
        let stem = helper_bone_stem(actor_name, index);
        if let Byml::Map(map) = &mut entry {
            map.insert(
                "FilePath".into(),
                byml_string(format!("Work/Phive/HelperBone/{stem}.phhb")),
            );
        }
        helper_list.push(entry);
        entries.push((format!("Phive/HelperBone/{stem}.bphhb"), data));
    }
    as_map_mut(&mut controller.pio, "ControllerSetParam")?.insert(
        "HelperBoneList".into(),
        Byml::Array(unique_names(helper_list)),
    );
    as_map_mut(&mut physics_param.pio, "PhysicsParam")?.insert(
        "ControllerSetPath".into(),
        byml_string(format!(
            "Work/Phive/ControllerSetParam/{actor_name}.phive__ControllerSetParam.gyml"
        )),
    );
    entries.push((controller_path, controller.to_binary_preserving_header()?));
    entries.push((
        physics_path.clone(),
        physics_param.to_binary_preserving_header()?,
    ));

    let components = actor
        .pio
        .as_mut_map()
        .map_err(|_| invalid(format!("{actor_file} root is not a map")))?
        .entry("Components".into())
        .or_insert_with(|| Byml::Map(Map::default()));
    as_map_mut(components, "ActorParam Components")?
        .insert("PhysicsRef".into(), byml_string(format!("?{physics_path}")));
    let actor_bytes = actor.to_binary_preserving_header()?;
    if let Some(slot) = entries.iter_mut().find(|(name, _)| name == actor_file) {
        slot.1 = actor_bytes;
    }
    Ok(())
}

/// `Work/Phive/HelperBone/X.phhb` -> `Phive/HelperBone/X.bphhb`.
fn helper_bone_internal_path(work_path: &str) -> String {
    let internal = actor_pack::work_path_to_internal(work_path);
    match internal.strip_suffix(".phhb") {
        Some(stem) => format!("{stem}.bphhb"),
        None => internal,
    }
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
        let (reference, entries) =
            merged_physics_entries(romfs, romfs, &donors, "Armor_900_Head", zstd)
                .unwrap()
                .unwrap();
        assert_eq!(
            reference,
            "?Component/Physics/Armor_005_Head.engine__component__PhysicsParam.bgyml"
        );
        assert!(entries
            .iter()
            .any(|entry| entry.path == "Phive/Cloth/Armor_005_Head.bphcl"));
        assert!(merged_physics_entries(
            romfs,
            romfs,
            &[String::new()],
            "Armor_900_Head",
            zstd_for(romfs)
        )
        .unwrap()
        .is_none());
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
            merged_physics_entries(romfs, romfs, &donors, "Armor_900_Head", zstd.clone())
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
            "Phive/HelperBone/Armor_005_Head.bphhb",
            "Phive/HelperBone/Armor_180_RaulSkin_Upper.bphhb",
        ] {
            assert!(entry(replaced).is_none(), "{replaced}");
        }
        for kept in [
            "Phive/HelperBone/Armor_900_Head.bphhb",
            "Phive/ClothReaction/Mant_Large.phive__ClothReaction.bgyml",
            "Phive/ClothReaction/Linl_Hat.phive__ClothReaction.bgyml",
        ] {
            assert!(entry(kept).is_some(), "{kept}");
        }
        // The first donor's helper bones, renamed; the cape's are not carried.
        let head_entries = pack_entries(romfs, "Armor_005_Head", zstd.clone());
        assert_eq!(
            entry("Phive/HelperBone/Armor_900_Head.bphhb"),
            super::tests::entry(&head_entries, "Phive/HelperBone/Armor_005_Head.bphhb")
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.path.starts_with("Phive/HelperBone/"))
                .count(),
            1
        );
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
            ["Default"]
        );
        assert_eq!(
            list_paths(&controller.pio, "HelperBoneList", "FilePath"),
            ["Work/Phive/HelperBone/Armor_900_Head.phhb"]
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

    /// Vanilla `Armor_999_Head` (Link's hair, no helper bones) merged with
    /// `Armor_225_Head` (a hat with cloth and a `Hat_1_Armor` helper bone)
    /// must come out like the hand-merged pack: the hair piece's parameters
    /// with the hat's cloths, reactions and collidables appended, an empty
    /// `HelperBoneList` and no helper-bone file at all.
    #[test]
    fn hair_and_hat_merge_matches_the_hand_merged_pack() {
        let Some((romfs, zstd)) = romfs_zstd() else {
            return;
        };
        let donors = ["Armor_999_Head".to_string(), "Armor_225_Head".to_string()];
        let (_, entries) =
            merged_physics_entries(romfs, romfs, &donors, "Armor_999_Head", zstd.clone())
                .unwrap()
                .unwrap();
        let entry = |path: &str| entry_data(&entries, path);
        let merged =
            BphclDocument::parse(entry("Phive/Cloth/Armor_999_Head.bphcl").unwrap()).unwrap();
        merged.validate().unwrap();
        let cloth_names: Vec<_> = merged
            .cloth
            .iter()
            .map(|cloth| cloth.name.as_str())
            .collect();
        assert_eq!(cloth_names.len(), 19);
        assert_eq!(cloth_names[0], "Hair_1_Havok");
        assert_eq!(
            &cloth_names[16..],
            ["Hair_ABC_225_Havok", "Hair_D_225_Havok", "Hat_225_Havok"]
        );
        assert_eq!(merged.skeletons.len(), 19);
        assert_eq!(merged.collidables.len(), 50);
        assert!(!entries
            .iter()
            .any(|entry| entry.path.starts_with("Phive/HelperBone/")));
        for kept in [
            "Phive/ClothReaction/Link_Hair_H_A.phive__ClothReaction.bgyml",
            "Phive/ClothReaction/Link_Hair_Back_SimulationMesh.phive__ClothReaction.bgyml",
            "Phive/ClothReaction/Link_Hair_2_Fine.phive__ClothReaction.bgyml",
            "Phive/ClothReaction/Linl_Hat.phive__ClothReaction.bgyml",
            "Phive/ClothReaction/Link_Hair_Kishin.phive__ClothReaction.bgyml",
        ] {
            assert!(entry(kept).is_some(), "{kept}");
        }
        assert!(entry("Phive/Cloth/Armor_225_Head.bphcl").is_none());
        let controller = BymlFile::from_binary(
            entry("Phive/ControllerSetParam/Armor_999_Head.phive__ControllerSetParam.bgyml")
                .unwrap(),
            zstd.clone(),
            "controller",
        )
        .unwrap();
        assert!(list_entries(&controller.pio, "HelperBoneList").is_empty());
        assert!(controller
            .pio
            .as_map()
            .unwrap()
            .contains_key("HelperBoneList"));
        let reactions = list_paths(&controller.pio, "ClothReaction", "Name");
        assert_eq!(reactions.len(), 16 + 3);
        assert_eq!(
            reactions[16..],
            ["Hair_D_225_Havok", "Hat_225_Havok", "Hair_ABC_225_Havok"]
        );
        assert_eq!(
            list_entries(&controller.pio, "ClothAdvandecOption").len(),
            16
        );
        let cloth_param = BymlFile::from_binary(
            entry("Phive/ClothParam/Armor_999_Head.phive__ClothParam.bgyml").unwrap(),
            zstd,
            "cloth-param",
        )
        .unwrap();
        let mesh_names = list_paths(&cloth_param.pio, "MeshList", "Name");
        assert_eq!(mesh_names.len(), 19);
        assert_eq!(
            mesh_names[16..],
            ["Hair_ABC_225_Havok", "Hair_D_225_Havok", "Hat_225_Havok"]
        );
        let collidable_names = list_paths(&cloth_param.pio, "CollidableList", "Name");
        assert_eq!(collidable_names.len(), 42 + 6);
        assert!(collidable_names.contains(&"Link:Collidable_Head_Cap".to_string()));
    }

    /// Every entry of a vanilla actor pack, as the armor generator holds them.
    fn pack_entries(
        romfs: &Path,
        actor: &str,
        zstd: Arc<TotkZstd<'static>>,
    ) -> Vec<(String, Vec<u8>)> {
        let path = romfs.join("Pack/Actor").join(format!("{actor}.pack.zs"));
        let pack =
            crate::file_format::Pack::PackFile::from_binary(&std::fs::read(path).unwrap(), zstd)
                .unwrap();
        pack.sarc
            .files()
            .map(|file| (file.name().unwrap().to_owned(), file.data().to_vec()))
            .collect()
    }

    fn entry<'e>(entries: &'e [(String, Vec<u8>)], path: &str) -> Option<&'e [u8]> {
        entries
            .iter()
            .find(|(name, _)| name == path)
            .map(|(_, data)| data.as_slice())
    }

    #[test]
    fn helper_bones_replace_the_cloth_pieces_helper_file_and_rename_the_binding() {
        let Some((romfs, zstd)) = romfs_zstd() else {
            return;
        };
        // Armor_005_Head: cloth + helper bones of its own; the donor's
        // helper bones (Armor_001_Upper's RaulSkin file) take their place.
        let actor_file = "Actor/Armor_005_Head.engine__actor__ActorParam.bgyml";
        let mut entries = pack_entries(romfs, "Armor_005_Head", zstd.clone());
        let before = entries.len();
        transfer_helper_bones(
            romfs,
            " Armor_001_Upper ",
            "Armor_900_Head",
            actor_file,
            &mut entries,
            zstd.clone(),
        )
        .unwrap();
        assert_eq!(entries.len(), before);
        for gone in [
            "Phive/HelperBone/Armor_005_Head.bphhb",
            "Phive/ControllerSetParam/Armor_005_Head.phive__ControllerSetParam.bgyml",
            "Component/Physics/Armor_005_Head.engine__component__PhysicsParam.bgyml",
        ] {
            assert!(entry(&entries, gone).is_none(), "{gone}");
        }
        let donor_entries = pack_entries(romfs, "Armor_001_Upper", zstd.clone());
        assert_eq!(
            entry(&entries, "Phive/HelperBone/Armor_900_Head.bphhb"),
            entry(
                &donor_entries,
                "Phive/HelperBone/Armor_001_RaulSkin_Upper.bphhb"
            )
        );
        // The cloth side is untouched.
        assert!(entry(&entries, "Phive/Cloth/Armor_005_Head.bphcl").is_some());
        assert!(entry(
            &entries,
            "Phive/ClothParam/Armor_005_Head.phive__ClothParam.bgyml"
        )
        .is_some());
        let controller = BymlFile::from_binary(
            entry(
                &entries,
                "Phive/ControllerSetParam/Armor_900_Head.phive__ControllerSetParam.bgyml",
            )
            .unwrap(),
            zstd.clone(),
            "controller",
        )
        .unwrap();
        assert_eq!(
            list_paths(&controller.pio, "HelperBoneList", "FilePath"),
            ["Work/Phive/HelperBone/Armor_900_Head.phhb"]
        );
        assert_eq!(
            list_paths(&controller.pio, "HelperBoneList", "Name"),
            ["Default"]
        );
        assert_eq!(
            list_paths(&controller.pio, "ClothList", "Path"),
            ["Work/Phive/Cloth/Armor_005_Head.phcl"]
        );
        assert_eq!(list_entries(&controller.pio, "ClothReaction").len(), 8);
        let physics = BymlFile::from_binary(
            entry(
                &entries,
                "Component/Physics/Armor_900_Head.engine__component__PhysicsParam.bgyml",
            )
            .unwrap(),
            zstd.clone(),
            "physics",
        )
        .unwrap();
        assert_eq!(
            map_string(&physics.pio, "ControllerSetPath").as_deref(),
            Some("Work/Phive/ControllerSetParam/Armor_900_Head.phive__ControllerSetParam.gyml")
        );
        assert_eq!(
            map_string(&physics.pio, "$parent").as_deref(),
            Some("Work/Component/Physics/ProjectBase.engine__component__PhysicsParam.gyml")
        );
        let actor =
            BymlFile::from_binary(entry(&entries, actor_file).unwrap(), zstd, "actor").unwrap();
        let components = actor
            .pio
            .as_map()
            .unwrap()
            .get("Components")
            .unwrap()
            .clone();
        assert_eq!(
            map_string(&components, "PhysicsRef").as_deref(),
            Some("?Component/Physics/Armor_900_Head.engine__component__PhysicsParam.bgyml")
        );
        assert!(map_string(&components, "ArmorRef").is_some());
    }

    #[test]
    fn helper_bones_give_a_physicsless_piece_a_helper_only_controller_set() {
        let Some((romfs, zstd)) = romfs_zstd() else {
            return;
        };
        // Armor_001_Lower points at the shared Dummy PhysicsParam and has no
        // Phive entries; the result mirrors vanilla helper-only pieces.
        let actor_file = "Actor/Armor_001_Lower.engine__actor__ActorParam.bgyml";
        let mut entries = pack_entries(romfs, "Armor_001_Lower", zstd.clone());
        assert!(!entries.iter().any(|(name, _)| name.starts_with("Phive/")));
        let before = entries.len();
        transfer_helper_bones(
            romfs,
            "Armor_020_Lower",
            "Armor_901_Lower",
            actor_file,
            &mut entries,
            zstd.clone(),
        )
        .unwrap();
        assert_eq!(entries.len(), before + 3);
        let donor_entries = pack_entries(romfs, "Armor_020_Lower", zstd.clone());
        assert_eq!(
            entry(&entries, "Phive/HelperBone/Armor_901_Lower.bphhb"),
            entry(&donor_entries, "Phive/HelperBone/Armor_020_Lower.bphhb")
        );
        let controller = BymlFile::from_binary(
            entry(
                &entries,
                "Phive/ControllerSetParam/Armor_901_Lower.phive__ControllerSetParam.bgyml",
            )
            .unwrap(),
            zstd.clone(),
            "controller",
        )
        .unwrap();
        let keys: BTreeSet<String> = controller
            .pio
            .as_map()
            .unwrap()
            .keys()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            keys.iter().map(String::as_str).collect::<Vec<_>>(),
            ["ConstraintControllerPath", "HelperBoneList"]
        );
        assert_eq!(
            list_paths(&controller.pio, "HelperBoneList", "FilePath"),
            ["Work/Phive/HelperBone/Armor_901_Lower.phhb"]
        );
        let physics = BymlFile::from_binary(
            entry(
                &entries,
                "Component/Physics/Armor_901_Lower.engine__component__PhysicsParam.bgyml",
            )
            .unwrap(),
            zstd.clone(),
            "physics",
        )
        .unwrap();
        assert_eq!(
            map_string(&physics.pio, "ControllerSetPath").as_deref(),
            Some("Work/Phive/ControllerSetParam/Armor_901_Lower.phive__ControllerSetParam.gyml")
        );
        let actor =
            BymlFile::from_binary(entry(&entries, actor_file).unwrap(), zstd.clone(), "actor")
                .unwrap();
        assert_eq!(
            map_string(
                actor.pio.as_map().unwrap().get("Components").unwrap(),
                "PhysicsRef"
            )
            .as_deref(),
            Some("?Component/Physics/Armor_901_Lower.engine__component__PhysicsParam.bgyml")
        );
        // A donor without helper bones (or without a pack) is an error.
        let mut untouched = pack_entries(romfs, "Armor_001_Lower", zstd.clone());
        assert!(transfer_helper_bones(
            romfs,
            "Armor_006_Head",
            "Armor_901_Lower",
            actor_file,
            &mut untouched,
            zstd.clone()
        )
        .is_err());
        assert!(transfer_helper_bones(
            romfs,
            "Armor_Nope",
            "Armor_901_Lower",
            actor_file,
            &mut untouched,
            zstd
        )
        .is_err());
    }
}
