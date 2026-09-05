use crate::{
    file_format::Pack::get_sarc_entries_data,
    Settings::Magic,
    Zstd::{sha256, TotkFileType, TotkZstd},
};
use roead::sarc::Sarc;
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
    error::Error,
    fmt::{self, Display, Formatter},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use walkdir::WalkDir;

const ALIGNMENT: u64 = 0x20;

/// Calculates the runtime allocation stored in a Tears of the Kingdom RESTBL.
///
/// `TotkFileType` is the primary selector. `resource_path` is still required
/// because the application enum intentionally groups formats whose RESTBL
/// formulas differ, such as `.byml` and `.bgyml`, under one viewer type.
#[derive(Clone)]
pub struct RstbEstimator<'a> {
    zstd: Arc<TotkZstd<'a>>,
    dev_mode: bool,
    vanilla_romfs: Option<PathBuf>,
    vanilla_sarc_hashes: Option<Arc<HashMap<String, String>>>,
    pub modified_sarc_entries: HashSet<String>,
    pub entries: HashMap<String, u32>,
}

impl<'a> RstbEstimator<'a> {
    pub fn new(zstd: Arc<TotkZstd<'a>>) -> Self {
        Self {
            zstd,
            dev_mode: false,
            vanilla_romfs: None,
            vanilla_sarc_hashes: None,
            modified_sarc_entries: HashSet::new(),
            entries: HashMap::new(),
        }
    }

    pub fn with_dev_mode(zstd: Arc<TotkZstd<'a>>, dev_mode: bool) -> Self {
        Self {
            zstd,
            dev_mode,
            vanilla_romfs: None,
            vanilla_sarc_hashes: None,
            modified_sarc_entries: HashSet::new(),
            entries: HashMap::new(),
        }
    }

    pub const fn dev_mode(&self) -> bool {
        self.dev_mode
    }

    /// Uses matching archives from a clean ROMFS to identify unchanged SARC
    /// members before estimating them.
    pub fn set_vanilla_romfs(&mut self, romfs: impl AsRef<Path>) {
        self.vanilla_romfs = Some(romfs.as_ref().to_path_buf());
    }

    /// Recursively estimates every regular resource file below `folder`.
    ///
    /// Keys use forward slashes and are relative to `folder`. Compression
    /// wrappers (`.zs`, `.zstd`, and `.mc`) are removed from normal RESTBL
    /// resource paths; `.ta.zs` remains unchanged because it has its own rule.
    /// Existing RESTBL tables are skipped. SARC members are added using their
    /// full internal paths, except when their SHA-256 matches the bundled
    /// vanilla hash table.
    ///
    /// The update is transactional: `self.entries` changes only after the
    /// complete folder succeeds.
    pub fn estimate_folder(
        &mut self,
        folder: impl AsRef<Path>,
    ) -> Result<&HashMap<String, u32>, RstbEstimateError> {
        let root = folder.as_ref();
        if !root.is_dir() {
            return Err(RstbEstimateError::new(format!(
                "estimate folder is not a directory: '{}'",
                root.display()
            )));
        }

        let mut estimated = HashMap::<String, u32>::new();
        let mut modified_sarc_entries = HashSet::new();
        for entry in WalkDir::new(root).follow_links(false) {
            let entry = entry.map_err(|error| {
                RstbEstimateError::new(format!("failed to traverse '{}': {error}", root.display()))
            })?;
            if !entry.file_type().is_file() {
                continue;
            }

            let disk_path = entry.path();
            let relative = disk_path.strip_prefix(root).map_err(|error| {
                RstbEstimateError::new(format!(
                    "failed to make '{}' relative to '{}': {error}",
                    disk_path.display(),
                    root.display()
                ))
            })?;
            if relative
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("rstb.yaml"))
            {
                continue;
            }
            let resource_path = restbl_entry_path(relative);
            let file_type = infer_totk_file_type(&resource_path);
            if file_type == TotkFileType::Restbl {
                continue;
            }

            let data = fs::read(disk_path).map_err(|error| {
                RstbEstimateError::new(format!("failed to read '{}': {error}", disk_path.display()))
            })?;
            let normalized_path = normalize_path(relative);
            let effective_data = self.effective_data(relative, &data).map_err(|error| {
                RstbEstimateError::new(format!(
                    "failed to estimate '{}': {error}",
                    disk_path.display()
                ))
            })?;
            let value = self
                .estimate_effective(file_type, &normalized_path, &effective_data)
                .map_err(|error| {
                    RstbEstimateError::new(format!(
                        "failed to estimate '{}': {error}",
                        disk_path.display()
                    ))
                })?;
            insert_estimate(&mut estimated, resource_path, value);

            if !normalized_path.starts_with("mals/") {
                if let Some(sarc_data) = sarc_payload(&effective_data).map_err(|error| {
                    RstbEstimateError::new(format!(
                        "failed to read archive '{}': {error}",
                        disk_path.display()
                    ))
                })? {
                    if let Some(vanilla_hashes) = self.vanilla_archive_hashes(relative)? {
                        self.estimate_sarc_entries(
                            relative,
                            &sarc_data,
                            &vanilla_hashes,
                            &mut estimated,
                            &mut modified_sarc_entries,
                        )?;
                    } else {
                        if self.vanilla_sarc_hashes.is_none() {
                            self.vanilla_sarc_hashes = Some(get_sarc_entries_data());
                        }
                        let vanilla_hashes =
                            self.vanilla_sarc_hashes.as_ref().ok_or_else(|| {
                                RstbEstimateError::new(
                                    "failed to initialize the vanilla SARC hash cache",
                                )
                            })?;
                        self.estimate_sarc_entries(
                            relative,
                            &sarc_data,
                            vanilla_hashes,
                            &mut estimated,
                            &mut modified_sarc_entries,
                        )?;
                    }
                }
            }
        }

        self.entries = estimated;
        self.modified_sarc_entries = modified_sarc_entries;
        Ok(&self.entries)
    }

    fn vanilla_archive_hashes(
        &self,
        relative_path: &Path,
    ) -> Result<Option<HashMap<String, String>>, RstbEstimateError> {
        let Some(romfs) = &self.vanilla_romfs else {
            return Ok(None);
        };
        let source = romfs.join(relative_path);
        if !source.is_file() {
            return Ok(None);
        }
        let bytes = fs::read(&source).map_err(|error| {
            RstbEstimateError::new(format!(
                "failed to read vanilla archive '{}': {error}",
                source.display()
            ))
        })?;
        let effective = self
            .effective_data(relative_path, &bytes)
            .map_err(|error| {
                RstbEstimateError::new(format!(
                    "failed to decompress vanilla archive '{}': {error}",
                    source.display()
                ))
            })?;
        let Some(data) = sarc_payload(&effective).map_err(|error| {
            RstbEstimateError::new(format!(
                "failed to read vanilla archive '{}': {error}",
                source.display()
            ))
        })?
        else {
            return Err(RstbEstimateError::new(format!(
                "matching vanilla resource is not a SARC: '{}'",
                source.display()
            )));
        };
        let sarc = Sarc::new(data).map_err(|error| {
            RstbEstimateError::new(format!(
                "failed to parse vanilla SARC '{}': {error}",
                source.display()
            ))
        })?;
        let hashes = sarc
            .files()
            .filter_map(|file| {
                file.name
                    .filter(|name| !name.is_empty())
                    .map(|name| (name.to_owned(), sha256(file.data().to_vec())))
            })
            .collect();
        Ok(Some(hashes))
    }

    /// Recursively estimates `folder` and saves the resulting map as
    /// `rstb.yaml` in the selected folder's parent directory.
    pub fn estimate_folder_and_save(
        &mut self,
        folder: impl AsRef<Path>,
    ) -> Result<PathBuf, RstbEstimateError> {
        let folder = folder.as_ref();
        self.estimate_folder(folder)?;
        let parent = folder.parent().ok_or_else(|| {
            RstbEstimateError::new(format!(
                "estimate folder has no parent directory: '{}'",
                folder.display()
            ))
        })?;
        let output = parent.join("rstb.yaml");
        self.save_yaml(&output)?;
        Ok(output)
    }

    /// Saves entries as pretty, key-sorted JSON.
    pub fn save_json(&self, output: impl AsRef<Path>) -> Result<(), RstbEstimateError> {
        let output = output.as_ref();
        let mut text = serde_json::to_string_pretty(&self.sorted_entries()).map_err(|error| {
            RstbEstimateError::new(format!("failed to serialize RESTBL JSON: {error}"))
        })?;
        text.push('\n');
        write_export(output, text.as_bytes())
    }

    /// Saves entries as key-sorted YAML.
    pub fn save_yaml(&self, output: impl AsRef<Path>) -> Result<(), RstbEstimateError> {
        let output = output.as_ref();
        let text = serde_yaml::to_string(&self.sorted_entries()).map_err(|error| {
            RstbEstimateError::new(format!("failed to serialize RESTBL YAML: {error}"))
        })?;
        write_export(output, text.as_bytes())
    }

    /// Selects JSON or YAML serialization from the output extension.
    pub fn save_entries(&self, output: impl AsRef<Path>) -> Result<(), RstbEstimateError> {
        let output = output.as_ref();
        match output
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("json") => self.save_json(output),
            Some("yaml" | "yml") => self.save_yaml(output),
            _ => Err(RstbEstimateError::new(
                "RESTBL estimate export must use .json, .yaml, or .yml",
            )),
        }
    }

    fn sorted_entries(&self) -> BTreeMap<String, u32> {
        let mut entries = BTreeMap::<String, u32>::new();
        for (path, value) in &self.entries {
            let path = slash_path(path);
            entries
                .entry(path)
                .and_modify(|existing| *existing = (*existing).max(*value))
                .or_insert(*value);
        }
        entries
    }

    /// Estimates an entry from an effective, normally decompressed payload.
    ///
    /// A `.zs` or `.zstd` suffix is stripped for format classification. If
    /// `data` is still a Zstandard frame, use [`Self::estimate_maybe_compressed`]
    /// so the correct TotK dictionary can be selected. `.ta.zs` is the one
    /// intentional exception: its compressed byte length is used directly.
    pub fn estimate(
        &self,
        file_type: TotkFileType,
        resource_path: impl AsRef<Path>,
        data: &[u8],
    ) -> Result<u32, RstbEstimateError> {
        let path = normalize_path(resource_path.as_ref());
        if !path.ends_with(".ta.zs") && Magic::is_zstd(data) {
            return Err(RstbEstimateError::new(
                "compressed Zstandard input requires estimate_maybe_compressed",
            ));
        }
        self.estimate_effective(file_type, &path, data)
    }

    /// Decompresses an input Zstandard frame when necessary, then calculates
    /// the RESTBL allocation. TotK dictionary selection is delegated to the
    /// application's existing `TotkZstd` service.
    pub fn estimate_maybe_compressed(
        &self,
        file_type: TotkFileType,
        resource_path: impl AsRef<Path>,
        data: &[u8],
    ) -> Result<u32, RstbEstimateError> {
        let path_ref = resource_path.as_ref();
        let path = normalize_path(path_ref);
        let effective_data = self.effective_data(path_ref, data)?;
        self.estimate_effective(file_type, &path, &effective_data)
    }

    fn effective_data<'b>(
        &self,
        resource_path: &Path,
        data: &'b [u8],
    ) -> Result<Cow<'b, [u8]>, RstbEstimateError> {
        let path = normalize_path(resource_path);
        if Magic::is_mcpk(data) {
            let decompressed = self.zstd.decompress_mcpk(data).map_err(|error| {
                RstbEstimateError::new(format!(
                    "failed to MCPK-decompress '{}': {error}",
                    resource_path.display()
                ))
            })?;
            if !decompressed.starts_with(b"FRES") {
                return Err(RstbEstimateError::new(format!(
                    "MCPK payload is not a raw FRES resource: '{}'",
                    resource_path.display()
                )));
            }
            Ok(Cow::Owned(decompressed))
        } else if !path.ends_with(".ta.zs") && Magic::is_zstd(data) {
            let (decompressed, _) = self
                .zstd
                .try_decompress_for_path(resource_path, data)
                .map_err(|error| {
                    RstbEstimateError::new(format!(
                        "failed to decompress '{}': {error}",
                        resource_path.display()
                    ))
                })?;
            Ok(Cow::Owned(decompressed))
        } else {
            Ok(Cow::Borrowed(data))
        }
    }

    fn estimate_sarc_entries(
        &self,
        archive_path: &Path,
        data: &[u8],
        vanilla_hashes: &HashMap<String, String>,
        estimated: &mut HashMap<String, u32>,
        modified: &mut HashSet<String>,
    ) -> Result<(), RstbEstimateError> {
        let sarc = Sarc::new(data.to_vec()).map_err(|error| {
            RstbEstimateError::new(format!(
                "failed to parse SARC '{}': {error}",
                archive_path.display()
            ))
        })?;

        for file in sarc.files() {
            let Some(file_name) = file.name.filter(|name| !name.is_empty()) else {
                continue;
            };
            let file_data = file.data();
            let hash = sha256(file_data.to_vec());
            if vanilla_hashes
                .get(file_name)
                .is_some_and(|vanilla_hash| vanilla_hash == &hash)
            {
                continue;
            }

            let resource_path = restbl_entry_path(Path::new(file_name));
            if vanilla_hashes.contains_key(file_name) {
                modified.insert(resource_path.clone());
            }
            let file_type = infer_totk_file_type(&resource_path);
            if file_type == TotkFileType::Restbl {
                continue;
            }
            let value = self
                .estimate_maybe_compressed(file_type, &resource_path, file_data)
                .map_err(|error| {
                    RstbEstimateError::new(format!(
                        "failed to estimate internal file '{}' from '{}': {error}",
                        file_name,
                        archive_path.display()
                    ))
                })?;
            insert_estimate(estimated, resource_path, value);
        }
        Ok(())
    }

    fn estimate_effective(
        &self,
        file_type: TotkFileType,
        resource_path: &str,
        data: &[u8],
    ) -> Result<u32, RstbEstimateError> {
        if resource_path.ends_with(".ta.zs") {
            let value = checked_add(align_32(data.len() as u64)?, 256, ".ta.zs overhead")?;
            return self.finish(value);
        }

        let effective_path = strip_zstd_suffix(resource_path);
        let effective_path = effective_path
            .strip_suffix(".mc")
            .unwrap_or(&effective_path);
        let effective_size = sarc_declared_size(data).unwrap_or(data.len() as u64);
        let aligned_size = align_32(effective_size)?;
        let rule = SizeRule::for_resource(file_type, &effective_path)?;
        let mut value = rule.calculate(aligned_size, data)?;

        if is_shader_archive(&effective_path) {
            value = checked_add(value, 3712, "shader archive overhead")?;
        }
        if effective_path.contains("event/eventflow/dm_ed_0004.bfevfl") {
            value = checked_add(value, 192, "Dm_ED_0004 overhead")?;
        }
        if effective_path.contains("static.nin_nx_nvn.esetb.byml") {
            value = checked_add(value, 3840, "static ESETB overhead")?;
        }

        self.finish(value)
    }

    fn finish(&self, value: u64) -> Result<u32, RstbEstimateError> {
        let value = if self.dev_mode {
            checked_add(
                checked_mul(value, 13, "development-mode multiplier")?,
                5,
                "development-mode rounding",
            )? / 10
        } else {
            value
        };
        u32::try_from(value)
            .map_err(|_| RstbEstimateError::new("estimated RESTBL value exceeds u32"))
    }
}

fn sarc_declared_size(data: &[u8]) -> Option<u64> {
    if data.len() < 0x0c || &data[..4] != b"SARC" {
        return None;
    }
    let bytes: [u8; 4] = data[8..12].try_into().ok()?;
    let size = match &data[6..8] {
        [0xfe, 0xff] => u32::from_be_bytes(bytes),
        [0xff, 0xfe] => u32::from_le_bytes(bytes),
        _ => return None,
    };
    (size != 0 && size as usize <= data.len()).then_some(u64::from(size))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SizeRule {
    Fixed(u64),
    Ainb,
    Asb,
    Bgyml,
    Bfres,
    Bstar,
    Generic,
}

impl SizeRule {
    fn for_resource(
        file_type: TotkFileType,
        resource_path: &str,
    ) -> Result<Self, RstbEstimateError> {
        // These multi-part suffixes cannot be represented by TotkFileType.
        if resource_path.ends_with(".casset.byml") {
            return Ok(Self::Fixed(448));
        }
        if resource_path.ends_with(".bgyml") {
            return Ok(Self::Bgyml);
        }
        if resource_path.ends_with(".bstar") {
            return Ok(Self::Bstar);
        }

        let rule = match file_type {
            TotkFileType::AINB => Self::Ainb,
            TotkFileType::ASB => Self::Asb,
            TotkFileType::Sarc | TotkFileType::MalsSarc => Self::Fixed(384),
            TotkFileType::Byml
            | TotkFileType::TagProduct
            | TotkFileType::Bcett
            | TotkFileType::Esetb
            | TotkFileType::Bntx
            | TotkFileType::Bphcl
            | TotkFileType::Bphhb
            | TotkFileType::Xlink
            | TotkFileType::Text => Self::Fixed(256),
            TotkFileType::Evfl => Self::Fixed(288),
            TotkFileType::Bfres => Self::Bfres,
            TotkFileType::Restbl => {
                return Err(RstbEstimateError::new(
                    "RESTBL files do not receive RESTBL allocation entries",
                ));
            }
            TotkFileType::None => {
                return Err(RstbEstimateError::new(
                    "cannot estimate an unclassified file",
                ));
            }
            TotkFileType::Aamp
            | TotkFileType::Archive
            | TotkFileType::Bars
            | TotkFileType::Compressed
            | TotkFileType::Fbx
            | TotkFileType::G1M
            | TotkFileType::Glb
            | TotkFileType::Mii
            | TotkFileType::Hkcl
            | TotkFileType::Hkrg
            | TotkFileType::Bphyssb
            | TotkFileType::Image
            | TotkFileType::Msbt
            | TotkFileType::Bwav
            | TotkFileType::Bfwav
            | TotkFileType::Amta
            | TotkFileType::Riff
            | TotkFileType::Other
            | TotkFileType::SmoSaveFile => rule_from_path(resource_path),
        };
        Ok(rule)
    }

    fn calculate(self, aligned_size: u64, data: &[u8]) -> Result<u64, RstbEstimateError> {
        match self {
            Self::Fixed(overhead) => checked_add(aligned_size, overhead, "format overhead"),
            Self::Bgyml => checked_mul(
                checked_add(aligned_size, 1000, "BGYML base overhead")?,
                8,
                "BGYML multiplier",
            ),
            Self::Bfres => checked_add(
                checked_mul(aligned_size, 20, "BFRES allocation")?,
                20_000,
                "BFRES safety overhead",
            ),
            Self::Generic => checked_mul(
                checked_add(aligned_size, 1500, "generic base overhead")?,
                4,
                "generic multiplier",
            ),
            Self::Bstar => {
                let entry_count = u64::from(read_u32_le(data, 0x08, "BSTAR entry count")?);
                checked_add(
                    checked_add(aligned_size, 288, "BSTAR base overhead")?,
                    checked_mul(entry_count, 8, "BSTAR entry allocation")?,
                    "BSTAR allocation",
                )
            }
            Self::Asb => {
                let node_count = u64::from(read_u32_le(data, 0x10, "ASB node count")?);
                let exb = exb_allocation(data, 0x60, false, "ASB")?;
                checked_add(
                    checked_add(
                        checked_add(aligned_size, 552, "ASB base overhead")?,
                        checked_mul(node_count, 40, "ASB node allocation")?,
                        "ASB node total",
                    )?,
                    exb,
                    "ASB EXB allocation",
                )
            }
            Self::Ainb => {
                let exb = exb_allocation(data, 0x44, true, "AINB")?;
                checked_add(
                    checked_add(aligned_size, 392, "AINB base overhead")?,
                    exb,
                    "AINB EXB allocation",
                )
            }
        }
    }
}

fn rule_from_path(resource_path: &str) -> SizeRule {
    if resource_path.ends_with(".ainb") {
        return SizeRule::Ainb;
    }
    if resource_path.ends_with(".asb") {
        return SizeRule::Asb;
    }

    let fixed = [
        (".baatarc", 256),
        (".baev", 288),
        (".bagst", 256),
        (".bars", 576),
        (".bcul", 256),
        (".beco", 256),
        (".belnk", 256),
        (".bfarc", 256),
        (".bfevfl", 288),
        (".bfsha", 256),
        (".bhtmp", 256),
        (".blal", 256),
        (".blarc", 256),
        (".blwp", 256),
        (".bnsh", 256),
        (".bntx", 256),
        (".bphcl", 256),
        (".bphhb", 256),
        (".bphnm", 288),
        (".bphsh", 368),
        (".bslnk", 256),
        (".byml", 256),
        (".cai", 256),
        (".chunk", 256),
        (".crbin", 256),
        (".cutinfo", 256),
        (".dpi", 256),
        (".genvb", 384),
        (".jpg", 256),
        (".pack", 384),
        (".png", 256),
        (".quad", 256),
        (".sarc", 384),
        (".tscb", 256),
        (".txt", 256),
        (".txtg", 256),
        (".vsts", 256),
        (".wbr", 256),
    ];
    fixed
        .into_iter()
        .find_map(|(suffix, overhead)| {
            resource_path
                .ends_with(suffix)
                .then_some(SizeRule::Fixed(overhead))
        })
        .unwrap_or(SizeRule::Generic)
}

fn exb_allocation(
    data: &[u8],
    offset_field: usize,
    allocate_empty_header: bool,
    format_name: &str,
) -> Result<u64, RstbEstimateError> {
    let exb_offset = read_u32_le(data, offset_field, &format!("{format_name} EXB offset"))?;
    if exb_offset == 0 {
        return Ok(if allocate_empty_header { 16 } else { 0 });
    }

    let exb_offset = usize::try_from(exb_offset)
        .map_err(|_| RstbEstimateError::new(format!("{format_name} EXB offset exceeds usize")))?;
    let signature_offset_field =
        checked_usize_add(exb_offset, 0x20, &format!("{format_name} EXB header"))?;
    let relative_signature_offset = usize::try_from(read_u32_le(
        data,
        signature_offset_field,
        &format!("{format_name} EXB signature offset"),
    )?)
    .map_err(|_| {
        RstbEstimateError::new(format!("{format_name} EXB signature offset exceeds usize"))
    })?;
    let signature_count_offset = checked_usize_add(
        exb_offset,
        relative_signature_offset,
        &format!("{format_name} EXB signature table"),
    )?;
    let signature_count = u64::from(read_u32_le(
        data,
        signature_count_offset,
        &format!("{format_name} EXB signature count"),
    )?);
    let signature_pairs = checked_add(signature_count, 1, "EXB signature rounding")? / 2;
    checked_add(
        16,
        checked_mul(signature_pairs, 8, "EXB signature allocation")?,
        "EXB allocation",
    )
}

fn read_u32_le(data: &[u8], offset: usize, field: &str) -> Result<u32, RstbEstimateError> {
    let end = checked_usize_add(offset, 4, field)?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| RstbEstimateError::new(format!("{field} is outside the file")))?;
    let bytes: [u8; 4] = bytes
        .try_into()
        .map_err(|_| RstbEstimateError::new(format!("{field} must contain four bytes")))?;
    Ok(u32::from_le_bytes(bytes))
}

fn align_32(value: u64) -> Result<u64, RstbEstimateError> {
    let rounded = checked_add(value, ALIGNMENT - 1, "32-byte alignment")?;
    Ok((rounded / ALIGNMENT) * ALIGNMENT)
}

fn checked_add(left: u64, right: u64, field: &str) -> Result<u64, RstbEstimateError> {
    left.checked_add(right)
        .ok_or_else(|| RstbEstimateError::new(format!("{field} overflow")))
}

fn checked_mul(left: u64, right: u64, field: &str) -> Result<u64, RstbEstimateError> {
    left.checked_mul(right)
        .ok_or_else(|| RstbEstimateError::new(format!("{field} overflow")))
}

fn checked_usize_add(left: usize, right: usize, field: &str) -> Result<usize, RstbEstimateError> {
    left.checked_add(right)
        .ok_or_else(|| RstbEstimateError::new(format!("{field} offset overflow")))
}

fn slash_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn insert_estimate(entries: &mut HashMap<String, u32>, path: String, value: u32) {
    entries
        .entry(slash_path(&path))
        .and_modify(|existing| *existing = (*existing).max(value))
        .or_insert(value);
}

fn normalize_path(path: &Path) -> String {
    slash_path(&path.to_string_lossy()).to_ascii_lowercase()
}

fn restbl_entry_path(path: &Path) -> String {
    let mut value = slash_path(&path.to_string_lossy());
    if value.to_ascii_lowercase().ends_with(".ta.zs") {
        return value;
    }
    loop {
        let lower = value.to_ascii_lowercase();
        let suffix = [".zstd", ".zs", ".mc"]
            .into_iter()
            .find(|suffix| lower.ends_with(suffix));
        let Some(suffix) = suffix else {
            return value;
        };
        value.truncate(value.len() - suffix.len());
    }
}

fn infer_totk_file_type(resource_path: &str) -> TotkFileType {
    let path = resource_path.to_ascii_lowercase();
    let file_name = path.rsplit('/').next().unwrap_or(&path);

    if file_name.starts_with("resourcesizetable.product.") && file_name.contains(".rsizetable") {
        TotkFileType::Restbl
    } else if file_name.starts_with("tag.product") {
        TotkFileType::TagProduct
    } else if path.ends_with(".ainb") {
        TotkFileType::AINB
    } else if path.ends_with(".asb") {
        TotkFileType::ASB
    } else if path.ends_with(".pack") || path.ends_with(".sarc") {
        TotkFileType::Sarc
    } else if path.ends_with(".bcett.byml") {
        TotkFileType::Bcett
    } else if path.ends_with(".esetb.byml") {
        TotkFileType::Esetb
    } else if path.ends_with(".byml") || path.ends_with(".bgyml") {
        TotkFileType::Byml
    } else if path.ends_with(".bfres") {
        TotkFileType::Bfres
    } else if path.ends_with(".bntx") {
        TotkFileType::Bntx
    } else if path.ends_with(".bphcl") {
        TotkFileType::Bphcl
    } else if path.ends_with(".bphhb") {
        TotkFileType::Bphhb
    } else if path.ends_with(".hkcl") {
        TotkFileType::Hkcl
    } else if path.ends_with(".msbt") || path.ends_with(".msyt") {
        TotkFileType::Msbt
    } else if path.ends_with(".bfevfl") {
        TotkFileType::Evfl
    } else if path.ends_with(".belnk") {
        TotkFileType::Xlink
    } else if path.ends_with(".txt") || path.ends_with(".txtg") {
        TotkFileType::Text
    } else if [
        ".bxml",
        ".baiprog",
        ".bas",
        ".baslist",
        ".bgparamlist",
        ".bmodellist",
        ".bphysics",
        ".bphyssb",
        ".bphnm",
        ".bphsc",
        ".bphsh",
        ".bslnk",
    ]
    .into_iter()
    .any(|suffix| path.ends_with(suffix))
    {
        TotkFileType::Aamp
    } else {
        TotkFileType::Other
    }
}

fn strip_zstd_suffix(path: &str) -> String {
    if let Some(value) = path.strip_suffix(".zstd") {
        value.to_owned()
    } else if let Some(value) = path.strip_suffix(".zs") {
        value.to_owned()
    } else {
        path.to_owned()
    }
}

fn sarc_payload(data: &[u8]) -> Result<Option<Cow<'_, [u8]>>, RstbEstimateError> {
    if Magic::is_sarc(data) {
        return Ok(Some(Cow::Borrowed(data)));
    }
    if Magic::is_yaz0(data) {
        let decompressed = TotkZstd::decompress_yaz0(data).map_err(|error| {
            RstbEstimateError::new(format!("failed to decompress Yaz0 data: {error}"))
        })?;
        if Magic::is_sarc(&decompressed) {
            return Ok(Some(Cow::Owned(decompressed)));
        }
    }
    Ok(None)
}

fn is_shader_archive(path: &str) -> bool {
    [
        "agl_resource.nin_nx_nvn.release.sarc",
        "gsys_resource.nin_nx_nvn.release.sarc",
        "tera_resource.nin_nx_nvn.release.sarc",
        "applicationpackage.nin_nx_nvn.release.sarc",
    ]
    .into_iter()
    .any(|name| path.contains(name))
}

fn write_export(output: &Path, data: &[u8]) -> Result<(), RstbEstimateError> {
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            RstbEstimateError::new(format!(
                "failed to create export directory '{}': {error}",
                parent.display()
            ))
        })?;
    }
    fs::write(output, data).map_err(|error| {
        RstbEstimateError::new(format!(
            "failed to save RESTBL estimates to '{}': {error}",
            output.display()
        ))
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RstbEstimateError {
    message: String,
}

impl RstbEstimateError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for RstbEstimateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for RstbEstimateError {}

#[cfg(test)]
mod tests {
    use super::RstbEstimator;
    use crate::{
        file_format::Pack::PackFile,
        parser::rstb::ResourceSizeTable,
        tools::items_creator::rstb::ModRstbProcessor,
        TotkConfig::TotkConfig,
        Zstd::{sha256, TotkFileType, TotkZstd, TOTK_ZSTD_COMPRESSION_LEVEL},
    };
    use roead::{sarc::SarcWriter, Endian};
    use std::{
        collections::{BTreeMap, HashMap},
        error::Error,
        fs,
        path::{Path, PathBuf},
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    type TestResult = Result<(), Box<dyn Error>>;

    fn test_zstd() -> Arc<TotkZstd<'static>> {
        Arc::new(TotkZstd::dictionaryless(
            Arc::new(TotkConfig::default()),
            TOTK_ZSTD_COMPRESSION_LEVEL,
        ))
    }

    fn test_estimator() -> RstbEstimator<'static> {
        RstbEstimator::new(test_zstd())
    }

    #[test]
    #[ignore = "requires a configured clean ROMFS"]
    fn real_unmodified_pack_preserves_internal_restbl_values_exactly() -> TestResult {
        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let pack_path = romfs.join("Pack/Actor/Weapon_Lsword_108.pack.zs");
        if !pack_path.is_file() {
            return Ok(());
        }
        let mut config = TotkConfig::default();
        config.romfs = romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(
            Arc::new(config),
            TOTK_ZSTD_COMPRESSION_LEVEL,
        )?);
        let output_romfs = unique_temp_directory()?;
        fs::create_dir_all(output_romfs.join("Pack/Actor"))?;
        fs::copy(
            &pack_path,
            output_romfs.join("Pack/Actor/Weapon_Lsword_108.pack.zs"),
        )?;
        let report = ModRstbProcessor::new(romfs, &output_romfs, zstd.clone()).generate()?;
        let pack = PackFile::from_binary(&fs::read(&pack_path)?, zstd.clone())?;
        let clean_rstb_path = romfs.join("System/Resource").join(
            report
                .output
                .file_name()
                .ok_or("generated RSTB filename is missing")?,
        );
        let (clean_raw, _) =
            zstd.try_decompress_for_path(&clean_rstb_path, &fs::read(&clean_rstb_path)?)?;
        let clean = ResourceSizeTable::from_bytes(&clean_raw)?;
        let (generated_raw, _) =
            zstd.try_decompress_for_path(&report.output, &fs::read(&report.output)?)?;
        let generated = ResourceSizeTable::from_bytes(&generated_raw)?;

        let mut checked = 0usize;
        for file in pack.sarc.files() {
            let Some(name) = file.name() else { continue };
            let entry = name.replace('\\', "/");
            let Some(expected) = clean.get(entry.clone()).copied() else {
                continue;
            };
            assert!(!report.entries.contains_key(&entry));
            assert_eq!(generated.get(entry), Some(&expected));
            checked += 1;
            if checked == 20 {
                break;
            }
        }
        assert_eq!(
            checked, 20,
            "not enough internal vanilla RESTBL entries found"
        );
        assert!(report
            .entries
            .contains_key("Pack/Actor/Weapon_Lsword_108.pack"));
        fs::remove_dir_all(output_romfs)?;
        Ok(())
    }

    #[test]
    fn fixed_and_generic_rules_use_aligned_size() -> TestResult {
        let estimator = test_estimator();
        let data = vec![0; 33];
        assert_eq!(
            estimator.estimate(TotkFileType::Bphcl, "Physics/Test.bphcl", &data)?,
            320
        );
        assert_eq!(
            estimator.estimate(TotkFileType::Bfres, "Model/Test.bfres", &data)?,
            21_280
        );
        Ok(())
    }

    #[test]
    fn fixed_path_rules_match_the_reference_table() -> TestResult {
        let estimator = test_estimator();
        let cases = [
            ("Test.baatarc", 256),
            ("Test.baev", 288),
            ("Test.bagst", 256),
            ("Test.bars", 576),
            ("Test.bcul", 256),
            ("Test.beco", 256),
            ("Test.belnk", 256),
            ("Test.bfarc", 256),
            ("Test.bfevfl", 288),
            ("Test.bfsha", 256),
            ("Test.bhtmp", 256),
            ("Test.blal", 256),
            ("Test.blarc", 256),
            ("Test.blwp", 256),
            ("Test.bnsh", 256),
            ("Test.bntx", 256),
            ("Test.bphcl", 256),
            ("Test.bphhb", 256),
            ("Test.bphnm", 288),
            ("Test.bphsh", 368),
            ("Test.bslnk", 256),
            ("Test.byml", 256),
            ("Test.cai", 256),
            ("Test.chunk", 256),
            ("Test.crbin", 256),
            ("Test.cutinfo", 256),
            ("Test.dpi", 256),
            ("Test.genvb", 384),
            ("Test.jpg", 256),
            ("Test.pack", 384),
            ("Test.png", 256),
            ("Test.quad", 256),
            ("Test.sarc", 384),
            ("Test.tscb", 256),
            ("Test.txt", 256),
            ("Test.txtg", 256),
            ("Test.vsts", 256),
            ("Test.wbr", 256),
        ];
        for (path, overhead) in cases {
            assert_eq!(
                estimator.estimate(TotkFileType::Other, path, &[0])?,
                32 + overhead,
                "{path}"
            );
        }
        Ok(())
    }

    #[test]
    fn byml_suffixes_keep_distinct_rules() -> TestResult {
        let estimator = test_estimator();
        let data = vec![0; 33];
        assert_eq!(
            estimator.estimate(TotkFileType::Byml, "RSDB/Test.byml", &data)?,
            320
        );
        assert_eq!(
            estimator.estimate(TotkFileType::Byml, "RSDB/Test.bgyml", &data)?,
            8_512
        );
        assert_eq!(
            estimator.estimate(TotkFileType::Byml, "RSDB/Test.casset.byml", &data)?,
            512
        );
        Ok(())
    }

    #[test]
    fn asb_uses_node_and_exb_signature_counts() -> TestResult {
        let estimator = test_estimator();
        let mut data = vec![0; 0x100];
        data[0x10..0x14].copy_from_slice(&2_u32.to_le_bytes());
        data[0x60..0x64].copy_from_slice(&0x80_u32.to_le_bytes());
        data[0xa0..0xa4].copy_from_slice(&0x40_u32.to_le_bytes());
        data[0xc0..0xc4].copy_from_slice(&3_u32.to_le_bytes());
        assert_eq!(
            estimator.estimate(TotkFileType::ASB, "AS/Test.asb", &data)?,
            920
        );
        Ok(())
    }

    #[test]
    fn ainb_allocates_empty_exb_header() -> TestResult {
        let estimator = test_estimator();
        let data = vec![0; 0x80];
        assert_eq!(
            estimator.estimate(TotkFileType::AINB, "AI/Test.ainb", &data)?,
            536
        );
        Ok(())
    }

    #[test]
    fn bstar_reads_entry_count() -> TestResult {
        let estimator = test_estimator();
        let mut data = vec![0; 12];
        data[0x08..0x0c].copy_from_slice(&3_u32.to_le_bytes());
        assert_eq!(
            estimator.estimate(TotkFileType::Other, "Sequence/Test.bstar", &data)?,
            344
        );
        Ok(())
    }

    #[test]
    fn path_specific_overheads_are_applied() -> TestResult {
        let estimator = test_estimator();
        assert_eq!(
            estimator.estimate(
                TotkFileType::Sarc,
                "Shader/agl_resource.Nin_NX_NVN.release.sarc",
                &[0],
            )?,
            4128
        );
        assert_eq!(
            estimator.estimate(
                TotkFileType::Evfl,
                "Event/EventFlow/Dm_ED_0004.bfevfl",
                &[0],
            )?,
            512
        );
        assert_eq!(
            estimator.estimate(
                TotkFileType::Esetb,
                "Effect/static.Nin_NX_NVN.esetb.byml",
                &[0],
            )?,
            4128
        );
        Ok(())
    }

    #[test]
    fn ta_zs_uses_compressed_length_and_dev_mode_is_applied_last() -> TestResult {
        let data = vec![0; 33];
        assert_eq!(
            test_estimator().estimate(TotkFileType::Other, "Terrain/Test.ta.zs", &data)?,
            320
        );
        assert_eq!(
            RstbEstimator::with_dev_mode(test_zstd(), true).estimate(
                TotkFileType::Bphcl,
                "Physics/Test.bphcl",
                &data
            )?,
            416
        );
        Ok(())
    }

    #[test]
    fn bfres_uses_effective_fres_payload_size() -> TestResult {
        let estimator = test_estimator();
        let mut raw = vec![0; 33];
        raw[..4].copy_from_slice(b"FRES");
        assert_eq!(
            estimator.estimate(TotkFileType::Bfres, "Model/Test.bfres", &raw)?,
            21_280
        );
        Ok(())
    }

    #[test]
    fn folder_estimates_relative_paths_and_exports_json_and_yaml() -> TestResult {
        let root = unique_temp_directory()?;
        fs::create_dir_all(root.join("Physics"))?;
        fs::create_dir_all(root.join("Model"))?;
        fs::create_dir_all(root.join("System/Resource"))?;
        let compressed_bphcl = zstd::stream::encode_all(&vec![0; 33][..], 1)?;
        fs::write(root.join("Physics/Test.bphcl.zs"), compressed_bphcl)?;

        let mut raw_bfres = vec![0; 33];
        raw_bfres[..4].copy_from_slice(b"FRES");
        fs::write(root.join("Model/Test.bfres"), raw_bfres)?;
        fs::write(
            root.join("System/Resource/ResourceSizeTable.Product.121.rsizetable"),
            b"RESTBL",
        )?;

        let mut estimator = test_estimator();
        estimator.estimate_folder(&root)?;

        assert_eq!(estimator.entries["Physics/Test.bphcl"], 320);
        assert_eq!(estimator.entries["Model/Test.bfres"], 21_280);
        assert_eq!(estimator.entries.len(), 2);
        assert!(estimator.entries.keys().all(|path| !path.contains('\\')));

        let json_path = root.join("exports/entries.json");
        let yaml_path = root.join("exports/entries.yaml");
        estimator.save_entries(&json_path)?;
        estimator.save_entries(&yaml_path)?;

        let json: BTreeMap<String, u32> = serde_json::from_slice(&fs::read(json_path)?)?;
        let yaml: BTreeMap<String, u32> = serde_yaml::from_slice(&fs::read(yaml_path)?)?;
        let expected: BTreeMap<String, u32> = estimator
            .entries
            .iter()
            .map(|(path, value)| (path.clone(), *value))
            .collect();
        assert_eq!(json, expected);
        assert_eq!(yaml, expected);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn folder_estimate_can_save_rstb_yaml_in_parent() -> TestResult {
        let parent = unique_temp_directory()?;
        let folder = parent.join("mod");
        fs::create_dir_all(folder.join("Physics"))?;
        fs::write(folder.join("Physics/Test.bphcl"), [0; 33])?;

        let mut estimator = test_estimator();
        let output = estimator.estimate_folder_and_save(&folder)?;

        assert_eq!(output, parent.join("rstb.yaml"));
        let yaml: BTreeMap<String, u32> = serde_yaml::from_slice(&fs::read(&output)?)?;
        assert_eq!(yaml["Physics/Test.bphcl"], 320);
        assert_eq!(yaml.len(), 1);

        fs::remove_dir_all(parent)?;
        Ok(())
    }

    #[test]
    fn sarc_members_use_full_paths_and_skip_vanilla_hash_matches() -> TestResult {
        let root = unique_temp_directory()?;
        fs::create_dir_all(root.join("Actor/Pack"))?;

        let vanilla_data = vec![0; 17];
        let changed_data = vec![0; 33];
        let compressed_data = zstd::stream::encode_all(&vec![0; 33][..], 1)?;
        let mut writer = SarcWriter::new(Endian::Little);
        writer.add_file("Physics/Vanilla.bphcl", vanilla_data.clone());
        writer.add_file("Physics/Changed.bphcl", changed_data);
        writer.add_file("Physics/Compressed.bphcl.zs", compressed_data);
        writer.add_file("Physics\\WindowsPath.bphcl", vec![0; 33]);
        fs::write(root.join("Actor/Pack/Test.pack"), writer.to_binary())?;

        let mut vanilla_hashes = HashMap::new();
        vanilla_hashes.insert("Physics/Vanilla.bphcl".to_owned(), sha256(vanilla_data));
        let mut estimator = test_estimator();
        estimator.vanilla_sarc_hashes = Some(Arc::new(vanilla_hashes));
        estimator.estimate_folder(&root)?;

        assert!(estimator.entries.contains_key("Actor/Pack/Test.pack"));
        assert_eq!(estimator.entries["Physics/Changed.bphcl"], 320);
        assert_eq!(estimator.entries["Physics/Compressed.bphcl"], 320);
        assert_eq!(estimator.entries["Physics/WindowsPath.bphcl"], 320);
        assert!(!estimator.entries.contains_key("Physics/Vanilla.bphcl"));
        assert!(estimator.entries.keys().all(|path| !path.contains('\\')));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn mals_archive_does_not_emit_member_entries() -> TestResult {
        let parent = unique_temp_directory()?;
        let clean = parent.join("clean");
        let output = parent.join("mod");
        fs::create_dir_all(clean.join("Mals"))?;
        fs::create_dir_all(output.join("Mals"))?;

        let mut vanilla = SarcWriter::new(Endian::Little);
        vanilla.add_file("ChallengeMsg/Unchanged.msbt", vec![1; 33]);
        vanilla.add_file("ChallengeMsg/Changed.msbt", vec![2; 33]);
        fs::write(clean.join("Mals/Test.sarc"), vanilla.to_binary())?;

        let mut modified = SarcWriter::new(Endian::Little);
        modified.add_file("ChallengeMsg/Unchanged.msbt", vec![1; 33]);
        modified.add_file("ChallengeMsg/Changed.msbt", vec![3; 33]);
        modified.add_file("ChallengeMsg/Custom.msbt", vec![4; 33]);
        fs::write(output.join("Mals/Test.sarc"), modified.to_binary())?;

        let mut estimator = test_estimator();
        estimator.set_vanilla_romfs(&clean);
        estimator.estimate_folder(&output)?;

        assert!(estimator.entries.contains_key("Mals/Test.sarc"));
        assert!(!estimator
            .entries
            .contains_key("ChallengeMsg/Unchanged.msbt"));
        assert!(!estimator.entries.contains_key("ChallengeMsg/Changed.msbt"));
        assert!(!estimator.entries.contains_key("ChallengeMsg/Custom.msbt"));

        fs::remove_dir_all(parent)?;
        Ok(())
    }

    #[test]
    fn exports_normalize_publicly_inserted_windows_paths() -> TestResult {
        let root = unique_temp_directory()?;
        fs::create_dir_all(&root)?;
        let output = root.join("entries.yaml");
        let mut estimator = test_estimator();
        estimator
            .entries
            .insert("Physics\\Manual.bphcl".into(), 320);

        estimator.save_yaml(&output)?;

        let yaml: BTreeMap<String, u32> = serde_yaml::from_slice(&fs::read(output)?)?;
        assert_eq!(yaml["Physics/Manual.bphcl"], 320);
        assert!(!yaml.keys().any(|path| path.contains('\\')));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn failed_folder_estimate_preserves_existing_entries() -> TestResult {
        let root = unique_temp_directory()?;
        fs::create_dir_all(root.join("AI"))?;
        fs::write(root.join("AI/Broken.ainb"), [0; 8])?;

        let mut estimator = test_estimator();
        estimator.entries.insert("Existing.bin".into(), 123);
        assert!(estimator.estimate_folder(&root).is_err());
        assert_eq!(estimator.entries.len(), 1);
        assert_eq!(estimator.entries["Existing.bin"], 123);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    fn unique_temp_directory() -> Result<PathBuf, std::time::SystemTimeError> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        Ok(std::env::temp_dir().join(format!(
            "totkbits-rstb-estimator-{}-{unique}",
            std::process::id()
        )))
    }
}
