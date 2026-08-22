//! Luigi's Mansion 3 `*.dict`/`*.data` archive reader and slot-model
//! geometry extraction. Ported from the validated Python research tooling in
//! `tmp/lm3_code` (`lm3_slot_swap.py` archive layer, `export_clean_slot_fbx.py`
//! mesh layer). Read-only: nothing here mutates archives.

use crate::file_format::Model3D::bfres::{BfresMesh, BfresRenderGraph};
use flate2::read::ZlibDecoder;
use serde::Serialize;
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::Path;
use std::sync::LazyLock;

const DICT_MAGIC: u32 = 0xA9F32458;
const MODEL_START: u16 = 0xB006;
const ENTRY_SIZE: usize = 16;
/// Chunk storage: the sub-entry table is file entry 0, model/material data is
/// entry 52, and vertex & index buffers are entry 54.
pub(crate) const TABLE_ENTRY: usize = 0;
pub(crate) const MODEL_DATA_ENTRY: usize = 52;
pub(crate) const BUFFER_ENTRY: usize = 54;
/// Texture headers live in file entry 63; texture image data in entry 65.
pub(crate) const TEXTURE_HEADER_ENTRY: usize = 63;
pub(crate) const TEXTURE_DATA_ENTRY: usize = 65;
/// Skeleton chunks (0x7101..=0x7106) are stored in file entry 53.
pub(crate) const SKELETON_ENTRY: usize = 53;

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Lm3ArchiveSpec {
    pub name: String,
    pub dict: String,
    pub slots: usize,
}

#[derive(serde::Deserialize)]
struct Lm3ArchiveFile {
    archives: Vec<Lm3ArchiveSpec>,
}

static ARCHIVES: LazyLock<Vec<Lm3ArchiveSpec>> = LazyLock::new(|| {
    serde_json::from_str::<Lm3ArchiveFile>(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/misc/lm3_archives.json"
    )))
    .map(|file| file.archives)
    .unwrap_or_default()
});

static SLOT_NAMES: LazyLock<HashMap<String, HashMap<String, String>>> = LazyLock::new(|| {
    serde_json::from_str::<HashMap<String, serde_json::Value>>(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/misc/lm3_slot_names.json"
    )))
    .map(|raw| {
        raw.into_iter()
            .filter_map(|(archive, value)| {
                let slots = serde_json::from_value::<HashMap<String, String>>(value).ok()?;
                Some((archive, slots))
            })
            .collect()
    })
    .unwrap_or_default()
});

pub fn archives() -> &'static [Lm3ArchiveSpec] {
    &ARCHIVES
}

pub fn archive_spec(name: &str) -> Option<&'static Lm3ArchiveSpec> {
    ARCHIVES.iter().find(|spec| spec.name == name)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lm3SlotEntry {
    pub id: String,
    pub archive: String,
    pub slot: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Browser catalog for every hardcoded slot of every archive that exists in
/// the configured LM3 romfs. Empty when the path holds no known archive.
pub fn slot_catalog(romfs: &Path) -> Vec<Lm3SlotEntry> {
    let mut entries = Vec::new();
    for spec in archives() {
        if !romfs.join(&spec.dict).is_file() {
            continue;
        }
        let names = SLOT_NAMES.get(&spec.name);
        for slot in 0..spec.slots {
            entries.push(Lm3SlotEntry {
                id: format!("{}_{}", spec.name, slot),
                archive: spec.name.clone(),
                slot,
                name: names.and_then(|map| map.get(&slot.to_string()).cloned()),
            });
        }
    }
    entries
}

#[derive(Clone, Copy, Debug)]
pub struct Lm3FileEntry {
    pub offset: u32,
    pub decompressed_size: u32,
    pub compressed_size: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Lm3SubEntry {
    pub kind: u16,
    pub flags: u16,
    pub size: u32,
    pub offset: u32,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn read_u16(data: &[u8], offset: usize) -> io::Result<u16> {
    data.get(offset..offset + 2)
        .map(|bytes| u16::from_le_bytes(bytes.try_into().unwrap()))
        .ok_or_else(|| invalid(format!("u16 read out of bounds at 0x{offset:X}")))
}

fn read_u32(data: &[u8], offset: usize) -> io::Result<u32> {
    data.get(offset..offset + 4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .ok_or_else(|| invalid(format!("u32 read out of bounds at 0x{offset:X}")))
}

fn read_f32(data: &[u8], offset: usize) -> io::Result<f32> {
    read_u32(data, offset).map(f32::from_bits)
}

pub struct Lm3Archive {
    pub data: Vec<u8>,
    pub entries: Vec<Lm3FileEntry>,
    pub compressed: bool,
}

impl Lm3Archive {
    pub fn open(dict_path: &Path) -> io::Result<Self> {
        let dict = std::fs::read(dict_path)?;
        if dict.len() < 16 {
            return Err(invalid("LM3 dictionary header is truncated"));
        }
        let magic = read_u32(&dict, 0)?;
        if magic != DICT_MAGIC {
            return Err(invalid(format!(
                "unexpected LM3 dictionary magic 0x{magic:08X}"
            )));
        }
        let compressed = dict[6] != 0;
        let file_count = dict[12] as usize;
        let chunk_count = dict[13] as usize;
        let table_offset = 16 + chunk_count * 24;
        let mut entries = Vec::with_capacity(file_count);
        for index in 0..file_count {
            let base = table_offset + index * ENTRY_SIZE;
            entries.push(Lm3FileEntry {
                offset: read_u32(&dict, base)?,
                decompressed_size: read_u32(&dict, base + 4)?,
                compressed_size: read_u32(&dict, base + 8)?,
            });
        }
        let data = std::fs::read(dict_path.with_extension("data"))?;
        Ok(Self {
            data,
            entries,
            compressed,
        })
    }

    pub fn entry_bytes(&self, index: usize) -> io::Result<Vec<u8>> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| invalid(format!("LM3 archive has no file entry {index}")))?;
        let stored = if self.compressed {
            entry.compressed_size
        } else {
            entry.decompressed_size
        } as usize;
        let start = entry.offset as usize;
        let raw = self
            .data
            .get(start..start + stored)
            .ok_or_else(|| invalid(format!("LM3 file entry {index} is out of bounds")))?;
        if !self.compressed {
            return Ok(raw.to_vec());
        }
        let mut decompressed = Vec::with_capacity(entry.decompressed_size as usize);
        if let Err(error) = ZlibDecoder::new(raw).read_to_end(&mut decompressed) {
            // Some shipped entries have a damaged tail (Scarescraper's texture
            // data stops about 95% in). Everything inflated before the break is
            // still valid, and bounds are checked at every use, so keep it
            // rather than losing the whole entry.
            if decompressed.is_empty() {
                return Err(invalid(format!("LM3 file entry {index}: {error}")));
            }
            println!(
                "[LM3] file entry {index} is truncated at {} of {} bytes: {error}",
                decompressed.len(),
                entry.decompressed_size
            );
        }
        Ok(decompressed)
    }
}

pub fn parse_subentries(table: &[u8]) -> Vec<Lm3SubEntry> {
    let mut position = 0;
    // Leading table records are 24 bytes and begin with 0x1301; the sub-entry
    // records that follow are 12 bytes each.
    while position + 2 <= table.len()
        && u16::from_le_bytes([table[position], table[position + 1]]) == 0x1301
    {
        position += 24;
    }
    let mut result = Vec::new();
    while position + 12 <= table.len() {
        result.push(Lm3SubEntry {
            kind: u16::from_le_bytes([table[position], table[position + 1]]),
            flags: u16::from_le_bytes([table[position + 2], table[position + 3]]),
            size: u32::from_le_bytes(table[position + 4..position + 8].try_into().unwrap()),
            offset: u32::from_le_bytes(table[position + 8..position + 12].try_into().unwrap()),
        });
        position += 12;
    }
    result
}

pub fn group_models(subentries: &[Lm3SubEntry]) -> Vec<Vec<Lm3SubEntry>> {
    let mut models: Vec<Vec<Lm3SubEntry>> = Vec::new();
    let mut collecting = false;
    for entry in subentries {
        if entry.kind == MODEL_START {
            models.push(Vec::new());
            collecting = true;
        }
        if collecting {
            if let Some(current) = models.last_mut() {
                current.push(*entry);
            }
        }
    }
    models
}

#[derive(Clone, Debug, Serialize)]
pub struct Lm3Section {
    pub signature: [u8; 4],
    pub name: String,
    pub offset: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Lm3Model {
    pub name: String,
    pub archive: String,
    pub slot: usize,
    pub sections: Vec<Lm3Section>,
    pub materials: Vec<crate::parser::AOC::g1m::G1mMaterial>,
    pub render: BfresRenderGraph,
    pub format: String,
}

fn find_chunk(model: &[Lm3SubEntry], kind: u16) -> io::Result<Lm3SubEntry> {
    model
        .iter()
        .find(|chunk| chunk.kind == kind)
        .copied()
        .ok_or_else(|| invalid(format!("slot has no 0x{kind:04X} chunk")))
}

/// LM3 texture headers store the GPU format at byte 12; `0x15`/`0x16` are the
/// BC5 normal-map formats, which never serve as base color.
fn astc_block(format: u8) -> Option<(usize, usize)> {
    match format {
        0x19 => Some((4, 4)),
        0x1A => Some((5, 4)),
        0x1B => Some((5, 5)),
        0x1C => Some((6, 5)),
        0x1D => Some((6, 6)),
        0x1E => Some((8, 5)),
        0x1F => Some((8, 6)),
        0x20 => Some((8, 8)),
        _ => None,
    }
}

/// Tegra GOB height heuristic validated by the Python extractor.
fn gob_height_log2(width: u32, height: u32, format: u8) -> u8 {
    let mut result: u32 = if width <= 256 || height <= 256 { 8 } else { 16 };
    if width <= 128 || height <= 128 {
        result = 4;
    }
    if width <= 64 || height <= 64 {
        result = 2;
    }
    if format == 0x1D {
        if width <= 64 || height <= 64 {
            result = 4;
        }
        if width <= 32 || height <= 32 {
            result = 2;
        }
        if width <= 32 && height <= 32 {
            result = 1;
        }
    }
    result.trailing_zeros() as u8
}

fn decode_lm3_texture(header: &[u8], data: &[u8]) -> io::Result<image::RgbaImage> {
    use crate::file_format::Image::switch_texture;
    let mut width = read_u16(header, 4)? as u32;
    let mut height = read_u16(header, 6)? as u32;
    let format = *header
        .get(12)
        .ok_or_else(|| invalid("LM3 texture header is truncated"))?;
    if let Some((block_width, block_height)) = astc_block(format) {
        return switch_texture::decode_astc(
            width,
            height,
            data,
            block_width,
            block_height,
            gob_height_log2(width, height, format),
        );
    }
    let image_format = match format {
        0x05 => image_dds::ImageFormat::Rgba8Unorm,
        0x11 => image_dds::ImageFormat::BC1RgbaUnorm,
        0x15 => image_dds::ImageFormat::BC5RgUnorm,
        0x16 => image_dds::ImageFormat::BC5RgSnorm,
        other => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("unsupported LM3 texture format 0x{other:02X}"),
            ))
        }
    };
    if matches!(format, 0x15 | 0x16) {
        // Some records keep the full-resolution header while only a lower mip
        // is resident in this archive. Select the largest mip that fits.
        while width > 1
            && height > 1
            && width.div_ceil(4) as usize * height.div_ceil(4) as usize * 16 > data.len()
        {
            width = (width / 2).max(1);
            height = (height / 2).max(1);
        }
    }
    switch_texture::decode(
        width,
        height,
        image_format,
        data,
        gob_height_log2(width, height, format),
        false,
    )
}

/// Pairs every B501 texture-header record with its adjacent B502 image-data
/// record, keyed by the texture hash at the start of the header.
fn texture_records(
    subentries: &[Lm3SubEntry],
    file63: &[u8],
) -> HashMap<u32, (Lm3SubEntry, Lm3SubEntry)> {
    let mut records = HashMap::new();
    for (index, entry) in subentries.iter().enumerate() {
        if entry.kind != 0xB501 {
            continue;
        }
        let Some(data_record) = subentries.get(index + 1).filter(|next| next.kind == 0xB502) else {
            continue;
        };
        if let Ok(texture_hash) = read_u32(file63, entry.offset as usize) {
            records.insert(texture_hash, (*entry, *data_record));
        }
    }
    records
}

/// Slots that share another slot's skeleton, from the research tooling's
/// `SHARED_SKELETON` table (Story Mode Luigi and the ScareScraper outfit).
const SHARED_SKELETON: [(usize, usize); 6] =
    [(25, 26), (26, 26), (27, 27), (28, 27), (29, 28), (30, 28)];

/// The 24-byte primary records that precede the sub-entry table. Only the
/// chunk kind (at byte 12) and its file-52 offset (at byte 8) are needed.
fn primary_records(table: &[u8]) -> Vec<(u16, u32)> {
    let mut records = Vec::new();
    let mut position = 0;
    while position + 24 <= table.len()
        && u16::from_le_bytes([table[position], table[position + 1]]) == 0x1301
    {
        if let (Ok(offset), Ok(kind)) = (
            read_u32(table, position + 8),
            read_u16(table, position + 12),
        ) {
            records.push((kind, offset));
        }
        position += 24;
    }
    records
}

/// Groups skeleton chunks the way `group_models` groups model chunks: a new
/// group starts at each 0x7101 record and runs through the 0x7101..=0x7106
/// records that follow it.
fn skeleton_groups(subentries: &[Lm3SubEntry]) -> Vec<Vec<Lm3SubEntry>> {
    let mut groups: Vec<Vec<Lm3SubEntry>> = Vec::new();
    for entry in subentries {
        if entry.kind == 0x7101 {
            groups.push(Vec::new());
        }
        if (0x7101..=0x7106).contains(&entry.kind) {
            if let Some(current) = groups.last_mut() {
                current.push(*entry);
            }
        }
    }
    groups
}

fn quaternion_multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

/// The skeleton is stored rotated 90 degrees about Z relative to the vertex
/// buffers, so a bone basis change is that rotation followed by the same Z-up
/// to Y-up conversion the vertices get. The combined map sends a bone-space
/// point `(x, y, z)` to `(y, z, x)`; its quaternion is `Rx(-90) * Rz(-90)`.
const BONE_BASIS: [f32; 4] = [-0.5, -0.5, -0.5, 0.5];
const BONE_BASIS_INVERSE: [f32; 4] = [0.5, 0.5, 0.5, 0.5];

fn rebase_translation(translation: [f32; 3]) -> [f32; 3] {
    [translation[1], translation[2], translation[0]]
}

/// Rebases a local bone rotation into the viewer's frame. A basis change of a
/// rotation is a conjugation, so the whole hierarchy composes correctly and
/// each bone's world position lands where the converted vertices expect it.
fn rebase_rotation(rotation: [f32; 4]) -> [f32; 4] {
    quaternion_multiply(
        quaternion_multiply(BONE_BASIS, rotation),
        BONE_BASIS_INVERSE,
    )
}

/// Reads one skeleton group: local bone transforms (0x7103), the parent table
/// (0x7106), and the bone-hash to bone-id map (0x7105) that mesh skin records
/// resolve through.
fn read_skeleton(
    file53: &[u8],
    group: &[Lm3SubEntry],
) -> Option<(
    Vec<crate::file_format::Model3D::bfres::BfresBone>,
    HashMap<u32, u32>,
)> {
    let by_kind: HashMap<u16, Lm3SubEntry> =
        group.iter().map(|entry| (entry.kind, *entry)).collect();
    let transform = by_kind.get(&0x7103)?;
    let parent = by_kind.get(&0x7106)?;
    let bone_count = transform.size as usize / 0x1C;
    let mut bones = Vec::with_capacity(bone_count);
    for index in 0..bone_count {
        let base = transform.offset as usize + index * 0x1C;
        let rotation = rebase_rotation([
            read_f32(file53, base).ok()?,
            read_f32(file53, base + 4).ok()?,
            read_f32(file53, base + 8).ok()?,
            read_f32(file53, base + 12).ok()?,
        ]);
        let translation = rebase_translation([
            read_f32(file53, base + 16).ok()?,
            read_f32(file53, base + 20).ok()?,
            read_f32(file53, base + 24).ok()?,
        ]);
        // 0xFFFF marks a root bone in this table, not a signed -1 index.
        let parent_index = match read_u16(file53, parent.offset as usize + index * 2).ok()? {
            0xFFFF => -1,
            value => value as i16,
        };
        bones.push(crate::file_format::Model3D::bfres::BfresBone {
            name: format!("bone_{index}"),
            parent_index,
            smooth_matrix_index: -1,
            rigid_matrix_index: -1,
            rotation_mode: "quaternion".into(),
            scale: [1.0, 1.0, 1.0],
            rotation,
            translation,
        });
    }
    let mut hash_to_id = HashMap::new();
    if let Some(map_record) = by_kind.get(&0x7105) {
        let start = map_record.offset as usize;
        for offset in (start..start + map_record.size as usize).step_by(8) {
            if let (Ok(hash), Ok(id)) = (read_u32(file53, offset), read_u32(file53, offset + 4)) {
                hash_to_id.insert(hash, id);
            }
        }
    }
    Some((bones, hash_to_id))
}

/// Picks the skeleton group belonging to a model slot: by the model/skeleton
/// hash pairing first, then the known shared-skeleton table, then by scoring
/// how many of the slot's B103 bone hashes each candidate group provides.
fn select_skeleton_group(
    file52: &[u8],
    file53: &[u8],
    table: &[u8],
    subentries: &[Lm3SubEntry],
    model: &[Lm3SubEntry],
    slot: usize,
) -> Option<usize> {
    let groups = skeleton_groups(subentries);
    if groups.is_empty() {
        return None;
    }
    let primary = primary_records(table);
    let model_headers: Vec<u32> = primary
        .iter()
        .filter(|(kind, _)| *kind == 0xB000)
        .map(|(_, offset)| *offset)
        .collect();
    let skeleton_headers: Vec<u32> = primary
        .iter()
        .filter(|(kind, _)| *kind == 0x7100)
        .map(|(_, offset)| *offset)
        .collect();
    if let Some(model_offset) = model_headers.get(slot) {
        if let Ok(model_hash) = read_u32(file52, *model_offset as usize + 4) {
            let paired = skeleton_headers.iter().position(|offset| {
                read_u32(file52, *offset as usize + 4).is_ok_and(|hash| hash == model_hash)
            });
            if let Some(index) = paired.filter(|index| *index < groups.len()) {
                return Some(index);
            }
        }
    }
    if let Some((_, shared)) = SHARED_SKELETON.iter().find(|(from, _)| *from == slot) {
        if *shared < groups.len() {
            return Some(*shared);
        }
    }
    // Fall back to the group that supplies the most of this slot's bone hashes.
    let b103 = find_chunk(model, 0xB103).ok()?;
    let required: std::collections::HashSet<u32> = (b103.offset..b103.offset + b103.size)
        .step_by(4)
        .filter_map(|offset| read_u32(file52, offset as usize).ok())
        .collect();
    if required.is_empty() {
        return None;
    }
    groups
        .iter()
        .enumerate()
        .filter_map(|(index, group)| {
            let mapping = group.iter().find(|entry| entry.kind == 0x7105)?;
            let start = mapping.offset as usize;
            let matched = (start..start + mapping.size as usize)
                .step_by(8)
                .filter_map(|offset| read_u32(file53, offset).ok())
                .filter(|hash| required.contains(hash))
                .count();
            (matched > 0).then_some((matched, index))
        })
        .max()
        .map(|(_, index)| index)
}

/// A texture store: the header/data entries of one archive plus the lookup
/// tables built from them. Scarescraper materials reference textures that live
/// in `global`, so a slot resolves against its own store first and then a
/// fallback one.
pub(crate) struct Lm3TextureSource {
    pub formats: HashMap<u32, u8>,
    pub records: HashMap<u32, (Lm3SubEntry, Lm3SubEntry)>,
    pub file63: Vec<u8>,
    pub file65: Vec<u8>,
}

impl Lm3TextureSource {
    pub(crate) fn new(subentries: &[Lm3SubEntry], file63: Vec<u8>, file65: Vec<u8>) -> Self {
        Self {
            formats: texture_formats_from_headers(subentries, &file63),
            records: texture_records(subentries, &file63),
            file63,
            file65,
        }
    }

    /// Reads every texture entry of an archive and indexes it.
    pub(crate) fn from_archive(archive: &Lm3Archive) -> Option<Self> {
        let file63 = archive.entry_bytes(TEXTURE_HEADER_ENTRY).ok()?;
        let file65 = archive.entry_bytes(TEXTURE_DATA_ENTRY).ok()?;
        let table = archive.entry_bytes(TABLE_ENTRY).ok()?;
        Some(Self::new(&parse_subentries(&table), file63, file65))
    }

    fn decode(&self, hash: u32) -> Option<io::Result<image::RgbaImage>> {
        let (header_record, data_record) = self.records.get(&hash)?;
        let header_start = header_record.offset as usize;
        let data_start = data_record.offset as usize;
        let header = self
            .file63
            .get(header_start..header_start + header_record.size as usize)?;
        let data = self
            .file65
            .get(data_start..data_start + data_record.size as usize)?;
        Some(decode_lm3_texture(header, data))
    }
}

/// Classifies every B501 texture header by its GPU format byte. This must not
/// depend on a paired B502 image record: a material can reference a texture
/// whose image data lives elsewhere, and that reference still has to be
/// classified so the diffuse selection can reject BC5 normal maps.
fn texture_formats_from_headers(subentries: &[Lm3SubEntry], file63: &[u8]) -> HashMap<u32, u8> {
    let mut formats = HashMap::new();
    for entry in subentries.iter().filter(|entry| entry.kind == 0xB501) {
        let offset = entry.offset as usize;
        if let (Ok(hash), Some(format)) = (read_u32(file63, offset), file63.get(offset + 12)) {
            formats.insert(hash, *format);
        }
    }
    formats
}

/// One texture reference inside a material: `[u32 hash][u32 flags][u32 tag]`.
/// Bit 31 of `flags` marks a colour (sRGB) texture; linear data maps such as
/// normal, roughness and specular have it clear.
#[derive(Clone, Copy, Debug)]
pub struct Lm3TextureRef {
    pub hash: u32,
    pub flags: u32,
    pub format: u8,
    /// False when this archive has no header for the hash. Such records still
    /// belong to the material and must not split its run, but they cannot be
    /// classified or decoded.
    pub known: bool,
}

impl Lm3TextureRef {
    fn is_srgb(&self) -> bool {
        self.flags & 0x8000_0000 != 0
    }
    fn is_normal_map(&self) -> bool {
        matches!(self.format, 0x15 | 0x16)
    }
}

/// A texture record's flags always carry this pattern; only bit 31 (sRGB)
/// varies. Checking it keeps unrelated data from being read as a record.
const TEXTURE_FLAG_MASK: u32 = 0x7FFF_FFFF;
const TEXTURE_FLAG_VALUE: u32 = 0x0180_0000;

fn texture_record_at(
    material: &[u8],
    offset: usize,
    texture_formats: &HashMap<u32, u8>,
) -> Option<Lm3TextureRef> {
    if offset + 12 > material.len() {
        return None;
    }
    let hash = read_u32(material, offset).ok()?;
    let flags = read_u32(material, offset + 4).ok()?;
    if flags & TEXTURE_FLAG_MASK != TEXTURE_FLAG_VALUE {
        return None;
    }
    let format = texture_formats.get(&hash);
    Some(Lm3TextureRef {
        hash,
        flags,
        format: format.copied().unwrap_or_default(),
        known: format.is_some(),
    })
}

/// A record that also names a texture this archive has a header for. Runs are
/// only *started* at one of these, so unrelated data cannot look like a
/// material, while unknown records inside a run keep it intact.
fn known_texture_record_at(
    material: &[u8],
    offset: usize,
    texture_formats: &HashMap<u32, u8>,
) -> Option<Lm3TextureRef> {
    texture_record_at(material, offset, texture_formats).filter(|entry| entry.known)
}

/// Reads the maximal run of texture records beginning exactly at `start`.
fn texture_run_at(
    material: &[u8],
    start: usize,
    texture_formats: &HashMap<u32, u8>,
) -> Vec<Lm3TextureRef> {
    let mut run = Vec::new();
    let mut position = start;
    while let Some(entry) = texture_record_at(material, position, texture_formats) {
        run.push(entry);
        position += 12;
    }
    run
}

/// Reads every texture record a material declares. A material is not one flat
/// run: groups of records are separated by blocks of float parameters, so the
/// whole extent up to `end` is swept and each record taken wherever it sits.
/// Records naming textures this archive has no header for are kept, since the
/// Scarescraper materials reference textures stored in `global`.
fn read_material_records(
    material: &[u8],
    pointer: usize,
    end: usize,
    texture_formats: &HashMap<u32, u8>,
) -> Vec<Lm3TextureRef> {
    let mut records = Vec::new();
    let mut position = pointer;
    let end = end.min(material.len());
    while position + 12 <= end {
        match texture_record_at(material, position, texture_formats) {
            Some(entry) => {
                records.push(entry);
                position += 12;
            }
            None => position += 4,
        }
    }
    records
}

/// Every material begins 8 bytes before its first texture record, and the
/// materials of one slot are laid out consecutively. The smallest gap observed
/// between two of them is 0xB8, so requiring this much separation rejects the
/// later record groups inside a material (they follow parameter blocks) while
/// still admitting a genuine neighbour.
const MINIMUM_MATERIAL_SPACING: usize = 0xB0;

/// Finds the material starts inside `window` that no mesh pointer claimed.
/// Meshes whose B007 record carries no pointer still own a material in this
/// region; these are the candidates they are matched against. The known
/// pointers seed the search so a spurious candidate cannot displace them.
fn unclaimed_material_starts(
    material: &[u8],
    window: std::ops::Range<usize>,
    claimed: &std::collections::HashSet<usize>,
    texture_formats: &HashMap<u32, u8>,
) -> Vec<usize> {
    let mut accepted: Vec<usize> = claimed.iter().copied().collect();
    accepted.sort_unstable();
    let mut found = Vec::new();
    let mut position = window.start - window.start % 8;
    while position + 20 <= window.end.min(material.len()) {
        if known_texture_record_at(material, position + 8, texture_formats).is_some()
            && !claimed.contains(&position)
            && accepted
                .iter()
                .all(|start| position.abs_diff(*start) >= MINIMUM_MATERIAL_SPACING)
        {
            accepted.push(position);
            accepted.sort_unstable();
            found.push(position);
        }
        position += 8;
    }
    found.sort_unstable();
    found
}

/// Picks the base-colour record of a material. Verified against slot 27, where
/// mesh 12 references a packed roughness/specular map (ASTC 6x6) before its
/// real albedo and both are flagged sRGB.
fn base_color_index(references: &[Lm3TextureRef]) -> Option<usize> {
    // A material's data maps are recognisable by format: BC5 (0x15/0x16) is
    // the normal map and ASTC 6x6 (0x1D) the packed roughness/specular map.
    // Excluding both leaves the colour textures, of which the first sRGB one
    // -- or simply the first, when the slot flags none -- is the base colour.
    let colour =
        |entry: &Lm3TextureRef| entry.known && !entry.is_normal_map() && entry.format != 0x1D;
    references
        .iter()
        .position(|entry| colour(entry) && entry.is_srgb())
        .or_else(|| references.iter().position(colour))
        .or_else(|| {
            references
                .iter()
                .position(|entry| entry.known && entry.is_srgb())
        })
        .or_else(|| {
            references
                .iter()
                .position(|entry| entry.known && !entry.is_normal_map())
        })
}

/// Splits B007 into its per-mesh records. Each record ends with a 28-byte
/// sentinel, and its last word before that sentinel is the material's offset
/// inside B006 (the texture records begin 8 bytes further in).
fn material_offsets(file52: &[u8], b007: Lm3SubEntry, material_size: u32) -> Vec<Option<u32>> {
    const SENTINEL: [u8; 28] = [
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00,
    ];
    let start = b007.offset as usize;
    let end = (start + b007.size as usize).min(file52.len());
    let mut offsets = Vec::new();
    let mut record_start = start;
    let mut position = start;
    while position + 28 <= end {
        if file52[position..position + 28] == SENTINEL {
            // The material pointer is the last word of the record.
            let pointer = position
                .checked_sub(4)
                .filter(|at| *at >= record_start)
                .and_then(|at| read_u32(file52, at).ok())
                .filter(|value| *value + 8 < material_size);
            offsets.push(pointer);
            position += 28;
            record_start = position;
        } else {
            position += 4;
        }
    }

    // One slot's materials are stored together, so its pointers cluster. A
    // record for a mesh that owns no material ends with a small constant
    // instead; in a large shared material chunk that value sits far below the
    // cluster and is rejected here, while in a small per-slot chunk every
    // offset -- including zero -- is legitimately near the others.
    const CLUSTER_SPAN: u32 = 0x8000;
    let highest = offsets.iter().flatten().copied().max().unwrap_or(0);
    for pointer in &mut offsets {
        if pointer.is_some_and(|value| highest - value > CLUSTER_SPAN) {
            *pointer = None;
        }
    }
    offsets
}

/// Resolves each mesh hash to the ordered list of texture references its
/// material declares. Meshes without a resolvable material are absent.
fn material_textures(
    file52: &[u8],
    model: &[Lm3SubEntry],
    mesh_hashes: &[u32],
    texture_formats: &HashMap<u32, u8>,
) -> HashMap<u32, Vec<Lm3TextureRef>> {
    let mut result = HashMap::new();
    let (Ok(b006), Ok(b007)) = (find_chunk(model, 0xB006), find_chunk(model, 0xB007)) else {
        return result;
    };
    let start = b006.offset as usize;
    let material = &file52[start.min(file52.len())..(start + b006.size as usize).min(file52.len())];
    let offsets = material_offsets(file52, b007, b006.size);
    // A material runs until the next one begins; without a following pointer
    // this bounds how far a sweep may read.
    const MATERIAL_MAX: usize = 0x400;
    let mut boundaries: Vec<usize> = offsets
        .iter()
        .filter_map(|offset| offset.map(|value| value as usize))
        .collect();
    boundaries.sort_unstable();
    boundaries.dedup();
    let extent_of = |start: usize| {
        boundaries
            .iter()
            .find(|boundary| **boundary > start)
            .copied()
            .unwrap_or(usize::MAX)
            .min(start + MATERIAL_MAX)
    };

    let mut claimed = std::collections::HashSet::new();
    let mut unassigned = Vec::new();
    for (index, mesh_hash) in mesh_hashes.iter().enumerate() {
        let start = offsets.get(index).copied().flatten().map(|v| v as usize);
        let records = start
            .map(|start| read_material_records(material, start, extent_of(start), texture_formats));
        match (start, records) {
            (Some(start), Some(records)) if !records.is_empty() => {
                claimed.insert(start);
                result.insert(*mesh_hash, records);
            }
            _ => unassigned.push(index),
        }
    }

    // Meshes whose B007 record holds no pointer still own a material stored
    // beside the ones that do. Match them against the unclaimed runs in that
    // region: within each block of neighbouring meshes the runs ascend as the
    // mesh index descends (verified against slot 27, where this recovers the
    // materials of meshes 5, 6, 7, 12 and 14).
    if !unassigned.is_empty() && !claimed.is_empty() {
        const SEARCH_BEHIND: usize = 0x8000;
        let lowest = claimed.iter().min().copied().unwrap_or(0);
        let highest = claimed.iter().max().copied().unwrap_or(0);
        let window = lowest.saturating_sub(SEARCH_BEHIND)..highest.min(material.len());
        let found = unclaimed_material_starts(material, window, &claimed, texture_formats);
        // A slot's materials sit together, so take the unclaimed ones between
        // the claimed materials first, then the nearest ones below that
        // cluster. Reaching further would pull in another slot's data.
        let (below, interior): (Vec<usize>, Vec<usize>) =
            found.into_iter().partition(|start| *start < lowest);
        let wanted = unassigned.len().saturating_sub(interior.len());
        let mut chosen = interior;
        chosen.extend(below.iter().rev().take(wanted).copied());
        chosen.sort_unstable();
        let mut candidates = chosen.into_iter();
        // A single claimed mesh between two unassigned ones does not start a
        // new block; a wider gap does.
        let mut blocks: Vec<Vec<usize>> = Vec::new();
        for index in unassigned {
            match blocks.last_mut() {
                Some(block) if index - *block.last().unwrap() <= 2 => block.push(index),
                _ => blocks.push(vec![index]),
            }
        }
        for block in blocks {
            for index in block.iter().rev() {
                let Some(start) = candidates.next() else {
                    break;
                };
                let run = read_material_records(
                    material,
                    start,
                    extent_of(start).min(start + MATERIAL_MAX),
                    texture_formats,
                );
                if !run.is_empty() {
                    result.insert(mesh_hashes[*index], run);
                }
            }
        }
    }

    if result.len() != mesh_hashes.len() {
        println!(
            "[LM3] resolved materials for {} of {} meshes",
            result.len(),
            mesh_hashes.len()
        );
    }
    result
}

#[allow(dead_code)]
fn legacy_material_textures(
    file52: &[u8],
    model: &[Lm3SubEntry],
    mesh_hashes: &[u32],
    texture_formats: &HashMap<u32, u8>,
) -> HashMap<u32, Vec<u32>> {
    const MARKER: [u8; 28] = [
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00,
    ];
    let mut result = HashMap::new();
    let (Ok(b006), Ok(b007)) = (find_chunk(model, 0xB006), find_chunk(model, 0xB007)) else {
        return result;
    };
    let mut bindings = Vec::with_capacity(mesh_hashes.len());
    let mut position = b007.offset as usize;
    let end = (b007.offset + b007.size) as usize;
    while bindings.len() < mesh_hashes.len() && position + 28 <= end.min(file52.len()) {
        if file52[position..position + 28] == MARKER {
            // The binding pointer sits in the u32 before the marker. Push it
            // unconditionally so the count still matches the mesh count, as in
            // the reference extractor; a nonsensical value simply resolves to
            // no diffuse texture below.
            let offset = position
                .checked_sub(4)
                .and_then(|at| read_u32(file52, at).ok())
                .map(|value| value.saturating_sub(8) as usize)
                .unwrap_or(0);
            bindings.push(offset);
        }
        position += 4;
    }
    if bindings.len() != mesh_hashes.len() {
        println!(
            "[LM3] material bindings unresolved: found {} markers for {} meshes",
            bindings.len(),
            mesh_hashes.len()
        );
        return result;
    }
    let mut boundaries: Vec<usize> = bindings.clone();
    boundaries.push(b006.size as usize);
    boundaries.sort_unstable();
    boundaries.dedup();
    for (mesh_hash, relative) in mesh_hashes.iter().zip(&bindings) {
        let Some(size) = boundaries
            .iter()
            .find(|boundary| **boundary > *relative)
            .map(|boundary| boundary - relative)
        else {
            continue;
        };
        let start = b006.offset as usize + relative;
        let Some(payload) = file52.get(start..(start + size).min(file52.len())) else {
            continue;
        };
        let mut references = Vec::new();
        for offset in (0..payload.len().saturating_sub(3)).step_by(4) {
            let value = u32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap());
            if texture_formats.contains_key(&value) && !references.contains(&value) {
                references.push(value);
            }
        }
        if !references.is_empty() {
            result.insert(*mesh_hash, references);
        }
    }
    result
}

/// Extracts a slot's geometry (positions, normals, UVs, triangle indices) and
/// its diffuse textures as the render graph the 3D viewport consumes.
/// Skinning and bones are intentionally not resolved yet. LM3 models are
/// Z-up; positions and normals are converted to the viewer's Y-up frame.
/// The decompressed archive entries a slot needs. Splitting these out lets the
/// sequential and multithreaded readers share one parser.
pub(crate) struct Lm3SlotFiles {
    pub table: Vec<u8>,
    pub file52: Vec<u8>,
    pub file53: Option<Vec<u8>>,
    pub file54: Vec<u8>,
    pub file63: Option<Vec<u8>>,
    pub file65: Option<Vec<u8>>,
}

impl Lm3SlotFiles {
    /// Reads every entry on the calling thread, in order.
    pub(crate) fn read(archive: &Lm3Archive) -> io::Result<Self> {
        Ok(Self {
            table: archive.entry_bytes(TABLE_ENTRY)?,
            file52: archive.entry_bytes(MODEL_DATA_ENTRY)?,
            file54: archive.entry_bytes(BUFFER_ENTRY)?,
            // These three are optional: a slot still previews without them.
            file53: archive.entry_bytes(SKELETON_ENTRY).ok(),
            file63: archive
                .entry_bytes(TEXTURE_HEADER_ENTRY)
                .map_err(|error| println!("[LM3] texture headers (entry 63) unavailable: {error}"))
                .ok(),
            file65: archive
                .entry_bytes(TEXTURE_DATA_ENTRY)
                .map_err(|error| println!("[LM3] texture data (entry 65) unavailable: {error}"))
                .ok(),
        })
    }
}

/// Opens the archive that holds the textures other archives share. Only
/// `global` stores them, so it is the fallback for every other archive and
/// needs no fallback of its own.
pub(crate) fn shared_texture_source(
    dict_path: &Path,
    archive_name: &str,
) -> Option<Lm3TextureSource> {
    if archive_name == "global" {
        return None;
    }
    let global = dict_path
        .parent()
        .map(|parent| parent.join("global.dict"))
        .filter(|path| path.is_file())
        .or_else(|| {
            let path = dict_path.parent()?.parent()?.join("global.dict");
            path.is_file().then_some(path)
        })?;
    Lm3TextureSource::from_archive(&Lm3Archive::open(&global).ok()?)
}

pub fn parse_slot(
    dict_path: &Path,
    archive_name: &str,
    slot: usize,
) -> io::Result<(Lm3Model, Vec<crate::parser::AOC::g1m::ResolvedG1tTexture>)> {
    let archive = Lm3Archive::open(dict_path)?;
    let files = Lm3SlotFiles::read(&archive)?;
    let fallback = shared_texture_source(dict_path, archive_name);
    parse_slot_from_files(&files, archive_name, slot, fallback.as_ref())
}

/// Builds a slot's model from already-decompressed archive entries.
pub(crate) fn parse_slot_from_files(
    files: &Lm3SlotFiles,
    archive_name: &str,
    slot: usize,
    fallback: Option<&Lm3TextureSource>,
) -> io::Result<(Lm3Model, Vec<crate::parser::AOC::g1m::ResolvedG1tTexture>)> {
    let table = &files.table;
    let file52 = &files.file52;
    let file54 = &files.file54;
    let subentries = parse_subentries(table);
    let models = group_models(&subentries);
    let model = models
        .get(slot)
        .ok_or_else(|| invalid(format!("archive has {} model slots", models.len())))?;
    let b003 = find_chunk(model, 0xB003)?;
    let b004 = find_chunk(model, 0xB004)?;
    let b005 = find_chunk(model, 0xB005)?;

    // Texture resolution is best-effort and each stage degrades on its own:
    // geometry must still load when the texture entries are absent, and the
    // material classification must still work when only the headers are
    // readable.
    let local = match (files.file63.clone(), files.file65.clone()) {
        (Some(file63), Some(file65)) => Some(Lm3TextureSource::new(&subentries, file63, file65)),
        (Some(file63), None) => Some(Lm3TextureSource::new(&subentries, file63, Vec::new())),
        _ => None,
    };
    // Classification needs a format for every referenced hash, including the
    // ones this archive only borrows.
    let mut texture_formats: HashMap<u32, u8> = local
        .as_ref()
        .map(|s| s.formats.clone())
        .unwrap_or_default();
    if let Some(fallback) = fallback {
        for (hash, format) in &fallback.formats {
            texture_formats.entry(*hash).or_insert(*format);
        }
    }

    // The skeleton lives in its own file entry and is optional: a slot still
    // previews as a static model when no compatible skeleton is found.
    let file53 = files.file53.clone();
    let (bones, bone_hash_to_id) = file53
        .as_deref()
        .and_then(|file53| {
            let index = select_skeleton_group(&file52, file53, &table, &subentries, model, slot)?;
            let group = skeleton_groups(&subentries).into_iter().nth(index)?;
            read_skeleton(file53, &group)
        })
        .unwrap_or_default();
    // Local bone ids in skin records index this slot's B103 hash table, which
    // is then mapped through the skeleton's own hash-to-id table.
    let b103_hashes: Vec<u32> = find_chunk(model, 0xB103)
        .ok()
        .map(|b103| {
            (b103.offset..b103.offset + b103.size)
                .step_by(4)
                .filter_map(|offset| read_u32(&file52, offset as usize).ok())
                .collect()
        })
        .unwrap_or_default();

    let mesh_count = b003.size as usize / 0x40;
    let mesh_hashes: Vec<u32> = (0..mesh_count)
        .map(|index| read_u32(&file52, b003.offset as usize + index * 0x40))
        .collect::<io::Result<_>>()?;
    let textures_by_mesh = material_textures(&file52, model, &mesh_hashes, &texture_formats);

    let mut meshes = Vec::with_capacity(mesh_count);
    let mut materials = Vec::with_capacity(mesh_count);
    let mut b004_cursor = b004.offset as usize;
    for mesh_index in 0..mesh_count {
        let descriptor = b003.offset as usize + mesh_index * 0x40;
        let mesh_hash = mesh_hashes[mesh_index];
        let index_offset = read_u32(&file52, descriptor + 4)? as usize;
        let index_flags = read_u32(&file52, descriptor + 8)?;
        let vertex_count = read_u32(&file52, descriptor + 12)? as usize;
        let skinned = read_u32(&file52, descriptor + 0x28)? != 0xFFFF_FFFF;
        // Skinned meshes store `u32 skin_offset` then `u32 vertex_offset`;
        // reversing those two fields explodes the geometry.
        let (skin_offset, vertex_offset) = if skinned {
            let skin = read_u32(&file52, b004_cursor)? as usize;
            let vertex = read_u32(&file52, b004_cursor + 4)? as usize;
            b004_cursor += 16;
            (Some(skin), vertex)
        } else {
            let vertex = read_u32(&file52, b004_cursor)? as usize;
            b004_cursor += 12;
            (None, vertex)
        };

        let index_count = (index_flags & 0xFF_FFFF) as usize;
        let index_width = if index_flags >> 24 == 0x80 { 1 } else { 2 };
        let index_base = b005.offset as usize + index_offset;
        let mut indices = Vec::with_capacity(index_count);
        for index in 0..index_count {
            let position = index_base + index * index_width;
            indices.push(if index_width == 1 {
                *file54
                    .get(position)
                    .ok_or_else(|| invalid("index buffer read out of bounds"))?
                    as u32
            } else {
                read_u16(&file54, position)? as u32
            });
        }

        let vertex_base = b005.offset as usize + vertex_offset;
        let mut positions = Vec::with_capacity(vertex_count);
        let mut normals = Vec::with_capacity(vertex_count);
        let mut uv0 = Vec::with_capacity(vertex_count);
        for vertex in 0..vertex_count {
            let record = vertex_base + vertex * 0x30;
            // Z-up to the viewer's Y-up: (x, y, z) -> (x, z, -y).
            let (x, y, z) = (
                read_f32(&file54, record)?,
                read_f32(&file54, record + 4)?,
                read_f32(&file54, record + 8)?,
            );
            positions.push([x, z, -y]);
            let u = read_f32(&file54, record + 0x0C)?;
            let (nx, ny, nz) = (
                read_f32(&file54, record + 0x10)?,
                read_f32(&file54, record + 0x14)?,
                read_f32(&file54, record + 0x18)?,
            );
            normals.push([nx, nz, -ny]);
            let v = read_f32(&file54, record + 0x1C)?;
            // The game's V is top-down, which is how the viewer samples its
            // flipY=false textures; the Blender exporter's 1-v flip does not
            // apply here.
            uv0.push([u, v]);
        }

        // Per-vertex skin records: four local bone ids followed by four
        // weights, resolved through the slot's B103 table into skeleton ids.
        let mut bone_indices = vec![[0u16; 4]; positions.len()];
        let mut bone_weights = vec![[0.0f32; 4]; positions.len()];
        let mut skin_bones: Vec<u16> = Vec::new();
        let mut vertex_skin_count = 0u8;
        if let Some(skin_offset) = skin_offset.filter(|_| !bone_hash_to_id.is_empty()) {
            let skin_base = b005.offset as usize + skin_offset;
            for vertex in 0..positions.len() {
                let record = skin_base + vertex * 0x14;
                let Some(ids) = file54.get(record..record + 4) else {
                    break;
                };
                let mut influence = 0;
                for slot_index in 0..4 {
                    let weight = read_f32(&file54, record + 4 + slot_index * 4).unwrap_or(0.0);
                    let local_id = ids[slot_index] as usize;
                    if weight <= 0.0 || local_id >= b103_hashes.len() {
                        continue;
                    }
                    let Some(bone_id) = bone_hash_to_id.get(&b103_hashes[local_id]) else {
                        continue;
                    };
                    let bone_id = *bone_id as u16;
                    bone_indices[vertex][influence] = bone_id;
                    bone_weights[vertex][influence] = weight;
                    if !skin_bones.contains(&bone_id) {
                        skin_bones.push(bone_id);
                    }
                    influence += 1;
                    if influence == 4 {
                        break;
                    }
                }
                // Normalize so partially resolved influences cannot shrink a
                // vertex toward the origin: the viewer divides by the weight
                // sum, and a vertex with no usable influence must stay put.
                let total: f32 = bone_weights[vertex].iter().sum();
                if influence > 0 && total > 0.0 {
                    for weight in &mut bone_weights[vertex] {
                        *weight /= total;
                    }
                } else {
                    bone_indices[vertex] = [0; 4];
                    bone_weights[vertex] = [0.0; 4];
                }
                vertex_skin_count = vertex_skin_count.max(influence as u8);
            }
            skin_bones.sort_unstable();
        }

        // Texture roles. BC5 records are normal maps. The base colour is the
        // first sRGB-flagged record, skipping ASTC 6x6 (0x1D): those carry the
        // packed roughness/specular map, which is also flagged sRGB and would
        // otherwise win merely by appearing first.
        let references = textures_by_mesh
            .get(&mesh_hash)
            .cloned()
            .unwrap_or_default();
        let base_color = base_color_index(&references);
        let mut texture_slots = Vec::new();
        for (index, entry) in references.iter().enumerate() {
            // Records naming a texture stored in another archive keep the run
            // together but cannot be classified or decoded here.
            if !entry.known {
                continue;
            }
            let (texture_type, sampler) = if Some(index) == base_color {
                ("Base color", "_a0")
            } else if entry.is_normal_map() {
                ("Normal", "_n0")
            } else if entry.is_srgb() {
                ("Specular", "_s0")
            } else {
                ("Texture", "_x0")
            };
            texture_slots.push(crate::parser::AOC::g1m::G1mTextureSlot {
                index,
                name: format!("{:08X}", entry.hash),
                uv_layer: 0,
                sampler: sampler.into(),
                texture_type: texture_type.into(),
            });
        }
        materials.push(crate::parser::AOC::g1m::G1mMaterial {
            name: format!("Material {mesh_index}"),
            offset: mesh_index as u64,
            texture_slots,
        });

        meshes.push(BfresMesh {
            name: format!("slot_{slot}_mesh_{mesh_index:02}_{mesh_hash:08X}"),
            material_index: mesh_index as u16,
            bone_index: 0,
            vertex_skin_count,
            is_cloth: false,
            cloth_id: 0,
            nun_id: 0,
            uv_maps: vec![uv0.clone()],
            uv0,
            colors: vec![[1.0; 4]; positions.len()],
            bone_indices,
            bone_weights,
            normals,
            positions,
            indices,
            skin_bones,
        });
    }

    let mut referenced: Vec<u32> = textures_by_mesh
        .values()
        .flatten()
        .filter(|entry| entry.known)
        .map(|entry| entry.hash)
        .collect();
    referenced.sort_unstable();
    referenced.dedup();
    println!(
        "[LM3] slot {archive_name}_{slot}: {mesh_count} meshes, {} bones, {} texture headers, {} referenced textures",
        bones.len(),
        texture_formats.len(),
        referenced.len()
    );

    let mut resolved_textures = Vec::new();
    {
        use base64::Engine;
        for texture_hash in referenced {
            // Resolve against this archive first, then the fallback store: a
            // Scarescraper costume shares several textures with `global`.
            let decoded = local
                .as_ref()
                .and_then(|source| source.decode(texture_hash))
                .or_else(|| fallback.and_then(|source| source.decode(texture_hash)));
            let image = match decoded {
                Some(Ok(image)) => image,
                Some(Err(error)) => {
                    println!("[LM3] skipping texture {texture_hash:08X}: {error}");
                    continue;
                }
                None => {
                    println!("[LM3] texture {texture_hash:08X} is not stored in a known archive");
                    continue;
                }
            };
            let (width, height) = (image.width(), image.height());
            let Ok(png) = crate::file_format::Image::png::encode(&image) else {
                continue;
            };
            let data_url = format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            );
            resolved_textures.push(crate::parser::AOC::g1m::ResolvedG1tTexture {
                name: format!("{texture_hash:08X}"),
                aliases: Vec::new(),
                path: format!("lm3://{archive_name}/{slot}/{texture_hash:08X}"),
                source: "embedded".into(),
                data_url,
                width,
                height,
                array_count: 1,
                renderable: true,
                data_urls: Vec::new(),
            });
        }
    }

    let name = format!("{archive_name}_{slot}");
    Ok((
        Lm3Model {
            sections: vec![Lm3Section {
                signature: *b"FMDL",
                name: name.clone(),
                offset: 0,
            }],
            name,
            archive: archive_name.to_string(),
            slot,
            materials,
            render: BfresRenderGraph {
                bones,
                matrix_to_bone: Vec::new(),
                meshes,
            },
            format: "LM3".into(),
        },
        resolved_textures,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardcoded_archive_specs_parse_and_cover_known_slots() {
        let specs = archives();
        assert_eq!(specs.len(), 2);
        let global = archive_spec("global").expect("global archive spec");
        assert_eq!(global.dict, "global.dict");
        assert_eq!(global.slots, 815);
        let persistent = archive_spec("persistent").expect("persistent archive spec");
        assert_eq!(persistent.dict, "Scarescraper/Persistent.dict");
        assert!(persistent.slots > 61, "documented slots reach 61");
        assert!(archive_spec("unknown").is_none());
    }

    #[test]
    fn slot_catalog_reports_only_existing_archives_with_known_names() {
        let missing = slot_catalog(Path::new("Z:/does/not/exist"));
        assert!(missing.is_empty());

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../tmp/lm3-catalog-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("global.dict"), b"stub").unwrap();
        let catalog = slot_catalog(&root);
        assert_eq!(catalog.len(), 815);
        assert_eq!(catalog[25].id, "global_25");
        assert_eq!(catalog[25].archive, "global");
        assert_eq!(catalog[25].slot, 25);
        assert_eq!(catalog[27].name.as_deref(), Some("Story Mode Luigi"));
        assert!(catalog[0].name.is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn texture_formats_come_from_headers_alone_and_reject_normal_maps() {
        // A B501 header with no following B502 must still be classified: the
        // material binding needs the format to reject BC5 normal maps even
        // when the image data lives in another archive.
        let mut file63 = vec![0u8; 32];
        file63[0..4].copy_from_slice(&0xAAAA_0001u32.to_le_bytes());
        file63[12] = 0x16; // BC5 normal map
        file63[16..20].copy_from_slice(&0xBBBB_0002u32.to_le_bytes());
        file63[28] = 0x19; // ASTC 4x4 diffuse
        let subentries = vec![
            Lm3SubEntry {
                kind: 0xB501,
                flags: 0,
                size: 16,
                offset: 0,
            },
            Lm3SubEntry {
                kind: 0xB501,
                flags: 0,
                size: 16,
                offset: 16,
            },
        ];
        let formats = texture_formats_from_headers(&subentries, &file63);
        assert_eq!(formats.len(), 2);
        assert_eq!(formats[&0xAAAA_0001], 0x16);
        assert_eq!(formats[&0xBBBB_0002], 0x19);
        // Neither header is paired with a B502, so nothing is decodable...
        assert!(texture_records(&subentries, &file63).is_empty());

        // ...yet the material still resolves its diffuse, skipping the normal map.
        let model = vec![
            Lm3SubEntry {
                kind: 0xB006,
                flags: 0,
                size: 0x10,
                offset: 0x60,
            },
            Lm3SubEntry {
                kind: 0xB007,
                flags: 0,
                size: 0x20,
                offset: 0x70,
            },
        ];
        let mut file52 = vec![0u8; 0xA0];
        // The material payload references the normal map before the diffuse.
        file52[0x60..0x64].copy_from_slice(&0xAAAA_0001u32.to_le_bytes());
        file52[0x64..0x68].copy_from_slice(&0xBBBB_0002u32.to_le_bytes());
        file52[0x70..0x74].copy_from_slice(&8u32.to_le_bytes());
        file52[0x74..0x90].copy_from_slice(&[
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00,
            0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00,
        ]);
        assert_eq!(formats[&0xAAAA_0001], 0x16, "normal map stays classified");
    }

    #[test]
    fn base_color_skips_the_packed_specular_map() {
        let entry = |hash, flags, format| Lm3TextureRef {
            hash,
            flags,
            format,
            known: true,
        };
        // Slot 27 mesh 12: a BC5 normal, then the sRGB ASTC 6x6 roughness /
        // specular map, then the real albedo. Picking the first sRGB record
        // would wrongly select 1AC09D9C.
        let mesh12 = vec![
            entry(0x1A78_3B98, 0x0180_0000, 0x16),
            entry(0x1AC0_9D9C, 0x8180_0000, 0x1D),
            entry(0x198C_FD0B, 0x8180_0000, 0x1E),
            entry(0x1A66_2317, 0x8180_0000, 0x1E),
        ];
        assert_eq!(base_color_index(&mesh12), Some(2));
        assert_eq!(mesh12[2].hash, 0x198C_FD0B);

        // The common shape: albedo first, then its linear data maps.
        let mesh0 = vec![
            entry(0x83DE_AF97, 0x8180_0000, 0x1E),
            entry(0x84C9_EE24, 0x0180_0000, 0x16),
            entry(0x8512_5028, 0x0180_0000, 0x1D),
        ];
        assert_eq!(base_color_index(&mesh0), Some(0));

        // Only a 6x6 sRGB candidate: it is still better than nothing.
        let only_6x6 = vec![entry(0x1111_1111, 0x8180_0000, 0x1D)];
        assert_eq!(base_color_index(&only_6x6), Some(0));
        // No sRGB at all: fall back to the first non-normal-map record.
        let linear = vec![
            entry(0x2222_2222, 0x0180_0000, 0x16),
            entry(0x3333_3333, 0x0180_0000, 0x19),
        ];
        assert_eq!(base_color_index(&linear), Some(1));
        assert_eq!(base_color_index(&[]), None);
    }

    #[test]
    fn material_offsets_come_from_sentinel_delimited_b007_records() {
        // Two records: the first ends with a material pointer, the second with
        // a value that is not one, so that mesh has no material of its own.
        const SENTINEL: [u8; 28] = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00,
            0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00,
        ];
        let mut file52 = Vec::new();
        file52.extend_from_slice(&0x1234u32.to_le_bytes());
        file52.extend_from_slice(&0x0033_DAC0u32.to_le_bytes());
        file52.extend_from_slice(&SENTINEL);
        file52.extend_from_slice(&0x5678u32.to_le_bytes());
        file52.extend_from_slice(&8u32.to_le_bytes());
        file52.extend_from_slice(&SENTINEL);
        let b007 = Lm3SubEntry {
            kind: 0xB007,
            flags: 0,
            size: file52.len() as u32,
            offset: 0,
        };
        // The second record ends with the small "no material of its own"
        // constant, which must not be read as an offset.
        let offsets = material_offsets(&file52, b007, 0x0033_E550);
        assert_eq!(offsets, vec![Some(0x0033_DAC0), None]);
        // A pointer past the material chunk is rejected rather than trusted.
        assert_eq!(material_offsets(&file52, b007, 0x100), vec![None, None]);
    }

    #[test]
    fn skeleton_reads_local_transforms_into_the_viewer_frame() {
        let mut file53 = vec![0u8; 0x100];
        // 0x7103 transforms at 0: two bones of quaternion(xyzw) + position.
        let write_f32 = |data: &mut Vec<u8>, at: usize, value: f32| {
            data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        for (index, position) in [[1.0f32, 2.0, 3.0], [0.0, 0.0, 0.0]].iter().enumerate() {
            let base = index * 0x1C;
            write_f32(&mut file53, base + 12, 1.0); // identity quaternion w
            for (axis, value) in position.iter().enumerate() {
                write_f32(&mut file53, base + 16 + axis * 4, *value);
            }
        }
        // 0x7106 parents at 0x40: bone 0 is a root (0xFFFF), bone 1 chains to it.
        file53[0x40..0x42].copy_from_slice(&0xFFFFu16.to_le_bytes());
        file53[0x42..0x44].copy_from_slice(&0u16.to_le_bytes());
        // 0x7105 hash-to-id map at 0x50.
        file53[0x50..0x54].copy_from_slice(&0xFEED_0001u32.to_le_bytes());
        file53[0x54..0x58].copy_from_slice(&7u32.to_le_bytes());

        let group = vec![
            Lm3SubEntry {
                kind: 0x7103,
                flags: 0,
                size: 0x38,
                offset: 0,
            },
            Lm3SubEntry {
                kind: 0x7106,
                flags: 0,
                size: 4,
                offset: 0x40,
            },
            Lm3SubEntry {
                kind: 0x7105,
                flags: 0,
                size: 8,
                offset: 0x50,
            },
        ];
        let (bones, hash_to_id) = read_skeleton(&file53, &group).expect("skeleton");
        assert_eq!(bones.len(), 2);
        // 0xFFFF is a root marker, not a signed -1 bone index.
        assert_eq!(bones[0].parent_index, -1);
        assert_eq!(bones[1].parent_index, 0);
        // The skeleton is stored rotated 90 degrees about Z from the vertex
        // buffers, so a bone point (x, y, z) lands at (y, z, x).
        assert_eq!(bones[0].translation, [2.0, 3.0, 1.0]);
        // Conjugating an identity rotation leaves it identity.
        assert!(bones[0].rotation[3].abs() > 0.999);
        assert_eq!(bones[0].rotation_mode, "quaternion");
        assert_eq!(bones[0].scale, [1.0, 1.0, 1.0]);
        assert_eq!(hash_to_id.get(&0xFEED_0001), Some(&7));

        // A skeleton group without transforms or parents is not usable.
        assert!(read_skeleton(&file53, &group[..1]).is_none());
    }

    #[test]
    fn bone_basis_rotation_matches_its_translation_swizzle() {
        // The conjugation quaternion and the position swizzle must describe the
        // same basis change, otherwise a bone chain drifts away from the mesh.
        let rotate = |q: [f32; 4], v: [f32; 3]| {
            let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
            let t = [
                2.0 * (y * v[2] - z * v[1]),
                2.0 * (z * v[0] - x * v[2]),
                2.0 * (x * v[1] - y * v[0]),
            ];
            [
                v[0] + w * t[0] + (y * t[2] - z * t[1]),
                v[1] + w * t[1] + (z * t[0] - x * t[2]),
                v[2] + w * t[2] + (x * t[1] - y * t[0]),
            ]
        };
        for axis in [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 2.0, 3.0],
        ] {
            let rotated = rotate(BONE_BASIS, axis);
            let swizzled = rebase_translation(axis);
            for component in 0..3 {
                assert!(
                    (rotated[component] - swizzled[component]).abs() < 1e-5,
                    "axis {axis:?}: quaternion gave {rotated:?}, swizzle gave {swizzled:?}"
                );
            }
        }
        // Conjugating by the basis and its inverse leaves identity untouched.
        let identity = rebase_rotation([0.0, 0.0, 0.0, 1.0]);
        assert!((identity[3].abs() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn synthetic_archive_roundtrips_slot_geometry() {
        // One uncompressed archive; slot 0 holds a single unskinned triangle
        // with one bound texture, so every offset path in parse_slot is
        // exercised. The entry count must cover the texture files (63 and 65).
        let chunk_count = 0usize;
        let file_count = 66usize;

        let texture_hash = 0xCAFE0001u32;

        // Sub-entry table (entry 0): one 0x1301 primary record, the leading
        // B501/B502 texture block, then the model's B006, B003, B004, B005,
        // and B007 sub-entries.
        let mut table = Vec::new();
        table.extend_from_slice(&0x1301u16.to_le_bytes());
        table.extend_from_slice(&vec![0u8; 22]);
        let mut push_subentry = |table: &mut Vec<u8>, kind: u16, size: u32, offset: u32| {
            table.extend_from_slice(&kind.to_le_bytes());
            table.extend_from_slice(&0u16.to_le_bytes());
            table.extend_from_slice(&size.to_le_bytes());
            table.extend_from_slice(&offset.to_le_bytes());
        };
        push_subentry(&mut table, 0xB501, 16, 0); // texture header in file 63
        push_subentry(&mut table, 0xB502, 8 * 8 * 4, 0); // texture data in file 65
                                                         // File 52 layout: B003 descriptor at 0, B004 record at 0x40, B006
                                                         // material payload at 0x60, B007 binding markers at 0x70.
        push_subentry(&mut table, 0xB006, 0x80, 0x100);
        push_subentry(&mut table, 0xB003, 0x40, 0);
        push_subentry(&mut table, 0xB004, 12, 0x40);
        // File 54 layout: indices at 0, vertices at 8.
        push_subentry(&mut table, 0xB005, 0x100, 0);
        push_subentry(&mut table, 0xB007, 0x20, 0x180);

        let mut file52 = vec![0u8; 0x1A0];
        file52[0..4].copy_from_slice(&0xDEADBEEFu32.to_le_bytes()); // mesh hash
        file52[4..8].copy_from_slice(&0u32.to_le_bytes()); // index offset
        file52[8..12].copy_from_slice(&3u32.to_le_bytes()); // 3 u16 indices
        file52[12..16].copy_from_slice(&3u32.to_le_bytes()); // 3 vertices
        file52[0x28..0x2C].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // unskinned
        file52[0x40..0x44].copy_from_slice(&8u32.to_le_bytes()); // vertex offset
                                                                 // B006 material: the pointer is 0x40 and the run sits a variable
                                                                 // header later, at material offset 0x48 (file52 0x148).
        file52[0x148..0x14C].copy_from_slice(&texture_hash.to_le_bytes());
        file52[0x14C..0x150].copy_from_slice(&0x8180_0000u32.to_le_bytes()); // sRGB
        file52[0x150..0x154].copy_from_slice(&0x0000_0801u32.to_le_bytes());
        // B007: one record whose last word before the sentinel is the pointer.
        file52[0x180..0x184].copy_from_slice(&0x40u32.to_le_bytes());
        file52[0x184..0x1A0].copy_from_slice(&[
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00,
            0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00,
        ]);

        // File 63: 16-byte texture header (hash, 8x8, RGBA8 format 0x05).
        let mut file63 = vec![0u8; 16];
        file63[0..4].copy_from_slice(&texture_hash.to_le_bytes());
        file63[4..6].copy_from_slice(&8u16.to_le_bytes());
        file63[6..8].copy_from_slice(&8u16.to_le_bytes());
        file63[12] = 0x05;

        // File 65: the swizzled 8x8 RGBA surface (GOB height 2 for 8x8).
        let linear: Vec<u8> = (0..8u32)
            .flat_map(|y| (0..8u32).map(move |x| (x, y)))
            .flat_map(|(x, y)| [(x * 32) as u8, (y * 32) as u8, 100, 255])
            .collect();
        let file65 = tegra_swizzle::surface::swizzle_surface(
            8,
            8,
            1,
            &linear,
            tegra_swizzle::surface::BlockDim::uncompressed(),
            Some(tegra_swizzle::BlockHeight::new(2).unwrap()),
            4,
            1,
            1,
        )
        .unwrap();

        let mut file54 = vec![0u8; 8 + 3 * 0x30];
        for (index, value) in [0u16, 1, 2].iter().enumerate() {
            file54[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        for vertex in 0..3usize {
            let record = 8 + vertex * 0x30;
            let position = [vertex as f32, 0.0, 1.0];
            for (axis, value) in position.iter().enumerate() {
                file54[record + axis * 4..record + axis * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
            file54[record + 0x0C..record + 0x10].copy_from_slice(&0.25f32.to_le_bytes()); // u
            file54[record + 0x10..record + 0x14].copy_from_slice(&0.0f32.to_le_bytes());
            file54[record + 0x14..record + 0x18].copy_from_slice(&1.0f32.to_le_bytes());
            file54[record + 0x18..record + 0x1C].copy_from_slice(&0.0f32.to_le_bytes());
            file54[record + 0x1C..record + 0x20].copy_from_slice(&0.25f32.to_le_bytes());
            // v
        }

        let payloads: Vec<Vec<u8>> = (0..file_count)
            .map(|index| match index {
                0 => table.clone(),
                52 => file52.clone(),
                54 => file54.clone(),
                63 => file63.clone(),
                65 => file65.clone(),
                _ => Vec::new(),
            })
            .collect();
        let mut data = Vec::new();
        let mut dict = Vec::new();
        dict.extend_from_slice(&super::DICT_MAGIC.to_le_bytes());
        dict.extend_from_slice(&0u16.to_le_bytes());
        dict.push(0); // uncompressed
        dict.push(0);
        dict.extend_from_slice(&0u32.to_le_bytes());
        dict.push(file_count as u8);
        dict.push(chunk_count as u8);
        dict.push(0);
        dict.push(0);
        for payload in &payloads {
            let offset = data.len() as u32;
            data.extend_from_slice(payload);
            dict.extend_from_slice(&offset.to_le_bytes());
            dict.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            dict.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            dict.extend_from_slice(&[0u8; 4]);
        }

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../tmp/lm3-parse-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("global.dict"), &dict).unwrap();
        std::fs::write(root.join("global.data"), &data).unwrap();

        let (model, resolved_textures) =
            parse_slot(&root.join("global.dict"), "global", 0).unwrap();
        assert_eq!(model.name, "global_0");
        assert_eq!(model.format, "LM3");
        assert_eq!(model.sections.len(), 1);
        assert_eq!(&model.sections[0].signature, b"FMDL");
        assert_eq!(model.render.meshes.len(), 1);
        assert_eq!(model.materials.len(), 1);
        // The material binding resolves to the B501/B502 texture, which decodes
        // and reaches the viewer as an embedded PNG data URL.
        assert_eq!(model.materials[0].texture_slots.len(), 1);
        assert_eq!(model.materials[0].texture_slots[0].name, "CAFE0001");
        assert_eq!(
            model.materials[0].texture_slots[0].texture_type,
            "Base color"
        );
        assert_eq!(resolved_textures.len(), 1);
        assert_eq!(resolved_textures[0].name, "CAFE0001");
        assert_eq!(
            (resolved_textures[0].width, resolved_textures[0].height),
            (8, 8)
        );
        assert!(resolved_textures[0]
            .data_url
            .starts_with("data:image/png;base64,"));
        assert_eq!(resolved_textures[0].source, "embedded");
        let mesh = &model.render.meshes[0];
        assert_eq!(mesh.name, "slot_0_mesh_00_DEADBEEF");
        assert_eq!(mesh.indices, vec![0, 1, 2]);
        assert_eq!(mesh.positions.len(), 3);
        // Z-up source position (1, 0, 1) lands at Y-up (1, 1, 0).
        assert_eq!(mesh.positions[1], [1.0, 1.0, -0.0]);
        // V is stored raw for the viewer's flipY=false textures.
        assert_eq!(mesh.uv0[0], [0.25, 0.25]);
        assert_eq!(mesh.colors.len(), 3);

        let error = parse_slot(&root.join("global.dict"), "global", 5).unwrap_err();
        assert!(error.to_string().contains("1 model slots"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
