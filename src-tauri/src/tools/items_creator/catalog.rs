//! Read-only lookups for the Item Creator UI: which vanilla actors can serve
//! as templates (with their pouch names), which travelling merchants exist,
//! the values a template carries, and the cached base icons.

use super::{actor_pack, armor::ArmorSlot, WeaponKind};
use crate::{
    file_format::{BinTextFile::BymlFile, Pack::PackFile},
    parser::msbt::{token::TextPart, Msbt},
    Zstd::TotkZstd,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateEntry {
    /// Vanilla actor, for example `Weapon_Sword_001` or `Armor_001_Head`.
    pub actor: String,
    /// `SmallSword`, `LargeSword`, `Spear`, `Bow`, `Shield`, `Head`, `Upper`, `Lower`.
    pub kind: String,
    /// Pouch display name (control tags removed), empty when the actor has no label.
    pub name: String,
    pub has_icon: bool,
    /// Weapon whose model is the rusted variant: another actor renders the
    /// same model project through its `<project>_Before` (pristine) mesh
    /// while this actor renders the plain one.
    pub decayed: bool,
    /// Weapon rendering a `_Before` (pristine, pre-decay) mesh.
    pub pristine: bool,
}

/// One travelling merchant (Beedle) and where he stands.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorEntry {
    pub actor: String,
    /// `Location.msbt` label of the stable or town he stands at, empty when unknown.
    pub location_label: String,
    /// Localized name of that place ("New Serenne Stable"), empty when unknown.
    pub location: String,
}

/// Where each `Npc_TripMaster_*` stands, derived from his placement
/// coordinates and the nearest `Stable` marker of
/// `Banc/MainField/LocationArea/MainField.locationarea.byml.zs`
/// (`Npc_TripMaster_14` stands in Kara Kara Bazaar, 560 m from any stable).
const BEEDLE_LOCATIONS: [(&str, &str, &str); 14] = [
    (
        "Npc_TripMaster_00",
        "NewHyruleWestHatago",
        "New Serenne Stable",
    ),
    ("Npc_TripMaster_01", "NorthHatelHatago", "Wetland Stable"),
    ("Npc_TripMaster_02", "RiverSideHatago", "Riverside Stable"),
    ("Npc_TripMaster_03", "HyruleDepthHatago", "Outskirt Stable"),
    ("Npc_TripMaster_04", "ForestHatago", "Woodland Stable"),
    (
        "Npc_TripMaster_05",
        "DeathMountainHatago",
        "Foothill Stable",
    ),
    (
        "Npc_TripMaster_07",
        "FaronHatago000",
        "Dueling Peaks Stable",
    ),
    (
        "Npc_TripMaster_08",
        "TabantaBridgeHatago",
        "Tabantha Bridge Stable",
    ),
    ("Npc_TripMaster_09", "FaronHatago002", "Highland Stable"),
    ("Npc_TripMaster_10", "FaronHatago001", "Lakeside Stable"),
    ("Npc_TripMaster_11", "TabantaHatago", "Snowfield Stable"),
    (
        "Npc_TripMaster_12",
        "TamurulHatago_02",
        "South Akkala Stable",
    ),
    ("Npc_TripMaster_13", "TamourHatago", "East Akkala Stable"),
    ("Npc_TripMaster_14", "Oasis", "Kara Kara Bazaar"),
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub romfs: String,
    pub icon_dir: Option<String>,
    pub templates: Vec<TemplateEntry>,
    /// `Npc_TripMaster_*` actors that can sell the items, with their stable.
    pub vendors: Vec<VendorEntry>,
}

/// Values a template carries, used to pre-fill the form.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub actor: String,
    pub name: String,
    pub description: String,
    pub weapon_type: Option<String>,
    pub base_attack: Option<i32>,
    pub max_life: Option<i32>,
    pub additional_damage: Option<i32>,
    pub shield_bash_damage: Option<i32>,
    pub defense: Option<i32>,
    pub series_name: Option<String>,
    pub buying_price: Option<i32>,
    pub selling_price: Option<i32>,
}

const WEAPON_KINDS: [(WeaponKind, &str); 5] = [
    (WeaponKind::SmallSword, "SmallSword"),
    (WeaponKind::LargeSword, "LargeSword"),
    (WeaponKind::Spear, "Spear"),
    (WeaponKind::Bow, "Bow"),
    (WeaponKind::Shield, "Shield"),
];

/// Lists every vanilla weapon/shield/bow/armor base actor with its pouch name.
pub fn catalog(clean_romfs: &Path, zstd: Arc<TotkZstd<'_>>) -> io::Result<Catalog> {
    let actor_dir = clean_romfs.join("Pack/Actor");
    if !actor_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("RomFS has no Pack/Actor folder: {}", actor_dir.display()),
        ));
    }
    let names = pouch_labels(clean_romfs, zstd.clone()).unwrap_or_default();
    let places = location_names(clean_romfs, zstd.clone()).unwrap_or_default();
    let (decayed_actors, pristine_actors) = weapon_variants(clean_romfs, zstd).unwrap_or_default();
    let icon_dir = icon_directory();
    let mut templates = Vec::new();
    let mut vendors = Vec::new();
    for entry in fs::read_dir(&actor_dir)? {
        let path = entry?.path();
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(actor) = file_name.strip_suffix(".pack.zs") else {
            continue;
        };
        if actor.starts_with("Npc_TripMaster_") {
            let known = BEEDLE_LOCATIONS
                .iter()
                .find(|(vendor, _, _)| *vendor == actor);
            vendors.push(VendorEntry {
                actor: actor.to_owned(),
                location_label: known
                    .map(|(_, label, _)| (*label).to_owned())
                    .unwrap_or_default(),
                location: known
                    .map(|(_, label, fallback)| {
                        places
                            .get(*label)
                            .cloned()
                            .filter(|text| !text.is_empty())
                            .unwrap_or_else(|| (*fallback).to_owned())
                    })
                    .unwrap_or_default(),
            });
            continue;
        }
        let kind = if actor.starts_with("Armor_") {
            let Some(slot) = ArmorSlot::from_actor_name(actor) else {
                continue;
            };
            let numeric_id = actor
                .strip_prefix("Armor_")
                .and_then(|rest| rest.split('_').next())
                .is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()));
            if !numeric_id {
                continue;
            }
            format!("{slot:?}")
        } else {
            let Some((kind, label)) = WEAPON_KINDS
                .iter()
                .find(|(kind, _)| actor.starts_with(kind.actor_prefix()))
            else {
                continue;
            };
            let numeric_id = actor
                .strip_prefix(kind.actor_prefix())
                .is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()));
            if !numeric_id {
                continue;
            }
            (*label).to_owned()
        };
        let has_icon = icon_dir
            .as_ref()
            .is_some_and(|dir| dir.join(format!("{actor}.webp")).is_file());
        let raw_name = names
            .get(actor)
            .map(|(name, _)| name.as_str())
            .unwrap_or_default();
        templates.push(TemplateEntry {
            name: strip_control_tags(raw_name),
            pristine: pristine_actors.contains(actor),
            decayed: decayed_actors.contains(actor),
            actor: actor.to_owned(),
            kind,
            has_icon,
        });
    }
    templates.sort_by(|a, b| a.actor.cmp(&b.actor));
    vendors.sort_by(|a, b| a.actor.cmp(&b.actor));
    Ok(Catalog {
        romfs: clean_romfs.to_string_lossy().into_owned(),
        icon_dir: icon_dir.map(|dir| dir.to_string_lossy().into_owned()),
        templates,
        vendors,
    })
}

/// Reads the values of one template actor for pre-filling the form.
pub fn template_info(
    clean_romfs: &Path,
    actor: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<TemplateInfo> {
    let pack_path = clean_romfs
        .join("Pack/Actor")
        .join(format!("{actor}.pack.zs"));
    let bytes = fs::read(&pack_path)?;
    let mut info = TemplateInfo {
        actor: actor.to_owned(),
        ..Default::default()
    };
    if let Ok(labels) = pouch_labels(clean_romfs, zstd.clone()) {
        if let Some((name, caption)) = labels.get(actor) {
            info.name = strip_control_tags(name);
            info.description = strip_control_tags(caption);
        }
    }
    if actor.starts_with("Armor_") {
        let pack = PackFile::from_binary(&bytes, zstd.clone())?;
        let armor_path = format!("Component/ArmorParam/{actor}.game__component__ArmorParam.bgyml");
        if let Ok(armor) = pack.byml_file(&armor_path) {
            if let Ok(map) = armor.pio.as_map() {
                info.defense = map.get("BaseDefense").and_then(|v| v.as_i32().ok());
                info.series_name = map
                    .get("SeriesName")
                    .and_then(|v| v.as_string().ok())
                    .map(ToString::to_string);
            }
        }
    } else if let Ok(weapon) = actor_pack::load_weapon_actor_info(&bytes, actor, zstd.clone()) {
        info.weapon_type = weapon.weapon_type.clone();
        info.base_attack = weapon.base_attack;
        info.max_life = weapon.durability;
        info.additional_damage = weapon.attachment.additional_damage;
        info.shield_bash_damage = weapon.attachment.shield_bash_damage;
    }
    if let Some((buying, selling)) = pouch_prices(clean_romfs, actor, zstd) {
        info.buying_price = buying;
        info.selling_price = selling;
    }
    Ok(info)
}

/// Base icons cached as `.cache/webp/<actor>.webp`, returned as data URLs.
pub fn icons(names: &[String]) -> HashMap<String, String> {
    use base64::Engine;
    let mut result = HashMap::new();
    let Some(dir) = icon_directory() else {
        return result;
    };
    for name in names {
        if name.is_empty() || name.contains(['/', '\\', '.']) {
            continue;
        }
        if let Ok(bytes) = fs::read(dir.join(format!("{name}.webp"))) {
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            result.insert(name.clone(), format!("data:image/webp;base64,{encoded}"));
        }
    }
    result
}

/// `.cache/webp` next to the executable, above the build directory, or in
/// the repository during development.
pub fn icon_directory() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe_dir) = crate::utils::running_exe_dir() {
        candidates.push(exe_dir.join(".cache/webp"));
        let mut ancestor = exe_dir.clone();
        for _ in 0..3 {
            if let Some(parent) = ancestor.parent() {
                ancestor = parent.to_path_buf();
                candidates.push(ancestor.join(".cache/webp"));
            }
        }
    }
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.cache/webp"));
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(".cache/webp"));
    }
    candidates.into_iter().find(|path| path.is_dir())
}

/// Removes MSBT control tags such as `{{icon type="PristineWeaponSparkle"}}`.
fn strip_control_tags(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        match rest[start..].find("}}") {
            Some(end) => rest = &rest[start + end + 2..],
            None => {
                rest = "";
            }
        }
    }
    result.push_str(rest);
    result.trim().to_owned()
}

/// `(decayed, pristine)` weapons from `RSDB/ActorInfo`: pristine actors
/// render a `<project>_Before` FmdbName; the actor rendering the same
/// project without `_Before` is the decayed (rusted) one.
fn weapon_variants(
    clean_romfs: &Path,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<(
    std::collections::BTreeSet<String>,
    std::collections::BTreeSet<String>,
)> {
    let (_, source) = super::version::discover_product_file(
        &clean_romfs.join("RSDB"),
        "ActorInfo.Product.",
        ".rstbl.byml.zs",
    )?;
    let file = BymlFile::new(&source, zstd)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid ActorInfo"))?;
    let rows = file
        .pio
        .as_array()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "ActorInfo is not an array"))?;
    let mut models: Vec<(String, String, String)> = Vec::new();
    for row in rows {
        let Ok(map) = row.as_map() else { continue };
        let string = |key: &str| {
            map.get(key)
                .and_then(|value| value.as_string().ok())
                .map(ToString::to_string)
        };
        let (Some(actor), Some(fmdb), Some(project)) = (
            string("__RowId"),
            string("FmdbName"),
            string("ModelProjectName"),
        ) else {
            continue;
        };
        if actor.starts_with("Weapon_") {
            models.push((actor, fmdb, project));
        }
    }
    let pristine_projects: std::collections::BTreeSet<&str> = models
        .iter()
        .filter(|(_, fmdb, project)| fmdb.strip_suffix("_Before") == Some(project.as_str()))
        .map(|(_, _, project)| project.as_str())
        .collect();
    let decayed = models
        .iter()
        .filter(|(_, fmdb, project)| {
            !fmdb.ends_with("_Before") && pristine_projects.contains(project.as_str())
        })
        .map(|(actor, _, _)| actor.clone())
        .collect();
    let pristine = models
        .iter()
        .filter(|(_, fmdb, _)| fmdb.ends_with("_Before"))
        .map(|(actor, _, _)| actor.clone())
        .collect();
    Ok((decayed, pristine))
}

/// Label -> text of `LocationMsg/Location.msbt` (map location names).
fn location_names(
    clean_romfs: &Path,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<BTreeMap<String, String>> {
    let (_, source) = super::version::discover_product_file(
        &clean_romfs.join("Mals"),
        "USen.Product.",
        ".sarc.zs",
    )?;
    let pack = PackFile::from_binary(&fs::read(&source)?, zstd)?;
    let data = pack
        .sarc
        .get_data("LocationMsg/Location.msbt")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Location.msbt is missing"))?;
    let msbt = Msbt::from_bytes(data)?;
    Ok(msbt
        .messages
        .iter()
        .filter_map(|message| {
            let label = message.label.clone()?;
            let text: String = message
                .parts
                .iter()
                .filter_map(|part| match part {
                    TextPart::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            Some((label, text))
        })
        .collect())
}

/// `<actor>_Name` / `<actor>_Caption` of `ActorMsg/PouchContent.msbt`.
fn pouch_labels(
    clean_romfs: &Path,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<BTreeMap<String, (String, String)>> {
    let (_, source) = super::version::discover_product_file(
        &clean_romfs.join("Mals"),
        "USen.Product.",
        ".sarc.zs",
    )?;
    let pack = PackFile::from_binary(&fs::read(&source)?, zstd)?;
    let data = pack
        .sarc
        .get_data("ActorMsg/PouchContent.msbt")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "PouchContent.msbt is missing"))?;
    let msbt = Msbt::from_bytes(data)?;
    let mut labels: BTreeMap<String, (String, String)> = BTreeMap::new();
    for message in &msbt.messages {
        let Some(label) = message.label.as_deref() else {
            continue;
        };
        let text: String = message
            .parts
            .iter()
            .filter_map(|part| match part {
                TextPart::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        if let Some(actor) = label.strip_suffix("_Name") {
            labels.entry(actor.to_owned()).or_default().0 = text;
        } else if let Some(actor) = label.strip_suffix("_Caption") {
            labels.entry(actor.to_owned()).or_default().1 = text;
        }
    }
    Ok(labels)
}

fn pouch_prices(
    clean_romfs: &Path,
    actor: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> Option<(Option<i32>, Option<i32>)> {
    let (_, source) = super::version::discover_product_file(
        &clean_romfs.join("RSDB"),
        "PouchActorInfo.Product.",
        ".rstbl.byml.zs",
    )
    .ok()?;
    let file = BymlFile::new(&source, zstd)?;
    let rows = file.pio.as_array().ok()?;
    let row = rows.iter().find_map(|row| {
        let map = row.as_map().ok()?;
        (map.get("__RowId")?.as_string().ok()?.as_str() == actor).then_some(map)
    })?;
    Some((
        row.get("BuyingPrice").and_then(|v| v.as_i32().ok()),
        row.get("SellingPrice").and_then(|v| v.as_i32().ok()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TotkConfig::TotkConfig;

    /// Diagnostic against the TOTK dump: the catalog must list every weapon
    /// family and armor slot with pouch names, and template values must load.
    #[test]
    #[ignore = "needs the TOTK dump"]
    fn catalog_and_template_info_load_from_romfs() {
        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        if !romfs.is_dir() {
            return;
        }
        let mut config = TotkConfig::safe_new(false).unwrap();
        config.romfs = romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), 16).unwrap());
        let catalog = catalog(romfs, zstd.clone()).unwrap();
        let mut per_kind: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
        for template in &catalog.templates {
            let entry = per_kind.entry(template.kind.as_str()).or_default();
            entry.0 += 1;
            entry.1 += usize::from(!template.name.is_empty());
            entry.2 += usize::from(template.has_icon);
        }
        eprintln!("icon dir: {:?}", catalog.icon_dir);
        eprintln!("vendors: {:?}", catalog.vendors);
        for (kind, (count, named, with_icon)) in &per_kind {
            eprintln!("{kind}: {count} templates, {named} named, {with_icon} with icon");
        }
        let decayed: Vec<_> = catalog.templates.iter().filter(|t| t.decayed).collect();
        let pristine: Vec<_> = catalog.templates.iter().filter(|t| t.pristine).collect();
        eprintln!(
            "{} decayed, {} pristine; e.g. {:?}",
            decayed.len(),
            pristine.len(),
            decayed
                .iter()
                .take(4)
                .map(|t| (t.actor.as_str(), t.name.as_str()))
                .collect::<Vec<_>>()
        );
        assert!(decayed.iter().any(|t| t.actor == "Weapon_Sword_106"));
        assert!(pristine.iter().any(|t| t.actor == "Weapon_Sword_001"));
        assert!(catalog.templates.iter().all(|t| !t.name.contains("{{")));
        for kind in [
            "SmallSword",
            "LargeSword",
            "Spear",
            "Bow",
            "Shield",
            "Head",
            "Upper",
            "Lower",
        ] {
            assert!(
                per_kind
                    .get(kind)
                    .is_some_and(|(count, named, _)| *count > 0 && *named > 0),
                "{kind}"
            );
        }
        for vendor in &catalog.vendors {
            eprintln!(
                "{} -> {} ({})",
                vendor.actor, vendor.location, vendor.location_label
            );
        }
        let serenne = catalog
            .vendors
            .iter()
            .find(|v| v.actor == "Npc_TripMaster_00")
            .expect("Beedle 00");
        assert_eq!(serenne.location, "New Serenne Stable");
        assert!(catalog.vendors.iter().all(|v| !v.location.is_empty()));
        for actor in [
            "Weapon_Sword_001",
            "Weapon_Shield_001",
            "Weapon_Bow_001",
            "Armor_001_Head",
        ] {
            let info = template_info(romfs, actor, zstd.clone()).unwrap();
            eprintln!("{actor}: {info:?}");
            assert!(!info.name.is_empty(), "{actor} name");
            // Only armor carries shop prices in PouchActorInfo; weapons get
            // theirs from the vendor entry when a mod adds them to a shop.
            if actor.starts_with("Armor_") {
                assert!(info.buying_price.is_some(), "{actor} price");
            } else {
                assert!(info.base_attack.is_some(), "{actor} attack");
            }
        }
        let icons = icons(&[
            "Weapon_Sword_001".to_owned(),
            "Armor_001_Head".to_owned(),
            "Nope".to_owned(),
        ]);
        eprintln!(
            "icons: {:?}",
            icons
                .iter()
                .map(|(k, v)| (k.clone(), v.len()))
                .collect::<Vec<_>>()
        );
        assert_eq!(icons.len(), 2);
        assert!(icons["Weapon_Sword_001"].starts_with("data:image/webp;base64,"));
    }
}
