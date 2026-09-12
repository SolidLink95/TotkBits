//! Existing travelling-vendor shop pack processing.

use super::VendorTarget;
use crate::{file_format::Pack::PackFile, Zstd::TotkZstd};
use roead::byml::Byml;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorPackReport {
    pub output: PathBuf,
    pub vendor_actor: String,
    pub shop_param: String,
    pub weapon_actor: String,
    pub quantity: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorGenerationReport {
    pub vendor_packs: Vec<VendorPackReport>,
    pub rsdb_outputs: Vec<PathBuf>,
}

pub struct VendorProcessor<'a> {
    clean_romfs: PathBuf,
    output_romfs: PathBuf,
    zstd: Arc<TotkZstd<'a>>,
}

impl<'a> VendorProcessor<'a> {
    pub fn new(clean_romfs: &Path, output_romfs: &Path, zstd: Arc<TotkZstd<'a>>) -> Self {
        Self {
            clean_romfs: clean_romfs.to_path_buf(),
            output_romfs: output_romfs.to_path_buf(),
            zstd,
        }
    }

    pub fn add_weapon(
        &self,
        weapon_actor: &str,
        vendor: &VendorTarget,
    ) -> io::Result<VendorPackReport> {
        validate_actor_name(weapon_actor)?;
        validate_vendor(vendor)?;
        super::assets::ensure_output_outside_romfs(&self.clean_romfs, &self.output_romfs)?;
        let pack_name = format!("{}.pack.zs", vendor.actor_name);
        // Several weapons may target the same merchant: keep building on the
        // pack already written to the output ROMFS so earlier goods survive.
        let generated = self.output_romfs.join("Pack/Actor").join(&pack_name);
        let source = if generated.is_file() {
            generated
        } else {
            self.clean_romfs.join("Pack/Actor").join(&pack_name)
        };
        if !source.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("vendor actor pack is missing: {}", source.display()),
            ));
        }
        let source_bytes = fs::read(&source)?;
        let pack = PackFile::from_binary(&source_bytes, self.zstd.clone())?;
        let actor_path = format!(
            "Actor/{}.engine__actor__ActorParam.bgyml",
            vendor.actor_name
        );
        let actor = pack.byml_file(&actor_path)?.pio;
        let shop_ref = resolve_component_ref(&pack, &actor, "ShopRef", &mut BTreeSet::new())?
            .ok_or_else(|| invalid(format!("vendor {} has no ShopRef", vendor.actor_name)))?;
        let shop_path = reference_to_internal(&shop_ref);
        if !shop_path.starts_with("Component/ShopParam/") {
            return Err(invalid(format!(
                "vendor ShopRef is outside Component/ShopParam/: {shop_path}"
            )));
        }
        let mut shop = pack.byml_file(&shop_path)?;
        upsert_goods(&mut shop.pio, weapon_actor, vendor.quantity)?;
        let rebuilt_shop = shop.to_binary_preserving_header()?;

        let mut entries = Vec::new();
        for file in pack.sarc.files() {
            let name = file
                .name()
                .ok_or_else(|| invalid_data("vendor pack contains unnamed entry"))?;
            entries.push((
                name.to_owned(),
                if name == shop_path {
                    rebuilt_shop.clone()
                } else {
                    file.data().to_vec()
                },
            ));
        }
        let output_bytes = pack.rebuild_binary(entries)?;
        let output = self
            .output_romfs
            .join("Pack/Actor")
            .join(format!("{}.pack.zs", vendor.actor_name));
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&output, output_bytes)?;
        Ok(VendorPackReport {
            output,
            vendor_actor: vendor.actor_name.clone(),
            shop_param: shop_path,
            weapon_actor: weapon_actor.into(),
            quantity: vendor.quantity,
        })
    }
}

fn upsert_goods(shop: &mut Byml, actor: &str, quantity: u32) -> io::Result<()> {
    let stock: i32 = quantity
        .try_into()
        .map_err(|_| invalid("vendor quantity exceeds i32 range"))?;
    let goods = shop
        .as_mut_map()
        .map_err(|_| invalid_data("ShopParam root is not a map"))?
        .get_mut("GoodsList")
        .ok_or_else(|| invalid_data("ShopParam has no GoodsList"))?
        .as_mut_array()
        .map_err(|_| invalid_data("ShopParam GoodsList is not an array"))?;
    let actor_path = format!("Work/Actor/{actor}.engine__actor__ActorParam.gyml");
    let mut entry = roead::byml::Map::default();
    entry.insert("Actor".into(), Byml::String(actor_path.clone().into()));
    entry.insert("PriceOffset".into(), Byml::I32(0));
    entry.insert("StockNum".into(), Byml::I32(stock));
    let entry = Byml::Map(entry);
    if let Some(existing) = goods.iter_mut().find(|item| {
        item.as_map()
            .ok()
            .and_then(|map| map.get("Actor"))
            .and_then(|value| value.as_string().ok())
            .is_some_and(|value| value.as_str() == actor_path)
    }) {
        *existing = entry;
    } else {
        goods.push(entry);
    }
    Ok(())
}

fn resolve_component_ref(
    pack: &PackFile<'_>,
    actor: &Byml,
    key: &str,
    visited: &mut BTreeSet<String>,
) -> io::Result<Option<String>> {
    let map = actor
        .as_map()
        .map_err(|_| invalid_data("ActorParam root is not a map"))?;
    if let Some(value) = map
        .get("Components")
        .and_then(|value| value.as_map().ok())
        .and_then(|components| components.get(key))
        .and_then(|value| value.as_string().ok())
    {
        return Ok(Some(value.to_string()));
    }
    let Some(parent) = map.get("$parent").and_then(|value| value.as_string().ok()) else {
        return Ok(None);
    };
    let parent = reference_to_internal(parent.trim_start_matches("Work/"));
    if !visited.insert(parent.clone()) {
        return Err(invalid_data(format!("ActorParam parent cycle at {parent}")));
    }
    let parent = pack.byml_file(&parent)?.pio;
    resolve_component_ref(pack, &parent, key, visited)
}

fn reference_to_internal(value: &str) -> String {
    value.trim_start_matches('?').replace(".gyml", ".bgyml")
}

fn validate_vendor(vendor: &VendorTarget) -> io::Result<()> {
    if !vendor.actor_name.starts_with("Npc_TripMaster_") {
        return Err(invalid("vendor must be an existing Npc_TripMaster_* actor"));
    }
    validate_actor_name(&vendor.actor_name)?;
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
    Ok(())
}

fn validate_actor_name(value: &str) -> io::Result<()> {
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(invalid(format!("invalid actor name: {value}")));
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
    use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};

    #[test]
    fn vendor_json_defaults_quantity_to_one() {
        let vendor: VendorTarget =
            serde_json::from_str(r#"{"actor_name":"Npc_TripMaster_00"}"#).unwrap();
        assert_eq!(vendor.quantity, 1);
        assert_eq!(vendor.buying_price, None);
        assert_eq!(vendor.selling_price, None);
    }

    #[test]
    #[ignore = "requires a configured clean ROMFS vendor pack"]
    fn real_vendor_pack_adds_zero_offset_weapon_entry() {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let source = clean_romfs.join("Pack/Actor/Npc_TripMaster_00.pack.zs");
        if !source.is_file() {
            return;
        }
        let original = fs::read(&source).unwrap();
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).unwrap());
        let output_romfs =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/vendor_generated_romfs");
        let vendor = VendorTarget {
            actor_name: "Npc_TripMaster_00".into(),
            buying_price: Some(500),
            selling_price: Some(250),
            quantity: 3,
        };
        let report = VendorProcessor::new(clean_romfs, &output_romfs, zstd.clone())
            .add_weapon("Weapon_Lsword_900", &vendor)
            .unwrap();
        assert_eq!(fs::read(&source).unwrap(), original);
        let pack = PackFile::from_binary(&fs::read(&report.output).unwrap(), zstd).unwrap();
        let shop = pack.byml_file(&report.shop_param).unwrap().pio;
        let entry = shop
            .as_map()
            .unwrap()
            .get("GoodsList")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| {
                entry
                    .as_map()
                    .ok()
                    .and_then(|map| map.get("Actor"))
                    .and_then(|value| value.as_string().ok())
                    .is_some_and(|value| value.contains("Weapon_Lsword_900"))
            })
            .unwrap()
            .as_map()
            .unwrap();
        assert_eq!(entry.get("PriceOffset"), Some(&Byml::I32(0)));
        assert_eq!(entry.get("StockNum"), Some(&Byml::I32(3)));
        fs::remove_dir_all(output_romfs).unwrap();
    }
}
