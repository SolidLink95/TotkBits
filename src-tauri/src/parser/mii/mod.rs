use std::collections::{BTreeMap, BTreeSet};

use super::binary::{BinaryReader, BinaryWriter, Endian};

pub type MiiResult<T> = Result<T, String>;

const MAGIC: &[u8; 4] = b"RNOD";
pub const SLOT_COUNT: usize = 100;
pub const SLOT_SIZE: usize = 74;
const SLOT_OFFSET: usize = 4;
const CHECKSUM_OFFSET: usize = 127_454;
const MIN_SIZE: usize = CHECKSUM_OFFSET + 2;

pub struct RflDbFile {
    entries: BTreeMap<String, Vec<u8>>,
    original: Vec<u8>,
}

impl RflDbFile {
    pub fn parse(data: &[u8]) -> MiiResult<Self> {
        let mut reader = BinaryReader::with_endian(data, Endian::Big);
        if reader.len() < MIN_SIZE
            || reader
                .read_bytes(MAGIC.len())
                .map_err(|error| error.to_string())?
                != MAGIC
        {
            return Err("invalid RFL_DB.dat: expected RNOD database".into());
        }
        let mut entries = BTreeMap::new();
        for index in 0..SLOT_COUNT {
            let slot = reader
                .read_bytes(SLOT_SIZE)
                .map_err(|error| format!("invalid RFL_DB.dat slot {index}: {error}"))?;
            // A valid RCD may begin with 0x00 (for example MiiJS output for a
            // non-favourite male Mii). Empty RFL slots are entirely zeroed.
            if slot.iter().any(|byte| *byte != 0) {
                entries.insert(Self::entry_path(index, slot), slot.to_vec());
            }
        }
        Ok(Self {
            entries,
            original: data.to_vec(),
        })
    }

    pub fn build(&self) -> MiiResult<Vec<u8>> {
        if self.entries.len() > SLOT_COUNT {
            return Err(format!("RFL_DB.dat holds at most {SLOT_COUNT} Miis"));
        }
        let reader = BinaryReader::with_endian(&self.original, Endian::Big);
        if reader.len() < MIN_SIZE {
            return Err("RFL_DB.dat template is truncated".into());
        }
        let mut writer = BinaryWriter::from_vec(self.original.clone(), Endian::Big);
        writer.seek(SLOT_OFFSET);
        writer.write_bytes(&vec![0; SLOT_COUNT * SLOT_SIZE]);

        let mut occupied = BTreeSet::new();
        let mut pending = Vec::new();
        for (path, data) in &self.entries {
            if data.len() != SLOT_SIZE {
                return Err(format!(
                    "{path} is {} bytes; RFL_DB.dat requires 74-byte Wii Mii data",
                    data.len()
                ));
            }
            if let Some(index) = Self::index_from_path(path).filter(|index| occupied.insert(*index))
            {
                let start = SLOT_OFFSET + index * SLOT_SIZE;
                writer.seek(start);
                writer.write_bytes(data);
            } else {
                pending.push(data);
            }
        }
        let mut free = (0..SLOT_COUNT).filter(|index| !occupied.contains(index));
        for data in pending {
            let index = free.next().ok_or("RFL_DB.dat has no free Mii slots")?;
            let start = SLOT_OFFSET + index * SLOT_SIZE;
            writer.seek(start);
            writer.write_bytes(data);
        }
        let result = writer.into_inner();
        let checksum = {
            let checksum_region = BinaryReader::new(&result)
                .slice(0, CHECKSUM_OFFSET)
                .map_err(|error| error.to_string())?;
            checksum(checksum_region)
        };
        let mut writer = BinaryWriter::from_vec(result, Endian::Big);
        writer.write_u16_at(CHECKSUM_OFFSET, checksum);
        Ok(writer.into_inner())
    }

    pub fn normalize_mii(data: Vec<u8>) -> MiiResult<Vec<u8>> {
        match data.len() {
            SLOT_SIZE => Ok(data),
            76 => Ok(data[..SLOT_SIZE].to_vec()),
            size => Err(format!(
                "RFL_DB.dat accepts 74/76-byte Wii RFL Mii data; this Mii format is {size} bytes"
            )),
        }
    }

    pub fn entries(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.entries
    }

    pub fn entries_mut(&mut self) -> &mut BTreeMap<String, Vec<u8>> {
        &mut self.entries
    }

    fn entry_path(index: usize, data: &[u8]) -> String {
        format!("Miis/{index:03}_{}.miigx", mii_name(data))
    }

    fn index_from_path(path: &str) -> Option<usize> {
        let filename = path.rsplit('/').next()?;
        let index = filename.split('_').next()?.parse::<usize>().ok()?;
        (index < SLOT_COUNT).then_some(index)
    }
}

fn mii_name(data: &[u8]) -> String {
    let mut reader = BinaryReader::with_endian(data, Endian::Big);
    let _ = reader.skip(2);
    let units = (0..10)
        .map_while(|_| reader.read_u16().ok())
        .take_while(|unit| *unit != 0)
        .collect::<Vec<_>>();
    let name = String::from_utf16_lossy(&units).trim().to_string();
    let safe = name
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => character,
        })
        .collect::<String>();
    if safe.is_empty() {
        "Mii".into()
    } else {
        safe
    }
}

fn checksum(data: &[u8]) -> u16 {
    let mut value = 0_u16;
    for byte in data {
        for bit in (0..8).rev() {
            let top = value & 0x8000 != 0;
            value = (value << 1) | u16::from((byte >> bit) & 1);
            if top {
                value ^= 0x1021;
            }
        }
    }
    for _ in 0..16 {
        let top = value & 0x8000 != 0;
        value <<= 1;
        if top {
            value ^= 0x1021;
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_database_roundtrips_and_recomputes_checksum() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/mii/RFL_DB.dat");
        let source = std::fs::read(path).unwrap();
        let database = RflDbFile::parse(&source).unwrap();
        assert!(!database.entries().is_empty());
        assert!(database.entries().len() <= SLOT_COUNT);
        let rebuilt = database.build().unwrap();
        assert_eq!(rebuilt.len(), source.len());
        assert_eq!(
            rebuilt[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 2],
            checksum(&rebuilt[..CHECKSUM_OFFSET]).to_be_bytes()
        );
        assert_eq!(
            RflDbFile::parse(&rebuilt).unwrap().entries().len(),
            database.entries().len()
        );

        let mut edited = RflDbFile::parse(&source).unwrap();
        let first = edited.entries().values().next().unwrap().clone();
        edited.entries_mut().clear();
        edited.entries_mut().insert("Imported.mae".into(), first);
        let reopened = RflDbFile::parse(&edited.build().unwrap()).unwrap();
        assert_eq!(reopened.entries().len(), 1);
        assert!(reopened
            .entries()
            .keys()
            .next()
            .unwrap()
            .starts_with("Miis/000_"));
    }
}
