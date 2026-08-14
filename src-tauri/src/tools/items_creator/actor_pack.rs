//! Explicit cloning and specialization of a vanilla actor pack.

use crate::{
    file_format::{BinTextFile::BymlFile, Pack::PackFile},
    Zstd::TotkZstd,
};
use roead::{byml::Byml, Endian};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorPackEntryKind {
    /// The path and payload match a known vanilla SARC entry.
    Vanilla,
    /// The internal path exists in vanilla data, but its payload differs.
    Modified,
    /// The internal path is not present in the vanilla SARC lookup.
    Added,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InternalRename {
    pub from: String,
    pub to: String,
}

/// One component in a BYML parameter path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BymlPathComponent {
    Key(String),
    Index(usize),
}

/// Typed values supported by actor parameter edits.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BymlValue {
    String(String),
    Bool(bool),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    Float(f32),
    Double(f64),
    Null,
}

impl BymlValue {
    fn into_byml(self) -> Byml {
        match self {
            Self::String(value) => Byml::String(value.into()),
            Self::Bool(value) => Byml::Bool(value),
            Self::I32(value) => Byml::I32(value),
            Self::U32(value) => Byml::U32(value),
            Self::I64(value) => Byml::I64(value),
            Self::U64(value) => Byml::U64(value),
            Self::Float(value) => Byml::Float(value),
            Self::Double(value) => Byml::Double(value),
            Self::Null => Byml::Null,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BymlParameterEdit {
    /// Internal path after `renames` have been applied.
    pub file: String,
    /// Exact map-key/array-index path to an existing value.
    pub path: Vec<BymlPathComponent>,
    pub value: BymlValue,
    /// Allow creating the final map key (used for overriding an inherited component).
    #[serde(default)]
    pub insert_if_missing: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ActorPackPolicy {
    /// Exact internal paths to rename. Files not listed here retain their vanilla names.
    #[serde(default)]
    pub renames: Vec<InternalRename>,
    /// Typed edits to existing BYML values. Type changes are rejected.
    #[serde(default)]
    pub parameter_edits: Vec<BymlParameterEdit>,
}

/// Common actor-specific values observed in vanilla weapon packs.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WeaponParameterOverrides {
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub base_attack: Option<i32>,
    #[serde(default)]
    pub max_life: Option<i32>,
    #[serde(default)]
    pub additional_damage: Option<i32>,
    #[serde(default)]
    pub shield_bash_damage: Option<i32>,
    /// Component reference such as
    /// `?Component/ChemicalParam/Weapon_Chemical_Wood.game__component__ChemicalParam.bgyml`.
    #[serde(default)]
    pub chemical_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WeaponModelInfo {
    pub fmdb_name: String,
    pub model_project_name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WeaponAttachmentInfo {
    pub additional_damage: Option<i32>,
    pub shield_bash_damage: Option<i32>,
    pub subtypes: Vec<String>,
}

/// Basic editable information loaded by following references from the actor's ActorParam.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WeaponActorInfo {
    pub actor_name: String,
    pub parent: Option<String>,
    pub category: Option<String>,
    pub base_attack: Option<i32>,
    pub durability: Option<i32>,
    pub weapon_type: Option<String>,
    pub weapon_subtypes: Vec<String>,
    pub chemical_ref: Option<String>,
    pub chemical_material: Option<String>,
    pub attachment: WeaponAttachmentInfo,
    pub model: WeaponModelInfo,
    /// Resolved component paths from the ActorParam, retained for review and advanced editing.
    pub component_refs: BTreeMap<String, String>,
}

/// JSON/TOML/form-friendly input for the first generation milestone: one custom actor pack.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WeaponPackRequest {
    #[serde(alias = "name")]
    pub actor_name: String,
    #[serde(alias = "base")]
    pub template_actor: String,
    #[serde(default)]
    pub kind: Option<super::WeaponKind>,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default, alias = "attack")]
    pub base_attack: Option<i32>,
    #[serde(default, alias = "dur")]
    pub durability: Option<i32>,
    #[serde(default)]
    pub chemical_ref: Option<String>,
    /// Existing vanilla actor whose complete chemical bundle should be reused.
    #[serde(default, alias = "chemical_actor")]
    pub chemical: Option<String>,
    #[serde(default)]
    pub attachment_damage: Option<i32>,
    #[serde(default)]
    pub shield_bash_damage: Option<i32>,
    /// Optional sound-link parameter source.
    #[serde(default)]
    pub sound: Option<LinkParameterSource>,
    /// Optional effect-link parameter source.
    #[serde(default)]
    pub effect: Option<LinkParameterSource>,
    /// Existing vanilla actor whose complete physics bundle should be reused.
    #[serde(default, alias = "physics_actor")]
    pub physics: Option<String>,
    /// Actor assigned to the first ShootableActorSettings entry.
    #[serde(default)]
    pub shootable: Option<String>,
    /// Escape hatch for uncommon fields not yet represented above.
    #[serde(default)]
    pub extra_edits: Vec<BymlParameterEdit>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum LinkParameterSource {
    /// Import a standalone SLink/ELink parameter BYML and retarget it to the custom actor.
    File { path: PathBuf },
    /// Reuse the resolved link parameter entry from an existing vanilla actor pack.
    VanillaActor {
        #[serde(alias = "name")]
        actor_name: String,
    },
}

#[derive(Clone, Debug)]
struct InjectedPackEntry {
    path: String,
    data: Vec<u8>,
}

impl WeaponPackRequest {
    pub fn from_json(text: &str) -> io::Result<Self> {
        serde_json::from_str(text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn from_toml(text: &str) -> io::Result<Self> {
        toml::from_str(text).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn load_template(
        &self,
        clean_romfs: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<WeaponActorInfo> {
        validate_actor_name(&self.template_actor)?;
        let source = clean_romfs
            .join("Pack/Actor")
            .join(format!("{}.pack.zs", self.template_actor));
        load_weapon_actor_info(&fs::read(source)?, &self.template_actor, zstd)
    }

    pub fn generate_pack(
        &self,
        clean_romfs: &Path,
        output_pack: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<()> {
        if self.chemical.is_some() && self.chemical_ref.is_some() {
            return Err(invalid("chemical and chemical_ref cannot both be provided"));
        }
        let explicit_kind = self.kind;
        let kind = explicit_kind
            .or_else(|| super::WeaponKind::from_actor_name(&self.template_actor))
            .ok_or_else(|| invalid("kind must be LargeSword, SmallSword, Spear, Shield, or Bow"))?;
        if !self.actor_name.starts_with(kind.actor_prefix()) {
            return Err(invalid(format!(
                "actor {} does not match {kind:?} prefix {}",
                self.actor_name,
                kind.actor_prefix()
            )));
        }
        validate_item_template_category(clean_romfs, &self.template_actor, kind, zstd.clone())?;
        let shield_bash_damage =
            if super::rsdb::template_is_shield(clean_romfs, &self.template_actor, zstd.clone())? {
                None
            } else {
                self.shield_bash_damage
            };
        let overrides = WeaponParameterOverrides {
            model_name: self.model_name.clone(),
            base_attack: self.base_attack,
            max_life: self.durability,
            additional_damage: self.attachment_damage,
            shield_bash_damage,
            chemical_ref: self.chemical_ref.clone(),
        };
        let mut policy = if explicit_kind.is_some() {
            ActorPackPolicy::standard_item_clone(
                &self.template_actor,
                &self.actor_name,
                kind,
                overrides,
            )?
        } else {
            ActorPackPolicy::standard_item_clone_preserving_kind(
                &self.template_actor,
                &self.actor_name,
                kind,
                overrides,
            )?
        };
        policy.parameter_edits.extend(self.extra_edits.clone());
        clone_vanilla_actor_pack_with_links(
            clean_romfs,
            &self.template_actor,
            &self.actor_name,
            output_pack,
            &policy,
            self.sound.as_ref(),
            self.effect.as_ref(),
            self.physics.as_deref(),
            self.chemical.as_deref(),
            self.shootable.as_deref(),
            zstd,
        )
    }
}

/// Clones an actor pack while optionally importing SLink and ELink parameters.
///
/// A file source is renamed and retargeted to `new_actor`. A vanilla source retains the
/// resolved internal entry path and exact bytes from that actor's pack.
#[allow(clippy::too_many_arguments)]
pub fn clone_vanilla_actor_pack_with_links(
    clean_romfs: &Path,
    template_actor: &str,
    new_actor: &str,
    output_pack: &Path,
    policy: &ActorPackPolicy,
    sound: Option<&LinkParameterSource>,
    effect: Option<&LinkParameterSource>,
    physics_actor: Option<&str>,
    chemical_actor: Option<&str>,
    shootable_actor: Option<&str>,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<()> {
    validate_actor_name(new_actor)?;
    let mut policy = policy.clone();
    let actor_file = format!("Actor/{new_actor}.engine__actor__ActorParam.bgyml");
    let template_actor_file = format!("Actor/{template_actor}.engine__actor__ActorParam.bgyml");
    let final_actor_file = policy
        .renames
        .iter()
        .find(|rename| rename.from == template_actor_file)
        .map(|rename| rename.to.as_str())
        .unwrap_or(&template_actor_file);
    if final_actor_file != actor_file {
        return Err(invalid(format!(
            "custom actor name must match the ActorParam filename: expected {actor_file}, policy produces {final_actor_file}"
        )));
    }
    if let Some(shootable_actor) = shootable_actor {
        prepare_shootable_edits(
            clean_romfs,
            template_actor,
            new_actor,
            shootable_actor,
            &actor_file,
            &mut policy,
            zstd.clone(),
        )?;
    }
    let mut injected = Vec::new();
    if let Some(source) = sound {
        let (path, data) = prepare_link_entry(
            clean_romfs,
            source,
            new_actor,
            LinkKind::Sound,
            zstd.clone(),
        )?;
        policy.parameter_edits.push(string_edit_insert(
            &actor_file,
            &["Components", "SLinkRef"],
            format!("?{path}"),
        ));
        injected.push(InjectedPackEntry { path, data });
    }
    if let Some(source) = effect {
        let (path, data) = prepare_link_entry(
            clean_romfs,
            source,
            new_actor,
            LinkKind::Effect,
            zstd.clone(),
        )?;
        policy.parameter_edits.push(string_edit_insert(
            &actor_file,
            &["Components", "ELinkRef"],
            format!("?{path}"),
        ));
        injected.push(InjectedPackEntry { path, data });
    }
    let mut replaced_prefixes = Vec::new();
    if let Some(source_actor) = physics_actor {
        let (physics_ref, entries) =
            prepare_physics_entries(clean_romfs, source_actor, zstd.clone())?;
        policy.parameter_edits.push(string_edit_insert(
            &actor_file,
            &["Components", "PhysicsRef"],
            physics_ref,
        ));
        injected.extend(entries);
        replaced_prefixes.extend(["Phive/", "Component/Physics/"]);
    }
    if let Some(source_actor) = chemical_actor {
        let (chemical_ref, entries) =
            prepare_chemical_entries(clean_romfs, source_actor, zstd.clone())?;
        policy.parameter_edits.push(string_edit_insert(
            &actor_file,
            &["Components", "ChemicalRef"],
            chemical_ref,
        ));
        injected.extend(entries);
        replaced_prefixes.extend(["Chemical/", "Component/ChemicalParam/"]);
    }
    clone_vanilla_actor_pack_with_entries(
        clean_romfs,
        template_actor,
        output_pack,
        &policy,
        &injected,
        &replaced_prefixes,
        zstd.clone(),
    )?;
    validate_saved_actor_name(output_pack, new_actor, zstd)
}

pub(super) fn validate_weapon_template_category(
    clean_romfs: &Path,
    template_actor: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<()> {
    let kind = super::WeaponKind::from_actor_name(template_actor)
        .ok_or_else(|| invalid("unsupported template item kind"))?;
    validate_item_template_category(clean_romfs, template_actor, kind, zstd)
}

pub(super) fn validate_item_template_category(
    clean_romfs: &Path,
    template_actor: &str,
    kind: super::WeaponKind,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<()> {
    validate_actor_name(template_actor)?;
    let pack_path = clean_romfs
        .join("Pack/Actor")
        .join(format!("{template_actor}.pack.zs"));
    if !pack_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "vanilla template actor pack is missing: {}",
                pack_path.display()
            ),
        ));
    }
    let pack = PackFile::from_binary(&fs::read(&pack_path)?, zstd)?;
    let actor_path = format!("Actor/{template_actor}.engine__actor__ActorParam.bgyml");
    let actor = parse_pack_byml(&pack, &actor_path)?;
    let category = actor
        .as_map()
        .ok()
        .and_then(|map| map.get("Category"))
        .and_then(|value| value.as_string().ok())
        .ok_or_else(|| {
            invalid(format!(
                "base ActorParam has no string Category: {actor_path}"
            ))
        })?;
    let expected = kind.actor_category();
    if category.as_str() != expected {
        return Err(invalid(format!(
            "base actor {template_actor} has ActorParam Category {category:?}, expected {expected:?} for {kind:?}"
        )));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn prepare_shootable_edits(
    clean_romfs: &Path,
    template_actor: &str,
    new_actor: &str,
    shootable_actor: &str,
    actor_file: &str,
    policy: &mut ActorPackPolicy,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<()> {
    validate_actor_name(shootable_actor)?;
    let pack_path = clean_romfs
        .join("Pack/Actor")
        .join(format!("{template_actor}.pack.zs"));
    if !pack_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "vanilla template actor pack is missing: {}",
                pack_path.display()
            ),
        ));
    }
    let pack = PackFile::from_binary(&fs::read(pack_path)?, zstd)?;
    let template_actor_path = format!("Actor/{template_actor}.engine__actor__ActorParam.bgyml");
    let actor = parse_pack_byml(&pack, &template_actor_path)?;
    let shooter_ref = resolve_component_ref(&pack, &actor, "ShooterRef", &mut BTreeSet::new())?
        .ok_or_else(|| invalid(format!("template actor {template_actor} has no ShooterRef")))?;
    let old_path = reference_to_internal(&shooter_ref);
    if !old_path.starts_with("Component/ShooterParam/") {
        return Err(invalid(format!(
            "template ShooterRef is outside Component/ShooterParam/: {old_path}"
        )));
    }
    let shooter = parse_pack_byml(&pack, &old_path)?;
    shooter
        .as_map()
        .ok()
        .and_then(|map| map.get("ShootableActorSettings"))
        .and_then(|value| value.as_array().ok())
        .and_then(|settings| settings.first())
        .and_then(|setting| setting.as_map().ok())
        .and_then(|setting| setting.get("Actor"))
        .and_then(|actor| actor.as_string().ok())
        .ok_or_else(|| invalid("ShooterParam has no first ShootableActorSettings Actor entry"))?;

    let new_path =
        format!("Component/ShooterParam/{new_actor}.game__component__ShooterParam.bgyml");
    policy.renames.push(InternalRename {
        from: old_path,
        to: new_path.clone(),
    });
    policy.parameter_edits.push(string_edit_insert(
        actor_file,
        &["Components", "ShooterRef"],
        format!("?{new_path}"),
    ));
    policy.parameter_edits.push(BymlParameterEdit {
        file: new_path,
        path: vec![
            BymlPathComponent::Key("ShootableActorSettings".into()),
            BymlPathComponent::Index(0),
            BymlPathComponent::Key("Actor".into()),
        ],
        value: BymlValue::String(format!(
            "Work/Actor/{shootable_actor}.engine__actor__ActorParam.gyml"
        )),
        insert_if_missing: false,
    });
    Ok(())
}

fn prepare_chemical_entries(
    clean_romfs: &Path,
    actor_name: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<(String, Vec<InjectedPackEntry>)> {
    validate_actor_name(actor_name)?;
    let pack_path = clean_romfs
        .join("Pack/Actor")
        .join(format!("{actor_name}.pack.zs"));
    if !pack_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "vanilla chemical-source actor pack is missing: {}",
                pack_path.display()
            ),
        ));
    }
    let pack = PackFile::from_binary(&fs::read(&pack_path)?, zstd)?;
    let actor_path = format!("Actor/{actor_name}.engine__actor__ActorParam.bgyml");
    let actor = parse_pack_byml(&pack, &actor_path)?;
    let chemical_ref =
        resolve_component_ref(&pack, &actor, "ChemicalRef", &mut BTreeSet::new())?
            .ok_or_else(|| invalid(format!("vanilla actor {actor_name} has no ChemicalRef")))?;
    let chemical_path = reference_to_internal(&chemical_ref);
    let mut entries = Vec::new();
    for file in pack.sarc.files() {
        let path = file
            .name()
            .ok_or_else(|| invalid("chemical-source pack contains an unnamed SARC entry"))?;
        if path.starts_with("Chemical/") || path.starts_with("Component/ChemicalParam/") {
            entries.push(InjectedPackEntry {
                path: path.to_owned(),
                data: file.data().to_vec(),
            });
        }
    }
    if entries.is_empty() {
        return Err(invalid(format!(
            "vanilla actor {actor_name} has no Chemical or Component/ChemicalParam entries"
        )));
    }
    if !entries.iter().any(|entry| entry.path == chemical_path) {
        return Err(invalid(format!(
            "ChemicalRef target is not present in {actor_name}'s chemical bundle: {chemical_path}"
        )));
    }
    Ok((chemical_ref, entries))
}

fn validate_saved_actor_name(
    output_pack: &Path,
    new_actor: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<()> {
    let pack = PackFile::from_binary(&fs::read(output_pack)?, zstd)?;
    let expected = format!("Actor/{new_actor}.engine__actor__ActorParam.bgyml");
    if pack.sarc.get_data(&expected).is_none() {
        return Err(invalid(format!(
            "generated pack ActorParam filename does not match custom actor name {new_actor}: expected {expected}"
        )));
    }
    Ok(())
}

fn prepare_physics_entries(
    clean_romfs: &Path,
    actor_name: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<(String, Vec<InjectedPackEntry>)> {
    validate_actor_name(actor_name)?;
    let pack_path = clean_romfs
        .join("Pack/Actor")
        .join(format!("{actor_name}.pack.zs"));
    if !pack_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "vanilla physics-source actor pack is missing: {}",
                pack_path.display()
            ),
        ));
    }
    let pack = PackFile::from_binary(&fs::read(&pack_path)?, zstd)?;
    let actor_path = format!("Actor/{actor_name}.engine__actor__ActorParam.bgyml");
    let actor = parse_pack_byml(&pack, &actor_path)?;
    let physics_ref = resolve_component_ref(&pack, &actor, "PhysicsRef", &mut BTreeSet::new())?
        .ok_or_else(|| invalid(format!("vanilla actor {actor_name} has no PhysicsRef")))?;
    let physics_path = reference_to_internal(&physics_ref);
    let mut entries = Vec::new();
    for file in pack.sarc.files() {
        let path = file
            .name()
            .ok_or_else(|| invalid("physics-source pack contains an unnamed SARC entry"))?;
        if path.starts_with("Phive/") || path.starts_with("Component/Physics/") {
            entries.push(InjectedPackEntry {
                path: path.to_owned(),
                data: file.data().to_vec(),
            });
        }
    }
    if entries.is_empty() {
        return Err(invalid(format!(
            "vanilla actor {actor_name} has no Phive or Component/Physics entries"
        )));
    }
    if !entries.iter().any(|entry| entry.path == physics_path) {
        return Err(invalid(format!(
            "PhysicsRef target is not present in {actor_name}'s physics bundle: {physics_path}"
        )));
    }
    Ok((physics_ref, entries))
}

#[derive(Clone, Copy)]
enum LinkKind {
    Sound,
    Effect,
}

impl LinkKind {
    fn component_key(self) -> &'static str {
        match self {
            Self::Sound => "SLinkRef",
            Self::Effect => "ELinkRef",
        }
    }

    fn directory(self) -> &'static str {
        match self {
            Self::Sound => "SLink",
            Self::Effect => "ELink",
        }
    }

    fn type_name(self) -> &'static str {
        match self {
            Self::Sound => "SLinkParam",
            Self::Effect => "ELinkParam",
        }
    }
}

fn prepare_link_entry(
    clean_romfs: &Path,
    source: &LinkParameterSource,
    new_actor: &str,
    kind: LinkKind,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<(String, Vec<u8>)> {
    match source {
        LinkParameterSource::File { path } => {
            if !path.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("link parameter file is missing: {}", path.display()),
                ));
            }
            let mut document = BymlFile::new(path, zstd.clone())
                .ok_or_else(|| invalid("invalid link parameter BYML"))?;
            let map = document
                .pio
                .as_mut_map()
                .map_err(|_| invalid("link parameter BYML root is not a map"))?;
            match map.get("UserName") {
                Some(Byml::String(_)) => {
                    map.insert("UserName".into(), Byml::String(new_actor.into()));
                }
                Some(_) => return Err(invalid("link parameter UserName is not a string")),
                None => return Err(invalid("link parameter UserName is missing")),
            }
            let rebuilt = document.to_binary_preserving_header()?;
            Ok((
                format!(
                    "Component/{}/{}.engine__component__{}.bgyml",
                    kind.directory(),
                    new_actor,
                    kind.type_name()
                ),
                rebuilt,
            ))
        }
        LinkParameterSource::VanillaActor { actor_name } => {
            validate_actor_name(actor_name)?;
            let pack_path = clean_romfs
                .join("Pack/Actor")
                .join(format!("{actor_name}.pack.zs"));
            if !pack_path.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "vanilla link-source actor pack is missing: {}",
                        pack_path.display()
                    ),
                ));
            }
            let pack = PackFile::from_binary(&fs::read(pack_path)?, zstd)?;
            let actor_path = format!("Actor/{actor_name}.engine__actor__ActorParam.bgyml");
            let actor = parse_pack_byml(&pack, &actor_path)?;
            let reference =
                resolve_component_ref(&pack, &actor, kind.component_key(), &mut BTreeSet::new())?
                    .ok_or_else(|| {
                    invalid(format!(
                        "vanilla actor {actor_name} has no {}",
                        kind.component_key()
                    ))
                })?;
            let path = reference_to_internal(&reference);
            let data = pack
                .sarc
                .get_data(&path)
                .ok_or_else(|| invalid(format!("resolved vanilla link entry is missing: {path}")))?
                .to_vec();
            pack.byml_file(&path)?;
            Ok((path, data))
        }
    }
}

impl ActorPackPolicy {
    /// Builds the six-entry clone policy used by ordinary weapons such as the restored
    /// `Weapon_Lsword_005` (template `Weapon_Lsword_108`). Shared physics, capture,
    /// controller, link, and shape entries intentionally remain unchanged.
    pub fn standard_weapon_clone(
        template_actor: &str,
        new_actor: &str,
        overrides: WeaponParameterOverrides,
    ) -> io::Result<Self> {
        let kind = super::WeaponKind::from_actor_name(new_actor)
            .or_else(|| super::WeaponKind::from_actor_name(template_actor))
            .ok_or_else(|| invalid("unsupported custom item kind"))?;
        Self::standard_item_clone_preserving_kind(template_actor, new_actor, kind, overrides)
    }

    pub fn standard_item_clone(
        template_actor: &str,
        new_actor: &str,
        kind: super::WeaponKind,
        overrides: WeaponParameterOverrides,
    ) -> io::Result<Self> {
        Self::standard_item_clone_impl(template_actor, new_actor, kind, overrides, true)
    }

    pub fn standard_item_clone_preserving_kind(
        template_actor: &str,
        new_actor: &str,
        kind: super::WeaponKind,
        overrides: WeaponParameterOverrides,
    ) -> io::Result<Self> {
        Self::standard_item_clone_impl(template_actor, new_actor, kind, overrides, false)
    }

    fn standard_item_clone_impl(
        template_actor: &str,
        new_actor: &str,
        kind: super::WeaponKind,
        overrides: WeaponParameterOverrides,
        write_kind: bool,
    ) -> io::Result<Self> {
        validate_actor_name(template_actor)?;
        validate_actor_name(new_actor)?;
        if template_actor == new_actor {
            return Err(invalid("template actor and new actor must differ"));
        }
        let template_model = model_info_actor_name(template_actor);
        let new_model_path = model_info_actor_name(new_actor);
        let model_name = overrides.model_name;

        let actor_file = format!("Actor/{new_actor}.engine__actor__ActorParam.bgyml");
        let attachment_file =
            format!("Component/AttachmentParam/{new_actor}.game__component__AttachmentParam.bgyml");
        let life_param_file =
            format!("Component/LifeParam/{new_actor}.game__component__LifeParam.bgyml");
        let model_file =
            format!("Component/ModelInfo/{new_model_path}.engine__component__ModelInfo.bgyml");
        let (parameter_ref, parameter_component) = kind.parameter_component();
        let parameter_file = format!(
            "Component/{parameter_component}/{new_actor}.game__component__{parameter_component}.bgyml"
        );
        let life_file = format!("Life/LifeParameters/{new_actor}.game__life__LifeParameters.bgyml");

        let renames = vec![
            rename(
                format!("Actor/{template_actor}.engine__actor__ActorParam.bgyml"),
                &actor_file,
            ),
            rename(
                format!("Component/AttachmentParam/{template_actor}.game__component__AttachmentParam.bgyml"),
                &attachment_file,
            ),
            rename(
                format!("Component/LifeParam/{template_actor}.game__component__LifeParam.bgyml"),
                &life_param_file,
            ),
            rename(
                format!("Component/ModelInfo/{template_model}.engine__component__ModelInfo.bgyml"),
                &model_file,
            ),
            rename(
                format!("Component/{parameter_component}/{template_actor}.game__component__{parameter_component}.bgyml"),
                &parameter_file,
            ),
            rename(
                format!("Life/LifeParameters/{template_actor}.game__life__LifeParameters.bgyml"),
                &life_file,
            ),
        ];

        let mut parameter_edits = vec![
            string_edit(
                &actor_file,
                &["Components", "AttachmentRef"],
                format!("?{attachment_file}"),
            ),
            string_edit(
                &actor_file,
                &["Components", "LifeRef"],
                format!("?{life_param_file}"),
            ),
            string_edit(
                &actor_file,
                &["Components", "ModelInfoRef"],
                format!("?{model_file}"),
            ),
            string_edit(
                &actor_file,
                &["Components", parameter_ref],
                format!("?{parameter_file}"),
            ),
            string_edit(&actor_file, &["Category"], kind.actor_category().to_owned()),
            string_edit(
                &life_param_file,
                &["LifeParameters"],
                format!("Work/{life_file}").replace(".bgyml", ".gyml"),
            ),
        ];
        if let Some(model_name) = model_name {
            parameter_edits.push(string_edit(&model_file, &["FmdbName"], model_name.clone()));
            parameter_edits.push(string_edit(&model_file, &["ModelProjectName"], model_name));
        }
        if write_kind {
            if let Some(weapon_type) = kind.melee_weapon_type() {
                parameter_edits.push(string_edit(
                    &parameter_file,
                    &["WeaponType"],
                    weapon_type.to_owned(),
                ));
            }
        }
        if let Some(value) = overrides.base_attack {
            let strength_key = if kind == super::WeaponKind::Shield {
                "GuardPower"
            } else {
                "BaseAttack"
            };
            parameter_edits.push(i32_edit(&parameter_file, &[strength_key], value));
        }
        if let Some(value) = overrides.max_life {
            parameter_edits.push(i32_edit(&life_file, &["MaxLife"], value));
        }
        if let Some(value) = overrides.additional_damage {
            parameter_edits.push(i32_edit(&attachment_file, &["AdditionalDamage"], value));
        }
        if let Some(value) = overrides.shield_bash_damage {
            parameter_edits.push(i32_edit(&attachment_file, &["ShieldBashDamage"], value));
        }
        if let Some(value) = overrides.chemical_ref {
            parameter_edits.push(BymlParameterEdit {
                file: actor_file.clone(),
                path: vec![
                    BymlPathComponent::Key("Components".into()),
                    BymlPathComponent::Key("ChemicalRef".into()),
                ],
                value: BymlValue::String(value),
                insert_if_missing: true,
            });
        }
        Ok(Self {
            renames,
            parameter_edits,
        })
    }
}

fn rename(from: String, to: &str) -> InternalRename {
    InternalRename {
        from,
        to: to.to_owned(),
    }
}

fn string_edit(file: &str, path: &[&str], value: String) -> BymlParameterEdit {
    BymlParameterEdit {
        file: file.to_owned(),
        path: path
            .iter()
            .map(|key| BymlPathComponent::Key((*key).to_owned()))
            .collect(),
        value: BymlValue::String(value),
        insert_if_missing: false,
    }
}

fn string_edit_insert(file: &str, path: &[&str], value: String) -> BymlParameterEdit {
    let mut edit = string_edit(file, path, value);
    edit.insert_if_missing = true;
    edit
}

fn i32_edit(file: &str, path: &[&str], value: i32) -> BymlParameterEdit {
    BymlParameterEdit {
        file: file.to_owned(),
        path: path
            .iter()
            .map(|key| BymlPathComponent::Key((*key).to_owned()))
            .collect(),
        value: BymlValue::I32(value),
        insert_if_missing: false,
    }
}

fn model_info_actor_name(actor: &str) -> String {
    actor.replace("_Lsword_", "_LSword_")
}

/// Clone `Pack/Actor/<template_actor>.pack.zs` from clean ROMFS and write a specialized pack.
pub fn clone_vanilla_actor_pack(
    clean_romfs: &Path,
    template_actor: &str,
    output_pack: &Path,
    policy: &ActorPackPolicy,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<()> {
    clone_vanilla_actor_pack_with_entries(
        clean_romfs,
        template_actor,
        output_pack,
        policy,
        &[],
        &[],
        zstd,
    )
}

fn clone_vanilla_actor_pack_with_entries(
    clean_romfs: &Path,
    template_actor: &str,
    output_pack: &Path,
    policy: &ActorPackPolicy,
    injected: &[InjectedPackEntry],
    replaced_prefixes: &[&str],
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<()> {
    validate_actor_name(template_actor)?;
    ensure_output_outside_romfs(clean_romfs, output_pack)?;
    let source = clean_romfs
        .join("Pack/Actor")
        .join(format!("{template_actor}.pack.zs"));
    if !source.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("vanilla actor pack is missing: {}", source.display()),
        ));
    }

    let source_bytes = fs::read(&source)?;
    let output_bytes = specialize_actor_pack_with_entries(
        &source_bytes,
        policy,
        injected,
        replaced_prefixes,
        zstd,
    )?;
    if let Some(parent) = output_pack.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_pack, output_bytes)
}

/// Loads the commonly edited weapon values while retaining the original pack as the source of
/// truth for every field this API does not expose.
pub fn load_weapon_actor_info(
    source_bytes: &[u8],
    actor_name: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<WeaponActorInfo> {
    validate_actor_name(actor_name)?;
    let pack = PackFile::from_binary(source_bytes, zstd)?;
    let actor_path = format!("Actor/{actor_name}.engine__actor__ActorParam.bgyml");
    let actor = parse_pack_byml(&pack, &actor_path)?;
    let actor_map = expect_map(&actor, &actor_path)?;
    let parent = map_string(actor_map, "$parent");
    let category = map_string(actor_map, "Category");
    let direct_components = actor_map.get("Components").and_then(as_map);
    let mut component_refs = BTreeMap::new();
    if let Some(components) = direct_components {
        for (key, value) in components {
            if let Byml::String(value) = value {
                component_refs.insert(key.to_string(), value.to_string());
            }
        }
    }
    for key in [
        "WeaponRef",
        "ShieldRef",
        "BowRef",
        "LifeRef",
        "AttachmentRef",
        "ModelInfoRef",
        "ChemicalRef",
    ] {
        if let Some(value) = resolve_component_ref(&pack, &actor, key, &mut BTreeSet::new())? {
            component_refs.entry(key.into()).or_insert(value);
        }
    }
    let chemical_ref = component_refs.get("ChemicalRef").cloned();

    let parameter_ref = match category.as_deref() {
        Some("Weapon") => "WeaponRef",
        Some("Shield") => "ShieldRef",
        Some("Bow") => "BowRef",
        value => {
            return Err(invalid(format!(
                "unsupported ActorParam Category for custom item: {value:?}"
            )))
        }
    };
    let weapon_path = required_component_path(&component_refs, parameter_ref)?;
    let life_param_path = required_component_path(&component_refs, "LifeRef")?;
    let attachment_path = required_component_path(&component_refs, "AttachmentRef")?;
    let model_path = required_component_path(&component_refs, "ModelInfoRef")?;
    let weapon = parse_pack_byml(&pack, &weapon_path)?;
    let weapon_map = expect_map(&weapon, &weapon_path)?;
    let life_param = parse_pack_byml(&pack, &life_param_path)?;
    let life_param_map = expect_map(&life_param, &life_param_path)?;
    let life_path = map_string(life_param_map, "LifeParameters")
        .ok_or_else(|| invalid(format!("LifeParameters is missing from {life_param_path}")))?;
    let life_path = work_path_to_internal(&life_path);
    let life = parse_pack_byml(&pack, &life_path)?;
    let life_map = expect_map(&life, &life_path)?;
    let attachment = parse_pack_byml(&pack, &attachment_path)?;
    let attachment_map = expect_map(&attachment, &attachment_path)?;
    let model = parse_pack_byml(&pack, &model_path)?;
    let model_map = expect_map(&model, &model_path)?;

    let chemical_material = if let Some(reference) = &chemical_ref {
        let path = reference_to_internal(reference);
        let chemical = parse_pack_byml(&pack, &path)?;
        as_map(&chemical)
            .and_then(|map| map.get("Object"))
            .and_then(|value| match value {
                Byml::Array(objects) => objects.first(),
                _ => None,
            })
            .and_then(as_map)
            .and_then(|object| map_string(object, "ChemicalMaterial"))
    } else {
        None
    };

    let strength = map_i32(
        weapon_map,
        if category.as_deref() == Some("Shield") {
            "GuardPower"
        } else {
            "BaseAttack"
        },
    );
    Ok(WeaponActorInfo {
        actor_name: actor_name.to_owned(),
        parent,
        category,
        base_attack: strength,
        durability: map_i32(life_map, "MaxLife"),
        weapon_type: map_string(weapon_map, "WeaponType"),
        weapon_subtypes: map_string_array(weapon_map, "SubType"),
        chemical_ref,
        chemical_material,
        attachment: WeaponAttachmentInfo {
            additional_damage: map_i32(attachment_map, "AdditionalDamage"),
            shield_bash_damage: map_i32(attachment_map, "ShieldBashDamage"),
            subtypes: map_string_array(attachment_map, "AdditionalSubType"),
        },
        model: WeaponModelInfo {
            fmdb_name: map_string(model_map, "FmdbName")
                .ok_or_else(|| invalid(format!("FmdbName is missing from {model_path}")))?,
            model_project_name: map_string(model_map, "ModelProjectName")
                .ok_or_else(|| invalid(format!("ModelProjectName is missing from {model_path}")))?,
        },
        component_refs,
    })
}

fn parse_pack_byml(pack: &PackFile<'_>, path: &str) -> io::Result<Byml> {
    pack.byml_file(path).map(|file| file.pio)
}

fn expect_map<'a>(value: &'a Byml, path: &str) -> io::Result<&'a roead::byml::Map> {
    as_map(value).ok_or_else(|| invalid(format!("BYML root is not a map: {path}")))
}

fn as_map(value: &Byml) -> Option<&roead::byml::Map> {
    match value {
        Byml::Map(map) => Some(map),
        _ => None,
    }
}

fn map_string(map: &roead::byml::Map, key: &str) -> Option<String> {
    match map.get(key) {
        Some(Byml::String(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn map_i32(map: &roead::byml::Map, key: &str) -> Option<i32> {
    match map.get(key) {
        Some(Byml::I32(value)) => Some(*value),
        _ => None,
    }
}

fn map_string_array(map: &roead::byml::Map, key: &str) -> Vec<String> {
    match map.get(key) {
        Some(Byml::Array(values)) => values
            .iter()
            .filter_map(|value| match value {
                Byml::String(value) => Some(value.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn required_component_path(refs: &BTreeMap<String, String>, key: &str) -> io::Result<String> {
    refs.get(key)
        .map(|value| reference_to_internal(value))
        .ok_or_else(|| invalid(format!("ActorParam component reference is missing: {key}")))
}

fn reference_to_internal(value: &str) -> String {
    value.trim_start_matches('?').replace(".gyml", ".bgyml")
}

fn work_path_to_internal(value: &str) -> String {
    reference_to_internal(value.trim_start_matches("Work/"))
}

fn resolve_component_ref(
    pack: &PackFile<'_>,
    actor: &Byml,
    key: &str,
    visited: &mut BTreeSet<String>,
) -> io::Result<Option<String>> {
    let map = expect_map(actor, "ActorParam")?;
    if let Some(value) = map
        .get("Components")
        .and_then(as_map)
        .and_then(|components| map_string(components, key))
    {
        return Ok(Some(value));
    }
    let Some(parent) = map_string(map, "$parent") else {
        return Ok(None);
    };
    let parent_path = work_path_to_internal(&parent);
    if !visited.insert(parent_path.clone()) {
        return Err(invalid(format!(
            "actor parent cycle detected at {parent_path}"
        )));
    }
    let parent = parse_pack_byml(pack, &parent_path)?;
    resolve_component_ref(pack, &parent, key, visited)
}

/// Pure specialization entry point, also useful to callers that stage files in memory.
pub fn specialize_actor_pack(
    source_bytes: &[u8],
    policy: &ActorPackPolicy,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<Vec<u8>> {
    specialize_actor_pack_with_entries(source_bytes, policy, &[], &[], zstd)
}

fn specialize_actor_pack_with_entries(
    source_bytes: &[u8],
    policy: &ActorPackPolicy,
    injected: &[InjectedPackEntry],
    replaced_prefixes: &[&str],
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<Vec<u8>> {
    validate_policy(policy)?;
    let injected_paths: BTreeSet<&str> = injected.iter().map(|entry| entry.path.as_str()).collect();
    if injected_paths.len() != injected.len() {
        return Err(invalid("duplicate injected actor-pack path"));
    }
    for entry in injected {
        validate_internal_path(&entry.path)?;
        BymlFile::from_binary(&entry.data, zstd.clone(), &entry.path).map_err(|error| {
            invalid(format!("injected BYML {} is invalid: {error}", entry.path))
        })?;
    }
    let pack = PackFile::from_binary(source_bytes, zstd.clone())?;
    let mut edits: BTreeMap<&str, Vec<&BymlParameterEdit>> = BTreeMap::new();
    for edit in &policy.parameter_edits {
        edits.entry(&edit.file).or_default().push(edit);
    }

    let original_paths: BTreeSet<String> = pack
        .sarc
        .files()
        .filter_map(|file| file.name().map(str::to_owned))
        .collect();
    let mut rename_map: BTreeMap<String, &str> = BTreeMap::new();
    for rename in &policy.renames {
        let source = if original_paths.contains(&rename.from) {
            rename.from.clone()
        } else {
            let matches: Vec<_> = original_paths
                .iter()
                .filter(|path| path.eq_ignore_ascii_case(&rename.from))
                .collect();
            if matches.len() != 1 {
                return Err(invalid(format!(
                    "rename source is absent from actor pack: {}",
                    rename.from
                )));
            }
            matches
                .first()
                .ok_or_else(|| {
                    invalid(format!(
                        "rename source is absent from actor pack: {}",
                        rename.from
                    ))
                })?
                .to_string()
        };
        rename_map.insert(source, rename.to.as_str());
    }

    let mut rebuilt_entries = Vec::new();
    let mut final_paths = BTreeSet::new();
    for file in pack.sarc.files() {
        let old_path = file
            .name()
            .ok_or_else(|| invalid("actor pack contains an unnamed SARC entry"))?;
        let final_path = rename_map.get(old_path).copied().unwrap_or(old_path);
        if replaced_prefixes
            .iter()
            .any(|prefix| final_path.starts_with(prefix))
        {
            continue;
        }
        if injected_paths.contains(final_path) {
            continue;
        }
        if !final_paths.insert(final_path.to_owned()) {
            return Err(invalid(format!(
                "internal path collision after rename: {final_path}"
            )));
        }
        let mut data = file.data().to_vec();
        if let Some(file_edits) = edits.remove(final_path) {
            let mut document = BymlFile::from_binary(&data, zstd.clone(), final_path)?;
            for edit in file_edits {
                apply_parameter_edit(&mut document.pio, edit)?;
            }
            data = document.to_binary_preserving_header()?;
        }
        rebuilt_entries.push((final_path.to_owned(), data));
    }
    if let Some((missing, _)) = edits.first_key_value() {
        return Err(invalid(format!(
            "BYML edit file is absent from actor pack: {missing}"
        )));
    }
    for entry in injected {
        if !final_paths.insert(entry.path.clone()) {
            return Err(invalid(format!(
                "internal path collision for injected entry: {}",
                entry.path
            )));
        }
        rebuilt_entries.push((entry.path.clone(), entry.data.clone()));
    }
    pack.rebuild_binary(rebuilt_entries)
}

/// Classifies entries using TotkBits' vanilla internal-path/hash lookup.
pub fn audit_actor_pack(
    source_bytes: &[u8],
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<BTreeMap<String, ActorPackEntryKind>> {
    let pack = PackFile::from_binary(source_bytes, zstd)?;
    let vanilla = crate::utils::LookupData::sarc_sha256();
    let mut result = BTreeMap::new();
    for file in pack.sarc.files() {
        let path = file
            .name()
            .ok_or_else(|| invalid("actor pack contains an unnamed SARC entry"))?;
        let hash = crate::Zstd::sha256(file.data().to_vec());
        let kind = match vanilla.get(path) {
            Some(expected) if expected == &hash => ActorPackEntryKind::Vanilla,
            Some(_) => ActorPackEntryKind::Modified,
            None => ActorPackEntryKind::Added,
        };
        result.insert(path.to_owned(), kind);
    }
    Ok(result)
}

fn apply_parameter_edit(document: &mut Byml, edit: &BymlParameterEdit) -> io::Result<()> {
    if edit.path.is_empty() {
        return Err(invalid(format!(
            "BYML edit path is empty for {}",
            edit.file
        )));
    }
    let (last, parents) = edit
        .path
        .split_last()
        .ok_or_else(|| invalid(format!("BYML edit path is empty for {}", edit.file)))?;
    let mut node = document;
    for component in parents {
        node = match (node, component) {
            (Byml::Map(map), BymlPathComponent::Key(key)) => map
                .get_mut(key.as_str())
                .ok_or_else(|| invalid(format!("missing BYML key {key:?} in {}", edit.file)))?,
            (Byml::Array(array), BymlPathComponent::Index(index)) => {
                array.get_mut(*index).ok_or_else(|| {
                    invalid(format!(
                        "BYML index {index} is out of bounds in {}",
                        edit.file
                    ))
                })?
            }
            (_, component) => {
                return Err(invalid(format!(
                    "BYML path component {component:?} does not match its container in {}",
                    edit.file
                )))
            }
        };
    }
    let replacement = edit.value.clone().into_byml();
    let target = match (node, last) {
        (Byml::Map(map), BymlPathComponent::Key(key)) => {
            if !map.contains_key(key.as_str()) && edit.insert_if_missing {
                map.insert(key.as_str().into(), replacement);
                return Ok(());
            }
            map.get_mut(key.as_str())
                .ok_or_else(|| invalid(format!("missing BYML key {key:?} in {}", edit.file)))?
        }
        (Byml::Array(array), BymlPathComponent::Index(index)) => {
            array.get_mut(*index).ok_or_else(|| {
                invalid(format!(
                    "BYML index {index} is out of bounds in {}",
                    edit.file
                ))
            })?
        }
        (_, component) => {
            return Err(invalid(format!(
                "BYML path component {component:?} does not match its container in {}",
                edit.file
            )))
        }
    };
    if std::mem::discriminant(target) != std::mem::discriminant(&replacement) {
        return Err(invalid(format!(
            "BYML edit would change the existing value type in {} at {:?}",
            edit.file, edit.path
        )));
    }
    *target = replacement;
    Ok(())
}

fn byml_endian(data: &[u8]) -> io::Result<Endian> {
    match data.get(..2) {
        Some(b"YB") => Ok(Endian::Little),
        Some(b"BY") => Ok(Endian::Big),
        _ => Err(invalid(
            "parameter edit target is not an uncompressed BYML file",
        )),
    }
}

fn validate_policy(policy: &ActorPackPolicy) -> io::Result<()> {
    let mut sources = BTreeSet::new();
    let mut destinations = BTreeSet::new();
    for rename in &policy.renames {
        validate_internal_path(&rename.from)?;
        validate_internal_path(&rename.to)?;
        if rename.from == rename.to {
            return Err(invalid(format!(
                "rename source and destination are equal: {}",
                rename.from
            )));
        }
        if !sources.insert(&rename.from) {
            return Err(invalid(format!("duplicate rename source: {}", rename.from)));
        }
        if !destinations.insert(&rename.to) {
            return Err(invalid(format!(
                "duplicate rename destination: {}",
                rename.to
            )));
        }
    }
    for edit in &policy.parameter_edits {
        validate_internal_path(&edit.file)?;
    }
    Ok(())
}

fn validate_internal_path(path: &str) -> io::Result<()> {
    let value = Path::new(path);
    if path.is_empty()
        || value.is_absolute()
        || value.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || path.contains('\\')
    {
        return Err(invalid(format!("unsafe internal actor-pack path: {path}")));
    }
    Ok(())
}

fn validate_actor_name(actor: &str) -> io::Result<()> {
    if actor.is_empty()
        || actor.contains(['/', '\\'])
        || actor == "."
        || actor == ".."
        || !actor
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(invalid(format!("invalid template actor name: {actor}")));
    }
    Ok(())
}

fn ensure_output_outside_romfs(clean_romfs: &Path, output: &Path) -> io::Result<()> {
    let clean = clean_romfs.canonicalize()?;
    let output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()?.join(output)
    };
    let output = normalize_path(&output);
    if output.starts_with(&clean) {
        return Err(invalid(format!(
            "output pack must be outside clean ROMFS: {}",
            output.display()
        )));
    }
    Ok(())
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};
    use roead::sarc::{Sarc, SarcWriter};

    fn dictionaryless_zstd() -> Arc<TotkZstd<'static>> {
        Arc::new(TotkZstd::dictionaryless(
            Arc::new(TotkConfig::default()),
            TOTK_ZSTD_COMPRESSION_LEVEL,
        ))
    }

    #[test]
    fn renames_selected_files_and_edits_typed_byml_parameters() {
        let mut root = roead::byml::Map::default();
        root.insert("ActorName".into(), Byml::String("Weapon_Lsword_060".into()));
        root.insert("AttackPower".into(), Byml::I32(20));
        let byml = Byml::Map(root).to_binary(Endian::Little);
        let old = "Actor/Weapon_Lsword_060.engine__actor__ActorParam.bgyml";
        let new = "Actor/Weapon_Lsword_900.engine__actor__ActorParam.bgyml";
        let mut source = SarcWriter::new(Endian::Little);
        source.add_file(old, byml);
        source.add_file(
            "Phive/SharedTemplate.phive__ShapeParam.bgyml",
            b"shared".to_vec(),
        );

        let policy = ActorPackPolicy {
            renames: vec![InternalRename {
                from: old.into(),
                to: new.into(),
            }],
            parameter_edits: vec![
                BymlParameterEdit {
                    file: new.into(),
                    path: vec![BymlPathComponent::Key("ActorName".into())],
                    value: BymlValue::String("Weapon_Lsword_900".into()),
                    insert_if_missing: false,
                },
                BymlParameterEdit {
                    file: new.into(),
                    path: vec![BymlPathComponent::Key("AttackPower".into())],
                    value: BymlValue::I32(80),
                    insert_if_missing: false,
                },
            ],
        };

        let result = specialize_actor_pack(&source.to_binary(), &policy, dictionaryless_zstd())
            .expect("specialize actor pack");
        let sarc = Sarc::new(result).expect("reopen specialized SARC");
        assert!(sarc.get_data(old).is_none());
        assert_eq!(
            sarc.get_data("Phive/SharedTemplate.phive__ShapeParam.bgyml"),
            Some(b"shared".as_slice())
        );
        let edited = Byml::from_binary(sarc.get_data(new).expect("renamed BYML")).unwrap();
        let Byml::Map(map) = edited else {
            panic!("expected map")
        };
        assert!(
            matches!(map.get("ActorName"), Some(Byml::String(value)) if value == "Weapon_Lsword_900")
        );
        assert!(matches!(map.get("AttackPower"), Some(Byml::I32(80))));
    }

    #[test]
    fn rejects_parameter_type_changes() {
        let mut root = roead::byml::Map::default();
        root.insert("AttackPower".into(), Byml::I32(20));
        let mut source = SarcWriter::new(Endian::Little);
        source.add_file(
            "Actor/Test.bgyml",
            Byml::Map(root).to_binary(Endian::Little),
        );
        let policy = ActorPackPolicy {
            renames: vec![],
            parameter_edits: vec![BymlParameterEdit {
                file: "Actor/Test.bgyml".into(),
                path: vec![BymlPathComponent::Key("AttackPower".into())],
                value: BymlValue::U32(20),
                insert_if_missing: false,
            }],
        };
        let error = specialize_actor_pack(&source.to_binary(), &policy, dictionaryless_zstd())
            .expect_err("type change must fail");
        assert!(error.to_string().contains("change the existing value type"));
    }

    #[test]
    fn standard_weapon_policy_targets_actor_model_weapon_and_life_parameters() {
        let policy = ActorPackPolicy::standard_weapon_clone(
            "Weapon_Lsword_108",
            "Weapon_Lsword_005",
            WeaponParameterOverrides {
                model_name: Some("Weapon_Lsword_005".into()),
                base_attack: Some(18),
                max_life: Some(27),
                additional_damage: Some(18),
                shield_bash_damage: Some(18),
                chemical_ref: None,
            },
        )
        .unwrap();
        assert_eq!(policy.renames.len(), 6);
        assert!(policy.renames.iter().any(|rename| {
            rename.to == "Component/ModelInfo/Weapon_LSword_005.engine__component__ModelInfo.bgyml"
        }));
        assert!(policy.parameter_edits.iter().any(|edit| {
            edit.file
                .ends_with("Weapon_Lsword_005.game__component__WeaponParam.bgyml")
                && edit.path == [BymlPathComponent::Key("BaseAttack".into())]
                && edit.value == BymlValue::I32(18)
        }));
        assert!(policy.parameter_edits.iter().any(|edit| {
            edit.file
                .ends_with("Weapon_Lsword_005.game__life__LifeParameters.bgyml")
                && edit.path == [BymlPathComponent::Key("MaxLife".into())]
                && edit.value == BymlValue::I32(27)
        }));
    }

    #[test]
    fn weapon_pack_request_accepts_legacy_style_json_names() {
        let request = WeaponPackRequest::from_json(
            r#"{
                "name": "Weapon_Lsword_900",
                "base": "Weapon_Lsword_108",
                "model_name": "CustomBlade",
                "attack": 42,
                "dur": 77,
                "attachment_damage": 42
            }"#,
        )
        .unwrap();
        assert_eq!(request.actor_name, "Weapon_Lsword_900");
        assert_eq!(request.template_actor, "Weapon_Lsword_108");
        assert_eq!(request.base_attack, Some(42));
        assert_eq!(request.durability, Some(77));
    }

    #[test]
    fn weapon_pack_request_parses_link_sources() {
        let request = WeaponPackRequest::from_json(
            r#"{
                "name": "Weapon_Lsword_900",
                "base": "Weapon_Lsword_108",
                "sound": {
                    "source": "vanilla_actor",
                    "actor_name": "Weapon_Lsword_103"
                },
                "effect": {
                    "source": "file",
                    "path": "custom.engine__component__ELinkParam.bgyml"
                },
                "physics": "Weapon_Lsword_103",
                "chemical": "Weapon_Lsword_103",
                "shootable": "CustomProjectile"
            }"#,
        )
        .unwrap();
        assert_eq!(
            request.sound,
            Some(LinkParameterSource::VanillaActor {
                actor_name: "Weapon_Lsword_103".into()
            })
        );
        assert_eq!(
            request.effect,
            Some(LinkParameterSource::File {
                path: "custom.engine__component__ELinkParam.bgyml".into()
            })
        );
        assert_eq!(request.physics.as_deref(), Some("Weapon_Lsword_103"));
        assert_eq!(request.chemical.as_deref(), Some("Weapon_Lsword_103"));
        assert_eq!(request.shootable.as_deref(), Some("CustomProjectile"));
    }

    #[test]
    fn chemical_bundle_source_conflicts_with_raw_reference_override() {
        let request = WeaponPackRequest::from_json(
            r#"{
                "name": "Weapon_Lsword_900",
                "base": "Weapon_Lsword_108",
                "chemical": "Weapon_Lsword_103",
                "chemical_ref": "?Component/ChemicalParam/Other.game__component__ChemicalParam.bgyml"
            }"#,
        )
        .unwrap();
        let error = request
            .generate_pack(
                Path::new("."),
                Path::new("../tmp/unused.pack.zs"),
                dictionaryless_zstd(),
            )
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("chemical and chemical_ref cannot both be provided"));
    }

    #[test]
    fn omitted_request_values_do_not_create_parameter_edits() {
        let request = WeaponPackRequest::from_json(
            r#"{
                "name": "Weapon_Lsword_900",
                "base": "Weapon_Lsword_108"
            }"#,
        )
        .unwrap();
        assert_eq!(request.model_name, None);
        assert_eq!(request.base_attack, None);
        assert_eq!(request.durability, None);
        assert_eq!(request.chemical_ref, None);
        assert_eq!(request.attachment_damage, None);
        assert_eq!(request.shield_bash_damage, None);
        assert!(request.extra_edits.is_empty());

        let policy = ActorPackPolicy::standard_weapon_clone(
            &request.template_actor,
            &request.actor_name,
            WeaponParameterOverrides::default(),
        )
        .unwrap();
        assert!(policy.parameter_edits.iter().all(|edit| {
            !matches!(
                edit.path.last(),
                Some(BymlPathComponent::Key(key))
                    if matches!(
                        key.as_str(),
                        "BaseAttack"
                            | "WeaponType"
                            | "MaxLife"
                            | "AdditionalDamage"
                            | "ShieldBashDamage"
                            | "FmdbName"
                            | "ModelProjectName"
                            | "ChemicalRef"
                )
            )
        }));
        assert_eq!(policy.parameter_edits.len(), 6);
    }

    #[test]
    fn weapon_pack_request_requires_custom_actor_name() {
        let error = WeaponPackRequest::from_json(
            r#"{
                "base": "Weapon_Lsword_108",
                "attack": 42
            }"#,
        )
        .expect_err("custom actor name must be present");
        assert!(error.to_string().contains("actor_name"));
    }

    #[test]
    #[ignore = "requires the optional clean Weapon_Lsword_108 fixture"]
    fn loads_basic_weapon_info_from_real_template() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/items_creator_l108.sarc");
        if !fixture.is_file() {
            return;
        }
        let info = load_weapon_actor_info(
            &fs::read(fixture).unwrap(),
            "Weapon_Lsword_108",
            dictionaryless_zstd(),
        )
        .unwrap();
        assert_eq!(info.base_attack, Some(8));
        assert_eq!(info.durability, Some(26));
        assert_eq!(info.weapon_type.as_deref(), Some("LargeSword"));
        assert_eq!(info.attachment.additional_damage, Some(7));
        assert_eq!(info.model.fmdb_name, "Weapon_Lsword_108");
        assert!(info
            .chemical_material
            .as_deref()
            .is_some_and(|path| path.contains("weapon_wood")));
    }

    #[test]
    #[ignore = "diagnostic dump of tmp/packtest item component layouts"]
    fn inspects_packtest_item_layouts() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/packtest");
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let mut config = crate::TotkConfig::TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL).unwrap(),
        );
        for actor in [
            "Weapon_Sword_025",
            "Weapon_Spear_163",
            "Weapon_Shield_006",
            "Weapon_Bow_033",
        ] {
            let path = root.join(format!("{actor}.pack.zs"));
            let pack = PackFile::from_binary(&fs::read(path).unwrap(), zstd.clone()).unwrap();
            println!("ACTOR {actor}");
            for file in pack.sarc.files() {
                let Some(name) = file.name() else { continue };
                if name.contains(actor) && (name.starts_with("Actor/") || name.contains("Param/")) {
                    let byml = BymlFile::from_binary(file.data(), zstd.clone(), name).unwrap();
                    println!("PATH {name}\n{}", byml.pio.to_text());
                }
            }
        }
    }

    #[test]
    #[ignore = "generates all supported custom item kinds from tmp/packtest"]
    fn generates_supported_kinds_from_packtest() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/packtest");
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let mut config = crate::TotkConfig::TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL).unwrap(),
        );
        for (template, custom, kind, category, component_ref, component, key, expected) in [
            (
                "Weapon_Sword_025",
                "Weapon_Sword_925",
                super::super::WeaponKind::SmallSword,
                "Weapon",
                "WeaponRef",
                "WeaponParam",
                "WeaponType",
                Byml::String("SmallSword".into()),
            ),
            (
                "Weapon_Spear_163",
                "Weapon_Lsword_963",
                super::super::WeaponKind::LargeSword,
                "Weapon",
                "WeaponRef",
                "WeaponParam",
                "WeaponType",
                Byml::String("LargeSword".into()),
            ),
            (
                "Weapon_Spear_163",
                "Weapon_Spear_963",
                super::super::WeaponKind::Spear,
                "Weapon",
                "WeaponRef",
                "WeaponParam",
                "WeaponType",
                Byml::String("Spear".into()),
            ),
            (
                "Weapon_Shield_006",
                "Weapon_Shield_906",
                super::super::WeaponKind::Shield,
                "Shield",
                "ShieldRef",
                "ShieldParam",
                "GuardPower",
                Byml::I32(77),
            ),
            (
                "Weapon_Bow_033",
                "Weapon_Bow_933",
                super::super::WeaponKind::Bow,
                "Bow",
                "BowRef",
                "BowParam",
                "BaseAttack",
                Byml::I32(77),
            ),
        ] {
            let source = fs::read(root.join(format!("{template}.pack.zs"))).unwrap();
            let policy = ActorPackPolicy::standard_item_clone(
                template,
                custom,
                kind,
                WeaponParameterOverrides {
                    base_attack: Some(77),
                    ..Default::default()
                },
            )
            .unwrap();
            let generated = specialize_actor_pack(&source, &policy, zstd.clone()).unwrap();
            let pack = PackFile::from_binary(&generated, zstd.clone()).unwrap();
            let actor_path = format!("Actor/{custom}.engine__actor__ActorParam.bgyml");
            let actor = parse_pack_byml(&pack, &actor_path).unwrap();
            let actor = actor.as_map().unwrap();
            assert_eq!(actor.get("Category"), Some(&Byml::String(category.into())));
            let expected_ref =
                format!("?Component/{component}/{custom}.game__component__{component}.bgyml");
            assert_eq!(
                actor
                    .get("Components")
                    .and_then(|value| value.as_map().ok())
                    .and_then(|components| components.get(component_ref)),
                Some(&Byml::String(expected_ref.into()))
            );
            let parameter_path =
                format!("Component/{component}/{custom}.game__component__{component}.bgyml");
            let parameter = parse_pack_byml(&pack, &parameter_path).unwrap();
            assert_eq!(parameter.as_map().unwrap().get(key), Some(&expected));
            if kind.melee_weapon_type().is_some() {
                assert_eq!(
                    parameter.as_map().unwrap().get("BaseAttack"),
                    Some(&Byml::I32(77))
                );
            }
        }
    }

    #[test]
    #[ignore = "loads melee, shield, and bow values from tmp/packtest"]
    fn loads_all_packtest_item_parameter_kinds() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/packtest");
        let mut config = crate::TotkConfig::TotkConfig::default();
        config.romfs = "E:/TOTK_modding/0100F2C0115B6000/romfs".into();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL).unwrap(),
        );
        for (actor, category, strength, weapon_type) in [
            ("Weapon_Sword_025", "Weapon", 16, Some("SmallSword")),
            ("Weapon_Spear_163", "Weapon", 4, Some("Spear")),
            ("Weapon_Shield_006", "Shield", 25, None),
            ("Weapon_Bow_033", "Bow", 50, None),
        ] {
            let info = load_weapon_actor_info(
                &fs::read(root.join(format!("{actor}.pack.zs"))).unwrap(),
                actor,
                zstd.clone(),
            )
            .unwrap();
            assert_eq!(info.category.as_deref(), Some(category));
            assert_eq!(info.base_attack, Some(strength));
            assert_eq!(info.weapon_type.as_deref(), weapon_type);
        }
    }

    #[test]
    #[ignore = "requires the optional Weapon Restoration fixture"]
    fn audits_weapon_restoration_fixture() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/items_creator_l005.sarc");
        if !fixture.is_file() {
            return;
        }
        let entries = audit_actor_pack(&fs::read(fixture).unwrap(), dictionaryless_zstd()).unwrap();
        for (path, kind) in entries {
            println!("{kind:?}\t{path}");
        }
    }

    #[test]
    #[ignore = "requires the optional clean Weapon_Lsword_108 fixture"]
    fn standard_policy_reproduces_weapon_restoration_entry_layout() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/items_creator_l108.sarc");
        if !fixture.is_file() {
            return;
        }
        let policy = ActorPackPolicy::standard_weapon_clone(
            "Weapon_Lsword_108",
            "Weapon_Lsword_005",
            WeaponParameterOverrides {
                model_name: Some("Weapon_Lsword_005".into()),
                base_attack: Some(18),
                max_life: Some(27),
                additional_damage: Some(18),
                shield_bash_damage: Some(18),
                chemical_ref: None,
            },
        )
        .unwrap();
        let generated =
            specialize_actor_pack(&fs::read(fixture).unwrap(), &policy, dictionaryless_zstd())
                .unwrap();
        let audit = audit_actor_pack(&generated, dictionaryless_zstd()).unwrap();
        assert_eq!(
            audit
                .values()
                .filter(|kind| **kind == ActorPackEntryKind::Added)
                .count(),
            6
        );
        assert_eq!(
            audit
                .values()
                .filter(|kind| **kind == ActorPackEntryKind::Vanilla)
                .count(),
            33
        );
        assert!(!audit
            .values()
            .any(|kind| *kind == ActorPackEntryKind::Modified));
    }

    #[test]
    #[ignore = "requires a configured clean ROMFS"]
    fn json_request_generates_reopenable_pack_zs() {
        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        if !romfs.join("Pack/Actor/Weapon_Lsword_108.pack.zs").is_file() {
            return;
        }
        let mut config = TotkConfig::default();
        config.romfs = romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        let request = WeaponPackRequest::from_json(
            r#"{
                "name": "Weapon_Lsword_900",
                "base": "Weapon_Lsword_108",
                "model_name": "CustomBlade",
                "attack": 42,
                "dur": 77,
                "attachment_damage": 42,
                "shield_bash_damage": 42
            }"#,
        )
        .unwrap();
        let output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/Weapon_Lsword_900.generated.pack.zs");
        request.generate_pack(romfs, &output, zstd.clone()).unwrap();
        let generated = fs::read(&output).unwrap();
        let info = load_weapon_actor_info(&generated, "Weapon_Lsword_900", zstd.clone()).unwrap();
        let _ = fs::remove_file(&output);
        assert_eq!(info.base_attack, Some(42));
        assert_eq!(info.durability, Some(77));
        assert_eq!(info.attachment.additional_damage, Some(42));
        assert_eq!(info.model.fmdb_name, "CustomBlade");

        let inherited = WeaponPackRequest::from_json(
            r#"{
                "name": "Weapon_Lsword_901",
                "base": "Weapon_Lsword_108"
            }"#,
        )
        .unwrap();
        let inherited_output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/Weapon_Lsword_901.generated.pack.zs");
        inherited
            .generate_pack(romfs, &inherited_output, zstd.clone())
            .unwrap();
        let inherited_info = load_weapon_actor_info(
            &fs::read(&inherited_output).unwrap(),
            "Weapon_Lsword_901",
            zstd,
        )
        .unwrap();
        let _ = fs::remove_file(&inherited_output);
        assert_eq!(inherited_info.base_attack, Some(8));
        assert_eq!(inherited_info.durability, Some(26));
        assert_eq!(inherited_info.attachment.additional_damage, Some(7));
        assert_eq!(inherited_info.model.fmdb_name, "Weapon_Lsword_108");
    }

    #[test]
    #[ignore = "requires a configured clean ROMFS and the Weapon Restoration fixture"]
    fn real_pack_imports_links_physics_and_chemical_bundles() {
        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let effect_file = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/weapon005/Component/ELink/Weapon_Lsword_103.engine__component__ELinkParam.bgyml",
        );
        if !romfs.join("Pack/Actor/Weapon_Lsword_108.pack.zs").is_file()
            || !romfs.join("Pack/Actor/Weapon_Lsword_103.pack.zs").is_file()
            || !effect_file.is_file()
        {
            return;
        }
        let mut config = TotkConfig::default();
        config.romfs = romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        let custom_actor = "Weapon_Lsword_902";
        let output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/Weapon_Lsword_902.links.generated.pack.zs");
        let request = WeaponPackRequest {
            actor_name: custom_actor.into(),
            template_actor: "Weapon_Lsword_108".into(),
            kind: None,
            model_name: None,
            base_attack: None,
            durability: None,
            chemical_ref: None,
            chemical: Some("Weapon_Lsword_103".into()),
            attachment_damage: None,
            shield_bash_damage: None,
            sound: Some(LinkParameterSource::VanillaActor {
                actor_name: "Weapon_Lsword_103".into(),
            }),
            effect: Some(LinkParameterSource::File { path: effect_file }),
            physics: Some("Weapon_Lsword_103".into()),
            shootable: None,
            extra_edits: Vec::new(),
        };
        let mut missing_physics = request.clone();
        missing_physics.physics = Some("Weapon_Lsword_DoesNotExist".into());
        let missing_output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/Weapon_Lsword_902.missing-physics.pack.zs");
        assert!(!missing_output.exists());
        let error = missing_physics
            .generate_pack(romfs, &missing_output, zstd.clone())
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(!missing_output.exists());

        request.generate_pack(romfs, &output, zstd.clone()).unwrap();

        let generated = PackFile::from_binary(&fs::read(&output).unwrap(), zstd.clone()).unwrap();
        let actor_path = format!("Actor/{custom_actor}.engine__actor__ActorParam.bgyml");
        let actor = parse_pack_byml(&generated, &actor_path).unwrap();
        let actor_map = actor.as_map().unwrap();
        let components = actor_map.get("Components").unwrap().as_map().unwrap();
        let sound_ref = components.get("SLinkRef").unwrap().as_string().unwrap();
        let effect_ref = components.get("ELinkRef").unwrap().as_string().unwrap();
        let physics_ref = components.get("PhysicsRef").unwrap().as_string().unwrap();
        let chemical_ref = components.get("ChemicalRef").unwrap().as_string().unwrap();
        assert!(generated
            .sarc
            .get_data(&reference_to_internal(sound_ref))
            .is_some());
        let effect_path = reference_to_internal(effect_ref);
        let effect = Byml::from_binary(generated.sarc.get_data(&effect_path).unwrap()).unwrap();
        assert_eq!(
            effect
                .as_map()
                .unwrap()
                .get("UserName")
                .unwrap()
                .as_string()
                .unwrap(),
            custom_actor
        );

        let source = PackFile::from_binary(
            &fs::read(romfs.join("Pack/Actor/Weapon_Lsword_103.pack.zs")).unwrap(),
            zstd,
        )
        .unwrap();
        let source_physics: BTreeMap<_, _> = source
            .sarc
            .files()
            .filter_map(|file| {
                let path = file.name()?;
                (path.starts_with("Phive/") || path.starts_with("Component/Physics/"))
                    .then(|| (path.to_owned(), file.data().to_vec()))
            })
            .collect();
        let generated_physics: BTreeMap<_, _> = generated
            .sarc
            .files()
            .filter_map(|file| {
                let path = file.name()?;
                (path.starts_with("Phive/") || path.starts_with("Component/Physics/"))
                    .then(|| (path.to_owned(), file.data().to_vec()))
            })
            .collect();
        assert_eq!(generated_physics, source_physics);
        assert!(generated_physics.contains_key(&reference_to_internal(physics_ref)));
        let source_chemical: BTreeMap<_, _> = source
            .sarc
            .files()
            .filter_map(|file| {
                let path = file.name()?;
                (path.starts_with("Chemical/") || path.starts_with("Component/ChemicalParam/"))
                    .then(|| (path.to_owned(), file.data().to_vec()))
            })
            .collect();
        let generated_chemical: BTreeMap<_, _> = generated
            .sarc
            .files()
            .filter_map(|file| {
                let path = file.name()?;
                (path.starts_with("Chemical/") || path.starts_with("Component/ChemicalParam/"))
                    .then(|| (path.to_owned(), file.data().to_vec()))
            })
            .collect();
        assert_eq!(generated_chemical, source_chemical);
        assert!(generated_chemical.contains_key(&reference_to_internal(chemical_ref)));
        let _ = fs::remove_file(output);
    }

    #[test]
    #[ignore = "requires a configured clean ROMFS shooter weapon"]
    fn real_pack_renames_shooter_and_changes_only_first_shootable_actor() {
        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        if !romfs.join("Pack/Actor/Weapon_Lsword_041.pack.zs").is_file() {
            return;
        }
        let mut config = TotkConfig::default();
        config.romfs = romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).unwrap());
        let request = WeaponPackRequest {
            actor_name: "Weapon_Lsword_941".into(),
            template_actor: "Weapon_Lsword_041".into(),
            kind: None,
            model_name: None,
            base_attack: None,
            durability: None,
            chemical_ref: None,
            chemical: None,
            attachment_damage: None,
            shield_bash_damage: None,
            sound: None,
            effect: None,
            physics: None,
            shootable: Some("CustomProjectile".into()),
            extra_edits: Vec::new(),
        };
        let output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/Weapon_Lsword_941.shooter.generated.pack.zs");
        request.generate_pack(romfs, &output, zstd.clone()).unwrap();
        let generated = PackFile::from_binary(&fs::read(&output).unwrap(), zstd.clone()).unwrap();
        let actor = parse_pack_byml(
            &generated,
            "Actor/Weapon_Lsword_941.engine__actor__ActorParam.bgyml",
        )
        .unwrap();
        let shooter_ref =
            resolve_component_ref(&generated, &actor, "ShooterRef", &mut BTreeSet::new())
                .unwrap()
                .unwrap();
        let new_path = reference_to_internal(&shooter_ref);
        assert_eq!(
            new_path,
            "Component/ShooterParam/Weapon_Lsword_941.game__component__ShooterParam.bgyml"
        );
        let shooter = parse_pack_byml(&generated, &new_path).unwrap();
        let settings = shooter
            .as_map()
            .unwrap()
            .get("ShootableActorSettings")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(settings.len(), 4);
        assert_eq!(
            settings[0]
                .as_map()
                .unwrap()
                .get("Actor")
                .unwrap()
                .as_string()
                .unwrap(),
            "Work/Actor/CustomProjectile.engine__actor__ActorParam.gyml"
        );
        assert_eq!(
            settings[1]
                .as_map()
                .unwrap()
                .get("Actor")
                .unwrap()
                .as_string()
                .unwrap(),
            "Work/Actor/AssassinWindCutterFire.engine__actor__ActorParam.gyml"
        );
        assert!(generated
            .sarc
            .get_data(
                "Component/ShooterParam/Weapon_Lsword_114.game__component__ShooterParam.bgyml"
            )
            .is_none());
        let _ = fs::remove_file(output);
    }

    #[test]
    #[ignore = "requires a configured clean ROMFS non-weapon actor"]
    fn real_non_weapon_base_aborts_before_creating_output() {
        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        if !romfs.join("Pack/Actor/Npc_TripMaster_00.pack.zs").is_file() {
            return;
        }
        let mut config = TotkConfig::default();
        config.romfs = romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).unwrap());
        let request = WeaponPackRequest::from_json(
            r#"{
                "name": "Weapon_Lsword_999",
                "base": "Npc_TripMaster_00"
            }"#,
        )
        .unwrap();
        let output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/Weapon_Lsword_999.invalid-base.pack.zs");
        assert!(!output.exists());
        let error = request.generate_pack(romfs, &output, zstd).unwrap_err();
        assert!(error.to_string().contains("is not a weapon"));
        assert!(error.to_string().contains("NPC"));
        assert!(!output.exists());
    }
}
