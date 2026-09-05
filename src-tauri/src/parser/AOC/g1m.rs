use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
};

use crate::{
    file_format::Model3D::bfres::{BfresBone, BfresMesh, BfresRenderGraph},
    parser::{
        binary::{BinaryReader, Endian},
        AOC::{g1t::G1tFile, kidsobj::KidsObjFile, ktid::KtidFile},
    },
};

#[path = "g1m_import_task.rs"]
mod g1m_import_task;
use g1m_import_task::G1mImportTask;

const PARALLEL_IMPORT_MIN_BYTES: usize = 1024 * 1024;

static PAIRS: LazyLock<HashMap<String, serde_json::Value>> = LazyLock::new(|| {
    serde_json::from_str(&crate::utils::LookupData::read_support_json(
        "G1M_to_G1T_pairs.json",
    ))
    .unwrap_or_default()
});

static BOTW_BONE_NAMES: LazyLock<HashMap<usize, String>> = LazyLock::new(|| {
    serde_json::from_str::<HashMap<String, String>>(&crate::utils::LookupData::read_support_json(
        "bones_botw.json",
    ))
    .unwrap_or_default()
    .into_iter()
    .filter_map(|(bone, name)| {
        bone.strip_prefix("bone_")
            .and_then(|index| index.parse().ok())
            .map(|index| (index, name))
    })
    .collect()
});

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AocModelEntry {
    pub name: String,
    pub size: u64,
}

static AOC_NAMES: LazyLock<HashMap<String, AocModelEntry>> = LazyLock::new(|| {
    serde_json::from_str(&crate::utils::LookupData::read_support_json(
        "AOC_names.json",
    ))
    .unwrap_or_default()
});

pub(crate) fn aoc_names() -> &'static HashMap<String, AocModelEntry> {
    &AOC_NAMES
}

pub(crate) fn model_texture_pairs() -> &'static HashMap<String, serde_json::Value> {
    &PAIRS
}

pub(crate) fn bone_name(index: usize) -> String {
    BOTW_BONE_NAMES
        .get(&index)
        .cloned()
        .unwrap_or_else(|| format!("bone_{index}"))
}

pub(crate) fn bone_index(name: &str) -> Option<usize> {
    BOTW_BONE_NAMES
        .iter()
        .find_map(|(index, candidate)| (candidate == name).then_some(*index))
        .or_else(|| name.strip_prefix("bone_")?.parse().ok())
}

static KIDSOBJ_CACHE: LazyLock<Mutex<HashMap<PathBuf, Arc<KidsObjFile>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize)]
pub struct G1mHeader {
    pub version: [u8; 4],
    pub endian: String,
    pub target_address_size: u8,
    pub alignment_exponent: u8,
    pub file_size: u64,
    pub string_pool_offset: u64,
    pub string_pool_size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct G1mSection {
    pub signature: [u8; 4],
    pub offset: u64,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct G1mTextureSlot {
    pub index: usize,
    pub name: String,
    pub uv_layer: u16,
    pub sampler: String,
    pub texture_type: String,
}

impl G1mTextureSlot {
    /// The slot feeding the base colour, whichever label the source format
    /// uses for it (G1M says `Diffuse`, LM3 says `Base color`).
    pub fn is_diffuse(&self) -> bool {
        matches!(self.texture_type.as_str(), "Diffuse" | "Base color")
    }

    pub fn is_normal(&self) -> bool {
        self.texture_type == "Normal"
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct G1mMaterial {
    pub name: String,
    pub offset: u64,
    pub texture_slots: Vec<G1mTextureSlot>,
}

#[derive(Debug, Clone, Serialize)]
pub struct G1mFile {
    pub header: G1mHeader,
    pub model_hash: String,
    pub global_to_local_bones: Vec<u16>,
    pub name: Option<String>,
    pub sections: Vec<G1mSection>,
    pub materials: Vec<G1mMaterial>,
    pub render: BfresRenderGraph,
    pub format: String,
}

/// The slice of a parsed model the FBX and glTF exporters consume. G1M and
/// LM3 models both carry it, so one exporter serves both formats.
#[derive(Clone, Copy)]
pub struct ExportModel<'a> {
    pub materials: &'a [G1mMaterial],
    pub render: &'a BfresRenderGraph,
    /// Whether meshes skinned to a single bone store their vertices in that
    /// bone's space (G1M) rather than in model space (LM3). The exporters
    /// bake the bone's bind transform into the former so every mesh lands in
    /// model space; doing that to the latter scatters them.
    pub rigid_meshes_in_bone_space: bool,
}

impl G1mFile {
    pub fn export_model(&self) -> ExportModel<'_> {
        ExportModel {
            materials: &self.materials,
            render: &self.render,
            rigid_meshes_in_bone_space: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedG1tTexture {
    pub name: String,
    pub aliases: Vec<String>,
    pub path: String,
    pub source: String,
    pub data_url: String,
    pub width: u32,
    pub height: u32,
    pub array_count: u32,
    pub renderable: bool,
    pub data_urls: Vec<String>,
}

pub struct G1mTextureResolution {
    pub textures: Vec<ResolvedG1tTexture>,
    pub total: usize,
    pub skipped: usize,
}

#[derive(Clone)]
struct VertexBuffer {
    offset: usize,
    stride: usize,
    count: usize,
}
#[derive(Clone)]
struct Attribute {
    buffer: usize,
    offset: usize,
    data_type: u8,
    semantic: u8,
    layer: u8,
}
#[derive(Clone, Default)]
struct AttributeSet {
    buffers: Vec<usize>,
    attributes: Vec<Attribute>,
}
#[derive(Clone)]
struct IndexBuffer {
    offset: usize,
    count: usize,
    stride: usize,
}
#[derive(Clone, Default)]
struct Submesh {
    submesh_type: u32,
    vertex_buffer: usize,
    bone_palette: usize,
    bone_index: u32,
    material: u32,
    index_buffer: usize,
    primitive: u32,
    index_offset: usize,
    index_count: usize,
}
#[derive(Default)]
struct Geometry {
    materials: Vec<G1mMaterial>,
    vertex_buffers: Vec<VertexBuffer>,
    attributes: Vec<AttributeSet>,
    palettes: Vec<Vec<u32>>,
    index_buffers: Vec<IndexBuffer>,
    submeshes: Vec<Submesh>,
    visible_submeshes: Option<BTreeSet<usize>>,
    cloth_submeshes: HashMap<usize, (u16, u32)>,
}

#[derive(Clone)]
struct NunoEntry {
    parent_bone_id: u16,
    control_points: Vec<[f32; 4]>,
    control_parents: Vec<i32>,
}

impl G1mFile {
    pub fn from_path(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        Self::parse(
            &std::fs::read(path)?,
            path.file_stem().and_then(|v| v.to_str()).unwrap_or("G1M"),
        )
    }

    pub fn parse(data: &[u8], name: &str) -> io::Result<Self> {
        Self::parse_internal(data, name, false, None).map(|(model, _)| model)
    }

    pub fn parse_for_export(data: &[u8], name: &str) -> io::Result<Self> {
        Self::parse_internal(data, name, true, None).map(|(model, _)| model)
    }

    pub fn parse_with_textures(
        data: &[u8],
        name: &str,
        source: &Path,
        aoc_path: &Path,
    ) -> io::Result<(Self, G1mTextureResolution)> {
        let (model, textures) = Self::parse_internal(data, name, false, Some((source, aoc_path)))?;
        Ok((
            model,
            textures.unwrap_or(G1mTextureResolution {
                textures: Vec::new(),
                total: 0,
                skipped: 0,
            }),
        ))
    }

    fn parse_internal<'a>(
        data: &'a [u8],
        name: &str,
        include_hidden_meshes: bool,
        texture_paths: Option<(&'a Path, &'a Path)>,
    ) -> io::Result<(Self, Option<G1mTextureResolution>)> {
        let endian = match data.get(..4) {
            Some(b"_M1G") => Endian::Little,
            Some(b"G1M_") => Endian::Big,
            _ => return Err(invalid("not a G1M model")),
        };
        let mut reader = BinaryReader::with_endian(data, endian);
        reader.skip(4)?;
        let version = reader.read_array_at::<4>(4)?;
        reader.skip(4)?;
        let file_size = reader.read_u32()? as usize;
        let chunk_offset = reader.read_u32()? as usize;
        reader.skip(4)?;
        let chunk_count = reader.read_u32()? as usize;
        if file_size > data.len() || chunk_count > 4096 {
            return Err(invalid("invalid G1M header"));
        }
        reader.seek(chunk_offset)?;
        let mut sections = Vec::with_capacity(chunk_count);
        let mut bones = Vec::new();
        let mut bone_ids = Vec::new();
        let mut nuno_entries = Vec::new();
        let mut geometry = None;
        for _ in 0..chunk_count {
            let start = reader.position();
            let mut signature = [0; 4];
            signature.copy_from_slice(reader.read_bytes(4)?);
            reader.skip(4)?;
            let size = reader.read_u32()? as usize;
            if size < 12 || start.checked_add(size).is_none_or(|end| end > data.len()) {
                return Err(invalid("invalid G1M chunk size"));
            }
            sections.push(G1mSection {
                signature,
                offset: start as u64,
                name: None,
            });
            if matches!(&signature, b"G1MS" | b"SM1G") && bones.is_empty() {
                (bones, bone_ids) = parse_skeleton(&data[start..start + size], endian)?;
            } else if matches!(&signature, b"G1MG" | b"GM1G") {
                geometry = Some(parse_geometry(&data[start..start + size], endian, start)?);
            } else if matches!(&signature, b"NUNO" | b"ONUN") {
                nuno_entries = parse_nuno(&data[start..start + size], endian)?;
            }
            reader.seek(start + size)?;
        }
        let geometry = geometry.ok_or_else(|| invalid("G1M has no G1MG geometry chunk"))?;
        let all_slots: BTreeSet<_> = geometry
            .materials
            .iter()
            .flat_map(|material| material.texture_slots.iter())
            .map(|slot| slot.name.clone())
            .collect();
        let task = G1mImportTask::new(data.len(), geometry.submeshes.len());
        let texture_task = texture_paths.map(|(source, aoc_path)| {
            move || resolve_texture_slots(source, aoc_path, &all_slots, true)
        });
        let (meshes, texture_resolution) = task.run(
            |worker, workers| {
                build_meshes(
                    data,
                    &geometry,
                    endian,
                    &bones,
                    &bone_ids,
                    &nuno_entries,
                    include_hidden_meshes,
                    worker,
                    workers,
                )
            },
            texture_task,
        )?;
        let display_name = AOC_NAMES
            .get(&name.to_ascii_lowercase())
            .map(|entry| &entry.name)
            .filter(|value| !value.is_empty())
            .map(|value| value.replace(' ', "_"))
            .unwrap_or_else(|| name.to_owned());
        Ok((
            Self {
                header: G1mHeader {
                    version,
                    endian: format!("{endian:?}"),
                    target_address_size: 4,
                    alignment_exponent: 0,
                    file_size: file_size as u64,
                    string_pool_offset: 0,
                    string_pool_size: 0,
                },
                model_hash: name.to_ascii_lowercase(),
                global_to_local_bones: bone_ids,
                name: Some(display_name),
                sections,
                materials: geometry.materials,
                render: BfresRenderGraph {
                    bones,
                    matrix_to_bone: Vec::new(),
                    meshes,
                },
                format: "G1M".into(),
            },
            texture_resolution,
        ))
    }

    pub fn open(
        path: &Path,
    ) -> Option<(
        crate::file_format::BinTextFile::OpenedFile<'static>,
        crate::Open_and_Save::SendData,
    )> {
        let magic = std::fs::read(path).ok()?;
        if !crate::Settings::Magic::is_g1m(&magic) {
            return None;
        }
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.path = crate::Settings::Pathlib::new(path);
        opened.file_type = crate::Zstd::TotkFileType::Other;
        let mut send = crate::Open_and_Save::SendData::default();
        send.path = crate::Settings::Pathlib::new(path);
        send.file_label = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| AOC_NAMES.get(&stem.to_ascii_lowercase()))
            .map(|entry| &entry.name)
            .filter(|name| !name.trim().is_empty())
            .map(|name| format!("[{}] {} [G1M]", name.trim(), send.path.name))
            .unwrap_or_else(|| format!("{} [G1M]", send.path.name));
        send.file_metadata = "[G1M]".into();
        send.file_type = crate::Zstd::TotkFileType::G1M;
        send.status_text = format!("Opened G1M {}", path.display());
        send.tab = "3D".into();
        send.read_only = false;
        Some((opened, send))
    }

    pub fn open_binary<P: AsRef<Path>>(
        path: P,
        bytes: &[u8],
    ) -> Option<(
        crate::file_format::BinTextFile::OpenedFile<'static>,
        crate::Open_and_Save::SendData,
    )> {
        if !crate::Settings::Magic::is_g1m(bytes) {
            return None;
        }
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.path = crate::Settings::Pathlib::new(&path);
        opened.file_type = crate::Zstd::TotkFileType::Other;
        let mut send = crate::Open_and_Save::SendData::default();
        send.path = crate::Settings::Pathlib::new(path.as_ref());
        send.file_label = send
            .path
            .name
            .split_once('.')
            .map(|(stem, _)| stem)
            .map(|stem| format!("{stem} [G1M]"))
            .unwrap_or_else(|| format!("{} [G1M]", send.path.name));
        send.file_metadata = "[G1M]".into();
        send.file_type = crate::Zstd::TotkFileType::G1M;
        send.status_text = format!("Opened G1M {}", send.path.full_path);
        send.tab = "3D".into();
        send.read_only = false;
        Some((opened, send))
    }

    pub fn resolve_textures(&self, source: &Path, aoc_path: &Path) -> G1mTextureResolution {
        self.resolve_textures_internal(source, aoc_path, true)
    }

    /// Resolves textures at their original dimensions for file export. The
    /// regular resolver intentionally limits preview images to 384 pixels.
    pub fn resolve_textures_for_export(
        &self,
        source: &Path,
        aoc_path: &Path,
    ) -> G1mTextureResolution {
        self.resolve_textures_internal(source, aoc_path, false)
    }

    fn resolve_textures_internal(
        &self,
        source: &Path,
        aoc_path: &Path,
        preview: bool,
    ) -> G1mTextureResolution {
        let all_slots: BTreeSet<_> = self
            .materials
            .iter()
            .flat_map(|material| material.texture_slots.iter())
            .map(|slot| slot.name.clone())
            .collect();
        resolve_texture_slots(source, aoc_path, &all_slots, preview)
    }
}

fn resolve_texture_slots(
    source: &Path,
    aoc_path: &Path,
    all_slots: &BTreeSet<String>,
    preview: bool,
) -> G1mTextureResolution {
    let total = all_slots.len();
    let Some(pair) = paired_texture_info(source) else {
        return G1mTextureResolution {
            textures: Vec::new(),
            total,
            skipped: 0,
        };
    };
    let resolved = resolve_kidsobj_textures(source, aoc_path, &pair, &all_slots, preview);
    if !resolved.is_empty() {
        return G1mTextureResolution {
            textures: resolved,
            total,
            skipped: 0,
        };
    }
    // Standalone dumps sometimes place the paired value directly beside
    // the model as a G1T without KTID/KidsObj metadata.
    pair.ktid_hash
        .as_deref()
        .and_then(|hash| resolve_direct_g1t(source, aoc_path, hash, &all_slots, preview))
        .unwrap_or(G1mTextureResolution {
            textures: Vec::new(),
            total,
            skipped: 0,
        })
}

fn parse_skeleton(data: &[u8], endian: Endian) -> io::Result<(Vec<BfresBone>, Vec<u16>)> {
    let mut reader = BinaryReader::with_endian(data, endian);
    reader.skip(12)?;
    let joint_offset = reader.read_u32()? as usize;
    reader.skip(4)?;
    let joint_count = reader.read_u16()? as usize;
    let index_count = reader.read_u16()? as usize;
    reader.skip(4)?;
    let bone_ids = (0..index_count)
        .map(|_| reader.read_u16())
        .collect::<io::Result<Vec<_>>>()?;
    reader.seek(joint_offset)?;
    let mut bones = Vec::with_capacity(joint_count);
    for index in 0..joint_count {
        let scale = [reader.read_f32()?, reader.read_f32()?, reader.read_f32()?];
        let parent = reader.read_i32()?;
        let rotation = [
            reader.read_f32()?,
            reader.read_f32()?,
            reader.read_f32()?,
            reader.read_f32()?,
        ];
        let translation = [reader.read_f32()?, reader.read_f32()?, reader.read_f32()?];
        reader.skip(4)?;
        let global_index = bone_ids
            .iter()
            .position(|local| usize::from(*local) == index)
            .unwrap_or(index);
        bones.push(BfresBone {
            name: bone_name(global_index),
            parent_index: i16::try_from(parent).unwrap_or(-1),
            smooth_matrix_index: -1,
            rigid_matrix_index: -1,
            rotation_mode: "quaternion".into(),
            scale,
            rotation,
            translation,
        });
    }
    Ok((bones, bone_ids))
}

fn parse_nuno(data: &[u8], endian: Endian) -> io::Result<Vec<NunoEntry>> {
    let mut reader = BinaryReader::with_endian(data, endian);
    reader.skip(4)?;
    let version = reader.read_u32()?;
    reader.skip(4)?;
    let chunk_count = reader.read_u32()? as usize;
    let mut entries = Vec::new();
    for _ in 0..chunk_count {
        let chunk_start = reader.position();
        let kind = reader.read_u32()?;
        let chunk_size = reader.read_u32()? as usize;
        let entry_count = reader.read_u32()? as usize;
        if version >= 0x3030_3335 {
            reader.skip(4)?;
        }
        if kind == 0x0003_0001 {
            entries.reserve(entry_count);
            for _ in 0..entry_count {
                let parent_bone_id = reader.read_u16()?;
                reader.skip(2)?;
                let control_count = reader.read_u32()? as usize;
                let unknown_count = reader.read_u32()? as usize;
                let skip_1 = reader.read_u32()? as usize;
                let skip_2 = reader.read_u32()? as usize;
                let skip_3 = reader.read_u32()? as usize;
                reader.skip(0x3c)?;
                if version > 0x3030_3233 {
                    reader.skip(0x10)?;
                }
                if version >= 0x3030_3235 {
                    reader.skip(0x10)?;
                }
                let control_points = (0..control_count)
                    .map(|_| {
                        Ok([
                            reader.read_f32()?,
                            reader.read_f32()?,
                            reader.read_f32()?,
                            reader.read_f32()?,
                        ])
                    })
                    .collect::<io::Result<Vec<_>>>()?;
                let control_parents = (0..control_count)
                    .map(|_| {
                        reader.read_i32()?;
                        reader.read_i32()?;
                        let parent = reader.read_i32()?;
                        reader.read_i32()?;
                        reader.skip(8)?;
                        Ok(parent)
                    })
                    .collect::<io::Result<Vec<_>>>()?;
                reader.skip(unknown_count.saturating_mul(48))?;
                reader.skip(skip_1.saturating_mul(4))?;
                reader.skip(skip_2.saturating_mul(4))?;
                reader.skip(skip_3.saturating_mul(4))?;
                entries.push(NunoEntry {
                    parent_bone_id,
                    control_points,
                    control_parents,
                });
            }
        }
        let chunk_end = chunk_start.saturating_add(chunk_size);
        if chunk_end > data.len() {
            return Err(invalid("NUNO chunk exceeds file bounds"));
        }
        reader.seek(chunk_end)?;
    }
    Ok(entries)
}

fn parse_geometry(data: &[u8], endian: Endian, file_offset: usize) -> io::Result<Geometry> {
    let mut reader = BinaryReader::with_endian(data, endian);
    reader.skip(12 + 4 + 4 + 24)?;
    let section_count = reader.read_u32()? as usize;
    let version = BinaryReader::with_endian(data, endian).read_u32_at(4)?;
    let mut geometry = Geometry::default();
    for _ in 0..section_count {
        let section_start = reader.position();
        let kind = reader.read_u32()?;
        let size = reader.read_u32()? as usize;
        let count = reader.read_u32()? as usize;
        let section_end = section_start
            .checked_add(size)
            .ok_or_else(|| invalid("section overflow"))?;
        match kind {
            0x0001_0002 => {
                geometry.materials.reserve(count);
                for material_index in 0..count {
                    reader.skip(4)?;
                    let texture_count = reader.read_u32()? as usize;
                    reader.skip(8)?;
                    let mut slots = Vec::with_capacity(texture_count);
                    for slot_index in 0..texture_count {
                        let id = reader.read_u16()?;
                        let layer = reader.read_u16()?;
                        let texture_type = reader.read_u16()?;
                        let subtype = reader.read_u16()?;
                        reader.skip(4)?;
                        let (sampler, classification) = classify_texture(texture_type, subtype);
                        slots.push(G1mTextureSlot {
                            index: slot_index,
                            name: id.to_string(),
                            uv_layer: layer,
                            sampler,
                            texture_type: classification,
                        });
                    }
                    geometry.materials.push(G1mMaterial {
                        name: format!("Material {material_index}"),
                        offset: section_start as u64,
                        texture_slots: slots,
                    });
                }
            }
            0x0001_0004 => {
                geometry.vertex_buffers.reserve(count);
                for _ in 0..count {
                    reader.skip(4)?;
                    let stride = reader.read_u32()? as usize;
                    let vertex_count = reader.read_u32()? as usize;
                    if version > 0x3030_3430 {
                        reader.skip(4)?;
                    }
                    let offset = file_offset + reader.position();
                    geometry.vertex_buffers.push(VertexBuffer {
                        offset,
                        stride,
                        count: vertex_count,
                    });
                    reader.skip(
                        stride
                            .checked_mul(vertex_count)
                            .ok_or_else(|| invalid("vertex buffer overflow"))?,
                    )?;
                }
            }
            0x0001_0005 => {
                geometry.attributes.reserve(count);
                for _ in 0..count {
                    let list_count = reader.read_u32()? as usize;
                    let mut buffers = Vec::with_capacity(list_count);
                    for _ in 0..list_count {
                        buffers.push(reader.read_u32()? as usize);
                    }
                    let attr_count = reader.read_u32()? as usize;
                    let mut attributes = Vec::with_capacity(attr_count);
                    for _ in 0..attr_count {
                        let buffer = reader.read_u16()? as usize;
                        let offset = reader.read_u16()? as usize;
                        let data_type = reader.read_u8()?;
                        reader.skip(1)?;
                        let semantic = reader.read_u8()?;
                        let layer = reader.read_u8()?;
                        attributes.push(Attribute {
                            buffer,
                            offset,
                            data_type,
                            semantic,
                            layer,
                        });
                    }
                    geometry.attributes.push(AttributeSet {
                        buffers,
                        attributes,
                    });
                }
            }
            0x0001_0006 => {
                geometry.palettes.reserve(count);
                for _ in 0..count {
                    let joint_count = reader.read_u32()? as usize;
                    let mut palette = Vec::with_capacity(joint_count);
                    for _ in 0..joint_count {
                        reader.skip(8)?;
                        palette.push(reader.read_u32()? & 0x7fff_ffff);
                    }
                    geometry.palettes.push(palette);
                }
            }
            0x0001_0007 => {
                geometry.index_buffers.reserve(count);
                for _ in 0..count {
                    let index_count = reader.read_u32()? as usize;
                    let bits = reader.read_u32()? as usize;
                    if version > 0x3030_3430 {
                        reader.skip(4)?;
                    }
                    let stride = bits / 8;
                    let offset = file_offset + reader.position();
                    geometry.index_buffers.push(IndexBuffer {
                        offset,
                        count: index_count,
                        stride,
                    });
                    reader.skip(
                        stride
                            .checked_mul(index_count)
                            .ok_or_else(|| invalid("index buffer overflow"))?,
                    )?;
                    reader.align(4)?;
                }
            }
            0x0001_0008 => {
                geometry.submeshes.reserve(count);
                for _ in 0..count {
                    let mut values = [0; 14];
                    for value in &mut values {
                        *value = reader.read_u32()?;
                    }
                    geometry.submeshes.push(Submesh {
                        submesh_type: values[0],
                        vertex_buffer: values[1] as usize,
                        bone_palette: values[2] as usize,
                        bone_index: values[3],
                        material: values[6],
                        index_buffer: values[7] as usize,
                        primitive: values[9],
                        index_offset: values[12] as usize,
                        index_count: values[13] as usize,
                    });
                }
            }
            0x0001_0009 => {
                for group_index in 0..count {
                    let (submesh_count_1, submesh_count_2) = if version > 0x3030_3330 {
                        reader.skip(12)?;
                        let first = reader.read_u32()? as usize;
                        let second = reader.read_u32()? as usize;
                        if version > 0x3030_3430 {
                            reader.skip(16)?;
                        }
                        (first, second)
                    } else {
                        reader.skip(4)?;
                        let first = reader.read_u32()? as usize;
                        let second = reader.read_u32()? as usize;
                        (first, second)
                    };
                    for _ in 0..submesh_count_1.saturating_add(submesh_count_2) {
                        reader.skip(16)?;
                        let cloth_id = reader.read_u16()?;
                        reader.skip(2)?;
                        let nun_id = reader.read_u32()?;
                        let index_count = reader.read_u32()? as usize;
                        if group_index == 0 {
                            let visible = geometry.visible_submeshes.get_or_insert_default();
                            for _ in 0..index_count {
                                let submesh_index = reader.read_u32()? as usize;
                                visible.insert(submesh_index);
                                if cloth_id != 0 {
                                    geometry
                                        .cloth_submeshes
                                        .insert(submesh_index, (cloth_id, nun_id));
                                }
                            }
                        } else if index_count == 0 {
                            reader.skip(4)?;
                        } else {
                            reader.skip(index_count.saturating_mul(4))?;
                        }
                        if group_index == 0 && index_count == 0 {
                            reader.skip(4)?;
                        }
                    }
                }
            }
            _ => {}
        }
        if section_end > data.len() {
            return Err(invalid("G1MG section exceeds chunk"));
        }
        reader.seek(section_end)?;
    }
    Ok(geometry)
}

fn build_meshes(
    data: &[u8],
    geometry: &Geometry,
    endian: Endian,
    bones: &[BfresBone],
    bone_ids: &[u16],
    nuno_entries: &[NunoEntry],
    include_hidden_meshes: bool,
    worker: usize,
    workers: usize,
) -> io::Result<Vec<BfresMesh>> {
    let source = BinaryReader::with_endian(data, endian);
    let mut meshes = Vec::with_capacity(geometry.submeshes.len());
    let per_worker = geometry.submeshes.len().div_ceil(workers);
    let worker_start = worker * per_worker;
    let worker_end = (worker_start + per_worker).min(geometry.submeshes.len());
    for (mesh_index, submesh) in geometry.submeshes.iter().enumerate() {
        if mesh_index < worker_start || mesh_index >= worker_end {
            continue;
        }
        if !include_hidden_meshes
            && geometry
                .visible_submeshes
                .as_ref()
                .is_some_and(|visible| !visible.contains(&mesh_index))
        {
            continue;
        }
        let Some(attrs) = geometry.attributes.get(submesh.vertex_buffer) else {
            continue;
        };
        let Some(index_buffer) = geometry.index_buffers.get(submesh.index_buffer) else {
            continue;
        };
        let mut raw_indices = Vec::with_capacity(submesh.index_count);
        for index in submesh.index_offset
            ..submesh
                .index_offset
                .saturating_add(submesh.index_count)
                .min(index_buffer.count)
        {
            let offset = index_buffer.offset + index * index_buffer.stride;
            raw_indices.push(match index_buffer.stride {
                2 => source.read_u16_at(offset)? as u32,
                4 => source.read_u32_at(offset)?,
                _ => return Err(invalid("unsupported G1M index width")),
            });
        }
        let mut indices = if submesh.primitive == 4 {
            strip_to_triangles(&raw_indices)
        } else {
            raw_indices
        };
        if indices.is_empty() {
            continue;
        }
        let vertex_count = attrs
            .buffers
            .iter()
            .filter_map(|index| geometry.vertex_buffers.get(*index).map(|v| v.count))
            .min()
            .unwrap_or(0);
        let mut positions = vec![[0.0; 3]; vertex_count];
        let mut position4 = vec![[0.0; 4]; vertex_count];
        let mut normals = vec![[0.0; 3]; vertex_count];
        let mut normal4 = vec![[0.0; 4]; vertex_count];
        let mut uv_maps: Vec<Vec<[f32; 2]>> = Vec::with_capacity(attrs.attributes.len());
        let mut colors = vec![[1.0; 4]; vertex_count];
        let mut bone_indices = vec![[0; 4]; vertex_count];
        let mut bone_weights = vec![[0.0; 4]; vertex_count];
        let mut bone_index_layers: Vec<Vec<[u16; 4]>> = Vec::new();
        let mut bone_weight_layers: Vec<Vec<[f32; 4]>> = Vec::new();
        let mut has_bone_indices = false;
        let mut blend_controls = vec![[0.0; 4]; vertex_count];
        let mut cloth_psize = vec![[0.0; 4]; vertex_count];
        let mut cloth_texcoord = vec![[0.0; 4]; vertex_count];
        let mut cloth_binormal = vec![[0.0; 4]; vertex_count];
        let mut cloth_fog = vec![[0.0; 4]; vertex_count];
        let mut cloth_color = vec![[0.0; 4]; vertex_count];
        for attribute in &attrs.attributes {
            let Some(buffer_index) = attrs.buffers.get(attribute.buffer).copied() else {
                continue;
            };
            let Some(buffer) = geometry.vertex_buffers.get(buffer_index) else {
                continue;
            };
            let is_uv =
                attribute.semantic == 5 && matches!(attribute.data_type, 0x01 | 0x03 | 0x0a | 0x0b);
            if is_uv {
                let highest_layer = if matches!(attribute.data_type, 0x03 | 0x0b) {
                    1
                } else {
                    attribute.layer as usize
                };
                while uv_maps.len() <= highest_layer {
                    uv_maps.push(vec![[0.0; 2]; vertex_count]);
                }
            }
            for vertex in 0..vertex_count.min(buffer.count) {
                let values = read_attribute(
                    &source,
                    buffer.offset + vertex * buffer.stride + attribute.offset,
                    attribute.data_type,
                )?;
                match attribute.semantic {
                    0 => {
                        positions[vertex] = [values[0], values[1], values[2]];
                        position4[vertex] = values;
                    }
                    1 => {
                        let layer = attribute.layer as usize;
                        while bone_weight_layers.len() <= layer {
                            bone_weight_layers.push(vec![[0.0; 4]; vertex_count]);
                        }
                        bone_weight_layers[layer][vertex] = values;
                    }
                    2 => {
                        has_bone_indices = true;
                        let layer = attribute.layer as usize;
                        while bone_index_layers.len() <= layer {
                            bone_index_layers.push(vec![[0; 4]; vertex_count]);
                        }
                        if layer == 0 {
                            blend_controls[vertex] = values;
                        }
                        let palette = geometry.palettes.get(submesh.bone_palette);
                        for component in 0..4 {
                            let local = values[component] as usize / 3;
                            let joint = palette
                                .and_then(|p| p.get(local))
                                .copied()
                                .unwrap_or(local as u32);
                            bone_index_layers[layer][vertex][component] = joint as u16;
                        }
                    }
                    3 => {
                        normals[vertex] = [values[0], values[1], values[2]];
                        normal4[vertex] = values;
                    }
                    4 => cloth_psize[vertex] = values,
                    5 if attribute.layer > 2 => cloth_texcoord[vertex] = values,
                    5 if is_uv => {
                        if matches!(attribute.data_type, 0x03 | 0x0b) {
                            uv_maps[0][vertex] = [values[0], values[1]];
                            uv_maps[1][vertex] = [values[2], values[3]];
                        } else {
                            uv_maps[attribute.layer as usize][vertex] = [values[0], values[1]];
                        }
                    }
                    7 => cloth_binormal[vertex] = values,
                    10 if attribute.layer == 0 => colors[vertex] = values,
                    10 => cloth_color[vertex] = values,
                    11 => cloth_fog[vertex] = values,
                    _ => {}
                }
            }
        }
        let is_rigid = submesh.submesh_type & 0x2 != 0;
        if has_bone_indices {
            collapse_skin_influences(
                &bone_index_layers,
                &bone_weight_layers,
                &mut bone_indices,
                &mut bone_weights,
            );
        }
        let rigid_joint = geometry
            .palettes
            .get(submesh.bone_palette)
            .and_then(|palette| palette.first())
            .copied()
            .unwrap_or(submesh.bone_index);
        let rigid_bone = rigid_joint as u16;
        let vertex_skin_count = if is_rigid {
            // Project-G1M binds rigid submeshes to local palette index zero,
            // overriding skin attributes that may exist in a shared buffer.
            bone_indices.fill([rigid_bone; 4]);
            bone_weights.fill([1.0, 0.0, 0.0, 0.0]);
            1
        } else if has_bone_indices {
            // A single non-zero weight does not make a G1M submesh rigid.
            // Non-rigid vertices remain in model bind space even when every
            // vertex happens to use only one influence. Only submesh_type bit
            // 1 identifies bone-local geometry that needs a rest transform.
            4
        } else {
            0
        };
        if let Some((cloth_id, nun_id)) = geometry.cloth_submeshes.get(&mesh_index) {
            if *cloth_id == 1 {
                let nuno_index = (*nun_id % 10_000) as usize;
                if let Some(entry) = nuno_entries.get(nuno_index) {
                    // NUNO controls and weights are layer-paired. The merged
                    // skin weights are correct for rendering/export, but the
                    // cloth position solver must use the layer-zero weights
                    // alongside the layer-zero controls captured above.
                    let cloth_weights = bone_weight_layers
                        .first()
                        .map(Vec::as_slice)
                        .unwrap_or(&bone_weights);
                    reconstruct_nuno_mesh(
                        &mut positions,
                        &mut normals,
                        &position4,
                        &normal4,
                        &blend_controls,
                        cloth_weights,
                        &cloth_psize,
                        &cloth_texcoord,
                        &cloth_binormal,
                        &cloth_fog,
                        &cloth_color,
                        entry,
                        bones,
                        bone_ids,
                    );
                }
            }
        }
        // G1M files commonly carry placeholder TEXCOORD layers. A layer whose
        // vertices all collapse onto one coordinate cannot map a 2D texture;
        // discard it so the first exposed layer is the first useful UV set.
        uv_maps.retain(|uv_map| useful_uv_map(uv_map));
        if indices
            .iter()
            .all(|index| (*index as usize) < positions.len())
        {
            let mut remap = HashMap::<u32, u32>::new();
            let mut used = Vec::new();
            for index in &mut indices {
                let next = remap.len() as u32;
                let mapped = *remap.entry(*index).or_insert_with(|| {
                    used.push(*index as usize);
                    next
                });
                *index = mapped;
            }
            retain_vertices(&mut positions, &used);
            retain_vertices(&mut normals, &used);
            retain_vertices(&mut colors, &used);
            retain_vertices(&mut bone_indices, &used);
            retain_vertices(&mut bone_weights, &used);
            for uv_map in &mut uv_maps {
                retain_vertices(uv_map, &used);
            }
        }
        let skin_bones = bone_indices
            .iter()
            .flatten()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let uv0 = uv_maps.first().cloned().unwrap_or_default();
        meshes.push(BfresMesh {
            name: format!("Mesh {mesh_index}"),
            material_index: submesh.material as u16,
            bone_index: if is_rigid {
                rigid_bone
            } else {
                submesh.bone_index as u16
            },
            vertex_skin_count,
            is_cloth: geometry.cloth_submeshes.contains_key(&mesh_index),
            cloth_id: geometry
                .cloth_submeshes
                .get(&mesh_index)
                .map_or(0, |value| value.0),
            nun_id: geometry
                .cloth_submeshes
                .get(&mesh_index)
                .map_or(0, |value| value.1),
            positions,
            normals,
            uv0,
            uv_maps,
            colors,
            bone_indices,
            bone_weights,
            indices,
            skin_bones,
        });
    }
    Ok(meshes)
}

fn retain_vertices<T: Copy>(values: &mut Vec<T>, used: &[usize]) {
    *values = used
        .iter()
        .filter_map(|index| values.get(*index).copied())
        .collect();
}

fn collapse_skin_influences(
    index_layers: &[Vec<[u16; 4]>],
    weight_layers: &[Vec<[f32; 4]>],
    indices: &mut [[u16; 4]],
    weights: &mut [[f32; 4]],
) {
    for vertex in 0..indices.len() {
        let mut combined = std::collections::BTreeMap::<u16, f32>::new();
        for (layer, layer_indices) in index_layers.iter().enumerate() {
            let Some(vertex_indices) = layer_indices.get(vertex) else {
                continue;
            };
            let vertex_weights = weight_layers
                .get(layer)
                .and_then(|values| values.get(vertex));
            for influence in 0..4 {
                let weight = vertex_weights.map_or(
                    if layer == 0 && influence == 0 {
                        1.0
                    } else {
                        0.0
                    },
                    |values| values[influence],
                );
                if weight.is_finite() && weight > 0.0 {
                    *combined.entry(vertex_indices[influence]).or_default() += weight;
                }
            }
        }
        let mut combined: Vec<_> = combined.into_iter().collect();
        combined.sort_by(|left, right| right.1.total_cmp(&left.1));
        let total: f32 = combined.iter().take(4).map(|(_, weight)| weight).sum();
        indices[vertex] = [0; 4];
        weights[vertex] = [0.0; 4];
        if total > f32::EPSILON {
            for (influence, (bone, weight)) in combined.into_iter().take(4).enumerate() {
                indices[vertex][influence] = bone;
                weights[vertex][influence] = weight / total;
            }
        } else {
            indices[vertex][0] = index_layers
                .first()
                .and_then(|layer| layer.get(vertex))
                .map_or(0, |value| value[0]);
            weights[vertex][0] = 1.0;
        }
    }
}

fn reconstruct_nuno_mesh(
    positions: &mut [[f32; 3]],
    normals: &mut [[f32; 3]],
    position4: &[[f32; 4]],
    normal4: &[[f32; 4]],
    blend_controls: &[[f32; 4]],
    blend_weights: &[[f32; 4]],
    psize: &[[f32; 4]],
    texcoord: &[[f32; 4]],
    binormal: &[[f32; 4]],
    fog: &[[f32; 4]],
    color: &[[f32; 4]],
    entry: &NunoEntry,
    bones: &[BfresBone],
    bone_ids: &[u16],
) {
    let worlds = bone_world_transforms(bones);
    let parent_index = bone_ids
        .get(entry.parent_bone_id as usize)
        .copied()
        .unwrap_or(entry.parent_bone_id) as usize;
    let Some((parent_rotation, parent_position)) = worlds.get(parent_index).copied() else {
        return;
    };
    let mut controls = Vec::with_capacity(entry.control_points.len());
    for (index, point) in entry.control_points.iter().enumerate() {
        let point = [point[0], point[1], point[2]];
        let parent = entry.control_parents.get(index).copied().unwrap_or(-1);
        if parent < 0 {
            controls.push((
                parent_rotation,
                add3(quat_rotate(parent_rotation, point), parent_position),
            ));
            continue;
        }
        let Some((control_parent_rotation, control_parent_position)) =
            controls.get(parent as usize).copied()
        else {
            controls.push((
                parent_rotation,
                add3(quat_rotate(parent_rotation, point), parent_position),
            ));
            continue;
        };
        let inverse_control = quat_inverse(control_parent_rotation);
        let relative_rotation = quat_mul(parent_rotation, inverse_control);
        let relative_position = add3(
            quat_rotate(relative_rotation, point),
            quat_rotate(
                inverse_control,
                add3(parent_position, scale3(control_parent_position, -1.0)),
            ),
        );
        controls.push((
            normalize4(quat_mul(control_parent_rotation, relative_rotation)),
            add3(
                quat_rotate(control_parent_rotation, relative_position),
                control_parent_position,
            ),
        ));
    }
    for vertex in 0..positions.len() {
        let raw_position = position4[vertex];
        let raw_normal = normal4[vertex];
        if binormal[vertex]
            .iter()
            .all(|value| value.abs() <= f32::EPSILON)
        {
            positions[vertex] = add3(
                quat_rotate(
                    parent_rotation,
                    [raw_position[0], raw_position[1], raw_position[2]],
                ),
                parent_position,
            );
            normals[vertex] = normalize3(quat_rotate(
                parent_rotation,
                [raw_normal[0], raw_normal[1], raw_normal[2]],
            ));
            continue;
        }
        let zero = [0.0; 3];
        let a = weighted_centers(
            zero,
            raw_position,
            [
                blend_controls[vertex],
                psize[vertex],
                fog[vertex],
                texcoord[vertex],
            ],
            blend_weights[vertex],
            &controls,
        );
        let b = weighted_centers(
            zero,
            raw_position,
            [
                blend_controls[vertex],
                psize[vertex],
                fog[vertex],
                texcoord[vertex],
            ],
            color[vertex],
            &controls,
        );
        let c = weighted_centers(
            zero,
            binormal[vertex],
            [
                blend_controls[vertex],
                psize[vertex],
                fog[vertex],
                texcoord[vertex],
            ],
            blend_weights[vertex],
            &controls,
        );
        let d = cross3(b, c);
        positions[vertex] = add3(a, scale3(d, raw_normal[3]));
        normals[vertex] = normalize3(add3(
            add3(scale3(b, raw_normal[1]), scale3(c, raw_normal[0])),
            scale3(d, raw_normal[2]),
        ));
    }
}

fn weighted_centers(
    position: [f32; 3],
    weights: [f32; 4],
    control_sets: [[f32; 4]; 4],
    outer_weights: [f32; 4],
    controls: &[([f32; 4], [f32; 3])],
) -> [f32; 3] {
    let mut result = [0.0; 3];
    for set in 0..4 {
        let center = center_of_mass(position, weights, control_sets[set], controls);
        result = add3(result, scale3(center, outer_weights[set]));
    }
    result
}

fn center_of_mass(
    position: [f32; 3],
    weights: [f32; 4],
    control_ids: [f32; 4],
    controls: &[([f32; 4], [f32; 3])],
) -> [f32; 3] {
    let mut result = [0.0; 3];
    for component in 0..4 {
        let raw = control_ids[component].max(0.0) as usize;
        let index = if raw < controls.len() { raw } else { raw / 3 };
        let Some((rotation, translation)) = controls.get(index).copied() else {
            continue;
        };
        result = add3(
            result,
            add3(
                quat_rotate(rotation, position),
                scale3(translation, weights[component]),
            ),
        );
    }
    result
}

fn bone_world_transforms(bones: &[BfresBone]) -> Vec<([f32; 4], [f32; 3])> {
    let mut worlds: Vec<([f32; 4], [f32; 3])> = Vec::with_capacity(bones.len());
    for bone in bones {
        let local_rotation = normalize4(bone.rotation);
        let local_position = bone.translation;
        if bone.parent_index >= 0 {
            let parent = worlds[bone.parent_index as usize];
            worlds.push((
                quat_mul(parent.0, local_rotation),
                add3(quat_rotate(parent.0, local_position), parent.1),
            ));
        } else {
            worlds.push((local_rotation, local_position));
        }
    }
    worlds
}

fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn quat_inverse(value: [f32; 4]) -> [f32; 4] {
    let norm = value.iter().map(|part| part * part).sum::<f32>();
    if norm <= f32::EPSILON {
        return [0.0, 0.0, 0.0, 1.0];
    }
    [
        -value[0] / norm,
        -value[1] / norm,
        -value[2] / norm,
        value[3] / norm,
    ]
}

fn quat_rotate(q: [f32; 4], value: [f32; 3]) -> [f32; 3] {
    let vector = [q[0], q[1], q[2]];
    let uv = cross3(vector, value);
    let uuv = cross3(vector, uv);
    add3(value, add3(scale3(uv, 2.0 * q[3]), scale3(uuv, 2.0)))
}

fn normalize4(value: [f32; 4]) -> [f32; 4] {
    let length = value.iter().map(|part| part * part).sum::<f32>().sqrt();
    if length > f32::EPSILON {
        [
            value[0] / length,
            value[1] / length,
            value[2] / length,
            value[3] / length,
        ]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale3(value: [f32; 3], scale: f32) -> [f32; 3] {
    [value[0] * scale, value[1] * scale, value[2] * scale]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let length = value.iter().map(|part| part * part).sum::<f32>().sqrt();
    if length > f32::EPSILON {
        scale3(value, 1.0 / length)
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn read_attribute(reader: &BinaryReader<'_>, offset: usize, kind: u8) -> io::Result<[f32; 4]> {
    let mut values = [0.0; 4];
    match kind {
        0x00 => values[0] = f32::from_bits(reader.read_u32_at(offset)?),
        0x01..=0x03 => {
            let count = usize::from(kind) + 1;
            for (i, value) in values.iter_mut().enumerate().take(count) {
                *value = f32::from_bits(reader.read_u32_at(offset + i * 4)?);
            }
        }
        0x05 => {
            for (i, value) in values.iter_mut().enumerate() {
                *value = reader.read_u8_at(offset + i)? as f32;
            }
        }
        0x07 => {
            for (i, value) in values.iter_mut().enumerate() {
                *value = reader.read_u16_at(offset + i * 2)? as f32;
            }
        }
        0x0a => {
            for (i, value) in values.iter_mut().enumerate().take(2) {
                *value = half(reader.read_u16_at(offset + i * 2)?);
            }
        }
        0x0b => {
            for (i, value) in values.iter_mut().enumerate() {
                *value = half(reader.read_u16_at(offset + i * 2)?);
            }
        }
        0x0d => {
            for (i, value) in values.iter_mut().enumerate() {
                *value = reader.read_u8_at(offset + i)? as f32 / 255.0;
            }
        }
        _ => {}
    }
    Ok(values)
}

fn useful_uv_map(uv_map: &[[f32; 2]]) -> bool {
    let Some(first) = uv_map.first() else {
        return false;
    };
    first.iter().all(|value| value.is_finite())
        && uv_map.iter().skip(1).any(|uv| {
            uv.iter().all(|value| value.is_finite())
                && ((uv[0] - first[0]).abs() > 1.0e-6 || (uv[1] - first[1]).abs() > 1.0e-6)
        })
}

fn half(value: u16) -> f32 {
    let sign = ((value & 0x8000) as u32) << 16;
    let exponent = (value >> 10) & 0x1f;
    let fraction = (value & 0x03ff) as u32;
    let bits = match exponent {
        0 if fraction == 0 => sign,
        0 => {
            let shift = fraction.leading_zeros() - 21;
            sign | ((127 - 15 - shift) << 23) | ((fraction << (shift + 1)) & 0x7f_ffff)
        }
        31 => sign | 0x7f80_0000 | (fraction << 13),
        _ => sign | (((exponent as u32) + 112) << 23) | (fraction << 13),
    };
    f32::from_bits(bits)
}

fn strip_to_triangles(strip: &[u32]) -> Vec<u32> {
    let mut output = Vec::with_capacity(strip.len().saturating_sub(2).saturating_mul(3));
    for index in 0..strip.len().saturating_sub(2) {
        let triangle = if index % 2 == 0 {
            [strip[index], strip[index + 1], strip[index + 2]]
        } else {
            [strip[index], strip[index + 2], strip[index + 1]]
        };
        if triangle[0] != triangle[1] && triangle[1] != triangle[2] && triangle[0] != triangle[2] {
            output.extend(triangle);
        }
    }
    output
}

fn classify_texture(kind: u16, subtype: u16) -> (String, String) {
    match (kind, subtype) {
        (1, _) => ("_a0".into(), "Diffuse".into()),
        (3, _) => ("_n0".into(), "Normal".into()),
        (19, 0) => ("_e0".into(), "Emission".into()),
        (5, 5) => ("_ao0".into(), "AmbientOcclusion".into()),
        (37, 37) | (66, 66) => ("_s0".into(), "Specular".into()),
        _ => (String::new(), "Unknown".into()),
    }
}

struct PairedTextureInfo {
    ktid_hash: Option<String>,
    kidsobjdb: Option<String>,
}

fn paired_texture_info(source: &Path) -> Option<PairedTextureInfo> {
    let stem = source
        .file_stem()?
        .to_str()?
        .split('.')
        .next()?
        .to_ascii_lowercase();
    let pair = PAIRS.get(&stem)?;
    Some(PairedTextureInfo {
        ktid_hash: pair
            .get("g1t")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        kidsobjdb: pair
            .get("kidsobjdb")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    })
}

fn resolve_kidsobj_textures(
    source: &Path,
    aoc: &Path,
    pair: &PairedTextureInfo,
    slot_ids: &BTreeSet<String>,
    preview: bool,
) -> Vec<ResolvedG1tTexture> {
    let Some(ktid_path) = find_adjacent_companion(source, "ktid").or_else(|| {
        pair.ktid_hash
            .as_deref()
            .and_then(|hash| find_asset(source, aoc, "ktid", hash, "ktid"))
    }) else {
        return Vec::new();
    };
    let Ok(ktid) = KtidFile::parse(&std::fs::read(ktid_path).unwrap_or_default()) else {
        return Vec::new();
    };
    let required_hashes: HashSet<_> = slot_ids
        .iter()
        .filter_map(|slot| slot.parse::<u32>().ok())
        .filter_map(|index| ktid.get(index))
        .collect();
    let kids = resolve_kidsobj_hashes(source, aoc, pair.kidsobjdb.as_deref(), &required_hashes);
    let mut archives = BTreeMap::<PathBuf, Vec<String>>::new();
    for slot_id in slot_ids {
        let Ok(index) = slot_id.parse::<u32>() else {
            continue;
        };
        let Some(ktid_hash) = ktid.get(index) else {
            continue;
        };
        let Some(g1t_hash) = kids.get(&ktid_hash) else {
            continue;
        };
        let Some(path) = find_g1t(source, aoc, &format!("{g1t_hash:08x}")) else {
            continue;
        };
        archives.entry(path).or_default().push(slot_id.clone());
    }
    let mut textures = Vec::new();
    for (path, slot_ids) in archives {
        let data = std::fs::read(&path).unwrap_or_default();
        let parsed = if preview {
            G1tFile::parse_preview(&data)
        } else {
            G1tFile::parse(&data)
        };
        let Ok(g1t) = parsed else {
            continue;
        };
        for texture in g1t.textures {
            let names: Vec<_> = slot_ids
                .iter()
                .map(|slot_id| {
                    if texture.index == 0 {
                        slot_id.clone()
                    } else {
                        format!("{slot_id}:{}", texture.index)
                    }
                })
                .collect();
            let Some((name, aliases)) = names.split_first() else {
                continue;
            };
            textures.push(resolved_texture(
                name.clone(),
                aliases.to_vec(),
                path.clone(),
                texture,
                source,
            ));
        }
    }
    textures
}

fn resolve_direct_g1t(
    source: &Path,
    aoc: &Path,
    hash: &str,
    slot_ids: &BTreeSet<String>,
    preview: bool,
) -> Option<G1mTextureResolution> {
    let Some(path) = find_g1t(source, aoc, hash) else {
        return None;
    };
    let data = std::fs::read(&path).unwrap_or_default();
    let parsed = if preview {
        G1tFile::parse_preview(&data)
    } else {
        G1tFile::parse(&data)
    };
    let Ok(g1t) = parsed else {
        return None;
    };
    let total = g1t.textures.len();
    let textures: Vec<_> = g1t
        .textures
        .into_iter()
        .filter(|texture| slot_ids.contains(&texture.index.to_string()))
        .map(|texture| {
            resolved_texture(
                texture.index.to_string(),
                Vec::new(),
                path.clone(),
                texture,
                source,
            )
        })
        .collect();
    Some(G1mTextureResolution {
        skipped: total.saturating_sub(textures.len()),
        total,
        textures,
    })
}

fn resolved_texture(
    name: String,
    aliases: Vec<String>,
    path: PathBuf,
    texture: crate::parser::AOC::g1t::G1tTexture,
    source: &Path,
) -> ResolvedG1tTexture {
    ResolvedG1tTexture {
        name,
        aliases,
        path: format!("{}#{}", path.display(), texture.index),
        source: if path.parent() == source.parent() {
            "adjacent-g1t"
        } else {
            "aoc-path"
        }
        .into(),
        data_url: texture.data_url,
        width: texture.width,
        height: texture.height,
        array_count: texture.array_count,
        renderable: texture.renderable,
        data_urls: texture.data_urls,
    }
}

fn cached_kidsobj(path: &Path) -> Option<Arc<KidsObjFile>> {
    if let Some(parsed) = KIDSOBJ_CACHE.lock().ok()?.get(path).cloned() {
        return Some(parsed);
    }
    let parsed = Arc::new(KidsObjFile::parse(&std::fs::read(path).ok()?).ok()?);
    KIDSOBJ_CACHE
        .lock()
        .ok()?
        .insert(path.to_path_buf(), parsed.clone());
    Some(parsed)
}

fn push_kidsobj_files(paths: &mut Vec<PathBuf>, directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut found: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| {
                        value.eq_ignore_ascii_case("kidsobjdb")
                            || value.eq_ignore_ascii_case("kidssingletondb")
                    })
        })
        .collect();
    found.sort();
    for path in found {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
}

fn kidsobj_candidates(source: &Path, aoc: &Path, preferred: Option<&str>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(parent) = source.parent() {
        push_kidsobj_files(&mut paths, parent);
    }
    if let Some(name) = preferred {
        let (stem, extension) = name.rsplit_once('.').unwrap_or((name, "kidsobjdb"));
        if let Some(path) = find_asset(source, aoc, "kidsobjdb", stem, extension) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    if !aoc.as_os_str().is_empty() {
        push_kidsobj_files(&mut paths, aoc);
        push_kidsobj_files(&mut paths, &aoc.join("kidsobjdb"));
        if let Ok(editors) = std::fs::read_dir(aoc) {
            let mut editors: Vec<_> = editors
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect();
            editors.sort();
            for editor in editors {
                push_kidsobj_files(&mut paths, &editor.join("kidsobjdb"));
            }
        }
    }
    paths
}

fn resolve_kidsobj_hashes(
    source: &Path,
    aoc: &Path,
    preferred: Option<&str>,
    required: &HashSet<u32>,
) -> HashMap<u32, u32> {
    let mut resolved = HashMap::new();
    for path in kidsobj_candidates(source, aoc, preferred) {
        let Some(database) = cached_kidsobj(&path) else {
            continue;
        };
        for hash in required {
            if let Some(texture) = database.texture_for(*hash) {
                resolved.entry(*hash).or_insert(texture);
            }
        }
        if resolved.len() == required.len() {
            break;
        }
    }
    resolved
}

fn find_adjacent_companion(source: &Path, extension: &str) -> Option<PathBuf> {
    let mut matches = std::fs::read_dir(source.parent()?)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        });
    let companion = matches.next()?;
    matches.next().is_none().then_some(companion)
}

fn find_asset(
    source: &Path,
    aoc: &Path,
    folder: &str,
    stem: &str,
    extension: &str,
) -> Option<PathBuf> {
    let filename = format!("{stem}.{extension}");
    let adjacent = source.parent()?.join(&filename);
    if adjacent.is_file() {
        return Some(adjacent);
    }
    if aoc.as_os_str().is_empty() {
        return None;
    }
    if let Some(editor_root) = source.parent().and_then(Path::parent) {
        let candidate = editor_root.join(folder).join(&filename);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    for candidate in [aoc.join(&filename), aoc.join(folder).join(&filename)] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    std::fs::read_dir(aoc)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(folder).join(&filename))
        .find(|path| path.is_file())
}

fn find_g1t(source: &Path, aoc: &Path, hash: &str) -> Option<PathBuf> {
    let name = format!("{hash}.g1t");
    let adjacent = source.parent()?.join(&name);
    if adjacent.is_file() {
        return Some(adjacent);
    }
    if aoc.as_os_str().is_empty() {
        return None;
    }
    if let Some(editor_root) = source.parent().and_then(Path::parent) {
        let candidate = editor_root.join("g1t").join(&name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    for candidate in [
        aoc.join(&name),
        aoc.join("MaterialEditor/g1t").join(&name),
        aoc.join("g1t").join(&name),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    std::fs::read_dir(aoc)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("g1t").join(&name))
        .find(|path| path.is_file())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_73c7ab44_resolves_character_singleton_textures() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_aocss/73c7ab44.g1m");
        if !path.is_file() {
            return;
        }
        let pair = paired_texture_info(&path).unwrap();
        assert_eq!(pair.ktid_hash.as_deref(), Some("52b84bb7"));
        assert_eq!(
            pair.kidsobjdb.as_deref(),
            Some("CharacterEditor.kidssingletondb")
        );
        let Ok(config) = crate::TotkConfig::TotkConfig::safe_new(false) else {
            return;
        };
        if config.aoc_path.is_empty() {
            return;
        }
        let model = G1mFile::from_path(&path).unwrap();
        let resolution = model.resolve_textures(&path, Path::new(&config.aoc_path));
        assert!(resolution
            .textures
            .iter()
            .any(|texture| texture.name == "1"));
        assert!(resolution
            .textures
            .iter()
            .any(|texture| texture.name == "7"));
    }

    #[test]
    fn parses_legacy_g1m_corpus() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/g1m");
        let mut parsed = 0;
        let mut renderable = 0;
        for path in std::fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("g1m"))
            })
        {
            let model = G1mFile::from_path(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(model
                .render
                .meshes
                .iter()
                .flat_map(|mesh| &mesh.uv_maps)
                .all(|uv_map| useful_uv_map(uv_map)));
            parsed += 1;
            renderable += usize::from(!model.render.meshes.is_empty());
        }
        assert!(parsed >= 28);
        assert!(renderable >= 24);
    }

    #[test]
    fn large_8d40c774_preview_payload_stays_bounded() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_aocss/g1m/8d40c774.g1m");
        if !path.is_file() {
            return;
        }
        let data = std::fs::read(&path).unwrap();
        let Ok(config) = crate::TotkConfig::TotkConfig::safe_new(false) else {
            G1mFile::parse(&data, "8d40c774").unwrap();
            return;
        };
        let (model, resolution) =
            G1mFile::parse_with_textures(&data, "8d40c774", &path, Path::new(&config.aoc_path))
                .unwrap();
        assert!(resolution
            .textures
            .iter()
            .all(|texture| texture.data_urls.len() <= 2));
        let model_size = serde_json::to_vec(&model).unwrap().len();
        let texture_size = serde_json::to_vec(&resolution.textures).unwrap().len();
        let payload_size = model_size + texture_size;
        assert!(
            payload_size < 64 * 1024 * 1024,
            "{payload_size} byte payload ({model_size} model, {texture_size} textures)"
        );
    }

    #[test]
    #[ignore = "manual release-mode G1M parser benchmark"]
    fn benchmark_largest_g1m_single_vs_four_workers() {
        use std::time::Instant;

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_aocss/g1m");
        let mut fixtures: Vec<_> = std::fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("g1m"))
            })
            .collect();
        fixtures.sort_by_key(|path| std::cmp::Reverse(path.metadata().unwrap().len()));
        fixtures.truncate(5);

        println!("BENCH,file,bytes,one_thread_ms,four_worker_ms,speedup");
        for path in fixtures {
            let data = std::fs::read(&path).unwrap();
            let name = path.file_stem().unwrap().to_string_lossy();
            g1m_import_task::set_test_worker_limit(4);
            G1mFile::parse(&data, &name).unwrap();

            let mut single = Vec::new();
            let mut parallel = Vec::new();
            for round in 0..6 {
                let limits = if round % 2 == 0 { [1, 4] } else { [4, 1] };
                for limit in limits {
                    g1m_import_task::set_test_worker_limit(limit);
                    let started = Instant::now();
                    G1mFile::parse(&data, &name).unwrap();
                    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                    if limit == 1 {
                        single.push(elapsed);
                    } else {
                        parallel.push(elapsed);
                    }
                }
            }
            single.sort_by(f64::total_cmp);
            parallel.sort_by(f64::total_cmp);
            let one = (single[2] + single[3]) / 2.0;
            let four = (parallel[2] + parallel[3]) / 2.0;
            println!(
                "BENCH,{},{},{one:.3},{four:.3},{:.3}",
                path.file_name().unwrap().to_string_lossy(),
                data.len(),
                one / four
            );
        }
        g1m_import_task::set_test_worker_limit(0);
    }

    #[test]
    fn pairing_table_contains_sample_model() {
        assert_eq!(
            paired_texture_info(Path::new("00320929.g1m"))
                .and_then(|pair| pair.ktid_hash)
                .as_deref(),
            Some("5399aa72")
        );
    }

    #[test]
    fn bundled_character_pairings_include_ktid_hashes() {
        let hashes = [
            "004bb2f5", "032d46f0", "03e43062", "0bd08252", "11c5e069", "126b7483", "127fdca3",
            "1f5cd3ee", "270e8a01", "27e6df23", "2ecdf907", "2f007a4e", "300413c5", "360c066f",
            "36747403", "38be12e5", "38cc55a1", "405808c7", "40820d63", "42032f44", "46181a63",
            "4a4118fa", "4c34ee38", "4d22acf5", "4d8b1bad", "514e7e21", "5a0a45cd", "5c60da88",
            "5dc53599", "606a2eae", "611802fa", "61c3a6ba", "62a5ad15", "62a889f9", "71dc4528",
            "75df9a8a", "776dfc35", "7c4e33be", "7cf62ecf", "8483b409", "8810bc59", "8882252a",
            "8b238949", "8b41bcd7", "8bf1da1e", "90518fe2", "978081b2", "97ee610c", "991e4406",
            "9f285c6e", "9ff53642", "a153d8b1", "a7ceb317", "a9cbf438", "b39b63be", "b496e0d1",
            "b96f9a98", "bc5aba56", "bf11beca", "bf730b5d", "d28d3ee6", "d6a6f4aa", "e1e3c711",
            "e221658e", "e5021c29", "e9c18d66", "eb686dde", "f5821b7f", "f905784d", "fb38883b",
            "fe8c46d6",
        ];

        for hash in hashes {
            let pair = paired_texture_info(Path::new(hash))
                .unwrap_or_else(|| panic!("missing G1M pairing for {hash}"));
            assert!(pair.ktid_hash.is_some(), "missing KTID hash for {hash}");
        }
    }

    #[test]
    fn bundled_reported_character_models_include_ktid_hashes() {
        for (model_hash, expected_ktid_hash) in [
            ("e8775294", "73fd8e67"),
            ("b62b24b8", "5cc400c3"),
            ("d322bf57", "debfba04"),
        ] {
            let filename = format!("{model_hash}.g1m");
            assert_eq!(
                paired_texture_info(Path::new(&filename))
                    .and_then(|pair| pair.ktid_hash)
                    .as_deref(),
                Some(expected_ktid_hash),
                "incorrect bundled KTID pairing for {model_hash}"
            );
        }
    }

    #[test]
    fn configured_fd5c59c6_resolves_field_editor_textures() {
        let Ok(config) = crate::TotkConfig::TotkConfig::safe_new(false) else {
            return;
        };
        let root = Path::new(&config.aoc_path);
        let Some(path) = find_asset(
            &root.join("FieldEditor4/g1m/fd5c59c6.g1m"),
            root,
            "g1m",
            "fd5c59c6",
            "g1m",
        ) else {
            return;
        };
        let model = G1mFile::from_path(&path).unwrap();
        let resolution = model.resolve_textures(&path, root);
        assert!(!resolution.textures.is_empty());
    }

    #[test]
    fn configured_49c9ef33_resolves_material_editor_textures() {
        let Ok(config) = crate::TotkConfig::TotkConfig::safe_new(false) else {
            return;
        };
        let root = Path::new(&config.aoc_path);
        let path = root.join("CharacterEditor/g1m/49c9ef33.g1m");
        if !path.is_file() {
            return;
        }
        let model = G1mFile::from_path(&path).unwrap();
        let resolution = model.resolve_textures(&path, root);
        assert_eq!(resolution.textures.len(), 33);
        assert!(resolution
            .textures
            .iter()
            .all(|texture| texture.path.contains("MaterialEditor")));
    }

    #[test]
    fn configured_reported_character_models_resolve_material_editor_textures() {
        let Ok(config) = crate::TotkConfig::TotkConfig::safe_new(false) else {
            return;
        };
        let root = Path::new(&config.aoc_path);

        for model_hash in ["e8775294", "b62b24b8", "d322bf57"] {
            let path = root
                .join("CharacterEditor/g1m")
                .join(format!("{model_hash}.g1m"));
            if !path.is_file() {
                continue;
            }

            let model = G1mFile::from_path(&path).unwrap();
            let resolution = model.resolve_textures(&path, root);
            assert!(
                !resolution.textures.is_empty(),
                "no textures resolved for {model_hash}"
            );
            assert!(
                resolution
                    .textures
                    .iter()
                    .all(|texture| texture.path.contains("MaterialEditor")),
                "{model_hash} resolved a texture outside MaterialEditor"
            );
        }
    }

    #[test]
    fn configured_127fdca3_resolves_material_editor_textures() {
        let Ok(config) = crate::TotkConfig::TotkConfig::safe_new(false) else {
            return;
        };
        let root = Path::new(&config.aoc_path);
        let path = root.join("CharacterEditor/g1m/127fdca3.g1m");
        if !path.is_file() {
            return;
        }
        let model = G1mFile::from_path(&path).unwrap();
        let resolution = model.resolve_textures(&path, root);
        assert_eq!(resolution.textures.len(), 42);
        assert!(resolution
            .textures
            .iter()
            .all(|texture| texture.path.contains("MaterialEditor")));
    }

    #[test]
    fn standalone_e96046e8_uses_adjacent_90332493_ktid() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_aocss/data/e96046e8.g1m");
        let ktid = path.parent().unwrap().join("90332493.ktid");
        if !path.is_file() || !ktid.is_file() {
            return;
        }
        assert_eq!(
            paired_texture_info(&path)
                .and_then(|pair| pair.ktid_hash)
                .as_deref(),
            Some("90332493")
        );
        assert_eq!(
            find_adjacent_companion(&path, "ktid").as_deref(),
            Some(ktid.as_path())
        );

        let Ok(config) = crate::TotkConfig::TotkConfig::safe_new(false) else {
            return;
        };
        let model = G1mFile::from_path(&path).unwrap();
        let resolution = model.resolve_textures(&path, Path::new(&config.aoc_path));
        assert!(!resolution.textures.is_empty());
    }

    #[test]
    fn botw_bone_names_are_applied_by_joint_index() {
        assert_eq!(BOTW_BONE_NAMES.get(&0).map(String::as_str), Some("Root"));
        assert_eq!(BOTW_BONE_NAMES.get(&22).map(String::as_str), Some("Head"));
    }

    #[test]
    fn aoc_model_names_are_resolved_from_hashes() {
        assert_eq!(
            AOC_NAMES
                .get("038bb045")
                .map(|entry| entry.name.replace(' ', "_")),
            Some("Amber_Earrings".to_owned())
        );
        assert!(!AOC_NAMES.contains_key("ffffffff"));
    }

    #[test]
    fn support_catalog_accessors_reuse_lazy_caches() {
        assert!(std::ptr::eq(aoc_names(), aoc_names()));
        assert!(std::ptr::eq(model_texture_pairs(), model_texture_pairs()));
        assert!(std::ptr::eq(aoc_names(), &*AOC_NAMES));
        assert!(std::ptr::eq(model_texture_pairs(), &*PAIRS));
    }

    #[test]
    fn degenerate_uv_maps_are_rejected() {
        assert!(!useful_uv_map(&[]));
        assert!(!useful_uv_map(&[[0.0, 0.0], [0.0, 0.0]]));
        assert!(useful_uv_map(&[[0.0, 0.0], [1.0, 0.0]]));
    }

    #[test]
    fn cloth_control_indices_are_not_exposed_as_uv_maps() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/g1m/0cb432cf.g1m");
        let model = G1mFile::from_path(path).unwrap();
        assert!(model
            .render
            .meshes
            .iter()
            .all(|mesh| mesh.uv_maps.len() <= 2));
    }

    #[test]
    fn supplied_champion_model_has_safe_skin_bindings() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/g1m_importer/_ss/0cb432cf.g1m");
        if !path.is_file() {
            return;
        }
        let model = G1mFile::from_path(path).unwrap();
        let bone_count = model.render.bones.len() as u16;
        assert_eq!(model.render.meshes.len(), 4);
        for mesh in &model.render.meshes {
            assert!(mesh
                .bone_indices
                .iter()
                .zip(&mesh.bone_weights)
                .all(|(indices, weights)| (0..mesh.vertex_skin_count as usize)
                    .all(|index| weights[index] == 0.0 || indices[index] < bone_count)));
            if mesh.vertex_skin_count == 1 {
                assert!(mesh
                    .bone_weights
                    .iter()
                    .all(|weights| *weights == [1.0, 0.0, 0.0, 0.0]));
            }
        }
    }

    #[test]
    fn supplied_4d_model_uses_direct_geometry_palette_joints() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_aocss/4d58bba9.g1m");
        if !path.is_file() {
            return;
        }
        let model = G1mFile::from_path(path).unwrap();
        let hair = &model.render.meshes[0];
        assert_eq!(hair.vertex_skin_count, 4);
        assert!(hair.skin_bones.contains(&123));
        assert!(hair.skin_bones.contains(&225));
        assert!(hair
            .bone_indices
            .iter()
            .zip(&hair.bone_weights)
            .all(|(indices, weights)| (0..4).all(|index| {
                weights[index] == 0.0 || (indices[index] as usize) < model.render.bones.len()
            })));
    }

    #[test]
    fn supplied_4d_model_combines_both_skin_layers() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_aocss/4d58bba9.g1m");
        if !path.is_file() {
            return;
        }
        let model = G1mFile::from_path(path).unwrap();
        let body = &model.render.meshes[8];
        let waist_weight: f32 = body
            .bone_indices
            .iter()
            .zip(&body.bone_weights)
            .map(|(indices, weights)| {
                (0..4)
                    .filter(|&influence| indices[influence] == 2)
                    .map(|influence| weights[influence])
                    .sum::<f32>()
            })
            .sum();
        assert!(waist_weight < body.positions.len() as f32 * 0.1);
        assert!(body
            .bone_weights
            .iter()
            .all(|weights| { (weights.iter().sum::<f32>() - 1.0).abs() < 1.0e-4 }));
    }

    #[test]
    fn supplied_4d_model_export_keeps_full_texture_dimensions() {
        use base64::Engine;

        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/4d58bba9.g1m");
        if !path.is_file() {
            return;
        }
        let config = crate::TotkConfig::TotkConfig::safe_new(false).unwrap();
        if config.aoc_path.is_empty() {
            return;
        }
        let model = G1mFile::from_path(&path).unwrap();
        let decode_dimensions = |texture: &ResolvedG1tTexture| {
            let encoded = texture.data_url.split_once(',').unwrap().1;
            let png = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .unwrap();
            let image = crate::file_format::Image::raster::decode(&png).unwrap();
            (image.width(), image.height())
        };

        let export = model.resolve_textures_for_export(&path, Path::new(&config.aoc_path));
        let export_texture = export
            .textures
            .iter()
            .find(|texture| texture.width == 1024 && texture.height == 2048)
            .expect("missing expected 1024x2048 texture");
        assert_eq!(decode_dimensions(export_texture), (1024, 2048));
    }

    #[test]
    fn supplied_642_model_skinning_remains_valid() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_aocss/64200672.g1m");
        if !path.is_file() {
            return;
        }
        let model = G1mFile::from_path(path).unwrap();
        let bone_count = model.render.bones.len();
        assert!(model.render.meshes.iter().all(|mesh| mesh
            .bone_indices
            .iter()
            .zip(&mesh.bone_weights)
            .all(|(indices, weights)| (0..4).all(|influence| {
                weights[influence] == 0.0 || (indices[influence] as usize) < bone_count
            }))));
    }

    #[test]
    fn named_g1m_open_metadata_is_preserved() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/g1m_importer/_ss/0cb432cf.g1m");
        if !path.is_file() {
            return;
        }
        let (_, data) = G1mFile::open(&path).expect("G1M opener rejected fixture");
        assert_eq!(data.file_label, "[Champion Revali] 0cb432cf.g1m [G1M]");
        assert_eq!(data.file_metadata, "[G1M]");
        assert!(!data.read_only);
    }

    #[test]
    fn champion_zelda_uses_binary_ktid_resource() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/g1m_importer/_ss/data/49c9ef33.g1m");
        if path.is_file() {
            let model = G1mFile::from_path(&path).expect("49c9ef33 fixture did not parse");
            let slots: BTreeSet<_> = model
                .materials
                .iter()
                .flat_map(|material| &material.texture_slots)
                .map(|slot| slot.name.as_str())
                .collect();
            assert!(slots.contains("16"));
            assert!(slots.contains("17"));
            assert!(slots.contains("22"));
        }
        assert_eq!(
            paired_texture_info(Path::new("49c9ef33.g1m"))
                .and_then(|pair| pair.ktid_hash)
                .as_deref(),
            Some("3cfe85a8")
        );
    }

    #[test]
    fn champion_zelda_emission_is_slot_17_not_slot_24() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/g1m_importer/_ss/data/49c9ef33.g1m");
        if !path.is_file() {
            return;
        }
        let model = G1mFile::from_path(path).unwrap();
        let material = &model.materials[0];
        assert_eq!(
            material
                .texture_slots
                .iter()
                .find(|slot| slot.texture_type == "Emission")
                .map(|slot| slot.name.as_str()),
            Some("17")
        );
        assert_eq!(
            material
                .texture_slots
                .iter()
                .find(|slot| slot.name == "24")
                .map(|slot| slot.texture_type.as_str()),
            Some("Unknown")
        );
    }

    #[test]
    fn opening_g1m_and_g1t_creates_no_sidecar_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/g1m_importer/_ss/data");
        let g1m_path = root.join("49c9ef33.g1m");
        let g1t_path = root.join("076238b3.g1t");
        if !g1m_path.is_file() || !g1t_path.is_file() {
            return;
        }

        let directory_entries = || -> BTreeSet<_> {
            std::fs::read_dir(&root)
                .expect("failed to inspect fixture directory")
                .filter_map(Result::ok)
                .map(|entry| entry.file_name())
                .collect()
        };
        let before = directory_entries();

        let model = G1mFile::from_path(&g1m_path).expect("G1M fixture did not parse");
        let _ = model.resolve_textures(&g1m_path, Path::new(""));
        let g1t_data = std::fs::read(&g1t_path).expect("failed to read G1T fixture");
        let _ = G1tFile::parse(&g1t_data).expect("G1T fixture did not parse");

        assert_eq!(before, directory_entries());
    }

    #[test]
    fn configured_80358426_identifies_cloth_meshes() {
        let path = Path::new("J:/AOC/_extracted/CharacterEditor/g1m/80358426.g1m");
        if !path.is_file() {
            return;
        }
        let model = G1mFile::from_path(path).expect("80358426 did not parse");
        assert_eq!(
            model
                .render
                .meshes
                .iter()
                .filter(|mesh| mesh.is_cloth)
                .count(),
            2
        );
        let cloth = model
            .render
            .meshes
            .iter()
            .filter(|mesh| mesh.is_cloth)
            .collect::<Vec<_>>();
        assert!(cloth.iter().all(|mesh| mesh
            .positions
            .iter()
            .all(|point| point.iter().all(|value| value.is_finite()))));
        assert!(cloth[0].positions.iter().all(|point| point[1] > 60.0));
        assert!(cloth[1].positions.iter().any(|point| point[2] < -100.0));
    }
}
