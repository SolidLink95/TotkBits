//! Parses a TOTK `.bphsh` (Phive container around a hk2022 TAG0 tagfile
//! holding one `hknpMeshShape`) into [`BphshShape`].
//!
//! Layout (all little endian except the TAG0 section words, which are big
//! endian `kind << 30 | size`):
//!
//! ```text
//! 0x00 "Phive\0"  u16 reserve(1)  u16 BOM(0xFEFF)  u8 major(0)  u8 minor(4)
//! 0x0C u32 hkt_offset(0x30)  u32 table0_offset  u32 table1_offset  u32 file_size
//! 0x1C u32 hkt_size          u32 table0_size    u32 table1_size    u32 0  u32 0
//! 0x30 TAG0 { SDKV "20220100", DATA <objects>, TYPE <reflection>, INDX { ITEM } }
//!      table0: { i32 material id, i32 0, u64 user shape tag mask } per material
//!      table1: u64 collision mask per material
//! ```
//!
//! Objects inside DATA are located through Havok's relative arrays
//! (`hkRelArray`: i64 offset, i32 size, i32 capacity; `hkRelArrayView`:
//! i32 offset, i32 size; `hkRelPtr`: i64 offset), each offset relative to
//! the field's own address.

use super::shape::*;
use crate::parser::binary::BinaryReader;
use crate::parser::physics::bphcl::Section;
use std::io::{self, ErrorKind};

pub const PHIVE_MAGIC: &[u8; 6] = b"Phive\0";
pub const TOTK_SDK_VERSION: &[u8; 8] = b"20220100";

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message.into())
}

/// The Phive container header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhiveHeader {
    pub reserve1: u16,
    pub bom: u16,
    pub major: u8,
    pub minor: u8,
    pub hkt_offset: u32,
    pub table0_offset: u32,
    pub table1_offset: u32,
    pub file_size: u32,
    pub hkt_size: u32,
    pub table0_size: u32,
    pub table1_size: u32,
}

impl PhiveHeader {
    pub fn read(data: &[u8]) -> io::Result<Self> {
        if data.len() < 0x30 || &data[..6] != PHIVE_MAGIC {
            return Err(invalid("missing Phive header"));
        }
        let r = BinaryReader::new(data);
        Ok(Self {
            reserve1: r.read_u16_at(6)?,
            bom: r.read_u16_at(8)?,
            major: r.read_u8_at(10)?,
            minor: r.read_u8_at(11)?,
            hkt_offset: r.read_u32_at(0x0C)?,
            table0_offset: r.read_u32_at(0x10)?,
            table1_offset: r.read_u32_at(0x14)?,
            file_size: r.read_u32_at(0x18)?,
            hkt_size: r.read_u32_at(0x1C)?,
            table0_size: r.read_u32_at(0x20)?,
            table1_size: r.read_u32_at(0x24)?,
        })
    }
}

/// The shape plus the raw TYPE section and SDK version, kept for parity
/// checks.
#[derive(Clone, Debug)]
pub struct ParsedBphsh {
    pub sdk_version: Vec<u8>,
    pub shape: BphshShape,
    pub type_section: Vec<u8>,
}

/// Reads a `hkRelArrayView<T, int>` header at `at`: (absolute data offset, size).
fn rel_array_view(r: &BinaryReader, at: usize) -> io::Result<(usize, usize)> {
    let offset = r.read_i32_at(at)?;
    let size = r.read_i32_at(at + 4)?;
    if size < 0 {
        return Err(invalid(format!("negative array size at {at:#x}")));
    }
    let abs = (at as i64 + offset as i64) as usize;
    Ok((abs, size as usize))
}

/// Reads a `hkRelArray<T>` header at `at`: (absolute data offset, size, capacity/flags).
fn rel_array(r: &BinaryReader, at: usize) -> io::Result<(usize, usize, i32)> {
    let offset = r.read_i64_at(at)?;
    let size = r.read_i32_at(at + 8)?;
    let capacity = r.read_i32_at(at + 12)?;
    if size < 0 {
        return Err(invalid(format!("negative array size at {at:#x}")));
    }
    Ok(((at as i64 + offset) as usize, size as usize, capacity))
}

fn read_f32x4(r: &BinaryReader, at: usize) -> io::Result<[f32; 4]> {
    Ok([
        r.read_f32_at(at)?,
        r.read_f32_at(at + 4)?,
        r.read_f32_at(at + 8)?,
        r.read_f32_at(at + 12)?,
    ])
}

fn read_simd_node(r: &BinaryReader, at: usize) -> io::Result<SimdTreeNode> {
    let data = [
        r.read_u32_at(at + 0x60)?,
        r.read_u32_at(at + 0x64)?,
        r.read_u32_at(at + 0x68)?,
        r.read_u32_at(at + 0x6C)?,
    ];
    Ok(SimdTreeNode {
        lx: read_f32x4(r, at)?,
        hx: read_f32x4(r, at + 0x10)?,
        ly: read_f32x4(r, at + 0x20)?,
        hy: read_f32x4(r, at + 0x30)?,
        lz: read_f32x4(r, at + 0x40)?,
        hz: read_f32x4(r, at + 0x50)?,
        data,
        is_leaf: r.read_u8_at(at + 0x70)? != 0,
        is_active: r.read_u8_at(at + 0x71)? != 0,
    })
}

fn read_aabb8_node(r: &BinaryReader, at: usize) -> io::Result<Aabb8TreeNode> {
    let bytes = r.read_bytes_at(at, 0x1C)?;
    let lane = |i: usize| -> [u8; 4] { [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]] };
    Ok(Aabb8TreeNode {
        lx: lane(0),
        hx: lane(4),
        ly: lane(8),
        hy: lane(12),
        lz: lane(16),
        hz: lane(20),
        data: lane(24),
    })
}

fn read_section(r: &BinaryReader, at: usize) -> io::Result<GeometrySection> {
    let (bvh_at, bvh_len) = rel_array_view(r, at)?;
    let (prim_at, prim_len) = rel_array_view(r, at + 8)?;
    let (vert_at, vert_bytes) = rel_array_view(r, at + 16)?;
    let (bits_at, bits_len) = rel_array_view(r, at + 24)?;
    let mut bvh = Vec::with_capacity(bvh_len);
    for i in 0..bvh_len {
        bvh.push(read_aabb8_node(r, bvh_at + i * 0x1C)?);
    }
    let mut primitives = Vec::with_capacity(prim_len);
    for i in 0..prim_len {
        let b = r.read_bytes_at(prim_at + i * 4, 4)?;
        primitives.push(Primitive {
            a: b[0],
            b: b[1],
            c: b[2],
            d: b[3],
        });
    }
    // The vertex buffer is typed as bytes in TOTK's reflection; its size is
    // `vertices * 6` plus a few zero padding bytes (see the corpus stats).
    let vertex_count = vert_bytes / 6;
    let mut vertices = Vec::with_capacity(vertex_count);
    for i in 0..vertex_count {
        let base = vert_at + i * 6;
        vertices.push([
            r.read_u16_at(base)?,
            r.read_u16_at(base + 2)?,
            r.read_u16_at(base + 4)?,
        ]);
    }
    let trailing = r.read_bytes_at(vert_at + vertex_count * 6, vert_bytes - vertex_count * 6)?;
    if trailing.iter().any(|b| *b != 0) {
        return Err(invalid(format!(
            "vertex buffer at {vert_at:#x} has non-zero padding"
        )));
    }
    let interior_primitive_bits = r.read_bytes_at(bits_at, bits_len)?.to_vec();
    let section_offset = [
        r.read_u32_at(at + 32)?,
        r.read_u32_at(at + 36)?,
        r.read_u32_at(at + 40)?,
    ];
    let bit_scale8_inv = [
        r.read_f32_at(at + 44)?,
        r.read_f32_at(at + 48)?,
        r.read_f32_at(at + 52)?,
    ];
    let bit_offset = [
        r.read_i16_at(at + 56)?,
        r.read_i16_at(at + 58)?,
        r.read_i16_at(at + 60)?,
    ];
    Ok(GeometrySection {
        bvh,
        primitives,
        vertices,
        interior_primitive_bits,
        vertex_buffer_size: vert_bytes,
        section_offset,
        bit_scale8_inv,
        bit_offset,
    })
}

fn read_primitive_mapping(r: &BinaryReader, at: usize) -> io::Result<PrimitiveMapping> {
    // hkReferencedObject (0x18) then two hkRelArray<u32> (0x10 each), then u32 x2.
    let (starts_at, starts_len, _) = rel_array(r, at + 0x18)?;
    let (bits_at, bits_len, _) = rel_array(r, at + 0x28)?;
    let mut section_start = Vec::with_capacity(starts_len);
    for i in 0..starts_len {
        section_start.push(r.read_u32_at(starts_at + i * 4)?);
    }
    let mut bit_string = Vec::with_capacity(bits_len);
    for i in 0..bits_len {
        bit_string.push(r.read_u32_at(bits_at + i * 4)?);
    }
    Ok(PrimitiveMapping {
        section_start,
        bit_string,
        bits_per_entry: r.read_u32_at(at + 0x38)?,
        triangle_index_bit_mask: r.read_u32_at(at + 0x3C)?,
    })
}

/// Parses the `hknpMeshShape` object graph whose root sits at `at`.
pub fn read_mesh_shape(data: &[u8], at: usize) -> io::Result<MeshShape> {
    let r = BinaryReader::new(data);
    let shape_type = r.read_u8_at(at + 0x18)?;
    let dispatch_type = r.read_u8_at(at + 0x19)?;
    let flags = r.read_u16_at(at + 0x1A)?;
    let num_shape_key_bits = r.read_u8_at(at + 0x1C)?;
    let convex_radius = r.read_f32_at(at + 0x20)?;
    let user_data = r.read_u64_at(at + 0x28)?;
    let (_, properties_len, _) = rel_array(&r, at + 0x30)?;
    if properties_len != 0 {
        return Err(invalid("hknpShape properties are not supported"));
    }
    let shape_tag_codec_info = r.read_u32_at(at + 0x40)?;
    let bit_scale16 = read_f32x4(&r, at + 0x50)?;
    let bit_scale16_inv = read_f32x4(&r, at + 0x60)?;
    let (tags_at, tags_len) = rel_array_view(&r, at + 0x70)?;
    let mut shape_tag_table = Vec::with_capacity(tags_len);
    for i in 0..tags_len {
        let entry_at = tags_at + i * 8;
        shape_tag_table.push(ShapeTagEntry {
            mesh_primitive_key: r.read_u32_at(entry_at)?,
            shape_tag: r.read_u16_at(entry_at + 4)?,
        });
    }
    let (nodes_at, nodes_len, _) = rel_array(&r, at + 0x78)?;
    let top_level_tree_is_compact = r.read_u8_at(at + 0x88)? != 0;
    let mut top_level_tree = Vec::with_capacity(nodes_len);
    for i in 0..nodes_len {
        top_level_tree.push(read_simd_node(&r, nodes_at + i * 0x80)?);
    }
    let (sections_at, sections_len) = rel_array_view(&r, at + 0x90)?;
    let mut sections = Vec::with_capacity(sections_len);
    for i in 0..sections_len {
        sections.push(read_section(&r, sections_at + i * 0x40)?);
    }
    let mapping_offset = r.read_i64_at(at + 0x98)?;
    let primitive_mapping = if mapping_offset != 0 {
        Some(read_primitive_mapping(
            &r,
            (at as i64 + 0x98 + mapping_offset) as usize,
        )?)
    } else {
        None
    };
    Ok(MeshShape {
        shape_type,
        dispatch_type,
        flags,
        num_shape_key_bits,
        convex_radius,
        user_data,
        shape_tag_codec_info,
        bit_scale16,
        bit_scale16_inv,
        shape_tag_table,
        top_level_tree,
        top_level_tree_is_compact,
        sections,
        primitive_mapping,
    })
}

/// Parses a complete `.bphsh` file (already decompressed).
pub fn parse(data: &[u8]) -> io::Result<ParsedBphsh> {
    let header = PhiveHeader::read(data)?;
    let hkt_offset = header.hkt_offset as usize;
    let tag = Section::read(data, hkt_offset, data.len(), Some("TAG0"))?;
    let sdkv = tag
        .find("SDKV")
        .ok_or_else(|| invalid("TAG0 has no SDKV section"))?;
    let sdk_version = data[sdkv.payload_offset..sdkv.payload_end()].to_vec();
    let data_section = tag
        .find("DATA")
        .ok_or_else(|| invalid("TAG0 has no DATA section"))?;
    let type_section = tag
        .find("TYPE")
        .ok_or_else(|| invalid("TAG0 has no TYPE section"))?;
    if tag.find("ITEM").is_none() {
        return Err(invalid("TAG0 has no ITEM section"));
    }
    let r = BinaryReader::new(data);
    let shape = read_mesh_shape(data, data_section.payload_offset)?;

    let table0 = header.table0_offset as usize;
    let table0_end = table0 + header.table0_size as usize;
    let table1 = header.table1_offset as usize;
    let table1_end = table1 + header.table1_size as usize;
    if table0_end > data.len() || table1_end > data.len() {
        return Err(invalid("material tables run past the end of the file"));
    }
    let material_count = header.table0_size as usize / 16;
    let mut materials = Vec::with_capacity(material_count);
    for i in 0..material_count {
        let at = table0 + i * 16;
        materials.push(MaterialEntry {
            material_id: r.read_i32_at(at)?,
            reserved: r.read_i32_at(at + 4)?,
            flags: r.read_u64_at(at + 8)?,
        });
    }
    let mut collision_masks = Vec::with_capacity(material_count);
    for i in 0..material_count {
        collision_masks.push(r.read_u64_at(table1 + i * 8)?);
    }
    Ok(ParsedBphsh {
        sdk_version,
        shape: BphshShape {
            shape,
            materials,
            collision_masks,
        },
        type_section: data[type_section.offset..type_section.payload_end()].to_vec(),
    })
}

/// True when `data` starts with the Phive magic (any Phive file type).
pub fn is_phive(data: &[u8]) -> bool {
    data.len() >= 6 && &data[..6] == PHIVE_MAGIC
}
