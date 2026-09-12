//! GameDataList generation for custom weapon inventory and compendium flags.

use crate::{
    file_format::{
        BinTextFile::{bytes_to_file, BymlFile},
        GameDataList::{recalculate_save_metadata, GameDataList},
    },
    Zstd::{TotkZstd, ZstdDictionary},
};
use roead::byml::Byml;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

const PRODUCT_PREFIX: &str = "GameDataList.Product.";
const PRODUCT_SUFFIX: &str = ".byml.zs";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WeaponGameDataRequest {
    #[serde(alias = "name")]
    pub actor_name: String,
    /// Add the compendium Struct, IsNew Bool, and State Enum entries.
    #[serde(default = "default_true")]
    pub picture_book: bool,
    /// Add IsGet and IsGetAnyway inventory flags and parent-struct links.
    #[serde(default = "default_true")]
    pub inventory_flags: bool,
}

/// Stateful processor for discovering, editing, and saving a versioned GameDataList.
pub struct GameDataListProcessor<'a> {
    clean_romfs: PathBuf,
    output_romfs: PathBuf,
    zstd: Arc<TotkZstd<'a>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponGameDataReport {
    pub output: PathBuf,
    pub product_version: String,
    pub hashes: Vec<GameDataHash>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDataHash {
    pub name: String,
    pub hash: u32,
}

impl WeaponGameDataRequest {
    pub fn from_json(text: &str) -> io::Result<Self> {
        serde_json::from_str(text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    /// Clone the user's versioned GameDataList and save the result under mod ROMFS.
    pub fn generate(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<WeaponGameDataReport> {
        GameDataListProcessor::new(clean_romfs, output_romfs, zstd).generate_weapon(self)
    }
}

impl<'a> GameDataListProcessor<'a> {
    pub fn new(clean_romfs: &Path, output_romfs: &Path, zstd: Arc<TotkZstd<'a>>) -> Self {
        Self {
            clean_romfs: clean_romfs.to_path_buf(),
            output_romfs: output_romfs.to_path_buf(),
            zstd,
        }
    }

    pub fn generate_weapon(
        &self,
        request: &WeaponGameDataRequest,
    ) -> io::Result<WeaponGameDataReport> {
        validate_actor_name(&request.actor_name)?;
        super::assets::ensure_output_outside_romfs(&self.clean_romfs, &self.output_romfs)?;
        let game_data = self.clean_romfs.join("GameData");
        let (version, source) =
            super::version::discover_product_file(&game_data, PRODUCT_PREFIX, PRODUCT_SUFFIX)?;
        let file_name = source
            .file_name()
            .ok_or_else(|| invalid_data("GameDataList filename is missing"))?;
        let output = self.output_romfs.join("GameData").join(file_name);
        let merge_source = if output.is_file() { &output } else { &source };
        let mut file = BymlFile::new(merge_source, self.zstd.clone())
            .ok_or_else(|| invalid_data("invalid clean GameDataList"))?;
        let hashes = apply_weapon_flags(
            &mut file.pio,
            &request.actor_name,
            request.picture_book,
            request.inventory_flags,
        )?;
        verify_hashes(&file.pio, &hashes)?;
        // The new flags enlarge the save data; keep the layout metadata in sync.
        recalculate_save_metadata(&mut file.pio)?;

        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = file.pio.to_text();
        let mut binary = GameDataList::text_to_binary(&text)?;
        BymlFile::from_binary(&binary, self.zstd.clone(), &output).map_err(|error| {
            invalid_data(format!(
                "GameDataList::text_to_binary produced invalid BYML: {error}"
            ))
        })?;
        if let Some(compression) = file.file_data.compression {
            binary = if compression == ZstdDictionary::Yaz0 {
                TotkZstd::compress_yaz0_with_alignment(&binary, file.file_data.yaz0_alignment)?
            } else {
                self.zstd.compress_with_dictionary(&binary, compression)?
            };
        }
        let output_text = output
            .to_str()
            .ok_or_else(|| invalid_data("GameDataList output path is not UTF-8"))?;
        bytes_to_file(binary, output_text)?;
        let saved = BymlFile::new(&output, self.zstd.clone())
            .ok_or_else(|| invalid_data("generated GameDataList cannot be reopened"))?;
        verify_hashes(&saved.pio, &hashes)?;
        Ok(WeaponGameDataReport {
            output,
            product_version: version,
            hashes,
        })
    }
}

fn default_true() -> bool {
    true
}

fn apply_weapon_flags(
    root: &mut Byml,
    actor: &str,
    picture_book: bool,
    inventory_flags: bool,
) -> io::Result<Vec<GameDataHash>> {
    let data = root
        .as_mut_map()
        .map_err(|_| invalid_data("GameDataList root is not a map"))?
        .get_mut("Data")
        .ok_or_else(|| invalid_data("GameDataList has no Data map"))?
        .as_mut_map()
        .map_err(|_| invalid_data("GameDataList Data is not a map"))?;
    let mut hashes = Vec::new();
    let actor_hash = record_hash(&mut hashes, actor);

    if inventory_flags {
        for parent in ["IsGet", "IsGetAnyway"] {
            let flag = format!("{parent}.{actor}");
            let flag_hash = record_hash(&mut hashes, &flag);
            upsert_flag(data, "Bool", bool_flag(flag_hash, 16))?;
            add_struct_member(data, murmur3_hash(parent), actor_hash, flag_hash)?;
        }
    }
    if picture_book {
        let structure = format!("PictureBookData.{actor}");
        let is_new = format!("{structure}.IsNew");
        let state = format!("{structure}.State");
        let structure_hash = record_hash(&mut hashes, &structure);
        let is_new_hash = record_hash(&mut hashes, &is_new);
        let state_hash = record_hash(&mut hashes, &state);
        let mut structure_value = roead::byml::Map::default();
        structure_value.insert(
            "DefaultValue".into(),
            Byml::Array(vec![
                hash_value_pair(murmur3_hash("IsNew"), is_new_hash),
                hash_value_pair(murmur3_hash("State"), state_hash),
            ]),
        );
        structure_value.insert("Hash".into(), Byml::U32(structure_hash));
        structure_value.insert("ResetTypeValue".into(), Byml::I32(0));
        structure_value.insert("SaveFileIndex".into(), Byml::I32(-1));
        upsert_flag(data, "Struct", Byml::Map(structure_value))?;
        upsert_flag(data, "Bool", bool_flag(is_new_hash, 80))?;
        upsert_flag(data, "Enum", picture_book_state(state_hash))?;
        add_struct_member(
            data,
            murmur3_hash("PictureBookData"),
            actor_hash,
            structure_hash,
        )?;
    }
    Ok(hashes)
}

fn bool_flag(hash: u32, reset: i32) -> Byml {
    let mut flag = roead::byml::Map::default();
    flag.insert("DefaultValue".into(), Byml::Bool(false));
    flag.insert("Hash".into(), Byml::U32(hash));
    flag.insert("ResetTypeValue".into(), Byml::I32(reset));
    flag.insert("SaveFileIndex".into(), Byml::I32(0));
    Byml::Map(flag)
}

fn picture_book_state(hash: u32) -> Byml {
    let names = ["Unopened", "TakePhoto", "Buy"];
    // Enum choices are stored as 64-bit values in the vanilla table.
    let values: Vec<_> = names
        .iter()
        .map(|name| Byml::U64(u64::from(murmur3_hash(name))))
        .collect();
    let mut flag = roead::byml::Map::default();
    flag.insert("DefaultValue".into(), Byml::U32(murmur3_hash("Unopened")));
    flag.insert("Hash".into(), Byml::U32(hash));
    flag.insert(
        "RawValues".into(),
        Byml::Array(
            names
                .iter()
                .map(|name| Byml::String((*name).into()))
                .collect(),
        ),
    );
    flag.insert("ResetTypeValue".into(), Byml::I32(80));
    flag.insert("SaveFileIndex".into(), Byml::I32(0));
    flag.insert("Values".into(), Byml::Array(values));
    Byml::Map(flag)
}

fn add_struct_member(
    data: &mut roead::byml::Map,
    struct_hash: u32,
    key_hash: u32,
    value_hash: u32,
) -> io::Result<()> {
    let structures = typed_array_mut(data, "Struct")?;
    let structure = structures
        .iter_mut()
        .find(|entry| entry_hash(entry) == Some(struct_hash))
        .ok_or_else(|| {
            invalid_data(format!(
                "required parent Struct {struct_hash:#010X} is missing"
            ))
        })?;
    let members = structure
        .as_mut_map()
        .map_err(|_| invalid_data("GameData Struct entry is not a map"))?
        .get_mut("DefaultValue")
        .ok_or_else(|| invalid_data("GameData Struct has no DefaultValue"))?
        .as_mut_array()
        .map_err(|_| invalid_data("GameData Struct DefaultValue is not an array"))?;
    if let Some(member) = members.iter_mut().find(|member| {
        member
            .as_map()
            .is_ok_and(|map| matches!(map.get("Hash"), Some(Byml::U32(hash)) if *hash == key_hash))
    }) {
        member
            .as_mut_map()
            .map_err(|_| invalid_data("GameData Struct member is not a map"))?
            .insert("Value".into(), Byml::U32(value_hash));
    } else {
        members.push(hash_value_pair(key_hash, value_hash));
    }
    members.sort_by_key(|entry| entry_hash(entry).unwrap_or_default());
    Ok(())
}

fn hash_value_pair(hash: u32, value: u32) -> Byml {
    let mut member = roead::byml::Map::default();
    member.insert("Hash".into(), Byml::U32(hash));
    member.insert("Value".into(), Byml::U32(value));
    Byml::Map(member)
}

fn upsert_flag(data: &mut roead::byml::Map, kind: &str, flag: Byml) -> io::Result<()> {
    let hash = entry_hash(&flag).ok_or_else(|| invalid_data("new GameData flag has no Hash"))?;
    let flags = typed_array_mut(data, kind)?;
    if let Some(existing) = flags
        .iter_mut()
        .find(|entry| entry_hash(entry) == Some(hash))
    {
        *existing = flag;
    } else {
        // The vanilla tables are not hash-ordered; new flags go at the end
        // so the existing layout is left untouched.
        flags.push(flag);
    }
    Ok(())
}

fn typed_array_mut<'a>(
    data: &'a mut roead::byml::Map,
    kind: &str,
) -> io::Result<&'a mut Vec<Byml>> {
    data.get_mut(kind)
        .ok_or_else(|| invalid_data(format!("GameDataList has no {kind} collection")))?
        .as_mut_array()
        .map_err(|_| invalid_data(format!("GameDataList {kind} collection is not an array")))
}

fn entry_hash(entry: &Byml) -> Option<u32> {
    match entry.as_map().ok()?.get("Hash")? {
        Byml::U32(hash) => Some(*hash),
        _ => None,
    }
}

fn verify_hashes(root: &Byml, expected: &[GameDataHash]) -> io::Result<()> {
    let data = root
        .as_map()
        .ok()
        .and_then(|root| root.get("Data"))
        .and_then(|data| data.as_map().ok())
        .ok_or_else(|| invalid_data("generated GameDataList has no Data map"))?;
    let Some(primary) = expected.first() else {
        return Ok(());
    };
    for expected in expected.iter().filter(|item| item.name != primary.name) {
        let present = ["Bool", "Enum", "Struct"].iter().any(|kind| {
            data.get(*kind)
                .and_then(|value| value.as_array().ok())
                .is_some_and(|entries| {
                    entries
                        .iter()
                        .any(|entry| entry_hash(entry) == Some(expected.hash))
                })
        });
        if !present {
            return Err(invalid_data(format!(
                "generated GameData hash is missing: {}",
                expected.name
            )));
        }
    }
    Ok(())
}

fn record_hash(output: &mut Vec<GameDataHash>, name: &str) -> u32 {
    let hash = murmur3_hash(name);
    output.push(GameDataHash {
        name: name.into(),
        hash,
    });
    hash
}

/// MurmurHash3 x86-32 with seed zero, equivalent to `mmh3.hash(name, signed=False)`.
pub fn murmur3_hash(name: &str) -> u32 {
    let bytes = name.as_bytes();
    let mut hash = 0u32;
    for block in bytes.chunks_exact(4) {
        let Ok(block): Result<&[u8; 4], _> = block.try_into() else {
            continue;
        };
        let mut value = u32::from_le_bytes(*block);
        value = value
            .wrapping_mul(0xcc9e_2d51)
            .rotate_left(15)
            .wrapping_mul(0x1b87_3593);
        hash ^= value;
        hash = hash
            .rotate_left(13)
            .wrapping_mul(5)
            .wrapping_add(0xe654_6b64);
    }
    let tail = bytes.chunks_exact(4).remainder();
    let mut value = 0u32;
    if let Some(byte) = tail.get(2) {
        value ^= u32::from(*byte) << 16;
    }
    if let Some(byte) = tail.get(1) {
        value ^= u32::from(*byte) << 8;
    }
    if let Some(first) = tail.first() {
        value ^= u32::from(*first);
        hash ^= value
            .wrapping_mul(0xcc9e_2d51)
            .rotate_left(15)
            .wrapping_mul(0x1b87_3593);
    }
    hash ^= bytes.len() as u32;
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85eb_ca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2_ae35);
    hash ^ (hash >> 16)
}

fn validate_actor_name(actor: &str) -> io::Result<()> {
    if actor.is_empty()
        || !actor
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || value == '_')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid weapon actor name",
        ));
    }
    Ok(())
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};

    #[test]
    fn murmur_hashes_match_totktools_and_real_weapon_data() {
        assert_eq!(murmur3_hash("Weapon_Lsword_174"), 0x64A9_17F2);
        assert_eq!(
            murmur3_hash("PictureBookData.Weapon_Lsword_174"),
            0xF623_7548
        );
        assert_eq!(murmur3_hash("IsGet.Weapon_Lsword_174"), 0x7289_7092);
        assert_eq!(murmur3_hash("Unopened"), 0x8D96_A2C5);
    }

    #[test]
    fn parses_optional_weapon_flag_groups() {
        let request = WeaponGameDataRequest::from_json(r#"{"name":"Weapon_Lsword_900"}"#).unwrap();
        assert!(request.picture_book);
        assert!(request.inventory_flags);
    }

    #[test]
    fn picture_book_enum_hashes_are_u64() {
        let state = picture_book_state(murmur3_hash("PictureBookData.Weapon_Lsword_005.State"));
        let values = state
            .as_map()
            .unwrap()
            .get("Values")
            .unwrap()
            .as_array()
            .unwrap();
        assert!(values.iter().all(|value| matches!(value, Byml::U64(_))));
    }

    #[test]
    #[ignore = "requires a configured clean ROMFS GameDataList"]
    fn generates_and_reopens_real_versioned_gamedatalist() {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        if super::super::version::discover_product_file(
            &clean_romfs.join("GameData"),
            PRODUCT_PREFIX,
            PRODUCT_SUFFIX,
        )
        .is_err()
        {
            return;
        }
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/gamedata_generated_romfs");
        let request = WeaponGameDataRequest {
            actor_name: "Weapon_Lsword_900".into(),
            picture_book: true,
            inventory_flags: true,
        };
        let report = request
            .generate(clean_romfs, &output, zstd.clone())
            .unwrap();
        assert_eq!(report.product_version, "110");
        let compressed = fs::read(&report.output).unwrap();
        let (raw, _) = zstd
            .try_decompress_for_path(&report.output, &compressed)
            .unwrap();
        let parsed = Byml::from_binary(&raw).unwrap();
        verify_hashes(&parsed, &report.hashes).unwrap();
        fs::remove_dir_all(output).unwrap();
    }
}
