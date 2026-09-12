//! Custom armor pieces (Head / Upper / Lower) cloned from a vanilla base actor.
//!
//! Armor is a pouch-only item: it has no SharpInfo, attachment rows or
//! compendium entry, but it does have sixteen dye icons, a shared `.anim`
//! BFRES per model project and an `ArmorParam` component that carries the
//! defense value. The clone keeps everything name-scoped under a new model
//! project (`Armor_001` → `Armor_900`), drops cloth physics (the placeholder
//! geometry has nothing to simulate) and severs the upgrade / hood-swap links.

use super::{
    armor_model::{self, CubeModelReport, CubeModelSpec},
    assets, gamedata, messages, rsdb, vendor, UiTextureReport, VendorTarget,
};
use crate::{
    compression::meshcodec::MeshCodec,
    file_format::{
        BinTextFile::BymlFile,
        Model3D::bfres::{toolbox::ResFile, BfresFile},
        Pack::PackFile,
    },
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
    #[serde(default)]
    pub assets: ArmorAssets,
    #[serde(default)]
    pub vendors: Vec<VendorTarget>,
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
        if let Some(model) = &self.model {
            if model.weights.is_empty() || model.weights.len() > 4 {
                return Err(invalid("cube weights must name one to four bones"));
            }
        }
        for vendor in &self.vendors {
            if !vendor.actor_name.starts_with("Npc_TripMaster_") {
                return Err(invalid(
                    "only existing Npc_TripMaster_* vendors are supported initially",
                ));
            }
            if vendor.quantity == 0 {
                return Err(invalid("vendor quantity must be greater than zero"));
            }
            if vendor.buying_price.is_some_and(|price| price < 0)
                || vendor.selling_price.is_some_and(|price| price < 0)
            {
                return Err(invalid(
                    "vendor buying and selling prices cannot be negative",
                ));
            }
        }
        if let Some(icon) = &self.assets.icon_png {
            let resolved = super::resolve_asset(asset_root, icon);
            if !resolved.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("required source asset is missing: {}", resolved.display()),
                ));
            }
        }
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
        self.validate(asset_root)?;
        assets::ensure_output_outside_romfs(clean_romfs, output_romfs)?;
        let slot = self.slot()?;
        let project = self.project()?;
        let template_project = self.template_project()?;
        let template = TemplateArmor::load(clean_romfs, &self.template_actor, zstd.clone())?;

        let actor_pack =
            self.clone_actor_pack(clean_romfs, output_romfs, &template, zstd.clone())?;
        let model = self.generate_model(clean_romfs, output_romfs, &template, zstd.clone())?;
        let model_anim = clone_project_anim(
            clean_romfs,
            output_romfs,
            &template_project,
            &project,
            zstd.clone(),
        )?;

        let mut ui_textures = Vec::new();
        for (source, destination, name) in
            icon_variants(clean_romfs, &self.template_actor, &self.actor_name)?
        {
            let png = if name == self.actor_name {
                self.assets
                    .icon_png
                    .as_deref()
                    .map(|path| super::resolve_asset(asset_root, path))
            } else {
                None
            };
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

        let messages = messages::generate_pouch_labels(
            clean_romfs,
            output_romfs,
            &self.actor_name,
            &self.display_name,
            &self.description,
            zstd.clone(),
        )?;
        let rsdb = self.generate_rsdb(clean_romfs, output_romfs, &template, zstd.clone())?;
        let game_data = gamedata::WeaponGameDataRequest {
            actor_name: self.actor_name.clone(),
            picture_book: false,
            inventory_flags: true,
        }
        .generate(clean_romfs, output_romfs, zstd.clone())?;
        let processor = vendor::VendorProcessor::new(clean_romfs, output_romfs, zstd);
        let vendor_packs = self
            .vendors
            .iter()
            .map(|target| processor.add_weapon(&self.actor_name, target))
            .collect::<io::Result<Vec<_>>>()?;

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
        })
    }

    /// Clones the template pack under the new project name. Every SARC entry
    /// and BYML string scoped to the template project is renamed, cloth physics
    /// is dropped and ArmorParam receives the custom defense/series values.
    fn clone_actor_pack(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        template: &TemplateArmor,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<PathBuf> {
        let project = self.project()?;
        let template_project = self.template_project()?;
        let pack = PackFile::from_binary(&template.pack_bytes, zstd.clone())?;
        let actor_file = format!("Actor/{}.engine__actor__ActorParam.bgyml", self.actor_name);
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

        let mut entries = Vec::new();
        for file in pack.sarc.files() {
            let name = file
                .name()
                .ok_or_else(|| invalid_data("template armor pack contains an unnamed entry"))?;
            // The placeholder cube carries no cloth; Havok assets and the
            // physics component that binds them are left out.
            if name.starts_with("Phive/") || name.starts_with("Component/Physics/") {
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
                let components = map_child_mut(&mut document.pio, "Components")?;
                components.insert(
                    "PhysicsRef".into(),
                    Byml::String(
                        "?Component/Physics/Dummy.engine__component__PhysicsParam.bgyml".into(),
                    ),
                );
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
                // `HideMaterialGroupNameList` names the material groups of
                // Link's body model (`G_Upper` torso/arm skin, `G_Lower` legs,
                // ...) that this piece covers. The template's list is kept; a
                // cube upper additionally hides the torso skin, which vanilla
                // tunics leave visible because their own mesh covers it.
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
        if !entries.iter().any(|(name, _)| *name == actor_file) {
            return Err(invalid_data(format!(
                "cloned pack has no {actor_file}; template project {template_project} was not found in the entry names"
            )));
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
        template: &TemplateArmor,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<GeneratedArmorModel> {
        let project = self.project()?;
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
        let external = assets::external_strings_for(clean_romfs, &raw)?;
        let mut file = ResFile::load(&raw, &external)
            .map_err(|error| invalid_data(format!("failed to load template BFRES: {error}")))?;
        if file.model_count() == 0 {
            return Err(invalid_data("template BFRES contains no model"));
        }
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
        for old_name in &texture_names {
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
        let compressed = MeshCodec::compress(&renamed)
            .map_err(|error| invalid_data(format!("failed to MCPK-compress BFRES: {error}")))?;
        let roundtrip = MeshCodec::decompress(&compressed).map_err(|error| {
            invalid_data(format!("generated MCPK does not decompress: {error}"))
        })?;
        BfresFile::from_bytes(&roundtrip).map_err(|error| {
            invalid_data(format!("round-tripped BFRES cannot be parsed: {error}"))
        })?;
        fs::write(&destination, compressed)?;
        Ok(GeneratedArmorModel {
            model: destination,
            cube,
            textures: copied,
            texture_names: required_texture_names.into_iter().collect(),
        })
    }

    /// ActorInfo, GameActorInfo, PouchActorInfo rows and the Tag entry. Armor
    /// has no AttachmentActorInfo row; the EnhancementMaterialInfo upgrade row
    /// is skipped because the clone is not upgradable.
    fn generate_rsdb(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        template: &TemplateArmor,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<Vec<PathBuf>> {
        let project = self.project()?;
        let template_project = self.template_project()?;
        let clean_rsdb = clean_romfs.join("RSDB");
        let output_rsdb = output_romfs.join("RSDB");
        fs::create_dir_all(&output_rsdb)?;
        let (version, actor_info_source) = super::version::discover_product_file(
            &clean_rsdb,
            "ActorInfo.Product.",
            ".rstbl.byml.zs",
        )?;
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

        let mut actor: BTreeMap<String, JsonValue> = BTreeMap::new();
        actor.insert(
            "ActorName".into(),
            JsonValue::String(self.actor_name.clone()),
        );
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
        let tables: [(String, BTreeMap<String, JsonValue>, &[&str]); 3] = [
            (actor_info, actor, &[]),
            (game_actor_info, BTreeMap::new(), &[]),
            (
                pouch_actor_info,
                pouch,
                &["ArmorNextRankActor", "ArmorHeadSwapActor"],
            ),
        ];
        let mut outputs = Vec::with_capacity(4);
        for (name, overrides, removals) in tables {
            let destination = output_rsdb.join(&name);
            let clean_source = clean_rsdb.join(&name);
            let source = if destination.is_file() {
                destination.clone()
            } else {
                clean_source
            };
            rsdb::WeaponRsdbProcessor::clone_rsdb_row(
                &source,
                &destination,
                &self.template_actor,
                &self.actor_name,
                &overrides,
                removals,
                zstd.clone(),
            )?;
            outputs.push(destination);
        }
        let tag_output = output_rsdb.join(&tag_product);
        let clean_tag = clean_rsdb.join(&tag_product);
        let tag_source = if tag_output.is_file() {
            tag_output.clone()
        } else {
            clean_tag
        };
        rsdb::WeaponRsdbProcessor::clone_tag_entry(
            &tag_source,
            &tag_output,
            &self.template_actor,
            &self.actor_name,
            zstd,
        )?;
        outputs.push(tag_output);
        Ok(outputs)
    }
}

struct GeneratedArmorModel {
    model: PathBuf,
    cube: Option<CubeModelReport>,
    textures: Vec<PathBuf>,
    texture_names: Vec<String>,
}

/// What the generator needs to know about the vanilla base actor.
struct TemplateArmor {
    pack_bytes: Vec<u8>,
    model_project: String,
    fmdb_name: String,
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
        })
    }
}

/// `Model/<project>.anim.bfres.zs` holds the dye material animations shared by
/// every piece of a project. It is copied once per new project with the
/// project name patched in place (same length, so the string pool is intact).
fn clone_project_anim(
    clean_romfs: &Path,
    output_romfs: &Path,
    template_project: &str,
    project: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<Option<PathBuf>> {
    let source = clean_romfs
        .join("Model")
        .join(format!("{template_project}.anim.bfres.zs"));
    if !source.is_file() || template_project.len() != project.len() {
        return Ok(None);
    }
    let destination = output_romfs
        .join("Model")
        .join(format!("{project}.anim.bfres.zs"));
    if destination.is_file() {
        return Ok(Some(destination));
    }
    let compressed = fs::read(&source)?;
    let (raw, dictionary) = zstd.try_decompress_for_path(&source, &compressed)?;
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
