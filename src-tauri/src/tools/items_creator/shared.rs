//! The mod-wide files every item edits, each opened once per run.
//!
//! GameDataList, the US English Mals archive, the RSDB tables, the Tag
//! product, the SharpInfo table and the vendor packs are shared by all items
//! of a mod. Generating several items (or one armor with upgrade ranks) used
//! to reopen, re-parse, re-serialize and re-compress each of them per actor,
//! which dominated the run time. [`SharedFiles`] keeps every such document
//! parsed in memory: the first request loads it from the mod ROMFS when a
//! previous run left one there, or from the clean ROMFS otherwise; every item
//! then edits the in-memory document, and [`SharedFiles::flush`] serializes,
//! verifies and writes each file exactly once.
//!
//! Per-item files (actor packs, models, textures, icons) are not shared and
//! stay with their generators.

use crate::{
    file_format::{
        BinTextFile::{BymlFile, FileData},
        GameDataList::{recalculate_save_metadata, GameDataList},
        Pack::PackFile,
        TagProduct::TagProduct,
    },
    Zstd::{TotkZstd, ZstdDictionary},
};
use roead::byml::Byml;
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

/// A check run against the document re-parsed from the bytes about to be
/// written, so a serialization defect never reaches the disk unnoticed.
pub type BymlCheck = Box<dyn Fn(&Byml) -> io::Result<()>>;
pub type PackCheck = Box<dyn Fn(&PackFile<'_>) -> io::Result<()>>;

/// A BYML table (RSDB, SharpInfo).
pub struct BymlDocument<'a> {
    pub file: BymlFile<'a>,
    checks: Vec<BymlCheck>,
}

impl BymlDocument<'_> {
    /// Verified on the re-parsed output at flush.
    pub fn expect(&mut self, check: impl Fn(&Byml) -> io::Result<()> + 'static) {
        self.checks.push(Box::new(check));
    }
}

/// The GameDataList: a BYML with its own binary writer and save-layout
/// metadata that must be recomputed after flags are added.
pub struct GameDataDocument<'a> {
    pub file: BymlFile<'a>,
    checks: Vec<BymlCheck>,
}

impl GameDataDocument<'_> {
    pub fn expect(&mut self, check: impl Fn(&Byml) -> io::Result<()> + 'static) {
        self.checks.push(Box::new(check));
    }
}

/// The Tag product, whose entries are edited through its parsed tag map.
pub struct TagDocument<'a> {
    pub tag: TagProduct<'a>,
    expected_paths: Vec<String>,
}

impl TagDocument<'_> {
    /// An `actor_tag_data` key that must exist in the written file.
    pub fn expect_path(&mut self, path: String) {
        self.expected_paths.push(path);
    }
}

/// A SARC archive (Mals, vendor packs) edited by replacing whole members.
pub struct PackDocument<'a> {
    pub pack: PackFile<'a>,
    replacements: BTreeMap<String, Vec<u8>>,
    checks: Vec<PackCheck>,
}

impl PackDocument<'_> {
    /// The member as it currently stands: an earlier replacement of this run
    /// when there is one, the loaded archive's copy otherwise, so consecutive
    /// items build on each other's edits.
    pub fn entry(&self, path: &str) -> io::Result<&[u8]> {
        if let Some(data) = self.replacements.get(path) {
            return Ok(data);
        }
        self.pack.sarc.get_data(path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("pack entry is missing: {path}"),
            )
        })
    }

    pub fn replace(&mut self, path: String, data: Vec<u8>) {
        self.replacements.insert(path, data);
    }

    pub fn is_modified(&self) -> bool {
        !self.replacements.is_empty()
    }

    pub fn expect(&mut self, check: impl Fn(&PackFile<'_>) -> io::Result<()> + 'static) {
        self.checks.push(Box::new(check));
    }
}

enum SharedDocument<'a> {
    Byml(BymlDocument<'a>),
    GameData(GameDataDocument<'a>),
    Tag(TagDocument<'a>),
    Pack(PackDocument<'a>),
}

struct Entry<'a> {
    /// Where the document was loaded from (the mod copy or the clean file).
    source: PathBuf,
    document: SharedDocument<'a>,
}

/// The shared documents of one generation run, keyed by destination.
pub struct SharedFiles<'a> {
    clean_romfs: PathBuf,
    output_romfs: PathBuf,
    zstd: Arc<TotkZstd<'a>>,
    entries: BTreeMap<PathBuf, Entry<'a>>,
}

impl<'a> SharedFiles<'a> {
    pub fn new(clean_romfs: &Path, output_romfs: &Path, zstd: Arc<TotkZstd<'a>>) -> Self {
        Self {
            clean_romfs: clean_romfs.to_path_buf(),
            output_romfs: output_romfs.to_path_buf(),
            zstd,
            entries: BTreeMap::new(),
        }
    }

    pub fn clean_romfs(&self) -> &Path {
        &self.clean_romfs
    }

    pub fn output_romfs(&self) -> &Path {
        &self.output_romfs
    }

    pub fn zstd(&self) -> Arc<TotkZstd<'a>> {
        self.zstd.clone()
    }

    /// `(clean file, mod file)` of a ROMFS-relative path.
    pub fn pair(&self, relative: &str) -> (PathBuf, PathBuf) {
        (
            self.clean_romfs.join(relative),
            self.output_romfs.join(relative),
        )
    }

    /// The file the document at `destination` is loaded from: the mod copy a
    /// previous run (or an earlier flush) wrote, else the clean file.
    fn source_for(clean_source: &Path, destination: &Path) -> PathBuf {
        if destination.is_file() {
            destination.to_path_buf()
        } else {
            clean_source.to_path_buf()
        }
    }

    fn key(destination: &Path) -> PathBuf {
        PathBuf::from(destination.to_string_lossy().replace('\\', "/"))
    }

    /// A BYML table, loaded on first use.
    pub fn byml(
        &mut self,
        clean_source: &Path,
        destination: &Path,
    ) -> io::Result<&mut BymlDocument<'a>> {
        let key = Self::key(destination);
        if !self.entries.contains_key(&key) {
            let source = Self::source_for(clean_source, destination);
            let file = BymlFile::new(&source, self.zstd.clone()).ok_or_else(|| {
                invalid_data(format!("failed to parse BYML {}", source.display()))
            })?;
            self.entries.insert(
                key.clone(),
                Entry {
                    source,
                    document: SharedDocument::Byml(BymlDocument {
                        file,
                        checks: Vec::new(),
                    }),
                },
            );
        }
        match &mut self.entries.get_mut(&key).expect("just inserted").document {
            SharedDocument::Byml(document) => Ok(document),
            _ => Err(invalid_data(format!(
                "{} is already open as a different document kind",
                destination.display()
            ))),
        }
    }

    /// The GameDataList, loaded on first use.
    pub fn game_data(
        &mut self,
        clean_source: &Path,
        destination: &Path,
    ) -> io::Result<&mut GameDataDocument<'a>> {
        let key = Self::key(destination);
        if !self.entries.contains_key(&key) {
            let source = Self::source_for(clean_source, destination);
            let file = BymlFile::new(&source, self.zstd.clone()).ok_or_else(|| {
                invalid_data(format!("invalid GameDataList {}", source.display()))
            })?;
            self.entries.insert(
                key.clone(),
                Entry {
                    source,
                    document: SharedDocument::GameData(GameDataDocument {
                        file,
                        checks: Vec::new(),
                    }),
                },
            );
        }
        match &mut self.entries.get_mut(&key).expect("just inserted").document {
            SharedDocument::GameData(document) => Ok(document),
            _ => Err(invalid_data(format!(
                "{} is already open as a different document kind",
                destination.display()
            ))),
        }
    }

    /// The Tag product, loaded on first use.
    pub fn tag(
        &mut self,
        clean_source: &Path,
        destination: &Path,
    ) -> io::Result<&mut TagDocument<'a>> {
        let key = Self::key(destination);
        if !self.entries.contains_key(&key) {
            let source = Self::source_for(clean_source, destination);
            let compressed = fs::read(&source)?;
            let tag = TagProduct::from_binary(&compressed, &source, self.zstd.clone()).ok_or_else(
                || invalid_data(format!("failed to parse Tag.Product {}", source.display())),
            )?;
            self.entries.insert(
                key.clone(),
                Entry {
                    source,
                    document: SharedDocument::Tag(TagDocument {
                        tag,
                        expected_paths: Vec::new(),
                    }),
                },
            );
        }
        match &mut self.entries.get_mut(&key).expect("just inserted").document {
            SharedDocument::Tag(document) => Ok(document),
            _ => Err(invalid_data(format!(
                "{} is already open as a different document kind",
                destination.display()
            ))),
        }
    }

    /// A SARC archive, loaded on first use.
    pub fn pack(
        &mut self,
        clean_source: &Path,
        destination: &Path,
    ) -> io::Result<&mut PackDocument<'a>> {
        let key = Self::key(destination);
        if !self.entries.contains_key(&key) {
            let source = Self::source_for(clean_source, destination);
            if !source.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("archive is missing: {}", source.display()),
                ));
            }
            let compressed = fs::read(&source)?;
            let pack = PackFile::from_binary(&compressed, self.zstd.clone())?;
            self.entries.insert(
                key.clone(),
                Entry {
                    source,
                    document: SharedDocument::Pack(PackDocument {
                        pack,
                        replacements: BTreeMap::new(),
                        checks: Vec::new(),
                    }),
                },
            );
        }
        match &mut self.entries.get_mut(&key).expect("just inserted").document {
            SharedDocument::Pack(document) => Ok(document),
            _ => Err(invalid_data(format!(
                "{} is already open as a different document kind",
                destination.display()
            ))),
        }
    }

    /// Serializes, verifies and writes every document once, then forgets
    /// them (a later request reloads the written file). Returns the files
    /// written, in path order.
    pub fn flush(&mut self) -> io::Result<Vec<PathBuf>> {
        let entries = std::mem::take(&mut self.entries);
        let mut written = Vec::with_capacity(entries.len());
        for (destination, entry) in entries {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            match entry.document {
                SharedDocument::Byml(document) => {
                    let rebuilt = document.file.to_binary_preserving_header()?;
                    let reparsed =
                        BymlFile::from_binary(&rebuilt, self.zstd.clone(), &destination)?;
                    for check in &document.checks {
                        check(&reparsed.pio)?;
                    }
                    let bytes = compress_like(&document.file.file_data, &self.zstd, rebuilt)?;
                    fs::write(&destination, bytes)?;
                }
                SharedDocument::GameData(mut document) => {
                    // The new flags enlarge the save data; keep the layout
                    // metadata in sync.
                    recalculate_save_metadata(&mut document.file.pio)?;
                    let text = document.file.pio.to_text();
                    let binary = GameDataList::text_to_binary(&text)?;
                    let reparsed = BymlFile::from_binary(&binary, self.zstd.clone(), &destination)
                        .map_err(|error| {
                            invalid_data(format!(
                                "GameDataList::text_to_binary produced invalid BYML: {error}"
                            ))
                        })?;
                    for check in &document.checks {
                        check(&reparsed.pio)?;
                    }
                    let bytes = compress_like(&document.file.file_data, &self.zstd, binary)?;
                    fs::write(&destination, bytes)?;
                }
                SharedDocument::Tag(mut document) => {
                    let text = document.tag.to_text();
                    document
                        .tag
                        .save(destination.to_string_lossy().into_owned(), &text)?;
                    if !document.expected_paths.is_empty() {
                        let saved = fs::read(&destination)?;
                        let verification =
                            TagProduct::from_binary(&saved, &destination, self.zstd.clone())
                                .ok_or_else(|| invalid_data("generated Tag.Product is invalid"))?;
                        for path in &document.expected_paths {
                            if !verification.actor_tag_data.contains_key(path) {
                                return Err(invalid_data(format!(
                                    "generated Tag.Product entry is missing: {path}"
                                )));
                            }
                        }
                    }
                }
                SharedDocument::Pack(document) => {
                    if !document.is_modified() {
                        // Nothing changed: the mod still needs its own copy
                        // when the archive came from the clean ROMFS.
                        if entry.source != destination {
                            fs::copy(&entry.source, &destination)?;
                        }
                    } else {
                        let bytes = document
                            .pack
                            .rebuild_replacing_entries(document.replacements)?;
                        let verification = PackFile::from_binary(&bytes, self.zstd.clone())?;
                        for check in &document.checks {
                            check(&verification)?;
                        }
                        fs::write(&destination, bytes)?;
                    }
                }
            }
            written.push(destination);
        }
        Ok(written)
    }
}

/// Recompresses serialized data the way the loaded file was compressed.
fn compress_like(file_data: &FileData, zstd: &TotkZstd<'_>, data: Vec<u8>) -> io::Result<Vec<u8>> {
    match file_data.compression {
        Some(ZstdDictionary::Yaz0) => {
            TotkZstd::compress_yaz0_with_alignment(&data, file_data.yaz0_alignment)
        }
        Some(dictionary) => zstd.compress_with_dictionary(&data, dictionary),
        None => Ok(data),
    }
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
