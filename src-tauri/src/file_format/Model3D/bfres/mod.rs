//! Parser and conservative name editor for Nintendo BFRES resource containers.
//!
//! BFRES stores its object graph as relocated pointers.  This module parses the
//! container header and inventories the typed resource sections without tying
//! the result to TotkBits' document/YAML representation.

mod material;
mod replace;
mod serializer;
mod skeleton;
pub mod tomodachi;
pub mod toolbox;

use rfd::{FileDialog, MessageDialog};
use serde::Serialize;
use std::{fmt, fs, io, path::Path, sync::Arc};

use crate::parser::binary::{BinaryReader, BinaryWriter, Endian as BinaryEndian};
use crate::Zstd::{TotkZstd, ZstdDictionary};

fn binary_endian(endian: Endian) -> BinaryEndian {
    match endian {
        Endian::Little => BinaryEndian::Little,
        Endian::Big => BinaryEndian::Big,
    }
}

const SECTION_SIGNATURES: &[&[u8; 4]] = &[
    b"FMDL", b"FSKL", b"FVTX", b"FSHP", b"FMAT", b"FSKA", b"FSHU", b"FSHA", b"FSCN", b"FTXP",
    b"FVIS", b"FMAA", b"FREL",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Endian {
    Little,
    Big,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BfresHeader {
    pub version: [u8; 4],
    pub endian: Endian,
    pub alignment_exponent: u8,
    pub target_address_size: u8,
    pub name_offset: u32,
    pub flags: u16,
    pub block_offset: u16,
    pub relocation_table_offset: u32,
    /// Size used by BFRES itself. Files are commonly padded beyond this value.
    pub file_size: u32,
    pub string_pool_size: u32,
    pub string_pool_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BfresSection {
    pub signature: [u8; 4],
    pub offset: u64,
    pub name: Option<String>,
}

impl BfresSection {
    pub fn signature_str(&self) -> &str {
        std::str::from_utf8(&self.signature).unwrap_or("????")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BfresFile {
    pub header: BfresHeader,
    pub name: Option<String>,
    pub sections: Vec<BfresSection>,
    pub materials: Vec<material::BfresMaterial>,
    pub render: BfresRenderGraph,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BfresRenderGraph {
    pub bones: Vec<BfresBone>,
    pub matrix_to_bone: Vec<u16>,
    pub meshes: Vec<BfresMesh>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BfresBone {
    pub name: String,
    pub parent_index: i16,
    pub smooth_matrix_index: i16,
    pub rigid_matrix_index: i16,
    pub rotation_mode: String,
    pub scale: [f32; 3],
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BfresMesh {
    pub name: String,
    pub material_index: u16,
    pub bone_index: u16,
    pub vertex_skin_count: u8,
    #[serde(default)]
    pub is_cloth: bool,
    #[serde(default)]
    pub cloth_id: u16,
    #[serde(default)]
    pub nun_id: u32,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uv0: Vec<[f32; 2]>,
    pub uv_maps: Vec<Vec<[f32; 2]>>,
    pub colors: Vec<[f32; 4]>,
    pub bone_indices: Vec<[u16; 4]>,
    pub bone_weights: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub skin_bones: Vec<u16>,
}

/// Encodes one vertex attribute value with a BFRES vertex format code
/// (`0x0515` half4, `0x020E` 10-10-10-2 normals, `0x030B` u8x4 indices, ...)
/// at `offset` inside `out`. `components` is the meaningful component count
/// for formats whose width depends on it (`0x0515`). Returns the written width.
pub(crate) fn encode_vertex_attribute(
    out: &mut [u8],
    offset: usize,
    format: u16,
    value: [f32; 4],
    components: usize,
) -> Result<usize, BfresError> {
    replace::encode(out, offset, format, value, components)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BfresError {
    pub offset: usize,
    pub message: String,
}

impl BfresError {
    fn new(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }
}

impl fmt::Display for BfresError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BFRES error at 0x{:X}: {}", self.offset, self.message)
    }
}

impl std::error::Error for BfresError {}

impl BfresFile {
    /// Replaces model geometry from an FBX while retaining the BFRES resource
    /// graph, skeleton, materials and all other sections.
    pub fn replace_geometry_from_fbx(data: &[u8], fbx: &[u8]) -> Result<Vec<u8>, BfresError> {
        replace::replace_geometry_from_fbx(data, fbx)
    }
    /// Renames the first FMDL model and the container's internal BFRES name.
    ///
    /// BFRES files keep absolute name pointers. The new `ResString` is placed in
    /// trailing file padding so all existing sections, geometry, and offsets remain unchanged.
    pub fn rename_first_model(data: &[u8], new_name: &str) -> Result<Vec<u8>, BfresError> {
        Self::rename_first_model_and_container(data, new_name, new_name)
    }

    /// Renames the first FMDL and the top-level BFRES container independently.
    pub fn rename_first_model_and_container(
        data: &[u8],
        model_name: &str,
        container_name: &str,
    ) -> Result<Vec<u8>, BfresError> {
        if !crate::Settings::Magic::is_bfres(data) {
            return Err(BfresError::new(
                0,
                "rename requires an uncompressed BFRES file",
            ));
        }
        if model_name.is_empty()
            || container_name.is_empty()
            || model_name.as_bytes().contains(&0)
            || container_name.as_bytes().contains(&0)
        {
            return Err(BfresError::new(
                0,
                "BFRES model name must be non-empty and contain no NUL",
            ));
        }
        if [model_name, container_name]
            .iter()
            .any(|name| name.len() > 0x1000 || u16::try_from(name.len()).is_err())
        {
            return Err(BfresError::new(0, "BFRES model name is too long"));
        }

        let parsed = Self::from_bytes(data)?;
        if !matches!(parsed.header.version[2], 8 | 9 | 10) {
            return Err(BfresError::new(
                8,
                "safe name-only serialization supports BFRES versions 8 through 10",
            ));
        }
        let model = parsed
            .sections_with_signature(b"FMDL")
            .next()
            .ok_or_else(|| BfresError::new(0, "BFRES contains no FMDL model"))?;
        let model_pointer_field = usize::try_from(model.offset)
            .ok()
            .and_then(|offset| {
                offset.checked_add(if parsed.header.version[2] <= 8 { 16 } else { 8 })
            })
            .ok_or_else(|| BfresError::new(0, "FMDL name pointer offset overflow"))?;
        let pointer_size = match parsed.header.target_address_size {
            4 => 4,
            8 | 0 => 8,
            value => {
                return Err(BfresError::new(
                    0x0F,
                    format!("unsupported BFRES pointer size {value}"),
                ))
            }
        };
        if model_pointer_field + pointer_size > data.len() {
            return Err(BfresError::new(
                model_pointer_field,
                "truncated FMDL name pointer",
            ));
        }

        let old_model_pointer = match pointer_size {
            4 => u32_at(data, model_pointer_field, parsed.header.endian)? as usize,
            _ => usize::try_from(u64_at(data, model_pointer_field, parsed.header.endian)?)
                .map_err(|_| BfresError::new(model_pointer_field, "model name pointer overflow"))?,
        };
        let old_model_name = model
            .name
            .as_deref()
            .ok_or_else(|| BfresError::new(old_model_pointer, "first FMDL has no name"))?;
        let internal_pointer = parsed.header.name_offset as usize;
        let old_internal_name = parsed.name.as_deref();
        let mut output = data.to_vec();

        // Keep all resource and buffer offsets stable. Growing a string in the
        // middle of the string pool also requires rewriting non-pointer relative
        // buffer offsets, which is easy to miss and corrupts vertex streams.
        // New strings are therefore inserted immediately before _RLT and only
        // their explicit owners are redirected to the new locations.
        if let Some(old_internal_name) = old_internal_name {
            let internal_slot = res_string_slot(data, internal_pointer, old_internal_name)?;
            let points_to_characters = internal_pointer == internal_slot + 2;
            let new_slot = append_res_string_before_relocation(&mut output, container_name)?;
            let new_pointer = new_slot + usize::from(points_to_characters) * 2;
            write_u32(
                &mut output,
                0x10,
                u32::try_from(new_pointer)
                    .map_err(|_| BfresError::new(0x10, "container name offset overflow"))?,
                parsed.header.endian,
            )?;
        }

        let shifted = Self::from_bytes(&output)?;
        let shifted_model = shifted
            .sections_with_signature(b"FMDL")
            .next()
            .ok_or_else(|| BfresError::new(0, "BFRES contains no FMDL model"))?;
        let shifted_model_pointer_field = shifted_model.offset as usize
            + if shifted.header.version[2] <= 8 {
                16
            } else {
                8
            };
        let shifted_model_pointer = match pointer_size {
            4 => u32_at(&output, shifted_model_pointer_field, shifted.header.endian)? as usize,
            _ => u64_at(&output, shifted_model_pointer_field, shifted.header.endian)? as usize,
        };
        let model_slot = res_string_slot(&output, shifted_model_pointer, old_model_name)?;
        let points_to_characters = shifted_model_pointer == model_slot + 2;
        let new_slot = append_res_string_before_relocation(&mut output, model_name)?;
        let new_pointer = new_slot + usize::from(points_to_characters) * 2;
        match pointer_size {
            4 => write_u32(
                &mut output,
                shifted_model_pointer_field,
                u32::try_from(new_pointer).map_err(|_| {
                    BfresError::new(shifted_model_pointer_field, "model name offset overflow")
                })?,
                shifted.header.endian,
            )?,
            _ => write_u64(
                &mut output,
                shifted_model_pointer_field,
                new_pointer as u64,
                shifted.header.endian,
            )?,
        }
        let model_slot = new_slot;

        let reopened = Self::from_bytes(&output)?;
        let reopened_model = reopened
            .sections_with_signature(b"FMDL")
            .next()
            .and_then(|section| section.name.as_deref());
        if reopened_model != Some(model_name)
            || (parsed.name.is_some() && reopened.name.as_deref() != Some(container_name))
        {
            return Err(BfresError::new(
                model_slot,
                format!(
                    "renamed BFRES failed name-pointer validation (model={reopened_model:?}, internal={:?})",
                    reopened.name
                ),
            ));
        }
        Ok(output)
    }

    pub fn rename_first_model_file(
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
        new_name: &str,
    ) -> io::Result<()> {
        let source = fs::read(source)?;
        let renamed = Self::rename_first_model(&source, new_name).map_err(io::Error::other)?;
        fs::write(destination, renamed)
    }

    /// Replaces `base` with `new_name` in every texture slot of every FMAT.
    /// Shared ResStrings (including resource-dictionary keys) are edited once.
    pub fn rename_material_texture_slots(
        data: &[u8],
        base: &str,
        new_name: &str,
    ) -> Result<Vec<u8>, BfresError> {
        if base.is_empty() {
            return Err(BfresError::new(0, "texture-name base must not be empty"));
        }
        if new_name.as_bytes().contains(&0) {
            return Err(BfresError::new(0, "texture name must contain no NUL"));
        }
        let parsed = Self::from_bytes(data)?;
        let mut replacements = std::collections::BTreeMap::<usize, (String, String)>::new();
        for section in parsed.sections_with_signature(b"FMAT") {
            let material = section.offset as usize;
            let (names_pointer_offset, count_offset) = match parsed.header.version[2] {
                0..=8 => (56, 168),
                9 => (48, 179),
                _ => (32, 163),
            };
            let names =
                u64_at(data, material + names_pointer_offset, parsed.header.endian)? as usize;
            let count = data.get(material + count_offset).copied().unwrap_or(0) as usize;
            for index in 0..count {
                let pointer_field = names + index * 8;
                let pointer_value = u64_at(data, pointer_field, parsed.header.endian)?;
                let pointer = pointer_value as usize;
                let old = read_string(data, pointer_value).ok_or_else(|| {
                    BfresError::new(pointer_field, "invalid FMAT texture-name pointer")
                })?;
                let new = old.replace(base, new_name);
                if new == old {
                    continue;
                }
                let slot = res_string_slot(data, pointer, &old)?;
                if let Some((_, existing)) = replacements.get(&slot) {
                    if existing != &new {
                        return Err(BfresError::new(
                            slot,
                            "shared texture string has conflicting replacements",
                        ));
                    }
                } else {
                    replacements.insert(slot, (old, new));
                }
            }
        }

        let expected: Vec<_> = parsed
            .materials
            .iter()
            .flat_map(|material| &material.texture_slots)
            .map(|slot| slot.name.replace(base, new_name))
            .collect();
        let mut output = data.to_vec();
        for (slot, (old, new)) in replacements {
            output = redirect_res_string(&output, slot, &old, &new)?;
        }
        let reopened = Self::from_bytes(&output)?;
        let actual: Vec<_> = reopened
            .materials
            .iter()
            .flat_map(|material| &material.texture_slots)
            .map(|slot| slot.name.clone())
            .collect();
        if actual != expected {
            return Err(BfresError::new(
                0,
                "rewritten BFRES texture slots failed validation",
            ));
        }
        Ok(output)
    }

    pub fn open_internal(
        bytes: Vec<u8>,
        path: &str,
        outer_path: Option<&str>,
    ) -> Option<(
        crate::file_format::BinTextFile::OpenedFile<'static>,
        crate::InternalFile::InternalFile<'static>,
        crate::Open_and_Save::SendData,
    )> {
        // #[cfg(not(debug_assertions))]
        // {
        //     return None;
        // }
        let bfres = Self::from_bytes(&bytes).ok()?;
        let mut data = crate::Open_and_Save::SendData::default();
        data.path = crate::Settings::Pathlib::new(path);
        data.file_label = format!("{} [BFRES]", data.path.name);
        data.file_metadata = "[BFRES] [3D]".into();
        data.file_type = crate::Zstd::TotkFileType::Bfres;
        data.status_text = match outer_path {
            Some(outer) => format!("Opened {path} inside {outer}"),
            None => format!("Opened {path} from archive"),
        };
        data.tab = "3D".into();
        data.read_only = true;

        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.file_type = crate::Zstd::TotkFileType::Bfres;
        opened.path = data.path.clone();
        opened.bfres = Some(bfres);
        opened.bfres_data = Some(bytes);
        let mut internal = crate::InternalFile::InternalFile::new(path.into());
        internal.file_type = crate::Zstd::TotkFileType::Bfres;
        Some((opened, internal, data))
    }

    /// Standalone BFRES save path for the currently read-only viewer.
    ///
    /// BFRES editing/serialization is not implemented yet. Once it is, both
    /// Save and Save As should call this function with newly serialized raw
    /// BFRES bytes.
    #[allow(dead_code)]
    pub fn save_bfres_version_10<P: AsRef<Path>>(
        source_path: P,
        bfres: &Self,
        serialized_bfres: &[u8],
        zstd: &crate::Zstd::TotkZstd<'_>,
    ) -> io::Result<Option<std::path::PathBuf>> {
        if bfres.header.version[2] != 10 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "MCPK save prompt is only available for BFRES version 10",
            ));
        }
        if !crate::Settings::Magic::is_bfres(serialized_bfres) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "BFRES serializer did not produce a raw FRES file",
            ));
        }

        let compress = MessageDialog::new()
            .set_title("BFRES version 10 compression")
            .set_description("Compress this BFRES with MCPK?")
            .set_level(rfd::MessageLevel::Info)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            == rfd::MessageDialogResult::Yes;

        let source_path = source_path.as_ref();
        let original_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("model.bfres");
        let raw_name = original_name.strip_suffix(".mc").unwrap_or(original_name);
        let suggested_name = if compress {
            format!("{raw_name}.mc")
        } else {
            raw_name.to_owned()
        };

        let mut dialog = FileDialog::new()
            .set_title("Save BFRES")
            .set_file_name(suggested_name);
        dialog = if compress {
            dialog.add_filter("MeshCodec BFRES", &["bfres.mc", "mc"])
        } else {
            dialog.add_filter("BFRES", &["bfres"])
        };
        let Some(destination) = dialog.save_file() else {
            return Ok(None);
        };

        let output = if compress {
            zstd.compress_mcpk(serialized_bfres)?
        } else {
            serialized_bfres.to_vec()
        };
        fs::write(&destination, output)?;
        Ok(Some(destination))
    }

    pub fn open(
        path: &std::path::Path,
        zstd: Arc<TotkZstd>,
    ) -> Option<(
        crate::file_format::BinTextFile::OpenedFile<'static>,
        crate::Open_and_Save::SendData,
    )> {
        let rawdata = fs::read(path).ok()?;
        // Handle MCPK before the generic Zstandard probing. Pseudo-MCPK files
        // contain a magicless Zstandard payload, so treating them as ordinary
        // Zstandard can leave the wrapper intact and reject an otherwise valid
        // BFRES before `from_bytes` gets a chance to parse it.
        let (source, compression) = if crate::Settings::Magic::is_mcpk(&rawdata) {
            (zstd.decompress_mcpk(&rawdata).ok()?, ZstdDictionary::Mcpk)
        } else {
            zstd.try_decompress_all_ordered_safe(&rawdata, path)
        };

        if !crate::Settings::Magic::is_bfres(&source) {
            return None;
        }
        let file = Self::from_bytes(&source).ok()?;
        let disk_path = crate::Settings::Pathlib::new(path);
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.file_type = crate::Zstd::TotkFileType::Bfres;
        opened.path = disk_path.clone();
        opened.bfres = Some(file);
        opened.bfres_data = Some(source);
        opened.compression = (compression != ZstdDictionary::None).then_some(compression);

        let mut data = crate::Open_and_Save::SendData::default();
        data.path = disk_path;
        data.file_label = format!("{} [BFRES]", data.path.name);
        data.file_metadata = "[BFRES] [3D]".into();
        data.file_type = crate::Zstd::TotkFileType::Bfres;
        if compression != ZstdDictionary::None {
            data.file_metadata += &format!(" [{:?}]", compression);
        }
        data.status_text = format!("Opened BFRES {}", path.display());
        data.tab = "3D".into();
        data.read_only = true;
        Some((opened, data))
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, BfresError> {
        let data = fs::read(path.as_ref())
            .map_err(|error| BfresError::new(0, format!("failed to read file: {error}")))?;
        Self::from_bytes(&data)
    }

    pub fn open_binary<P: AsRef<Path>>(
        bytes: &[u8],
        path: P,
        zstd: Arc<TotkZstd>,
    ) -> Option<(
        crate::file_format::BinTextFile::OpenedFile<'static>,
        crate::Open_and_Save::SendData,
    )> {
        let path_ref = path.as_ref();
        let (source, compression) = if crate::Settings::Magic::is_mcpk(bytes) {
            (
                zstd.decompress_mcpk(bytes).ok()?,
                crate::Zstd::ZstdDictionary::Mcpk,
            )
        } else {
            zstd.try_decompress_all_ordered_safe(bytes, path_ref)
        };
        if !crate::Settings::Magic::is_bfres(&source) {
            return None;
        }
        let file = Self::from_bytes(&source).ok()?;
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.file_type = crate::Zstd::TotkFileType::Bfres;
        opened.path = crate::Settings::Pathlib::new(path_ref);
        opened.bfres = Some(file);
        opened.bfres_data = Some(source);
        opened.compression = (compression != ZstdDictionary::None).then_some(compression);
        let mut data = crate::Open_and_Save::SendData::default();
        data.path = crate::Settings::Pathlib::new(path_ref);
        data.file_label = format!("{} [BFRES]", data.path.name);
        data.file_metadata = "[BFRES] [3D]".into();
        data.file_type = crate::Zstd::TotkFileType::Bfres;
        if compression != ZstdDictionary::None {
            data.file_metadata += &format!(" [{:?}]", compression);
        }
        data.status_text = format!("Opened BFRES {}", path_ref.display());
        data.tab = "3D".into();
        data.read_only = true;
        Some((opened, data))
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, BfresError> {
        if crate::Settings::Magic::is_mcpk(data) {
            let decompressed =
                crate::compression::meshcodec::MeshCodec::decompress(data).map_err(|error| {
                    BfresError::new(0, format!("MCPK decompression failed: {error}"))
                })?;
            if !crate::Settings::Magic::is_bfres(&decompressed) {
                return Err(BfresError::new(
                    0,
                    "MCPK payload is not a BFRES file (missing FRES signature)",
                ));
            }
            return Self::from_bytes(&decompressed);
        }
        if !crate::Settings::Magic::is_bfres(data) {
            let decompressed = crate::Zstd::TotkZstd::decompress_empty(data).map_err(|error| {
                BfresError::new(0, format!("Zstandard decompression failed: {error}"))
            })?;
            if !crate::Settings::Magic::is_bfres(&decompressed) {
                return Err(BfresError::new(
                    0,
                    "Zstandard payload is not a BFRES file (missing FRES signature)",
                ));
            }
            return Self::from_bytes(&decompressed);
        }
        if data.len() < 0x30 {
            return Err(BfresError::new(0, "header is truncated"));
        }
        if &data[..4] != b"FRES" {
            return Err(BfresError::new(0, "invalid FRES signature"));
        }

        let endian = match &data[0x0C..0x0E] {
            [0xFF, 0xFE] => Endian::Little,
            [0xFE, 0xFF] => Endian::Big,
            _ => return Err(BfresError::new(0x0C, "invalid byte-order mark")),
        };
        let read_u16 = |offset| u16_at(data, offset, endian);
        let read_u32 = |offset| u32_at(data, offset, endian);

        let header = BfresHeader {
            version: [data[8], data[9], data[10], data[11]],
            endian,
            alignment_exponent: data[0x0E],
            target_address_size: data[0x0F],
            name_offset: read_u32(0x10)?,
            flags: read_u16(0x14)?,
            block_offset: read_u16(0x16)?,
            relocation_table_offset: read_u32(0x18)?,
            file_size: read_u32(0x1C)?,
            string_pool_size: read_u32(0x20)?,
            string_pool_offset: read_u32(0x24)?,
        };

        if header.file_size as usize > data.len() {
            return Err(BfresError::new(0x1C, "declared file size exceeds input"));
        }
        if header.target_address_size != 0
            && header.target_address_size != 4
            && header.target_address_size != 8
        {
            return Err(BfresError::new(0x0F, "unsupported target address size"));
        }

        let name = read_string(data, header.name_offset as u64);
        let mut sections = Vec::new();
        for offset in (0..data.len().saturating_sub(3)).filter(|offset| offset % 4 == 0) {
            let signature = [
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ];
            if !SECTION_SIGNATURES
                .iter()
                .any(|candidate| **candidate == signature)
            {
                continue;
            }
            // All resource sections begin with their signature, four reserved
            // bytes, then an absolute pointer to their ResString name. FREL is
            // the sole unnamed container-level section.
            let section_name = if &signature == b"FREL" {
                None
            } else if header.target_address_size != 4 {
                let name_field = offset + if header.version[2] <= 8 { 16 } else { 8 };
                u64_at(data, name_field, endian)
                    .ok()
                    .and_then(|pointer| read_string(data, pointer))
            } else {
                u32_at(data, offset + 8, endian)
                    .ok()
                    .and_then(|pointer| read_string(data, pointer as u64))
            };
            // These named resources must have a valid ResString pointer. This
            // also filters coincidental magic bytes in appended geometry;
            // FSKL/FVTX and several animation resources may legitimately be
            // unnamed and therefore cannot use this check.
            if matches!(&signature, b"FMDL" | b"FMAT" | b"FSHP") && section_name.is_none() {
                continue;
            }
            sections.push(BfresSection {
                signature,
                offset: offset as u64,
                name: section_name,
            });
        }

        if sections.is_empty() {
            return Err(BfresError::new(0, "no BFRES resource sections found"));
        }
        let materials = material::parse_materials(data, &sections, endian, header.version[2]);
        let render = parse_render_graph(
            data,
            endian,
            header.file_size as usize,
            &sections,
            header.version[2],
        )?;
        Ok(Self {
            header,
            name,
            sections,
            materials,
            render,
        })
    }

    pub fn sections_with_signature<'a>(
        &'a self,
        signature: &'a [u8; 4],
    ) -> impl Iterator<Item = &'a BfresSection> + 'a {
        self.sections
            .iter()
            .filter(move |section| &section.signature == signature)
    }
}

#[derive(Clone)]
struct VertexStream {
    vertex_count: usize,
    attributes: Vec<VertexAttribute>,
    buffers: Vec<(usize, Vec<u8>)>,
}

#[derive(Clone)]
struct VertexAttribute {
    name: String,
    format: u16,
    offset: usize,
    buffer_index: usize,
}

fn parse_render_graph(
    data: &[u8],
    endian: Endian,
    logical_file_size: usize,
    sections: &[BfresSection],
    version_major: u8,
) -> Result<BfresRenderGraph, BfresError> {
    if endian != Endian::Little {
        return Ok(BfresRenderGraph::default());
    }
    let buffer_base = if version_major <= 8 {
        let memory_pool = u64_at(data, 0x90, endian)? as usize;
        u64_at(data, memory_pool + 8, endian)? as usize
    } else {
        let buffer_info = u64_at(data, 0xB0, endian)? as usize;
        let external_flags = byte_at(data, 0xEE).unwrap_or(0);
        let mcpk_resave = byte_at(data, 0xEF).unwrap_or(0);
        if buffer_info != 0 && buffer_info + 16 <= data.len() {
            u64_at(data, buffer_info + 8, endian)? as usize
        } else if external_flags & 1 != 0 || mcpk_resave != 0 {
            logical_file_size + 288
        } else {
            return Ok(BfresRenderGraph::default());
        }
    };
    // BFRES 8 stores an additional name pointer after each FVTX/FSHP
    // signature. Later versions removed it, shifting the remaining fields
    // eight bytes toward the start of each structure.
    let resource_field_offset = usize::from(version_major <= 8) * 8;
    let mut streams = std::collections::HashMap::new();
    for section in sections
        .iter()
        .filter(|section| &section.signature == b"FVTX")
    {
        let offset = section.offset as usize;
        if let Ok(stream) =
            parse_vertex_stream(data, offset, buffer_base, endian, resource_field_offset)
        {
            streams.insert(offset, stream);
        }
    }
    let (bones, matrix_to_bone) = sections
        .iter()
        .find(|section| &section.signature == b"FSKL")
        .and_then(|section| {
            skeleton::parse_skeleton(data, section.offset as usize, endian, version_major).ok()
        })
        .unwrap_or_default();
    let mut meshes = Vec::new();
    for section in sections
        .iter()
        .filter(|section| &section.signature == b"FSHP")
    {
        let Ok(mut parsed) = parse_shape(
            data,
            section,
            buffer_base,
            endian,
            &streams,
            &matrix_to_bone,
            resource_field_offset,
        ) else {
            continue;
        };
        meshes.append(&mut parsed);
    }
    let mut names = std::collections::HashMap::<String, usize>::new();
    for mesh in &mut meshes {
        let count = names.entry(mesh.name.clone()).or_default();
        if *count > 0 {
            mesh.name = format!("{} ({})", mesh.name, *count + 1);
        }
        *count += 1;
    }
    Ok(BfresRenderGraph {
        bones,
        matrix_to_bone,
        meshes,
    })
}

fn parse_vertex_stream(
    data: &[u8],
    offset: usize,
    buffer_base: usize,
    endian: Endian,
    field_offset: usize,
) -> Result<VertexStream, BfresError> {
    let attr_offset = u64_at(data, offset + 8 + field_offset, endian)? as usize;
    let sizes_offset = u64_at(data, offset + 48 + field_offset, endian)? as usize;
    let strides_offset = u64_at(data, offset + 56 + field_offset, endian)? as usize;
    let relative_buffer = u32_at(data, offset + 72 + field_offset, endian)? as usize;
    let attr_count = byte_at(data, offset + 76 + field_offset)? as usize;
    let buffer_count = byte_at(data, offset + 77 + field_offset)? as usize;
    let vertex_count = u32_at(data, offset + 80 + field_offset, endian)? as usize;
    let alignment = u16_at(data, offset + 86 + field_offset, endian)? as usize;
    let mut attributes = Vec::with_capacity(attr_count);
    for index in 0..attr_count {
        let entry = attr_offset + index * 16;
        attributes.push(VertexAttribute {
            name: read_string(data, u64_at(data, entry, endian)?)
                .unwrap_or_else(|| format!("attribute_{index}")),
            format: BinaryReader::with_endian(data, BinaryEndian::Big)
                .read_u16_at(entry + 8)
                .map_err(|error| BfresError::new(entry + 8, error.to_string()))?,
            offset: u16_at(data, entry + 12, endian)? as usize,
            buffer_index: byte_at(data, entry + 14)? as usize,
        });
    }
    let mut buffers = Vec::with_capacity(buffer_count);
    let mut cursor = buffer_base
        .checked_add(relative_buffer)
        .ok_or_else(|| BfresError::new(offset, "vertex buffer offset overflow"))?;
    for index in 0..buffer_count {
        cursor = align_up(cursor, alignment.max(1));
        let size = u32_at(data, sizes_offset + index * 16, endian)? as usize;
        let stride = u32_at(data, strides_offset + index * 16, endian)? as usize;
        let bytes = data
            .get(cursor..cursor + size)
            .ok_or_else(|| BfresError::new(cursor, "vertex buffer exceeds file"))?
            .to_vec();
        buffers.push((stride, bytes));
        cursor += size;
    }
    Ok(VertexStream {
        vertex_count,
        attributes,
        buffers,
    })
}

fn parse_shape(
    data: &[u8],
    section: &BfresSection,
    buffer_base: usize,
    endian: Endian,
    streams: &std::collections::HashMap<usize, VertexStream>,
    matrix_to_bone: &[u16],
    field_offset: usize,
) -> Result<Vec<BfresMesh>, BfresError> {
    let offset = section.offset as usize;
    let vertex_offset = u64_at(data, offset + 16 + field_offset, endian)? as usize;
    let mesh_offset = u64_at(data, offset + 24 + field_offset, endian)? as usize;
    let skin_offset = u64_at(data, offset + 32 + field_offset, endian)? as usize;
    let scalar_offset = if field_offset == 0 { 0 } else { 12 };
    let material_index = u16_at(data, offset + 82 + scalar_offset, endian)?;
    let bone_index = u16_at(data, offset + 84 + scalar_offset, endian)?;
    let skin_count = u16_at(data, offset + 88 + scalar_offset, endian)? as usize;
    let vertex_skin_count = byte_at(data, offset + 90 + scalar_offset)?;
    let mesh_count = byte_at(data, offset + 91 + scalar_offset)? as usize;
    let stream = streams
        .get(&vertex_offset)
        .ok_or_else(|| BfresError::new(offset + 16, "shape references unknown FVTX"))?;
    let skin_bones = if skin_count == 0 {
        Vec::new()
    } else {
        (0..skin_count)
            .map(|i| u16_at(data, skin_offset + i * 2, endian))
            .collect::<Result<Vec<_>, _>>()?
    };
    let positions: Vec<[f32; 3]> = decode_attribute(stream, data, "_p")?
        .into_iter()
        .map(|v| [v[0], v[1], v[2]])
        .collect();
    let normals: Vec<[f32; 3]> = decode_attribute(stream, data, "_n")
        .unwrap_or_default()
        .into_iter()
        .map(|v| [v[0], v[1], v[2]])
        .collect();
    let uv0: Vec<[f32; 2]> = decode_attribute(stream, data, "_u0")
        .unwrap_or_default()
        .into_iter()
        .map(|v| [v[0], v[1]])
        .collect();
    let uv_maps: Vec<Vec<[f32; 2]>> = (0..8)
        .filter_map(|index| decode_attribute(stream, data, &format!("_u{index}")).ok())
        .map(|values| {
            values
                .into_iter()
                .map(|value| [value[0], value[1]])
                .collect()
        })
        .filter(|values: &Vec<[f32; 2]>| values.len() == positions.len())
        .collect();
    let colors = decode_attribute(stream, data, "_c0").unwrap_or_default();

    // BFRES _i attributes contain skeleton matrix indices. MatrixToBone maps
    // those palette indices to actual FSKL bone indices. FSHP::SkinBoneIndices
    // is metadata listing bones used by the shape, not the vertex lookup table.
    let raw_indices = decode_attribute(stream, data, "_i").unwrap_or_default();
    let mut bone_indices: Vec<[u16; 4]> = raw_indices
        .into_iter()
        .map(|v| {
            let map = |value: f32| -> u16 {
                let fallback = if vertex_skin_count == 1 {
                    skin_bones.first().copied().unwrap_or(bone_index)
                } else {
                    bone_index
                };
                if !value.is_finite() || value < 0.0 {
                    return fallback;
                }
                let matrix_index = value.round() as usize;
                matrix_to_bone
                    .get(matrix_index)
                    .copied()
                    .unwrap_or(fallback)
            };
            [map(v[0]), map(v[1]), map(v[2]), map(v[3])]
        })
        .collect();

    let mut bone_weights = decode_attribute(stream, data, "_w").unwrap_or_default();

    // Rigid shapes have no per-vertex skin attributes. Bind every vertex to the
    // shape's rigid bone instead of leaving empty/zero skinning data.
    if vertex_skin_count == 0 || skin_bones.is_empty() {
        bone_indices = vec![[bone_index, bone_index, bone_index, bone_index]; stream.vertex_count];
        bone_weights = vec![[1.0, 0.0, 0.0, 0.0]; stream.vertex_count];
    } else {
        // Missing weights are valid for one-bone skinning in some files.
        if bone_weights.len() != stream.vertex_count {
            bone_weights = vec![[1.0, 0.0, 0.0, 0.0]; stream.vertex_count];
        }

        // Normalise and sanitise weights. Small quantisation errors are common,
        // but NaN/negative/zero-sum weights must never reach the renderer.
        for weights in &mut bone_weights {
            for weight in weights.iter_mut().skip(vertex_skin_count as usize) {
                *weight = 0.0;
            }
            for weight in weights.iter_mut() {
                if !weight.is_finite() || *weight < 0.0 {
                    *weight = 0.0;
                }
            }
            let sum: f32 = weights.iter().sum();
            if sum > f32::EPSILON {
                for weight in weights.iter_mut() {
                    *weight /= sum;
                }
            } else {
                *weights = [1.0, 0.0, 0.0, 0.0];
            }
        }
    }
    let mut result = Vec::new();
    // Mesh entries are ordered from highest to lowest detail. Rendering only
    // the first avoids drawing every LOD on top of the same shape.
    for mesh_index in 0..mesh_count.min(1) {
        let entry = mesh_offset + mesh_index * 56;
        let size_offset = u64_at(data, entry + 24, endian)? as usize;
        let face_offset = u32_at(data, entry + 32, endian)? as usize;
        let primitive = u32_at(data, entry + 36, endian)?;
        let format = u32_at(data, entry + 40, endian)?;
        let index_count = u32_at(data, entry + 44, endian)? as usize;
        // FSHP Mesh indices are relative to this base vertex. Omitting it makes
        // otherwise valid local indices reference vertices from another region
        // of the shared FVTX stream, producing long triangles/spikes.
        let first_vertex = u32_at(data, entry + 48, endian)?;
        let byte_size = u32_at(data, size_offset, endian)? as usize;
        let raw = data
            .get(buffer_base + face_offset..buffer_base + face_offset + byte_size)
            .ok_or_else(|| {
                BfresError::new(buffer_base + face_offset, "index buffer exceeds file")
            })?;
        let indices = decode_indices(raw, format, index_count, endian, primitive)?
            .chunks_exact(3)
            .filter_map(|triangle| {
                let triangle = [
                    triangle[0].checked_add(first_vertex)?,
                    triangle[1].checked_add(first_vertex)?,
                    triangle[2].checked_add(first_vertex)?,
                ];
                triangle
                    .iter()
                    .all(|index| *index < stream.vertex_count as u32)
                    .then_some(triangle)
            })
            .flatten()
            .collect();
        result.push(BfresMesh {
            name: if mesh_count > 1 {
                format!(
                    "{} #{mesh_index}",
                    section.name.as_deref().unwrap_or("Shape")
                )
            } else {
                section
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("Shape_{mesh_index}"))
            },
            material_index,
            bone_index,
            vertex_skin_count,
            is_cloth: false,
            cloth_id: 0,
            nun_id: 0,
            positions: positions.clone(),
            normals: normals.clone(),
            uv0: uv0.clone(),
            uv_maps: uv_maps.clone(),
            colors: colors.clone(),
            bone_indices: bone_indices.clone(),
            bone_weights: bone_weights.clone(),
            indices,
            skin_bones: skin_bones.clone(),
        });
    }
    Ok(result)
}

fn decode_attribute(
    stream: &VertexStream,
    _data: &[u8],
    prefix: &str,
) -> Result<Vec<[f32; 4]>, BfresError> {
    let attribute = stream
        .attributes
        .iter()
        .find(|attribute| attribute.name == prefix || attribute.name.starts_with(prefix))
        .ok_or_else(|| BfresError::new(0, format!("missing {prefix} vertex attribute")))?;
    let (stride, bytes) = stream
        .buffers
        .get(attribute.buffer_index)
        .ok_or_else(|| BfresError::new(0, "vertex attribute buffer index is invalid"))?;
    (0..stream.vertex_count)
        .map(|index| {
            decode_vertex_value(
                bytes,
                index * *stride + attribute.offset,
                attribute.format,
                prefix,
            )
        })
        .collect()
}

fn decode_vertex_value(
    data: &[u8],
    offset: usize,
    format: u16,
    semantic: &str,
) -> Result<[f32; 4], BfresError> {
    let b = |i| byte_at(data, offset + i);
    let u = |i| u16_at(data, offset + i, Endian::Little);
    let s = |i| i16_at(data, offset + i, Endian::Little);
    let f = |i| f32_at(data, offset + i, Endian::Little);
    Ok(match format {
        0x020E => {
            let packed = u32_at(data, offset, Endian::Little)?;
            let signed = |shift: u32| (((packed >> shift) & 0x3ff) as i32) << 22 >> 22;
            [
                signed(0) as f32 / 511.0,
                signed(10) as f32 / 511.0,
                signed(20) as f32 / 511.0,
                ((packed >> 30) & 3) as f32,
            ]
        }
        0x0518 => [f(0)?, f(4)?, f(8)?, 1.0],
        0x0519 => [f(0)?, f(4)?, f(8)?, f(12)?],
        0x0517 => [f(0)?, f(4)?, 0.0, 1.0],
        0x0516 => [f(0)?, 0.0, 0.0, 1.0],
        0x0512 => [half(u(0)?), half(u(2)?), 0.0, 1.0],
        0x0515 if semantic.starts_with("_p") || semantic.starts_with("_n") => {
            [half(u(0)?), half(u(2)?), half(u(4)?), 1.0]
        }
        0x0515 => [half(u(0)?), half(u(2)?), half(u(4)?), half(u(6)?)],
        0x010B => [
            b(0)? as f32 / 255.0,
            b(1)? as f32 / 255.0,
            b(2)? as f32 / 255.0,
            b(3)? as f32 / 255.0,
        ],
        0x020B => [
            b(0)? as i8 as f32 / 127.0,
            b(1)? as i8 as f32 / 127.0,
            b(2)? as i8 as f32 / 127.0,
            b(3)? as i8 as f32 / 127.0,
        ],
        0x030B => [b(0)? as f32, b(1)? as f32, b(2)? as f32, b(3)? as f32],
        0x0302 => [b(0)? as f32, 0.0, 0.0, 0.0],
        0x0115 => [
            u(0)? as f32 / 65535.0,
            u(2)? as f32 / 65535.0,
            u(4)? as f32 / 65535.0,
            u(6)? as f32 / 65535.0,
        ],
        0x0215 => [
            s(0)? as f32 / 32767.0,
            s(2)? as f32 / 32767.0,
            s(4)? as f32 / 32767.0,
            s(6)? as f32 / 32767.0,
        ],
        0x0315 => [u(0)? as f32, u(2)? as f32, u(4)? as f32, u(6)? as f32],
        0x0109 => [b(0)? as f32 / 255.0, b(1)? as f32 / 255.0, 0.0, 1.0],
        0x0309 => [b(0)? as f32, b(1)? as f32, 0.0, 1.0],
        0x0112 => [u(0)? as f32 / 65535.0, u(2)? as f32 / 65535.0, 0.0, 1.0],
        0x0212 => [s(0)? as f32 / 32767.0, s(2)? as f32 / 32767.0, 0.0, 1.0],
        _ => {
            return Err(BfresError::new(
                offset,
                format!("unsupported vertex format 0x{format:04X}"),
            ))
        }
    })
}

fn decode_indices(
    raw: &[u8],
    format: u32,
    count: usize,
    endian: Endian,
    primitive: u32,
) -> Result<Vec<u32>, BfresError> {
    let source = match format {
        0 => (0..count)
            .map(|i| byte_at(raw, i).map(u32::from))
            .collect::<Result<Vec<_>, _>>()?,
        1 => (0..count)
            .map(|i| u16_at(raw, i * 2, endian).map(u32::from))
            .collect::<Result<Vec<_>, _>>()?,
        2 => (0..count)
            .map(|i| u32_at(raw, i * 4, endian))
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(BfresError::new(0, "unsupported index format")),
    };
    if primitive == 3 {
        return Ok(source);
    }
    if primitive == 4 {
        let mut triangles = Vec::new();
        let restart = match format {
            0 => u8::MAX as u32,
            1 => u16::MAX as u32,
            _ => u32::MAX,
        };
        let mut strip = Vec::new();
        for index in source {
            if index == restart {
                strip.clear();
                continue;
            }
            strip.push(index);
            if strip.len() >= 3 {
                let i = strip.len() - 1;
                let (a, b) = if i % 2 == 0 {
                    (strip[i - 2], strip[i - 1])
                } else {
                    (strip[i - 1], strip[i - 2])
                };
                let c = strip[i];
                if a != b && b != c && a != c {
                    triangles.extend([a, b, c]);
                }
            }
        }
        return Ok(triangles);
    }
    Err(BfresError::new(
        0,
        format!("unsupported primitive type {primitive}"),
    ))
}

fn align_up(value: usize, alignment: usize) -> usize {
    if alignment <= 1 {
        value
    } else {
        (value + alignment - 1) / alignment * alignment
    }
}

fn half(value: u16) -> f32 {
    let sign = ((value >> 15) & 1) as u32;
    let exponent = ((value >> 10) & 0x1f) as u32;
    let fraction = (value & 0x3ff) as u32;
    let bits = if exponent == 0 {
        if fraction == 0 {
            sign << 31
        } else {
            let shift = fraction.leading_zeros() - 21;
            (sign << 31) | ((127 - 15 - shift) << 23) | ((fraction << (shift + 1) & 0x3ff) << 13)
        }
    } else if exponent == 31 {
        (sign << 31) | 0x7f800000 | (fraction << 13)
    } else {
        (sign << 31) | ((exponent + 112) << 23) | (fraction << 13)
    };
    f32::from_bits(bits)
}

fn byte_at(data: &[u8], offset: usize) -> Result<u8, BfresError> {
    BinaryReader::new(data)
        .read_u8_at(offset)
        .map_err(|error| BfresError::new(offset, error.to_string()))
}
pub(super) fn i16_at(data: &[u8], offset: usize, endian: Endian) -> Result<i16, BfresError> {
    Ok(u16_at(data, offset, endian)? as i16)
}
pub(super) fn f32_at(data: &[u8], offset: usize, endian: Endian) -> Result<f32, BfresError> {
    Ok(f32::from_bits(u32_at(data, offset, endian)?))
}

pub(super) fn u16_at(data: &[u8], offset: usize, endian: Endian) -> Result<u16, BfresError> {
    BinaryReader::with_endian(data, binary_endian(endian))
        .read_u16_at(offset)
        .map_err(|error| BfresError::new(offset, error.to_string()))
}

pub(super) fn u32_at(data: &[u8], offset: usize, endian: Endian) -> Result<u32, BfresError> {
    BinaryReader::with_endian(data, binary_endian(endian))
        .read_u32_at(offset)
        .map_err(|error| BfresError::new(offset, error.to_string()))
}

pub(super) fn u64_at(data: &[u8], offset: usize, endian: Endian) -> Result<u64, BfresError> {
    BinaryReader::with_endian(data, binary_endian(endian))
        .read_u64_at(offset)
        .map_err(|error| BfresError::new(offset, error.to_string()))
}

pub(super) fn read_string(data: &[u8], offset: u64) -> Option<String> {
    let offset = usize::try_from(offset).ok()?;
    if offset == 0 || offset >= data.len() {
        return None;
    }
    if let Some(length_bytes) = data.get(offset..offset + 2) {
        let length = BinaryReader::new(length_bytes).read_u16().ok()? as usize;
        if length > 0 && length <= 0x1000 {
            let bytes = data.get(offset + 2..offset + 2 + length)?;
            if !bytes.iter().any(|byte| *byte < 0x20 && *byte != b'\t') {
                if let Ok(value) = std::str::from_utf8(bytes) {
                    return Some(value.to_owned());
                }
            }
        }
    }
    let tail = &data[offset..];
    let end = tail.iter().position(|byte| *byte == 0)?;
    let bytes = &tail[..end];
    if bytes.is_empty()
        || bytes.len() > 0x1000
        || bytes.iter().any(|byte| *byte < 0x20 && *byte != b'\t')
    {
        return None;
    }
    std::str::from_utf8(bytes).ok().map(str::to_owned)
}

fn res_string_slot(data: &[u8], pointer: usize, expected: &str) -> Result<usize, BfresError> {
    let matches = |prefix: usize, characters: usize| {
        BinaryReader::new(data).read_u16_at(prefix).ok() == Some(expected.len() as u16)
            && data.get(characters..characters + expected.len()) == Some(expected.as_bytes())
    };
    if matches(pointer, pointer.saturating_add(2)) {
        return Ok(pointer);
    }
    if let Some(prefix) = pointer.checked_sub(2) {
        if matches(prefix, pointer) {
            return Ok(prefix);
        }
    }
    Err(BfresError::new(
        pointer,
        "name pointer does not reference the expected ResString",
    ))
}

fn write_res_string_in_place(
    data: &mut [u8],
    offset: usize,
    capacity: usize,
    value: &str,
) -> Result<(), BfresError> {
    if value.len() > capacity {
        return Err(BfresError::new(
            offset,
            "replacement ResString exceeds its slot",
        ));
    }
    let end = offset
        .checked_add(2 + capacity)
        .ok_or_else(|| BfresError::new(offset, "ResString destination overflow"))?;
    let destination = data
        .get_mut(offset..end)
        .ok_or_else(|| BfresError::new(offset, "truncated ResString destination"))?;
    destination.fill(0);
    let mut writer = BinaryWriter::from_vec(destination.to_vec(), BinaryEndian::Little);
    writer.write_u16_at(0, value.len() as u16);
    destination.copy_from_slice(&writer.into_inner());
    destination[2..2 + value.len()].copy_from_slice(value.as_bytes());
    Ok(())
}

fn append_res_string_before_relocation(
    data: &mut Vec<u8>,
    value: &str,
) -> Result<usize, BfresError> {
    let parsed = BfresFile::from_bytes(data)?;
    let relocation = parsed.header.relocation_table_offset as usize;
    if data.get(relocation..relocation.saturating_add(4)) != Some(b"_RLT") {
        return Err(BfresError::new(
            relocation,
            "BFRES relocation table is missing",
        ));
    }
    let mut encoded_writer = BinaryWriter::new();
    encoded_writer.write_u16(value.len() as u16);
    encoded_writer.write_bytes(value.as_bytes());
    encoded_writer.write_u8(0);
    let mut encoded = encoded_writer.into_inner();
    while encoded.len() % 8 != 0 {
        encoded.push(0);
    }
    let added = encoded.len();
    data.splice(relocation..relocation, encoded);
    let new_relocation = relocation
        .checked_add(added)
        .ok_or_else(|| BfresError::new(relocation, "relocation offset overflow"))?;
    let new_file_size = parsed
        .header
        .file_size
        .checked_add(
            u32::try_from(added)
                .map_err(|_| BfresError::new(relocation, "appended BFRES string is too large"))?,
        )
        .ok_or_else(|| BfresError::new(0x1C, "BFRES file size overflow"))?;
    write_u32(
        data,
        0x18,
        u32::try_from(new_relocation)
            .map_err(|_| BfresError::new(0x18, "relocation offset overflow"))?,
        parsed.header.endian,
    )?;
    write_u32(data, 0x1C, new_file_size, parsed.header.endian)?;
    write_u32(
        data,
        new_relocation + 4,
        u32::try_from(new_relocation)
            .map_err(|_| BfresError::new(new_relocation + 4, "relocation offset overflow"))?,
        parsed.header.endian,
    )?;
    Ok(relocation)
}

fn redirect_res_string(
    data: &[u8],
    slot: usize,
    old_value: &str,
    new_value: &str,
) -> Result<Vec<u8>, BfresError> {
    let slot = res_string_slot(data, slot, old_value)?;
    if old_value.len() == new_value.len() {
        let mut output = data.to_vec();
        write_res_string_in_place(&mut output, slot, old_value.len(), new_value)?;
        return Ok(output);
    }
    let parsed = BfresFile::from_bytes(data)?;
    let old_relocation = parsed.header.relocation_table_offset as usize;
    let relocation = parse_relocation_layout(data, old_relocation, parsed.header.endian)?;
    let mut owners = Vec::new();
    for field in relocation.pointer_fields {
        let pointer = u64_at(data, field, parsed.header.endian)? as usize;
        if pointer == slot || pointer == slot + 2 {
            owners.push((field, pointer == slot + 2));
        }
    }
    if owners.is_empty() {
        return Err(BfresError::new(
            slot,
            "ResString has no relocated pointer owner",
        ));
    }
    let mut output = data.to_vec();
    let new_slot = append_res_string_before_relocation(&mut output, new_value)?;
    let added = output
        .len()
        .checked_sub(data.len())
        .ok_or_else(|| BfresError::new(slot, "BFRES size unexpectedly decreased"))?;
    for (field, points_to_characters) in owners {
        let shifted_field = if field >= old_relocation {
            field
                .checked_add(added)
                .ok_or_else(|| BfresError::new(field, "pointer field overflow"))?
        } else {
            field
        };
        write_u64(
            &mut output,
            shifted_field,
            (new_slot + usize::from(points_to_characters) * 2) as u64,
            parsed.header.endian,
        )?;
    }
    Ok(output)
}

fn replace_res_string(
    data: &[u8],
    slot: usize,
    old_value: &str,
    new_value: &str,
) -> Result<Vec<u8>, BfresError> {
    let verified_slot = res_string_slot(data, slot, old_value)?;
    if verified_slot != slot {
        return Err(BfresError::new(slot, "ResString slot changed unexpectedly"));
    }
    if old_value.len() == new_value.len() {
        let mut output = data.to_vec();
        write_res_string_in_place(&mut output, slot, old_value.len(), new_value)?;
        return Ok(output);
    }

    let parsed = BfresFile::from_bytes(data)?;
    let old_end = slot
        .checked_add(2 + old_value.len())
        .ok_or_else(|| BfresError::new(slot, "old ResString end overflow"))?;
    let replacement_len = 2 + new_value.len();
    let delta = replacement_len as i64 - (2 + old_value.len()) as i64;
    let old_relocation = parsed.header.relocation_table_offset as usize;
    if old_end > old_relocation {
        return Err(BfresError::new(
            slot,
            "cannot resize a name inside the relocation table",
        ));
    }

    // Switch Toolbox rebuilds the ResFile through NintenTools, which regenerates
    // this table. For a name-only edit we can preserve the file byte-for-byte and
    // use the same table as the authoritative list of pointer fields instead.
    let relocation = parse_relocation_layout(data, old_relocation, parsed.header.endian)?;
    let mut pointer_updates = Vec::new();
    for &field in &relocation.pointer_fields {
        let value = u64_at(data, field, parsed.header.endian)?;
        if value >= old_end as u64 && value < parsed.header.file_size as u64 {
            pointer_updates.push((field, shifted_u64(value, delta, field)?));
        }
    }

    let mut replacement_writer = BinaryWriter::new();
    replacement_writer.write_u16(new_value.len() as u16);
    replacement_writer.write_bytes(new_value.as_bytes());
    let replacement = replacement_writer.into_inner();
    let mut output = Vec::with_capacity((data.len() as i64 + delta) as usize);
    output.extend_from_slice(&data[..slot]);
    output.extend_from_slice(&replacement);
    output.extend_from_slice(&data[old_end..]);

    for (old_field, value) in pointer_updates {
        let field = shifted_position(old_field, old_end, delta)?;
        write_u64(&mut output, field, value, parsed.header.endian)?;
    }

    let update_offset = |field: usize| -> Result<u32, BfresError> {
        let value = u32_at(data, field, parsed.header.endian)?;
        if value >= old_end as u32 && value < parsed.header.file_size {
            shifted_u32(value, delta, field)
        } else {
            Ok(value)
        }
    };
    write_u32(
        &mut output,
        0x10,
        update_offset(0x10)?,
        parsed.header.endian,
    )?;
    write_u32(
        &mut output,
        0x18,
        shifted_u32(parsed.header.relocation_table_offset, delta, 0x18)?,
        parsed.header.endian,
    )?;
    write_u32(
        &mut output,
        0x1C,
        shifted_u32(parsed.header.file_size, delta, 0x1C)?,
        parsed.header.endian,
    )?;
    let pool_start = parsed.header.string_pool_offset as usize;
    let pool_end = pool_start.saturating_add(parsed.header.string_pool_size as usize);
    let pool_size = if slot >= pool_start && old_end <= pool_end {
        shifted_u32(parsed.header.string_pool_size, delta, 0x20)?
    } else {
        parsed.header.string_pool_size
    };
    write_u32(&mut output, 0x20, pool_size, parsed.header.endian)?;
    write_u32(
        &mut output,
        0x24,
        update_offset(0x24)?,
        parsed.header.endian,
    )?;

    let new_relocation = shifted_position(old_relocation, old_end, delta)?;
    for section in &relocation.sections {
        let new_position = if section.position as usize >= old_end {
            shifted_u32(section.position, delta, section.position_field)?
        } else {
            section.position
        };
        let section_end = section.position as usize + section.size as usize;
        let new_size = if slot >= section.position as usize && old_end <= section_end {
            shifted_u32(section.size, delta, section.size_field)?
        } else {
            section.size
        };
        write_u32(
            &mut output,
            shifted_position(section.position_field, old_end, delta)?,
            new_position,
            parsed.header.endian,
        )?;
        write_u32(
            &mut output,
            shifted_position(section.size_field, old_end, delta)?,
            new_size,
            parsed.header.endian,
        )?;
    }
    for entry in &relocation.entries {
        let new_position = if entry.position as usize >= old_end {
            shifted_u32(entry.position, delta, entry.position_field)?
        } else {
            entry.position
        };
        write_u32(
            &mut output,
            shifted_position(entry.position_field, old_end, delta)?,
            new_position,
            parsed.header.endian,
        )?;
    }
    let rlt_self_field = new_relocation + 4;
    if output.get(rlt_self_field.saturating_sub(4)..rlt_self_field) == Some(b"_RLT") {
        write_u32(
            &mut output,
            rlt_self_field,
            u32::try_from(new_relocation)
                .map_err(|_| BfresError::new(rlt_self_field, "relocation offset overflow"))?,
            parsed.header.endian,
        )?;
    } else {
        return Err(BfresError::new(
            new_relocation,
            "shifted BFRES relocation table is missing",
        ));
    }
    Ok(output)
}

#[derive(Debug)]
struct RelocationSectionLayout {
    position_field: usize,
    size_field: usize,
    position: u32,
    size: u32,
}

#[derive(Debug)]
struct RelocationEntryLayout {
    position_field: usize,
    position: u32,
}

#[derive(Debug)]
struct RelocationLayout {
    sections: Vec<RelocationSectionLayout>,
    entries: Vec<RelocationEntryLayout>,
    pointer_fields: Vec<usize>,
}

fn parse_relocation_layout(
    data: &[u8],
    offset: usize,
    endian: Endian,
) -> Result<RelocationLayout, BfresError> {
    if data.get(offset..offset + 4) != Some(b"_RLT") {
        return Err(BfresError::new(offset, "BFRES relocation table is missing"));
    }
    let section_count = u32_at(data, offset + 8, endian)? as usize;
    if section_count == 0 || section_count > 64 {
        return Err(BfresError::new(
            offset + 8,
            "invalid relocation section count",
        ));
    }
    let section_table = offset + 16;
    let entry_table = section_table
        .checked_add(section_count * 24)
        .ok_or_else(|| BfresError::new(offset, "relocation section table overflow"))?;
    let mut sections = Vec::with_capacity(section_count);
    let mut total_entries = 0usize;
    for index in 0..section_count {
        let base = section_table + index * 24;
        let position_field = base + 8;
        let size_field = base + 12;
        let entry_index = u32_at(data, base + 16, endian)? as usize;
        let entry_count = u32_at(data, base + 20, endian)? as usize;
        total_entries = total_entries.max(
            entry_index
                .checked_add(entry_count)
                .ok_or_else(|| BfresError::new(base + 20, "relocation entry count overflow"))?,
        );
        sections.push(RelocationSectionLayout {
            position_field,
            size_field,
            position: u32_at(data, position_field, endian)?,
            size: u32_at(data, size_field, endian)?,
        });
    }
    if total_entries > data.len().saturating_sub(entry_table) / 8 {
        return Err(BfresError::new(entry_table, "truncated relocation entries"));
    }
    let mut entries = Vec::with_capacity(total_entries);
    let mut pointer_fields = Vec::new();
    for index in 0..total_entries {
        let base = entry_table + index * 8;
        let position = u32_at(data, base, endian)?;
        let struct_count = u16_at(data, base + 4, endian)? as usize;
        let offset_count = data[base + 6] as usize;
        let padding_count = data[base + 7] as usize;
        if struct_count == 0 || offset_count == 0 {
            return Err(BfresError::new(base, "invalid relocation entry dimensions"));
        }
        let stride = (offset_count + padding_count)
            .checked_mul(8)
            .ok_or_else(|| BfresError::new(base, "relocation stride overflow"))?;
        for structure in 0..struct_count {
            let first = position as usize + structure * stride;
            for pointer in 0..offset_count {
                let field = first + pointer * 8;
                if field + 8 > offset {
                    return Err(BfresError::new(
                        field,
                        "relocated pointer lies outside data section",
                    ));
                }
                pointer_fields.push(field);
            }
        }
        entries.push(RelocationEntryLayout {
            position_field: base,
            position,
        });
    }
    pointer_fields.sort_unstable();
    pointer_fields.dedup();
    Ok(RelocationLayout {
        sections,
        entries,
        pointer_fields,
    })
}

fn shifted_position(position: usize, threshold: usize, delta: i64) -> Result<usize, BfresError> {
    if position < threshold {
        return Ok(position);
    }
    usize::try_from(position as i128 + delta as i128)
        .map_err(|_| BfresError::new(position, "shifted file position is out of range"))
}

fn shifted_u64(value: u64, delta: i64, offset: usize) -> Result<u64, BfresError> {
    u64::try_from(value as i128 + delta as i128)
        .map_err(|_| BfresError::new(offset, "shifted 64-bit offset is out of range"))
}

fn shifted_u32(value: u32, delta: i64, offset: usize) -> Result<u32, BfresError> {
    u32::try_from(value as i64 + delta)
        .map_err(|_| BfresError::new(offset, "shifted 32-bit offset is out of range"))
}

fn write_u32(data: &mut [u8], offset: usize, value: u32, endian: Endian) -> Result<(), BfresError> {
    if offset.checked_add(4).is_none_or(|end| end > data.len()) {
        return Err(BfresError::new(offset, "truncated u32 destination"));
    }
    let mut writer = BinaryWriter::from_vec(data.to_vec(), binary_endian(endian));
    writer.write_u32_at(offset, value);
    data.copy_from_slice(&writer.into_inner());
    Ok(())
}

fn write_u64(data: &mut [u8], offset: usize, value: u64, endian: Endian) -> Result<(), BfresError> {
    if offset.checked_add(8).is_none_or(|end| end > data.len()) {
        return Err(BfresError::new(offset, "truncated u64 destination"));
    }
    let mut writer = BinaryWriter::from_vec(data.to_vec(), binary_endian(endian));
    writer.write_u64_at(offset, value);
    data.copy_from_slice(&writer.into_inner());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_bfres_data() {
        assert!(BfresFile::from_bytes(b"not a bfres file").is_err());
    }

    #[test]
    fn opens_generated_weapon_pseudo_mcpk() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/test_sic/romfs/Model/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc");
        if !path.is_file() {
            return;
        }
        let parsed = BfresFile::from_path(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        serde_json::to_value(&parsed).expect("generated BFRES must serialize for the 3D viewer");
        assert!(!parsed.render.meshes.is_empty());
        assert!(parsed.render.meshes.iter().all(|mesh| {
            usize::from(mesh.material_index) < parsed.materials.len()
                && mesh
                    .positions
                    .iter()
                    .flatten()
                    .all(|component| component.is_finite())
        }));
        let zstd = Arc::new(TotkZstd::dictionaryless(
            Arc::new(crate::TotkConfig::TotkConfig::default()),
            crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
        ));
        assert!(BfresFile::open(&path, zstd).is_some());
    }

    /// Nintendo Switch Sports ships BFRES 0.9: the version 10 FSKL header with
    /// version 8 bone records, and the 168-byte FMAT whose texture count sits
    /// at 0x9D. Both broke the viewer (NaN bone transforms, no texture slots).
    #[test]
    fn parses_switch_sports_version_9_skeleton_and_materials() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/sports/Ply_HeadHuman00.bfres");
        if !path.is_file() {
            return;
        }
        let parsed = BfresFile::from_path(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert_eq!(parsed.header.version[2], 9);
        let bones = &parsed.render.bones;
        assert_eq!(bones.len(), 15);
        assert_eq!(bones[0].name, "nw4f_root");
        assert_eq!(bones[0].parent_index, -1);
        assert_eq!(bones[1].name, "Face00");
        assert_eq!(bones[1].parent_index, 0);
        assert!(bones.iter().all(|bone| {
            bone.scale
                .iter()
                .chain(bone.rotation.iter())
                .chain(bone.translation.iter())
                .all(|component| component.is_finite())
                && bone.rotation_mode == "euler_xyz"
        }));
        assert!((bones[1].translation[1] - 0.1846).abs() < 1e-3);
        let head = parsed
            .materials
            .iter()
            .find(|material| material.name == "mPly_Head")
            .expect("mPly_Head material");
        let slot = |sampler: &str| {
            head.texture_slots
                .iter()
                .find(|slot| slot.sampler == sampler)
                .unwrap_or_else(|| panic!("missing sampler {sampler}"))
        };
        assert_eq!(head.texture_slots.len(), 7);
        assert_eq!(slot("_a0").name, "mPly_Head_Alb");
        assert_eq!(slot("_a0").texture_type, "Base color");
        assert_eq!(slot("_n0").texture_type, "Normal");
        assert_eq!(slot("_rgh0").texture_type, "Roughness");
        assert_eq!(slot("_tcl0").name, "mPly_Head_Tcl");
        assert_eq!(parsed.render.meshes.len(), 24);
    }

    #[test]
    #[ignore = "diagnostic comparison of TotkBits and Toolbox BFRES outputs"]
    fn compare_invisible_and_toolbox_bfres_layouts() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/BotW Weapon Restoration/romfs/_model");
        let imported = crate::parser::fbx::import::import_for_bfres(
            &fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/untitled.fbx")).unwrap(),
        )
        .unwrap();
        println!(
            "FBX MESHES={:?}",
            imported
                .meshes
                .iter()
                .map(|mesh| (
                    &mesh.name,
                    mesh.positions.len(),
                    mesh.indices.len(),
                    &mesh.palette_bones,
                ))
                .collect::<Vec<_>>()
        );
        for mesh in &imported.meshes {
            let (minimum, maximum) = mesh.positions.iter().fold(
                ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
                |(mut minimum, mut maximum), value| {
                    for axis in 0..3 {
                        minimum[axis] = minimum[axis].min(value[axis]);
                        maximum[axis] = maximum[axis].max(value[axis]);
                    }
                    (minimum, maximum)
                },
            );
            println!(
                "FBX EXTENT {:?}: {:?}",
                mesh.name,
                std::array::from_fn::<_, 3, _>(|axis| maximum[axis] - minimum[axis])
            );
            let unique_positions: std::collections::HashSet<_> = mesh
                .positions
                .iter()
                .map(|value| value.map(f32::to_bits))
                .collect();
            let unique_position_uv: std::collections::HashSet<_> = mesh
                .positions
                .iter()
                .enumerate()
                .map(|(index, position)| {
                    (
                        position.map(f32::to_bits),
                        mesh.uv_maps
                            .first()
                            .and_then(|uv| uv.get(index))
                            .copied()
                            .unwrap_or_default()
                            .map(f32::to_bits),
                    )
                })
                .collect();
            let unique_position_normal_uv: std::collections::HashSet<_> = mesh
                .positions
                .iter()
                .enumerate()
                .map(|(index, position)| {
                    (
                        position.map(f32::to_bits),
                        mesh.normals
                            .get(index)
                            .copied()
                            .unwrap_or_default()
                            .map(f32::to_bits),
                        mesh.uv_maps
                            .first()
                            .and_then(|uv| uv.get(index))
                            .copied()
                            .unwrap_or_default()
                            .map(f32::to_bits),
                    )
                })
                .collect();
            println!(
                "FBX UNIQUE {:?}: position={} position_uv={} position_normal_uv={} referenced={}",
                mesh.name,
                unique_positions.len(),
                unique_position_uv.len(),
                unique_position_normal_uv.len(),
                mesh.indices
                    .iter()
                    .copied()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
            );
            for scale in [
                100_000.0_f32,
                80_000.0,
                60_000.0,
                50_000.0,
                40_000.0,
                30_000.0,
                20_000.0,
                10_000.0,
            ] {
                let quantized: std::collections::HashSet<_> = mesh
                    .positions
                    .iter()
                    .enumerate()
                    .map(|(index, position)| {
                        let q = |value: f32| (value * scale).round() as i32;
                        (
                            position.map(q),
                            mesh.normals.get(index).copied().unwrap_or_default().map(q),
                            mesh.uv_maps
                                .first()
                                .and_then(|uv| uv.get(index))
                                .copied()
                                .unwrap_or_default()
                                .map(q),
                        )
                    })
                    .collect();
                println!("  quantized {scale}: {}", quantized.len());
            }
            for epsilon in [7.5e-6_f32, 8e-6, 8.05e-6, 8.1e-6, 8.15e-6, 8.2e-6, 8.25e-6] {
                let mut representatives: Vec<([f32; 3], [f32; 2], [f32; 3])> = Vec::new();
                for (index, position) in mesh.positions.iter().copied().enumerate() {
                    let uv = mesh
                        .uv_maps
                        .first()
                        .and_then(|values| values.get(index))
                        .copied()
                        .unwrap_or_default();
                    let mut normal = mesh.normals.get(index).copied().unwrap_or_default();
                    let length = normal.iter().map(|value| value * value).sum::<f32>().sqrt();
                    if length > 0.0 {
                        normal = normal.map(|value| value / length);
                    }
                    if !representatives
                        .iter()
                        .any(|(other_position, other_uv, other_normal)| {
                            position.map(f32::to_bits) == other_position.map(f32::to_bits)
                                && uv.map(f32::to_bits) == other_uv.map(f32::to_bits)
                                && normal
                                    .iter()
                                    .zip(other_normal)
                                    .all(|(left, right)| (*left - right).abs() <= epsilon)
                        })
                    {
                        representatives.push((position, uv, normal));
                    }
                }
                println!("  epsilon {epsilon}: {}", representatives.len());
            }
            for normal_scale in [45_000.0_f32, 50_000.0, 55_000.0, 60_000.0, 65_535.0] {
                let packed: std::collections::HashSet<_> = mesh
                    .positions
                    .iter()
                    .enumerate()
                    .map(|(index, position)| {
                        let normal = mesh.normals.get(index).copied().unwrap_or_default();
                        let uv = mesh
                            .uv_maps
                            .first()
                            .and_then(|values| values.get(index))
                            .copied()
                            .unwrap_or_default();
                        (
                            position.map(f32::to_bits),
                            uv.map(f32::to_bits),
                            normal.map(|value| (value * normal_scale).round() as i32),
                        )
                    })
                    .collect();
                println!("  packed normal {normal_scale}: {}", packed.len());
            }
            for epsilon in [8e-6_f32, 9e-6, 1e-5, 1.1e-5, 1.2e-5, 1.3e-5, 1.4e-5] {
                let mut representatives: Vec<([f32; 3], [f32; 2], [f32; 3])> = Vec::new();
                for (index, position) in mesh.positions.iter().copied().enumerate() {
                    let uv = mesh
                        .uv_maps
                        .first()
                        .and_then(|values| values.get(index))
                        .copied()
                        .unwrap_or_default();
                    let normal = mesh.normals.get(index).copied().unwrap_or_default();
                    if !representatives
                        .iter()
                        .any(|(other_position, other_uv, other_normal)| {
                            position.map(f32::to_bits) == other_position.map(f32::to_bits)
                                && uv.map(f32::to_bits) == other_uv.map(f32::to_bits)
                                && normal
                                    .iter()
                                    .zip(other_normal)
                                    .map(|(left, right)| (*left - right) * (*left - right))
                                    .sum::<f32>()
                                    <= epsilon * epsilon
                        })
                    {
                        representatives.push((position, uv, normal));
                    }
                }
                println!("  euclidean {epsilon}: {}", representatives.len());
            }
        }
        for label in ["totkbits", "toolbox"] {
            let path = root
                .join(label)
                .join("Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc");
            let parsed = BfresFile::from_path(&path).unwrap();
            println!("{label} HEADER={:?}", parsed.header);
            println!(
                "{label} SECTIONS={:?}",
                parsed
                    .sections
                    .iter()
                    .map(|section| (section.signature_str(), section.offset, &section.name))
                    .collect::<Vec<_>>()
            );
            println!(
                "{label} MESHES={:?}",
                parsed
                    .render
                    .meshes
                    .iter()
                    .map(|mesh| (
                        &mesh.name,
                        mesh.material_index,
                        mesh.bone_index,
                        mesh.vertex_skin_count,
                        mesh.positions.len(),
                        mesh.indices.len(),
                        &mesh.skin_bones,
                    ))
                    .collect::<Vec<_>>()
            );
            if label == "toolbox" {
                let compressed = fs::read(&path).unwrap();
                let zstd = crate::Zstd::TotkZstd::dictionaryless(
                    std::sync::Arc::new(crate::TotkConfig::TotkConfig::default()),
                    crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
                );
                let raw = zstd.decompress_mcpk(&compressed).unwrap();
                let recompressed = zstd.compress_mcpk(&raw).unwrap();
                println!(
                    "TOOLBOX_MCPK_RECOMPRESS_MATCH={}",
                    recompressed == compressed
                );
            }
        }
    }

    #[test]
    #[ignore = "regenerates the TotkBits BFRES comparison fixture"]
    fn regenerates_totkbits_bfres_comparison_fixture() {
        let tmp = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let source = tmp.join("works/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc");
        let fbx = tmp.join("untitled.fbx");
        let output = tmp.join(
            "BotW Weapon Restoration/romfs/_model/totkbits/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        let zstd = crate::Zstd::TotkZstd::dictionaryless(
            std::sync::Arc::new(crate::TotkConfig::TotkConfig::default()),
            crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
        );
        let raw = zstd.decompress_mcpk(&fs::read(source).unwrap()).unwrap();
        let replaced = BfresFile::replace_geometry_from_fbx(&raw, &fs::read(fbx).unwrap()).unwrap();
        let compressed = zstd.compress_mcpk(&replaced).unwrap();
        fs::write(&output, compressed).unwrap();
        let regenerated = BfresFile::from_path(output).unwrap();
        assert_eq!(regenerated.render.meshes.len(), 2);
        assert_eq!(
            regenerated
                .render
                .meshes
                .iter()
                .map(|mesh| (mesh.positions.len(), mesh.indices.len()))
                .collect::<Vec<_>>(),
            [(119, 186), (1633, 2211)]
        );
    }

    #[test]
    #[ignore = "regenerates only the test_sic custom-weapon BFRES"]
    fn regenerates_test_sic_custom_weapon_bfres_only() {
        let tmp = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let source = tmp.join("works/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc");
        let fbx = tmp.join("untitled.fbx");
        let output = tmp.join("test_sic/romfs/Model/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc");
        fs::create_dir_all(output.parent().unwrap()).unwrap();
        let raw = crate::compression::meshcodec::MeshCodec::decompress(&fs::read(source).unwrap())
            .unwrap();
        let replaced = BfresFile::replace_geometry_from_fbx(&raw, &fs::read(fbx).unwrap()).unwrap();
        let compressed = crate::compression::meshcodec::MeshCodec::compress(&replaced).unwrap();
        fs::write(&output, compressed).unwrap();
        let regenerated = BfresFile::from_path(&output).unwrap();
        assert_eq!(regenerated.render.meshes.len(), 2);
    }

    #[test]
    fn parses_mario_zombie_regression_sample() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_ss/MarioZombie.bfres");
        if !path.is_file() {
            return;
        }
        let file = BfresFile::from_path(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(!file.materials.is_empty());
        assert!(file
            .materials
            .iter()
            .any(|material| !material.texture_slots.is_empty()));
        assert!(!file.render.meshes.is_empty());
        assert!(file
            .render
            .meshes
            .iter()
            .all(|mesh| !mesh.positions.is_empty() && !mesh.indices.is_empty()));
        assert_eq!(file.render.bones.len(), 45);
        assert_eq!(file.render.bones[0].parent_index, -1);
        assert!(file
            .render
            .bones
            .iter()
            .skip(1)
            .any(|bone| bone.parent_index >= 0));
        let eyes = file
            .render
            .meshes
            .iter()
            .find(|mesh| mesh.material_index == 1)
            .expect("missing one-bone eye mesh");
        assert_eq!(eyes.vertex_skin_count, 1);
        assert!(eyes
            .bone_indices
            .iter()
            .all(|indices| indices[0] < file.render.bones.len() as u16));
    }

    #[test]
    fn disk_opener_retains_full_source_path() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bfres/Animal_Bull.Bull.bfres");
        if !path.is_file() {
            return;
        }
        let zstd = Arc::new(crate::Zstd::TotkZstd::dictionaryless(
            Arc::new(crate::TotkConfig::TotkConfig::default()),
            crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
        ));
        let (opened, data) = BfresFile::open(&path, zstd).expect("open BFRES from its disk path");
        let expected = path.to_string_lossy();
        assert_eq!(opened.path.full_path, expected);
        assert_eq!(data.path.full_path, expected);
    }

    #[cfg(windows)]
    #[test]
    fn decompresses_mcpk_before_parsing_bfres() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/mcpk");
        let mut parsed = 0;
        for entry in fs::read_dir(corpus).expect("missing MCPK corpus") {
            let path = entry.expect("invalid corpus entry").path();
            if path.extension().and_then(|value| value.to_str()) != Some("mc") {
                continue;
            }
            let bfres = BfresFile::from_path(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(!bfres.sections.is_empty(), "{}", path.display());
            parsed += 1;
        }
        assert!(parsed >= 4, "MCPK corpus is unexpectedly incomplete");
    }

    #[test]
    fn decompresses_zstandard_before_parsing_bfres() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bfres/Animal_Bull.Bull.bfres");
        if !path.is_file() {
            return;
        }
        let source = fs::read(path).unwrap();
        let codec = crate::Zstd::TotkZstd::dictionaryless(
            std::sync::Arc::new(crate::TotkConfig::TotkConfig::default()),
            crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
        );
        let compressed = codec
            .compress_with_dictionary(&source, crate::Zstd::ZstdDictionary::Empty)
            .unwrap();
        let bfres = BfresFile::from_bytes(&compressed).unwrap();
        assert!(!bfres.sections.is_empty());
    }

    #[test]
    fn parses_bfres_corpus() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bfres");
        if !corpus.is_dir() {
            return;
        }
        let mut parsed = 0;
        for entry in fs::read_dir(corpus).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|value| value.to_str()) != Some("bfres") {
                continue;
            }
            let bfres = BfresFile::from_path(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(bfres.header.endian, Endian::Little);
            assert!(bfres.sections_with_signature(b"FMDL").next().is_some());
            assert!(bfres.sections_with_signature(b"FSKL").next().is_some());
            assert!(bfres.sections_with_signature(b"FVTX").next().is_some());
            assert!(bfres.sections_with_signature(b"FSHP").next().is_some());
            assert!(bfres.sections_with_signature(b"FMAT").next().is_some());
            assert!(
                !bfres.materials.is_empty(),
                "{} has no decoded materials",
                path.display()
            );
            assert!(
                bfres
                    .materials
                    .iter()
                    .any(|material| !material.texture_slots.is_empty()),
                "{} has no decoded material texture slots",
                path.display()
            );
            assert!(
                !bfres.render.bones.is_empty(),
                "{} has no decoded bones",
                path.display()
            );
            assert!(
                !bfres.render.meshes.is_empty(),
                "{} has no decoded meshes",
                path.display()
            );
            assert!(
                bfres
                    .render
                    .meshes
                    .iter()
                    .all(|mesh| !mesh.positions.is_empty() && !mesh.indices.is_empty()),
                "{} has incomplete geometry",
                path.display()
            );
            assert!(
                bfres.render.meshes.iter().all(|mesh| mesh
                    .indices
                    .iter()
                    .all(|index| *index < mesh.positions.len() as u32)),
                "{} has out-of-range mesh indices",
                path.display()
            );
            assert!(
                bfres
                    .render
                    .meshes
                    .iter()
                    .all(|mesh| mesh.normals.len() == mesh.positions.len()),
                "{} has incomplete vertex normals",
                path.display()
            );
            parsed += 1;
        }
        assert!(parsed > 0, "BFRES corpus is empty");
    }

    #[test]
    fn renames_first_model_and_internal_name_without_changing_geometry() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bfres");
        if !corpus.is_dir() {
            return;
        }
        let new_name = "Custom_Sword_900";
        let mut tested = 0;
        for entry in fs::read_dir(corpus).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|value| value.to_str()) != Some("bfres") {
                continue;
            }
            let source = fs::read(&path).unwrap();
            let before = BfresFile::from_bytes(&source).unwrap();
            let first_model = before
                .sections_with_signature(b"FMDL")
                .next()
                .expect("fixture has no FMDL");
            let pointer_field =
                first_model.offset as usize + if before.header.version[2] <= 8 { 16 } else { 8 };
            let pointer_size = match before.header.target_address_size {
                4 => 4,
                _ => 8,
            };
            let model_string = match pointer_size {
                4 => u32_at(&source, pointer_field, before.header.endian).unwrap() as usize,
                _ => u64_at(&source, pointer_field, before.header.endian).unwrap() as usize,
            };
            let model_string_start =
                res_string_slot(&source, model_string, first_model.name.as_deref().unwrap())
                    .unwrap();
            let before_relocation = parse_relocation_layout(
                &source,
                before.header.relocation_table_offset as usize,
                before.header.endian,
            )
            .unwrap();
            let before_model_references = before_relocation
                .pointer_fields
                .iter()
                .filter(|field| {
                    let target = u64_at(&source, **field, before.header.endian).unwrap() as usize;
                    target == model_string_start || target == model_string_start + 2
                })
                .count();
            let renamed = BfresFile::rename_first_model(&source, new_name)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let after = BfresFile::from_bytes(&renamed).unwrap();

            assert_eq!(after.name.as_deref(), Some(new_name), "{}", path.display());
            assert_eq!(
                after
                    .sections_with_signature(b"FMDL")
                    .next()
                    .and_then(|section| section.name.as_deref()),
                Some(new_name),
                "{}",
                path.display()
            );
            let after_model_pointer = match pointer_size {
                4 => u32_at(&renamed, pointer_field, after.header.endian).unwrap() as usize,
                _ => u64_at(&renamed, pointer_field, after.header.endian).unwrap() as usize,
            };
            assert_eq!(after_model_pointer, model_string_start);
            let after_model_string_start =
                res_string_slot(&renamed, after_model_pointer, new_name).unwrap();
            let after_relocation = parse_relocation_layout(
                &renamed,
                after.header.relocation_table_offset as usize,
                after.header.endian,
            )
            .unwrap();
            let after_model_references = after_relocation
                .pointer_fields
                .iter()
                .filter(|field| {
                    let target = u64_at(&renamed, **field, after.header.endian).unwrap() as usize;
                    target == after_model_string_start || target == after_model_string_start + 2
                })
                .count();
            assert!(
                before_model_references >= 2,
                "{} model name is not referenced by both FMDL and dictionary",
                path.display()
            );
            assert_eq!(
                after_model_references,
                before_model_references,
                "{} lost a relocated model-name reference",
                path.display()
            );
            assert_eq!(after.render.bones.len(), before.render.bones.len());
            assert_eq!(after.render.meshes.len(), before.render.meshes.len());
            assert!(after.render.meshes.iter().zip(&before.render.meshes).all(
                |(after, before)| after.name == before.name
                    && after.positions.len() == before.positions.len()
                    && after.indices == before.indices
            ));
            assert_eq!(
                after.materials,
                before.materials,
                "{} materials changed",
                path.display()
            );
            assert_eq!(
                after
                    .sections
                    .iter()
                    .map(|section| (section.signature, section.offset))
                    .collect::<Vec<_>>(),
                before
                    .sections
                    .iter()
                    .map(|section| (section.signature, section.offset))
                    .collect::<Vec<_>>(),
                "{} section layout changed",
                path.display()
            );

            let expected_delta = 2 * new_name.len() as isize
                - first_model.name.as_deref().unwrap().len() as isize
                - before.name.as_deref().unwrap().len() as isize;
            assert_eq!(
                renamed.len() as isize,
                source.len() as isize + expected_delta
            );
            tested += 1;
        }
        assert_eq!(tested, 7, "unexpected BFRES fixture count");
    }

    #[test]
    fn resizes_bfres_names_by_exact_length_difference() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bfres");
        if !corpus.is_dir() {
            return;
        }
        let names = ["X", "Custom_Sword_Model_900"];
        let mut tested = 0;
        for entry in fs::read_dir(corpus).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|value| value.to_str()) != Some("bfres") {
                continue;
            }
            let source = fs::read(&path).unwrap();
            let before = BfresFile::from_bytes(&source).unwrap();
            let old_model_len = before
                .sections_with_signature(b"FMDL")
                .next()
                .and_then(|section| section.name.as_deref())
                .unwrap()
                .len();
            let old_internal_len = before.name.as_deref().unwrap().len();
            for name in names {
                let renamed = BfresFile::rename_first_model(&source, name)
                    .unwrap_or_else(|error| panic!("{} ({name}): {error}", path.display()));
                let after = BfresFile::from_bytes(&renamed).unwrap();
                let expected_delta =
                    2 * name.len() as isize - old_model_len as isize - old_internal_len as isize;
                assert_eq!(
                    renamed.len() as isize,
                    source.len() as isize + expected_delta,
                    "{} ({name})",
                    path.display()
                );
                assert_eq!(after.name.as_deref(), Some(name));
                assert_eq!(
                    after
                        .sections_with_signature(b"FMDL")
                        .next()
                        .and_then(|section| section.name.as_deref()),
                    Some(name)
                );
                assert_eq!(after.materials, before.materials);
                assert_eq!(after.render.bones.len(), before.render.bones.len());
                assert_eq!(after.render.meshes.len(), before.render.meshes.len());
                assert!(after.render.meshes.iter().zip(&before.render.meshes).all(
                    |(after, before)| after.name == before.name
                        && after.positions.len() == before.positions.len()
                        && after.indices == before.indices
                ));
            }
            tested += 1;
        }
        assert_eq!(tested, 7, "unexpected BFRES fixture count");
    }

    #[test]
    fn renames_material_texture_slots_without_changing_geometry() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bfres");
        if !corpus.is_dir() {
            return;
        }
        let mut tested = 0;
        for entry in fs::read_dir(corpus).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|value| value.to_str()) != Some("bfres") {
                continue;
            }
            let source = fs::read(&path).unwrap();
            let before = BfresFile::from_bytes(&source).unwrap();
            let Some(base) = before
                .materials
                .iter()
                .flat_map(|material| &material.texture_slots)
                .map(|slot| slot.name.clone())
                .next()
            else {
                continue;
            };
            let custom = "Custom_Texture_900";
            let renamed = BfresFile::rename_material_texture_slots(&source, &base, custom)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let after = BfresFile::from_bytes(&renamed).unwrap();
            assert!(after
                .materials
                .iter()
                .flat_map(|material| &material.texture_slots)
                .any(|slot| slot.name == custom));
            assert_eq!(after.render.bones, before.render.bones);
            assert_eq!(after.render.matrix_to_bone, before.render.matrix_to_bone);
            assert!(after.render.meshes.iter().zip(&before.render.meshes).all(
                |(after, before)| after.name == before.name
                    && after.positions.len() == before.positions.len()
                    && after.indices == before.indices
            ));
            assert_eq!(after.materials.len(), before.materials.len());
            tested += 1;
        }
        assert!(tested > 0, "BFRES corpus has no material textures");
    }

    #[test]
    fn bull_skinning_uses_the_matrix_palette_and_head_bone() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bfres/Animal_Bull.Bull.bfres");
        if !path.is_file() {
            return;
        }
        let bfres = BfresFile::from_path(path).unwrap();
        let body = bfres
            .render
            .meshes
            .iter()
            .find(|mesh| mesh.name.starts_with("Body"))
            .unwrap();
        assert!(body
            .bone_indices
            .iter()
            .zip(&body.bone_weights)
            .all(|(indices, weights)| (0..body.vertex_skin_count as usize)
                .all(|index| weights[index] <= 0.0001 || indices[index] != 0)));
        for name in ["Eye", "Horn"] {
            let mesh = bfres
                .render
                .meshes
                .iter()
                .find(|mesh| mesh.name.starts_with(name))
                .unwrap();
            assert!(mesh.bone_indices.iter().all(|indices| indices[0] == 14));
        }
        assert!(bfres
            .render
            .bones
            .iter()
            .all(|bone| bone.rotation_mode == "euler_xyz"));
    }
}
