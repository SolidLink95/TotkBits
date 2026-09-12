use super::{
    BinTextFile::{BymlFile, OpenedFile},
    Wrapper::ExeWrapper,
};
use crate::{
    Open_and_Save::SendData,
    Settings::Pathlib,
    Zstd::{is_gamedatalist, TotkFileType, TotkZstd},
};
use roead::byml::{Byml, Map};
use std::{io, path::Path, sync::Arc};

pub struct GameDataList;

/// Save-file header reserved ahead of the flag data in every save slot.
const SAVE_HEADER_SIZE: i32 = 0x130;
/// Number of save-data slots described by `MetaData/SaveDirectory`.
const SAVE_FILE_SLOTS: usize = 7;

/// Every flag table that contributes to the save-data layout.
const SAVE_TABLES: [&str; 33] = [
    "Binary",
    "BinaryArray",
    "Bool",
    "Bool64bitKey",
    "BoolArray",
    "Enum",
    "EnumArray",
    "Float",
    "FloatArray",
    "Int",
    "Int64",
    "Int64Array",
    "IntArray",
    "String16",
    "String16Array",
    "String32",
    "String32Array",
    "String64",
    "String64Array",
    "UInt",
    "UInt64",
    "UInt64Array",
    "UIntArray",
    "Vector2",
    "Vector2Array",
    "Vector3",
    "Vector3Array",
    "WString16",
    "WString16Array",
    "WString32",
    "WString32Array",
    "WString64",
    "WString64Array",
];

/// Running totals for [`recalculate_save_metadata`].
#[derive(Default)]
struct SaveLayout {
    offsets: [i32; SAVE_FILE_SLOTS],
    sizes: [i32; SAVE_FILE_SLOTS],
    key_size_added: [bool; SAVE_FILE_SLOTS],
    all_offset: i32,
    all_size: i32,
}

/// Recomputes `MetaData/AllDataSaveOffset`, `AllDataSaveSize`,
/// `SaveDataOffsetPos` and `SaveDataSize` from the flag tables, so a
/// GameDataList with added or removed flags describes a save layout the game
/// can allocate. Mirrors the save-data writer TKMM applies after merging.
pub fn recalculate_save_metadata(root: &mut Byml) -> io::Result<()> {
    let root = root
        .as_mut_map()
        .map_err(|_| invalid_data("GameDataList root is not a map"))?;
    let tables = root
        .get("Data")
        .ok_or_else(|| invalid_data("GameDataList has no Data map"))?
        .as_map()
        .map_err(|_| invalid_data("GameDataList Data is not a map"))?;
    let meta = root
        .get("MetaData")
        .ok_or_else(|| invalid_data("GameDataList has no MetaData map"))?
        .as_map()
        .map_err(|_| invalid_data("GameDataList MetaData is not a map"))?;
    let directories: Vec<String> = meta
        .get("SaveDirectory")
        .ok_or_else(|| invalid_data("MetaData has no SaveDirectory"))?
        .as_array()
        .map_err(|_| invalid_data("SaveDirectory is not an array"))?
        .iter()
        .map(|entry| entry.as_string().map(|s| s.to_string()))
        .collect::<Result<_, _>>()
        .map_err(|_| invalid_data("SaveDirectory contains a non-string"))?;

    let mut layout = SaveLayout {
        all_offset: SAVE_HEADER_SIZE,
        all_size: SAVE_HEADER_SIZE,
        ..SaveLayout::default()
    };
    for table_name in SAVE_TABLES {
        let Some(table) = tables.get(table_name).and_then(|t| t.as_array().ok()) else {
            continue;
        };
        for entry in table {
            let entry = entry
                .as_map()
                .map_err(|_| invalid_data(format!("{table_name} entry is not a map")))?;
            layout.add_entry(table_name, entry, &directories)?;
        }
    }
    for slot in 0..SAVE_FILE_SLOTS {
        if layout.offsets[slot] > 0 {
            layout.offsets[slot] += SAVE_HEADER_SIZE;
        }
        if layout.sizes[slot] > 0 {
            layout.sizes[slot] += SAVE_HEADER_SIZE;
        }
    }

    let meta = root
        .get_mut("MetaData")
        .and_then(|meta| meta.as_mut_map().ok())
        .ok_or_else(|| invalid_data("GameDataList MetaData is not a map"))?;
    meta.insert("AllDataSaveOffset".into(), Byml::I32(layout.all_offset));
    meta.insert("AllDataSaveSize".into(), Byml::I32(layout.all_size));
    meta.insert(
        "SaveDataOffsetPos".into(),
        Byml::Array(layout.offsets.iter().map(|v| Byml::I32(*v)).collect()),
    );
    meta.insert(
        "SaveDataSize".into(),
        Byml::Array(layout.sizes.iter().map(|v| Byml::I32(*v)).collect()),
    );
    Ok(())
}

impl SaveLayout {
    fn add_entry(
        &mut self,
        table_name: &str,
        entry: &Map,
        directories: &[String],
    ) -> io::Result<()> {
        let Some(save_file_index) = entry
            .get("SaveFileIndex")
            .and_then(|value| value.as_i32().ok())
        else {
            return Ok(());
        };
        let is_bool64_key = table_name == "Bool64bitKey";
        let slot = usize::try_from(save_file_index)
            .ok()
            .filter(|slot| *slot < SAVE_FILE_SLOTS && !directories[*slot].is_empty());

        // Flags without a save file still occupy space in the all-data block.
        let Some(slot) = slot else {
            if !is_bool64_key {
                self.all_offset += 8;
                self.all_size += entry_size(table_name, entry)?;
            }
            return Ok(());
        };

        if !is_bool64_key {
            self.offsets[slot] += 8;
            self.all_offset += 8;
        } else if !self.key_size_added[slot] {
            // 64-bit keyed bools share one 8-byte header per save file.
            self.sizes[slot] += 8;
            self.all_size += 8;
            self.key_size_added[slot] = true;
        }
        let size = entry_size(table_name, entry)?;
        self.sizes[slot] += size;
        self.all_size += size;
        Ok(())
    }
}

/// Bytes one flag entry occupies in the save data.
fn entry_size(table_name: &str, entry: &Map) -> io::Result<i32> {
    let (element_type, count, base) = match table_name.strip_suffix("Array") {
        Some(element_type) => (element_type, array_count(table_name, entry)?, 0xC),
        None => (table_name, 1, 0x8),
    };
    Ok(match table_name {
        "BoolArray" => {
            // Bit-packed, at least four bytes, padded to a multiple of four.
            let bytes = (count + 7) / 8;
            let bytes = if bytes > 3 { bytes } else { 4 };
            base + (bytes + 3) / 4 * 4
        }
        "IntArray" | "FloatArray" | "UIntArray" | "EnumArray" => base + count * 4,
        "Binary" | "BinaryArray" => match entry.get("DefaultValue") {
            Some(Byml::U32(default_size)) => base + count * 4 + count * (*default_size as i32),
            _ => base + count * 4,
        },
        _ => {
            base + count
                * match element_type {
                    "UInt64" | "Int64" | "Vector2" => 8,
                    "Vector3" => 12,
                    "String16" => 16,
                    "String32" | "WString16" => 32,
                    "String64" | "WString32" => 64,
                    "WString64" => 128,
                    _ => 0,
                }
        }
    })
}

fn array_count(table_name: &str, entry: &Map) -> io::Result<i32> {
    for key in ["ArraySize", "Size"] {
        if let Some(value) = entry.get(key) {
            return value
                .as_int::<i32>()
                .map_err(|_| invalid_data(format!("{table_name} {key} is not an integer")));
        }
    }
    if let Some(Byml::Array(default)) = entry.get("DefaultValue") {
        return Ok(default.len() as i32);
    }
    Err(invalid_data(format!(
        "the length of an array entry in '{table_name}' could not be determined"
    )))
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

impl GameDataList {
    fn converter() -> ExeWrapper {
        ExeWrapper::new("bin/cpp/oead_byml_pipe.exe".to_string(), Vec::new())
    }

    pub fn binary_to_text(data: &[u8], zstd: Arc<TotkZstd<'_>>) -> io::Result<String> {
        let file_data = BymlFile::byml_data_to_bytes(&data.to_vec(), zstd)?;
        Self::converter().binary_to_string(&file_data.data, "byml_binary_to_text".to_string())
    }

    pub fn text_to_binary(text: &str) -> io::Result<Vec<u8>> {
        Self::converter().string_to_binary(text, "byml_text_to_binary".to_string())
    }

    pub fn open<'a, P: AsRef<Path>>(
        path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(OpenedFile<'a>, SendData)> {
        let path = path.as_ref();
        if !is_gamedatalist(path) {
            return None;
        }

        let raw_data = std::fs::read(path).ok()?;
        let file_data = BymlFile::byml_data_to_bytes(&raw_data, zstd.clone()).ok()?;
        let text = Self::converter()
            .binary_to_string(&file_data.data, "byml_binary_to_text".to_string())
            .ok()?;
        let endian = BymlFile::get_endiannes(&file_data.data);
        let pathlib = Pathlib::new(path);

        let mut opened_file = OpenedFile::default();
        opened_file.path = pathlib.clone();
        opened_file.endian = endian;
        opened_file.file_type = TotkFileType::Byml;

        let mut send_data = SendData::default();
        send_data.status_text = format!("Opened {}", pathlib.full_path);
        send_data.path = pathlib;
        send_data.text = text;
        send_data.get_file_label(TotkFileType::Byml, endian);
        Some((opened_file, send_data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};

    /// The vanilla table's stored layout must come back unchanged from the
    /// flags it already contains.
    #[test]
    #[ignore = "requires the TOTK RomFS dump"]
    fn recalculated_metadata_matches_vanilla_gamedatalist() {
        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let mut config = TotkConfig::default();
        config.romfs = romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).unwrap());
        let path = romfs.join("GameData/GameDataList.Product.110.byml.zs");
        let file = BymlFile::new(&path, zstd).expect("vanilla GameDataList opens");
        let before = file.pio.clone();
        let mut after = before.clone();
        recalculate_save_metadata(&mut after).unwrap();
        let meta = |root: &Byml| root.as_map().unwrap().get("MetaData").unwrap().clone();
        let (before, after) = (meta(&before), meta(&after));
        for key in [
            "AllDataSaveOffset",
            "AllDataSaveSize",
            "SaveDataOffsetPos",
            "SaveDataSize",
        ] {
            assert_eq!(
                before.as_map().unwrap().get(key),
                after.as_map().unwrap().get(key),
                "{key}"
            );
        }
    }
}
