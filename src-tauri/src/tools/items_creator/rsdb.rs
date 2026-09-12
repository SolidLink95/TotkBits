//! Clone-and-patch generation for weapon-related RSDB products.

use crate::{
    file_format::{BinTextFile::BymlFile, TagProduct::TagProduct},
    Zstd::TotkZstd,
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

const PRODUCT_SUFFIX: &str = ".rstbl.byml.zs";
const ACTOR_INFO_PREFIX: &str = "ActorInfo.Product.";

/// Per-table escape hatches. Values must match the type of the cloned template field.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WeaponRsdbOverrides {
    #[serde(default)]
    pub actor_info: BTreeMap<String, JsonValue>,
    #[serde(default)]
    pub attachment_actor_info: BTreeMap<String, JsonValue>,
    #[serde(default)]
    pub game_actor_info: BTreeMap<String, JsonValue>,
    #[serde(default)]
    pub pouch_actor_info: BTreeMap<String, JsonValue>,
}

/// JSON/dict-friendly request for the five RSDB products used by an ordinary weapon.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WeaponRsdbRequest {
    #[serde(alias = "name")]
    pub actor_name: String,
    #[serde(alias = "base")]
    pub template_actor: String,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default, alias = "life")]
    pub max_life: Option<i32>,
    #[serde(default, alias = "attack")]
    pub equipment_performance: Option<i32>,
    #[serde(default)]
    pub buying_price: Option<i32>,
    #[serde(default)]
    pub selling_price: Option<i32>,
    #[serde(default)]
    pub attachment_damage: Option<i32>,
    #[serde(default)]
    pub shield_bash_damage: Option<i32>,
    #[serde(default)]
    pub overrides: WeaponRsdbOverrides,
}

/// Stateful processor for version discovery and weapon-related RSDB generation.
pub struct WeaponRsdbProcessor<'a> {
    clean_romfs: PathBuf,
    output_romfs: PathBuf,
    zstd: Arc<TotkZstd<'a>>,
}

impl WeaponRsdbRequest {
    pub fn from_json(text: &str) -> io::Result<Self> {
        serde_json::from_str(text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    /// Generate only weapon-related RSDB files under `<output_romfs>/RSDB`.
    pub fn generate(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<Vec<PathBuf>> {
        WeaponRsdbProcessor::new(clean_romfs, output_romfs, zstd).generate_weapon(self)
    }
}

impl<'a> WeaponRsdbProcessor<'a> {
    pub fn new(clean_romfs: &Path, output_romfs: &Path, zstd: Arc<TotkZstd<'a>>) -> Self {
        Self {
            clean_romfs: clean_romfs.to_path_buf(),
            output_romfs: output_romfs.to_path_buf(),
            zstd,
        }
    }

    pub fn generate_weapon(&self, request: &WeaponRsdbRequest) -> io::Result<Vec<PathBuf>> {
        Self::validate_actor_name(&request.actor_name)?;
        Self::validate_actor_name(&request.template_actor)?;
        if request.buying_price.is_some_and(|price| price < 0)
            || request.selling_price.is_some_and(|price| price < 0)
        {
            return Err(Self::invalid(
                "buying and selling prices cannot be negative",
            ));
        }
        if request.actor_name == request.template_actor {
            return Err(Self::invalid("custom and template actor names must differ"));
        }
        Self::ensure_output_outside_romfs(&self.clean_romfs, &self.output_romfs)?;
        let clean_rsdb = self.clean_romfs.join("RSDB");
        let output_rsdb = self.output_romfs.join("RSDB");
        fs::create_dir_all(&output_rsdb)?;
        let (version, actor_info_source) =
            super::version::discover_product_file(&clean_rsdb, ACTOR_INFO_PREFIX, PRODUCT_SUFFIX)?;
        let actor_info = actor_info_source
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| Self::invalid_data("ActorInfo filename is not UTF-8"))?
            .to_owned();
        let attachment_info = Self::versioned_rsdb_name("AttachmentActorInfo", &version)?;
        let game_actor_info = Self::versioned_rsdb_name("GameActorInfo", &version)?;
        let pouch_actor_info = Self::versioned_rsdb_name("PouchActorInfo", &version)?;
        let tag_product = Self::versioned_rsdb_name("Tag", &version)?;
        let template_is_shield = Self::pouch_category(
            &clean_rsdb.join(&pouch_actor_info),
            &request.template_actor,
            self.zstd.clone(),
        )?
        .as_deref()
            == Some("Shield");

        let model_name = request.model_name.as_deref().unwrap_or(&request.actor_name);
        let mut actor = request.overrides.actor_info.clone();
        actor.insert(
            "ActorName".into(),
            JsonValue::String(request.actor_name.clone()),
        );
        actor.insert("FmdbName".into(), JsonValue::String(model_name.into()));
        actor.insert(
            "ModelProjectName".into(),
            JsonValue::String(model_name.into()),
        );

        let mut attachment = request.overrides.attachment_actor_info.clone();
        if let Some(value) = request.attachment_damage {
            attachment.insert("AttachmentAdditionalDamage".into(), value.into());
        }
        if !template_is_shield {
            if let Some(value) = request.shield_bash_damage {
                attachment.insert("AttachmentShieldBashDamage".into(), value.into());
            }
        }

        let mut game = request.overrides.game_actor_info.clone();
        if let Some(value) = request.max_life {
            game.insert("MaxLife".into(), value.into());
        }
        let mut pouch = request.overrides.pouch_actor_info.clone();
        if let Some(value) = request.equipment_performance {
            pouch.insert("EquipmentPerformance".into(), value.into());
        }
        if let Some(value) = request.buying_price {
            pouch.insert("BuyingPrice".into(), value.into());
        }
        if let Some(value) = request.selling_price {
            pouch.insert("SellingPrice".into(), value.into());
        }

        let tables = [
            (actor_info, actor),
            (attachment_info, attachment),
            (game_actor_info, game),
            (pouch_actor_info, pouch),
        ];
        let mut outputs = Vec::with_capacity(5);
        for (name, overrides) in tables {
            let destination = output_rsdb.join(&name);
            let clean_source = clean_rsdb.join(&name);
            let source = if destination.is_file() {
                &destination
            } else {
                &clean_source
            };
            Self::clone_rsdb_row(
                source,
                &destination,
                &request.template_actor,
                &request.actor_name,
                &overrides,
                &[],
                self.zstd.clone(),
            )?;
            outputs.push(destination);
        }
        let tag_output = output_rsdb.join(&tag_product);
        let clean_tag = clean_rsdb.join(&tag_product);
        let tag_source = if tag_output.is_file() {
            &tag_output
        } else {
            &clean_tag
        };
        Self::clone_tag_entry(
            tag_source,
            &tag_output,
            &request.template_actor,
            &request.actor_name,
            self.zstd.clone(),
        )?;
        outputs.push(tag_output);
        Ok(outputs)
    }

    pub(super) fn versioned_rsdb_name(product: &str, version: &str) -> io::Result<String> {
        super::version::product_name(&format!("{product}.Product."), version, PRODUCT_SUFFIX)
    }

    pub(super) fn clone_rsdb_row(
        source: &Path,
        destination: &Path,
        template_actor: &str,
        actor_name: &str,
        overrides: &BTreeMap<String, JsonValue>,
        removals: &[&str],
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<()> {
        let mut file = BymlFile::new(source, zstd.clone())
            .ok_or_else(|| Self::invalid_data(format!("failed to parse {}", source.display())))?;
        let root = &mut file.pio;
        let rows = root.as_mut_array().map_err(|_| {
            Self::invalid_data(format!("RSDB root is not an array: {}", source.display()))
        })?;
        let template = rows
            .iter()
            .find(|row| Self::row_id(row).as_deref() == Some(template_actor))
            .cloned()
            .ok_or_else(|| {
                Self::invalid_data(format!("template row {template_actor} is missing"))
            })?;
        let mut row = template;
        let map = row.as_mut_map().map_err(|_| {
            Self::invalid_data(format!("template row {template_actor} is not a map"))
        })?;
        map.insert("__RowId".into(), Byml::String(actor_name.into()));
        for key in removals {
            map.remove(*key);
        }
        for (key, value) in overrides {
            let converted = if let Some(current) = map.get(key.as_str()) {
                Self::json_to_matching_byml(value, current, key)?
            } else if matches!(key.as_str(), "BuyingPrice" | "SellingPrice") {
                Byml::I32(
                    value
                        .as_i64()
                        .and_then(|value| value.try_into().ok())
                        .ok_or_else(|| Self::invalid(format!("{key} must be a 32-bit integer")))?,
                )
            } else {
                return Err(Self::invalid(format!("template row has no field {key}")));
            };
            map.insert(key.clone().into(), converted);
        }
        if let Some(index) = rows
            .iter()
            .position(|candidate| Self::row_id(candidate).as_deref() == Some(actor_name))
        {
            let existing = rows
                .get_mut(index)
                .ok_or_else(|| Self::invalid_data("located RSDB row index is invalid"))?;
            *existing = row;
        } else {
            rows.push(row);
        }
        rows.sort_by_key(|candidate| Self::row_id(candidate).unwrap_or_default());
        let rebuilt = file.to_binary_preserving_header()?;
        let reparsed = BymlFile::from_binary(&rebuilt, zstd, destination)?;
        if !reparsed
            .pio
            .as_array()
            .map_err(|_| Self::invalid_data("generated RSDB root is not an array"))?
            .iter()
            .any(|candidate| Self::row_id(candidate).as_deref() == Some(actor_name))
        {
            return Err(Self::invalid_data("generated RSDB row is missing"));
        }
        file.save(destination.to_string_lossy().into_owned())
    }

    pub(super) fn clone_tag_entry(
        source: &Path,
        destination: &Path,
        template_actor: &str,
        actor_name: &str,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<()> {
        let compressed = fs::read(source)?;
        let mut tag = TagProduct::from_binary(&compressed, source, zstd.clone())
            .ok_or_else(|| Self::invalid_data("failed to parse clean Tag.Product"))?;
        let template_path = Self::actor_tag_path(template_actor);
        let new_path = Self::actor_tag_path(actor_name);
        let inherited = tag
            .actor_tag_data
            .get(&template_path)
            .cloned()
            .ok_or_else(|| {
                Self::invalid_data(format!("template tag entry is missing: {template_path}"))
            })?;
        let mut selected = inherited;
        selected.sort();
        selected.dedup();
        tag.actor_tag_data.insert(new_path.clone(), selected);
        let text = tag.to_text();
        tag.save(destination.to_string_lossy().into_owned(), &text)?;
        let saved = fs::read(destination)?;
        let verification = TagProduct::from_binary(&saved, destination, zstd)
            .ok_or_else(|| Self::invalid_data("generated Tag.Product is invalid"))?;
        if !verification.actor_tag_data.contains_key(&new_path) {
            return Err(Self::invalid_data("generated Tag.Product entry is missing"));
        }
        Ok(())
    }

    fn actor_tag_path(actor: &str) -> String {
        format!("Work/Actor/|{actor}|.engine__actor__ActorParam.gyml")
    }

    fn row_id(row: &Byml) -> Option<String> {
        row.as_map()
            .ok()?
            .get("__RowId")?
            .as_string()
            .ok()
            .map(ToString::to_string)
    }

    fn pouch_category(
        source: &Path,
        actor_name: &str,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<Option<String>> {
        let file = BymlFile::new(source, zstd)
            .ok_or_else(|| Self::invalid_data(format!("failed to parse {}", source.display())))?;
        let rows = file
            .pio
            .as_array()
            .map_err(|_| Self::invalid_data("PouchActorInfo root is not an array"))?;
        let row = rows
            .iter()
            .find(|row| Self::row_id(row).as_deref() == Some(actor_name))
            .ok_or_else(|| {
                Self::invalid_data(format!("PouchActorInfo row is missing: {actor_name}"))
            })?;
        Ok(row
            .as_map()
            .ok()
            .and_then(|row| row.get("PouchCategory"))
            .and_then(|value| value.as_string().ok())
            .map(ToString::to_string))
    }

    fn json_to_matching_byml(value: &JsonValue, template: &Byml, path: &str) -> io::Result<Byml> {
        let mismatch = || {
            Self::invalid(format!(
                "JSON value for {path} does not match the template type"
            ))
        };
        Ok(match template {
            Byml::String(_) => Byml::String(value.as_str().ok_or_else(mismatch)?.into()),
            Byml::Bool(_) => Byml::Bool(value.as_bool().ok_or_else(mismatch)?),
            Byml::I32(_) => Byml::I32(
                Self::json_i64(value, path)?
                    .try_into()
                    .map_err(|_| mismatch())?,
            ),
            Byml::U32(_) => Byml::U32(
                Self::json_u64(value, path)?
                    .try_into()
                    .map_err(|_| mismatch())?,
            ),
            Byml::I64(_) => Byml::I64(Self::json_i64(value, path)?),
            Byml::U64(_) => Byml::U64(Self::json_u64(value, path)?),
            Byml::Float(_) => Byml::Float(value.as_f64().ok_or_else(mismatch)? as f32),
            Byml::Double(_) => Byml::Double(value.as_f64().ok_or_else(mismatch)?),
            Byml::Null if value.is_null() => Byml::Null,
            Byml::Map(template_map) => {
                let object = value.as_object().ok_or_else(mismatch)?;
                let mut result = template_map.clone();
                for (key, child) in object {
                    let current = result.get(key.as_str()).ok_or_else(|| {
                        Self::invalid(format!("template value has no field {path}.{key}"))
                    })?;
                    result.insert(
                        key.clone().into(),
                        Self::json_to_matching_byml(child, current, &format!("{path}.{key}"))?,
                    );
                }
                Byml::Map(result)
            }
            Byml::Array(template_array) => {
                let array = value.as_array().ok_or_else(mismatch)?;
                if array.len() != template_array.len() {
                    return Err(Self::invalid(format!("array length differs for {path}")));
                }
                Byml::Array(
                    array
                        .iter()
                        .zip(template_array)
                        .enumerate()
                        .map(|(index, (child, current))| {
                            Self::json_to_matching_byml(child, current, &format!("{path}[{index}]"))
                        })
                        .collect::<io::Result<Vec<_>>>()?,
                )
            }
            _ => return Err(mismatch()),
        })
    }

    fn json_i64(value: &JsonValue, path: &str) -> io::Result<i64> {
        value
            .as_i64()
            .ok_or_else(|| Self::invalid(format!("{path} must be an integer")))
    }

    fn json_u64(value: &JsonValue, path: &str) -> io::Result<u64> {
        value
            .as_u64()
            .ok_or_else(|| Self::invalid(format!("{path} must be a nonnegative integer")))
    }

    fn validate_actor_name(actor: &str) -> io::Result<()> {
        if actor.is_empty()
            || !actor
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err(Self::invalid(format!("invalid actor name: {actor}")));
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
        if output.starts_with(clean) {
            return Err(Self::invalid("output must be outside clean ROMFS"));
        }
        Ok(())
    }

    fn invalid(message: impl Into<String>) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput, message.into())
    }

    fn invalid_data(message: impl Into<String>) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, message.into())
    }
}

pub(super) fn template_is_shield(
    clean_romfs: &Path,
    actor_name: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<bool> {
    let clean_rsdb = clean_romfs.join("RSDB");
    let (version, _) =
        super::version::discover_product_file(&clean_rsdb, ACTOR_INFO_PREFIX, PRODUCT_SUFFIX)?;
    let pouch = clean_rsdb.join(WeaponRsdbProcessor::versioned_rsdb_name(
        "PouchActorInfo",
        &version,
    )?);
    Ok(WeaponRsdbProcessor::pouch_category(&pouch, actor_name, zstd)?.as_deref() == Some("Shield"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};

    #[test]
    fn json_overrides_preserve_template_types() {
        assert_eq!(
            WeaponRsdbProcessor::json_to_matching_byml(
                &JsonValue::from(42),
                &Byml::I32(8),
                "Attack",
            )
            .unwrap(),
            Byml::I32(42)
        );
        assert!(WeaponRsdbProcessor::json_to_matching_byml(
            &JsonValue::String("42".into()),
            &Byml::I32(8),
            "Attack",
        )
        .is_err());
    }

    #[test]
    fn request_requires_custom_and_template_names() {
        assert!(WeaponRsdbRequest::from_json(r#"{"name":"Weapon_Lsword_900"}"#).is_err());
        let request = WeaponRsdbRequest::from_json(
            r#"{
                "name": "Weapon_Lsword_900",
                "base": "Weapon_Lsword_108",
                "life": 77,
                "attack": 42
            }"#,
        )
        .unwrap();
        assert_eq!(request.max_life, Some(77));
        assert_eq!(request.equipment_performance, Some(42));
    }

    #[test]
    #[ignore = "requires a configured clean ROMFS"]
    fn generates_five_reopenable_weapon_rsdb_products() {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let Ok((version, _)) = super::super::version::discover_product_file(
            &clean_romfs.join("RSDB"),
            ACTOR_INFO_PREFIX,
            PRODUCT_SUFFIX,
        ) else {
            return;
        };
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        let request = WeaponRsdbRequest {
            actor_name: "Weapon_Lsword_900".into(),
            template_actor: "Weapon_Lsword_108".into(),
            model_name: Some("CustomBlade".into()),
            max_life: Some(77),
            equipment_performance: Some(42),
            buying_price: Some(500),
            selling_price: Some(250),
            attachment_damage: Some(42),
            shield_bash_damage: Some(42),
            overrides: WeaponRsdbOverrides::default(),
        };
        let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/rsdb_generated_romfs");
        let generated = request
            .generate(clean_romfs, &output, zstd.clone())
            .unwrap();
        assert_eq!(generated.len(), 5);
        for product in [
            "ActorInfo",
            "AttachmentActorInfo",
            "GameActorInfo",
            "PouchActorInfo",
        ] {
            let name = WeaponRsdbProcessor::versioned_rsdb_name(product, &version).unwrap();
            let raw = zstd
                .decompress_zs(&fs::read(output.join("RSDB").join(&name)).unwrap())
                .unwrap();
            let root = Byml::from_binary(&raw).unwrap();
            let row = root
                .as_array()
                .unwrap()
                .iter()
                .find(|row| {
                    WeaponRsdbProcessor::row_id(row).as_deref() == Some("Weapon_Lsword_900")
                })
                .unwrap();
            if product == "PouchActorInfo" {
                let row = row.as_map().unwrap();
                assert_eq!(row.get("BuyingPrice"), Some(&Byml::I32(500)));
                assert_eq!(row.get("SellingPrice"), Some(&Byml::I32(250)));
            }
        }
        let tag_name = WeaponRsdbProcessor::versioned_rsdb_name("Tag", &version).unwrap();
        let tag_bytes = fs::read(output.join("RSDB").join(&tag_name)).unwrap();
        let tag = TagProduct::from_binary(&tag_bytes, &tag_name, zstd).unwrap();
        assert!(tag
            .actor_tag_data
            .contains_key(&WeaponRsdbProcessor::actor_tag_path("Weapon_Lsword_900",)));
        fs::remove_dir_all(output).unwrap();
    }
}
