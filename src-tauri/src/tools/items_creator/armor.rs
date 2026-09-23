//! Custom armor pieces (Head / Upper / Lower) cloned from a vanilla base actor.
//!
//! Armor is a pouch-only item: it has no SharpInfo, attachment rows or
//! compendium entry, but it does have sixteen dye icons, a shared `.anim`
//! BFRES per model project and an `ArmorParam` component that carries the
//! defense value. The clone keeps everything name-scoped under a new model
//! project (`Armor_001` → `Armor_900`), keeps the template's physics unless a
//! donor actor replaces it, and severs the upgrade / hood-swap links.

use super::{
    actor_pack,
    armor_model::{self, CubeModelReport, CubeModelSpec},
    assets, gamedata, messages, rsdb, vendor, UiTextureReport, VendorTarget,
};
use crate::{
    compression::meshcodec::MeshCodec,
    file_format::{
        BinTextFile::BymlFile,
        Model3D::bfres::{
            material_anim::{
                read_material_anim_bfres, write_material_anim_bfres, TexturePatternAnim,
                TexturePatternMaterial,
            },
            toolbox::ResFile,
            BfresFile,
        },
        Pack::PackFile,
    },
    parser::textogo::{writer as textogo_writer, TexToGoFile},
    Zstd::{TotkZstd, ZstdDictionary},
};
use roead::byml::Byml;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ArmorSlot {
    Head,
    Upper,
    Lower,
}

impl ArmorSlot {
    pub(crate) fn suffix(self) -> &'static str {
        match self {
            Self::Head => "_Head",
            Self::Upper => "_Upper",
            Self::Lower => "_Lower",
        }
    }

    /// `Armor_001_Head` → `Head`. Variant actors such as `Armor_001_Head_B`
    /// deliberately do not match: they are runtime mesh swaps, not items.
    pub(crate) fn from_actor_name(actor: &str) -> Option<Self> {
        [Self::Head, Self::Upper, Self::Lower]
            .into_iter()
            .find(|slot| actor.ends_with(slot.suffix()))
    }
}

/// Custom PNGs for armor. Armor has no compendium images.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArmorAssets {
    /// Replacement for the undyed inventory icon.
    #[serde(default)]
    pub icon_png: Option<PathBuf>,
    /// Custom mesh replacing the template's shapes (skinned to its bones).
    #[serde(default)]
    pub fbx: Option<PathBuf>,
}

fn one() -> i32 {
    1
}

fn default_true() -> bool {
    true
}

/// `"physics": "Armor_005_Head"`, `["Armor_005_Head", "Armor_180_Upper"]`
/// or `null`.
fn physics_donors<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Donors {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Option::<Donors>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(Donors::One(actor)) => vec![actor],
        Some(Donors::Many(actors)) => actors,
    })
}

/// One `ArmorEffect` entry of ArmorParam (`ArmorEffectType` plus the
/// optional `ArmorEffectLevel`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ArmorEffectSpec {
    #[serde(rename = "type", alias = "effect_type", alias = "ArmorEffectType")]
    pub effect_type: String,
    #[serde(
        default,
        alias = "ArmorEffectLevel",
        skip_serializing_if = "Option::is_none"
    )]
    pub level: Option<i32>,
}

/// `"armor_effects": ["QuietnessUp", {"type": "DecreaseZonauEnergy", "level": 1}]`
/// or `null`.
fn armor_effects<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ArmorEffectSpec>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Name(String),
        Full(ArmorEffectSpec),
    }
    Ok(Option::<Vec<Entry>>::deserialize(deserializer)?
        .unwrap_or_default()
        .into_iter()
        .map(|entry| match entry {
            Entry::Name(effect_type) => ArmorEffectSpec {
                effect_type,
                level: None,
            },
            Entry::Full(spec) => spec,
        })
        .collect())
}

/// The fifteen dye colours in the order of the `<Slot>_ftp` animation
/// frames 1..15 and of the vanilla `_<Color>` icon variants, with the tint
/// used when a piece is made dyeable from a single-colour template.
pub const DYE_COLORS: [(&str, [u8; 3]); 15] = [
    ("Blue", [60, 90, 200]),
    ("Red", [190, 40, 40]),
    ("Yellow", [230, 200, 50]),
    ("White", [235, 235, 235]),
    ("Black", [35, 35, 35]),
    ("Purple", [120, 60, 160]),
    ("Green", [60, 150, 70]),
    ("LightBlue", [120, 190, 230]),
    ("Navy", [30, 45, 110]),
    ("Orange", [230, 130, 40]),
    ("Pink", [235, 130, 180]),
    ("Crimson", [150, 20, 50]),
    ("LightYellow", [245, 235, 160]),
    ("Brown", [120, 80, 50]),
    ("Gray", [128, 128, 128]),
];

/// Hylian Hood defense gain per rank over rank 1 (3 → 5 → 8 → 12 → 20),
/// used when a template has no upgrade chain of its own.
const HYLIAN_DEFENSE_DELTAS: [i32; 4] = [2, 5, 9, 17];

/// One material the Great Fairy asks for.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ArmorUpgradeMaterial {
    /// Vanilla item actor, for example `Item_Enemy_77`.
    #[serde(alias = "name")]
    pub actor: String,
    #[serde(default = "one", alias = "number")]
    pub count: i32,
}

/// One Great Fairy step; `upgrades[0]` is rank 2 (★). At most four.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ArmorUpgradeSpec {
    /// `BaseDefense` / `EquipmentPerformance` at this rank.
    pub defense: i32,
    /// Rupees the Great Fairy charges for this step.
    #[serde(default, alias = "price")]
    pub rupees: i32,
    #[serde(default, alias = "items")]
    pub materials: Vec<ArmorUpgradeMaterial>,
    /// Rank actor name. Defaults to the base name followed by the step
    /// (`Armor_900_Head` → `Armor_900_Head_1`, `Armor_900_Head_2`, ...).
    #[serde(default)]
    pub actor_name: Option<String>,
    /// Shop prices of the rank actor; default to the base piece's prices.
    #[serde(default)]
    pub buying_price: Option<i32>,
    #[serde(default)]
    pub selling_price: Option<i32>,
    /// `ActivateSetBonus` in ArmorParam (vanilla sets it from rank 3 on
    /// series with a set bonus).
    #[serde(default)]
    pub activate_set_bonus: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ArmorSpec {
    /// `Armor_<id>_<Head|Upper|Lower>`.
    pub actor_name: String,
    /// Vanilla base actor with the same slot, for example `Armor_001_Head`.
    pub template_actor: String,
    pub display_name: String,
    pub description: String,
    /// `BaseDefense` in ArmorParam and `EquipmentPerformance` in PouchActorInfo.
    #[serde(default)]
    pub defense: Option<i32>,
    /// Armor set key (`SeriesName`). Defaults to the template's series.
    #[serde(default)]
    pub series_name: Option<String>,
    /// Placeholder geometry. When omitted the template mesh is kept.
    #[serde(default, alias = "cube")]
    pub model: Option<CubeModelSpec>,
    /// Actors whose `Phive/*` and `Component/Physics/*` entries are
    /// transferred into the clone (cloth, helper bones, ...); a single string
    /// is accepted too. Each donor is a vanilla actor name (its pack is read
    /// from the RomFS) or the path of an actor pack on disk (`.pack.zs` /
    /// `.pack`, e.g. a piece from another mod). One usable donor is copied
    /// verbatim. Two or more are merged into one bundle named after the
    /// actor: every cloth (with its skeleton) and collidable of their BPHCL
    /// files, their ClothParams, cloth reactions and advanced options; the
    /// helper bones come from the first donor only (renamed after the
    /// actor), since they drive bones of that donor's model.
    /// Empty, malformed or non-existent donors are ignored, and the
    /// template's own physics entries are preserved when no donor is usable.
    #[serde(default, alias = "physics_actor", deserialize_with = "physics_donors")]
    pub physics: Vec<String>,
    /// Vanilla armor actor whose `Phive/HelperBone/*` files are transferred
    /// into the clone after the physics step (whatever physics the piece
    /// ended up with keeps its cloth; its `HelperBoneList` is replaced).
    /// The helper-bone files, the ControllerSetParam, `Component/Physics`
    /// and the ActorParam `PhysicsRef` are renamed after the actor. `None`
    /// or blank keeps the helper bones the physics step left.
    #[serde(default, alias = "helper_bones", alias = "helper_bone_actor")]
    pub helper_bone: Option<String>,
    /// With a custom FBX: replace the bones with the FBX skeleton (Toolbox
    /// "Import Bones") instead of keeping the template skeleton.
    #[serde(default, alias = "import_skeleton")]
    pub replace_bones: bool,
    #[serde(default)]
    pub assets: ArmorAssets,
    #[serde(default)]
    pub vendors: Vec<VendorTarget>,
    /// Great Fairy upgrade ranks (★ to ★★★★), each a rank actor of its own.
    /// Empty with `upgrades_enabled` means "copy the template's vanilla
    /// chain" (see [`ArmorSpec::effective_upgrades`]).
    #[serde(default)]
    pub upgrades: Vec<ArmorUpgradeSpec>,
    /// Whether the Great Fairies can upgrade this piece at all.
    #[serde(default = "default_true", alias = "enable_upgrades")]
    pub upgrades_enabled: bool,
    /// Make the piece dyeable at the dye shop even when its template is
    /// not: adds the colour-variation component, sixteen tinted albedo
    /// slices, the `<Slot>_ftp` animation and the fifteen icon variants.
    /// A dyeable template stays dyeable either way.
    #[serde(default, alias = "make_dyeable")]
    pub dyeable: bool,
    /// Vanilla armor actor whose ArmorParam material visibility is copied
    /// into the new piece: both `HiddenMaterialGroupList` (the skin
    /// materials of Link's body model this piece hides, e.g. `G_Skin` with
    /// `Mt_Upper_Skin`) and `HideMaterialGroupNameList` (the body-model
    /// material groups it covers, e.g. `G_UpperBeltSet`, `G_Head`). A key
    /// the donor lacks is dropped so the ArmorParam default applies, exactly
    /// as on the donor. `None` keeps the template's lists.
    #[serde(default, alias = "skin_material_actor")]
    pub skin_material: Option<String>,
    /// `ArmorEffect` entries of the ArmorParam (the first one also becomes
    /// the PouchActorInfo `ArmorEffectType`); empty keeps the template's.
    #[serde(default, alias = "effects", deserialize_with = "armor_effects")]
    pub armor_effects: Vec<ArmorEffectSpec>,
    /// Audio: vanilla actor whose `Component/SLink` file is transferred into
    /// the piece (the ActorParam `SLinkRef` binds it) together with the audio
    /// keys of its ArmorParam (`SoundMaterial`, `HasSoundCloth`,
    /// `IsBarefootSound`). A blank name, or one without a pack or SLink in
    /// the RomFS, is skipped and the template's own sound stays.
    #[serde(default, alias = "audio", alias = "sound_actor")]
    pub sound: Option<String>,
    /// Effect: a vanilla actor whose `Component/ELink` file is transferred
    /// into the piece (the ActorParam `ELinkRef` binds it) together with the
    /// effect keys of its ArmorParam (`WindEffectMesh`, `WindEffectScale`),
    /// skipped like `sound` when unusable; or an ELinkParam `.bgyml` / actor
    /// pack file used as it is; or a hand-typed ELink user name
    /// (`{"source": "user_name", "user_name": "Item_Weapon_01_custom"}`).
    #[serde(default, alias = "effect_actor")]
    pub effect: Option<actor_pack::LinkParameterSource>,
    /// Advanced effect: XLink keys (asset-call-table entries of the ELink
    /// user the piece ends up with, e.g. `Miasma_Status_In` of `Player`)
    /// that a generated root AI search-and-emits once at spawn, giving the
    /// piece a permanent effect. Empty adds no AI; see `effect_ai`. Upgrade
    /// ranks inherit it with the rest of the base pack.
    #[serde(default, alias = "xlink_keys", alias = "effect_ai_keys")]
    pub effect_keys: Vec<String>,
}

/// Everything written for one upgrade rank actor.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmorUpgradeReport {
    pub actor_name: String,
    pub rank: u8,
    pub defense: i32,
    pub actor_pack: PathBuf,
    pub ui_textures: Vec<UiTextureReport>,
    pub messages: PathBuf,
    pub rsdb: Vec<PathBuf>,
    pub game_data: gamedata::WeaponGameDataReport,
}

/// Everything written for one armor piece, before the shared RSTB pass.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmorGenerationReport {
    pub actor_name: String,
    pub slot: ArmorSlot,
    pub actor_pack: PathBuf,
    pub model: PathBuf,
    pub model_anim: Option<PathBuf>,
    pub cube: Option<CubeModelReport>,
    pub textures: Vec<PathBuf>,
    pub texture_names: Vec<String>,
    pub ui_textures: Vec<UiTextureReport>,
    pub messages: PathBuf,
    pub rsdb: Vec<PathBuf>,
    pub game_data: gamedata::WeaponGameDataReport,
    pub vendor_packs: Vec<vendor::VendorPackReport>,
    pub upgrades: Vec<ArmorUpgradeReport>,
    /// Whether the generated piece can be dyed (template dyeable or made so).
    pub dyeable: bool,
}

impl ArmorSpec {
    pub fn slot(&self) -> io::Result<ArmorSlot> {
        ArmorSlot::from_actor_name(&self.actor_name).ok_or_else(|| {
            invalid(format!(
                "armor actor {} must end with _Head, _Upper or _Lower",
                self.actor_name
            ))
        })
    }

    /// Model project name, `Armor_900` for `Armor_900_Head`.
    pub fn project(&self) -> io::Result<String> {
        let slot = self.slot()?;
        Ok(self.actor_name[..self.actor_name.len() - slot.suffix().len()].to_owned())
    }

    /// Actor names of the explicit upgrade ranks (rank 2 first).
    pub fn rank_actor_names(&self) -> Vec<String> {
        Self::rank_actor_names_for(&self.actor_name, &self.upgrades)
    }

    /// Rank actor names for `upgrades`: an explicit `actor_name`, otherwise
    /// `<base>_<step>` (`Armor_900_Head_1` for rank 2 and so on).
    pub fn rank_actor_names_for(base: &str, upgrades: &[ArmorUpgradeSpec]) -> Vec<String> {
        upgrades
            .iter()
            .enumerate()
            .map(|(index, upgrade)| match &upgrade.actor_name {
                Some(name) if !name.trim().is_empty() => name.trim().to_owned(),
                _ => format!("{base}_{}", index + 1),
            })
            .collect()
    }

    /// The upgrade ranks the generator writes: none when upgrades are
    /// disabled, the explicit list when given, otherwise the template's
    /// vanilla chain (defense, prices, materials and rupees per rank). A
    /// template without a chain gets the Hylian Hood chain of the same slot
    /// with the defense rebuilt from this piece's own value plus the Hylian
    /// per-rank gains.
    pub fn effective_upgrades(
        &self,
        clean_romfs: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<Vec<ArmorUpgradeSpec>> {
        if !self.upgrades_enabled {
            return Ok(Vec::new());
        }
        if !self.upgrades.is_empty() {
            return Ok(self.upgrades.clone());
        }
        let slot = self.slot()?;
        let from_template =
            super::catalog::armor_upgrades(clean_romfs, &self.template_actor, zstd.clone())
                .unwrap_or_default();
        let base_defense = match self.defense {
            Some(value) => Some(value),
            None => {
                TemplateArmor::load(clean_romfs, &self.template_actor, zstd.clone())?.base_defense
            }
        };
        let (chain, defenses): (Vec<_>, Vec<Option<i32>>) = if from_template.is_empty() {
            let hylian = format!("Armor_001{}", slot.suffix());
            let chain = super::catalog::armor_upgrades(clean_romfs, &hylian, zstd)
                .filter(|chain| !chain.is_empty())
                .ok_or_else(|| {
                    invalid_data(format!(
                        "neither {} nor {hylian} has an upgrade chain in the RomFS",
                        self.template_actor
                    ))
                })?;
            let base = base_defense.unwrap_or(3);
            let defenses = HYLIAN_DEFENSE_DELTAS
                .iter()
                .map(|delta| Some(base + delta))
                .collect();
            (chain, defenses)
        } else {
            let defenses = from_template.iter().map(|rank| rank.defense).collect();
            (from_template, defenses)
        };
        Ok(chain
            .into_iter()
            .zip(defenses)
            .take(4)
            .enumerate()
            .map(|(index, (rank, defense))| ArmorUpgradeSpec {
                defense: defense.or(rank.defense).unwrap_or_else(|| {
                    base_defense.unwrap_or(3) + HYLIAN_DEFENSE_DELTAS[index.min(3)]
                }),
                rupees: rank.rupees.max(0),
                materials: rank
                    .materials
                    .into_iter()
                    .map(|material| ArmorUpgradeMaterial {
                        actor: material.actor,
                        count: material.count.max(1),
                    })
                    .collect(),
                actor_name: None,
                buying_price: rank.buying_price,
                selling_price: rank.selling_price,
                activate_set_bonus: false,
            })
            .collect())
    }

    fn template_project(&self) -> io::Result<String> {
        let slot = ArmorSlot::from_actor_name(&self.template_actor).ok_or_else(|| {
            invalid(format!(
                "template {} must be a base armor actor ending with _Head, _Upper or _Lower (variants such as _B are not items)",
                self.template_actor
            ))
        })?;
        Ok(self.template_actor[..self.template_actor.len() - slot.suffix().len()].to_owned())
    }

    pub fn validate(&self, asset_root: &Path) -> io::Result<()> {
        validate_actor_name(&self.actor_name)?;
        validate_actor_name(&self.template_actor)?;
        if !self.actor_name.starts_with("Armor_") {
            return Err(invalid(format!(
                "armor actor {} must start with Armor_",
                self.actor_name
            )));
        }
        for actor in [&self.skin_material, &self.helper_bone]
            .into_iter()
            .flatten()
            .map(|actor| actor.trim())
            .filter(|actor| !actor.is_empty())
        {
            validate_actor_name(actor)?;
        }
        for effect in &self.armor_effects {
            let name = effect.effect_type.trim();
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(invalid(format!(
                    "armor effect {:?} is not a valid ArmorEffectType",
                    effect.effect_type
                )));
            }
            if effect.level.is_some_and(|level| level < 0) {
                return Err(invalid(format!(
                    "armor effect {name}: level cannot be negative"
                )));
            }
        }
        let slot = self.slot()?;
        let template_slot = ArmorSlot::from_actor_name(&self.template_actor);
        if template_slot != Some(slot) {
            return Err(invalid(format!(
                "template {} does not match the {slot:?} slot of {}",
                self.template_actor, self.actor_name
            )));
        }
        if self.actor_name == self.template_actor {
            return Err(invalid("custom actor and template actor must differ"));
        }
        let project = self.project()?;
        let template_project = self.template_project()?;
        if project == template_project {
            return Err(invalid(format!(
                "armor {} must use a model project different from its template's {template_project}",
                self.actor_name
            )));
        }
        // Icon BNTX names are rewritten inside the vanilla string slots.
        if self.actor_name.len() > self.template_actor.len() {
            return Err(invalid(format!(
                "actor name {} is longer than template {}; BNTX texture names cannot grow",
                self.actor_name, self.template_actor
            )));
        }
        if self.display_name.trim().is_empty() || self.description.trim().is_empty() {
            return Err(invalid("display name and description are required"));
        }
        if self.defense.is_some_and(|value| value < 0) {
            return Err(invalid("defense cannot be negative"));
        }
        if self.upgrades.len() > 4 {
            return Err(invalid(
                "armor supports at most four upgrade ranks (★ to ★★★★)",
            ));
        }
        let rank_names = self.rank_actor_names();
        for (index, (upgrade, name)) in self.upgrades.iter().zip(&rank_names).enumerate() {
            let rank = index + 2;
            if upgrade.defense < 0 {
                return Err(invalid(format!("rank {rank} defense cannot be negative")));
            }
            if upgrade.rupees < 0 {
                return Err(invalid(format!("rank {rank} rupees cannot be negative")));
            }
            if upgrade.buying_price.is_some_and(|price| price < 0)
                || upgrade.selling_price.is_some_and(|price| price < 0)
            {
                return Err(invalid(format!("rank {rank} prices cannot be negative")));
            }
            for material in &upgrade.materials {
                validate_actor_name(&material.actor)?;
                if material.count < 1 {
                    return Err(invalid(format!(
                        "rank {rank} material {} needs a count of at least one",
                        material.actor
                    )));
                }
            }
            validate_actor_name(name)?;
            if *name == self.actor_name || *name == self.template_actor {
                return Err(invalid(format!(
                    "rank {rank} actor {name} must differ from the base and template actors"
                )));
            }
            if rank_names[..index].contains(name) {
                return Err(invalid(format!("rank actor {name} is used twice")));
            }
        }
        if let Some(model) = &self.model {
            if model.weights.is_empty() || model.weights.len() > 4 {
                return Err(invalid("cube weights must name one to four bones"));
            }
        }
        for vendor in &self.vendors {
            vendor::validate_vendor(vendor)?;
        }
        for asset in [&self.assets.icon_png, &self.assets.fbx]
            .into_iter()
            .flatten()
        {
            let resolved = super::resolve_asset(asset_root, asset);
            if !resolved.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("required source asset is missing: {}", resolved.display()),
                ));
            }
        }
        if self.assets.fbx.is_some() && self.model.is_some() {
            return Err(invalid(
                "choose either a custom FBX or the placeholder cube, not both",
            ));
        }
        // `physics` is not validated: unusable donors are skipped and the
        // template's own physics files stay when none is usable.
        Ok(())
    }

    /// Writes every per-piece file: actor pack, model (+ project anim), TexToGo
    /// textures, the sixteen icon BNTX, pouch messages, RSDB rows, GameData
    /// flags and vendor packs. RSTB is left to the caller.
    pub fn generate_files(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        asset_root: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<ArmorGenerationReport> {
        let mut shared = super::shared::SharedFiles::new(clean_romfs, output_romfs, zstd);
        let report = self.generate_files_with(&mut shared, asset_root)?;
        shared.flush()?;
        Ok(report)
    }

    /// [`ArmorSpec::generate_files`] for one piece of a batch: the per-piece
    /// files are written now, the mod-wide ones (messages, RSDB, GameDataList,
    /// vendor packs) are edited in `shared` and written when the caller
    /// flushes it.
    pub fn generate_files_with(
        &self,
        shared: &mut super::shared::SharedFiles<'_>,
        asset_root: &Path,
    ) -> io::Result<ArmorGenerationReport> {
        self.validate(asset_root)?;
        let clean_romfs = shared.clean_romfs().to_path_buf();
        let output_romfs = shared.output_romfs().to_path_buf();
        let zstd = shared.zstd();
        let clean_romfs = clean_romfs.as_path();
        let output_romfs = output_romfs.as_path();
        assets::ensure_output_outside_romfs(clean_romfs, output_romfs)?;
        let slot = self.slot()?;
        let project = self.project()?;
        let template_project = self.template_project()?;
        let template = TemplateArmor::load(clean_romfs, &self.template_actor, zstd.clone())?;
        let upgrades = self.effective_upgrades(clean_romfs, zstd.clone())?;
        let mut stopwatch = super::Stopwatch::new("armor");
        stopwatch.lap("template + upgrade chain");
        // A dyeable template stays dyeable; `dyeable` only adds what a
        // single-colour template lacks.
        let make_dyeable = self.dyeable && !template.dyeable;
        if make_dyeable && !dye_assets_supported() {
            return Err(invalid(
                "this build cannot write the dye textures and animation (TexToGo / FMAA writers unavailable)",
            ));
        }

        let links = self.resolve_links(clean_romfs, zstd.clone())?;
        let actor_pack = self.clone_actor_pack(
            clean_romfs,
            asset_root,
            output_romfs,
            &template,
            &upgrades,
            make_dyeable,
            &links,
            zstd.clone(),
        )?;
        stopwatch.lap("actor pack");
        let model = self.generate_model(
            clean_romfs,
            output_romfs,
            asset_root,
            &template,
            make_dyeable,
            zstd.clone(),
        )?;
        stopwatch.lap("model");
        let model_anim = match model.anim.clone() {
            Some(anim) => Some(anim),
            None => clone_project_anim(
                clean_romfs,
                output_romfs,
                &template_project,
                &project,
                slot,
                zstd.clone(),
            )?,
        };

        stopwatch.lap("project anim");
        let custom_icon = self
            .assets
            .icon_png
            .as_deref()
            .map(|path| super::resolve_asset(asset_root, path));
        let mut ui_textures = Vec::new();
        for (source, destination, name) in
            icon_variants(clean_romfs, &self.template_actor, &self.actor_name)?
        {
            let png = (name == self.actor_name)
                .then(|| custom_icon.clone())
                .flatten();
            ui_textures.push(super::generate_ui_texture(
                clean_romfs,
                output_romfs,
                source,
                destination,
                name,
                png,
                zstd.clone(),
            )?);
        }
        stopwatch.lap("icons");
        if make_dyeable {
            ui_textures.extend(generate_dye_icons(
                clean_romfs,
                output_romfs,
                &self.template_actor,
                &self.actor_name,
                custom_icon.as_deref(),
                zstd.clone(),
            )?);
        }

        stopwatch.lap("dye icons");
        let messages = messages::apply_pouch_labels(
            shared,
            &self.actor_name,
            &self.display_name,
            &self.description,
        )?;
        stopwatch.lap("messages");
        let mut rsdb = self.generate_rsdb(shared, &template, &upgrades, make_dyeable, &links)?;
        stopwatch.lap("rsdb");
        let game_data = gamedata::WeaponGameDataRequest {
            actor_name: self.actor_name.clone(),
            picture_book: false,
            inventory_flags: true,
        }
        .apply(shared)?;
        stopwatch.lap("game data");
        let upgrades =
            self.generate_upgrade_files(shared, &ui_textures, &upgrades, make_dyeable, &mut rsdb)?;
        stopwatch.lap("upgrade ranks");
        let mut vendor_packs = Vec::new();
        for target in &self.vendors {
            vendor_packs.extend(vendor::apply_weapon(shared, &self.actor_name, target)?);
        }

        stopwatch.lap("vendors");
        Ok(ArmorGenerationReport {
            actor_name: self.actor_name.clone(),
            slot,
            actor_pack,
            model: model.model,
            model_anim,
            cube: model.cube,
            textures: model.textures,
            texture_names: model.texture_names,
            ui_textures,
            messages,
            rsdb,
            game_data,
            vendor_packs,
            upgrades,
            dyeable: template.dyeable || make_dyeable,
        })
    }

    /// Audio / effect: a usable donor actor's SLink / ELink file replaces
    /// the template's own, the ActorParam reference binds it, the matching
    /// ArmorParam keys follow the donor and the RSDB ActorInfo row names the
    /// donor's link user. An unusable donor is skipped and the template's
    /// own sound / effect stays.
    /// Makes a relative effect file path absolute against `base`.
    pub fn anchor_link_paths(&mut self, base: &Path) {
        self.effect = self.effect.as_ref().map(|link| link.anchored(base));
    }

    fn resolve_links(
        &self,
        clean_romfs: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<Vec<ResolvedLink>> {
        let mut links = Vec::new();
        if let Some((path, data)) = actor_pack::optional_vanilla_link_entry(
            clean_romfs,
            self.sound.as_deref(),
            actor_pack::LinkKind::Sound,
            zstd.clone(),
        ) {
            let donor_keys = donor_armor_param_keys(
                clean_romfs,
                self.sound.as_deref().unwrap_or_default().trim(),
                &SOUND_ARMOR_KEYS[..],
                zstd.clone(),
            );
            links.push((actor_pack::LinkKind::Sound, path, data, donor_keys));
        }
        // The effect may also be a file or a typed user name; only a vanilla
        // donor has an ArmorParam to take the wind-effect keys from.
        if let Some(source) = &self.effect {
            if let Some((path, data)) = actor_pack::optional_link_entry(
                clean_romfs,
                source,
                &self.actor_name,
                actor_pack::LinkKind::Effect,
                zstd.clone(),
            )? {
                let donor_keys = match source {
                    actor_pack::LinkParameterSource::VanillaActor { actor_name } => {
                        donor_armor_param_keys(
                            clean_romfs,
                            actor_name.trim(),
                            &EFFECT_ARMOR_KEYS[..],
                            zstd.clone(),
                        )
                    }
                    _ => Vec::new(),
                };
                links.push((actor_pack::LinkKind::Effect, path, data, donor_keys));
            }
        }
        Ok(links)
    }

    /// Clones the template pack under the new project name. Every SARC entry
    /// and BYML string scoped to the template project is renamed, the physics
    /// bundle is swapped only when a usable donor actor is given, the helper
    /// bones are transferred from the chosen actor, and ArmorParam receives
    /// the custom defense/series values.
    fn clone_actor_pack(
        &self,
        clean_romfs: &Path,
        asset_root: &Path,
        output_romfs: &Path,
        template: &TemplateArmor,
        upgrades: &[ArmorUpgradeSpec],
        make_dyeable: bool,
        links: &[ResolvedLink],
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<PathBuf> {
        let project = self.project()?;
        let slot = self.slot()?;
        let template_project = self.template_project()?;
        let rank_names = Self::rank_actor_names_for(&self.actor_name, upgrades);
        let pack = PackFile::from_binary(&template.pack_bytes, zstd.clone())?;
        let actor_file = format!("Actor/{}.engine__actor__ActorParam.bgyml", self.actor_name);
        let model_info_file = format!(
            "Component/ModelInfo/{}.engine__component__ModelInfo.bgyml",
            self.actor_name
        );
        let color_variation_file = format!(
            "Component/ColorVariationParam/Armor{}.game__component__ColorVariationParam.bgyml",
            slot.suffix()
        );
        let armor_file = format!(
            "Component/ArmorParam/{}.game__component__ArmorParam.bgyml",
            self.actor_name
        );
        let price_file = format!(
            "GameParameter/PriceParam/{}.game__pouchcontent__PriceParam.bgyml",
            self.actor_name
        );
        let buying = self.vendors.first().and_then(|vendor| vendor.buying_price);
        let selling = self.vendors.first().and_then(|vendor| vendor.selling_price);

        // Physics: with a usable donor actor its Phive/Physics entries replace
        // the template's; otherwise the template's own physics files and its
        // PhysicsRef are preserved (renamed with the rest of the pack).
        let physics = super::physics::merged_physics_entries(
            clean_romfs,
            asset_root,
            &self.physics,
            &self.actor_name,
            zstd.clone(),
        )?;
        let replace_physics = physics.is_some();
        let physics_ref = physics.as_ref().map(|(reference, _)| reference.clone());
        // Advanced effect: a root AI emitting the chosen ELink keys at spawn
        // supersedes whatever root AI the template had.
        let effect_ai = super::effect_ai::build_effect_ai(&self.actor_name, &self.effect_keys)?;
        let mut entries: Vec<(String, Vec<u8>)> = physics
            .map(|(_, injected)| {
                injected
                    .into_iter()
                    .map(|entry| (entry.path, entry.data))
                    .collect()
            })
            .unwrap_or_default();
        for file in pack.sarc.files() {
            let name = file
                .name()
                .ok_or_else(|| invalid_data("template armor pack contains an unnamed entry"))?;
            if replace_physics
                && (name.starts_with("Phive/") || name.starts_with("Component/Physics/"))
            {
                continue;
            }
            if links
                .iter()
                .any(|(kind, ..)| name.starts_with(kind.entry_prefix()))
            {
                continue;
            }
            if effect_ai.is_some() && name.starts_with(super::effect_ai::ENTRY_PREFIX) {
                continue;
            }
            let new_name = replace_project(name, &template_project, &project);
            if !name.ends_with(".bgyml") {
                entries.push((new_name, file.data().to_vec()));
                continue;
            }
            let mut document = pack.byml_file(name)?;
            rename_project_strings(&mut document.pio, &template_project, &project);
            if new_name == actor_file {
                if let Some(physics_ref) = &physics_ref {
                    let components = map_child_mut(&mut document.pio, "Components")?;
                    components.insert(
                        "PhysicsRef".into(),
                        Byml::String(physics_ref.as_str().into()),
                    );
                }
                for (kind, path, ..) in links {
                    let components = map_child_mut(&mut document.pio, "Components")?;
                    components.insert(kind.component_key().into(), byml_string(format!("?{path}")));
                }
                // Vanilla armor templates carry no XLink component, and
                // without one the game never creates the piece's SLink /
                // ELink users: the shared ActorBasic reference binds them.
                // The rank packs clone this ActorParam, so every embedded
                // copy carries it.
                if !links.is_empty() && actor_pack::needs_xlink_component(&document.pio) {
                    let components = map_child_mut(&mut document.pio, "Components")?;
                    components.insert(
                        "XLinkRef".into(),
                        byml_string(actor_pack::XLINK_ACTOR_BASIC_REF),
                    );
                }
                if let Some(effect_ai) = &effect_ai {
                    let components = map_child_mut(&mut document.pio, "Components")?;
                    components.insert(
                        "AIInfoRef".into(),
                        byml_string(effect_ai.ai_info_ref.clone()),
                    );
                }
                if make_dyeable {
                    let components = map_child_mut(&mut document.pio, "Components")?;
                    components.insert(
                        "ColorVariationRef".into(),
                        byml_string(format!("?{color_variation_file}")),
                    );
                }
            } else if new_name == model_info_file && make_dyeable {
                // The dye animations of the project, listed for every slot
                // exactly as vanilla dyeable pieces do.
                let map = document
                    .pio
                    .as_mut_map()
                    .map_err(|_| invalid_data("ModelInfo root is not a map"))?;
                let anims = ["Head_ftp", "Upper_ftp", "Lower_ftp"]
                    .iter()
                    .map(|anim| {
                        let mut entry = roead::byml::Map::default();
                        entry.insert(
                            "Fmab".into(),
                            byml_string(format!(
                                "Work/Model/Player/Armor/{project}/output/{anim}.fmab"
                            )),
                        );
                        entry.insert("Frame".into(), Byml::Float(-1.0));
                        Byml::Map(entry)
                    })
                    .collect();
                map.insert("ModelVariationAnims".into(), Byml::Array(anims));
            } else if new_name == armor_file {
                let map = document
                    .pio
                    .as_mut_map()
                    .map_err(|_| invalid_data("ArmorParam root is not a map"))?;
                if let Some(value) = self.defense {
                    map.insert("BaseDefense".into(), Byml::I32(value));
                }
                if let Some(series) = &self.series_name {
                    map.insert("SeriesName".into(), Byml::String(series.as_str().into()));
                }
                for key in [
                    "NextRankActor",
                    "HeadSwapActor",
                    "WindEffectMesh",
                    "WindEffectScale",
                    "HasSoundCloth",
                ] {
                    map.remove(key);
                }
                // Audio / effect donors: their ArmorParam sound / wind-effect
                // keys replace the template's (a key the donor lacks is
                // dropped so the ArmorParam default applies).
                for (_, _, _, donor_keys) in links {
                    for (key, value) in donor_keys {
                        match value {
                            Some(value) => {
                                map.insert((*key).into(), value.clone());
                            }
                            None => {
                                map.remove(*key);
                            }
                        }
                    }
                }
                // The upgrade chain starts here: rank 2 is the next actor.
                if let Some(next) = rank_names.first() {
                    map.insert(
                        "NextRankActor".into(),
                        Byml::String(actor_param_work_path(next).into()),
                    );
                }
                // Material visibility: a chosen skin-material actor lends
                // both of its lists to the new piece. `HiddenMaterialGroupList`
                // names the skin materials of the body model the piece hides,
                // `HideMaterialGroupNameList` the body-model material groups
                // it covers; a key the donor lacks is dropped so the default
                // applies as it does on the donor.
                if let Some(actor) = self
                    .skin_material
                    .as_deref()
                    .map(str::trim)
                    .filter(|actor| !actor.is_empty())
                {
                    let visibility = material_visibility(clean_romfs, actor, zstd.clone())?;
                    map.insert("HiddenMaterialGroupList".into(), visibility.hidden_groups);
                    match visibility.hide_group_names {
                        Some(groups) => {
                            map.insert("HideMaterialGroupNameList".into(), groups);
                        }
                        None => {
                            map.remove("HideMaterialGroupNameList");
                        }
                    }
                }
                // `HideMaterialGroupNameList` names the material groups of
                // Link's body model (`G_Upper` torso/arm skin, `G_Lower` legs,
                // ...) that this piece covers. The template's (or donor's)
                // list is kept; a cube upper additionally hides the torso
                // skin, which vanilla tunics leave visible because their own
                // mesh covers it.
                if self.model.is_some() && self.slot()? == ArmorSlot::Upper {
                    let groups = map
                        .entry("HideMaterialGroupNameList".into())
                        .or_insert_with(|| Byml::Array(Vec::new()));
                    if let Ok(groups) = groups.as_mut_array() {
                        let present = groups
                            .iter()
                            .any(|value| value.as_string().is_ok_and(|v| v.as_str() == "G_Upper"));
                        if !present {
                            groups.insert(0, Byml::String("G_Upper".into()));
                        }
                    }
                }
                // `ArmorEffect`: the chosen effects replace the template's.
                if !self.armor_effects.is_empty() {
                    let effects = self
                        .armor_effects
                        .iter()
                        .map(|effect| {
                            let mut entry = roead::byml::Map::default();
                            entry.insert(
                                "ArmorEffectType".into(),
                                byml_string(effect.effect_type.trim()),
                            );
                            if let Some(level) = effect.level {
                                entry.insert("ArmorEffectLevel".into(), Byml::I32(level));
                            }
                            Byml::Map(entry)
                        })
                        .collect();
                    map.insert("ArmorEffect".into(), Byml::Array(effects));
                }
            } else if new_name == price_file {
                let map = document
                    .pio
                    .as_mut_map()
                    .map_err(|_| invalid_data("PriceParam root is not a map"))?;
                if let Some(value) = buying {
                    map.insert("BuyingPrice".into(), Byml::I32(value));
                }
                if let Some(value) = selling {
                    map.insert("SellingPrice".into(), Byml::I32(value));
                }
            }
            entries.push((new_name, document.to_binary_preserving_header()?));
        }
        for (_, path, data, _) in links {
            entries.push((path.clone(), data.clone()));
        }
        if let Some(effect_ai) = effect_ai {
            entries.extend(
                effect_ai
                    .entries
                    .into_iter()
                    .map(|entry| (entry.path, entry.data)),
            );
        }
        if !entries.iter().any(|(name, _)| *name == actor_file) {
            return Err(invalid_data(format!(
                "cloned pack has no {actor_file}; template project {template_project} was not found in the entry names"
            )));
        }
        if let Some(first) = upgrades.first() {
            set_enhancement_cost(&mut entries, &actor_file, first, zstd.clone())?;
        }
        // Helper bones: the chosen actor's Phive/HelperBone files replace the
        // ones the physics step left, and the physics binding is renamed.
        if let Some(donor) = self
            .helper_bone
            .as_deref()
            .map(str::trim)
            .filter(|donor| !donor.is_empty())
        {
            super::physics::transfer_helper_bones(
                clean_romfs,
                donor,
                &self.actor_name,
                &actor_file,
                &mut entries,
                zstd.clone(),
            )?;
        }
        if make_dyeable
            && !entries
                .iter()
                .any(|(name, _)| *name == color_variation_file)
        {
            entries.push(color_variation_entry(
                clean_romfs,
                slot,
                &color_variation_file,
                zstd.clone(),
            )?);
        }
        let output_bytes = pack.rebuild_binary(entries)?;
        let output = output_romfs
            .join("Pack/Actor")
            .join(format!("{}.pack.zs", self.actor_name));
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&output, &output_bytes)?;
        let verification = PackFile::from_binary(&output_bytes, zstd)?;
        let actor = verification.byml_file(&actor_file)?;
        let category = actor
            .pio
            .as_map()
            .ok()
            .and_then(|map| map.get("Category"))
            .and_then(|value| value.as_string().ok())
            .map(ToString::to_string);
        if category.as_deref() != Some("Armor") {
            return Err(invalid_data(format!(
                "generated {actor_file} has Category {category:?}, expected Armor"
            )));
        }
        Ok(output)
    }

    /// Clones the template model under the new project, optionally replacing
    /// its geometry with the skinned cube, and copies its textures (every dye
    /// slice of the albedo array included).
    fn generate_model(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        asset_root: &Path,
        template: &TemplateArmor,
        make_dyeable: bool,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<GeneratedArmorModel> {
        let project = self.project()?;
        let slot = self.slot()?;
        let template_project = self.template_project()?;
        let source = clean_romfs.join("Model").join(format!(
            "{}.{}.bfres.mc",
            template.model_project, template.fmdb_name
        ));
        if !source.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("template armor model is missing: {}", source.display()),
            ));
        }
        let mut stopwatch = super::Stopwatch::new("armor model");
        let new_fmdb = replace_project(&template.fmdb_name, &template_project, &project);
        if new_fmdb == template.fmdb_name {
            return Err(invalid_data(format!(
                "template model name {} is not scoped to project {template_project}",
                template.fmdb_name
            )));
        }
        let source_bytes = fs::read(&source)?;
        let raw = if crate::Settings::Magic::is_bfres(&source_bytes) {
            source_bytes
        } else {
            zstd.decompress_mcpk(&source_bytes).map_err(|error| {
                invalid_data(format!(
                    "failed to MCPK-decompress {}: {error}",
                    source.display()
                ))
            })?
        };
        // Custom FBX: Switch Toolbox's model import (with the optional
        // "Import Bones") on the Toolbox object model, like weapons.
        let fbx_bytes = match &self.assets.fbx {
            Some(fbx) => Some(fs::read(super::resolve_asset(asset_root, fbx))?),
            None => None,
        };
        let external = assets::external_strings_for(clean_romfs, &raw)?;
        let mut file = ResFile::load(&raw, &external)
            .map_err(|error| invalid_data(format!("failed to load template BFRES: {error}")))?;
        stopwatch.lap("load template bfres");
        if file.model_count() == 0 {
            return Err(invalid_data("template BFRES contains no model"));
        }
        if let Some(bytes) = &fbx_bytes {
            file.import_model_like_toolbox(bytes, self.replace_bones)
                .map_err(|error| invalid_data(format!("failed to import the FBX: {error}")))?;
        }
        stopwatch.lap("fbx import");
        let cube = match &self.model {
            Some(spec) => Some(armor_model::replace_with_skinned_cube(
                &mut file,
                spec,
                &template_project,
            )?),
            None => None,
        };

        // Textures: the material references `<base>_Alb.0`; dyeing needs the
        // whole `.0`–`.15` array, so every slice of a referenced array is copied.
        let texture_names = assets::model_texture_names(&file);
        let texture_sources = assets::index_textures(&clean_romfs.join("TexToGo"))?;
        let texture_output = output_romfs.join("TexToGo");
        fs::create_dir_all(&texture_output)?;
        let mut copied = Vec::new();
        // Making a single-colour template dyeable: every `_Alb` texture
        // becomes a sixteen-slice array (slice 0 untinted) and the material
        // points at slice 0, like vanilla dyeable pieces.
        let mut dye_materials: Vec<(String, String, String)> = Vec::new();
        if make_dyeable {
            dye_materials = write_dye_slices(
                &mut file,
                &texture_sources,
                &texture_output,
                &template_project,
                &project,
                &mut copied,
            )?;
        }
        for old_name in &texture_names {
            if make_dyeable && old_name.ends_with("_Alb") {
                continue;
            }
            let logical = old_name
                .strip_suffix(".txtg")
                .unwrap_or(old_name)
                .to_ascii_lowercase();
            let array_prefix = logical
                .rsplit_once('.')
                .filter(|(_, index)| index.chars().all(|c| c.is_ascii_digit()))
                .map(|(base, _)| format!("{base}."));
            for (key, source_texture) in &texture_sources {
                let matches = *key == logical
                    || array_prefix
                        .as_deref()
                        .is_some_and(|prefix| key.starts_with(prefix));
                if !matches {
                    continue;
                }
                let file_name = source_texture
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| invalid("texture filename is not UTF-8"))?;
                let destination_name = replace_project(file_name, &template_project, &project);
                if destination_name == file_name {
                    // Shared textures (CmnTex_*, Link_*) stay on the vanilla file.
                    continue;
                }
                let destination = texture_output.join(destination_name);
                fs::copy(source_texture, &destination)?;
                copied.push(destination);
            }
        }

        stopwatch.lap("textures");
        let container_name = format!("{project}.{new_fmdb}");
        file.set_internal_name(&container_name);
        file.rename_first_model(&new_fmdb)
            .map_err(|error| invalid_data(format!("failed to rename BFRES model: {error}")))?;
        file.rename_texture_slots(&template_project, &project);
        file.rename_model_strings(&template_project, &project);
        let leftovers: Vec<String> = file
            .strings_containing(&template_project)
            .into_iter()
            .filter(|(field, _)| field != "original_strings")
            .map(|(field, value)| format!("{field}={value}"))
            .collect();
        if !leftovers.is_empty() {
            return Err(invalid_data(format!(
                "generated BFRES still references the template project: {}",
                leftovers.join(", ")
            )));
        }
        let required_texture_names = assets::model_texture_names(&file);
        let renamed = file
            .save_like_toolbox()
            .map_err(|error| invalid_data(format!("failed to serialize BFRES: {error}")))?;
        stopwatch.lap("save bfres");
        let verified = BfresFile::from_bytes(&renamed)
            .map_err(|error| invalid_data(format!("failed to reopen generated BFRES: {error}")))?;
        assets::validate_bfres_geometry(&verified)?;
        let placeholder = assets::placeholder_texture_path()?;
        assets::ensure_material_textures(
            &texture_output,
            &required_texture_names,
            &placeholder,
            &mut copied,
            &texture_sources,
        )?;

        let destination = output_romfs
            .join("Model")
            .join(format!("{container_name}.bfres.mc"));
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        stopwatch.lap("verify + placeholder textures");
        let compressed = MeshCodec::compress(&renamed)
            .map_err(|error| invalid_data(format!("failed to MCPK-compress BFRES: {error}")))?;
        stopwatch.lap("meshcodec compress");
        let roundtrip = MeshCodec::decompress(&compressed).map_err(|error| {
            invalid_data(format!("generated MCPK does not decompress: {error}"))
        })?;
        BfresFile::from_bytes(&roundtrip).map_err(|error| {
            invalid_data(format!("round-tripped BFRES cannot be parsed: {error}"))
        })?;
        stopwatch.lap("meshcodec roundtrip check");
        fs::write(&destination, compressed)?;
        let anim = if make_dyeable {
            // Material names were renamed with the project above; the slice
            // names carry the project already.
            let materials = dye_materials
                .into_iter()
                .map(|(material, sampler, albedo)| {
                    (
                        replace_project(&material, &template_project, &project),
                        sampler,
                        albedo,
                    )
                })
                .collect::<Vec<_>>();
            Some(write_dye_anim(
                clean_romfs,
                output_romfs,
                &project,
                slot,
                &materials,
                zstd,
            )?)
        } else {
            None
        };
        stopwatch.lap("dye anim");
        Ok(GeneratedArmorModel {
            model: destination,
            cube,
            textures: copied,
            texture_names: required_texture_names.into_iter().collect(),
            anim,
        })
    }

    /// ActorInfo, GameActorInfo, PouchActorInfo rows and the Tag entry. Armor
    /// has no AttachmentActorInfo row. With upgrades the pouch row keeps its
    /// `ArmorNextRankActor`, now pointing at the rank-2 actor.
    fn generate_rsdb(
        &self,
        shared: &mut super::shared::SharedFiles<'_>,
        template: &TemplateArmor,
        upgrades: &[ArmorUpgradeSpec],
        make_dyeable: bool,
        links: &[ResolvedLink],
    ) -> io::Result<Vec<PathBuf>> {
        let project = self.project()?;
        let template_project = self.template_project()?;
        let mut actor: BTreeMap<String, JsonValue> = BTreeMap::new();
        actor.insert(
            "ActorName".into(),
            JsonValue::String(self.actor_name.clone()),
        );
        // The donor's link user, as vanilla rows mirror their own link files.
        for (kind, _, data, _) in links {
            actor.insert(
                kind.actor_info_key().into(),
                JsonValue::String(actor_pack::link_user_name(data)?),
            );
        }
        actor.insert(
            "FmdbName".into(),
            JsonValue::String(replace_project(
                &template.fmdb_name,
                &template_project,
                &project,
            )),
        );
        actor.insert(
            "ModelProjectName".into(),
            JsonValue::String(project.clone()),
        );
        let mut pouch: BTreeMap<String, JsonValue> = BTreeMap::new();
        if let Some(value) = self.defense {
            pouch.insert("EquipmentPerformance".into(), value.into());
        }
        if let Some(value) = self.vendors.first().and_then(|vendor| vendor.buying_price) {
            pouch.insert("BuyingPrice".into(), value.into());
        }
        if let Some(value) = self.vendors.first().and_then(|vendor| vendor.selling_price) {
            pouch.insert("SellingPrice".into(), value.into());
        }
        if let Some(next) = Self::rank_actor_names_for(&self.actor_name, upgrades).first() {
            pouch.insert("ArmorRank".into(), 1.into());
            pouch.insert(
                "ArmorNextRankActor".into(),
                JsonValue::String(actor_param_work_path(next)),
            );
        }
        if make_dyeable {
            pouch.insert(
                "ColorVariationType".into(),
                JsonValue::String("ArmorDye".into()),
            );
        }
        // Vanilla rows list one effect: the first ArmorParam entry.
        if let Some(effect) = self.armor_effects.first() {
            pouch.insert(
                "ArmorEffectType".into(),
                JsonValue::String(effect.effect_type.trim().to_owned()),
            );
        }
        clone_armor_rsdb_rows(
            shared,
            &self.template_actor,
            &self.actor_name,
            &actor,
            &pouch,
            &["ArmorNextRankActor", "ArmorHeadSwapActor"],
        )
    }

    /// Writes one actor pack, RSDB rows, GameData flags, pouch labels and
    /// icons per upgrade rank, plus the EnhancementMaterialInfo rows that
    /// price every step. Rank packs are the base pack plus rank-scoped
    /// ActorParam / ArmorParam / GameParameterTable / EnhancementMaterial /
    /// PriceParam documents that `$parent` the base ones, exactly like
    /// `Armor_002_Head` sits on `Armor_001_Head`.
    ///
    /// Icons are not built again per rank. The game loads
    /// `UI/Tex/Icon/<actor>[_<Color>].bntx.zs` by the actor's own name (only
    /// `ActorInfo.ActorName` redirects, and that also merges the pouch
    /// identity, so it is unusable for ranks), and vanilla ranks such as
    /// `Armor_002_Head` ship pixel-identical copies of the base icons under
    /// their own names. The rank files are therefore byte clones of the
    /// base's finished `base_icons` with just the texture name swapped: no
    /// PNG decoding, tinting or ASTC encoding happens for ranks.
    fn generate_upgrade_files(
        &self,
        shared: &mut super::shared::SharedFiles<'_>,
        base_icons: &[UiTextureReport],
        upgrades: &[ArmorUpgradeSpec],
        make_dyeable: bool,
        rsdb_outputs: &mut Vec<PathBuf>,
    ) -> io::Result<Vec<ArmorUpgradeReport>> {
        let Some(first_upgrade) = upgrades.first() else {
            return Ok(Vec::new());
        };
        let clean_romfs = shared.clean_romfs().to_path_buf();
        let output_romfs = shared.output_romfs().to_path_buf();
        let zstd = shared.zstd();
        let clean_romfs = clean_romfs.as_path();
        let output_romfs = output_romfs.as_path();
        let names = Self::rank_actor_names_for(&self.actor_name, upgrades);
        let vanilla_packs = clean_romfs.join("Pack/Actor");
        for name in &names {
            if vanilla_packs.join(format!("{name}.pack.zs")).is_file() {
                return Err(invalid(format!(
                    "rank actor {name} already exists in the vanilla RomFS; choose another actor_name"
                )));
            }
        }
        for material in upgrades.iter().flat_map(|upgrade| &upgrade.materials) {
            if !vanilla_packs
                .join(format!("{}.pack.zs", material.actor))
                .is_file()
            {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "upgrade material {} is not a vanilla actor (no Pack/Actor/{}.pack.zs)",
                        material.actor, material.actor
                    ),
                ));
            }
        }

        let base_pack_path = output_romfs
            .join("Pack/Actor")
            .join(format!("{}.pack.zs", self.actor_name));
        let base_bytes = fs::read(&base_pack_path)?;
        let pack = PackFile::from_binary(&base_bytes, zstd.clone())?;
        let mut base_entries: Vec<(String, Vec<u8>)> = Vec::new();
        for file in pack.sarc.files() {
            let name = file
                .name()
                .ok_or_else(|| invalid_data("generated armor pack contains an unnamed entry"))?;
            base_entries.push((name.to_owned(), file.data().to_vec()));
        }
        let actor_file = format!("Actor/{}.engine__actor__ActorParam.bgyml", self.actor_name);
        let base_armor_file = format!(
            "Component/ArmorParam/{}.game__component__ArmorParam.bgyml",
            self.actor_name
        );
        let actor_doc = pack.byml_file(&actor_file)?;
        let table_path = component_ref(&actor_doc.pio, "GameParameterTableRef")?;
        let table_doc = pack.byml_file(&table_path)?;
        let base_price_path = component_ref(&table_doc.pio, "PriceParam")?;
        let base_price = pack.byml_file(&base_price_path)?;
        let base_price_map = base_price
            .pio
            .as_map()
            .map_err(|_| invalid_data("base PriceParam is not a map"))?;
        let base_buying = base_price_map
            .get("BuyingPrice")
            .and_then(|value| value.as_i32().ok())
            .unwrap_or(0);
        let base_selling = base_price_map
            .get("SellingPrice")
            .and_then(|value| value.as_i32().ok())
            .unwrap_or(0);
        let like = pack
            .sarc
            .get_data(&base_armor_file)
            .ok_or_else(|| invalid_data(format!("generated pack has no {base_armor_file}")))?
            .to_vec();
        let mut table_template = table_doc
            .pio
            .as_map()
            .map_err(|_| invalid_data("base GameParameterTable is not a map"))?
            .clone();
        table_template.remove("$parent");

        let mut reports = Vec::with_capacity(upgrades.len());
        let mut enhancement_rows = vec![rsdb::EnhancementRow {
            actor: self.actor_name.clone(),
            materials: material_pairs(first_upgrade),
            price: first_upgrade.rupees,
        }];
        for (index, upgrade) in upgrades.iter().enumerate() {
            let mut stopwatch = super::Stopwatch::new("armor rank");
            let rank = index + 2;
            let name = names
                .get(index)
                .ok_or_else(|| invalid_data("rank actor name list is too short"))?;
            let previous = index
                .checked_sub(1)
                .and_then(|i| names.get(i))
                .unwrap_or(&self.actor_name);
            let next = names.get(index + 1);
            let buying = upgrade.buying_price.unwrap_or(base_buying);
            let selling = upgrade.selling_price.unwrap_or(base_selling);

            let rank_actor_file = format!("Actor/{name}.engine__actor__ActorParam.bgyml");
            let rank_armor_file =
                format!("Component/ArmorParam/{name}.game__component__ArmorParam.bgyml");
            let rank_table_file = format!(
                "GameParameter/GameParameterTable/{name}.engine__actor__GameParameterTable.bgyml"
            );
            let rank_cost_file = format!(
                "GameParameter/EnhancementMaterial/{name}.game__pouchcontent__EnhancementMaterial.bgyml"
            );
            let rank_price_file =
                format!("GameParameter/PriceParam/{name}.game__pouchcontent__PriceParam.bgyml");

            let mut components = roead::byml::Map::default();
            components.insert(
                "ActorNameRef".into(),
                byml_string(format!(
                    "?ActorSystem/ActorName/{name}.engine__actor__ActorName.bgyml"
                )),
            );
            components.insert(
                "ArmorRef".into(),
                byml_string(format!("?{rank_armor_file}")),
            );
            components.insert(
                "GameParameterTableRef".into(),
                byml_string(format!("?{rank_table_file}")),
            );
            components.insert(
                "PouchContentRef".into(),
                byml_string(
                    "?Component/PouchContentParam/ArmorGetTypeMedium.game__component__PouchContentParam.bgyml",
                ),
            );
            let mut actor_map = roead::byml::Map::default();
            actor_map.insert(
                "$parent".into(),
                byml_string(actor_param_work_path(&self.actor_name)),
            );
            actor_map.insert("Components".into(), Byml::Map(components));

            let mut armor_map = roead::byml::Map::default();
            armor_map.insert(
                "$parent".into(),
                byml_string(format!(
                    "Work/Component/ArmorParam/{previous}.game__component__ArmorParam.gyml"
                )),
            );
            armor_map.insert("BaseDefense".into(), Byml::I32(upgrade.defense));
            armor_map.insert("Rank".into(), Byml::I32(rank as i32));
            if let Some(next) = next {
                armor_map.insert(
                    "NextRankActor".into(),
                    byml_string(actor_param_work_path(next)),
                );
            }
            if upgrade.activate_set_bonus {
                armor_map.insert("ActivateSetBonus".into(), Byml::Bool(true));
            }

            let mut table_map = table_template.clone();
            let mut table_components = table_map
                .get("Components")
                .and_then(|value| value.as_map().ok())
                .cloned()
                .unwrap_or_default();
            table_components.insert(
                "PriceParam".into(),
                byml_string(format!("?{rank_price_file}")),
            );
            if next.is_some() {
                table_components.insert(
                    "EnhancementMaterial".into(),
                    byml_string(format!("?{rank_cost_file}")),
                );
            } else {
                table_components.remove("EnhancementMaterial");
            }
            table_map.insert("Components".into(), Byml::Map(table_components));

            let mut price_map = roead::byml::Map::default();
            price_map.insert("BuyingPrice".into(), Byml::I32(buying));
            price_map.insert("CreatingPrice".into(), Byml::I32(0));
            price_map.insert("SaleRevivalCount".into(), Byml::I32(-1));
            price_map.insert("SellingPrice".into(), Byml::I32(selling));

            let mut entries = base_entries.clone();
            entries.push((
                rank_actor_file.clone(),
                byml_bytes_like(Byml::Map(actor_map), &like, zstd.clone())?,
            ));
            entries.push((
                rank_armor_file.clone(),
                byml_bytes_like(Byml::Map(armor_map), &like, zstd.clone())?,
            ));
            entries.push((
                rank_table_file,
                byml_bytes_like(Byml::Map(table_map), &like, zstd.clone())?,
            ));
            entries.push((
                rank_price_file,
                byml_bytes_like(Byml::Map(price_map), &like, zstd.clone())?,
            ));
            if let Some(next_upgrade) = upgrades.get(index + 1) {
                entries.push((
                    rank_cost_file,
                    byml_bytes_like(enhancement_cost_byml(next_upgrade), &like, zstd.clone())?,
                ));
                enhancement_rows.push(rsdb::EnhancementRow {
                    actor: name.clone(),
                    materials: material_pairs(next_upgrade),
                    price: next_upgrade.rupees,
                });
            }
            let output_bytes = pack.rebuild_binary(entries)?;
            let actor_pack = output_romfs
                .join("Pack/Actor")
                .join(format!("{name}.pack.zs"));
            fs::write(&actor_pack, &output_bytes)?;
            let verification = PackFile::from_binary(&output_bytes, zstd.clone())?;
            verification.byml_file(&rank_actor_file)?;
            verification.byml_file(&rank_armor_file)?;

            let mut actor: BTreeMap<String, JsonValue> = BTreeMap::new();
            actor.insert("ActorName".into(), JsonValue::String(name.clone()));
            let mut pouch: BTreeMap<String, JsonValue> = BTreeMap::new();
            pouch.insert("ArmorRank".into(), (rank as i32).into());
            pouch.insert("EquipmentPerformance".into(), upgrade.defense.into());
            pouch.insert("BuyingPrice".into(), buying.into());
            pouch.insert("SellingPrice".into(), selling.into());
            pouch.insert("PouchGetType".into(), JsonValue::String("Medium".into()));
            if let Some(next) = next {
                pouch.insert(
                    "ArmorNextRankActor".into(),
                    JsonValue::String(actor_param_work_path(next)),
                );
            }
            if make_dyeable {
                pouch.insert(
                    "ColorVariationType".into(),
                    JsonValue::String("ArmorDye".into()),
                );
            }
            stopwatch.lap("rank pack");
            let rsdb = clone_armor_rsdb_rows(
                shared,
                &self.actor_name,
                name,
                &actor,
                &pouch,
                &["ArmorNextRankActor"],
            )?;
            stopwatch.lap("rank rsdb");
            let game_data = gamedata::WeaponGameDataRequest {
                actor_name: name.clone(),
                picture_book: false,
                inventory_flags: true,
            }
            .apply(shared)?;
            stopwatch.lap("rank game data");
            let messages =
                messages::apply_pouch_labels(shared, name, &self.display_name, &self.description)?;
            stopwatch.lap("rank messages");
            let ui_textures = clone_rank_icons(
                clean_romfs,
                output_romfs,
                base_icons,
                &self.actor_name,
                name,
                zstd.clone(),
            )?;
            stopwatch.lap("rank icons");
            reports.push(ArmorUpgradeReport {
                actor_name: name.clone(),
                rank: rank as u8,
                defense: upgrade.defense,
                actor_pack,
                ui_textures,
                messages,
                rsdb,
                game_data,
            });
        }

        let clean_rsdb = clean_romfs.join("RSDB");
        let output_rsdb = output_romfs.join("RSDB");
        let (version, _) = super::version::discover_product_file(
            &clean_rsdb,
            "ActorInfo.Product.",
            ".rstbl.byml.zs",
        )?;
        let table =
            rsdb::WeaponRsdbProcessor::versioned_rsdb_name("EnhancementMaterialInfo", &version)?;
        let destination = output_rsdb.join(&table);
        rsdb::WeaponRsdbProcessor::upsert_enhancement_material_rows(
            shared,
            &clean_rsdb.join(&table),
            &destination,
            &enhancement_rows,
        )?;
        if !rsdb_outputs.contains(&destination) {
            rsdb_outputs.push(destination);
        }
        Ok(reports)
    }
}

/// ActorInfo, GameActorInfo and PouchActorInfo rows plus the Tag entry,
/// cloned from `template_actor`'s rows (vanilla or already generated).
#[allow(clippy::too_many_arguments)]
fn clone_armor_rsdb_rows(
    shared: &mut super::shared::SharedFiles<'_>,
    template_actor: &str,
    actor_name: &str,
    actor_overrides: &BTreeMap<String, JsonValue>,
    pouch_overrides: &BTreeMap<String, JsonValue>,
    pouch_removals: &[&str],
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
    let game_actor_info =
        rsdb::WeaponRsdbProcessor::versioned_rsdb_name("GameActorInfo", &version)?;
    let pouch_actor_info =
        rsdb::WeaponRsdbProcessor::versioned_rsdb_name("PouchActorInfo", &version)?;
    let tag_product = rsdb::WeaponRsdbProcessor::versioned_rsdb_name("Tag", &version)?;
    let tables: [(String, &BTreeMap<String, JsonValue>, &[&str]); 3] = [
        (actor_info, actor_overrides, &[]),
        (game_actor_info, &BTreeMap::new(), &[]),
        (pouch_actor_info, pouch_overrides, pouch_removals),
    ];
    let mut outputs = Vec::with_capacity(4);
    for (name, overrides, removals) in tables {
        let destination = output_rsdb.join(&name);
        rsdb::WeaponRsdbProcessor::clone_rsdb_row(
            shared,
            &clean_rsdb.join(&name),
            &destination,
            template_actor,
            actor_name,
            overrides,
            removals,
        )?;
        outputs.push(destination);
    }
    let tag_output = output_rsdb.join(&tag_product);
    rsdb::WeaponRsdbProcessor::clone_tag_entry(
        shared,
        &clean_rsdb.join(&tag_product),
        &tag_output,
        template_actor,
        actor_name,
    )?;
    outputs.push(tag_output);
    Ok(outputs)
}

/// `Work/Actor/<actor>.engine__actor__ActorParam.gyml`, the form NextRankActor
/// and RSDB references use.
pub(crate) fn actor_param_work_path(actor: &str) -> String {
    format!("Work/Actor/{actor}.engine__actor__ActorParam.gyml")
}

fn byml_string(value: impl Into<String>) -> Byml {
    let value: String = value.into();
    Byml::String(value.as_str().into())
}

/// `Components.<key>` of an ActorParam / GameParameterTable document with
/// the leading `?` removed, as a pack-internal path.
fn component_ref(document: &Byml, key: &str) -> io::Result<String> {
    document
        .as_map()
        .ok()
        .and_then(|map| map.get("Components"))
        .and_then(|components| components.as_map().ok())
        .and_then(|components| components.get(key))
        .and_then(|value| value.as_string().ok())
        .map(|value| value.trim_start_matches('?').to_owned())
        .ok_or_else(|| invalid_data(format!("document has no Components.{key} reference")))
}

/// `{Items: [{Actor, Number}], Price}` for one Great Fairy step.
fn enhancement_cost_byml(upgrade: &ArmorUpgradeSpec) -> Byml {
    let items = upgrade
        .materials
        .iter()
        .map(|material| {
            let mut item = roead::byml::Map::default();
            item.insert(
                "Actor".into(),
                byml_string(actor_param_work_path(&material.actor)),
            );
            item.insert("Number".into(), Byml::I32(material.count));
            Byml::Map(item)
        })
        .collect();
    let mut map = roead::byml::Map::default();
    map.insert("Items".into(), Byml::Array(items));
    map.insert("Price".into(), Byml::I32(upgrade.rupees));
    Byml::Map(map)
}

fn material_pairs(upgrade: &ArmorUpgradeSpec) -> Vec<(String, i32)> {
    upgrade
        .materials
        .iter()
        .map(|material| (material.actor.clone(), material.count))
        .collect()
}

/// Serializes `value` with the endian and BYML version of `like`.
fn byml_bytes_like(value: Byml, like: &[u8], zstd: Arc<TotkZstd<'_>>) -> io::Result<Vec<u8>> {
    let mut document = BymlFile::from_binary(like, zstd, "generated.bgyml")?;
    document.pio = value;
    document.to_binary_preserving_header()
}

/// Makes the base pack's EnhancementMaterial hold the rank-2 cost: the file
/// the base GameParameterTable references is rewritten, or added together
/// with its component when the template had none.
fn set_enhancement_cost(
    entries: &mut Vec<(String, Vec<u8>)>,
    actor_file: &str,
    upgrade: &ArmorUpgradeSpec,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<()> {
    let entry_index = |entries: &Vec<(String, Vec<u8>)>, name: &str| {
        entries.iter().position(|(entry, _)| entry == name)
    };
    let actor_index = entry_index(entries, actor_file)
        .ok_or_else(|| invalid_data(format!("cloned pack has no {actor_file}")))?;
    let actor_doc = BymlFile::from_binary(&entries[actor_index].1, zstd.clone(), actor_file)?;
    let table_path = component_ref(&actor_doc.pio, "GameParameterTableRef")?;
    let table_index = entry_index(entries, &table_path)
        .ok_or_else(|| invalid_data(format!("cloned pack has no {table_path}")))?;
    let mut table_doc = BymlFile::from_binary(&entries[table_index].1, zstd.clone(), &table_path)?;
    let actor_name = actor_file
        .trim_start_matches("Actor/")
        .split('.')
        .next()
        .unwrap_or_default()
        .to_owned();
    let cost_path = match component_ref(&table_doc.pio, "EnhancementMaterial") {
        Ok(path) => path,
        Err(_) => {
            let path = format!(
                "GameParameter/EnhancementMaterial/{actor_name}.game__pouchcontent__EnhancementMaterial.bgyml"
            );
            let components = map_child_mut(&mut table_doc.pio, "Components")?;
            components.insert(
                "EnhancementMaterial".into(),
                byml_string(format!("?{path}")),
            );
            entries[table_index].1 = table_doc.to_binary_preserving_header()?;
            path
        }
    };
    let like = entries[table_index].1.clone();
    let cost = byml_bytes_like(enhancement_cost_byml(upgrade), &like, zstd)?;
    match entry_index(entries, &cost_path) {
        Some(index) => entries[index].1 = cost,
        None => entries.push((cost_path, cost)),
    }
    Ok(())
}

struct GeneratedArmorModel {
    model: PathBuf,
    cube: Option<CubeModelReport>,
    textures: Vec<PathBuf>,
    texture_names: Vec<String>,
    /// `Model/<project>.anim.bfres.zs` written for a piece made dyeable.
    anim: Option<PathBuf>,
}

/// The material-visibility lists of a vanilla armor actor's ArmorParam.
struct MaterialVisibility {
    /// `HiddenMaterialGroupList` (an empty list when the actor states none).
    hidden_groups: Byml,
    /// `HideMaterialGroupNameList`, `None` when the actor's ArmorParam lacks it.
    hide_group_names: Option<Byml>,
}

/// Both material-visibility lists of a vanilla armor actor's ArmorParam.
fn material_visibility(
    clean_romfs: &Path,
    actor: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<MaterialVisibility> {
    let pack_path = clean_romfs
        .join("Pack/Actor")
        .join(format!("{actor}.pack.zs"));
    if !pack_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "skin material actor {actor} has no vanilla actor pack: {}",
                pack_path.display()
            ),
        ));
    }
    let pack = PackFile::from_binary(&fs::read(&pack_path)?, zstd)?;
    let armor_path = format!("Component/ArmorParam/{actor}.game__component__ArmorParam.bgyml");
    let armor = pack.byml_file(&armor_path).map_err(|error| {
        invalid(format!(
            "skin material actor {actor} has no ArmorParam ({armor_path}): {error}"
        ))
    })?;
    let map = armor
        .pio
        .as_map()
        .map_err(|_| invalid_data(format!("{armor_path} is not a map")))?;
    let hidden_groups = map
        .get("HiddenMaterialGroupList")
        .cloned()
        .unwrap_or_else(|| Byml::Array(Vec::new()));
    if hidden_groups.as_array().is_err() {
        return Err(invalid_data(format!(
            "{armor_path}: HiddenMaterialGroupList is not a list"
        )));
    }
    let hide_group_names = map.get("HideMaterialGroupNameList").cloned();
    if hide_group_names
        .as_ref()
        .is_some_and(|value| value.as_array().is_err())
    {
        return Err(invalid_data(format!(
            "{armor_path}: HideMaterialGroupNameList is not a list"
        )));
    }
    Ok(MaterialVisibility {
        hidden_groups,
        hide_group_names,
    })
}

/// ArmorParam keys that follow the audio donor.
const SOUND_ARMOR_KEYS: [&str; 3] = ["SoundMaterial", "HasSoundCloth", "IsBarefootSound"];
/// ArmorParam keys that follow the effect donor.
const EFFECT_ARMOR_KEYS: [&str; 2] = ["WindEffectMesh", "WindEffectScale"];

/// Per key: the donor's ArmorParam value, or `None` when its ArmorParam lacks it.
type DonorArmorKeys = Vec<(&'static str, Option<Byml>)>;

/// A sound / effect donor resolved for the piece: the link kind, the pack
/// path and bytes of the donor's link file, and the donor's ArmorParam keys.
type ResolvedLink = (actor_pack::LinkKind, String, Vec<u8>, DonorArmorKeys);

/// The listed ArmorParam keys of a vanilla donor actor. Empty when the donor
/// is not an armor actor (its pack has no ArmorParam), in which case the
/// template's own keys stay.
fn donor_armor_param_keys(
    clean_romfs: &Path,
    actor: &str,
    keys: &[&'static str],
    zstd: Arc<TotkZstd<'_>>,
) -> DonorArmorKeys {
    let pack_path = clean_romfs
        .join("Pack/Actor")
        .join(format!("{actor}.pack.zs"));
    let Ok(bytes) = fs::read(&pack_path) else {
        return Vec::new();
    };
    let Ok(pack) = PackFile::from_binary(&bytes, zstd) else {
        return Vec::new();
    };
    let armor_path = format!("Component/ArmorParam/{actor}.game__component__ArmorParam.bgyml");
    let Ok(armor) = pack.byml_file(&armor_path) else {
        return Vec::new();
    };
    let Ok(map) = armor.pio.as_map() else {
        return Vec::new();
    };
    keys.iter()
        .map(|key| (*key, map.get(*key).cloned()))
        .collect()
}

/// What the generator needs to know about the vanilla base actor.
struct TemplateArmor {
    pack_bytes: Vec<u8>,
    model_project: String,
    fmdb_name: String,
    /// ArmorParam `BaseDefense`, when the template states one itself.
    base_defense: Option<i32>,
    /// The template ActorParam has a `ColorVariationRef` (dye shop support).
    dyeable: bool,
}

impl TemplateArmor {
    fn load(clean_romfs: &Path, template_actor: &str, zstd: Arc<TotkZstd<'_>>) -> io::Result<Self> {
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
        let pack_bytes = fs::read(&pack_path)?;
        let pack = PackFile::from_binary(&pack_bytes, zstd)?;
        let actor_path = format!("Actor/{template_actor}.engine__actor__ActorParam.bgyml");
        let actor = pack.byml_file(&actor_path)?;
        let actor_map = actor
            .pio
            .as_map()
            .map_err(|_| invalid_data(format!("{actor_path} is not a map")))?;
        let category = actor_map
            .get("Category")
            .and_then(|value| value.as_string().ok())
            .map(ToString::to_string);
        if category.as_deref() != Some("Armor") {
            return Err(invalid(format!(
                "base actor {template_actor} has ActorParam Category {category:?}, expected \"Armor\""
            )));
        }
        let dyeable = actor_map
            .get("Components")
            .and_then(|components| components.as_map().ok())
            .is_some_and(|components| components.contains_key("ColorVariationRef"));
        let base_defense = pack
            .byml_file(&format!(
                "Component/ArmorParam/{template_actor}.game__component__ArmorParam.bgyml"
            ))
            .ok()
            .and_then(|armor| armor.pio.as_map().ok()?.get("BaseDefense")?.as_i32().ok());
        let model_info_path =
            format!("Component/ModelInfo/{template_actor}.engine__component__ModelInfo.bgyml");
        let model_info = pack.byml_file(&model_info_path)?;
        let model_map = model_info
            .pio
            .as_map()
            .map_err(|_| invalid_data(format!("{model_info_path} is not a map")))?;
        let string = |key: &str| -> io::Result<String> {
            model_map
                .get(key)
                .and_then(|value| value.as_string().ok())
                .map(ToString::to_string)
                .ok_or_else(|| invalid_data(format!("{model_info_path} has no {key}")))
        };
        Ok(Self {
            fmdb_name: string("FmdbName")?,
            model_project: string("ModelProjectName")?,
            pack_bytes,
            base_defense,
            dyeable,
        })
    }
}

/// Whether the TexToGo and material-animation writers this build carries can
/// produce the dye assets (they are stubs until the FMAA work lands).
pub fn dye_assets_supported() -> bool {
    let probe = TexturePatternAnim::texture_per_frame(
        "Head_ftp",
        vec![TexturePatternMaterial {
            material: "Mt_Probe".into(),
            sampler: "_a0".into(),
            textures: (0..16).map(|i| format!("Probe_Alb.{i}")).collect(),
        }],
    );
    let anim_ready = match write_material_anim_bfres("Probe.anim", &[probe]) {
        Ok(_) => true,
        Err(error) => !error.to_string().contains("not implemented"),
    };
    let empty = TexToGoFile {
        header: crate::parser::textogo::TexToGoHeader {
            header_size: 0,
            version: 0,
            width: 0,
            height: 0,
            depth: 0,
            mip_count: 0,
            format_flag: 0,
            format_setting: 0,
            component_selectors: [0; 4],
            hash: [0; 32],
            format: 0,
            texture_settings: [0; 4],
        },
        surfaces: Vec::new(),
    };
    let textures_ready = match textogo_writer::to_rgba(&empty) {
        Ok(_) => true,
        Err(error) => !error.to_string().contains("not implemented"),
    };
    anim_ready && textures_ready
}

/// Luminance-preserving tint: the pixel's brightness drives the dye colour,
/// blended with a little of the original so detail survives.
fn tint_rgba(image: &image::RgbaImage, tint: [u8; 3]) -> image::RgbaImage {
    let mut tinted = image.clone();
    for pixel in tinted.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let luminance =
            (0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)) / 255.0;
        let mix = |original: u8, channel: u8| -> u8 {
            let dyed = luminance * f32::from(channel);
            (0.15 * f32::from(original) + 0.85 * dyed)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        pixel.0 = [mix(r, tint[0]), mix(g, tint[1]), mix(b, tint[2]), a];
    }
    tinted
}

/// The icons of one upgrade rank: every finished icon of the base piece
/// (`<base>.bntx.zs` and its `<base>_<Color>` dye variants) is cloned under
/// `<rank>` / `<rank>_<Color>` with only the texture name swapped, the way
/// `Armor_002_Head`'s icons are pixel-identical copies of `Armor_001_Head`'s.
/// The image payload the base already produced (custom PNG, dye tint, ASTC
/// encode) is reused as is.
fn clone_rank_icons(
    clean_romfs: &Path,
    output_romfs: &Path,
    base_icons: &[UiTextureReport],
    base_actor: &str,
    rank_actor: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<Vec<UiTextureReport>> {
    if base_icons.is_empty() {
        return Err(invalid_data(format!(
            "no base icons were generated for {base_actor}; cannot derive {rank_actor}'s"
        )));
    }
    let mut reports = Vec::with_capacity(base_icons.len());
    for base in base_icons {
        let suffix = match base.name.strip_prefix(base_actor) {
            Some("") => "",
            Some(rest) if rest.starts_with('_') => rest,
            _ => {
                return Err(invalid_data(format!(
                    "base icon {} is not named after {base_actor}",
                    base.name
                )))
            }
        };
        let name = format!("{rank_actor}{suffix}");
        let destination = format!("UI/Tex/Icon/{name}.bntx.zs");
        let request = assets::WeaponBntxAssetRequest {
            // Absolute: the source is the base's finished icon in the output
            // tree, not a vanilla file.
            texture_source: base.destination.clone(),
            png_source: None,
            new_name: name,
            texture_destination: PathBuf::from(&destination),
        };
        let report = request.generate(clean_romfs, output_romfs, zstd.clone())?;
        reports.push(UiTextureReport {
            destination: output_romfs.join(&destination),
            name: report.name,
            format: report.format,
            width: report.width,
            height: report.height,
            png_applied: base.png_applied,
            similarity: report.similarity,
            warning: base.warning.clone(),
        });
    }
    Ok(reports)
}

/// The fifteen `<actor>_<Color>` icons for a piece whose template ships only
/// the undyed icon: the base picture (the custom PNG or the template's icon)
/// is tinted per dye colour and pushed through the normal icon path.
fn generate_dye_icons(
    clean_romfs: &Path,
    output_romfs: &Path,
    template_actor: &str,
    actor_name: &str,
    custom_icon: Option<&Path>,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<Vec<UiTextureReport>> {
    let source = format!("UI/Tex/Icon/{template_actor}.bntx.zs");
    let base = match custom_icon {
        Some(png) => image::open(png)
            .map_err(|error| invalid_data(format!("{}: {error}", png.display())))?
            .to_rgba8(),
        None => {
            let path = clean_romfs.join(&source);
            let bytes = fs::read(&path)?;
            let raw = if crate::Settings::Magic::is_bntx(&bytes) {
                bytes
            } else {
                zstd.try_decompress_for_path(&path, &bytes)?.0
            };
            let bntx = crate::parser::bntx::BntxFile::parse(&raw)
                .map_err(|error| invalid_data(error.to_string()))?;
            bntx.decode_texture(0)
                .map_err(|error| invalid_data(error.to_string()))?
        }
    };
    let scratch = output_romfs
        .parent()
        .unwrap_or(output_romfs)
        .join("_dye_tmp")
        .join(actor_name);
    fs::create_dir_all(&scratch)?;
    let mut reports = Vec::with_capacity(DYE_COLORS.len());
    let result = (|| -> io::Result<()> {
        for (color, tint) in DYE_COLORS {
            let name = format!("{actor_name}_{color}");
            let png = scratch.join(format!("{name}.png"));
            tint_rgba(&base, tint)
                .save(&png)
                .map_err(|error| invalid_data(format!("{}: {error}", png.display())))?;
            reports.push(super::generate_ui_texture(
                clean_romfs,
                output_romfs,
                source.clone(),
                format!("UI/Tex/Icon/{name}.bntx.zs"),
                name,
                Some(png),
                zstd.clone(),
            )?);
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&scratch);
    // Drop the shared `_dye_tmp` parent too once the last actor is done with it.
    if let Some(parent) = scratch.parent() {
        let _ = fs::remove_dir(parent);
    }
    result?;
    Ok(reports)
}

/// The shared per-slot `ColorVariationParam` document, taken from the
/// vanilla Hylian piece of the same slot and checked to name `<Slot>_ftp`.
fn color_variation_entry(
    clean_romfs: &Path,
    slot: ArmorSlot,
    entry_name: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<(String, Vec<u8>)> {
    let donor = format!("Armor_001{}", slot.suffix());
    let path = clean_romfs
        .join("Pack/Actor")
        .join(format!("{donor}.pack.zs"));
    let pack = PackFile::from_binary(&fs::read(&path)?, zstd)?;
    let data = pack
        .sarc
        .get_data(entry_name)
        .ok_or_else(|| invalid_data(format!("{donor} pack has no {entry_name}")))?
        .to_vec();
    let document = pack.byml_file(entry_name)?;
    let expected = format!("{}_ftp", slot.suffix().trim_start_matches('_'));
    let anim = document
        .pio
        .as_map()
        .ok()
        .and_then(|map| map.get("VariationAnim"))
        .and_then(|value| value.as_array().ok())
        .and_then(|values| values.first())
        .and_then(|value| value.as_string().ok())
        .map(ToString::to_string);
    if anim.as_deref() != Some(expected.as_str()) {
        return Err(invalid_data(format!(
            "{donor}'s ColorVariationParam plays {anim:?}, expected {expected}"
        )));
    }
    Ok((entry_name.to_owned(), data))
}

/// Writes `<name>_Alb.0`..`.15` for every `_Alb` texture the model's
/// materials reference (slice 0 untinted) and points the materials at slice
/// 0. Returns `(material, albedo sampler, renamed albedo base)` per material.
fn write_dye_slices(
    file: &mut ResFile,
    texture_sources: &BTreeMap<String, PathBuf>,
    texture_output: &Path,
    template_project: &str,
    project: &str,
    copied: &mut Vec<PathBuf>,
) -> io::Result<Vec<(String, String, String)>> {
    let mut written: BTreeMap<String, ()> = BTreeMap::new();
    let mut materials = Vec::new();
    for model in &mut file.models {
        for material in &mut model.materials {
            for (index, texture) in material.texture_refs.iter_mut().enumerate() {
                if !texture.ends_with("_Alb") {
                    continue;
                }
                let old = texture.clone();
                let new_base = replace_project(&old, template_project, project);
                if !written.contains_key(&old) {
                    let source =
                        texture_sources
                            .get(&old.to_ascii_lowercase())
                            .ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::NotFound,
                                    format!("albedo texture {old}.txtg is missing from TexToGo"),
                                )
                            })?;
                    let bytes = fs::read(source)?;
                    let parsed = TexToGoFile::parse(&bytes)
                        .map_err(|error| invalid_data(format!("{old}: {error}")))?;
                    let picture = textogo_writer::to_rgba(&parsed)
                        .map_err(|error| invalid_data(format!("{old}: {error}")))?;
                    for (slice, tint) in std::iter::once(None)
                        .chain(DYE_COLORS.iter().map(|(_, tint)| Some(*tint)))
                        .enumerate()
                    {
                        let image = match tint {
                            Some(tint) => tint_rgba(&picture, tint),
                            None => picture.clone(),
                        };
                        let encoded = crate::tools::txtg_edit::encode_replacement(
                            &parsed,
                            &image,
                            crate::tools::txtg_edit::SurfaceStyle::Compact,
                        )
                        .map_err(|error| {
                            io::Error::new(error.kind(), format!("{old}.{slice}: {error}"))
                        })?;
                        let destination = texture_output.join(format!("{new_base}.{slice}.txtg"));
                        fs::write(&destination, encoded)?;
                        copied.push(destination);
                    }
                    written.insert(old.clone(), ());
                }
                *texture = format!("{old}.0");
                let sampler = material
                    .samplers
                    .get(index)
                    .map(|sampler| sampler.name.clone())
                    .unwrap_or_else(|| "_a0".to_owned());
                materials.push((material.name.clone(), sampler, new_base));
            }
        }
    }
    if materials.is_empty() {
        return Err(invalid(
            "the template model references no _Alb texture, so it cannot be made dyeable",
        ));
    }
    Ok(materials)
}

/// `Model/<project>.anim.bfres.zs` with the `<Slot>_ftp` texture-pattern
/// animation that steps every dyed material through its sixteen slices.
fn write_dye_anim(
    clean_romfs: &Path,
    output_romfs: &Path,
    project: &str,
    slot: ArmorSlot,
    materials: &[(String, String, String)],
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<PathBuf> {
    // `FrameCount` stores the last frame index (15 for sixteen slices), exactly
    // like the vanilla `<Slot>_ftp` animations.
    let name = slot_anim_name(slot);
    let anim = TexturePatternAnim::texture_per_frame(
        name.clone(),
        materials
            .iter()
            .map(|(material, sampler, albedo)| TexturePatternMaterial {
                material: material.clone(),
                sampler: sampler.clone(),
                textures: (0..16).map(|slice| format!("{albedo}.{slice}")).collect(),
            })
            .collect(),
    );
    write_project_anim(clean_romfs, output_romfs, project, vec![anim], &name, zstd)
}

/// The texture-pattern animation the dye shop plays for a slot (`Head_ftp`).
fn slot_anim_name(slot: ArmorSlot) -> String {
    format!("{}_ftp", slot.suffix().trim_start_matches('_'))
}

/// Writes `Model/<project>.anim.bfres.zs`. An earlier piece of the same
/// project in this run may have written the file already: its animations are
/// kept, `own` (this piece's slot animation) replaces the one of that name,
/// and the other given animations only fill in names still missing.
fn write_project_anim(
    clean_romfs: &Path,
    output_romfs: &Path,
    project: &str,
    anims: Vec<TexturePatternAnim>,
    own: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<PathBuf> {
    let destination = output_romfs
        .join("Model")
        .join(format!("{project}.anim.bfres.zs"));
    let mut merged = if destination.is_file() {
        let compressed = fs::read(&destination)?;
        let (raw, _) = zstd.try_decompress_for_path(&destination, &compressed)?;
        let (_, existing) = read_material_anim_bfres(&raw).map_err(|error| {
            invalid_data(format!(
                "{} cannot be read back: {error}",
                destination.display()
            ))
        })?;
        texture_pattern_anims_only(existing, &destination.display().to_string())
    } else {
        Vec::new()
    };
    for anim in anims {
        match merged
            .iter()
            .position(|existing| existing.name == anim.name)
        {
            Some(index) if anim.name == own => merged[index] = anim,
            Some(_) => {}
            None => merged.push(anim),
        }
    }
    let raw = write_material_anim_bfres(&format!("{project}.anim"), &merged)
        .map_err(|error| invalid_data(format!("failed to write the dye animation: {error}")))?;
    // Same container settings as the vanilla project animations.
    let reference = clean_romfs.join("Model/Armor_001.anim.bfres.zs");
    let dictionary = fs::read(&reference)
        .ok()
        .and_then(|bytes| zstd.try_decompress_for_path(&reference, &bytes).ok())
        .map(|(_, dictionary)| dictionary)
        .unwrap_or(ZstdDictionary::Zs);
    let output = match dictionary {
        ZstdDictionary::None => raw,
        ZstdDictionary::Yaz0 => TotkZstd::compress_yaz0_with_alignment(&raw, 0)?,
        other => zstd.compress_with_dictionary(&raw, other)?,
    };
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&destination, output)?;
    Ok(destination)
}

/// Drops the animations the material-animation writer cannot emit (shader
/// parameter animations such as the `_fsp` light toggles read back with no
/// texture-pattern material), warning about each one.
fn texture_pattern_anims_only(
    anims: Vec<TexturePatternAnim>,
    source: &str,
) -> Vec<TexturePatternAnim> {
    anims
        .into_iter()
        .filter(|anim| {
            if anim.materials.is_empty() {
                eprintln!(
                    "warning: {source}: dropping {} (not a texture-pattern animation)",
                    anim.name
                );
                return false;
            }
            true
        })
        .collect()
}

/// `Model/<project>.anim.bfres.zs` holds the dye material animations shared by
/// every piece of a project. A dyeable template's file is cloned under the
/// new project: with names of equal length the vanilla bytes are patched in
/// place (string pool intact, every animation kept); otherwise, or when an
/// earlier piece of the project already wrote the file, the texture-pattern
/// animations are read, renamed and rewritten (shader-parameter animations,
/// which the writer cannot emit, are dropped with a warning).
fn clone_project_anim(
    clean_romfs: &Path,
    output_romfs: &Path,
    template_project: &str,
    project: &str,
    slot: ArmorSlot,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<Option<PathBuf>> {
    let source = clean_romfs
        .join("Model")
        .join(format!("{template_project}.anim.bfres.zs"));
    if !source.is_file() {
        return Ok(None);
    }
    let destination = output_romfs
        .join("Model")
        .join(format!("{project}.anim.bfres.zs"));
    let compressed = fs::read(&source)?;
    let (raw, dictionary) = zstd.try_decompress_for_path(&source, &compressed)?;
    if template_project.len() != project.len() || destination.is_file() {
        let (_, anims) = read_material_anim_bfres(&raw).map_err(|error| {
            invalid_data(format!("{} cannot be read: {error}", source.display()))
        })?;
        let anims: Vec<TexturePatternAnim> =
            texture_pattern_anims_only(anims, &source.display().to_string())
                .into_iter()
                .map(|mut anim| {
                    anim.name = replace_project(&anim.name, template_project, project);
                    for material in &mut anim.materials {
                        material.material =
                            replace_project(&material.material, template_project, project);
                        for texture in &mut material.textures {
                            *texture = replace_project(texture, template_project, project);
                        }
                    }
                    anim
                })
                .collect();
        if anims.is_empty() {
            return Ok(None);
        }
        return write_project_anim(
            clean_romfs,
            output_romfs,
            project,
            anims,
            &slot_anim_name(slot),
            zstd,
        )
        .map(Some);
    }
    let patched = replace_bytes(&raw, template_project.as_bytes(), project.as_bytes());
    let output = match dictionary {
        ZstdDictionary::None => patched,
        ZstdDictionary::Yaz0 => TotkZstd::compress_yaz0_with_alignment(&patched, 0)?,
        other => zstd.compress_with_dictionary(&patched, other)?,
    };
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&destination, output)?;
    Ok(Some(destination))
}

/// `UI/Tex/Icon/<template>.bntx.zs` plus its `_<DyeColor>` variants, mapped to
/// `(source, destination, new texture name)`.
fn icon_variants(
    clean_romfs: &Path,
    template_actor: &str,
    actor_name: &str,
) -> io::Result<Vec<(String, String, String)>> {
    let icon_dir = clean_romfs.join("UI/Tex/Icon");
    let mut variants = Vec::new();
    for entry in fs::read_dir(&icon_dir)? {
        let path = entry?.path();
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(stem) = file_name.strip_suffix(".bntx.zs") else {
            continue;
        };
        let suffix = match stem.strip_prefix(template_actor) {
            Some("") => "",
            Some(rest) if rest.starts_with('_') => rest,
            _ => continue,
        };
        // `Armor_001_Head_B` style variants are actors of their own, not dyes.
        if suffix.len() == 2 && suffix.as_bytes()[1].is_ascii_uppercase() {
            continue;
        }
        let name = format!("{actor_name}{suffix}");
        variants.push((
            format!("UI/Tex/Icon/{file_name}"),
            format!("UI/Tex/Icon/{name}.bntx.zs"),
            name,
        ));
    }
    if variants.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no icon BNTX found for template {template_actor}"),
        ));
    }
    variants.sort();
    Ok(variants)
}

/// Replaces `from` with `to` wherever it occurs as a whole project token, that
/// is not followed by another alphanumeric character (`Armor_115` must not
/// touch `Armor_1152`).
pub(crate) fn replace_project(value: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return value.to_owned();
    }
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(position) = rest.find(from) {
        let end = position + from.len();
        let bounded = rest[end..]
            .chars()
            .next()
            .map_or(true, |next| !next.is_ascii_alphanumeric());
        result.push_str(&rest[..position]);
        if bounded {
            result.push_str(to);
        } else {
            result.push_str(from);
        }
        rest = &rest[end..];
    }
    result.push_str(rest);
    result
}

fn rename_project_strings(value: &mut Byml, from: &str, to: &str) {
    match value {
        Byml::String(text) => {
            let renamed = replace_project(text, from, to);
            if renamed != text.as_str() {
                *text = renamed.into();
            }
        }
        Byml::Array(items) => {
            for item in items {
                rename_project_strings(item, from, to);
            }
        }
        Byml::Map(map) => {
            for (_, item) in map.iter_mut() {
                rename_project_strings(item, from, to);
            }
        }
        _ => {}
    }
}

fn replace_bytes(data: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    debug_assert_eq!(from.len(), to.len());
    let mut result = data.to_vec();
    if from.is_empty() || from.len() != to.len() {
        return result;
    }
    let mut index = 0;
    while index + from.len() <= result.len() {
        if &result[index..index + from.len()] == from
            && result
                .get(index + from.len())
                .map_or(true, |next| !next.is_ascii_alphanumeric())
        {
            result[index..index + to.len()].copy_from_slice(to);
            index += from.len();
        } else {
            index += 1;
        }
    }
    result
}

fn map_child_mut<'a>(value: &'a mut Byml, key: &str) -> io::Result<&'a mut roead::byml::Map> {
    value
        .as_mut_map()
        .map_err(|_| invalid_data("BYML root is not a map"))?
        .get_mut(key)
        .ok_or_else(|| invalid_data(format!("BYML has no {key} map")))?
        .as_mut_map()
        .map_err(|_| invalid_data(format!("BYML {key} is not a map")))
}

fn validate_actor_name(actor: &str) -> io::Result<()> {
    if actor.is_empty()
        || !actor
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(invalid(format!("invalid actor name: {actor}")));
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

    fn spec(upgrades: Vec<ArmorUpgradeSpec>) -> ArmorSpec {
        ArmorSpec {
            actor_name: "Armor_950_Head".into(),
            template_actor: "Armor_005_Head".into(),
            display_name: "Test Cap".into(),
            description: "A cap for tests.".into(),
            defense: Some(4),
            series_name: None,
            model: None,
            physics: Vec::new(),
            helper_bone: None,
            replace_bones: false,
            assets: ArmorAssets::default(),
            vendors: Vec::new(),
            upgrades,
            upgrades_enabled: true,
            dyeable: false,
            skin_material: None,
            armor_effects: Vec::new(),
            sound: None,
            effect: None,
            effect_keys: Vec::new(),
        }
    }

    fn romfs_zstd() -> Option<(&'static Path, Arc<TotkZstd<'static>>)> {
        use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        if !clean_romfs.is_dir() {
            return None;
        }
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).ok()?);
        Some((clean_romfs, zstd))
    }

    fn upgrade(defense: i32) -> ArmorUpgradeSpec {
        ArmorUpgradeSpec {
            defense,
            rupees: 10,
            materials: vec![ArmorUpgradeMaterial {
                actor: "Item_Fruit_K".into(),
                count: 3,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn physics_accepts_a_string_a_list_or_null() {
        let parse = |physics: &str| -> Vec<String> {
            let text = format!(
                r#"{{"actor_name":"Armor_950_Head","template_actor":"Armor_005_Head","display_name":"x","description":"y","physics":{physics}}}"#
            );
            serde_json::from_str::<ArmorSpec>(&text).unwrap().physics
        };
        assert_eq!(parse(r#""Armor_005_Head""#), ["Armor_005_Head"]);
        assert_eq!(
            parse(r#"["Armor_005_Head","Armor_180_Upper"]"#),
            ["Armor_005_Head", "Armor_180_Upper"]
        );
        assert!(parse("null").is_empty());
        assert!(spec(Vec::new()).physics.is_empty());
    }

    #[test]
    fn rank_names_append_the_step_or_use_explicit_names() {
        let mut spec = spec(vec![upgrade(5), upgrade(8), upgrade(12)]);
        assert_eq!(
            spec.rank_actor_names(),
            ["Armor_950_Head_1", "Armor_950_Head_2", "Armor_950_Head_3"]
        );
        spec.upgrades[1].actor_name = Some("Armor_960_Head".into());
        assert_eq!(
            spec.rank_actor_names(),
            ["Armor_950_Head_1", "Armor_960_Head", "Armor_950_Head_3"]
        );
        spec.actor_name = "Armor_Custom_Head".into();
        assert_eq!(
            spec.rank_actor_names(),
            [
                "Armor_Custom_Head_1",
                "Armor_960_Head",
                "Armor_Custom_Head_3"
            ]
        );
        // Long rank names and names without the slot suffix are fine.
        let mut long = spec.clone();
        long.actor_name = "Armor_950_Head".into();
        long.upgrades[0].actor_name = Some("Armor_950_Head_Rank2_Long".into());
        long.upgrades[2].actor_name = Some("Armor_950_Star4".into());
        assert!(long.validate(Path::new(".")).is_ok());
    }

    #[test]
    fn disabled_upgrades_yield_no_ranks_and_explicit_ones_pass_through() {
        let Some((clean_romfs, zstd)) = romfs_zstd() else {
            return;
        };
        let mut explicit = spec(vec![upgrade(5), upgrade(8)]);
        assert_eq!(
            explicit
                .effective_upgrades(clean_romfs, zstd.clone())
                .unwrap(),
            explicit.upgrades
        );
        explicit.upgrades_enabled = false;
        assert!(explicit
            .effective_upgrades(clean_romfs, zstd.clone())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn default_upgrades_copy_the_template_chain_or_the_hylian_one() {
        let Some((clean_romfs, zstd)) = romfs_zstd() else {
            return;
        };
        // Armor_005_Head → Armor_035_Head → ... : the template's own chain.
        let korok = spec(Vec::new());
        let derived = korok.effective_upgrades(clean_romfs, zstd.clone()).unwrap();
        assert_eq!(derived.len(), 4, "{derived:?}");
        assert!(derived.iter().all(|rank| !rank.materials.is_empty()));
        assert!(derived
            .windows(2)
            .all(|pair| pair[0].defense < pair[1].defense));
        assert_eq!(derived[0].rupees, 10);
        assert_eq!(derived[0].materials[0].actor, "Item_Fruit_K");
        // Armor_022_Head has no chain: Hylian materials, defense 3 + deltas.
        let mut mask = spec(Vec::new());
        mask.template_actor = "Armor_022_Head".into();
        mask.defense = Some(3);
        let derived = mask.effective_upgrades(clean_romfs, zstd).unwrap();
        assert_eq!(
            derived.iter().map(|rank| rank.defense).collect::<Vec<_>>(),
            [5, 8, 12, 20]
        );
        assert_eq!(derived[0].materials[0].actor, "Item_Enemy_77");
        assert_eq!(derived[0].materials[0].count, 5);
        assert_eq!(derived[0].rupees, 10);
        assert_eq!(derived[3].rupees, 500);
    }

    #[test]
    fn upgrade_validation_rejects_bad_ranks() {
        let root = Path::new(".");
        assert!(spec(vec![upgrade(5), upgrade(8)]).validate(root).is_ok());
        assert!(spec(vec![upgrade(1); 5]).validate(root).is_err());
        let mut negative = spec(vec![upgrade(-1)]);
        assert!(negative.validate(root).is_err());
        negative.upgrades[0].defense = 5;
        negative.upgrades[0].rupees = -5;
        assert!(negative.validate(root).is_err());
        let mut count = spec(vec![upgrade(5)]);
        count.upgrades[0].materials[0].count = 0;
        assert!(count.validate(root).is_err());
        let mut same = spec(vec![upgrade(5)]);
        same.upgrades[0].actor_name = Some("Armor_950_Head".into());
        assert!(same.validate(root).is_err());
        let mut twice = spec(vec![upgrade(5), upgrade(8)]);
        twice.upgrades[1].actor_name = Some("Armor_950_Head_1".into());
        assert!(twice.validate(root).is_err());
    }

    #[test]
    fn enhancement_cost_document_matches_the_vanilla_shape() {
        let byml = enhancement_cost_byml(&upgrade(5));
        let map = byml.as_map().unwrap();
        assert_eq!(map.get("Price").unwrap().as_i32().unwrap(), 10);
        let items = map.get("Items").unwrap().as_array().unwrap();
        let item = items[0].as_map().unwrap();
        assert_eq!(
            item.get("Actor").unwrap().as_string().unwrap().as_str(),
            "Work/Actor/Item_Fruit_K.engine__actor__ActorParam.gyml"
        );
        assert_eq!(item.get("Number").unwrap().as_i32().unwrap(), 3);
    }

    /// Both visibility lists come straight from the chosen vanilla actor's
    /// ArmorParam (the Hylian tunic hides the torso skin and covers the belt
    /// set; the Korok mask hides no skin but covers `G_Head`; unknown actors
    /// are refused).
    #[test]
    #[ignore = "needs the TOTK dump"]
    fn skin_material_groups_come_from_the_chosen_actor() {
        let Some((clean_romfs, zstd)) = romfs_zstd() else {
            return;
        };
        let tunic = material_visibility(clean_romfs, "Armor_001_Upper", zstd.clone()).unwrap();
        let names: Vec<String> = tunic
            .hide_group_names
            .as_ref()
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_string().unwrap().to_string())
            .collect();
        assert!(
            names.iter().any(|name| name == "G_UpperBeltSet"),
            "{names:?}"
        );
        let groups = tunic.hidden_groups;
        let list = groups.as_array().unwrap();
        assert_eq!(list.len(), 1, "{groups:?}");
        let group = list[0].as_map().unwrap();
        assert_eq!(
            group
                .get("GroupName")
                .unwrap()
                .as_string()
                .unwrap()
                .as_str(),
            "G_Skin"
        );
        assert!(group
            .get("MaterialNameList")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name
                .as_string()
                .is_ok_and(|n| n.as_str() == "Mt_Upper_Skin")));
        let mask = material_visibility(clean_romfs, "Armor_005_Head", zstd.clone()).unwrap();
        assert!(mask.hidden_groups.as_array().unwrap().is_empty());
        let names = mask.hide_group_names.unwrap();
        assert_eq!(names.as_array().unwrap().len(), 1);
        assert_eq!(
            names.as_array().unwrap()[0].as_string().unwrap().as_str(),
            "G_Head"
        );
        assert!(material_visibility(clean_romfs, "Armor_Missing_Head", zstd).is_err());
    }

    /// Generates a Korok-mask clone with two Great Fairy ranks from the real
    /// RomFS and checks the rank chain across packs, PouchActorInfo and
    /// EnhancementMaterialInfo.
    #[test]
    #[ignore = "needs the TOTK dump"]
    fn generates_upgrade_rank_actors_from_romfs() {
        let Some((clean_romfs, zstd)) = romfs_zstd() else {
            return;
        };
        let output_root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_CLAUDE/armor_upgrade_test");
        if output_root.is_dir() {
            fs::remove_dir_all(&output_root).unwrap();
        }
        let output_romfs = output_root.join("romfs");
        fs::create_dir_all(&output_romfs).unwrap();

        let mut rank2 = upgrade(6);
        rank2.rupees = 25;
        rank2.materials.push(ArmorUpgradeMaterial {
            actor: "Item_Enemy_53".into(),
            count: 2,
        });
        let mut rank3 = upgrade(9);
        rank3.rupees = 100;
        rank3.activate_set_bonus = true;
        let spec = spec(vec![rank2, rank3]);
        let report = spec
            .generate_files(clean_romfs, &output_romfs, &output_root, zstd.clone())
            .unwrap();
        assert_eq!(report.upgrades.len(), 2);
        let base = "Armor_950_Head";
        let ranks = ["Armor_950_Head_1", "Armor_950_Head_2"];

        let read_pack = |actor: &str| {
            let path = output_romfs
                .join("Pack/Actor")
                .join(format!("{actor}.pack.zs"));
            assert!(path.is_file(), "{}", path.display());
            PackFile::from_binary(&fs::read(&path).unwrap(), zstd.clone()).unwrap()
        };
        let armor_of = |pack: &PackFile<'_>, actor: &str| {
            pack.byml_file(&format!(
                "Component/ArmorParam/{actor}.game__component__ArmorParam.bgyml"
            ))
            .unwrap()
            .pio
        };
        let string_of = |value: &Byml, key: &str| -> Option<String> {
            value
                .as_map()
                .ok()?
                .get(key)?
                .as_string()
                .ok()
                .map(ToString::to_string)
        };

        let base_pack = read_pack(base);
        let base_armor = armor_of(&base_pack, base);
        assert_eq!(
            string_of(&base_armor, "NextRankActor").as_deref(),
            Some("Work/Actor/Armor_950_Head_1.engine__actor__ActorParam.gyml")
        );
        let rank2_pack = read_pack(ranks[0]);
        assert!(rank2_pack
            .sarc
            .get_data(&format!("Actor/{base}.engine__actor__ActorParam.bgyml"))
            .is_some());
        let rank2_armor = armor_of(&rank2_pack, ranks[0]);
        assert_eq!(
            string_of(&rank2_armor, "$parent").as_deref(),
            Some("Work/Component/ArmorParam/Armor_950_Head.game__component__ArmorParam.gyml")
        );
        assert_eq!(
            rank2_armor
                .as_map()
                .unwrap()
                .get("Rank")
                .unwrap()
                .as_i32()
                .unwrap(),
            2
        );
        assert_eq!(
            rank2_armor
                .as_map()
                .unwrap()
                .get("BaseDefense")
                .unwrap()
                .as_i32()
                .unwrap(),
            6
        );
        assert_eq!(
            string_of(&rank2_armor, "NextRankActor").as_deref(),
            Some("Work/Actor/Armor_950_Head_2.engine__actor__ActorParam.gyml")
        );
        let rank2_actor = rank2_pack
            .byml_file(&format!(
                "Actor/{}.engine__actor__ActorParam.bgyml",
                ranks[0]
            ))
            .unwrap()
            .pio;
        assert_eq!(
            string_of(&rank2_actor, "$parent").as_deref(),
            Some("Work/Actor/Armor_950_Head.engine__actor__ActorParam.gyml")
        );
        let rank3_pack = read_pack(ranks[1]);
        let rank3_armor = armor_of(&rank3_pack, ranks[1]);
        assert!(string_of(&rank3_armor, "NextRankActor").is_none());
        assert_eq!(
            rank3_armor
                .as_map()
                .unwrap()
                .get("Rank")
                .unwrap()
                .as_i32()
                .unwrap(),
            3
        );
        assert!(rank3_pack
            .sarc
            .get_data(&format!(
                "GameParameter/EnhancementMaterial/{}.game__pouchcontent__EnhancementMaterial.bgyml",
                ranks[1]
            ))
            .is_none());
        let rank2_cost = rank2_pack
            .byml_file(&format!(
                "GameParameter/EnhancementMaterial/{}.game__pouchcontent__EnhancementMaterial.bgyml",
                ranks[0]
            ))
            .unwrap()
            .pio;
        assert_eq!(
            rank2_cost
                .as_map()
                .unwrap()
                .get("Price")
                .unwrap()
                .as_i32()
                .unwrap(),
            100
        );

        let (version, _) = super::super::version::discover_product_file(
            &clean_romfs.join("RSDB"),
            "ActorInfo.Product.",
            ".rstbl.byml.zs",
        )
        .unwrap();
        let rows = |product: &str| {
            let name = rsdb::WeaponRsdbProcessor::versioned_rsdb_name(product, &version).unwrap();
            BymlFile::new(output_romfs.join("RSDB").join(name), zstd.clone())
                .unwrap()
                .pio
        };
        let pouch = rows("PouchActorInfo");
        let pouch_row = |actor: &str| -> roead::byml::Map {
            pouch
                .as_array()
                .unwrap()
                .iter()
                .find(|row| string_of(row, "__RowId").as_deref() == Some(actor))
                .unwrap()
                .as_map()
                .unwrap()
                .clone()
        };
        for (actor, rank, next) in [
            (base, 1, Some(ranks[0])),
            (ranks[0], 2, Some(ranks[1])),
            (ranks[1], 3, None),
        ] {
            let row = pouch_row(actor);
            assert_eq!(
                row.get("ArmorRank").unwrap().as_i32().unwrap(),
                rank,
                "{actor}"
            );
            let next_ref = row
                .get("ArmorNextRankActor")
                .map(|value| value.as_string().unwrap().to_string());
            assert_eq!(next_ref, next.map(actor_param_work_path), "{actor}");
        }
        assert_eq!(
            pouch_row(ranks[0])
                .get("EquipmentPerformance")
                .unwrap()
                .as_i32()
                .unwrap(),
            6
        );
        let enhancement = rows("EnhancementMaterialInfo");
        let cost_row = |actor: &str| {
            enhancement
                .as_array()
                .unwrap()
                .iter()
                .find(|row| {
                    string_of(row, "__RowId").as_deref()
                        == Some(actor_param_work_path(actor).as_str())
                })
                .cloned()
        };
        let base_cost = cost_row(base).unwrap();
        assert_eq!(
            base_cost
                .as_map()
                .unwrap()
                .get("Price")
                .unwrap()
                .as_i32()
                .unwrap(),
            25
        );
        assert_eq!(
            base_cost
                .as_map()
                .unwrap()
                .get("Items")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let rank2_row = cost_row(ranks[0]).unwrap();
        assert_eq!(
            rank2_row
                .as_map()
                .unwrap()
                .get("Price")
                .unwrap()
                .as_i32()
                .unwrap(),
            100
        );
        assert!(cost_row(ranks[1]).is_none());
        for actor in ranks {
            assert!(output_romfs
                .join("UI/Tex/Icon")
                .join(format!("{actor}.bntx.zs"))
                .is_file());
            assert!(output_romfs
                .join("UI/Tex/Icon")
                .join(format!("{actor}_Blue.bntx.zs"))
                .is_file());
        }
    }

    /// A dyeable template whose project name is longer than the new one
    /// (Armor_1091 -> Armor_760) cannot be byte-patched: the vanilla
    /// animations are rewritten under the new project, and a piece written
    /// earlier for the same project keeps its own slot animation.
    #[test]
    #[ignore = "needs the TOTK dump"]
    fn clones_a_dyeable_template_anim_under_a_project_of_another_length() {
        use crate::file_format::Model3D::bfres::material_anim::read_material_anim_bfres;
        let Some((clean_romfs, zstd)) = romfs_zstd() else {
            return;
        };
        let output_romfs =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_CLAUDE/anim_clone_test/romfs");
        if output_romfs.is_dir() {
            fs::remove_dir_all(&output_romfs).unwrap();
        }
        fs::create_dir_all(&output_romfs).unwrap();
        let read = |path: &Path| {
            let compressed = fs::read(path).unwrap();
            let (raw, _) = zstd.try_decompress_for_path(path, &compressed).unwrap();
            read_material_anim_bfres(&raw).unwrap()
        };
        let (_, vanilla) = read(&clean_romfs.join("Model/Armor_1091.anim.bfres.zs"));
        assert!(vanilla.iter().any(|anim| anim.name == "Head_ftp"));

        // An Upper piece made dyeable from a single-colour template first.
        let upper = write_dye_anim(
            clean_romfs,
            &output_romfs,
            "Armor_760",
            ArmorSlot::Upper,
            &[(
                "Mt_Custom_Upper".into(),
                "_a0".into(),
                "Armor_760_Upper_Alb".into(),
            )],
            zstd.clone(),
        )
        .unwrap();
        let destination = output_romfs.join("Model/Armor_760.anim.bfres.zs");
        assert_eq!(upper, destination);

        let cloned = clone_project_anim(
            clean_romfs,
            &output_romfs,
            "Armor_1091",
            "Armor_760",
            ArmorSlot::Head,
            zstd.clone(),
        )
        .unwrap();
        assert_eq!(cloned.as_deref(), Some(destination.as_path()));
        let (name, anims) = read(&destination);
        assert_eq!(name, "Armor_760.anim");
        let head = anims.iter().find(|anim| anim.name == "Head_ftp").unwrap();
        let vanilla_head = vanilla.iter().find(|anim| anim.name == "Head_ftp").unwrap();
        assert_eq!(head.frame_count, vanilla_head.frame_count);
        assert_eq!(head.materials.len(), vanilla_head.materials.len());
        for (ours, theirs) in head.materials.iter().zip(&vanilla_head.materials) {
            assert_eq!(
                ours.material,
                replace_project(&theirs.material, "Armor_1091", "Armor_760")
            );
            assert_eq!(ours.sampler, theirs.sampler);
            assert_eq!(ours.textures.len(), 16);
            for (a, b) in ours.textures.iter().zip(&theirs.textures) {
                assert_eq!(a, &replace_project(b, "Armor_1091", "Armor_760"));
                assert!(!a.contains("Armor_1091"));
            }
        }
        // The earlier piece's own slot animation survives the clone.
        let upper = anims.iter().find(|anim| anim.name == "Upper_ftp").unwrap();
        assert_eq!(upper.materials.len(), 1);
        assert_eq!(upper.materials[0].material, "Mt_Custom_Upper");
        assert_eq!(upper.materials[0].textures[15], "Armor_760_Upper_Alb.15");
        for anim in &anims {
            assert!(!anim.name.contains("Armor_1091"));
        }
    }

    /// A Bokoblin-mask clone (single-colour template, no upgrade chain) made
    /// dyeable with the default four ranks.
    #[test]
    #[ignore = "needs the TOTK dump"]
    fn makes_a_single_colour_template_dyeable_with_default_ranks() {
        let Some((clean_romfs, zstd)) = romfs_zstd() else {
            return;
        };
        let output_root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_CLAUDE/armor_dye_test");
        if output_root.is_dir() {
            fs::remove_dir_all(&output_root).unwrap();
        }
        let output_romfs = output_root.join("romfs");
        fs::create_dir_all(&output_romfs).unwrap();
        let supported = dye_assets_supported();
        let mut spec = spec(Vec::new());
        spec.template_actor = "Armor_022_Head".into();
        spec.defense = Some(3);
        spec.dyeable = true;
        if !supported {
            println!("dye asset writers are not available: generating without dye textures");
            spec.dyeable = false;
        }
        let report = spec
            .generate_files(clean_romfs, &output_romfs, &output_root, zstd.clone())
            .unwrap();
        let ranks = [
            "Armor_950_Head_1",
            "Armor_950_Head_2",
            "Armor_950_Head_3",
            "Armor_950_Head_4",
        ];
        assert_eq!(
            report
                .upgrades
                .iter()
                .map(|rank| rank.actor_name.as_str())
                .collect::<Vec<_>>(),
            ranks
        );
        assert_eq!(
            report
                .upgrades
                .iter()
                .map(|rank| rank.defense)
                .collect::<Vec<_>>(),
            [5, 8, 12, 20]
        );
        for actor in ranks {
            assert!(output_romfs
                .join("Pack/Actor")
                .join(format!("{actor}.pack.zs"))
                .is_file());
        }
        if !supported {
            return;
        }
        assert!(report.dyeable);
        let pack = PackFile::from_binary(
            &fs::read(output_romfs.join("Pack/Actor/Armor_950_Head.pack.zs")).unwrap(),
            zstd.clone(),
        )
        .unwrap();
        let actor = pack
            .byml_file("Actor/Armor_950_Head.engine__actor__ActorParam.bgyml")
            .unwrap()
            .pio;
        assert_eq!(
            component_ref(&actor, "ColorVariationRef").unwrap(),
            "Component/ColorVariationParam/Armor_Head.game__component__ColorVariationParam.bgyml"
        );
        assert!(pack
            .sarc
            .get_data("Component/ColorVariationParam/Armor_Head.game__component__ColorVariationParam.bgyml")
            .is_some());
        let model_info = pack
            .byml_file("Component/ModelInfo/Armor_950_Head.engine__component__ModelInfo.bgyml")
            .unwrap()
            .pio;
        let anims = model_info
            .as_map()
            .unwrap()
            .get("ModelVariationAnims")
            .unwrap()
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(anims.len(), 3);
        assert_eq!(
            anims[0]
                .as_map()
                .unwrap()
                .get("Fmab")
                .unwrap()
                .as_string()
                .unwrap()
                .as_str(),
            "Work/Model/Player/Armor/Armor_950/output/Head_ftp.fmab"
        );
        let (version, _) = super::super::version::discover_product_file(
            &clean_romfs.join("RSDB"),
            "ActorInfo.Product.",
            ".rstbl.byml.zs",
        )
        .unwrap();
        let pouch_name =
            rsdb::WeaponRsdbProcessor::versioned_rsdb_name("PouchActorInfo", &version).unwrap();
        let pouch = BymlFile::new(output_romfs.join("RSDB").join(pouch_name), zstd.clone())
            .unwrap()
            .pio;
        for actor in ["Armor_950_Head", "Armor_950_Head_1", "Armor_950_Head_4"] {
            let row = pouch
                .as_array()
                .unwrap()
                .iter()
                .find(|row| {
                    row.as_map()
                        .ok()
                        .and_then(|map| map.get("__RowId"))
                        .and_then(|value| value.as_string().ok())
                        .map(|value| value.as_str())
                        == Some(actor)
                })
                .unwrap();
            assert_eq!(
                row.as_map()
                    .unwrap()
                    .get("ColorVariationType")
                    .unwrap()
                    .as_string()
                    .unwrap()
                    .as_str(),
                "ArmorDye",
                "{actor}"
            );
        }
        for (color, _) in DYE_COLORS {
            assert!(output_romfs
                .join("UI/Tex/Icon")
                .join(format!("Armor_950_Head_{color}.bntx.zs"))
                .is_file());
        }
        for slice in 0..16 {
            assert!(output_romfs
                .join("TexToGo")
                .join(format!("Armor_950_Head_Alb.{slice}.txtg"))
                .is_file());
        }
        let anim_path = output_romfs.join("Model/Armor_950.anim.bfres.zs");
        assert!(anim_path.is_file());
        assert_eq!(report.model_anim.as_deref(), Some(anim_path.as_path()));
        let compressed = fs::read(&anim_path).unwrap();
        let (raw, _) = zstd
            .try_decompress_for_path(&anim_path, &compressed)
            .unwrap();
        let (_, anims) =
            crate::file_format::Model3D::bfres::material_anim::read_material_anim_bfres(&raw)
                .unwrap();
        assert_eq!(anims.len(), 1);
        assert_eq!(anims[0].name, "Head_ftp");
        assert_eq!(anims[0].frame_count, 15);
        assert!(anims[0]
            .materials
            .iter()
            .all(|material| material.textures.len() == 16
                && material.textures[0].ends_with("_Alb.0")));
        assert!(
            !fs::read_dir(output_root.join("_dye_tmp")).is_ok_and(|mut dir| dir.next().is_some())
        );
    }
}
