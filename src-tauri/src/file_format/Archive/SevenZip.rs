use super::{detect_archive_magic, validate_entry_path, ArchiveCodec, ArchiveMagic, ArchiveResult};
use sevenz_rust2::{ArchiveEntry, ArchiveReader, ArchiveWriter, Error as SevenZError, Password};
use std::{collections::BTreeMap, io::Cursor};

#[derive(Default)]
pub struct SevenZipFile {
    entries: BTreeMap<String, Vec<u8>>,
}

impl ArchiveCodec for SevenZipFile {
    fn from_bytes(data: &[u8]) -> ArchiveResult<Self> {
        if detect_archive_magic(data) != Some(ArchiveMagic::SevenZip) {
            return Err("7z magic bytes do not match".into());
        }
        let cursor = Cursor::new(data.to_vec());
        let mut reader = ArchiveReader::new(cursor, Password::empty())
            .map_err(|e| format!("invalid, encrypted, or unsupported 7z archive: {e}"))?;
        let mut entries = BTreeMap::new();
        reader
            .for_each_entries(|entry, source| {
                if entry.is_directory() {
                    return Ok(true);
                }
                let name = entry.name().replace('\\', "/");
                validate_entry_path(&name).map_err(|e| SevenZError::Other(e.into()))?;
                let mut bytes = Vec::new();
                source.read_to_end(&mut bytes)?;
                entries.insert(name, bytes);
                Ok(true)
            })
            .map_err(|e| {
                format!("unable to read 7z archive (encrypted archives are unsupported): {e}")
            })?;
        Ok(Self { entries })
    }
    fn to_bytes(&self) -> ArchiveResult<Vec<u8>> {
        let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).map_err(|e| e.to_string())?;
        for (name, bytes) in &self.entries {
            validate_entry_path(name)?;
            let entry = ArchiveEntry::new_file(name);
            writer
                .push_archive_entry(entry, Some(Cursor::new(bytes)))
                .map_err(|e| e.to_string())?;
        }
        Ok(writer.finish().map_err(|e| e.to_string())?.into_inner())
    }
    fn entries(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.entries
    }
    fn entries_mut(&mut self) -> &mut BTreeMap<String, Vec<u8>> {
        &mut self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seven_zip_roundtrip_and_edit() {
        let mut archive = SevenZipFile::default();
        archive
            .entries
            .insert("folder/a.txt".into(), b"old".to_vec());
        let bytes = archive.to_bytes().unwrap();
        let mut reopened = SevenZipFile::from_bytes(&bytes).unwrap();
        reopened.replace("folder/a.txt", b"new".to_vec()).unwrap();
        let final_archive = SevenZipFile::from_bytes(&reopened.to_bytes().unwrap()).unwrap();
        assert_eq!(final_archive.get("folder/a.txt"), Some(&b"new"[..]));
    }
}
