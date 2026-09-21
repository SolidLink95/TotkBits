//! Zonai devices for the items creator.
//!
//! A Zonai device is two actors: the device itself (`SpObj_Cannon_A_01`) and
//! the capsule that is the pouch item (`SpObj_Cannon_Capsule_A_01`, whose
//! CapsuleParam / AttachmentParam name the device). A custom device clones
//! both packs under new names, with any parameter of any BYML entry of the
//! device pack overridable (`params`), plus optional *companions*: other
//! vanilla actors the device references (its projectile, for a cannon) that
//! are cloned with their own parameter overrides and swapped into the
//! device's references. The device and each companion may also take over
//! the chemistry of another vanilla actor (`chemical`: its `Chemical/*` and
//! `Component/ChemicalParam/*` entries are copied in and the ActorParam
//! `ChemicalRef` pointed at them), the way a beam becomes a fire beam.
//! A companion whose `actor_name` equals its `template_actor` is edited *in
//! place*: the vanilla pack is rewritten under its own name with the
//! overrides applied (no entry is renamed) and its RSDB rows are updated
//! rather than cloned. That is the only way to change a projectile the
//! device's native logic insists on (the Beam Emitter fires nothing but
//! `BeamosBeam`; a renamed copy never fires), at the price of every vanilla
//! user of that actor seeing the change.
//! Like a weapon, the device always gets a model file of its own
//! (`Model/<project>.<actor>.bfres.mc`: the template model, or the custom
//! one, renamed after the actor) and its ActorInfo row follows the pack
//! (`FmdbName`, the ELink / SLink user names, the instance heap).
//! No actor, file or parameter name is assumed here:
//! the list of devices comes from PouchActorInfo, the editable parameters
//! from the template pack, and the companion candidates from the references
//! the pack really holds.
//!
//! Cloning rules (shared by device, capsule and companions):
//! - The ActorParam is renamed after the new actor (it names the actor).
//! - Every other BYML entry keeps its vanilla name unless it was edited: an
//!   unchanged entry is the vanilla file, and the game's own derived actors
//!   keep the base names too (`Drake_Beam_Small_Fire` carries
//!   `Actor/BeamosBeam` and `GameBalance/AttackParam/BeamosBeam`). Renaming
//!   an unchanged entry breaks identities other actors share with it: the
//!   Blackboard param table a shooter and its projectile both carry is how
//!   the Beamos device hands its beam the range and damage, and a beam whose
//!   copy was renamed never fires.
//! - An edited entry named after the template is renamed after the new
//!   actor; an edited entry with a shared name
//!   (`Life/LifeParameters/SpObj_Weapon_A`) is renamed after the new actor
//!   as well ("made private"), because the game's resource cache is keyed
//!   by path and a vanilla actor loading the same path first would
//!   otherwise hand the edited actor the vanilla values. Every reference to
//!   a renamed entry in the pack follows.
//! - ELink / SLink user names, AS, effect and AI files keep their vanilla
//!   names: the effect sets, sounds, animation sets and AI they name are
//!   vanilla files the clone goes on using.

use super::{
    actor_pack, assets, collision, gamedata, messages, rsdb, shared::SharedFiles, vendor,
    UiTextureReport, VendorTarget,
};
use crate::{
    compression::meshcodec::MeshCodec,
    file_format::{
        BinTextFile::BymlFile,
        Model3D::bfres::{toolbox::ResFile, BfresFile},
        Pack::PackFile,
    },
    Zstd::TotkZstd,
};
use roead::byml::Byml;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

/// The `kind` value that marks a Zonai entry in a mixed spec list.
pub const KIND: &str = "Zonai";

/// `template pack entry path -> field path -> value`. Field paths are the
/// ones [`inspect_params`] lists: map keys joined with `.`, array elements
/// as `[index]` (`CannonParam.Gravity`, `AttackParams[0].DamageMultiplier`).
/// A path that names a whole list of scalars takes a JSON list
/// (`AttackParams[0].DamageElements: ["Fire"]`).
pub type ParamOverrides = BTreeMap<String, BTreeMap<String, JsonValue>>;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ZonaiAssets {
    /// Custom device model: a `.bfres` / `.bfres.mc` (any compression) that
    /// replaces the template model. Written as `Model/<project>.<actor>.bfres.mc`.
    #[serde(default, alias = "bfres")]
    pub model: Option<PathBuf>,
    /// Custom FBX whose meshes replace the geometry (of `model` when given,
    /// else of the template model).
    #[serde(default)]
    pub fbx: Option<PathBuf>,
    /// With an FBX: import its skeleton too ("Replace bones").
    #[serde(default, alias = "import_skeleton")]
    pub replace_bones: bool,
    /// Capsule (pouch) icon.
    #[serde(default)]
    pub icon_png: Option<PathBuf>,
    /// Device icon (`UI/Tex/Icon/<device>.bntx.zs`, used by the fuse /
    /// Autobuild screens); the capsule icon is not reused for it.
    #[serde(default)]
    pub device_icon_png: Option<PathBuf>,
    /// Collision mesh (OBJ, in the model's space): decomposed into convex
    /// pieces with CoACD and written as the device's Polytope ShapeParam,
    /// the rigid body's centre of mass moved to its centroid (see
    /// [`super::collision`]). Blank keeps the template collision.
    #[serde(default, alias = "collision_obj")]
    pub physics_obj: Option<PathBuf>,
    /// Phive material preset of the collision pieces (`Material_Stone` by default).
    #[serde(default, alias = "collision_material")]
    pub physics_material: Option<String>,
}

/// Another vanilla actor the device references (a projectile), cloned under
/// a new name with its own overrides; every reference to the template in the
/// device pack is pointed at the clone.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CompanionSpec {
    pub template_actor: String,
    pub actor_name: String,
    #[serde(default)]
    pub params: ParamOverrides,
    /// Vanilla actor whose chemistry the clone takes over (see [`ZonaiSpec::chemical`]).
    #[serde(default, alias = "chemical_actor")]
    pub chemical: Option<String>,
}

impl CompanionSpec {
    /// The vanilla actor is edited in place instead of cloned.
    pub fn is_in_place(&self) -> bool {
        self.actor_name.trim() == self.template_actor
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ZonaiSpec {
    /// The new device actor.
    pub actor_name: String,
    /// Vanilla device actor (one with a capsule), e.g. `SpObj_Cannon_A_01`.
    pub template_actor: String,
    /// `"Zonai"`; the loader also recognises a `SpObj_` template.
    #[serde(default)]
    pub kind: Option<String>,
    /// The new capsule (pouch item) actor. Defaults to the template capsule
    /// with the device's trailing number, else `<actor_name>_Capsule`.
    #[serde(default)]
    pub capsule_name: Option<String>,
    /// Pouch name and description of the capsule.
    pub display_name: String,
    pub description: String,
    /// Attachment (fuse) adjective of the device; defaults to the display name.
    #[serde(default)]
    pub adjective: Option<String>,
    #[serde(default)]
    pub params: ParamOverrides,
    /// Vanilla actor whose `Chemical/*` and `Component/ChemicalParam/*`
    /// entries are copied into the clone, its ActorParam `ChemicalRef`
    /// pointed at them (`Drake_Beam_Small_Fire` turns a beam into a fire
    /// beam). Blank keeps the template chemistry.
    #[serde(default, alias = "chemical_actor")]
    pub chemical: Option<String>,
    /// Zonaite the Autobuild screen charges for the device (every vanilla
    /// device costs 3). Written the way `Obj_AirPlatform` prices its sky
    /// platform at 100: a `GameParameter/AutoBuilderReplacementParam` entry
    /// holding `OverwriteConsumptionNum`, referenced from the device's
    /// GameParameterTable. Blank keeps the template's price.
    #[serde(default, alias = "zonaite_cost")]
    pub autobuild_cost: Option<i32>,
    #[serde(default)]
    pub companions: Vec<CompanionSpec>,
    #[serde(default)]
    pub assets: ZonaiAssets,
    #[serde(default)]
    pub vendors: Vec<VendorTarget>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionReport {
    pub actor_name: String,
    pub template_actor: String,
    pub actor_pack: PathBuf,
    pub private_files: Vec<String>,
    /// The `ChemicalRef` written when a chemical donor was given.
    pub chemical_ref: Option<String>,
    pub rsdb: Vec<PathBuf>,
}

/// Everything written for one Zonai device, before the shared RSTB pass.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZonaiGenerationReport {
    pub actor_name: String,
    pub capsule_name: String,
    pub template_actor: String,
    pub template_capsule: String,
    pub actor_pack: PathBuf,
    pub capsule_pack: PathBuf,
    /// Shared-name entries that were edited and therefore renamed after the actor.
    pub private_files: Vec<String>,
    /// The `ChemicalRef` written when a chemical donor was given.
    pub chemical_ref: Option<String>,
    /// The AutoBuilderReplacementParam entry written for `autobuild_cost`.
    pub autobuild_param: Option<String>,
    pub companions: Vec<CompanionReport>,
    /// The convex collision written for `physics_obj`.
    pub collision: Option<collision::CollisionReport>,
    pub model: Option<PathBuf>,
    pub textures: Vec<PathBuf>,
    pub ui_textures: Vec<UiTextureReport>,
    pub messages: PathBuf,
    pub rsdb: Vec<PathBuf>,
    pub game_data: gamedata::WeaponGameDataReport,
    pub vendor_packs: Vec<vendor::VendorPackReport>,
}

/// One vanilla device the creator can start from.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZonaiDeviceEntry {
    /// The device actor (template).
    pub actor: String,
    /// Its capsule (the pouch item, whose icon and name the UI shows).
    pub capsule: String,
    /// Pouch name of the capsule.
    pub name: String,
    pub has_icon: bool,
}

/// One editable scalar of a pack entry.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamField {
    pub path: String,
    /// `int`, `uint`, `float`, `bool` or `string`.
    pub kind: &'static str,
    pub value: JsonValue,
    /// Not in the entry itself but in a `$parent` it inherits from.
    pub inherited: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamFile {
    /// Entry path inside the template pack (the key `params` uses).
    pub path: String,
    /// Named after the actor (renamed after the new actor when edited).
    pub own: bool,
    pub parent: Option<String>,
    pub fields: Vec<ParamField>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamInspection {
    pub actor: String,
    pub files: Vec<ParamFile>,
    /// Other actors the pack references (`Work/Actor/<X>...ActorParam.gyml`)
    /// that have a pack of their own: the companion candidates.
    pub referenced_actors: Vec<String>,
}

/// Every vanilla device that has a capsule: PouchActorInfo rows with a
/// `CapsuleContent` and no `BundleActor` (the `_Bundle_A` rows are the
/// dispenser bundles of the same capsule). Sorted by pouch name.
pub fn zonai_devices(
    clean_romfs: &Path,
    zstd: Arc<TotkZstd<'_>>,
    item_names: &BTreeMap<String, String>,
) -> io::Result<Vec<ZonaiDeviceEntry>> {
    let rsdb = clean_romfs.join("RSDB");
    let (_, source) =
        super::version::discover_product_file(&rsdb, "PouchActorInfo.Product.", ".rstbl.byml.zs")?;
    let file = BymlFile::new(&source, zstd)
        .ok_or_else(|| invalid_data(format!("invalid PouchActorInfo {}", source.display())))?;
    let rows = file
        .pio
        .as_array()
        .map_err(|_| invalid_data("PouchActorInfo is not an array"))?;
    let actor_dir = clean_romfs.join("Pack/Actor");
    let icon_dir = clean_romfs.join("UI/Tex/Icon");
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    for row in rows {
        let Ok(map) = row.as_map() else { continue };
        if map.contains_key("BundleActor") {
            continue;
        }
        let Some(content) = map.get("CapsuleContent").and_then(|v| v.as_string().ok()) else {
            continue;
        };
        let Some(capsule) = map.get("__RowId").and_then(|v| v.as_string().ok()) else {
            continue;
        };
        let device = bare_actor(content);
        if device.is_empty() || !seen.insert(device.clone()) {
            continue;
        }
        if !actor_dir.join(format!("{device}.pack.zs")).is_file()
            || !actor_dir.join(format!("{capsule}.pack.zs")).is_file()
        {
            continue;
        }
        entries.push(ZonaiDeviceEntry {
            name: item_names
                .get(capsule.as_str())
                .cloned()
                .unwrap_or_default(),
            has_icon: icon_dir.join(format!("{capsule}.bntx.zs")).is_file(),
            actor: device,
            capsule: capsule.to_string(),
        });
    }
    entries.sort_by(|a, b| {
        (a.name.is_empty(), &a.name, &a.actor).cmp(&(b.name.is_empty(), &b.name, &b.actor))
    });
    Ok(entries)
}

/// Every scalar of every BYML entry of `actor`'s pack, `$parent`-inherited
/// values included, plus the actors the pack references.
pub fn inspect_params(
    clean_romfs: &Path,
    actor: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<ParamInspection> {
    validate_actor_name(actor)?;
    let bytes = read_vanilla_pack(clean_romfs, actor)?;
    let pack = PackFile::from_binary(&bytes, zstd.clone())?;
    let mut documents: BTreeMap<String, Byml> = BTreeMap::new();
    for file in pack.sarc.files() {
        let Some(name) = file.name() else { continue };
        if !name.ends_with(".bgyml") {
            continue;
        }
        let document = BymlFile::from_binary(file.data(), zstd.clone(), name)?;
        documents.insert(name.to_owned(), document.pio);
    }
    let actor_dir = clean_romfs.join("Pack/Actor");
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for document in documents.values() {
        collect_actor_references(document, &mut referenced);
    }
    let referenced_actors = referenced
        .into_iter()
        .filter(|name| name != actor && actor_dir.join(format!("{name}.pack.zs")).is_file())
        .collect();
    let mut files = Vec::with_capacity(documents.len());
    for (path, document) in &documents {
        let parent = parent_reference(document);
        let resolved_parent = resolve_parent(document, &documents, clean_romfs, zstd.clone(), 0)?;
        let mut fields = Vec::new();
        flatten(document, "", false, &mut fields);
        if let Some(parent_value) = &resolved_parent {
            let own: BTreeSet<String> = fields.iter().map(|f| f.path.clone()).collect();
            let mut inherited = Vec::new();
            flatten(parent_value, "", true, &mut inherited);
            fields.extend(
                inherited
                    .into_iter()
                    .filter(|field| field.path != "$parent" && !own.contains(&field.path)),
            );
        }
        fields.sort_by(|a, b| path_sort_key(&a.path).cmp(&path_sort_key(&b.path)));
        files.push(ParamFile {
            own: file_name_of(path).contains(actor),
            path: path.clone(),
            parent,
            fields,
        });
    }
    // Actor-specific entries first, then the shared ones, each group by path.
    files.sort_by(|a, b| (!a.own, &a.path).cmp(&(!b.own, &b.path)));
    Ok(ParamInspection {
        actor: actor.to_owned(),
        files,
        referenced_actors,
    })
}

/// The template device (with its capsule and packs) a spec builds on.
struct TemplateZonai {
    capsule: String,
    device_pack: Vec<u8>,
    capsule_pack: Vec<u8>,
}

impl TemplateZonai {
    fn load(clean_romfs: &Path, template_actor: &str, zstd: Arc<TotkZstd<'_>>) -> io::Result<Self> {
        let devices = zonai_devices(clean_romfs, zstd, &BTreeMap::new())?;
        let entry = devices
            .into_iter()
            .find(|entry| entry.actor == template_actor)
            .ok_or_else(|| {
                invalid(format!(
                    "{template_actor} is not a Zonai device with a capsule (no PouchActorInfo row names it as CapsuleContent)"
                ))
            })?;
        Ok(Self {
            device_pack: read_vanilla_pack(clean_romfs, template_actor)?,
            capsule_pack: read_vanilla_pack(clean_romfs, &entry.capsule)?,
            capsule: entry.capsule,
        })
    }
}

impl ZonaiSpec {
    /// The capsule actor written: the explicit name, else the template
    /// capsule with the device's trailing number (`SpObj_Cannon_Capsule_A_01`
    /// + `SpObj_Cannon_A_90` -> `SpObj_Cannon_Capsule_A_90`), else
    /// `<actor>_Capsule`.
    pub fn capsule_name_for(&self, template_capsule: &str) -> String {
        if let Some(name) = self.capsule_name.as_deref().map(str::trim) {
            if !name.is_empty() {
                return name.to_owned();
            }
        }
        default_capsule_name(&self.template_actor, template_capsule, &self.actor_name)
    }

    /// Every *new* actor this entry writes besides the device (a companion
    /// edited in place is a vanilla actor, not a new one).
    pub fn extra_actor_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .companions
            .iter()
            .filter(|companion| !companion.is_in_place())
            .map(|companion| companion.actor_name.trim().to_owned())
            .collect();
        if let Some(capsule) = self.capsule_name.as_deref().map(str::trim) {
            if !capsule.is_empty() {
                names.push(capsule.to_owned());
            }
        }
        names
    }

    pub fn validate(&self, asset_root: &Path) -> io::Result<()> {
        validate_actor_name(&self.actor_name)?;
        validate_actor_name(&self.template_actor)?;
        if self.actor_name == self.template_actor {
            return Err(invalid("the device actor must differ from its template"));
        }
        if self.display_name.trim().is_empty() {
            return Err(invalid("display_name is required"));
        }
        if self.description.trim().is_empty() {
            return Err(invalid("description is required"));
        }
        let mut names: BTreeSet<&str> = BTreeSet::new();
        names.insert(&self.actor_name);
        if let Some(capsule) = self.capsule_name.as_deref().map(str::trim) {
            if !capsule.is_empty() {
                validate_actor_name(capsule)?;
                if !names.insert(capsule) {
                    return Err(invalid(format!("{capsule} is used twice in the entry")));
                }
            }
        }
        if let Some(donor) = chemical_donor(self.chemical.as_deref()) {
            validate_actor_name(donor)?;
        }
        if self.autobuild_cost.is_some_and(|cost| cost < 0) {
            return Err(invalid("autobuild_cost cannot be negative"));
        }
        for companion in &self.companions {
            let name = companion.actor_name.trim();
            validate_actor_name(name)?;
            validate_actor_name(&companion.template_actor)?;
            if let Some(donor) = chemical_donor(companion.chemical.as_deref()) {
                validate_actor_name(donor)?;
            }
            // A companion named like its template is the vanilla actor
            // edited in place (see the module notes).
            if !names.insert(name) {
                return Err(invalid(format!("{name} is used twice in the entry")));
            }
        }
        for (label, path) in [
            ("model", self.assets.model.as_deref()),
            ("fbx", self.assets.fbx.as_deref()),
            ("icon_png", self.assets.icon_png.as_deref()),
            ("device_icon_png", self.assets.device_icon_png.as_deref()),
            ("physics_obj", self.assets.physics_obj.as_deref()),
        ] {
            if let Some(path) = path {
                let resolved = super::resolve_asset(asset_root, path);
                if !resolved.is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("{label} is missing: {}", resolved.display()),
                    ));
                }
            }
        }
        for target in &self.vendors {
            vendor::validate_vendor(target)?;
        }
        Ok(())
    }

    pub fn generate_files(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        asset_root: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<ZonaiGenerationReport> {
        let mut shared = SharedFiles::new(clean_romfs, output_romfs, zstd);
        let report = self.generate_files_with(&mut shared, asset_root)?;
        shared.flush()?;
        Ok(report)
    }

    /// The device pack, the capsule pack, the companion packs and the model
    /// are written now; messages, RSDB rows, GameData flags and vendor packs
    /// are edited in `shared` and written when the caller flushes it.
    pub fn generate_files_with(
        &self,
        shared: &mut SharedFiles<'_>,
        asset_root: &Path,
    ) -> io::Result<ZonaiGenerationReport> {
        self.validate(asset_root)?;
        let clean_romfs = shared.clean_romfs().to_path_buf();
        let output_romfs = shared.output_romfs().to_path_buf();
        let zstd = shared.zstd();
        let clean_romfs = clean_romfs.as_path();
        let output_romfs = output_romfs.as_path();
        assets::ensure_output_outside_romfs(clean_romfs, output_romfs)?;
        let mut stopwatch = super::Stopwatch::new("zonai");
        let template = TemplateZonai::load(clean_romfs, &self.template_actor, zstd.clone())?;
        let capsule_name = self.capsule_name_for(&template.capsule);
        validate_actor_name(&capsule_name)?;
        if capsule_name == template.capsule || capsule_name == self.actor_name {
            return Err(invalid(format!(
                "capsule name {capsule_name} must differ from the template capsule and the device"
            )));
        }
        stopwatch.lap("template");

        // Companions first: the device's references are swapped to them.
        let mut companions = Vec::with_capacity(self.companions.len());
        let mut swaps: Vec<(String, String)> = Vec::new();
        for companion in &self.companions {
            let name = companion.actor_name.trim();
            let bytes = read_vanilla_pack(clean_romfs, &companion.template_actor)?;
            let cloned = clone_actor_pack(
                &bytes,
                &companion.template_actor,
                name,
                &companion.params,
                chemical_donor(companion.chemical.as_deref()),
                None,
                &[],
                None,
                None,
                clean_romfs,
                zstd.clone(),
            )?;
            let actor_pack = write_pack(output_romfs, name, &cloned.bytes)?;
            let actor_overrides = actor_info_overrides(
                shared,
                &companion.template_actor,
                name,
                &cloned.documents,
                chemical_donor(companion.chemical.as_deref()),
            )?;
            let rsdb = clone_rsdb_rows(
                shared,
                &companion.template_actor,
                name,
                &actor_overrides,
                &BTreeMap::new(),
                &game_actor_overrides(&cloned.documents),
                &BTreeMap::new(),
            )?;
            swaps.push((companion.template_actor.clone(), name.to_owned()));
            companions.push(CompanionReport {
                actor_name: name.to_owned(),
                template_actor: companion.template_actor.clone(),
                actor_pack,
                private_files: cloned.private_files,
                chemical_ref: cloned.chemical_ref,
                rsdb,
            });
            stopwatch.lap(&format!("companion {name}"));
        }

        // A collision OBJ is decomposed once, before the pack is cloned.
        let collision = match &self.assets.physics_obj {
            Some(path) => Some(collision::ConvexCollision::from_obj(
                &super::resolve_asset(asset_root, path),
                self.assets.physics_material.as_deref(),
            )?),
            None => None,
        };
        stopwatch.lap("collision");

        // The device always gets a model of its own, like a weapon does.
        let model_fmdb = Some(self.actor_name.clone());
        // The device names its own capsule too (AttachmentParam
        // `PushToPouchSpecialParts`, what Ultrahand returns to the pouch).
        swaps.push((template.capsule.clone(), capsule_name.clone()));
        let device = clone_actor_pack(
            &template.device_pack,
            &self.template_actor,
            &self.actor_name,
            &self.params,
            chemical_donor(self.chemical.as_deref()),
            self.autobuild_cost,
            &swaps,
            model_fmdb.as_deref(),
            collision.as_ref(),
            clean_romfs,
            zstd.clone(),
        )?;
        let actor_pack = write_pack(output_romfs, &self.actor_name, &device.bytes)?;
        stopwatch.lap("device pack");

        let capsule_swaps = [(self.template_actor.clone(), self.actor_name.clone())];
        let capsule = clone_actor_pack(
            &template.capsule_pack,
            &template.capsule,
            &capsule_name,
            &BTreeMap::new(),
            None,
            None,
            &capsule_swaps,
            None,
            None,
            clean_romfs,
            zstd.clone(),
        )?;
        let capsule_pack = write_pack(output_romfs, &capsule_name, &capsule.bytes)?;
        stopwatch.lap("capsule pack");

        let (model, textures) = {
            let generated =
                self.generate_model(clean_romfs, output_romfs, asset_root, zstd.clone())?;
            (Some(generated.0), generated.1)
        };
        stopwatch.lap("model");

        let mut ui_textures = Vec::new();
        for (template_icon, new_icon, png) in [
            (
                template.capsule.as_str(),
                capsule_name.as_str(),
                self.assets.icon_png.as_deref(),
            ),
            (
                self.template_actor.as_str(),
                self.actor_name.as_str(),
                self.assets.device_icon_png.as_deref(),
            ),
        ] {
            let source = format!("UI/Tex/Icon/{template_icon}.bntx.zs");
            if !clean_romfs.join(&source).is_file() {
                continue;
            }
            ui_textures.push(super::generate_ui_texture(
                clean_romfs,
                output_romfs,
                source,
                format!("UI/Tex/Icon/{new_icon}.bntx.zs"),
                new_icon.to_owned(),
                png.map(|path| super::resolve_asset(asset_root, path)),
                zstd.clone(),
            )?);
        }
        stopwatch.lap("icons");

        let messages = messages::apply_pouch_labels(
            shared,
            &capsule_name,
            self.display_name.trim(),
            self.description.trim(),
        )?;
        let adjective = self
            .adjective
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| self.display_name.trim());
        messages::apply_attachment_labels(
            shared,
            &self.actor_name,
            self.display_name.trim(),
            adjective,
        )?;
        stopwatch.lap("messages");

        let mut actor_overrides: BTreeMap<String, JsonValue> = BTreeMap::new();
        if let Some(fmdb) = &model_fmdb {
            actor_overrides.insert("FmdbName".into(), JsonValue::String(fmdb.clone()));
        }
        actor_overrides.extend(actor_info_overrides(
            shared,
            &self.template_actor,
            &self.actor_name,
            &device.documents,
            chemical_donor(self.chemical.as_deref()),
        )?);
        // The device's special-parts registration returns it to its own
        // capsule (the pack's AttachmentParam names the same one).
        let mut attachment_overrides: BTreeMap<String, JsonValue> = BTreeMap::new();
        attachment_overrides.insert(
            "PushToPouchSpecialParts".into(),
            JsonValue::String(actor_param_work_path(&capsule_name)),
        );
        let mut rsdb = clone_rsdb_rows(
            shared,
            &self.template_actor,
            &self.actor_name,
            &actor_overrides,
            &attachment_overrides,
            &game_actor_overrides(&device.documents),
            &BTreeMap::new(),
        )?;
        let mut pouch: BTreeMap<String, JsonValue> = BTreeMap::new();
        pouch.insert(
            "CapsuleContent".into(),
            JsonValue::String(actor_param_work_path(&self.actor_name)),
        );
        if let Some(value) = self.vendors.first().and_then(|vendor| vendor.buying_price) {
            pouch.insert("BuyingPrice".into(), value.into());
        }
        if let Some(value) = self.vendors.first().and_then(|vendor| vendor.selling_price) {
            pouch.insert("SellingPrice".into(), value.into());
        }
        rsdb.extend(clone_rsdb_rows(
            shared,
            &template.capsule,
            &capsule_name,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &pouch,
        )?);
        rsdb.sort();
        rsdb.dedup();
        stopwatch.lap("rsdb");

        let game_data = gamedata::WeaponGameDataRequest {
            actor_name: capsule_name.clone(),
            picture_book: false,
            inventory_flags: true,
        }
        .apply(shared)?;
        let mut vendor_packs = Vec::new();
        for target in &self.vendors {
            vendor_packs.extend(vendor::apply_weapon(shared, &capsule_name, target)?);
        }
        stopwatch.lap("game data + vendors");

        Ok(ZonaiGenerationReport {
            actor_name: self.actor_name.clone(),
            capsule_name,
            template_actor: self.template_actor.clone(),
            template_capsule: template.capsule,
            actor_pack,
            capsule_pack,
            private_files: device.private_files,
            chemical_ref: device.chemical_ref,
            autobuild_param: device.autobuild_param,
            companions,
            collision: device.collision,
            model,
            textures,
            ui_textures,
            messages,
            rsdb,
            game_data,
            vendor_packs,
        })
    }

    /// `Model/<project>.<actor>.bfres.mc`: the custom BFRES (or the template
    /// model) with the FBX geometry imported when one is given, its model
    /// renamed after the actor. Shape, material and bone names stay as they
    /// are so the project's animation set keeps addressing them; textures
    /// are shared by name with the project (only missing ones get the
    /// placeholder).
    fn generate_model(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        asset_root: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<(PathBuf, Vec<PathBuf>)> {
        let (vanilla_source, project) =
            assets::resolve_vanilla_model_source(clean_romfs, &self.template_actor, zstd.clone())?;
        let source = match &self.assets.model {
            Some(path) => super::resolve_asset(asset_root, path),
            None => vanilla_source,
        };
        let source_bytes = fs::read(&source).map_err(|error| {
            io::Error::new(error.kind(), format!("{}: {error}", source.display()))
        })?;
        let raw = if crate::Settings::Magic::is_bfres(&source_bytes) {
            source_bytes
        } else if crate::Settings::Magic::is_mcpk(&source_bytes) {
            zstd.decompress_mcpk(&source_bytes).map_err(|error| {
                invalid_data(format!(
                    "failed to MCPK-decompress {}: {error}",
                    source.display()
                ))
            })?
        } else {
            zstd.try_decompress_for_path(&source, &source_bytes)
                .map(|(data, _)| data)
                .map_err(|error| {
                    invalid_data(format!(
                        "failed to decompress {}: {error}",
                        source.display()
                    ))
                })?
        };
        let external = assets::external_strings_for(clean_romfs, &raw)?;
        let mut file = ResFile::load(&raw, &external)
            .map_err(|error| invalid_data(format!("failed to load the model BFRES: {error}")))?;
        if file.model_count() == 0 {
            return Err(invalid_data("the model BFRES contains no model"));
        }
        if let Some(fbx) = &self.assets.fbx {
            let bytes = fs::read(super::resolve_asset(asset_root, fbx))?;
            file.import_model_like_toolbox(&bytes, self.assets.replace_bones)
                .map_err(|error| invalid_data(format!("failed to import the FBX: {error}")))?;
        }
        let container_name = format!("{project}.{}", self.actor_name);
        file.set_internal_name(&container_name);
        file.rename_first_model(&self.actor_name)
            .map_err(|error| invalid_data(format!("failed to rename the BFRES model: {error}")))?;
        let required = assets::model_texture_names(&file);
        let renamed = file
            .save_like_toolbox()
            .map_err(|error| invalid_data(format!("failed to serialize BFRES: {error}")))?;
        let verified = BfresFile::from_bytes(&renamed)
            .map_err(|error| invalid_data(format!("failed to reopen generated BFRES: {error}")))?;
        assets::validate_bfres_geometry(&verified)?;
        let texture_sources = assets::index_textures(&clean_romfs.join("TexToGo"))?;
        let texture_output = output_romfs.join("TexToGo");
        fs::create_dir_all(&texture_output)?;
        let mut copied = Vec::new();
        assets::ensure_material_textures(
            &texture_output,
            &required,
            &assets::placeholder_texture_path()?,
            &mut copied,
            &texture_sources,
        )?;
        let destination = output_romfs
            .join("Model")
            .join(format!("{container_name}.bfres.mc"));
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let compressed = MeshCodec::compress(&renamed)
            .map_err(|error| invalid_data(format!("failed to MCPK-compress BFRES: {error}")))?;
        let roundtrip = MeshCodec::decompress(&compressed).map_err(|error| {
            invalid_data(format!("generated MCPK does not decompress: {error}"))
        })?;
        BfresFile::from_bytes(&roundtrip).map_err(|error| {
            invalid_data(format!("round-tripped BFRES cannot be parsed: {error}"))
        })?;
        fs::write(&destination, compressed)?;
        Ok((destination, copied))
    }
}

/// The capsule name a device gets by default; see [`ZonaiSpec::capsule_name_for`].
pub fn default_capsule_name(
    template_actor: &str,
    template_capsule: &str,
    actor_name: &str,
) -> String {
    let template_number = trailing_number(template_actor);
    let new_number = trailing_number(actor_name);
    if let (Some(from), Some(to)) = (template_number, new_number) {
        let template_stem = &template_actor[..template_actor.len() - from.len()];
        let new_stem = &actor_name[..actor_name.len() - to.len()];
        if template_stem == new_stem {
            if let Some(capsule_stem) = template_capsule.strip_suffix(from) {
                return format!("{capsule_stem}{to}");
            }
        }
    }
    format!("{actor_name}_Capsule")
}

/// `_<digits>` at the end of a name, with the underscore.
fn trailing_number(name: &str) -> Option<&str> {
    let digits = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if digits.len() == name.len() || !digits.ends_with('_') {
        return None;
    }
    Some(&name[digits.len() - 1..])
}

/// A cloned actor pack.
struct ClonedPack {
    bytes: Vec<u8>,
    /// Final BYML documents by their new entry path.
    documents: BTreeMap<String, Byml>,
    /// Shared-name entries that were edited and renamed after the actor.
    private_files: Vec<String>,
    /// The `ChemicalRef` written for a chemical donor.
    chemical_ref: Option<String>,
    /// The AutoBuilderReplacementParam entry written for an Autobuild cost.
    autobuild_param: Option<String>,
    /// The convex collision written from a `physics_obj`.
    collision: Option<collision::CollisionReport>,
}

/// The trimmed chemical donor of a spec, `None` when blank.
fn chemical_donor(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|donor| !donor.is_empty())
}

/// Clones an actor pack under a new name: takes over the chemistry of
/// `chemical` when given, prices it for Autobuild (`autobuild_cost`),
/// applies `params` (keyed by the template entry paths), swaps actor
/// references, sets the model name, swaps in the convex `collision` (its
/// shape and rigid body become entries of the actor), renames the
/// actor-specific and edited entries and rewrites the references between
/// them (see the module notes).
fn clone_actor_pack(
    pack_bytes: &[u8],
    template_actor: &str,
    new_actor: &str,
    params: &ParamOverrides,
    chemical: Option<&str>,
    autobuild_cost: Option<i32>,
    swaps: &[(String, String)],
    model_fmdb: Option<&str>,
    collision: Option<&collision::ConvexCollision>,
    clean_romfs: &Path,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<ClonedPack> {
    validate_actor_name(new_actor)?;
    let pack = PackFile::from_binary(pack_bytes, zstd.clone())?;
    let mut raw_entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut files: BTreeMap<String, BymlFile<'_>> = BTreeMap::new();
    let mut originals: BTreeMap<String, Byml> = BTreeMap::new();
    for file in pack.sarc.files() {
        let name = file
            .name()
            .ok_or_else(|| invalid_data("template pack contains an unnamed entry"))?;
        if name.ends_with(".bgyml") {
            let document = BymlFile::from_binary(file.data(), zstd.clone(), name)?;
            originals.insert(name.to_owned(), document.pio.clone());
            files.insert(name.to_owned(), document);
        } else {
            raw_entries.push((name.to_owned(), file.data().to_vec()));
        }
    }
    // Chemistry of the donor: its bundle joins the pack as if it were the
    // template's own (unedited, so it keeps its vanilla names), and the
    // ActorParam is pointed at it before the overrides apply.
    let mut injected: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut chemical_ref = None;
    if let Some(donor) = chemical {
        let (reference, entries) =
            actor_pack::prepare_chemical_entries(clean_romfs, donor, zstd.clone())
                .map_err(|error| invalid(format!("{new_actor}: chemical {donor}: {error}")))?;
        for entry in entries {
            if entry.path.ends_with(".bgyml") {
                let document = BymlFile::from_binary(&entry.data, zstd.clone(), &entry.path)?;
                originals.insert(entry.path.clone(), document.pio.clone());
                files.insert(entry.path.clone(), document);
                injected.insert(entry.path, entry.data);
            } else {
                raw_entries.retain(|(path, _)| path != &entry.path);
                raw_entries.push((entry.path, entry.data));
            }
        }
        let actor_file = format!("Actor/{template_actor}.engine__actor__ActorParam.bgyml");
        let document = files
            .get_mut(&actor_file)
            .ok_or_else(|| invalid_data(format!("template pack has no {actor_file}")))?;
        let map = document
            .pio
            .as_mut_map()
            .map_err(|_| invalid_data(format!("{actor_file} is not a map")))?;
        let components = map
            .entry("Components".into())
            .or_insert_with(|| Byml::Map(Default::default()));
        components
            .as_mut_map()
            .map_err(|_| invalid_data(format!("{actor_file}: Components is not a map")))?
            .insert(
                "ChemicalRef".into(),
                Byml::String(reference.as_str().into()),
            );
        chemical_ref = Some(reference);
    }
    // The Autobuild price joins the pack before the overrides and renames:
    // its entry is an edited one, so it is renamed after the actor below.
    let autobuild_entry = match autobuild_cost {
        Some(cost) => Some(
            set_autobuild_cost(
                &mut files,
                &mut originals,
                template_actor,
                cost,
                clean_romfs,
                zstd.clone(),
            )
            .map_err(|error| invalid(format!("{new_actor}: autobuild cost: {error}")))?,
        ),
        None => None,
    };
    // The collision replaces the template shape before the overrides apply,
    // so a parameter edit (gravity, mass) can still touch the rigid body.
    let collision_entries = match collision {
        Some(collision) => Some(
            collision::apply_to_pack(&mut files, &originals, template_actor, collision)
                .map_err(|error| invalid(format!("{new_actor}: collision: {error}")))?,
        ),
        None => None,
    };
    for (path, fields) in params {
        if !files.contains_key(path) {
            return Err(invalid(format!(
                "{template_actor}: the template pack has no entry {path}"
            )));
        }
        let parent = resolve_parent(&originals[path], &originals, clean_romfs, zstd.clone(), 0)?;
        let document = files
            .get_mut(path)
            .ok_or_else(|| invalid_data("entry vanished"))?;
        for (field, value) in fields {
            set_field(&mut document.pio, field, value, parent.as_ref())
                .map_err(|error| invalid(format!("{template_actor}: {path}: {field}: {error}")))?;
        }
    }
    if !swaps.is_empty() {
        let swaps: Vec<(String, String)> = swaps
            .iter()
            .map(|(from, to)| (actor_param_work_path(from), actor_param_work_path(to)))
            .collect();
        for document in files.values_mut() {
            replace_strings(&mut document.pio, &|text| {
                swaps
                    .iter()
                    .find(|(from, _)| from == text)
                    .map(|(_, to)| to.clone())
            });
        }
    }
    if let Some(fmdb) = model_fmdb {
        let actor_file = format!("Actor/{template_actor}.engine__actor__ActorParam.bgyml");
        let actor = files
            .get(&actor_file)
            .ok_or_else(|| invalid_data(format!("template pack has no {actor_file}")))?;
        let model_info = component_reference(&actor.pio, &originals, "ModelInfoRef", 0)?
            .map(|reference| reference_to_internal(&reference))
            .filter(|path| files.contains_key(path))
            .ok_or_else(|| {
                invalid(format!(
                    "{template_actor} has no ModelInfo entry of its own to hold the custom model name"
                ))
            })?;
        let document = files
            .get_mut(&model_info)
            .ok_or_else(|| invalid_data("entry vanished"))?;
        let map = document
            .pio
            .as_mut_map()
            .map_err(|_| invalid_data(format!("{model_info} is not a map")))?;
        map.insert("FmdbName".into(), Byml::String(fmdb.into()));
    }

    // Renames: the ActorParam always, then every edited entry (after the
    // actor when named after the template, else a private name); unchanged
    // entries keep their vanilla names (see the module notes).
    // An in-place edit (the new actor *is* the template) renames nothing:
    // the edited entries overwrite the vanilla ones under their own names.
    let in_place = new_actor == template_actor;
    let mut renames: BTreeMap<String, String> = BTreeMap::new();
    let mut private_files = Vec::new();
    let mut taken: BTreeSet<String> = files.keys().cloned().collect();
    let actor_file = format!("Actor/{template_actor}.engine__actor__ActorParam.bgyml");
    if files.contains_key(&actor_file) && !in_place {
        let new_path = rename_after_actor(&actor_file, template_actor, new_actor);
        taken.insert(new_path.clone());
        renames.insert(actor_file, new_path);
    }
    // A shared rigid body (`<X>_Body_0`) keeps its `_Body_0` tail under the
    // new actor, the way vanilla names them.
    if let Some((from, to)) = collision_entries
        .as_ref()
        .and_then(|entries| entries.body_rename.as_ref())
        .filter(|_| !in_place)
    {
        let to = collision::resolve_body_rename(to, new_actor);
        if !taken.contains(&to) {
            taken.insert(to.clone());
            renames.insert(from.clone(), to);
        }
    }
    while !in_place {
        let mut added = false;
        for (path, document) in &files {
            if renames.contains_key(path) || document.pio == originals[path] {
                continue;
            }
            let after_actor = rename_after_actor(path, template_actor, new_actor);
            let new_path =
                if file_name_of(path).contains(template_actor) && !taken.contains(&after_actor) {
                    after_actor
                } else {
                    private_files.push(path.clone());
                    private_name(path, new_actor, &taken)?
                };
            taken.insert(new_path.clone());
            renames.insert(path.clone(), new_path);
            added = true;
        }
        if !renames.is_empty() {
            let stems: Vec<(String, String)> = renames
                .iter()
                .map(|(from, to)| (reference_stem(from), reference_stem(to)))
                .collect();
            for document in files.values_mut() {
                replace_strings(&mut document.pio, &|text| rewrite_reference(text, &stems));
            }
        }
        if !added {
            break;
        }
    }
    private_files.sort();

    let autobuild_param = autobuild_entry.map(|path| renames.get(&path).cloned().unwrap_or(path));
    let collision = collision
        .zip(collision_entries)
        .map(|(collision, entries)| {
            let final_path = |path: String| renames.get(&path).cloned().unwrap_or(path);
            collision::CollisionReport {
                source: collision.source.clone(),
                shape_param: final_path(entries.shape),
                rigid_body: final_path(entries.body),
                material: collision.material.clone(),
                pieces: collision.pieces.len(),
                max_vertices: collision.max_vertices(),
                volume: collision::round6(collision.mass.volume),
            }
        });
    let mut entries = raw_entries;
    let mut documents = BTreeMap::new();
    for (path, document) in files {
        let new_path = renames.get(&path).cloned().unwrap_or_else(|| path.clone());
        let bytes = if renames.contains_key(&path) || document.pio != originals[&path] {
            document.to_binary_preserving_header()?
        } else if let Some(bytes) = injected.remove(&path) {
            bytes
        } else {
            pack.sarc
                .get_data(&path)
                .ok_or_else(|| invalid_data(format!("entry vanished: {path}")))?
                .to_vec()
        };
        documents.insert(new_path.clone(), document.pio);
        entries.push((new_path, bytes));
    }
    let actor_file = format!("Actor/{new_actor}.engine__actor__ActorParam.bgyml");
    if !documents.contains_key(&actor_file) {
        return Err(invalid_data(format!(
            "cloned pack has no {actor_file}: the template pack names its ActorParam after another actor"
        )));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let bytes = pack.rebuild_binary(entries)?;
    PackFile::from_binary(&bytes, zstd)?;
    Ok(ClonedPack {
        bytes,
        documents,
        private_files,
        chemical_ref,
        autobuild_param,
        collision,
    })
}

/// Sets the Autobuild zonaite price of the actor the way `Obj_AirPlatform`
/// does: the GameParameterTable the ActorParam names gets an
/// `AutoBuilderReplacementParam` component whose entry holds
/// `OverwriteConsumptionNum`. An existing entry is edited; otherwise one is
/// created after the template actor. A table the template references from
/// outside its pack joins the pack first. Both are then edited entries, so
/// the caller's rename pass names them after the new actor. Returns the
/// (pre-rename) path of the AutoBuilderReplacementParam entry.
fn set_autobuild_cost<'a>(
    files: &mut BTreeMap<String, BymlFile<'a>>,
    originals: &mut BTreeMap<String, Byml>,
    template_actor: &str,
    cost: i32,
    clean_romfs: &Path,
    zstd: Arc<TotkZstd<'a>>,
) -> io::Result<String> {
    let actor_file = format!("Actor/{template_actor}.engine__actor__ActorParam.bgyml");
    let actor = files
        .get(&actor_file)
        .ok_or_else(|| invalid_data(format!("template pack has no {actor_file}")))?;
    let table_path = component_reference(&actor.pio, originals, "GameParameterTableRef", 0)?
        .map(|reference| reference_to_internal(&reference))
        .ok_or_else(|| {
            invalid(format!(
                "{template_actor} names no GameParameterTable to hold the Autobuild price"
            ))
        })?;
    // A table (or price entry) referenced from outside the pack is copied
    // in from the RomFS, keeping its vanilla content as the "original".
    let ensure_entry = |files: &mut BTreeMap<String, BymlFile<'a>>,
                        originals: &mut BTreeMap<String, Byml>,
                        path: &str|
     -> io::Result<()> {
        if files.contains_key(path) {
            return Ok(());
        }
        let file = clean_romfs.join(path);
        let bytes = fs::read(&file).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("{path} is neither in the pack nor in the RomFS: {error}"),
            )
        })?;
        let document = BymlFile::from_binary(&bytes, zstd.clone(), path)?;
        originals.insert(path.to_owned(), document.pio.clone());
        files.insert(path.to_owned(), document);
        Ok(())
    };
    ensure_entry(files, originals, &table_path)?;
    let table = files
        .get_mut(&table_path)
        .ok_or_else(|| invalid_data("entry vanished"))?;
    let components = table
        .pio
        .as_mut_map()
        .map_err(|_| invalid_data(format!("{table_path} is not a map")))?
        .entry("Components".into())
        .or_insert_with(|| Byml::Map(Default::default()))
        .as_mut_map()
        .map_err(|_| invalid_data(format!("{table_path}: Components is not a map")))?;
    let existing = components
        .get("AutoBuilderReplacementParam")
        .and_then(|value| value.as_string().ok())
        .map(|reference| reference_to_internal(reference));
    let param_path = match existing {
        Some(path) => {
            ensure_entry(files, originals, &path)?;
            path
        }
        None => {
            let path = format!(
                "GameParameter/AutoBuilderReplacementParam/{template_actor}.game__specialpower__AutoBuilderReplacementParam.bgyml"
            );
            components.insert(
                "AutoBuilderReplacementParam".into(),
                Byml::String(format!("?{path}").as_str().into()),
            );
            // A new document with the table's endian and BYML version; a
            // `Null` original marks it edited for the rename pass.
            let like = table.to_binary_preserving_header()?;
            let mut document = BymlFile::from_binary(&like, zstd.clone(), &path)?;
            document.pio = Byml::Map(Default::default());
            originals.insert(path.clone(), Byml::Null);
            files.insert(path.clone(), document);
            path
        }
    };
    let param = files
        .get_mut(&param_path)
        .ok_or_else(|| invalid_data("entry vanished"))?;
    param
        .pio
        .as_mut_map()
        .map_err(|_| invalid_data(format!("{param_path} is not a map")))?
        .insert("OverwriteConsumptionNum".into(), Byml::I32(cost));
    Ok(param_path)
}

fn write_pack(output_romfs: &Path, actor: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    let output = output_romfs
        .join("Pack/Actor")
        .join(format!("{actor}.pack.zs"));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, bytes)?;
    Ok(output)
}

/// GameActorInfo `MaxLife` follows the pack's single `Life/LifeParameters`
/// entry when it has one (vanilla rows mirror it).
fn game_actor_overrides(documents: &BTreeMap<String, Byml>) -> BTreeMap<String, JsonValue> {
    let mut overrides = BTreeMap::new();
    let life: Vec<&Byml> = documents
        .iter()
        .filter(|(path, _)| path.starts_with("Life/LifeParameters/"))
        .map(|(_, document)| document)
        .collect();
    if let [document] = life.as_slice() {
        if let Some(value) = document
            .as_map()
            .ok()
            .and_then(|map| map.get("MaxLife"))
            .and_then(|value| value.as_i32().ok())
        {
            overrides.insert("MaxLife".into(), value.into());
        }
    }
    overrides
}

/// ActorInfo columns of a clone that must follow its pack: the user names
/// its ELink / SLink parameters declare (the row is where the game reads
/// which effect and sound resources an actor needs, and weapons mirror it
/// the same way), and, with a chemical donor, an instance heap at least as
/// large as the donor's, because the chemistry component needs room the
/// template never reserved and an actor whose heap runs out is silently
/// never created.
fn actor_info_overrides(
    shared: &mut SharedFiles<'_>,
    template_actor: &str,
    new_actor: &str,
    documents: &BTreeMap<String, Byml>,
    chemical: Option<&str>,
) -> io::Result<BTreeMap<String, JsonValue>> {
    let clean_rsdb = shared.clean_romfs().join("RSDB");
    let (_, source) =
        super::version::discover_product_file(&clean_rsdb, "ActorInfo.Product.", ".rstbl.byml.zs")?;
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| invalid_data("ActorInfo filename is not UTF-8"))?
        .to_owned();
    let destination = shared.output_romfs().join("RSDB").join(&file_name);
    let mut overrides = BTreeMap::new();
    let actor_file = format!("Actor/{new_actor}.engine__actor__ActorParam.bgyml");
    let Some(actor) = documents.get(&actor_file) else {
        return Ok(overrides);
    };
    for (column, component) in [("ELinkUserName", "ELinkRef"), ("SLinkUserName", "SLinkRef")] {
        let Some(reference) = component_reference(actor, documents, component, 0)? else {
            continue;
        };
        let user = documents
            .get(&reference_to_internal(&reference))
            .and_then(|document| document.as_map().ok())
            .and_then(|map| map.get("UserName"))
            .and_then(|value| value.as_string().ok())
            .map(|value| value.to_string());
        let Some(user) = user else {
            continue;
        };
        let current = rsdb_row_string(shared, &source, &destination, template_actor, column)?;
        if current.is_some_and(|value| value != user) {
            overrides.insert(column.into(), JsonValue::String(user));
        }
    }
    if let Some(donor) = chemical {
        let template_heap = rsdb_row_int(
            shared,
            &source,
            &destination,
            template_actor,
            "InstanceHeapSize",
        )?;
        let donor_heap = rsdb_row_int(shared, &source, &destination, donor, "InstanceHeapSize")?;
        if let (Some(template_heap), Some(donor_heap)) = (template_heap, donor_heap) {
            if donor_heap > template_heap {
                overrides.insert("InstanceHeapSize".into(), donor_heap.into());
            }
        }
    }
    Ok(overrides)
}

/// ActorInfo, AttachmentActorInfo, GameActorInfo and PouchActorInfo rows
/// plus the Tag entry, each cloned from the template when the template has
/// one. The AttachmentActorInfo row is what registers a device as a special
/// part (its create priority and the capsule it returns to), so a device
/// without one cannot be taken out of the pouch.
fn clone_rsdb_rows(
    shared: &mut SharedFiles<'_>,
    template_actor: &str,
    actor_name: &str,
    actor_overrides: &BTreeMap<String, JsonValue>,
    attachment_overrides: &BTreeMap<String, JsonValue>,
    game_actor_overrides: &BTreeMap<String, JsonValue>,
    pouch_overrides: &BTreeMap<String, JsonValue>,
) -> io::Result<Vec<PathBuf>> {
    let clean_rsdb = shared.clean_romfs().join("RSDB");
    let output_rsdb = shared.output_romfs().join("RSDB");
    fs::create_dir_all(&output_rsdb)?;
    let (version, actor_info_source) =
        super::version::discover_product_file(&clean_rsdb, "ActorInfo.Product.", ".rstbl.byml.zs")?;
    let actor_info = actor_info_source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| invalid_data("ActorInfo filename is not UTF-8"))?
        .to_owned();
    let attachment_actor_info =
        rsdb::WeaponRsdbProcessor::versioned_rsdb_name("AttachmentActorInfo", &version)?;
    let game_actor_info =
        rsdb::WeaponRsdbProcessor::versioned_rsdb_name("GameActorInfo", &version)?;
    let pouch_actor_info =
        rsdb::WeaponRsdbProcessor::versioned_rsdb_name("PouchActorInfo", &version)?;
    let tag_product = rsdb::WeaponRsdbProcessor::versioned_rsdb_name("Tag", &version)?;
    // ActorInfo `ActorName` is the pouch identity of the row (vanilla rows
    // name themselves; live fish and `_B` armor variants redirect to another
    // actor). A self-naming template row must name the clone, or the game
    // would file the clone's pouch item under the template.
    let mut actor_overrides = actor_overrides.clone();
    if !actor_overrides.contains_key("ActorName") {
        let source = clean_rsdb.join(&actor_info);
        let destination = output_rsdb.join(&actor_info);
        if rsdb_row_string(shared, &source, &destination, template_actor, "ActorName")?
            .is_some_and(|value| value == template_actor)
        {
            actor_overrides.insert("ActorName".into(), JsonValue::String(actor_name.to_owned()));
        }
    }
    let mut outputs = Vec::with_capacity(5);
    for (name, overrides) in [
        (actor_info, &actor_overrides),
        (attachment_actor_info, attachment_overrides),
        (game_actor_info, game_actor_overrides),
        (pouch_actor_info, pouch_overrides),
    ] {
        let source = clean_rsdb.join(&name);
        let destination = output_rsdb.join(&name);
        if !rsdb_has_row(shared, &source, &destination, template_actor)? {
            continue;
        }
        rsdb::WeaponRsdbProcessor::clone_rsdb_row(
            shared,
            &source,
            &destination,
            template_actor,
            actor_name,
            overrides,
            &[],
        )?;
        outputs.push(destination);
    }
    let tag_source = clean_rsdb.join(&tag_product);
    let tag_output = output_rsdb.join(&tag_product);
    let template_tag = format!("Work/Actor/|{template_actor}|.engine__actor__ActorParam.gyml");
    if shared
        .tag(&tag_source, &tag_output)?
        .tag
        .actor_tag_data
        .contains_key(&template_tag)
    {
        rsdb::WeaponRsdbProcessor::clone_tag_entry(
            shared,
            &tag_source,
            &tag_output,
            template_actor,
            actor_name,
        )?;
        outputs.push(tag_output);
    }
    Ok(outputs)
}

fn rsdb_has_row(
    shared: &mut SharedFiles<'_>,
    clean_source: &Path,
    destination: &Path,
    actor: &str,
) -> io::Result<bool> {
    rsdb_row_string(shared, clean_source, destination, actor, "__RowId")
        .map(|value| value.is_some())
}

/// String field `key` of `actor`'s row of a shared RSDB table (`None` when
/// the row or the field is missing).
fn rsdb_row_string(
    shared: &mut SharedFiles<'_>,
    clean_source: &Path,
    destination: &Path,
    actor: &str,
    key: &str,
) -> io::Result<Option<String>> {
    Ok(
        rsdb_row_value(shared, clean_source, destination, actor, key)?
            .and_then(|value| value.as_string().ok().map(|text| text.to_string())),
    )
}

/// Integer field `key` of `actor`'s row of a shared RSDB table.
fn rsdb_row_int(
    shared: &mut SharedFiles<'_>,
    clean_source: &Path,
    destination: &Path,
    actor: &str,
    key: &str,
) -> io::Result<Option<i64>> {
    Ok(
        rsdb_row_value(shared, clean_source, destination, actor, key)?.and_then(
            |value| match value {
                Byml::I32(number) => Some(i64::from(number)),
                Byml::U32(number) => Some(i64::from(number)),
                Byml::I64(number) => Some(number),
                Byml::U64(number) => i64::try_from(number).ok(),
                _ => None,
            },
        ),
    )
}

/// Field `key` of `actor`'s row of a shared RSDB table (`None` when the row
/// or the field is missing).
fn rsdb_row_value(
    shared: &mut SharedFiles<'_>,
    clean_source: &Path,
    destination: &Path,
    actor: &str,
    key: &str,
) -> io::Result<Option<Byml>> {
    let document = shared.byml(clean_source, destination)?;
    let rows = document.file.pio.as_array().map_err(|_| {
        invalid_data(format!(
            "RSDB root is not an array: {}",
            clean_source.display()
        ))
    })?;
    Ok(rows.iter().find_map(|row| {
        let map = row.as_map().ok()?;
        let id = map.get("__RowId")?.as_string().ok()?;
        if id.as_str() != actor {
            return None;
        }
        map.get(key).cloned()
    }))
}

// --- BYML helpers ---------------------------------------------------------

/// `Work/Actor/<actor>.engine__actor__ActorParam.gyml`.
fn actor_param_work_path(actor: &str) -> String {
    format!("Work/Actor/{actor}.engine__actor__ActorParam.gyml")
}

/// `Work/Actor/X.engine__actor__ActorParam.gyml` -> `X`; empty otherwise.
fn bare_actor(reference: &str) -> String {
    reference
        .strip_prefix("Work/Actor/")
        .and_then(|rest| rest.strip_suffix(".engine__actor__ActorParam.gyml"))
        .unwrap_or_default()
        .to_owned()
}

fn collect_actor_references(value: &Byml, out: &mut BTreeSet<String>) {
    match value {
        Byml::String(text) => {
            let actor = bare_actor(text);
            if !actor.is_empty() {
                out.insert(actor);
            }
        }
        Byml::Array(items) => items
            .iter()
            .for_each(|item| collect_actor_references(item, out)),
        Byml::Map(map) => map
            .values()
            .for_each(|item| collect_actor_references(item, out)),
        _ => {}
    }
}

/// Applies `edit` to every string of the document.
fn replace_strings(value: &mut Byml, edit: &dyn Fn(&str) -> Option<String>) {
    match value {
        Byml::String(text) => {
            if let Some(replacement) = edit(text.as_str()) {
                *text = replacement.into();
            }
        }
        Byml::Array(items) => items
            .iter_mut()
            .for_each(|item| replace_strings(item, edit)),
        Byml::Map(map) => map
            .iter_mut()
            .for_each(|(_, item)| replace_strings(item, edit)),
        _ => {}
    }
}

/// The last path segment.
fn file_name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `Dir/<Template>Rest.bgyml` -> `Dir/<New>Rest.bgyml` (file name only).
fn rename_after_actor(path: &str, template_actor: &str, new_actor: &str) -> String {
    let (dir, file) = match path.rsplit_once('/') {
        Some((dir, file)) => (Some(dir), file),
        None => (None, path),
    };
    let file = file.replace(template_actor, new_actor);
    match dir {
        Some(dir) => format!("{dir}/{file}"),
        None => file,
    }
}

/// A private name for an edited shared entry: `Dir/<Actor>.<type suffix>`,
/// or `Dir/<Actor>_<original stem>.<type suffix>` when that is taken.
fn private_name(path: &str, new_actor: &str, taken: &BTreeSet<String>) -> io::Result<String> {
    let (dir, file) = match path.rsplit_once('/') {
        Some((dir, file)) => (Some(dir), file),
        None => (None, path),
    };
    let (stem, suffix) = file
        .split_once('.')
        .ok_or_else(|| invalid_data(format!("pack entry has no type suffix: {path}")))?;
    let join = |name: String| match dir {
        Some(dir) => format!("{dir}/{name}.{suffix}"),
        None => format!("{name}.{suffix}"),
    };
    let candidate = join(new_actor.to_owned());
    if !taken.contains(&candidate) {
        return Ok(candidate);
    }
    let candidate = join(format!("{new_actor}_{stem}"));
    if !taken.contains(&candidate) {
        return Ok(candidate);
    }
    Err(invalid_data(format!(
        "no free private name for {path} under {new_actor}"
    )))
}

/// The part of an entry path a reference carries before the extension:
/// `Component/X/Y.game__component__XParam.bgyml` -> `Component/X/Y.game__component__XParam`.
fn reference_stem(path: &str) -> String {
    path.strip_suffix(".bgyml")
        .or_else(|| path.strip_suffix(".byml"))
        .unwrap_or(path)
        .to_owned()
}

/// Rewrites a `?<stem>.bgyml` / `Work/<stem>.gyml` reference of a renamed entry.
fn rewrite_reference(text: &str, stems: &[(String, String)]) -> Option<String> {
    if !text.contains('/') {
        return None;
    }
    for (from, to) in stems {
        let body = text.trim_start_matches('?');
        let body = body.strip_prefix("Work/").unwrap_or(body);
        let Some(rest) = body.strip_prefix(from.as_str()) else {
            continue;
        };
        if !rest.starts_with('.') {
            continue;
        }
        let prefix_len = text.len() - body.len();
        return Some(format!("{}{to}{rest}", &text[..prefix_len]));
    }
    None
}

/// `?Component/X.bgyml` / `Work/Component/X.gyml` -> `Component/X.bgyml`.
pub(super) fn reference_to_internal(reference: &str) -> String {
    let body = reference.trim_start_matches('?');
    let body = body.strip_prefix("Work/").unwrap_or(body);
    match body.strip_suffix(".gyml") {
        Some(stem) => format!("{stem}.bgyml"),
        None => body.to_owned(),
    }
}

pub(super) fn parent_reference(document: &Byml) -> Option<String> {
    document
        .as_map()
        .ok()?
        .get("$parent")?
        .as_string()
        .ok()
        .map(|value| value.to_string())
}

/// The document's `$parent` chain merged into one value (nearest first),
/// resolved inside the pack, else from the RomFS. `None` without a parent.
fn resolve_parent(
    document: &Byml,
    documents: &BTreeMap<String, Byml>,
    clean_romfs: &Path,
    zstd: Arc<TotkZstd<'_>>,
    depth: usize,
) -> io::Result<Option<Byml>> {
    if depth > 8 {
        return Ok(None);
    }
    let Some(reference) = parent_reference(document) else {
        return Ok(None);
    };
    let path = reference_to_internal(&reference);
    let parent = match documents.get(&path) {
        Some(parent) => parent.clone(),
        None => {
            let file = clean_romfs.join(&path);
            if !file.is_file() {
                return Ok(None);
            }
            match BymlFile::new(&file, zstd.clone()) {
                Some(file) => file.pio,
                None => return Ok(None),
            }
        }
    };
    let grand = resolve_parent(&parent, documents, clean_romfs, zstd, depth + 1)?;
    Ok(Some(match grand {
        Some(grand) => merge_over(parent, grand),
        None => parent,
    }))
}

/// `child` over `base`: maps merge key by key, everything else is the child's.
fn merge_over(child: Byml, base: Byml) -> Byml {
    match (child, base) {
        (Byml::Map(child), Byml::Map(mut base)) => {
            for (key, value) in child {
                let merged = match base.get(&key) {
                    Some(existing) => merge_over(value, existing.clone()),
                    None => value,
                };
                base.insert(key, merged);
            }
            Byml::Map(base)
        }
        (child, _) => child,
    }
}

/// Follows `key` of an ActorParam's `Components` through the `$parent` chain.
pub(super) fn component_reference(
    actor: &Byml,
    documents: &BTreeMap<String, Byml>,
    key: &str,
    depth: usize,
) -> io::Result<Option<String>> {
    let map = actor
        .as_map()
        .map_err(|_| invalid_data("ActorParam root is not a map"))?;
    if let Some(value) = map
        .get("Components")
        .and_then(|components| components.as_map().ok())
        .and_then(|components| components.get(key))
        .and_then(|value| value.as_string().ok())
    {
        return Ok(Some(value.to_string()));
    }
    if depth > 8 {
        return Ok(None);
    }
    let Some(parent) = parent_reference(actor) else {
        return Ok(None);
    };
    match documents.get(&reference_to_internal(&parent)) {
        Some(parent) => component_reference(parent, documents, key, depth + 1),
        None => Ok(None),
    }
}

fn scalar_kind(value: &Byml) -> Option<&'static str> {
    Some(match value {
        Byml::String(_) => "string",
        Byml::Bool(_) => "bool",
        Byml::I32(_) | Byml::I64(_) => "int",
        Byml::U32(_) | Byml::U64(_) => "uint",
        Byml::Float(_) | Byml::Double(_) => "float",
        _ => return None,
    })
}

fn scalar_json(value: &Byml) -> JsonValue {
    match value {
        Byml::String(text) => JsonValue::String(text.to_string()),
        Byml::Bool(flag) => JsonValue::Bool(*flag),
        Byml::I32(number) => (*number).into(),
        Byml::I64(number) => (*number).into(),
        Byml::U32(number) => (*number).into(),
        Byml::U64(number) => (*number).into(),
        Byml::Float(number) => serde_json::Number::from_f64(f64::from(*number))
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        Byml::Double(number) => serde_json::Number::from_f64(*number)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        _ => JsonValue::Null,
    }
}

fn flatten(value: &Byml, prefix: &str, inherited: bool, out: &mut Vec<ParamField>) {
    match value {
        Byml::Map(map) => {
            for (key, item) in map.iter() {
                let path = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(item, &path, inherited, out);
            }
        }
        Byml::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                flatten(item, &format!("{prefix}[{index}]"), inherited, out);
            }
        }
        other => {
            if let Some(kind) = scalar_kind(other) {
                if !prefix.is_empty() {
                    out.push(ParamField {
                        path: prefix.to_owned(),
                        kind,
                        value: scalar_json(other),
                        inherited,
                    });
                }
            }
        }
    }
}

enum Segment {
    Key(String),
    Index(usize),
}

/// Sort key of a field path: map keys alphabetically, array indices numerically.
fn path_sort_key(path: &str) -> Vec<(u8, String, usize)> {
    parse_path(path)
        .unwrap_or_default()
        .into_iter()
        .map(|segment| match segment {
            Segment::Key(key) => (0, key, 0),
            Segment::Index(index) => (1, String::new(), index),
        })
        .collect()
}

/// `A.B[2].C` -> Key(A), Key(B), Index(2), Key(C).
fn parse_path(path: &str) -> io::Result<Vec<Segment>> {
    let mut segments = Vec::new();
    for part in path.split('.') {
        if part.is_empty() {
            return Err(invalid("empty path segment"));
        }
        let (key, indices) = match part.find('[') {
            Some(position) => (&part[..position], &part[position..]),
            None => (part, ""),
        };
        if !key.is_empty() {
            segments.push(Segment::Key(key.to_owned()));
        }
        let mut rest = indices;
        while let Some(stripped) = rest.strip_prefix('[') {
            let (number, tail) = stripped
                .split_once(']')
                .ok_or_else(|| invalid(format!("unclosed index in {path}")))?;
            let index: usize = number
                .parse()
                .map_err(|_| invalid(format!("invalid index {number} in {path}")))?;
            segments.push(Segment::Index(index));
            rest = tail;
        }
        if !rest.is_empty() {
            return Err(invalid(format!("invalid path segment {part}")));
        }
    }
    if segments.is_empty() {
        return Err(invalid("empty parameter path"));
    }
    Ok(segments)
}

fn walk<'a>(value: &'a Byml, segment: &Segment) -> Option<&'a Byml> {
    match (value, segment) {
        (Byml::Map(map), Segment::Key(key)) => map.get(key.as_str()),
        (Byml::Array(items), Segment::Index(index)) => items.get(*index),
        _ => None,
    }
}

/// Sets one scalar of `root`, converting `value` to the type the field has.
/// A field the document only inherits is copied in from `fallback` (the
/// resolved parent) together with the map that holds it, so the document
/// keeps its siblings whichever way the game merges `$parent` maps.
fn set_field(
    root: &mut Byml,
    path: &str,
    value: &JsonValue,
    fallback: Option<&Byml>,
) -> io::Result<()> {
    let segments = parse_path(path)?;
    let mut current = root;
    let mut fallback = fallback;
    let last = segments.len() - 1;
    for (position, segment) in segments.iter().enumerate() {
        let fallback_child = fallback.and_then(|value| walk(value, segment));
        let present = walk(current, segment).is_some();
        if !present {
            let copied = fallback_child
                .cloned()
                .ok_or_else(|| invalid(format!("unknown parameter {path}")))?;
            match (&mut *current, segment) {
                (Byml::Map(map), Segment::Key(key)) => {
                    map.insert(key.as_str().into(), copied);
                }
                _ => return Err(invalid(format!("unknown parameter {path}"))),
            }
        }
        let next = match (current, segment) {
            (Byml::Map(map), Segment::Key(key)) => map.get_mut(key.as_str()),
            (Byml::Array(items), Segment::Index(index)) => items.get_mut(*index),
            _ => None,
        }
        .ok_or_else(|| invalid(format!("unknown parameter {path}")))?;
        if position == last {
            *next = coerce(value, next)?;
            return Ok(());
        }
        current = next;
        fallback = fallback_child;
    }
    Ok(())
}

/// `value` as the BYML type of `current`.
fn coerce(value: &JsonValue, current: &Byml) -> io::Result<Byml> {
    let number = || -> io::Result<f64> {
        match value {
            JsonValue::Number(number) => number
                .as_f64()
                .ok_or_else(|| invalid("number out of range")),
            JsonValue::String(text) => text
                .trim()
                .parse::<f64>()
                .map_err(|_| invalid(format!("{text:?} is not a number"))),
            JsonValue::Bool(flag) => Ok(if *flag { 1.0 } else { 0.0 }),
            _ => Err(invalid("a number is required")),
        }
    };
    let integer = |min: i128, max: i128| -> io::Result<i128> {
        let raw = match value {
            JsonValue::Number(number) => number
                .as_i64()
                .map(i128::from)
                .or_else(|| number.as_u64().map(i128::from))
                .or_else(|| {
                    number
                        .as_f64()
                        .filter(|v| v.fract() == 0.0)
                        .map(|v| v as i128)
                })
                .ok_or_else(|| invalid("an integer is required"))?,
            JsonValue::String(text) => text
                .trim()
                .parse::<i128>()
                .map_err(|_| invalid(format!("{text:?} is not an integer")))?,
            JsonValue::Bool(flag) => i128::from(*flag),
            _ => return Err(invalid("an integer is required")),
        };
        if raw < min || raw > max {
            return Err(invalid(format!("{raw} is out of range")));
        }
        Ok(raw)
    };
    Ok(match current {
        Byml::String(_) => Byml::String(
            match value {
                JsonValue::String(text) => text.clone(),
                JsonValue::Null => String::new(),
                JsonValue::Array(_) | JsonValue::Object(_) => {
                    return Err(invalid("a text value is required"))
                }
                other => other.to_string(),
            }
            .into(),
        ),
        Byml::Bool(_) => Byml::Bool(match value {
            JsonValue::Bool(flag) => *flag,
            JsonValue::String(text) => match text.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                other => return Err(invalid(format!("{other:?} is not a boolean"))),
            },
            JsonValue::Number(number) => number.as_f64().is_some_and(|v| v != 0.0),
            _ => return Err(invalid("a boolean is required")),
        }),
        Byml::I32(_) => Byml::I32(integer(i32::MIN.into(), i32::MAX.into())? as i32),
        Byml::U32(_) => Byml::U32(integer(0, u32::MAX.into())? as u32),
        Byml::I64(_) => Byml::I64(integer(i64::MIN.into(), i64::MAX.into())? as i64),
        Byml::U64(_) => Byml::U64(integer(0, u64::MAX.into())? as u64),
        Byml::Float(_) => Byml::Float(number()? as f32),
        Byml::Double(_) => Byml::Double(number()?),
        Byml::Array(items) => {
            // A list of scalars: every element takes the type the list
            // already holds, or the JSON type when the list is empty.
            let JsonValue::Array(values) = value else {
                return Err(invalid("a list is required"));
            };
            let mut out: Vec<Byml> = Vec::with_capacity(values.len());
            for element in values {
                let sample = items
                    .first()
                    .or_else(|| out.first())
                    .cloned()
                    .unwrap_or_else(|| scalar_template(element));
                if scalar_kind(&sample).is_none() {
                    return Err(invalid("only lists of scalars can be set"));
                }
                out.push(coerce(element, &sample)?);
            }
            Byml::Array(out)
        }
        _ => return Err(invalid("only scalar parameters can be set")),
    })
}

/// The BYML scalar a JSON value maps to when nothing dictates the type.
fn scalar_template(value: &JsonValue) -> Byml {
    match value {
        JsonValue::Bool(_) => Byml::Bool(false),
        JsonValue::Number(number) if number.is_i64() || number.is_u64() => {
            if number.as_i64().is_some_and(|v| i32::try_from(v).is_ok()) {
                Byml::I32(0)
            } else {
                Byml::I64(0)
            }
        }
        JsonValue::Number(_) => Byml::Float(0.0),
        _ => Byml::String("".into()),
    }
}

fn read_vanilla_pack(clean_romfs: &Path, actor: &str) -> io::Result<Vec<u8>> {
    validate_actor_name(actor)?;
    let source = clean_romfs
        .join("Pack/Actor")
        .join(format!("{actor}.pack.zs"));
    fs::read(&source).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("vanilla actor pack {}: {error}", source.display()),
        )
    })
}

fn validate_actor_name(actor: &str) -> io::Result<()> {
    if actor.is_empty()
        || !actor
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(invalid(format!("invalid actor name: {actor:?}")));
    }
    Ok(())
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

    #[test]
    fn default_capsule_follows_the_device_number() {
        assert_eq!(
            default_capsule_name(
                "SpObj_Cannon_A_01",
                "SpObj_Cannon_Capsule_A_01",
                "SpObj_Cannon_A_90"
            ),
            "SpObj_Cannon_Capsule_A_90"
        );
        assert_eq!(
            default_capsule_name(
                "SpObj_ElectricBoxGenerator",
                "SpObj_ElectricBoxGenerator_Capsule_A_01",
                "SpObj_ElectricBoxGenerator_90"
            ),
            "SpObj_ElectricBoxGenerator_90_Capsule"
        );
        assert_eq!(
            default_capsule_name("SpObj_Cannon_A_01", "SpObj_Cannon_Capsule_A_01", "Minigun"),
            "Minigun_Capsule"
        );
    }

    #[test]
    fn references_are_rewritten_for_renamed_entries_only() {
        let stems = vec![(
            "Component/ShooterParam/SpObj_Cannon_A_01.game__component__ShooterParam".to_owned(),
            "Component/ShooterParam/SpObj_Cannon_A_90.game__component__ShooterParam".to_owned(),
        )];
        assert_eq!(
            rewrite_reference(
                "?Component/ShooterParam/SpObj_Cannon_A_01.game__component__ShooterParam.bgyml",
                &stems
            )
            .as_deref(),
            Some("?Component/ShooterParam/SpObj_Cannon_A_90.game__component__ShooterParam.bgyml")
        );
        assert_eq!(
            rewrite_reference(
                "Work/Component/ShooterParam/SpObj_Cannon_A_01.game__component__ShooterParam.gyml",
                &stems
            )
            .as_deref(),
            Some(
                "Work/Component/ShooterParam/SpObj_Cannon_A_90.game__component__ShooterParam.gyml"
            )
        );
        assert_eq!(
            rewrite_reference("?AS/SpObj_Cannon_A_01.root.asb", &stems),
            None
        );
        assert_eq!(rewrite_reference("SpObj_Cannon_A_01", &stems), None);
    }

    #[test]
    fn private_names_avoid_taken_entries() {
        let mut taken = BTreeSet::new();
        taken.insert("Component/X/SpObj_Cannon_A_90.game__component__XParam.bgyml".to_owned());
        assert_eq!(
            private_name(
                "Component/X/Common.game__component__XParam.bgyml",
                "SpObj_Cannon_A_90",
                &taken
            )
            .unwrap(),
            "Component/X/SpObj_Cannon_A_90_Common.game__component__XParam.bgyml"
        );
        assert_eq!(
            private_name(
                "Life/LifeParameters/SpObj_Weapon_A.game__life__LifeParameters.bgyml",
                "SpObj_Cannon_A_90",
                &taken
            )
            .unwrap(),
            "Life/LifeParameters/SpObj_Cannon_A_90.game__life__LifeParameters.bgyml"
        );
    }

    #[test]
    fn fields_are_set_with_the_existing_type_and_inherited_ones_copied_in() {
        let mut child = Byml::from_text(
            "$parent: Work/X.gyml\nChargeTime: 2.5\nInitVel: {X: 0.0, Y: 0.0, Z: 80.0}\n",
        )
        .unwrap();
        let parent = Byml::from_text("EnergyConsumptionRate: 6.0\nCannonParam: {Gravity: 25.0, StraightDistance: 30.0}\nMaxActors: 8\n").unwrap();
        set_field(
            &mut child,
            "ChargeTime",
            &serde_json::json!(0.06),
            Some(&parent),
        )
        .unwrap();
        set_field(
            &mut child,
            "InitVel.Z",
            &serde_json::json!("120"),
            Some(&parent),
        )
        .unwrap();
        set_field(
            &mut child,
            "CannonParam.Gravity",
            &serde_json::json!(2),
            Some(&parent),
        )
        .unwrap();
        set_field(
            &mut child,
            "MaxActors",
            &serde_json::json!(200),
            Some(&parent),
        )
        .unwrap();
        let map = child.as_map().unwrap();
        assert_eq!(map.get("ChargeTime").unwrap().as_float().unwrap(), 0.06);
        assert_eq!(
            map.get("InitVel")
                .unwrap()
                .as_map()
                .unwrap()
                .get("Z")
                .unwrap()
                .as_float()
                .unwrap(),
            120.0
        );
        let cannon = map.get("CannonParam").unwrap().as_map().unwrap();
        assert_eq!(cannon.get("Gravity").unwrap().as_float().unwrap(), 2.0);
        // The sibling of the inherited map came along.
        assert_eq!(
            cannon.get("StraightDistance").unwrap().as_float().unwrap(),
            30.0
        );
        assert_eq!(map.get("MaxActors").unwrap().as_i32().unwrap(), 200);
        assert!(set_field(&mut child, "Nope.X", &serde_json::json!(1), Some(&parent)).is_err());
        assert!(set_field(
            &mut child,
            "MaxActors",
            &serde_json::json!("abc"),
            Some(&parent)
        )
        .is_err());
    }

    #[test]
    fn lists_of_scalars_are_set_whole() {
        let mut document =
            Byml::from_text("AttackParams:\n  - DamageElements: []\n    Ids: [1, 2]\nName: x\n")
                .unwrap();
        set_field(
            &mut document,
            "AttackParams[0].DamageElements",
            &serde_json::json!(["Fire"]),
            None,
        )
        .unwrap();
        set_field(
            &mut document,
            "AttackParams[0].Ids",
            &serde_json::json!(["3", 4.0]),
            None,
        )
        .unwrap();
        let root = document.as_map().unwrap();
        let attack = root["AttackParams"].as_array().unwrap()[0]
            .as_map()
            .unwrap();
        let elements: Vec<String> = attack["DamageElements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_string().unwrap().to_string())
            .collect();
        assert_eq!(elements, vec!["Fire".to_owned()]);
        let ids: Vec<i32> = attack["Ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i32().unwrap())
            .collect();
        assert_eq!(ids, vec![3, 4]);
        assert!(set_field(
            &mut document,
            "AttackParams[0].DamageElements",
            &serde_json::json!("Fire"),
            None
        )
        .is_err());
        assert!(set_field(&mut document, "Name", &serde_json::json!(["a"]), None).is_err());
    }

    #[test]
    fn flatten_lists_scalars_with_kinds() {
        let value = Byml::from_text("A: {B: [1, 2], C: true}\nD: text\nE: !u 3\n").unwrap();
        let mut fields = Vec::new();
        flatten(&value, "", false, &mut fields);
        fields.sort_by(|a, b| path_sort_key(&a.path).cmp(&path_sort_key(&b.path)));
        let paths: Vec<(String, &str)> = fields.iter().map(|f| (f.path.clone(), f.kind)).collect();
        assert_eq!(
            paths,
            vec![
                ("A.B[0]".to_owned(), "int"),
                ("A.B[1]".to_owned(), "int"),
                ("A.C".to_owned(), "bool"),
                ("D".to_owned(), "string"),
                ("E".to_owned(), "uint"),
            ]
        );
    }
}
