//! Existing shop pack processing: Beedle (`Npc_TripMaster_*`, rupees) and
//! the Bargainer Statues (poes).

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

/// Spec `actor_name` that stands for every Bargainer Statue at once.
pub const BARGAINER_STATUE: &str = "BargainerStatue";

/// Every actor whose `ShopRef` is the poe trade list: the six
/// `TwnObj_DemonStatue_C_*` statues (`C_02`..`C_06` inherit `C_01`'s
/// ActorParam and carry the same `DemonStatue_Shop` ShopParam) plus the
/// Lookout Landing statue `FldObj_AmosStatue_A_02`, which the vanilla lists
/// bind into one shared stock (`StockNumShareTargetList`).
/// `TwnObj_DemonStatue_A_01` and `FldObj_AmosStatue_A_03` have no shop.
pub const BARGAINER_STATUE_ACTORS: [&str; 7] = [
    "TwnObj_DemonStatue_C_01",
    "TwnObj_DemonStatue_C_02",
    "TwnObj_DemonStatue_C_03",
    "TwnObj_DemonStatue_C_04",
    "TwnObj_DemonStatue_C_05",
    "TwnObj_DemonStatue_C_06",
    "FldObj_AmosStatue_A_02",
];

/// ShopParam `Currency` of the Bargainer Statues (poes).
pub const POE_CURRENCY: &str = "MinusRupee";
/// Implicit currency of every other shop.
pub const RUPEE_CURRENCY: &str = "Rupee";

pub fn is_bargainer_statue(actor_name: &str) -> bool {
    actor_name == BARGAINER_STATUE
}

/// Actor packs a spec vendor expands to: the statue group becomes every
/// statue, anything else is itself.
pub fn vendor_pack_actors(actor_name: &str) -> Vec<String> {
    if is_bargainer_statue(actor_name) {
        BARGAINER_STATUE_ACTORS
            .iter()
            .map(|actor| (*actor).to_owned())
            .collect()
    } else {
        vec![actor_name.to_owned()]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorPackReport {
    pub output: PathBuf,
    pub vendor_actor: String,
    pub shop_param: String,
    pub weapon_actor: String,
    pub quantity: u32,
    /// `Rupee` for Beedle, `MinusRupee` (poes) for the Bargainer Statues.
    pub currency: String,
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

    /// Adds the item to every actor pack the vendor stands for (one for a
    /// Beedle, all statues for the Bargainer Statue). Writes only to mod ROMFS.
    pub fn add_weapon(
        &self,
        weapon_actor: &str,
        vendor: &VendorTarget,
    ) -> io::Result<Vec<VendorPackReport>> {
        validate_actor_name(weapon_actor)?;
        validate_vendor(vendor)?;
        super::assets::ensure_output_outside_romfs(&self.clean_romfs, &self.output_romfs)?;
        let poe = is_bargainer_statue(&vendor.actor_name);
        vendor_pack_actors(&vendor.actor_name)
            .iter()
            .map(|pack_actor| self.patch_pack(weapon_actor, pack_actor, vendor.quantity, poe))
            .collect()
    }

    fn patch_pack(
        &self,
        weapon_actor: &str,
        pack_actor: &str,
        quantity: u32,
        poe: bool,
    ) -> io::Result<VendorPackReport> {
        let pack_name = format!("{pack_actor}.pack.zs");
        // Several items may target the same merchant: keep building on the
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
        let actor_path = format!("Actor/{pack_actor}.engine__actor__ActorParam.bgyml");
        let actor = pack.byml_file(&actor_path)?.pio;
        let shop_ref = resolve_component_ref(&pack, &actor, "ShopRef", &mut BTreeSet::new())?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| invalid(format!("vendor {pack_actor} has no ShopRef")))?;
        let shop_path = reference_to_internal(&shop_ref);
        if !shop_path.starts_with("Component/ShopParam/") {
            return Err(invalid(format!(
                "vendor ShopRef is outside Component/ShopParam/: {shop_path}"
            )));
        }
        let mut shop = pack.byml_file(&shop_path)?;
        upsert_goods(&mut shop.pio, weapon_actor, quantity, poe)?;
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
        let output = self.output_romfs.join("Pack/Actor").join(&pack_name);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&output, output_bytes)?;
        Ok(VendorPackReport {
            output,
            vendor_actor: pack_actor.to_owned(),
            shop_param: shop_path,
            weapon_actor: weapon_actor.into(),
            quantity,
            currency: if poe { POE_CURRENCY } else { RUPEE_CURRENCY }.to_owned(),
        })
    }
}

/// Adds or replaces the item's GoodsList entry. `PriceOffset` stays 0 so
/// the shop charges the PouchActorInfo `BuyingPrice` (the vanilla statues
/// price their goods as `BuyingPrice + PriceOffset` in poes). Statue entries
/// carry `Currency: MinusRupee` and switch both release checks off, so the
/// item is on sale from the start regardless of story progress; the vanilla
/// `IsCheckGetFlag: true` would hide it until the player has owned one.
fn upsert_goods(shop: &mut Byml, actor: &str, quantity: u32, poe: bool) -> io::Result<()> {
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
    if poe {
        entry.insert("Currency".into(), Byml::String(POE_CURRENCY.into()));
    }
    entry.insert("PriceOffset".into(), Byml::I32(0));
    if poe {
        let mut requirements = roead::byml::Map::default();
        requirements.insert("IsCheckGetFlag".into(), Byml::Bool(false));
        requirements.insert("IsCheckReleaseGameData".into(), Byml::Bool(false));
        entry.insert("ReleaseRequirements".into(), Byml::Map(requirements));
    }
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

/// Shared by every spec: a vendor is one of Beedle's `Npc_TripMaster_*`
/// actors or the [`BARGAINER_STATUE`] group; new vendor creation is out of scope.
pub fn validate_vendor(vendor: &VendorTarget) -> io::Result<()> {
    if !vendor.actor_name.starts_with("Npc_TripMaster_") && !is_bargainer_statue(&vendor.actor_name)
    {
        return Err(invalid(format!(
            "vendor must be an existing Npc_TripMaster_* actor or {BARGAINER_STATUE}"
        )));
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
    fn validate_vendor_accepts_beedle_and_the_statue_group_only() {
        let target = |name: &str| VendorTarget {
            actor_name: name.into(),
            buying_price: None,
            selling_price: None,
            quantity: 1,
        };
        assert!(validate_vendor(&target("Npc_TripMaster_03")).is_ok());
        assert!(validate_vendor(&target(BARGAINER_STATUE)).is_ok());
        assert!(validate_vendor(&target("TwnObj_DemonStatue_C_01")).is_err());
        assert!(validate_vendor(&target("Npc_CustomVendor")).is_err());
        assert_eq!(
            vendor_pack_actors("Npc_TripMaster_03"),
            ["Npc_TripMaster_03"]
        );
        assert_eq!(
            vendor_pack_actors(BARGAINER_STATUE),
            BARGAINER_STATUE_ACTORS.map(str::to_owned)
        );
    }

    fn goods(shop: &Byml, actor: &str) -> roead::byml::Map {
        shop.as_map()
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
                    .is_some_and(|value| value.contains(actor))
            })
            .unwrap()
            .as_map()
            .unwrap()
            .clone()
    }

    #[test]
    fn statue_goods_use_poes_and_no_release_requirements() {
        let mut shop = Byml::Map(roead::byml::Map::from_iter([(
            "GoodsList".into(),
            Byml::Array(Vec::new()),
        )]));
        upsert_goods(&mut shop, "Weapon_Lsword_900", 2, true).unwrap();
        let entry = goods(&shop, "Weapon_Lsword_900");
        assert_eq!(
            entry.get("Currency"),
            Some(&Byml::String(POE_CURRENCY.into()))
        );
        assert_eq!(entry.get("PriceOffset"), Some(&Byml::I32(0)));
        assert_eq!(entry.get("StockNum"), Some(&Byml::I32(2)));
        let requirements = entry.get("ReleaseRequirements").unwrap().as_map().unwrap();
        assert_eq!(requirements.get("IsCheckGetFlag"), Some(&Byml::Bool(false)));
        assert_eq!(
            requirements.get("IsCheckReleaseGameData"),
            Some(&Byml::Bool(false))
        );

        // Beedle entries stay exactly as before: rupees, no requirements.
        upsert_goods(&mut shop, "Weapon_Lsword_901", 1, false).unwrap();
        let entry = goods(&shop, "Weapon_Lsword_901");
        assert_eq!(entry.get("Currency"), None);
        assert_eq!(entry.get("ReleaseRequirements"), None);

        // Re-adding replaces instead of duplicating.
        upsert_goods(&mut shop, "Weapon_Lsword_900", 5, true).unwrap();
        let list = shop
            .as_map()
            .unwrap()
            .get("GoodsList")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(
            goods(&shop, "Weapon_Lsword_900").get("StockNum"),
            Some(&Byml::I32(5))
        );
    }

    #[test]
    #[ignore = "requires a configured clean ROMFS statue pack"]
    fn real_statue_packs_all_receive_the_poe_entry() {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        if !clean_romfs
            .join("Pack/Actor/TwnObj_DemonStatue_C_01.pack.zs")
            .is_file()
        {
            return;
        }
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).unwrap());
        let output_romfs =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/statue_generated_romfs");
        let vendor = VendorTarget {
            actor_name: BARGAINER_STATUE.into(),
            buying_price: Some(300),
            selling_price: None,
            quantity: 1,
        };
        let reports = VendorProcessor::new(clean_romfs, &output_romfs, zstd.clone())
            .add_weapon("Armor_900_Head", &vendor)
            .unwrap();
        assert_eq!(reports.len(), BARGAINER_STATUE_ACTORS.len());
        for report in &reports {
            assert_eq!(report.currency, POE_CURRENCY);
            let pack =
                PackFile::from_binary(&fs::read(&report.output).unwrap(), zstd.clone()).unwrap();
            let shop = pack.byml_file(&report.shop_param).unwrap().pio;
            let entry = goods(&shop, "Armor_900_Head");
            assert_eq!(
                entry.get("Currency"),
                Some(&Byml::String(POE_CURRENCY.into()))
            );
            // The vanilla goods keep their poe prices and buy-back checks.
            let vanilla = goods(&shop, "Item_Material_11");
            assert_eq!(vanilla.get("PriceOffset"), Some(&Byml::I32(-10)));
        }
        fs::remove_dir_all(output_romfs).unwrap();
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
        let reports = VendorProcessor::new(clean_romfs, &output_romfs, zstd.clone())
            .add_weapon("Weapon_Lsword_900", &vendor)
            .unwrap();
        assert_eq!(reports.len(), 1);
        let report = &reports[0];
        assert_eq!(report.currency, RUPEE_CURRENCY);
        assert_eq!(fs::read(&source).unwrap(), original);
        let pack = PackFile::from_binary(&fs::read(&report.output).unwrap(), zstd).unwrap();
        let shop = pack.byml_file(&report.shop_param).unwrap().pio;
        let entry = goods(&shop, "Weapon_Lsword_900");
        assert_eq!(entry.get("Currency"), None);
        assert_eq!(entry.get("PriceOffset"), Some(&Byml::I32(0)));
        assert_eq!(entry.get("StockNum"), Some(&Byml::I32(3)));
        fs::remove_dir_all(output_romfs).unwrap();
    }
}
