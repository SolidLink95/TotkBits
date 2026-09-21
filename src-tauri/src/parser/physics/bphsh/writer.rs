//! Serializes a [`BphshShape`] into TOTK's `.bphsh` layout, byte for byte
//! the way the vanilla files are laid out (see the corpus test):
//!
//! ```text
//! DATA: hknpMeshShape (0xA0) | shape tag table | SIMD tree nodes |
//!       section headers (0x40 each) | per 1024-section chunk: every
//!       section BVH, every primitive array, every vertex buffer, every
//!       interior bit field | primitive mapping
//! ```
//!
//! Every array starts 16-byte aligned. The ITEM list names each array in
//! the same order with TOTK's TYPE indices, and the TYPE section is the
//! constant reflection block every vanilla mesh shape carries.

use super::reader::TOTK_SDK_VERSION;
use super::shape::*;
use crate::parser::binary::{BinaryWriter, Endian};
use std::io;

/// TOTK's TYPE section for `hknpMeshShape` files (section header included).
pub const TOTK_TYPE_SECTION: &[u8] = include_bytes!("totk_type_section.bin");

/// TYPE indices of the objects the ITEM list names, in TOTK's TYPE section.
pub mod type_index {
    pub const MESH_SHAPE: u32 = 0x01;
    pub const SHAPE_TAG_ENTRY: u32 = 0x05;
    pub const GEOMETRY_SECTION: u32 = 0x09;
    pub const SIMD_TREE_NODE: u32 = 0x10;
    pub const BYTE: u32 = 0x16;
    pub const AABB8_TREE_NODE: u32 = 0x2D;
    pub const PRIMITIVE: u32 = 0x2F;
    pub const PRIMITIVE_MAPPING: u32 = 0x0C;
    pub const UINT32: u32 = 0x1F;
}

const ITEM_OBJECT: u32 = 0x1000_0000;
const ITEM_ARRAY: u32 = 0x2000_0000;
const SECTION_CHUNK: usize = 1024;

fn align16(v: usize) -> usize {
    (v + 15) & !15
}

struct Item {
    type_index: u32,
    flags: u32,
    offset: usize,
    count: usize,
}

/// Writes a `hkRelArrayView` header (`i32 offset, i32 size`) at `header`
/// pointing at `target`.
fn patch_view(w: &mut BinaryWriter, header: usize, target: usize, size: usize) {
    w.write_u32_at(header, (target as i64 - header as i64) as i32 as u32);
    w.write_u32_at(header + 4, size as u32);
}

/// Writes a `hkRelArray` header (`i64 offset, i32 size, i32 capacity`).
fn patch_array(w: &mut BinaryWriter, header: usize, target: usize, size: usize, capacity: i32) {
    let offset = if size == 0 {
        0
    } else {
        target as i64 - header as i64
    };
    w.write_u64_at(header, offset as u64);
    w.write_u32_at(header + 8, size as u32);
    w.write_u32_at(header + 12, capacity as u32);
}

fn write_f32x4(w: &mut BinaryWriter, v: &[f32; 4]) {
    for x in v {
        w.write_f32(*x);
    }
}

fn write_simd_node(w: &mut BinaryWriter, n: &SimdTreeNode) {
    write_f32x4(w, &n.lx);
    write_f32x4(w, &n.hx);
    write_f32x4(w, &n.ly);
    write_f32x4(w, &n.hy);
    write_f32x4(w, &n.lz);
    write_f32x4(w, &n.hz);
    for d in n.data {
        w.write_u32(d);
    }
    w.write_u8(n.is_leaf as u8);
    w.write_u8(n.is_active as u8);
    w.write_zeros(14);
}

fn write_aabb8_node(w: &mut BinaryWriter, n: &Aabb8TreeNode) {
    w.write_bytes(&n.lx);
    w.write_bytes(&n.hx);
    w.write_bytes(&n.ly);
    w.write_bytes(&n.hy);
    w.write_bytes(&n.lz);
    w.write_bytes(&n.hz);
    w.write_bytes(&n.data);
}

/// Serializes the DATA payload (root object at offset 0) and returns it with
/// the ITEM entries that describe it.
fn write_data(shape: &MeshShape) -> io::Result<(Vec<u8>, Vec<Item>)> {
    let mut w = BinaryWriter::with_endian(Endian::Little);
    let mut items = vec![Item {
        type_index: 0,
        flags: 0,
        offset: 0,
        count: 0,
    }];

    // hknpMeshShape root.
    w.write_zeros(0x18); // hkReferencedObject
    w.write_u8(shape.shape_type);
    w.write_u8(shape.dispatch_type);
    w.write_u16(shape.flags);
    w.write_u8(shape.num_shape_key_bits);
    w.write_zeros(3);
    w.write_f32(shape.convex_radius);
    w.write_zeros(4);
    w.write_u64(shape.user_data);
    w.write_zeros(0x10); // properties (empty hkRelArray)
    w.write_u32(shape.shape_tag_codec_info);
    w.write_zeros(0x0C);
    write_f32x4(&mut w, &shape.bit_scale16);
    write_f32x4(&mut w, &shape.bit_scale16_inv);
    let tags_header = w.position();
    w.write_zeros(8);
    let nodes_header = w.position();
    w.write_zeros(0x10);
    w.write_u8(shape.top_level_tree_is_compact as u8);
    w.write_zeros(7);
    let sections_header = w.position();
    w.write_zeros(8);
    let mapping_header = w.position();
    w.write_zeros(8);
    debug_assert_eq!(w.position(), 0xA0);
    items.push(Item {
        type_index: type_index::MESH_SHAPE,
        flags: ITEM_OBJECT,
        offset: 0,
        count: 1,
    });

    // Shape tag table.
    let tags_at = w.position();
    patch_view(&mut w, tags_header, tags_at, shape.shape_tag_table.len());
    for entry in &shape.shape_tag_table {
        w.write_u32(entry.mesh_primitive_key);
        w.write_u16(entry.shape_tag);
        w.write_u16(0);
    }
    items.push(Item {
        type_index: type_index::SHAPE_TAG_ENTRY,
        flags: ITEM_ARRAY,
        offset: tags_at,
        count: shape.shape_tag_table.len(),
    });
    w.align(16)?;

    // Top-level SIMD tree.
    let nodes_at = w.position();
    patch_array(
        &mut w,
        nodes_header,
        nodes_at,
        shape.top_level_tree.len(),
        0,
    );
    for node in &shape.top_level_tree {
        write_simd_node(&mut w, node);
    }
    items.push(Item {
        type_index: type_index::SIMD_TREE_NODE,
        flags: ITEM_ARRAY,
        offset: nodes_at,
        count: shape.top_level_tree.len(),
    });
    w.align(16)?;

    // Section headers.
    let sections_at = w.position();
    patch_view(&mut w, sections_header, sections_at, shape.sections.len());
    for section in &shape.sections {
        w.write_zeros(32); // four hkRelArrayView headers
        for v in section.section_offset {
            w.write_u32(v);
        }
        for v in section.bit_scale8_inv {
            w.write_f32(v);
        }
        for v in section.bit_offset {
            w.write_i16(v);
        }
        w.write_u16(0);
    }
    items.push(Item {
        type_index: type_index::GEOMETRY_SECTION,
        flags: ITEM_ARRAY,
        offset: sections_at,
        count: shape.sections.len(),
    });
    w.align(16)?;

    // Section arrays, grouped by kind inside chunks of 1024 sections.
    let header_of = |index: usize| sections_at + index * 0x40;
    for chunk_start in (0..shape.sections.len()).step_by(SECTION_CHUNK) {
        let chunk_end = (chunk_start + SECTION_CHUNK).min(shape.sections.len());
        for (index, section) in shape
            .sections
            .iter()
            .enumerate()
            .take(chunk_end)
            .skip(chunk_start)
        {
            let at = w.position();
            patch_view(&mut w, header_of(index), at, section.bvh.len());
            for node in &section.bvh {
                write_aabb8_node(&mut w, node);
            }
            items.push(Item {
                type_index: type_index::AABB8_TREE_NODE,
                flags: ITEM_ARRAY,
                offset: at,
                count: section.bvh.len(),
            });
            w.align(16)?;
        }
        for (index, section) in shape
            .sections
            .iter()
            .enumerate()
            .take(chunk_end)
            .skip(chunk_start)
        {
            let at = w.position();
            patch_view(&mut w, header_of(index) + 8, at, section.primitives.len());
            for p in &section.primitives {
                w.write_bytes(&p.ids());
            }
            items.push(Item {
                type_index: type_index::PRIMITIVE,
                flags: ITEM_ARRAY,
                offset: at,
                count: section.primitives.len(),
            });
            w.align(16)?;
        }
        for (index, section) in shape
            .sections
            .iter()
            .enumerate()
            .take(chunk_end)
            .skip(chunk_start)
        {
            let at = w.position();
            let size = section.vertex_buffer_size;
            patch_view(&mut w, header_of(index) + 16, at, size);
            for v in &section.vertices {
                w.write_u16(v[0]);
                w.write_u16(v[1]);
                w.write_u16(v[2]);
            }
            w.write_zeros(size.saturating_sub(section.vertices.len() * 6));
            items.push(Item {
                type_index: type_index::BYTE,
                flags: ITEM_ARRAY,
                offset: at,
                count: size,
            });
            w.align(16)?;
        }
        for (index, section) in shape
            .sections
            .iter()
            .enumerate()
            .take(chunk_end)
            .skip(chunk_start)
        {
            let at = w.position();
            patch_view(
                &mut w,
                header_of(index) + 24,
                at,
                section.interior_primitive_bits.len(),
            );
            w.write_bytes(&section.interior_primitive_bits);
            items.push(Item {
                type_index: type_index::BYTE,
                flags: ITEM_ARRAY,
                offset: at,
                count: section.interior_primitive_bits.len(),
            });
            w.align(16)?;
        }
    }

    // Primitive mapping. No vanilla TOTK shape carries one and the builder
    // never emits one, so its ITEM layout is unverified: refuse to guess.
    if let Some(mapping) = &shape.primitive_mapping {
        let _ = mapping;
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "writing a hknpMeshShapePrimitiveMapping is not supported",
        ));
    }
    #[allow(unreachable_code)]
    if let Some(mapping) = &shape.primitive_mapping {
        let at = w.position();
        w.write_u64_at(mapping_header, (at as i64 - mapping_header as i64) as u64);
        w.write_zeros(0x18);
        let starts_header = w.position();
        w.write_zeros(0x10);
        let bits_header = w.position();
        w.write_zeros(0x10);
        w.write_u32(mapping.bits_per_entry);
        w.write_u32(mapping.triangle_index_bit_mask);
        items.push(Item {
            type_index: type_index::PRIMITIVE_MAPPING,
            flags: ITEM_OBJECT,
            offset: at,
            count: 1,
        });
        let starts_at = w.position();
        patch_array(
            &mut w,
            starts_header,
            starts_at,
            mapping.section_start.len(),
            0,
        );
        for v in &mapping.section_start {
            w.write_u32(*v);
        }
        items.push(Item {
            type_index: type_index::UINT32,
            flags: ITEM_ARRAY,
            offset: starts_at,
            count: mapping.section_start.len(),
        });
        w.align(16)?;
        let bits_at = w.position();
        patch_array(&mut w, bits_header, bits_at, mapping.bit_string.len(), 0);
        for v in &mapping.bit_string {
            w.write_u32(*v);
        }
        items.push(Item {
            type_index: type_index::UINT32,
            flags: ITEM_ARRAY,
            offset: bits_at,
            count: mapping.bit_string.len(),
        });
        w.align(16)?;
    }

    Ok((w.into_inner(), items))
}

/// Serializes a complete `.bphsh` file (uncompressed).
pub fn write(shape: &BphshShape) -> io::Result<Vec<u8>> {
    write_with_type_section(shape, TOTK_TYPE_SECTION, TOTK_SDK_VERSION)
}

/// Serializes with an explicit TYPE section and SDK version string.
pub fn write_with_type_section(
    shape: &BphshShape,
    type_section: &[u8],
    sdk_version: &[u8; 8],
) -> io::Result<Vec<u8>> {
    let (data, items) = write_data(&shape.shape)?;
    let mut w = BinaryWriter::with_endian(Endian::Little);
    w.write_zeros(0x30); // Phive header, patched at the end
    let tag0_at = w.position();
    w.write_zeros(4);
    w.write_bytes(b"TAG0");
    w.write_bytes(&[0x40, 0x00, 0x00, 0x10]);
    w.write_bytes(b"SDKV");
    w.write_bytes(sdk_version);
    let data_at = w.position();
    w.write_bytes(&(((data.len() + 8) as u32) | 0x4000_0000).to_be_bytes());
    w.write_bytes(b"DATA");
    w.write_bytes(&data);
    debug_assert_eq!(w.position() % 16, 0);
    w.write_bytes(type_section);
    let indx_at = w.position();
    w.write_zeros(4);
    w.write_bytes(b"INDX");
    let item_at = w.position();
    w.write_zeros(4);
    w.write_bytes(b"ITEM");
    for item in &items {
        w.write_u32(item.type_index | item.flags);
        w.write_u32(item.offset as u32);
        w.write_u32(item.count as u32);
    }
    let end = w.position();
    w.write_u32_at(indx_at, ((end - indx_at) as u32).swap_bytes());
    w.write_u32_at(
        item_at,
        (((end - item_at) as u32) | 0x4000_0000).swap_bytes(),
    );
    w.write_u32_at(tag0_at, ((end - tag0_at) as u32).swap_bytes());
    let _ = data_at;
    let hkt_size = align16(end - tag0_at);
    w.seek(tag0_at + hkt_size);

    let table0_at = w.position();
    for m in &shape.materials {
        w.write_i32(m.material_id);
        w.write_i32(m.reserved);
        w.write_u64(m.flags);
    }
    w.align(16)?;
    let table0_size = w.position() - table0_at;
    let table1_at = w.position();
    for mask in &shape.collision_masks {
        w.write_u64(*mask);
    }
    w.align(16)?;
    let table1_size = w.position() - table1_at;
    let file_size = w.position();

    w.seek(0);
    w.write_bytes(b"Phive\0");
    w.write_u16(1);
    w.write_u16(0xFEFF);
    w.write_u8(0);
    w.write_u8(4);
    w.write_u32(tag0_at as u32);
    w.write_u32(table0_at as u32);
    w.write_u32(table1_at as u32);
    w.write_u32(file_size as u32);
    w.write_u32(hkt_size as u32);
    w.write_u32(table0_size as u32);
    w.write_u32(table1_size as u32);
    w.write_u32(0);
    w.write_u32(0);
    w.seek(file_size);
    Ok(w.into_inner())
}
