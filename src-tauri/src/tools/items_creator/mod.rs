//! Planning and validation primitives for creating custom weapon mods.
//!
//! Binary mutation is intentionally split from planning. A weapon mod touches several
//! global databases, so callers should validate a manifest and review the generated
//! plan before any output files are written.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

pub mod actor_pack;
pub mod armor;
pub mod armor_model;
pub mod assets;
pub mod ecocat;
pub mod gamedata;
pub mod messages;
pub mod rsdb;
pub mod rstb;
pub mod sharp_info;
pub mod vendor;
mod version;

const SHARP_INFO: &str = "GameParameter/SharpInfo/Default.game__weapon__SharpInfoTable.bgyml";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WeaponKind {
    #[serde(rename = "SmallSword", alias = "small_sword")]
    SmallSword,
    #[serde(rename = "LargeSword", alias = "large_sword")]
    LargeSword,
    #[serde(rename = "Spear", alias = "spear")]
    Spear,
    #[serde(rename = "Bow", alias = "bow")]
    Bow,
    #[serde(rename = "Shield", alias = "shield")]
    Shield,
}

impl WeaponKind {
    pub(crate) fn actor_prefix(self) -> &'static str {
        match self {
            Self::SmallSword => "Weapon_Sword_",
            Self::LargeSword => "Weapon_Lsword_",
            Self::Spear => "Weapon_Spear_",
            Self::Bow => "Weapon_Bow_",
            Self::Shield => "Weapon_Shield_",
        }
    }

    pub(crate) fn actor_category(self) -> &'static str {
        match self {
            Self::SmallSword | Self::LargeSword | Self::Spear => "Weapon",
            Self::Shield => "Shield",
            Self::Bow => "Bow",
        }
    }

    pub(crate) fn parameter_component(self) -> (&'static str, &'static str) {
        match self {
            Self::SmallSword | Self::LargeSword | Self::Spear => ("WeaponRef", "WeaponParam"),
            Self::Shield => ("ShieldRef", "ShieldParam"),
            Self::Bow => ("BowRef", "BowParam"),
        }
    }

    pub(crate) fn melee_weapon_type(self) -> Option<&'static str> {
        match self {
            Self::SmallSword => Some("SmallSword"),
            Self::LargeSword => Some("LargeSword"),
            Self::Spear => Some("Spear"),
            Self::Shield | Self::Bow => None,
        }
    }

    pub(crate) fn from_actor_name(actor: &str) -> Option<Self> {
        [
            Self::SmallSword,
            Self::LargeSword,
            Self::Spear,
            Self::Shield,
            Self::Bow,
        ]
        .into_iter()
        .find(|kind| actor.starts_with(kind.actor_prefix()))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VendorTarget {
    #[serde(alias = "name", alias = "vendor")]
    /// Existing vanilla actor, for example `Npc_TripMaster_00`.
    pub actor_name: String,
    #[serde(default)]
    pub buying_price: Option<i32>,
    #[serde(default)]
    pub selling_price: Option<i32>,
    #[serde(default = "default_quantity", alias = "stock")]
    pub quantity: u32,
}

fn default_quantity() -> u32 {
    1
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeaponAssets {
    /// Optional custom FBX. The vanilla BFRES is resolved from `template_actor`.
    #[serde(default)]
    pub fbx: Option<PathBuf>,
    /// `.txtg` files copied into `romfs/TexToGo`.
    #[serde(default)]
    pub textures: Vec<PathBuf>,
    /// Optional replacement for the inventory icon image.
    #[serde(default)]
    pub icon_png: Option<PathBuf>,
    /// Optional replacement for the Hyrule Compendium small image.
    #[serde(default)]
    pub picture_book_icon_png: Option<PathBuf>,
    /// Optional replacement for the Hyrule Compendium detail image.
    #[serde(default)]
    pub picture_book_detail_png: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WeaponSpec {
    pub actor_name: String,
    #[serde(default)]
    pub kind: Option<WeaponKind>,
    /// Existing actor pack used as the structural template.
    pub template_actor: String,
    /// Model project/FMDB name. This normally equals `actor_name`.
    #[serde(default)]
    pub model_name: String,
    /// Common values written into WeaponParam, LifeParameters, and AttachmentParam.
    #[serde(default)]
    pub weapon_parameters: actor_pack::WeaponParameterOverrides,
    /// Explicit internal renames and typed parameter changes for the cloned actor pack.
    /// When empty, the standard six actor-specific files are specialized automatically.
    #[serde(default)]
    pub actor_pack: actor_pack::ActorPackPolicy,
    /// Optional standalone or vanilla-actor SLink parameter source.
    #[serde(default)]
    pub sound: Option<actor_pack::LinkParameterSource>,
    /// Optional standalone or vanilla-actor ELink parameter source.
    #[serde(default)]
    pub effect: Option<actor_pack::LinkParameterSource>,
    /// Existing vanilla actor whose Phive and Physics entries should be reused.
    #[serde(default, alias = "physics_actor")]
    pub physics: Option<String>,
    /// Existing vanilla actor whose Chemical entries should be reused.
    #[serde(default, alias = "chemical_actor")]
    pub chemical: Option<String>,
    /// Actor assigned to the first ShootableActorSettings entry.
    #[serde(default)]
    pub shootable: Option<String>,
    pub display_name: String,
    pub description: String,
    /// Short noun used by the inventory UI, such as `Bat` or `Longsword`.
    #[serde(default)]
    pub base_name: Option<String>,
    /// Fusion/attachment adjective. A placeholder is generated when omitted.
    #[serde(default, alias = "attachment_name")]
    pub attachment_adjective: Option<String>,
    /// Hyrule Compendium name; defaults to `display_name`.
    #[serde(default)]
    pub picture_book_name: Option<String>,
    /// Hyrule Compendium caption; defaults to `description`.
    #[serde(default, alias = "picture_book_caption")]
    pub picture_book_description: Option<String>,
    pub assets: WeaponAssets,
    /// Existing travelling merchants. New vendor creation is out of scope.
    #[serde(default)]
    pub vendors: Vec<VendorTarget>,
}

/// One generated inventory/compendium BNTX.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiTextureReport {
    pub destination: PathBuf,
    pub name: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    /// Set when a PNG replaced the image payload.
    pub png_applied: bool,
    pub similarity: f64,
    /// Why the supplied PNG was not applied, when it was not.
    pub warning: Option<String>,
}

/// Everything written for one weapon, before the shared RSTB pass.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponGenerationReport {
    pub actor_name: String,
    pub actor_pack: PathBuf,
    pub model: PathBuf,
    pub textures: Vec<PathBuf>,
    pub texture_names: Vec<String>,
    pub ui_textures: Vec<UiTextureReport>,
    pub messages: PathBuf,
    pub rsdb: Vec<PathBuf>,
    pub sharp_info: PathBuf,
    pub game_data: gamedata::WeaponGameDataReport,
    pub vendor_packs: Vec<vendor::VendorPackReport>,
}

/// The complete result of [`generate_weapon_mod`] / [`generate_item_mod`].
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModGenerationReport {
    pub output_romfs: PathBuf,
    pub weapons: Vec<WeaponGenerationReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub armors: Vec<armor::ArmorGenerationReport>,
    pub rstb: rstb::RstbGenerationReport,
}

/// One entry of a mixed specification list. Armor is recognised by its
/// `Armor_` actor prefix; everything else is a weapon/shield/bow.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ItemSpec {
    Weapon(WeaponSpec),
    Armor(armor::ArmorSpec),
}

impl ItemSpec {
    pub fn actor_name(&self) -> &str {
        match self {
            Self::Weapon(spec) => &spec.actor_name,
            Self::Armor(spec) => &spec.actor_name,
        }
    }

    pub fn template_actor(&self) -> &str {
        match self {
            Self::Weapon(spec) => &spec.template_actor,
            Self::Armor(spec) => &spec.template_actor,
        }
    }

    pub fn vendor_count(&self) -> usize {
        match self {
            Self::Weapon(spec) => spec.vendors.len(),
            Self::Armor(spec) => spec.vendors.len(),
        }
    }

    pub fn validate(&self, asset_root: &Path) -> io::Result<()> {
        match self {
            Self::Weapon(spec) => spec.validate(asset_root),
            Self::Armor(spec) => spec.validate(asset_root),
        }
    }
}

/// Like [`load_specs`], but every entry whose `actor_name` starts with
/// `Armor_` is read as an [`armor::ArmorSpec`].
pub fn load_item_specs(path: &Path) -> io::Result<Vec<ItemSpec>> {
    let text = fs::read_to_string(path)?;
    let is_toml = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("toml"));
    if is_toml {
        return WeaponSpec::from_toml(&text).map(|spec| vec![ItemSpec::Weapon(spec)]);
    }
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let values = match value {
        serde_json::Value::Array(values) => values,
        other => vec![other],
    };
    values
        .into_iter()
        .map(|value| {
            let is_armor = value
                .get("actor_name")
                .and_then(|name| name.as_str())
                .is_some_and(|name| name.starts_with("Armor_"));
            let parsed = if is_armor {
                serde_json::from_value(value).map(ItemSpec::Armor)
            } else {
                serde_json::from_value(value).map(ItemSpec::Weapon)
            };
            parsed.map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
        })
        .collect()
}

/// Generates one mod ROMFS for a mixed list of weapons and armor pieces and
/// finishes with a single RSTB pass.
pub fn generate_item_mod(
    specs: &[ItemSpec],
    clean_romfs: &Path,
    output_romfs: &Path,
    asset_root: &Path,
    zstd: std::sync::Arc<crate::Zstd::TotkZstd<'_>>,
    rstb_level: Option<i32>,
) -> io::Result<ModGenerationReport> {
    if specs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "at least one item specification is required",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for spec in specs {
        spec.validate(asset_root)?;
        if !seen.insert(spec.actor_name()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("actor {} is specified more than once", spec.actor_name()),
            ));
        }
        if clean_romfs
            .join("Pack/Actor")
            .join(format!("{}.pack.zs", spec.actor_name()))
            .is_file()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("actor {} already exists in clean ROMFS", spec.actor_name()),
            ));
        }
    }
    assets::ensure_output_outside_romfs(clean_romfs, output_romfs)?;
    fs::create_dir_all(output_romfs)?;
    let mut weapons = Vec::new();
    let mut armors = Vec::new();
    for spec in specs {
        match spec {
            ItemSpec::Weapon(spec) => weapons.push(spec.generate_files(
                clean_romfs,
                output_romfs,
                asset_root,
                zstd.clone(),
            )?),
            ItemSpec::Armor(spec) => armors.push(spec.generate_files(
                clean_romfs,
                output_romfs,
                asset_root,
                zstd.clone(),
            )?),
        }
    }
    let rstb = rstb::ModRstbProcessor::new(clean_romfs, output_romfs, zstd)
        .with_compression_level(rstb_level)
        .generate()?;
    Ok(ModGenerationReport {
        output_romfs: output_romfs.to_path_buf(),
        weapons,
        armors,
        rstb,
    })
}

/// Reads one or more weapon specifications. JSON accepts a single object or an
/// array; TOML accepts a single specification.
pub fn load_specs(path: &Path) -> io::Result<Vec<WeaponSpec>> {
    let text = fs::read_to_string(path)?;
    let is_toml = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("toml"));
    if is_toml {
        return WeaponSpec::from_toml(&text).map(|spec| vec![spec]);
    }
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let values = match value {
        serde_json::Value::Array(values) => values,
        other => vec![other],
    };
    values
        .into_iter()
        .map(|value| {
            serde_json::from_value(value)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
        })
        .collect()
}

/// Generates a complete mod ROMFS for every specification and finishes with one
/// RSTB pass covering all of them. Relative asset paths resolve against `asset_root`.
pub fn generate_weapon_mod(
    specs: &[WeaponSpec],
    clean_romfs: &Path,
    output_romfs: &Path,
    asset_root: &Path,
    zstd: std::sync::Arc<crate::Zstd::TotkZstd<'_>>,
) -> io::Result<ModGenerationReport> {
    generate_weapon_mod_with_rstb_level(specs, clean_romfs, output_romfs, asset_root, zstd, None)
}

/// [`generate_weapon_mod`] with an explicit Zstandard level for the
/// ResourceSizeTable (see `ModRstbProcessor::with_compression_level`).
pub fn generate_weapon_mod_with_rstb_level(
    specs: &[WeaponSpec],
    clean_romfs: &Path,
    output_romfs: &Path,
    asset_root: &Path,
    zstd: std::sync::Arc<crate::Zstd::TotkZstd<'_>>,
    rstb_level: Option<i32>,
) -> io::Result<ModGenerationReport> {
    if specs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "at least one weapon specification is required",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for spec in specs {
        spec.validate(asset_root)?;
        if !seen.insert(spec.actor_name.as_str()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("actor {} is specified more than once", spec.actor_name),
            ));
        }
        if clean_romfs
            .join("Pack/Actor")
            .join(format!("{}.pack.zs", spec.actor_name))
            .is_file()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("actor {} already exists in clean ROMFS", spec.actor_name),
            ));
        }
    }
    assets::ensure_output_outside_romfs(clean_romfs, output_romfs)?;
    fs::create_dir_all(output_romfs)?;
    let mut weapons = Vec::with_capacity(specs.len());
    for spec in specs {
        weapons.push(spec.generate_files(clean_romfs, output_romfs, asset_root, zstd.clone())?);
    }
    let rstb = rstb::ModRstbProcessor::new(clean_romfs, output_romfs, zstd)
        .with_compression_level(rstb_level)
        .generate()?;
    Ok(ModGenerationReport {
        output_romfs: output_romfs.to_path_buf(),
        weapons,
        armors: Vec::new(),
        rstb,
    })
}

fn resolve_asset(asset_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        asset_root.join(path)
    }
}

impl WeaponSpec {
    fn effective_kind(&self) -> io::Result<WeaponKind> {
        self.kind
            .or_else(|| WeaponKind::from_actor_name(&self.template_actor))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "kind must be LargeSword, SmallSword, Spear, Shield, or Bow, or inferable from template_actor",
                )
            })
    }

    pub fn from_json(text: &str) -> io::Result<Self> {
        serde_json::from_str(text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn from_toml(text: &str) -> io::Result<Self> {
        toml::from_str(text).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn effective_model_name(&self) -> &str {
        if self.model_name.is_empty() {
            &self.actor_name
        } else {
            &self.model_name
        }
    }

    /// Builds this weapon's actor pack from its vanilla template into a mod ROMFS tree.
    pub fn clone_actor_pack(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        zstd: std::sync::Arc<crate::Zstd::TotkZstd<'_>>,
    ) -> io::Result<PathBuf> {
        let kind = self.effective_kind()?;
        actor_pack::validate_item_template_category(
            clean_romfs,
            &self.template_actor,
            kind,
            zstd.clone(),
        )?;
        let output = output_romfs
            .join("Pack/Actor")
            .join(format!("{}.pack.zs", self.actor_name));
        let generated_policy;
        let policy = if self.actor_pack == actor_pack::ActorPackPolicy::default() {
            let mut parameters = self.weapon_parameters.clone();
            parameters.model_name = Some(self.effective_model_name().to_owned());
            if rsdb::template_is_shield(clean_romfs, &self.template_actor, zstd.clone())? {
                parameters.shield_bash_damage = None;
            }
            // Template packs differ in which component files they carry
            // themselves, so the policy is derived from the real pack.
            generated_policy = actor_pack::ActorPackPolicy::standard_item_clone_for_template(
                clean_romfs,
                &self.template_actor,
                &self.actor_name,
                kind,
                parameters,
                self.kind.is_some(),
                zstd.clone(),
            )?;
            &generated_policy
        } else {
            &self.actor_pack
        };
        actor_pack::clone_vanilla_actor_pack_with_links(
            clean_romfs,
            &self.template_actor,
            &self.actor_name,
            &output,
            policy,
            self.sound.as_ref(),
            self.effect.as_ref(),
            self.physics.as_deref(),
            self.chemical.as_deref(),
            self.shootable.as_deref(),
            zstd,
        )?;
        Ok(output)
    }

    /// Generates the weapon RSDB rows, including optional vendor buying/selling prices.
    pub fn generate_rsdb(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        zstd: std::sync::Arc<crate::Zstd::TotkZstd<'_>>,
    ) -> io::Result<Vec<PathBuf>> {
        let request = rsdb::WeaponRsdbRequest {
            actor_name: self.actor_name.clone(),
            template_actor: self.template_actor.clone(),
            model_name: Some(self.effective_model_name().to_owned()),
            max_life: self.weapon_parameters.max_life,
            equipment_performance: self.weapon_parameters.base_attack,
            buying_price: self.vendors.first().and_then(|vendor| vendor.buying_price),
            selling_price: self.vendors.first().and_then(|vendor| vendor.selling_price),
            attachment_damage: self.weapon_parameters.additional_damage,
            shield_bash_damage: self.weapon_parameters.shield_bash_damage,
            overrides: rsdb::WeaponRsdbOverrides::default(),
        };
        request.generate(clean_romfs, output_romfs, zstd)
    }

    /// Adds this weapon to the modifier table used when weapon instances are created.
    pub fn generate_sharp_info(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        zstd: std::sync::Arc<crate::Zstd::TotkZstd<'_>>,
    ) -> io::Result<PathBuf> {
        sharp_info::generate_weapon_sharp_info(
            clean_romfs,
            output_romfs,
            &self.actor_name,
            &self.template_actor,
            zstd,
        )
    }

    /// Adds this weapon to the selected existing vendor and writes only to mod ROMFS.
    pub fn generate_vendor_pack(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        zstd: std::sync::Arc<crate::Zstd::TotkZstd<'_>>,
    ) -> io::Result<Vec<vendor::VendorPackReport>> {
        let processor = vendor::VendorProcessor::new(clean_romfs, output_romfs, zstd);
        self.vendors
            .iter()
            .map(|target| processor.add_weapon(&self.actor_name, target))
            .collect()
    }

    /// Generates both the vendor ShopParam pack and matching priced RSDB weapon rows.
    pub fn generate_vendor(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        zstd: std::sync::Arc<crate::Zstd::TotkZstd<'_>>,
    ) -> io::Result<Option<vendor::VendorGenerationReport>> {
        if self.vendors.is_empty() {
            return Ok(None);
        }
        let rsdb_outputs = self.generate_rsdb(clean_romfs, output_romfs, zstd.clone())?;
        let processor = vendor::VendorProcessor::new(clean_romfs, output_romfs, zstd);
        let vendor_packs = self
            .vendors
            .iter()
            .map(|target| processor.add_weapon(&self.actor_name, target))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Some(vendor::VendorGenerationReport {
            vendor_packs,
            rsdb_outputs,
        }))
    }

    /// Finalizes a completed mod ROMFS by estimating all generated resources and updating RSTB.
    /// Call this after actor packs, assets, RSDB, messages, GameDataList, and vendor files exist.
    pub fn generate_rstb(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        zstd: std::sync::Arc<crate::Zstd::TotkZstd<'_>>,
    ) -> io::Result<rstb::RstbGenerationReport> {
        rstb::ModRstbProcessor::new(clean_romfs, output_romfs, zstd).generate()
    }

    /// Generates the complete mod for this single weapon, RSTB included.
    pub fn generate_mod(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        asset_root: &Path,
        zstd: std::sync::Arc<crate::Zstd::TotkZstd<'_>>,
    ) -> io::Result<ModGenerationReport> {
        generate_weapon_mod(
            std::slice::from_ref(self),
            clean_romfs,
            output_romfs,
            asset_root,
            zstd,
        )
    }

    /// Writes every per-weapon file: actor pack, model and textures, UI BNTX
    /// images, messages, RSDB rows, SharpInfo, GameDataList flags, and vendor
    /// packs. RSTB is left to the caller so batches update it once.
    pub fn generate_files(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        asset_root: &Path,
        zstd: std::sync::Arc<crate::Zstd::TotkZstd<'_>>,
    ) -> io::Result<WeaponGenerationReport> {
        self.validate(asset_root)?;
        let actor_pack = self.clone_actor_pack(clean_romfs, output_romfs, zstd.clone())?;

        let model_assets = assets::WeaponModelAssetsRequest {
            base_name: self.template_actor.clone(),
            new_name: self.effective_model_name().to_owned(),
            model_source: None,
            model_destination: None,
            fbx_path: self
                .assets
                .fbx
                .as_deref()
                .map(|path| resolve_asset(asset_root, path)),
        }
        .generate(clean_romfs, output_romfs, zstd.clone())?;
        let texture_output = output_romfs.join("TexToGo");
        for texture in &self.assets.textures {
            let source = resolve_asset(asset_root, texture);
            let name = source.file_name().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("texture path has no file name: {}", source.display()),
                )
            })?;
            fs::copy(&source, texture_output.join(name))?;
        }

        let mut ui_textures = Vec::with_capacity(3);
        for (source, destination, name, png) in [
            (
                format!("UI/Tex/Icon/{}.bntx.zs", self.template_actor),
                format!("UI/Tex/Icon/{}.bntx.zs", self.actor_name),
                self.actor_name.clone(),
                self.assets.icon_png.as_deref(),
            ),
            (
                format!("UI/Tex/PictureBook/{}_Icon.bntx.zs", self.template_actor),
                format!("UI/Tex/PictureBook/{}_Icon.bntx.zs", self.actor_name),
                format!("{}_Icon", self.actor_name),
                self.assets.picture_book_icon_png.as_deref(),
            ),
            (
                format!("UI/Tex/PictureBook/{}_Detail.bntx.zs", self.template_actor),
                format!("UI/Tex/PictureBook/{}_Detail.bntx.zs", self.actor_name),
                format!("{}_Detail", self.actor_name),
                self.assets.picture_book_detail_png.as_deref(),
            ),
        ] {
            ui_textures.push(generate_ui_texture(
                clean_romfs,
                output_romfs,
                source,
                destination,
                name,
                png.map(|path| resolve_asset(asset_root, path)),
                zstd.clone(),
            )?);
        }

        let messages = messages::WeaponMessageRequest {
            actor_name: self.actor_name.clone(),
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            base_name: self.base_name.clone(),
            attachment_adjective: self.attachment_adjective.clone(),
            picture_book_name: Some(
                self.picture_book_name
                    .clone()
                    .unwrap_or_else(|| self.display_name.clone()),
            ),
            picture_book_description: Some(
                self.picture_book_description
                    .clone()
                    .unwrap_or_else(|| self.description.clone()),
            ),
        }
        .generate_to_mod_romfs(clean_romfs, output_romfs, zstd.clone())?;
        let rsdb = self.generate_rsdb(clean_romfs, output_romfs, zstd.clone())?;
        let sharp_info = self.generate_sharp_info(clean_romfs, output_romfs, zstd.clone())?;
        let game_data = gamedata::WeaponGameDataRequest {
            actor_name: self.actor_name.clone(),
            picture_book: true,
            inventory_flags: true,
        }
        .generate(clean_romfs, output_romfs, zstd.clone())?;
        let vendor_packs = self.generate_vendor_pack(clean_romfs, output_romfs, zstd)?;

        Ok(WeaponGenerationReport {
            actor_name: self.actor_name.clone(),
            actor_pack,
            model: model_assets.model,
            textures: model_assets.textures,
            texture_names: model_assets.texture_names,
            ui_textures,
            messages,
            rsdb,
            sharp_info,
            game_data,
            vendor_packs,
        })
    }

    pub fn validate(&self, asset_root: &Path) -> io::Result<()> {
        let kind = self.effective_kind()?;
        if !self.actor_name.starts_with(kind.actor_prefix()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "actor {} does not match {:?} prefix {}",
                    self.actor_name,
                    kind,
                    kind.actor_prefix()
                ),
            ));
        }
        if self.actor_name == self.template_actor {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "custom actor and template actor must differ",
            ));
        }
        // UI BNTX texture names are rewritten inside the vanilla string slots,
        // which cannot grow beyond the template's name.
        if self.actor_name.len() > self.template_actor.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "actor name {} is longer than template {}; BNTX texture names cannot grow",
                    self.actor_name, self.template_actor
                ),
            ));
        }
        if self.display_name.trim().is_empty() || self.description.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "display name and description are required",
            ));
        }
        for vendor in &self.vendors {
            if !vendor.actor_name.starts_with("Npc_TripMaster_") {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "only existing Npc_TripMaster_* vendors are supported initially",
                ));
            }
            if vendor.quantity == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "vendor quantity must be greater than zero",
                ));
            }
            if vendor.buying_price.is_some_and(|price| price < 0)
                || vendor.selling_price.is_some_and(|price| price < 0)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "vendor buying and selling prices cannot be negative",
                ));
            }
        }

        for source in self.asset_sources() {
            let resolved = if source.is_absolute() {
                source.to_path_buf()
            } else {
                asset_root.join(source)
            };
            if !resolved.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("required source asset is missing: {}", resolved.display()),
                ));
            }
        }
        Ok(())
    }

    fn asset_sources(&self) -> Vec<&Path> {
        let mut result = [
            self.assets.fbx.as_deref(),
            self.assets.icon_png.as_deref(),
            self.assets.picture_book_icon_png.as_deref(),
            self.assets.picture_book_detail_png.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        result.extend(self.assets.textures.iter().map(PathBuf::as_path));
        result
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    CloneActorPack,
    CopyAsset,
    PatchByml,
    PatchTagProduct,
    PatchMessages,
    PatchRstb,
    PatchVendorPack,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlannedFile {
    pub relative_path: PathBuf,
    pub action: PlanAction,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct GenerationPlan {
    pub files: Vec<PlannedFile>,
}

impl GenerationPlan {
    pub fn for_weapon(spec: &WeaponSpec, clean_romfs: &Path) -> io::Result<Self> {
        let (version, _) = version::discover_product_file(
            &clean_romfs.join("RSDB"),
            "ActorInfo.Product.",
            ".rstbl.byml.zs",
        )?;
        Self::for_weapon_version(spec, &version)
    }

    pub fn for_weapon_version(spec: &WeaponSpec, version: &str) -> io::Result<Self> {
        let actor = &spec.actor_name;
        let mut files = vec![
            planned(
                format!("Pack/Actor/{actor}.pack.zs"),
                PlanAction::CloneActorPack,
                format!("clone and specialize {} actor pack", spec.template_actor),
            ),
            planned(
                format!("Model/{actor}.{actor}.bfres.mc"),
                PlanAction::CopyAsset,
                "custom render model",
            ),
            planned(
                format!("UI/Tex/Icon/{actor}.bntx.zs"),
                PlanAction::CopyAsset,
                "inventory icon",
            ),
            planned(
                format!("UI/Tex/PictureBook/{actor}_Icon.bntx.zs"),
                PlanAction::CopyAsset,
                "compendium icon",
            ),
            planned(
                format!("UI/Tex/PictureBook/{actor}_Detail.bntx.zs"),
                PlanAction::CopyAsset,
                "compendium detail image",
            ),
        ];
        files.extend(spec.assets.textures.iter().filter_map(|source| {
            source.file_name().map(|name| PlannedFile {
                relative_path: Path::new("TexToGo").join(name),
                action: PlanAction::CopyAsset,
                reason: "model texture".into(),
            })
        }));
        for product in [
            "ActorInfo",
            "AttachmentActorInfo",
            "GameActorInfo",
            "PouchActorInfo",
        ] {
            let name = version::product_name(
                &format!("RSDB/{product}.Product."),
                version,
                ".rstbl.byml.zs",
            )?;
            files.push(planned(
                name,
                PlanAction::PatchByml,
                "register custom weapon row",
            ));
        }
        files.push(planned(
            SHARP_INFO,
            PlanAction::PatchByml,
            "register custom weapon row",
        ));
        files.push(planned(
            version::product_name("RSDB/Tag.Product.", version, ".rstbl.byml.zs")?,
            PlanAction::PatchTagProduct,
            "register actor tag bitset and path",
        ));
        files.push(planned(
            version::product_name("Mals/USen.Product.", version, ".sarc.zs")?,
            PlanAction::PatchMessages,
            "add name, description, attachment, and compendium messages",
        ));
        files.push(planned(
            version::product_name(
                "System/Resource/ResourceSizeTable.Product.",
                version,
                ".rsizetable.zs",
            )?,
            PlanAction::PatchRstb,
            "add sizes for every new resource and update modified resources",
        ));
        for vendor in &spec.vendors {
            files.push(planned(
                format!("Pack/Actor/{}.pack.zs", vendor.actor_name),
                PlanAction::PatchVendorPack,
                "extend an existing travelling merchant selling list",
            ));
        }
        Ok(Self { files })
    }

    /// Creates only the output directories. It never copies or mutates game files.
    pub fn prepare_output_layout(&self, output_romfs: &Path) -> io::Result<()> {
        for file in &self.files {
            if file.relative_path.is_absolute()
                || file
                    .relative_path
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unsafe output path: {}", file.relative_path.display()),
                ));
            }
            if let Some(parent) = output_romfs.join(&file.relative_path).parent() {
                fs::create_dir_all(parent)?;
            }
        }
        Ok(())
    }
}

/// Clones one vanilla UI BNTX under the custom name. A supplied PNG replaces the
/// image payload when the container format supports it; ASTC containers keep the
/// vanilla image and the report carries a warning instead of failing the mod.
pub(super) fn generate_ui_texture(
    clean_romfs: &Path,
    output_romfs: &Path,
    source: String,
    destination: String,
    name: String,
    png: Option<PathBuf>,
    zstd: std::sync::Arc<crate::Zstd::TotkZstd<'_>>,
) -> io::Result<UiTextureReport> {
    let request = assets::WeaponBntxAssetRequest {
        texture_source: source.into(),
        png_source: png,
        new_name: name,
        texture_destination: PathBuf::from(&destination),
    };
    let mut warning = None;
    let report = match request.generate(clean_romfs, output_romfs, zstd.clone()) {
        Ok(report) => report,
        Err(error)
            if request.png_source.is_some()
                && error
                    .to_string()
                    .contains("ASTC BNTX replacement is not supported") =>
        {
            let partial = output_romfs.join(&request.texture_destination);
            if partial.is_file() {
                let mut permissions = fs::metadata(&partial)?.permissions();
                permissions.set_readonly(false);
                fs::set_permissions(&partial, permissions)?;
                fs::remove_file(&partial)?;
            }
            warning = Some(format!(
                "{destination}: {error}; the vanilla image was kept under the new name"
            ));
            let mut clone = request.clone();
            clone.png_source = None;
            clone.generate(clean_romfs, output_romfs, zstd)?
        }
        Err(error) => return Err(error),
    };
    Ok(UiTextureReport {
        destination: output_romfs.join(&destination),
        name: report.name,
        format: report.format,
        width: report.width,
        height: report.height,
        png_applied: request.png_source.is_some() && warning.is_none(),
        similarity: report.similarity,
        warning,
    })
}

fn planned(path: impl Into<PathBuf>, action: PlanAction, reason: impl Into<String>) -> PlannedFile {
    PlannedFile {
        relative_path: path.into(),
        action,
        reason: reason.into(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DifferenceKind {
    Added,
    Modified,
    Identical,
}

/// Compares a mod tree with clean ROMFS without parsing or changing either tree.
pub fn audit_mod_tree(
    mod_romfs: &Path,
    clean_romfs: &Path,
) -> io::Result<BTreeMap<String, DifferenceKind>> {
    let mut files = Vec::new();
    collect_files(mod_romfs, mod_romfs, &mut files)?;
    let mut result = BTreeMap::new();
    for (relative, mod_path) in files {
        let clean_path = clean_romfs.join(&relative);
        let kind = if !clean_path.is_file() {
            DifferenceKind::Added
        } else if fs::read(&mod_path)? == fs::read(clean_path)? {
            DifferenceKind::Identical
        } else {
            DifferenceKind::Modified
        };
        result.insert(relative.to_string_lossy().replace('\\', "/"), kind);
    }
    Ok(result)
}

fn collect_files(
    root: &Path,
    current: &Path,
    output: &mut Vec<(PathBuf, PathBuf)>,
) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, output)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?
                .to_path_buf();
            output.push((relative, path));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_item_kind_json_accepts_only_supported_kinds() {
        for (json, expected) in [
            (r#""LargeSword""#, WeaponKind::LargeSword),
            (r#""SmallSword""#, WeaponKind::SmallSword),
            (r#""Spear""#, WeaponKind::Spear),
            (r#""Shield""#, WeaponKind::Shield),
            (r#""Bow""#, WeaponKind::Bow),
        ] {
            assert_eq!(serde_json::from_str::<WeaponKind>(json).unwrap(), expected);
        }
        for unsupported in [r#""Sword""#, r#""Axe""#, r#""LargeSword2""#] {
            assert!(serde_json::from_str::<WeaponKind>(unsupported).is_err());
        }
    }

    #[test]
    fn omitted_kind_is_inferred_from_the_base_actor() {
        let spec = WeaponSpec::from_json(
            r#"{
                "actor_name":"Weapon_Sword_900",
                "template_actor":"Weapon_Sword_025",
                "display_name":"Test Sword",
                "description":"Test",
                "assets":{}
            }"#,
        )
        .unwrap();
        assert_eq!(spec.kind, None);
        assert_eq!(spec.effective_kind().unwrap(), WeaponKind::SmallSword);
    }

    #[test]
    #[ignore = "writes the complete real Weapon_Lsword_005 integration fixture"]
    fn generates_complete_weapon_lsword_005_comparison_mod() {
        use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};
        use std::sync::Arc;

        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let restoration =
            fixture_root.join("BotW Weapon Restoration/romfs/Pack/Actor/Weapon_Lsword_005.pack.zs");
        if !clean_romfs.is_dir() || !restoration.is_file() {
            return;
        }
        let output_root = fixture_root.join("test_sic");
        let output_romfs = output_root.join("romfs");
        if output_root.is_dir() {
            fs::remove_dir_all(&output_root).expect("remove previous test_sic output");
        }
        fs::create_dir_all(&output_romfs).expect("create test_sic ROMFS");

        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            crate::Zstd::TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        let restored_info = actor_pack::load_weapon_actor_info(
            &fs::read(&restoration).expect("read restoration actor pack"),
            "Weapon_Lsword_005",
            zstd.clone(),
        )
        .expect("read restoration weapon values");
        let custom_png = fixture_root.join("BotW Weapon Restoration/Weapon_Lsword_002.png");
        let input_json = serde_json::json!({
            "actor_name": "Weapon_Lsword_005",
            "kind": "large_sword",
            "template_actor": "Weapon_Lsword_108",
            "model_name": "Weapon_Lsword_005",
            "weapon_parameters": {
                "base_attack": restored_info.base_attack,
                "max_life": restored_info.durability,
                "additional_damage": restored_info.attachment.additional_damage,
                "shield_bash_damage": restored_info.attachment.shield_bash_damage
            },
            "sound": {"source": "vanilla_actor", "actor_name": "Weapon_Lsword_103"},
            "effect": {"source": "vanilla_actor", "actor_name": "Weapon_Lsword_103"},
            "physics": "Weapon_Lsword_108",
            "display_name": "Spiked Boko Bat",
            "description": "After much consideration by Bokoblins on how to improve the Boko bat, they simply attached sharp spikes to it.",
            "assets": {
                "fbx": fixture_root.join("untitled.fbx"),
                "icon_png": &custom_png,
                "picture_book_icon_png": &custom_png,
                "picture_book_detail_png": &custom_png
            },
            "vendors": [
                {
                    "actor_name": "Npc_TripMaster_00",
                    "buying_price": 500,
                    "selling_price": 125,
                    "quantity": 3
                }
            ]
        });
        let input_text = serde_json::to_string_pretty(&input_json).expect("serialize input JSON");
        let spec = WeaponSpec::from_json(&input_text).expect("parse complete item-creator JSON");
        fs::write(output_root.join("items_creator_input.json"), &input_text)
            .expect("write item-creator input JSON");
        fs::write(
            fixture_root.join("test_sic_items_creator_input.json"),
            &input_text,
        )
        .expect("write persistent item-creator input JSON");

        let pack_json = serde_json::json!({
            "name": "Weapon_Lsword_005",
            "base": "Weapon_Lsword_108",
            "model_name": "Weapon_Lsword_005",
            "attack": restored_info.base_attack,
            "dur": restored_info.durability,
            "attachment_damage": restored_info.attachment.additional_damage,
            "shield_bash_damage": restored_info.attachment.shield_bash_damage,
            "sound": {"source": "vanilla_actor", "name": "Weapon_Lsword_103"},
            "effect": {"source": "vanilla_actor", "name": "Weapon_Lsword_103"},
            "physics_actor": "Weapon_Lsword_108",
            "shootable": null,
            "extra_edits": []
        });
        actor_pack::WeaponPackRequest::from_json(&pack_json.to_string())
            .expect("parse exhaustive actor-pack JSON")
            .generate_pack(
                clean_romfs,
                &output_romfs.join("Pack/Actor/Weapon_Lsword_005.pack.zs"),
                zstd.clone(),
            )
            .expect("generate actor pack");

        let model_source = fixture_root.join("works/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc");
        let model_destination =
            output_romfs.join("Model/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc");
        fs::create_dir_all(model_destination.parent().expect("model parent"))
            .expect("create model directory");
        let model_source_raw = zstd
            .decompress_mcpk(&fs::read(&model_source).expect("read model base"))
            .expect("decompress model base");
        let custom_fbx = fixture_root.join("untitled.fbx");
        let replaced_model =
            crate::file_format::Model3D::bfres::BfresFile::replace_geometry_from_fbx(
                &model_source_raw,
                &fs::read(&custom_fbx).expect("read custom FBX"),
            )
            .expect("replace BFRES geometry");
        fs::write(
            &model_destination,
            zstd.compress_mcpk(&replaced_model)
                .expect("compress custom BFRES"),
        )
        .expect("write custom BFRES");
        let generated_model_assets = assets::GeneratedWeaponAssets {
            model: model_destination,
            textures: Vec::new(),
            texture_names: Vec::new(),
        };
        let texture_output = output_romfs.join("TexToGo");
        fs::create_dir_all(&texture_output).expect("create TexToGo directory");
        for name in [
            "Weapon_Lsword_005_Alb.txtg",
            "Weapon_Lsword_005_Nrm.txtg",
            "Weapon_Lsword_005_Spm.txtg",
        ] {
            fs::copy(
                fixture_root
                    .join("BotW Weapon Restoration/romfs/TexToGo")
                    .join(name),
                texture_output.join(name),
            )
            .expect("copy Weapon_Lsword_005 TexToGo texture");
        }
        let generated_model_raw = zstd
            .decompress_mcpk(
                &fs::read(&generated_model_assets.model).expect("read generated custom model"),
            )
            .expect("decompress generated custom model");
        let generated_model =
            crate::file_format::Model3D::bfres::BfresFile::from_bytes(&generated_model_raw)
                .expect("parse generated custom model");
        let required_texture_names: std::collections::BTreeSet<String> = generated_model
            .materials
            .iter()
            .flat_map(|material| &material.texture_slots)
            .map(|slot| slot.name.clone())
            .collect();
        let mut placeholder_textures = Vec::new();
        assets::ensure_material_textures(
            &output_romfs.join("TexToGo"),
            &required_texture_names,
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("misc/placeholder_tex.txtg"),
            &mut placeholder_textures,
            &BTreeMap::new(),
        )
        .expect("ensure every BFRES material texture exists");
        let imported_fbx = crate::parser::fbx::import::import_for_bfres(
            &fs::read(&custom_fbx).expect("read custom FBX"),
        )
        .expect("parse custom FBX");
        assert_eq!(generated_model.render.meshes.len(), 2);
        assert_eq!(imported_fbx.meshes.len(), 2);

        let mut asset_report = format!(
            "custom_fbx={}\nmodel={}\nmesh_count={}\nvertex_count={}\ntexture_names={}\nplaceholder_textures={}\n",
            custom_fbx.display(),
            generated_model_assets.model.display(),
            generated_model.render.meshes.len(),
            generated_model
                .render
                .meshes
                .iter()
                .map(|mesh| mesh.positions.len())
                .sum::<usize>(),
            required_texture_names.iter().cloned().collect::<Vec<_>>().join(","),
            placeholder_textures
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy())
                .collect::<Vec<_>>()
                .join(",")
        );
        for (source, destination, name) in [
            (
                "UI/Tex/Icon/Weapon_Lsword_005.bntx.zs",
                "UI/Tex/Icon/Weapon_Lsword_005.bntx.zs",
                "Weapon_Lsword_005",
            ),
            (
                "UI/Tex/PictureBook/Weapon_Lsword_005_Icon.bntx.zs",
                "UI/Tex/PictureBook/Weapon_Lsword_005_Icon.bntx.zs",
                "Weapon_Lsword_005_Icon",
            ),
            (
                "UI/Tex/PictureBook/Weapon_Lsword_005_Detail.bntx.zs",
                "UI/Tex/PictureBook/Weapon_Lsword_005_Detail.bntx.zs",
                "Weapon_Lsword_005_Detail",
            ),
        ] {
            let replacement = assets::WeaponBntxAssetRequest::from_json(
                &serde_json::json!({
                    "texture_source": fixture_root.join("BotW Weapon Restoration/romfs").join(source),
                    "png_source": &custom_png,
                    "name": name,
                    "texture_destination": destination
                }).to_string(),
            )
            .expect("parse BNTX request")
            .generate(clean_romfs, &output_romfs, zstd.clone())
            .expect("generate BNTX asset");
            use std::fmt::Write as _;
            writeln!(
                asset_report,
                "bntx={destination} name={} format={} size={}x{} similarity={:.6}",
                replacement.name,
                replacement.format,
                replacement.width,
                replacement.height,
                replacement.similarity
            )
            .expect("write BNTX report");
        }
        fs::write(output_root.join("custom_asset_report.txt"), asset_report)
            .expect("write custom asset report");

        messages::WeaponMessageRequest::from_json(
            &serde_json::json!({
                "name": "Weapon_Lsword_005",
                "display_name": "Spiked Boko Bat",
                "description": "After much consideration by Bokoblins on how \nto improve the Boko bat, they simply attached \nsharp spikes to it. A skilled fighter can cause \nimmense damage with this.",
                "base_name": "Bat",
                "attachment_name": "Spiked-Bat",
                "picture_book_name": "Spiked Boko Bat",
                "picture_book_caption": "After much consideration by Bokoblins on how \nto improve the Boko bat, they simply attached \nsharp spikes to it. A skilled fighter can cause \nimmense damage with this."
            })
            .to_string(),
        )
        .expect("parse message JSON")
        .generate_to_mod_romfs(clean_romfs, &output_romfs, zstd.clone())
        .expect("generate MALS");

        rsdb::WeaponRsdbRequest::from_json(
            &serde_json::json!({
                "name": "Weapon_Lsword_005",
                "base": "Weapon_Lsword_108",
                "model_name": "Weapon_Lsword_005",
                "life": 34,
                "attack": restored_info.base_attack,
                "buying_price": 500,
                "selling_price": 125,
                "attachment_damage": restored_info.attachment.additional_damage,
                "shield_bash_damage": restored_info.attachment.shield_bash_damage,
                "overrides": {
                    "actor_info": {},
                    "attachment_actor_info": {},
                    "game_actor_info": {},
                    "pouch_actor_info": {}
                }
            })
            .to_string(),
        )
        .expect("parse RSDB JSON")
        .generate(clean_romfs, &output_romfs, zstd.clone())
        .expect("generate RSDB");

        sharp_info::generate_weapon_sharp_info(
            clean_romfs,
            &output_romfs,
            "Weapon_Lsword_005",
            "Weapon_Lsword_103",
            zstd.clone(),
        )
        .expect("generate SharpInfo");

        gamedata::WeaponGameDataRequest::from_json(
            r#"{"name":"Weapon_Lsword_005","picture_book":true,"inventory_flags":true}"#,
        )
        .expect("parse GameData JSON")
        .generate(clean_romfs, &output_romfs, zstd.clone())
        .expect("generate GameDataList");

        for vendor in &spec.vendors {
            vendor::VendorProcessor::new(clean_romfs, &output_romfs, zstd.clone())
                .add_weapon("Weapon_Lsword_005", vendor)
                .expect("generate vendor pack");
        }

        rstb::ModRstbProcessor::new(clean_romfs, &output_romfs, zstd)
            .generate()
            .expect("generate RSTB");
    }

    #[test]
    #[ignore = "generates the complete mod from tmp/test_sic_items_creator_input.json"]
    fn generates_complete_mod_from_configured_romfs() {
        use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};
        use std::sync::Arc;

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let input = fs::read_to_string(root.join("test_sic_items_creator_input.json")).unwrap();
        let mut specs = load_specs(&root.join("test_sic_items_creator_input.json")).unwrap();
        let mut config = TotkConfig::safe_new(false).expect("load configured ROMFS path");
        let clean_romfs = PathBuf::from(&config.romfs);
        assert!(
            clean_romfs.is_dir(),
            "configured ROMFS is unavailable: {}",
            clean_romfs.display()
        );
        let output_root = root.join("test_sic");
        let output_romfs = output_root.join("romfs");
        if output_root.is_dir() {
            fs::remove_dir_all(&output_root).unwrap();
        }
        fs::create_dir_all(&output_romfs).unwrap();
        let zstd = Arc::new(
            crate::Zstd::TotkZstd::new(Arc::new(config.clone()), TOTK_ZSTD_COMPRESSION_LEVEL)
                .unwrap(),
        );

        for spec in &mut specs {
            if spec.assets.fbx.is_some() {
                spec.assets.fbx = Some(root.join("untitled.fbx"));
            }
            spec.assets.icon_png = Some(root.join("placeholder.png"));
            spec.assets.picture_book_icon_png = Some(root.join("placeholder.png"));
            spec.assets.picture_book_detail_png = Some(root.join("placeholder.png"));
        }
        let report =
            generate_weapon_mod(&specs, &clean_romfs, &output_romfs, &root, zstd.clone()).unwrap();
        assert_eq!(report.weapons.len(), specs.len());

        for (spec, weapon) in specs.iter().zip(&report.weapons) {
            let expected_model = output_romfs.join(format!(
                "Model/{0}.{0}.bfres.mc",
                spec.effective_model_name()
            ));
            assert_eq!(weapon.model, expected_model);
            let model_raw = zstd
                .decompress_mcpk(&fs::read(&weapon.model).unwrap())
                .unwrap();
            let model =
                crate::file_format::Model3D::bfres::BfresFile::from_bytes(&model_raw).unwrap();
            let expected_container = format!("{0}.{0}", spec.effective_model_name());
            assert_eq!(model.name.as_deref(), Some(expected_container.as_str()));
            assert_eq!(
                model
                    .sections_with_signature(b"FMDL")
                    .next()
                    .and_then(|v| v.name.as_deref()),
                Some(spec.effective_model_name())
            );
            assert!(
                !model_raw
                    .windows(spec.template_actor.len())
                    .any(|window| window == spec.template_actor.as_bytes()),
                "generated BFRES still contains internal template name {}",
                spec.template_actor
            );
            assert_eq!(weapon.ui_textures.len(), 3);
            for texture in &weapon.ui_textures {
                assert!(texture.destination.is_file());
            }
            assert!(weapon.messages.is_file());
            assert!(weapon.sharp_info.is_file());
            assert_eq!(weapon.vendor_packs.len(), spec.vendors.len());
        }
        assert!(report.rstb.output.is_file());
        fs::write(output_root.join("items_creator_input.json"), input).unwrap();
        config.romfs.clear();
    }

    #[test]
    #[ignore = "writes the expanded real-ROMFS test_sic item matrix"]
    fn generates_expanded_test_sic_item_matrix() {
        use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};
        use std::sync::Arc;

        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        if !clean_romfs.is_dir() {
            return;
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/test_sic/multi");
        if root.is_dir() {
            fs::remove_dir_all(&root).expect("remove previous expanded test_sic matrix");
        }
        fs::create_dir_all(&root).expect("create expanded test_sic matrix");
        let output_romfs = root.join("romfs");
        fs::create_dir_all(&output_romfs).expect("create shared multi-item ROMFS");

        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            crate::Zstd::TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        let vendor = serde_json::json!({
            "actor_name": "Npc_TripMaster_00",
            "buying_price": 500,
            "selling_price": 125,
            "quantity": 3
        });
        let cases = [
            (
                "Weapon_Bow_900",
                "bow",
                "Weapon_Bow_001",
                "Moonwhistle Bow",
                "A pale bow that hums whenever its arrows pass beneath starlight.",
            ),
            (
                "Weapon_Bow_901",
                "bow",
                "Weapon_Bow_002",
                "Brambleflash Bow",
                "Twisted forest fibers snap forward with the sound of a summer storm.",
            ),
            (
                "Weapon_Shield_900",
                "shield",
                "Weapon_Shield_001",
                "Cloudglass Shield",
                "A mirrored shield said to hold a tiny piece of the morning sky.",
            ),
            (
                "Weapon_Shield_901",
                "shield",
                "Weapon_Shield_002",
                "Mossback Guard",
                "Ancient wood and living moss soften blows with surprising strength.",
            ),
            (
                "Weapon_Sword_900",
                "small_sword",
                "Weapon_Sword_070",
                "Comet Needle",
                "A nimble blade forged from metal gathered after a brilliant meteor shower.",
            ),
            (
                "Weapon_Sword_901",
                "small_sword",
                "Weapon_Sword_001",
                "Cinderleaf Saber",
                "Its leaf-shaped edge leaves a warm orange trail through the air.",
            ),
            (
                "Weapon_Spear_900",
                "spear",
                "Weapon_Spear_001",
                "Riverstar Pike",
                "Blue lacquer guides rainwater along the shaft and toward its silver point.",
            ),
            (
                "Weapon_Spear_901",
                "spear",
                "Weapon_Spear_002",
                "Thunderreed Lance",
                "A flexible reed core lets this lance bend before striking like lightning.",
            ),
        ];

        let mut all_inputs = Vec::new();
        for (index, (actor, kind, template, display_name, description)) in
            cases.into_iter().enumerate()
        {
            let weapon_parameters = match (kind, template) {
                ("bow" | "shield", _) | (_, "Weapon_Sword_070") => serde_json::json!({
                    "base_attack": 18 + index as i32,
                    "max_life": 24 + index as i32
                }),
                _ => serde_json::json!({
                    "base_attack": 18 + index as i32,
                    "max_life": 24 + index as i32,
                    "additional_damage": 5 + index as i32,
                    "shield_bash_damage": 7 + index as i32
                }),
            };
            let input = serde_json::json!({
                "actor_name": actor,
                "kind": kind,
                "template_actor": template,
                "model_name": actor,
                "weapon_parameters": weapon_parameters,
                "display_name": display_name,
                "description": description,
                "assets": {},
                "vendors": [vendor.clone()]
            });
            all_inputs.push(input.clone());
            let input_text = serde_json::to_string_pretty(&input).unwrap();
            let spec = WeaponSpec::from_json(&input_text).expect("parse case input");
            spec.validate(&root).expect("validate case input");
            spec.clone_actor_pack(clean_romfs, &output_romfs, zstd.clone())
                .expect("clone actor pack");
            assets::WeaponModelAssetsRequest {
                base_name: template.to_owned(),
                new_name: actor.to_owned(),
                model_source: None,
                model_destination: None,
                fbx_path: None,
            }
            .generate(clean_romfs, &output_romfs, zstd.clone())
            .expect("generate per-item BFRES and TexToGo assets");
            for (suffix, source_suffix) in [("", ""), ("_Icon", "_Icon"), ("_Detail", "_Detail")] {
                let (folder, source_name, destination_name) = if suffix.is_empty() {
                    (
                        "UI/Tex/Icon",
                        format!("{template}.bntx.zs"),
                        format!("{actor}.bntx.zs"),
                    )
                } else {
                    (
                        "UI/Tex/PictureBook",
                        format!("{template}{source_suffix}.bntx.zs"),
                        format!("{actor}{suffix}.bntx.zs"),
                    )
                };
                assets::WeaponBntxAssetRequest {
                    texture_source: PathBuf::from(folder).join(source_name),
                    png_source: None,
                    new_name: format!("{actor}{suffix}"),
                    texture_destination: PathBuf::from(folder).join(destination_name),
                }
                .generate(clean_romfs, &output_romfs, zstd.clone())
                .expect("generate per-item BNTX asset");
            }
            spec.generate_rsdb(clean_romfs, &output_romfs, zstd.clone())
                .expect("generate RSDB");
            if !matches!(kind, "bow" | "shield") && template != "Weapon_Sword_070" {
                spec.generate_sharp_info(clean_romfs, &output_romfs, zstd.clone())
                    .expect("generate SharpInfo");
            }
            messages::WeaponMessageRequest::from_json(
                &serde_json::json!({
                    "actor_name": actor,
                    "display_name": display_name,
                    "description": description,
                    "base_name": display_name,
                    "attachment_adjective": display_name
                })
                .to_string(),
            )
            .expect("parse message request")
            .generate_to_mod_romfs(clean_romfs, &output_romfs, zstd.clone())
            .expect("generate MALS");
            gamedata::WeaponGameDataRequest::from_json(
                &serde_json::json!({
                    "actor_name": actor,
                    "picture_book": true,
                    "inventory_flags": true
                })
                .to_string(),
            )
            .expect("parse GameData request")
            .generate(clean_romfs, &output_romfs, zstd.clone())
            .expect("generate GameData");
            spec.generate_vendor_pack(clean_romfs, &output_romfs, zstd.clone())
                .expect("generate vendor pack");
        }
        fs::write(
            root.join("items_creator_input.json"),
            serde_json::to_string_pretty(&all_inputs).unwrap(),
        )
        .expect("write multi-item JSON list");
        rstb::ModRstbProcessor::new(clean_romfs, &output_romfs, zstd.clone())
            .generate()
            .expect("generate one merged RSTB");

        let actor_names: Vec<_> = cases.iter().map(|case| case.0).collect();
        for actor in &actor_names {
            assert!(output_romfs
                .join(format!("Pack/Actor/{actor}.pack.zs"))
                .is_file());
            assert!(output_romfs
                .join(format!("Model/{actor}.{actor}.bfres.mc"))
                .is_file());
            assert!(output_romfs
                .join(format!("UI/Tex/Icon/{actor}.bntx.zs"))
                .is_file());
            assert!(output_romfs
                .join(format!("UI/Tex/PictureBook/{actor}_Icon.bntx.zs"))
                .is_file());
            assert!(output_romfs
                .join(format!("UI/Tex/PictureBook/{actor}_Detail.bntx.zs"))
                .is_file());
        }

        let mals_path = output_romfs.join("Mals/USen.Product.121.sarc.zs");
        let mals = crate::file_format::Pack::PackFile::from_binary(
            &fs::read(&mals_path).expect("read merged MALS"),
            zstd.clone(),
        )
        .expect("parse merged MALS");
        let pouch = crate::parser::msbt::Msbt::from_bytes(
            mals.sarc
                .get_data("ActorMsg/PouchContent.msbt")
                .expect("merged pouch messages"),
        )
        .expect("parse merged pouch messages");
        for actor in &actor_names {
            assert!(pouch
                .messages
                .iter()
                .any(|message| message.label.as_deref() == Some(&format!("{actor}_Name"))));
        }

        let (_, path) = version::discover_product_file(
            &output_romfs.join("RSDB"),
            "ActorInfo.Product.",
            ".rstbl.byml.zs",
        )
        .expect("discover merged ActorInfo");
        let file = crate::file_format::BinTextFile::BymlFile::new(&path, zstd.clone())
            .expect("parse merged ActorInfo");
        let text = file.pio.to_text();
        for actor in &actor_names {
            assert!(text.contains(actor), "{path:?} is missing {actor}");
        }
        let (_, game_data_path) = version::discover_product_file(
            &output_romfs.join("GameData"),
            "GameDataList.Product.",
            ".byml.zs",
        )
        .expect("discover merged GameDataList");
        assert!(game_data_path.is_file());
    }

    #[test]
    #[ignore = "inspects the exact ROMFS Weapon_Bow_001 BFRES"]
    fn inspects_weapon_bow_001_romfs_bfres() {
        use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};
        use std::sync::Arc;
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let path = clean_romfs.join("Model/Weapon_Bow_001.Weapon_Bow_001.bfres.mc");
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            crate::Zstd::TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).unwrap(),
        );
        let raw = zstd.decompress_mcpk(&fs::read(&path).unwrap()).unwrap();
        let bfres = crate::file_format::Model3D::bfres::BfresFile::from_bytes(&raw).unwrap();
        let fmdl = bfres
            .sections
            .iter()
            .filter(|section| &section.signature == b"FMDL")
            .count();
        let fshp = bfres
            .sections
            .iter()
            .filter(|section| &section.signature == b"FSHP")
            .count();
        println!(
            "{} raw={} name={:?} sections={} fmdl={} fshp={} meshes={}",
            path.display(),
            raw.len(),
            bfres.name,
            bfres.sections.len(),
            fmdl,
            fshp,
            bfres.render.meshes.len()
        );
        assert!(!bfres.render.meshes.is_empty());
        let renamed =
            crate::file_format::Model3D::bfres::BfresFile::rename_first_model_and_container(
                &raw,
                "Weapon_Bow_900",
                "Weapon_Bow_900.Weapon_Bow_900",
            )
            .unwrap();
        let renamed_parsed =
            crate::file_format::Model3D::bfres::BfresFile::from_bytes(&renamed).unwrap();
        println!(
            "after model rename meshes={}",
            renamed_parsed.render.meshes.len()
        );
        let textures =
            crate::file_format::Model3D::bfres::BfresFile::rename_material_texture_slots(
                &renamed,
                "Weapon_Bow_001",
                "Weapon_Bow_900",
            )
            .unwrap();
        let textures_parsed =
            crate::file_format::Model3D::bfres::BfresFile::from_bytes(&textures).unwrap();
        println!(
            "after texture rename meshes={}",
            textures_parsed.render.meshes.len()
        );
        assert!(!textures_parsed.render.meshes.is_empty());
        let compressed = zstd.compress_mcpk(&textures).unwrap();
        let roundtrip = zstd.decompress_mcpk(&compressed).unwrap();
        let roundtrip_parsed =
            crate::file_format::Model3D::bfres::BfresFile::from_bytes(&roundtrip).unwrap();
        println!(
            "after MCPK roundtrip raw={}/{} meshes={}",
            textures.len(),
            roundtrip.len(),
            roundtrip_parsed.render.meshes.len()
        );
        assert!(!roundtrip_parsed.render.meshes.is_empty());
    }

    #[test]
    #[ignore = "compares tmp/test_sic with the Weapon Restoration fixture"]
    fn compares_weapon_lsword_005_generated_mod_semantics() {
        use crate::{
            file_format::{
                BinTextFile::BymlFile, Model3D::bfres::BfresFile, Pack::PackFile,
                TagProduct::TagProduct,
            },
            parser::{bntx::BntxFile, msbt::Msbt, rstb::ResourceSizeTable},
            TotkConfig::TotkConfig,
            Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
        };
        use roead::byml::Byml;
        use std::{fmt::Write as _, sync::Arc};

        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let generated = root.join("test_sic/romfs");
        let reference = root.join("BotW Weapon Restoration/romfs");
        if !generated.is_dir() || !reference.is_dir() {
            return;
        }
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            crate::Zstd::TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load dictionaries"),
        );
        let mut report = String::new();
        let mut errors = 0usize;

        let actor_relative = Path::new("Pack/Actor/Weapon_Lsword_005.pack.zs");
        let generated_pack = PackFile::from_binary(
            &fs::read(generated.join(actor_relative)).expect("read generated actor pack"),
            zstd.clone(),
        )
        .expect("open generated actor pack");
        let reference_pack = PackFile::from_binary(
            &fs::read(reference.join(actor_relative)).expect("read reference actor pack"),
            zstd.clone(),
        )
        .expect("open reference actor pack");
        let generated_names: BTreeMap<_, _> = generated_pack
            .sarc
            .files()
            .filter_map(|file| {
                file.name()
                    .map(|name| (name.to_owned(), file.data().to_vec()))
            })
            .collect();
        let reference_names: BTreeMap<_, _> = reference_pack
            .sarc
            .files()
            .filter_map(|file| {
                file.name()
                    .map(|name| (name.to_owned(), file.data().to_vec()))
            })
            .collect();
        writeln!(report, "ACTOR PACK").expect("write report");
        for name in generated_names
            .keys()
            .filter(|name| name.contains("Weapon_Lsword_005"))
        {
            match reference_names.get(name) {
                None => {
                    errors += 1;
                    writeln!(report, "ERROR missing reference internal path: {name}")
                        .expect("write report");
                }
                Some(reference_data) if name.ends_with("bgyml") => {
                    let generated_byml = BymlFile::from_binary(
                        generated_names.get(name).expect("generated entry"),
                        zstd.clone(),
                        name,
                    )
                    .expect("parse generated pack BYML");
                    let reference_byml = BymlFile::from_binary(reference_data, zstd.clone(), name)
                        .expect("parse reference pack BYML");
                    let mut normalized_generated = generated_byml.pio.clone();
                    let mut normalized_reference = reference_byml.pio.clone();
                    if name.starts_with("Actor/") {
                        for root in [&mut normalized_generated, &mut normalized_reference] {
                            if let Ok(root) = root.as_mut_map() {
                                if let Some(components) = root
                                    .get_mut("Components")
                                    .and_then(|value| value.as_mut_map().ok())
                                {
                                    components.remove("ChemicalRef");
                                }
                            }
                        }
                    }
                    if normalized_generated != normalized_reference {
                        errors += 1;
                        writeln!(
                            report,
                            "DIFF BYML: {name}\n  generated={}\n  reference={}",
                            generated_byml.pio.to_text(),
                            reference_byml.pio.to_text()
                        )
                        .expect("write report");
                    } else {
                        writeln!(report, "MATCH BYML: {name} (optional chemical normalized)")
                            .expect("write report");
                    }
                }
                Some(_) => {}
            }
        }

        writeln!(report, "\nRSDB").expect("write report");
        for product in [
            "ActorInfo",
            "AttachmentActorInfo",
            "GameActorInfo",
            "PouchActorInfo",
        ] {
            let name = format!("RSDB/{product}.Product.121.rstbl.byml.zs");
            let row = |base: &Path| {
                let file = BymlFile::new(base.join(&name), zstd.clone()).expect("open RSDB");
                file.pio
                    .as_array()
                    .expect("RSDB array")
                    .iter()
                    .find(|row| {
                        row.as_map()
                            .ok()
                            .and_then(|map| map.get("__RowId"))
                            .and_then(|value| value.as_string().ok())
                            .is_some_and(|value| value == "Weapon_Lsword_005")
                    })
                    .cloned()
            };
            let mut generated_row = row(&generated);
            let mut reference_row = row(&reference);
            if product == "PouchActorInfo" {
                for row in [&mut generated_row, &mut reference_row] {
                    if let Some(map) = row.as_mut().and_then(|value| value.as_mut_map().ok()) {
                        map.remove("BuyingPrice");
                        map.remove("SellingPrice");
                    }
                }
            }
            match (generated_row, reference_row) {
                (Some(left), Some(right)) if left == right => {
                    writeln!(report, "MATCH ROW: {product}").expect("write report")
                }
                (Some(_), Some(_)) => {
                    errors += 1;
                    let mut left = row(&generated).expect("generated row");
                    if product == "PouchActorInfo" {
                        if let Ok(map) = left.as_mut_map() {
                            map.remove("BuyingPrice");
                            map.remove("SellingPrice");
                        }
                    }
                    let right = row(&reference).expect("reference row");
                    writeln!(
                        report,
                        "DIFF ROW: {product}\n  generated={}\n  reference={}",
                        left.to_text(),
                        right.to_text()
                    )
                    .expect("write report")
                }
                values => {
                    errors += 1;
                    writeln!(report, "ERROR ROW: {product}: {values:?}").expect("write report")
                }
            }
        }

        let sharp_row = |base: &Path| {
            BymlFile::new(base.join(SHARP_INFO), zstd.clone()).and_then(|file| {
                file.pio
                    .as_map()
                    .ok()
                    .and_then(|root| root.get("SharpInfoList"))
                    .and_then(|rows| rows.as_array().ok())
                    .and_then(|rows| {
                        rows.iter()
                            .find(|row| {
                                row.as_map()
                                    .ok()
                                    .and_then(|row| row.get("ActorName"))
                                    .and_then(|value| value.as_string().ok())
                                    .is_some_and(|value| value == "Weapon_Lsword_005")
                            })
                            .cloned()
                    })
            })
        };
        match (sharp_row(&generated), sharp_row(&reference)) {
            (Some(left), Some(right)) if left == right => {
                writeln!(report, "MATCH SharpInfo row").expect("write report")
            }
            values => {
                errors += 1;
                writeln!(report, "DIFF SharpInfo row: {values:?}").expect("write report")
            }
        }

        let tag_name = Path::new("RSDB/Tag.Product.121.rstbl.byml.zs");
        let read_tag = |base: &Path| {
            let bytes = fs::read(base.join(tag_name)).expect("read Tag.Product");
            TagProduct::from_binary(&bytes, tag_name, zstd.clone())
        };
        let generated_tag = read_tag(&generated).expect("parse generated Tag.Product");
        let reference_tag = read_tag(&reference);
        let actor_tag_path = "Work/Actor/|Weapon_Lsword_005|.engine__actor__ActorParam.gyml";
        if generated_tag.actor_tag_data.contains_key(actor_tag_path) {
            writeln!(
                report,
                "VALID TAG PRODUCT entry: generated={:?}, reference={:?}",
                generated_tag.actor_tag_data.get(actor_tag_path),
                reference_tag
                    .as_ref()
                    .and_then(|tag| tag.actor_tag_data.get(actor_tag_path))
            )
            .expect("write report");
            if reference_tag.is_none() {
                writeln!(report, "REFERENCE WARNING Tag.Product does not parse")
                    .expect("write report");
            }
        } else {
            errors += 1;
            writeln!(report, "ERROR generated TAG PRODUCT entry is missing").expect("write report");
        }

        writeln!(report, "\nMALS/MSBT").expect("write report");
        let mals_name = Path::new("Mals/USen.Product.121.sarc.zs");
        let open_mals = |base: &Path| {
            PackFile::from_binary(
                &fs::read(base.join(mals_name)).expect("read MALS"),
                zstd.clone(),
            )
            .expect("open MALS")
        };
        let generated_mals = open_mals(&generated);
        let reference_mals = open_mals(&reference);
        writeln!(
            report,
            "MALS RAW SIZE: generated={}, reference={}",
            generated_mals.data.len(),
            reference_mals.data.len()
        )
        .expect("write report");
        for file in generated_mals.sarc.files() {
            let Some(name) = file.name() else { continue };
            if !name.ends_with(".msbt") {
                continue;
            }
            let generated_msbt = Msbt::from_bytes(file.data()).expect("parse generated MSBT");
            let Some(reference_data) = reference_mals.sarc.get_data(name) else {
                continue;
            };
            if file.data() != reference_data {
                writeln!(
                    report,
                    "MALS ENTRY BYTES DIFFER: {name}: generated={}, reference={}",
                    file.data().len(),
                    reference_data.len()
                )
                .expect("write report");
            }
            let reference_msbt = Msbt::from_bytes(reference_data).expect("parse reference MSBT");
            for message in generated_msbt.messages.iter().filter(|message| {
                message
                    .label
                    .as_deref()
                    .is_some_and(|label| label.contains("Weapon_Lsword_005"))
            }) {
                let matching = reference_msbt
                    .messages
                    .iter()
                    .find(|candidate| candidate.label == message.label);
                if matching == Some(message) {
                    writeln!(
                        report,
                        "MATCH MESSAGE: {name}:{}",
                        message.label.as_deref().unwrap_or("")
                    )
                    .expect("write report");
                } else {
                    errors += 1;
                    writeln!(
                        report,
                        "DIFF MESSAGE: {name}:{}\n  generated={:?}\n  reference={:?}",
                        message.label.as_deref().unwrap_or(""),
                        message.parts,
                        matching.map(|value| &value.parts)
                    )
                    .expect("write report");
                }
            }
        }

        writeln!(report, "\nBFRES").expect("write report");
        let model_name = Path::new("Model/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc");
        let parse_model = |base: &Path| {
            let compressed = fs::read(base.join(model_name)).expect("read model");
            let raw = zstd.decompress_mcpk(&compressed).expect("decompress model");
            BfresFile::from_bytes(&raw).expect("parse model")
        };
        let generated_model = parse_model(&generated);
        let reference_model = parse_model(&reference);
        writeln!(
            report,
            "generated name={:?}, reference name={:?}",
            generated_model.name, reference_model.name
        )
        .expect("write report");
        if generated_model.name != reference_model.name {
            errors += 1;
        }
        let generated_slots: Vec<_> = generated_model
            .materials
            .iter()
            .flat_map(|material| material.texture_slots.iter().map(|slot| slot.name.clone()))
            .collect();
        if generated_slots
            .iter()
            .any(|name| name.contains("Weapon_Sword_022"))
        {
            errors += 1;
            writeln!(report, "ERROR BFRES retains base texture names").expect("write report");
        }
        writeln!(
            report,
            "generated meshes={}, reference meshes={}",
            generated_model.render.meshes.len(),
            reference_model.render.meshes.len()
        )
        .expect("write report");

        writeln!(report, "\nBNTX").expect("write report");
        for relative in [
            "UI/Tex/Icon/Weapon_Lsword_005.bntx.zs",
            "UI/Tex/PictureBook/Weapon_Lsword_005_Icon.bntx.zs",
            "UI/Tex/PictureBook/Weapon_Lsword_005_Detail.bntx.zs",
        ] {
            if !generated.join(relative).is_file() {
                writeln!(report, "SKIPPED BNTX (not modified): {relative}").expect("write report");
                continue;
            }
            let parse = |base: &Path| {
                let compressed = fs::read(base.join(relative)).expect("read BNTX");
                let (raw, _) = zstd
                    .try_decompress_for_path(Path::new(relative), &compressed)
                    .expect("decompress BNTX");
                BntxFile::parse(&raw).expect("parse BNTX")
            };
            let left = parse(&generated);
            let right = parse(&reference);
            let left_texture = left.textures.first().expect("generated texture");
            let right_texture = right.textures.first().expect("reference texture");
            let expected_name = Path::new(relative)
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".bntx.zs"))
                .expect("BNTX expected name");
            if left_texture.name != expected_name
                || left_texture.format != right_texture.format
                || left_texture.width != right_texture.width
                || left_texture.height != right_texture.height
            {
                errors += 1;
                writeln!(report, "DIFF BNTX: {relative}: generated={left_texture:?}, reference={right_texture:?}").expect("write report");
            } else {
                writeln!(report, "MATCH BNTX metadata: {relative}").expect("write report");
                if right_texture.name != expected_name {
                    writeln!(
                        report,
                        "REFERENCE WARNING stale internal BNTX name: {}",
                        right_texture.name
                    )
                    .expect("write report");
                }
            }
        }

        writeln!(report, "\nGLOBAL OUTPUTS").expect("write report");
        let game_data = generated.join("GameData/GameDataList.Product.110.byml.zs");
        if BymlFile::new(&game_data, zstd.clone()).is_some() {
            writeln!(report, "VALID GameDataList reopens after hash insertion")
                .expect("write report");
        } else {
            errors += 1;
            writeln!(report, "ERROR GameDataList does not reopen").expect("write report");
        }
        let generated_game_data =
            BymlFile::new(&game_data, zstd.clone()).expect("open generated GameDataList");
        let reference_game_data = BymlFile::new(
            reference.join("GameData/GameDataList.Product.110.byml.zs"),
            zstd.clone(),
        )
        .expect("open reference GameDataList");
        let game_data_entry = |root: &Byml, hash: u32| {
            root.as_map()
                .ok()
                .and_then(|root| root.get("Data"))
                .and_then(|data| data.as_map().ok())
                .and_then(|data| {
                    ["Bool", "Enum", "Struct"].iter().find_map(|kind| {
                        data.get(*kind)
                            .and_then(|entries| entries.as_array().ok())
                            .and_then(|entries| {
                                entries.iter().find(|entry| {
                                    entry.as_map().ok().is_some_and(|entry| {
                                        entry.get("Hash") == Some(&Byml::U32(hash))
                                    })
                                })
                            })
                    })
                })
                .cloned()
        };
        for name in [
            "IsGet.Weapon_Lsword_005",
            "IsGetAnyway.Weapon_Lsword_005",
            "PictureBookData.Weapon_Lsword_005",
            "PictureBookData.Weapon_Lsword_005.IsNew",
            "PictureBookData.Weapon_Lsword_005.State",
        ] {
            let hash = gamedata::murmur3_hash(name);
            let left = game_data_entry(&generated_game_data.pio, hash);
            let right = game_data_entry(&reference_game_data.pio, hash);
            if left == right {
                writeln!(report, "MATCH GameDataList: {name} ({hash:#010X})")
                    .expect("write report");
            } else {
                errors += 1;
                writeln!(
                    report,
                    "DIFF GameDataList: {name} ({hash:#010X})\n  generated={left:?}\n  reference={right:?}"
                )
                .expect("write report");
            }
        }

        let generated_bootup_path = generated.join(ecocat::BOOTUP_PACK);
        if generated_bootup_path.is_file() {
            let generated_bootup = PackFile::from_binary(
                &fs::read(&generated_bootup_path).expect("read generated Bootup"),
                zstd.clone(),
            )
            .expect("open generated Bootup");
            let reference_bootup = PackFile::from_binary(
                &fs::read(reference.join(ecocat::BOOTUP_PACK)).expect("read reference Bootup"),
                zstd.clone(),
            )
            .expect("open reference Bootup");
            for path in [
                "Ecosystem/Ground.ecocat.byml",
                "Ecosystem/MinusField.ecocat.byml",
            ] {
                let generated_count = generated_bootup
                    .byml_file(path)
                    .expect("parse generated ecocat")
                    .pio
                    .to_text()
                    .matches("Weapon_Lsword_005")
                    .count();
                let reference_count = reference_bootup
                    .byml_file(path)
                    .expect("parse reference ecocat")
                    .pio
                    .to_text()
                    .matches("Weapon_Lsword_005")
                    .count();
                writeln!(report, "VALID {path}: generated occurrences={generated_count}, reference occurrences={reference_count}")
                    .expect("write report");
            }
        } else {
            writeln!(
                report,
                "SKIPPED Ecocat: generated mod does not modify Bootup"
            )
            .expect("write report");
        }

        let vendor_pack = PackFile::from_binary(
            &fs::read(generated.join("Pack/Actor/Npc_TripMaster_00.pack.zs"))
                .expect("read generated vendor"),
            zstd.clone(),
        )
        .expect("open generated vendor");
        let vendor_contains_weapon = vendor_pack.sarc.files().any(|file| {
            file.name().is_some_and(|name| name.contains("ShopParam"))
                && BymlFile::from_binary(file.data(), zstd.clone(), "vendor-shop")
                    .is_ok_and(|file| file.pio.to_text().contains("Weapon_Lsword_005"))
        });
        if vendor_contains_weapon {
            writeln!(report, "VALID vendor ShopParam contains Weapon_Lsword_005")
                .expect("write report");
        } else {
            errors += 1;
            writeln!(
                report,
                "ERROR vendor ShopParam is missing Weapon_Lsword_005"
            )
            .expect("write report");
        }

        let rstb_path =
            generated.join("System/Resource/ResourceSizeTable.Product.121.rsizetable.zs");
        let (rstb_raw, _) = zstd
            .try_decompress_for_path(&rstb_path, &fs::read(&rstb_path).expect("read RSTB"))
            .expect("decompress RSTB");
        let rstb = ResourceSizeTable::from_bytes(&rstb_raw).expect("parse RSTB");
        for path in [
            "Pack/Actor/Weapon_Lsword_005.pack",
            "Model/Weapon_Lsword_005.Weapon_Lsword_005.bfres",
        ] {
            if rstb.get(path.to_owned()).is_some() {
                writeln!(report, "VALID RSTB entry: {path}").expect("write report");
            } else {
                errors += 1;
                writeln!(report, "ERROR missing RSTB entry: {path}").expect("write report");
            }
        }

        writeln!(report, "\nTOTAL ERRORS: {errors}").expect("write report");
        fs::write(root.join("test_sic/comparison_report.txt"), report).expect("save report");
    }

    #[test]
    #[ignore = "diagnostic vendor pack inspection"]
    fn inspect_vendor_pack_entries() {
        use crate::{
            file_format::Pack::PackFile,
            TotkConfig::TotkConfig,
            Zstd::{TotkZstd, TOTK_ZSTD_COMPRESSION_LEVEL},
        };
        use roead::byml::Byml;
        use std::sync::Arc;

        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let mut config = TotkConfig::default();
        config.romfs = romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).unwrap());
        for (label, path) in [
            (
                "modified trip",
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../tmp/BotW Weapon Restoration/Npc_TripMaster_00.pack.zs"),
            ),
            (
                "clean trip",
                romfs.join("Pack/Actor/Npc_TripMaster_00.pack.zs"),
            ),
            (
                "restoration vendor",
                Path::new(env!("CARGO_MANIFEST_DIR")).join(
                    "../tmp/BotW Weapon Restoration/romfs/Pack/Actor/Npc_WeaponVendor001.pack.zs",
                ),
            ),
        ] {
            if !path.is_file() {
                continue;
            }
            let pack = PackFile::from_binary(&fs::read(&path).unwrap(), zstd.clone()).unwrap();
            let _ = label;
            for file in pack.sarc.files() {
                let Some(name) = file.name() else {
                    continue;
                };
                if name.contains("Shop") || name.contains("ActorParam") {
                    if name.ends_with(".bgyml") {
                        if let Ok(value) = Byml::from_binary(file.data()) {
                            let _ = value;
                        }
                    }
                }
            }
        }
        for product in ["ActorInfo", "GameActorInfo", "PouchActorInfo"] {
            for (label, root) in [
                (
                    "restoration",
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../tmp/BotW Weapon Restoration/romfs/RSDB"),
                ),
                ("clean", romfs.join("RSDB")),
            ] {
                let path = root.join(format!("{product}.Product.121.rstbl.byml.zs"));
                let raw = zstd.decompress_zs(&fs::read(path).unwrap()).unwrap();
                let rows = Byml::from_binary(&raw).unwrap();
                for actor in [
                    "Weapon_Lsword_005",
                    "Weapon_Lsword_108",
                    "Item_Weapon_032",
                    "Weapon_Sword_042",
                    "Weapon_Lsword_032",
                ] {
                    if let Some(row) = rows.as_array().unwrap().iter().find(|row| {
                        row.as_map()
                            .ok()
                            .and_then(|map| map.get("__RowId"))
                            .and_then(|value| value.as_string().ok())
                            .is_some_and(|value| value.as_str() == actor)
                    }) {
                        let _ = (label, product, actor, row);
                    }
                }
            }
        }
        for path in [
            romfs.join("Pack/Actor/Weapon_Lsword_108.pack.zs"),
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../tmp/BotW Weapon Restoration/romfs/Pack/Actor/Weapon_Lsword_005.pack.zs"),
        ] {
            let pack = PackFile::from_binary(&fs::read(&path).unwrap(), zstd.clone()).unwrap();
            for file in pack.sarc.files() {
                let Some(name) = file.name() else {
                    continue;
                };
                let Ok(value) = Byml::from_binary(file.data()) else {
                    continue;
                };
                let text = value.to_text();
                if text.contains("Price") {
                    let _ = (name, text);
                }
            }
        }
    }

    #[test]
    #[ignore = "diagnostic SharpInfo row inspection"]
    fn inspect_weapon_sharp_info_rows() {
        use crate::{
            file_format::BinTextFile::BymlFile,
            TotkConfig::TotkConfig,
            Zstd::{TotkZstd, TOTK_ZSTD_COMPRESSION_LEVEL},
        };
        use roead::byml::Byml;
        use std::sync::Arc;

        fn find_actor(value: &Byml, actor: &str, output: &mut Vec<Byml>) {
            match value {
                Byml::Map(map) => {
                    if map
                        .values()
                        .any(|value| value.as_string().is_ok_and(|value| value.as_str() == actor))
                    {
                        output.push(value.clone());
                    }
                    for value in map.values() {
                        find_actor(value, actor, output);
                    }
                }
                Byml::Array(values) => {
                    for value in values {
                        find_actor(value, actor, output);
                    }
                }
                _ => {}
            }
        }

        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).unwrap());
        for (label, path, actor) in [
            ("clean", clean_romfs.join(SHARP_INFO), "Weapon_Lsword_108"),
            (
                "reference",
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../tmp/BotW Weapon Restoration/romfs")
                    .join(SHARP_INFO),
                "Weapon_Lsword_005",
            ),
        ] {
            let file = BymlFile::new(path, zstd.clone()).unwrap();
            if let Ok(map) = file.pio.as_map() {
                println!("SHARP ROOT KEYS: {:?}", map.keys().collect::<Vec<_>>());
            }
            let mut rows = Vec::new();
            find_actor(&file.pio, actor, &mut rows);
            println!("SHARP {label} {actor}: {}", rows.len());
            for row in rows {
                println!("{}", row.to_text());
            }
        }
    }

    fn spec() -> WeaponSpec {
        WeaponSpec {
            actor_name: "Weapon_Lsword_900".into(),
            kind: Some(WeaponKind::LargeSword),
            template_actor: "Weapon_Lsword_060".into(),
            model_name: String::new(),
            weapon_parameters: actor_pack::WeaponParameterOverrides::default(),
            actor_pack: actor_pack::ActorPackPolicy::default(),
            sound: None,
            effect: None,
            physics: None,
            chemical: None,
            shootable: None,
            display_name: "Test Sword".into(),
            description: "A test weapon".into(),
            base_name: None,
            attachment_adjective: None,
            picture_book_name: None,
            picture_book_description: None,
            assets: WeaponAssets {
                fbx: Some("model.fbx".into()),
                textures: vec!["blade_Alb.txtg".into()],
                icon_png: Some("icon.png".into()),
                picture_book_icon_png: Some("book_icon.png".into()),
                picture_book_detail_png: Some("book_detail.png".into()),
            },
            vendors: vec![VendorTarget {
                actor_name: "Npc_TripMaster_00".into(),
                buying_price: None,
                selling_price: None,
                quantity: 1,
            }],
        }
    }

    #[test]
    fn plan_contains_core_registration_files() {
        let plan = GenerationPlan::for_weapon_version(&spec(), "112").unwrap();
        for path in [
            "RSDB/ActorInfo.Product.112.rstbl.byml.zs",
            "RSDB/PouchActorInfo.Product.112.rstbl.byml.zs",
            "RSDB/Tag.Product.112.rstbl.byml.zs",
            "Mals/USen.Product.112.sarc.zs",
            "System/Resource/ResourceSizeTable.Product.112.rsizetable.zs",
        ] {
            assert!(plan
                .files
                .iter()
                .any(|file| file.relative_path == Path::new(path)));
        }
        assert!(plan.files.iter().any(|file| {
            file.relative_path == Path::new("Pack/Actor/Npc_TripMaster_00.pack.zs")
                && file.action == PlanAction::PatchVendorPack
        }));
    }

    #[test]
    fn load_specs_accepts_a_json_object_or_array() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/items_creator_load_specs");
        fs::create_dir_all(&root).unwrap();
        let single = serde_json::to_string(&spec()).unwrap();
        fs::write(root.join("single.json"), &single).unwrap();
        fs::write(root.join("many.json"), format!("[{single},{single}]")).unwrap();
        assert_eq!(load_specs(&root.join("single.json")).unwrap().len(), 1);
        let many = load_specs(&root.join("many.json")).unwrap();
        assert_eq!(many.len(), 2);
        assert_eq!(many[1].actor_name, "Weapon_Lsword_900");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batches_reject_duplicate_actor_names() {
        let specs = [spec(), spec()];
        let error = generate_weapon_mod(
            &specs,
            Path::new("missing-romfs"),
            Path::new("missing-output"),
            Path::new("missing-assets"),
            std::sync::Arc::new(crate::Zstd::TotkZstd::dictionaryless(
                std::sync::Arc::new(crate::TotkConfig::TotkConfig::default()),
                crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
            )),
        )
        .unwrap_err();
        // Validation runs before any ROMFS access, so the missing asset wins.
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn rejects_new_vendor_actor_names() {
        let mut value = spec();
        value.vendors[0].actor_name = "Npc_CustomVendor".into();
        assert!(value.validate(Path::new("missing-assets")).is_err());
    }
}
