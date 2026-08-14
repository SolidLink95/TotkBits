use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path},
    sync::OnceLock,
};

use crate::Zstd::ZstdDictionary;

pub mod Bars;
pub mod Folder;
pub mod Rar;
pub mod SevenZip;
pub mod SevenZipCmd;
pub mod Zip;

pub type ArchiveResult<T> = Result<T, String>;

type BarsHashIndex = BTreeMap<String, BTreeMap<String, String>>;

fn bars_hash_index() -> Option<&'static BarsHashIndex> {
    static INDEX: OnceLock<Option<BarsHashIndex>> = OnceLock::new();
    INDEX
        .get_or_init(|| {
            serde_json::from_str(&crate::utils::LookupData::read_support_json(
                "bars_bwav_sha256.json",
            ))
            .ok()
        })
        .as_ref()
}

fn bars_baseline<'a>(
    archive_name: &str,
    index: &'a BarsHashIndex,
) -> Option<&'a BTreeMap<String, String>> {
    let lower = archive_name.to_ascii_lowercase();
    index
        .iter()
        .find(|(name, _)| name.to_ascii_lowercase() == lower)
        .or_else(|| {
            let alternate = if lower.ends_with(".bars.zs") {
                archive_name.strip_suffix(".zs")?.to_string()
            } else if lower.ends_with(".bars") {
                format!("{archive_name}.zs")
            } else {
                return None;
            };
            index
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(&alternate))
        })
        .map(|(_, entries)| entries)
}

fn classify_bars_entries(
    archive_name: &str,
    entries: &BTreeMap<String, Vec<u8>>,
    index: &BarsHashIndex,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let Some(baseline) = bars_baseline(archive_name, index) else {
        return (BTreeSet::new(), BTreeSet::new());
    };
    let mut added = BTreeSet::new();
    let mut modified = BTreeSet::new();
    let mut audio_count = 0;
    for (path, bytes) in entries {
        if !path.to_ascii_lowercase().ends_with(".bwav") {
            continue;
        }
        audio_count += 1;
        let Some(filename) = Path::new(path).file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        match baseline
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(filename))
        {
            None => {
                added.insert(path.clone());
            }
            Some((_, expected))
                if !crate::Zstd::sha256(bytes.clone()).eq_ignore_ascii_case(expected) =>
            {
                modified.insert(path.clone());
            }
            Some(_) => {}
        }
    }
    if audio_count > 0 && added.len() == audio_count {
        added.clear();
    }
    (added, modified)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveMagic {
    Zip,
    SevenZip,
    Rar,
    Bars,
}

pub fn detect_archive_magic(data: &[u8]) -> Option<ArchiveMagic> {
    if data.is_empty() {
        return None;
    }
    if crate::Settings::Magic::is_zip(data) {
        Some(ArchiveMagic::Zip)
    } else if crate::Settings::Magic::is_seven_zip(data) {
        Some(ArchiveMagic::SevenZip)
    } else if crate::Settings::Magic::is_rar(data) {
        Some(ArchiveMagic::Rar)
    } else if crate::Settings::Magic::is_bars(data) {
        Some(ArchiveMagic::Bars)
    } else {
        None
    }
}

pub enum RootArchive {
    Zip(Zip::ZipFile),
    SevenZip(SevenZipCmd::SevenZipCmd),
    Rar(Rar::RarFile),
    Folder(Folder::FolderFile),
    Bars(Bars::BarsFile),
}

pub struct ArchiveDocument {
    pub archive: RootArchive,
    pub path: String,
    pub added: BTreeSet<String>,
    pub modified: BTreeSet<String>,
    pub dictionary: Option<ZstdDictionary>,
}

impl ArchiveDocument {
    pub fn get_metadata(&self) -> String {
        let mut res = format!("[{}]", &self.file_type());
        if let Some(_dict) = self.dictionary {
            if _dict == ZstdDictionary::Bcett
                || _dict == ZstdDictionary::Empty
                || _dict == ZstdDictionary::Pack
                || _dict == ZstdDictionary::Zs
            {
                res += &format!(" [ZSTD: {:?}]", _dict);
            } else if _dict != ZstdDictionary::None {
                res += &format!(" [{:?}]", _dict)
            }
        }

        res
    }

    pub fn kind(&self) -> &'static str {
        self.file_type()
    }

    pub fn file_type(&self) -> &'static str {
        match &self.archive {
            RootArchive::Bars(_) => "BARS",
            RootArchive::Zip(_) => "ZIP",
            RootArchive::SevenZip(_) => "7Z",
            RootArchive::Rar(_) => "RAR",
            RootArchive::Folder(_) => "FOLDER",
            _ => "ARCHIVE",
        }
    }

    pub fn open_folder(path: &Path) -> ArchiveResult<Self> {
        Ok(Self {
            archive: RootArchive::Folder(Folder::FolderFile::from_directory(path)?),
            path: path.to_string_lossy().replace('\\', "/"),
            added: BTreeSet::new(),
            modified: BTreeSet::new(),
            dictionary: None,
        })
    }
    pub fn open(path: &Path) -> ArchiveResult<Option<Self>> {
        Self::open_impl(path, None)
    }
    pub fn open_with_zstd(
        path: &Path,
        zstd: &crate::Zstd::TotkZstd<'_>,
    ) -> ArchiveResult<Option<Self>> {
        Self::open_impl(path, Some(zstd))
    }

    pub fn from_path(
        path: impl AsRef<Path>,
        _dict: Option<ZstdDictionary>,
    ) -> ArchiveResult<Option<Self>> {
        let bytes = fs::read(path.as_ref()).unwrap_or_default();
        Self::from_binary(&bytes, path.as_ref(), _dict)
    }

    pub fn from_binary(
        bytes: &[u8],
        path: impl AsRef<Path>,
        _dict: Option<ZstdDictionary>,
    ) -> ArchiveResult<Option<Self>> {
        //Asume already decompressed
        let Some(_) = detect_archive_magic(&bytes) else {
            return Err("Not an archive".to_string());
        };
        let archive = match detect_archive_magic(&bytes) {
            Some(ArchiveMagic::Zip) => RootArchive::Zip(Zip::ZipFile::from_bytes(&bytes)?),
            Some(ArchiveMagic::SevenZip) => {
                RootArchive::SevenZip(SevenZipCmd::SevenZipCmd::from_bytes(&bytes)?)
            }
            Some(ArchiveMagic::Rar) => RootArchive::Rar(Rar::RarFile::from_bytes(&bytes)?),
            Some(ArchiveMagic::Bars) => RootArchive::Bars(Bars::BarsFile::from_bytes(&bytes)?),
            None => return Ok(None),
        };
        let mut document = Self {
            archive,
            path: path.as_ref().to_string_lossy().to_string(),
            added: BTreeSet::new(),
            modified: BTreeSet::new(),
            dictionary: _dict,
        };
        document.refresh_bars_changes();
        Ok(Some(document))
    }

    fn open_impl(
        path: &Path,
        zstd: Option<&crate::Zstd::TotkZstd<'_>>,
    ) -> ArchiveResult<Option<Self>> {
        let source =
            fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let decompressed;
        let mut dictionary = None;
        let bytes = if crate::Settings::Magic::is_zstd(&source) {
            let Some(decoder) = zstd else {
                return Ok(None);
            };
            let Ok((data, detected_dictionary)) = decoder.try_decompress_with_dictionary(&source)
            else {
                return Ok(None);
            };
            decompressed = data;
            if detect_archive_magic(&decompressed).is_none() {
                return Ok(None);
            }
            dictionary = Some(detected_dictionary);
            decompressed.as_slice()
        } else {
            source.as_slice()
        };
        let archive = match detect_archive_magic(&bytes) {
            Some(ArchiveMagic::Zip) => RootArchive::Zip(Zip::ZipFile::from_bytes(&bytes)?),
            Some(ArchiveMagic::SevenZip) => {
                RootArchive::SevenZip(SevenZipCmd::SevenZipCmd::from_bytes(&bytes)?)
            }
            Some(ArchiveMagic::Rar) => RootArchive::Rar(Rar::RarFile::from_bytes(&bytes)?),
            Some(ArchiveMagic::Bars) => RootArchive::Bars(Bars::BarsFile::from_bytes(&bytes)?),
            None => return Ok(None),
        };
        let mut document = Self {
            archive,
            path: path.to_string_lossy().replace('\\', "/"),
            added: BTreeSet::new(),
            modified: BTreeSet::new(),
            dictionary,
        };
        document.refresh_bars_changes();
        Ok(Some(document))
    }
    fn refresh_bars_changes(&mut self) {
        let RootArchive::Bars(bars) = &self.archive else {
            return;
        };
        let Some(index) = bars_hash_index() else {
            return;
        };
        let Some(name) = Path::new(&self.path)
            .file_name()
            .and_then(|name| name.to_str())
        else {
            return;
        };
        (self.added, self.modified) = classify_bars_entries(name, bars.entries(), index);
    }
    pub fn paths(&self) -> Vec<String> {
        self.archive.entries().keys().cloned().collect()
    }
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.archive.get(path)
    }
    pub fn set(&mut self, path: &str, bytes: Vec<u8>) -> ArchiveResult<()> {
        validate_entry_path(path)?;
        if self.archive.entries().contains_key(path) {
            self.modified.insert(path.into());
        } else {
            self.added.insert(path.into());
        }
        self.archive.entries_mut().insert(path.into(), bytes);
        Ok(())
    }
    pub fn remove_prefix(&mut self, path: &str) -> ArchiveResult<usize> {
        let prefix = format!("{}/", path.trim_end_matches('/'));
        let keys: Vec<_> = self
            .paths()
            .into_iter()
            .filter(|p| p == path || p.starts_with(&prefix))
            .collect();
        if keys.is_empty() {
            return Err(format!("archive entry not found: {path}"));
        }
        for key in &keys {
            self.archive.entries_mut().remove(key);
            self.added.remove(key);
            self.modified.remove(key);
        }
        Ok(keys.len())
    }
    pub fn rename_prefix(&mut self, from: &str, to: &str) -> ArchiveResult<usize> {
        validate_entry_path(to)?;
        let prefix = format!("{}/", from.trim_end_matches('/'));
        let keys: Vec<_> = self
            .paths()
            .into_iter()
            .filter(|p| p == from || p.starts_with(&prefix))
            .collect();
        if keys.is_empty() {
            return Err(format!("archive entry not found: {from}"));
        }
        let replacements: Vec<_> = keys
            .iter()
            .map(|old| {
                (
                    old.clone(),
                    if old == from {
                        to.into()
                    } else {
                        format!("{}{}", to.trim_end_matches('/'), &old[from.len()..])
                    },
                )
            })
            .collect();
        for (_, new) in &replacements {
            validate_entry_path(new)?;
            if self.archive.entries().contains_key(new) && !keys.contains(new) {
                return Err(format!("archive entry already exists: {new}"));
            }
        }
        for (old, new) in replacements {
            let bytes = self
                .archive
                .entries_mut()
                .remove(&old)
                .ok_or_else(|| format!("archive entry disappeared during rename: {old}"))?;
            self.archive.entries_mut().insert(new.clone(), bytes);
            self.added.remove(&old);
            self.modified.remove(&old);
            self.added.insert(new);
        }
        Ok(keys.len())
    }
    pub fn to_bytes(&self) -> ArchiveResult<Vec<u8>> {
        self.archive.to_bytes()
    }
    pub fn save_atomic(&mut self, destination: &Path) -> ArchiveResult<()> {
        self.save_atomic_impl(destination, None)
    }
    pub fn save_atomic_with_zstd(
        &mut self,
        destination: &Path,
        zstd: &crate::Zstd::TotkZstd<'_>,
    ) -> ArchiveResult<()> {
        self.save_atomic_impl(destination, Some(zstd))
    }
    fn save_atomic_impl(
        &mut self,
        destination: &Path,
        zstd: Option<&crate::Zstd::TotkZstd<'_>>,
    ) -> ArchiveResult<()> {
        if let RootArchive::Folder(folder) = &self.archive {
            folder.save_to_directory(destination)?;
            self.path = destination.to_string_lossy().replace('\\', "/");
            self.added.clear();
            self.modified.clear();
            return Ok(());
        }
        let raw = self.to_bytes()?;
        let bytes = if let Some(dictionary) = self.dictionary {
            zstd.ok_or("zs compressor is unavailable")?
                .compress_with_dictionary(&raw, dictionary)
                .map_err(|error| {
                    format!("failed to compress archive with {dictionary:?}: {error}")
                })?
        } else {
            raw
        };
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let tmp = parent.join(format!(
            ".{}.totkbits.tmp",
            destination
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("archive")
        ));
        if bytes.is_empty() {
            return Err("refusing to save an empty archive".into());
        }
        fs::write(&tmp, bytes).map_err(|e| format!("failed to write temporary archive: {e}"))?;
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows::{
                core::PCWSTR,
                Win32::Storage::FileSystem::{
                    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
                },
            };
            let source: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
            let target: Vec<u16> = destination
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            unsafe {
                MoveFileExW(
                    PCWSTR(source.as_ptr()),
                    PCWSTR(target.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
            .map_err(|e| format!("failed to atomically commit archive: {e}"))?;
        }
        #[cfg(not(windows))]
        fs::rename(&tmp, destination).map_err(|e| format!("failed to commit archive: {e}"))?;
        self.path = destination.to_string_lossy().replace('\\', "/");
        self.added.clear();
        self.modified.clear();
        self.refresh_bars_changes();
        Ok(())
    }
}

impl RootArchive {
    pub(crate) fn entries(&self) -> &BTreeMap<String, Vec<u8>> {
        match self {
            Self::Zip(v) => v.entries(),
            Self::SevenZip(v) => v.entries(),
            Self::Rar(v) => v.entries(),
            Self::Folder(v) => v.entries(),
            Self::Bars(v) => v.entries(),
        }
    }
    pub(crate) fn entries_mut(&mut self) -> &mut BTreeMap<String, Vec<u8>> {
        match self {
            Self::Zip(v) => v.entries_mut(),
            Self::SevenZip(v) => v.entries_mut(),
            Self::Rar(v) => v.entries_mut(),
            Self::Folder(v) => v.entries_mut(),
            Self::Bars(v) => v.entries_mut(),
        }
    }
    pub(crate) fn get(&self, path: &str) -> Option<&[u8]> {
        self.entries().get(path).map(Vec::as_slice)
    }
    pub(crate) fn to_bytes(&self) -> ArchiveResult<Vec<u8>> {
        match self {
            Self::Zip(v) => v.to_bytes(),
            Self::SevenZip(v) => v.to_bytes(),
            Self::Rar(v) => v.to_bytes(),
            Self::Folder(v) => v.to_bytes(),
            Self::Bars(v) => v.to_bytes(),
        }
    }
}

pub trait ArchiveCodec: Sized {
    fn from_bytes(data: &[u8]) -> ArchiveResult<Self>;
    fn to_bytes(&self) -> ArchiveResult<Vec<u8>>;
    fn entries(&self) -> &BTreeMap<String, Vec<u8>>;
    fn entries_mut(&mut self) -> &mut BTreeMap<String, Vec<u8>>;

    fn paths(&self) -> Vec<String> {
        self.entries().keys().cloned().collect()
    }
    fn get(&self, path: &str) -> Option<&[u8]> {
        self.entries().get(path).map(Vec::as_slice)
    }
    fn replace(&mut self, path: &str, data: Vec<u8>) -> ArchiveResult<()> {
        validate_entry_path(path)?;
        if !self.entries().contains_key(path) {
            return Err(format!("archive entry not found: {path}"));
        }
        self.entries_mut().insert(path.to_string(), data);
        Ok(())
    }
    fn remove(&mut self, path: &str) -> ArchiveResult<()> {
        self.entries_mut()
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| format!("archive entry not found: {path}"))
    }
    fn rename(&mut self, from: &str, to: &str) -> ArchiveResult<()> {
        validate_entry_path(to)?;
        if self.entries().contains_key(to) {
            return Err(format!("archive entry already exists: {to}"));
        }
        let data = self
            .entries_mut()
            .remove(from)
            .ok_or_else(|| format!("archive entry not found: {from}"))?;
        self.entries_mut().insert(to.to_string(), data);
        Ok(())
    }
}

pub fn validate_entry_path(path: &str) -> ArchiveResult<()> {
    if path.is_empty() || path.contains('\\') {
        return Err(format!("unsafe archive entry path: {path}"));
    }
    let candidate = Path::new(path);
    if candidate.is_absolute()
        || candidate.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("unsafe archive entry path: {path}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_detection_uses_magic_only() {
        assert_eq!(
            detect_archive_magic(b"PK\x03\x04payload"),
            Some(ArchiveMagic::Zip)
        );
        assert_eq!(
            detect_archive_magic(b"7z\xBC\xAF\x27\x1Cpayload"),
            Some(ArchiveMagic::SevenZip)
        );
        assert_eq!(
            detect_archive_magic(b"Rar!\x1A\x07\x01\x00payload"),
            Some(ArchiveMagic::Rar)
        );
        assert_eq!(detect_archive_magic(b"example.zip"), None);
    }

    #[test]
    fn rejects_zip_slip_paths() {
        assert!(validate_entry_path("../evil").is_err());
        assert!(validate_entry_path("/evil").is_err());
        assert!(validate_entry_path("safe/file.txt").is_ok());
    }

    #[test]
    fn classifies_bars_changes_from_filename_hashes() {
        let original = b"original audio".to_vec();
        let mut baseline_entries = BTreeMap::new();
        baseline_entries.insert("same.bwav".into(), crate::Zstd::sha256(original.clone()));
        baseline_entries.insert("changed.bwav".into(), crate::Zstd::sha256(original.clone()));
        let mut index = BTreeMap::new();
        index.insert("Voice.bars.zs".into(), baseline_entries);

        let mut entries = BTreeMap::new();
        entries.insert("Audio/same.bwav".into(), original);
        entries.insert("Audio/changed.bwav".into(), b"replacement".to_vec());
        entries.insert("Audio/added.bwav".into(), b"new".to_vec());
        entries.insert("Meta Data/same.amta".into(), b"metadata".to_vec());

        let (added, modified) = classify_bars_entries("Voice.bars", &entries, &index);
        assert_eq!(added, BTreeSet::from(["Audio/added.bwav".into()]));
        assert_eq!(modified, BTreeSet::from(["Audio/changed.bwav".into()]));
    }

    #[test]
    fn does_not_highlight_when_all_bars_audio_is_added() {
        let mut index = BTreeMap::new();
        index.insert(
            "Voice.bars".into(),
            BTreeMap::from([(
                "vanilla.bwav".into(),
                crate::Zstd::sha256(b"vanilla".to_vec()),
            )]),
        );
        let entries = BTreeMap::from([
            ("Audio/new-a.bwav".into(), b"a".to_vec()),
            ("Audio/new-b.bwav".into(), b"b".to_vec()),
        ]);

        let (added, modified) = classify_bars_entries("Voice.bars.zs", &entries, &index);
        assert!(added.is_empty());
        assert!(modified.is_empty());

        let (added, modified) = classify_bars_entries("Unknown.bars", &entries, &index);
        assert!(added.is_empty());
        assert!(modified.is_empty());
    }

    #[test]
    fn renamed_archive_entries_are_added_not_modified() {
        let mut archive = Zip::ZipFile::default();
        archive
            .entries_mut()
            .insert("old.bwav".into(), b"audio".to_vec());
        let mut document = ArchiveDocument {
            archive: RootArchive::Zip(archive),
            path: "test.zip".into(),
            added: BTreeSet::new(),
            modified: BTreeSet::from(["old.bwav".into()]),
            dictionary: None,
        };

        document.rename_prefix("old.bwav", "new.bwav").unwrap();

        assert_eq!(document.added, BTreeSet::from(["new.bwav".into()]));
        assert!(document.modified.is_empty());
    }

    #[test]
    fn opens_zstd_compressed_bars() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bars/Assassin_Senior.bars");
        let raw = fs::read(source).expect("failed to read BARS fixture");
        let app = crate::TotkApp::TotkBitsApp::default();
        let compressed = app
            .zstd
            .compress_with_dictionary(&raw, crate::Zstd::ZstdDictionary::Zs)
            .expect("failed to compress BARS fixture with zs dictionary");
        let path =
            std::env::temp_dir().join(format!("totkbits-zstd-bars-{}.bars.zs", std::process::id()));
        fs::write(&path, compressed).expect("failed to write compressed BARS fixture");
        let mut document = ArchiveDocument::open_with_zstd(&path, &app.zstd)
            .expect("compressed BARS open failed")
            .expect("compressed BARS was not recognized");
        assert!(matches!(document.archive, RootArchive::Bars(_)));
        assert!(!document.paths().is_empty());
        assert_eq!(document.dictionary, Some(ZstdDictionary::Zs));
        document
            .save_atomic_with_zstd(&path, &app.zstd)
            .expect("failed to save zs-compressed BARS");
        let saved = fs::read(&path).expect("failed to read saved BARS");
        assert!(crate::Settings::Magic::is_zstd(&saved));
        let decoded = app
            .zstd
            .try_decompress_using(&saved, crate::Zstd::ZstdDictionary::Zs)
            .expect("saved BARS is not valid zs compression");
        assert!(crate::Settings::Magic::is_bars(&decoded));
        let _ = fs::remove_file(path);
    }

    fn document_roundtrip(extension: &str, archive: RootArchive) {
        let path = std::env::temp_dir().join(format!(
            "totkbits-archive-document-{}-{}.{}",
            std::process::id(),
            extension,
            extension
        ));
        let mut document = ArchiveDocument {
            archive,
            path: path.to_string_lossy().into_owned(),
            added: BTreeSet::new(),
            modified: BTreeSet::new(),
            dictionary: None,
        };
        document
            .set("folder/original.txt", b"before".to_vec())
            .unwrap();
        document.save_atomic(&path).unwrap();
        let mut reopened = ArchiveDocument::open(&path).unwrap().unwrap();
        assert_eq!(
            reopened.get("folder/original.txt"),
            Some(b"before".as_slice())
        );
        reopened
            .set("folder/original.txt", b"after".to_vec())
            .unwrap();
        reopened.save_atomic(&path).unwrap();
        let final_document = ArchiveDocument::open(&path).unwrap().unwrap();
        assert_eq!(
            final_document.get("folder/original.txt"),
            Some(b"after".as_slice())
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn zip_document_open_edit_save() {
        document_roundtrip("zip", RootArchive::Zip(Zip::ZipFile::default()));
    }

    #[test]
    fn seven_zip_document_open_edit_save() {
        document_roundtrip(
            "7z",
            RootArchive::SevenZip(SevenZipCmd::SevenZipCmd::default()),
        );
    }

    #[test]
    fn mixed_zip_to_seven_zip_roundtrip() {
        let mut inner = SevenZip::SevenZipFile::default();
        inner
            .entries_mut()
            .insert("deep/value.txt".into(), b"before".to_vec());
        let mut outer = Zip::ZipFile::default();
        outer
            .entries_mut()
            .insert("nested.7z".into(), inner.to_bytes().unwrap());
        let reopened_outer = Zip::ZipFile::from_bytes(&outer.to_bytes().unwrap()).unwrap();
        let mut reopened_inner =
            SevenZip::SevenZipFile::from_bytes(reopened_outer.get("nested.7z").unwrap()).unwrap();
        reopened_inner
            .replace("deep/value.txt", b"after".to_vec())
            .unwrap();
        assert_eq!(
            SevenZip::SevenZipFile::from_bytes(&reopened_inner.to_bytes().unwrap())
                .unwrap()
                .get("deep/value.txt"),
            Some(b"after".as_slice())
        );
    }

    #[test]
    fn mixed_sarc_to_zip_roundtrip() {
        use roead::{
            sarc::{Sarc, SarcWriter},
            Endian,
        };
        let mut zip = Zip::ZipFile::default();
        zip.entries_mut()
            .insert("deep/value.txt".into(), b"value".to_vec());
        let mut sarc_writer = SarcWriter::new(Endian::Little);
        sarc_writer.add_file("nested.zip", zip.to_bytes().unwrap());
        let sarc = Sarc::new(sarc_writer.to_binary()).unwrap();
        let nested = Zip::ZipFile::from_bytes(sarc.get_data("nested.zip").unwrap()).unwrap();
        assert_eq!(nested.get("deep/value.txt"), Some(b"value".as_slice()));
    }

    #[test]
    fn mixed_zip_to_rar_when_runtime_is_installed() {
        if Rar::RarFile::discover_executable().is_err() {
            eprintln!("skipping nested RAR test: rar.exe not installed");
            return;
        }
        let mut rar = Rar::RarFile::default();
        rar.entries_mut()
            .insert("deep/value.txt".into(), b"value".to_vec());
        let mut zip = Zip::ZipFile::default();
        zip.entries_mut()
            .insert("nested.rar".into(), rar.to_bytes().unwrap());
        let reopened_zip = Zip::ZipFile::from_bytes(&zip.to_bytes().unwrap()).unwrap();
        let reopened_rar =
            Rar::RarFile::from_bytes(reopened_zip.get("nested.rar").unwrap()).unwrap();
        assert_eq!(
            reopened_rar.get("deep/value.txt"),
            Some(b"value".as_slice())
        );
    }
}
