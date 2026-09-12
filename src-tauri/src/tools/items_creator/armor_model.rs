//! Procedural placeholder geometry for custom armor: a single skinned cube
//! that replaces every shape of a cloned vanilla armor BFRES while keeping
//! the model's skeleton, material and vertex layout, so the game can render
//! and animate it exactly like the original piece.

use crate::file_format::Model3D::bfres::{
    encode_vertex_attribute,
    toolbox::{
        bone_world_position, Material, Mesh, ResFile, Shape, VertexAttrib, VertexBuffer, VertexData,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};

/// A cube centred on a bone, skinned to up to four bones.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CubeModelSpec {
    /// Edge lengths in metres (X, Y, Z).
    #[serde(default = "default_size")]
    pub size: [f32; 3],
    /// Bone whose bind-pose position becomes the cube centre. Defaults to the
    /// heaviest-weighted bone.
    #[serde(default)]
    pub center_bone: Option<String>,
    /// Model-space offset added to the centre.
    #[serde(default)]
    pub offset: [f32; 3],
    /// Skin weights by bone name. Normalised on write; at most four bones.
    pub weights: BTreeMap<String, f32>,
}

fn default_size() -> [f32; 3] {
    [0.25, 0.25, 0.25]
}

/// What was written, for the generation report.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CubeModelReport {
    pub center: [f32; 3],
    pub size: [f32; 3],
    pub material: String,
    pub shape: String,
    /// `(bone, matrix palette index, weight)` in slot order.
    pub influences: Vec<(String, u16, f32)>,
    /// Bones that had to be added to the skeleton's smooth matrix palette.
    pub palette_added: Vec<String>,
    /// Template shapes kept next to the cube: Link's own skin and hair parts
    /// (materials textured with `Link_*`), which the game expects the armor
    /// to supply once the body's `G_Upper`/`G_Head` groups are hidden.
    pub kept_shapes: Vec<String>,
}

const CUBE_VERTEX_COUNT: usize = 24;
const CUBE_INDEX_COUNT: usize = 36;

/// Replaces the first model's geometry with one skinned cube. `project` is the
/// model project name (`Armor_001`) used to recognise the armor's own textured
/// material among shared skin/hair materials.
pub fn replace_with_skinned_cube(
    file: &mut ResFile,
    spec: &CubeModelSpec,
    project: &str,
) -> io::Result<CubeModelReport> {
    if spec.weights.is_empty() || spec.weights.len() > 4 {
        return Err(invalid("a cube needs between one and four skin weights"));
    }
    if spec.size.iter().any(|value| !(*value > 0.0)) {
        return Err(invalid("cube size components must be positive"));
    }
    let model = file
        .models
        .first_mut()
        .ok_or_else(|| invalid("BFRES contains no model"))?;
    if model.shapes.is_empty() || model.vertex_buffers.is_empty() {
        return Err(invalid("template model has no shapes"));
    }

    // Influences, heaviest first.
    let mut influences: Vec<(String, f32)> = spec
        .weights
        .iter()
        .map(|(bone, weight)| (bone.clone(), *weight))
        .collect();
    if influences.iter().any(|(_, weight)| !(*weight > 0.0)) {
        return Err(invalid("skin weights must be positive"));
    }
    influences.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let total: f32 = influences.iter().map(|(_, weight)| weight).sum();
    for (_, weight) in &mut influences {
        *weight /= total;
    }
    let bone_index = |name: &str| -> io::Result<usize> {
        model
            .skeleton
            .bones
            .iter()
            .position(|bone| bone.name == name)
            .ok_or_else(|| invalid(format!("bone {name} is not part of the template skeleton")))
    };
    let influence_bones: Vec<usize> = influences
        .iter()
        .map(|(name, _)| bone_index(name))
        .collect::<io::Result<_>>()?;
    let center_bone = match &spec.center_bone {
        Some(name) => bone_index(name)?,
        None => influence_bones[0],
    };

    // Template shape: the armor's own textured material, largest shape.
    let is_own_material = |material: &Material| {
        material
            .texture_refs
            .iter()
            .any(|texture| texture.starts_with(&format!("{project}_")) && texture.contains("_Alb"))
    };
    let candidates: Vec<usize> = (0..model.shapes.len())
        .filter(|&index| {
            model
                .materials
                .get(usize::from(model.shapes[index].material_index))
                .is_some_and(is_own_material)
        })
        .collect();
    let pool = if candidates.is_empty() {
        (0..model.shapes.len()).collect()
    } else {
        candidates
    };
    let template_index = pool
        .into_iter()
        .max_by_key(|&index| {
            model
                .vertex_buffers
                .get(usize::from(model.shapes[index].vertex_buffer_index))
                .map(|buffer| buffer.vertex_count)
                .unwrap_or(0)
        })
        .ok_or_else(|| invalid("template model has no usable shape"))?;
    let template_shape = model.shapes[template_index].clone();
    let template_buffer = model
        .vertex_buffers
        .get(usize::from(template_shape.vertex_buffer_index))
        .cloned()
        .ok_or_else(|| invalid("template shape points at a missing vertex buffer"))?;
    let template_mesh = template_shape
        .meshes
        .first()
        .cloned()
        .ok_or_else(|| invalid("template shape has no LOD mesh"))?;
    let material = model
        .materials
        .get(usize::from(template_shape.material_index))
        .cloned()
        .ok_or_else(|| invalid("template shape points at a missing material"))?;

    // Every influence needs a smooth matrix palette slot. Vanilla armor leaves
    // Root/Skl_Root out of the palette, so extend it while keeping the palette
    // in bone order (the saver regenerates inverse matrices in that order).
    let skeleton = &mut model.skeleton;
    let old_smooth_count = skeleton.inverse_matrices.len();
    let mut palette_added = Vec::new();
    let mut smooth_bones: Vec<usize> = (0..skeleton.bones.len())
        .filter(|&index| skeleton.bones[index].smooth_matrix_index != -1)
        .collect();
    for &index in &influence_bones {
        if !smooth_bones.contains(&index) {
            smooth_bones.push(index);
            palette_added.push(skeleton.bones[index].name.clone());
        }
    }
    let old_palette = skeleton.matrix_to_bone.clone();
    if !palette_added.is_empty() {
        smooth_bones.sort_unstable();
        let rigid_tail: Vec<u16> = skeleton
            .matrix_to_bone
            .get(old_smooth_count..)
            .map(<[u16]>::to_vec)
            .unwrap_or_default();
        let new_smooth_count = smooth_bones.len();
        for (index, bone) in skeleton.bones.iter_mut().enumerate() {
            bone.smooth_matrix_index = smooth_bones
                .iter()
                .position(|candidate| *candidate == index)
                .map(|slot| slot as i16)
                .unwrap_or(-1);
            if bone.rigid_matrix_index != -1 {
                let rigid_slot = usize::try_from(bone.rigid_matrix_index)
                    .ok()
                    .and_then(|value| value.checked_sub(old_smooth_count))
                    .ok_or_else(|| invalid("template skeleton rigid matrix index is invalid"))?;
                bone.rigid_matrix_index = (new_smooth_count + rigid_slot) as i16;
            }
        }
        skeleton.matrix_to_bone = smooth_bones
            .iter()
            .map(|index| *index as u16)
            .chain(rigid_tail)
            .collect();
        skeleton.inverse_matrices.resize(
            new_smooth_count,
            [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        );
    }
    let matrix_slots: Vec<u16> = influence_bones
        .iter()
        .map(|&index| skeleton.bones[index].smooth_matrix_index as u16)
        .collect();
    // Old palette slot -> new palette slot, for the template shapes kept below.
    let palette_remap: Vec<u16> = old_palette
        .iter()
        .enumerate()
        .map(|(old_slot, bone)| {
            let bone = &skeleton.bones[usize::from(*bone)];
            if old_slot < old_smooth_count {
                bone.smooth_matrix_index as u16
            } else {
                bone.rigid_matrix_index as u16
            }
        })
        .collect();

    // Geometry in model (bind-pose) space.
    let anchor = bone_world_position(&skeleton.bones, center_bone);
    let center = [
        anchor[0] + spec.offset[0],
        anchor[1] + spec.offset[1],
        anchor[2] + spec.offset[2],
    ];
    let half = [spec.size[0] / 2.0, spec.size[1] / 2.0, spec.size[2] / 2.0];
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        // normal, tangent (u direction), bitangent (v direction)
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ];
    let mut positions = Vec::with_capacity(CUBE_VERTEX_COUNT);
    let mut normals = Vec::with_capacity(CUBE_VERTEX_COUNT);
    let mut tangents = Vec::with_capacity(CUBE_VERTEX_COUNT);
    let mut uvs = Vec::with_capacity(CUBE_VERTEX_COUNT);
    let mut indices: Vec<u32> = Vec::with_capacity(CUBE_INDEX_COUNT);
    for (face, (normal, tangent, bitangent)) in faces.iter().enumerate() {
        let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        for (u, v) in corners {
            let mut position = [0.0f32; 3];
            for axis in 0..3 {
                position[axis] = center[axis]
                    + half[axis] * (normal[axis] + u * tangent[axis] + v * bitangent[axis]);
            }
            positions.push(position);
            normals.push(*normal);
            tangents.push([tangent[0], tangent[1], tangent[2], 1.0]);
            uvs.push([(u + 1.0) / 2.0, 1.0 - (v + 1.0) / 2.0]);
        }
        let base = (face * 4) as u32;
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    // Weights as the unorm8 bytes the game reads, summing to exactly 255.
    let mut weight_bytes: Vec<u8> = influences
        .iter()
        .map(|(_, weight)| (weight * 255.0).round().clamp(0.0, 255.0) as u8)
        .collect();
    let sum: i32 = weight_bytes.iter().map(|value| i32::from(*value)).sum();
    weight_bytes[0] = (i32::from(weight_bytes[0]) + (255 - sum)).clamp(1, 255) as u8;
    let mut slot_indices = [0.0f32; 4];
    let mut slot_weights = [0.0f32; 4];
    for (slot, (matrix, weight)) in matrix_slots.iter().zip(&weight_bytes).enumerate() {
        slot_indices[slot] = f32::from(*matrix);
        slot_weights[slot] = f32::from(*weight) / 255.0;
    }

    // Vertex buffers in the template's exact attribute layout.
    let mut buffers: Vec<VertexData> = template_buffer
        .buffers
        .iter()
        .map(|buffer| VertexData {
            data: vec![0u8; buffer.stride as usize * CUBE_VERTEX_COUNT],
            stride: buffer.stride,
        })
        .collect();
    // Overlapping packed attributes must be written in offset order so the
    // later one wins the shared byte.
    let mut ordered_attributes: Vec<&VertexAttrib> = template_buffer.attributes.iter().collect();
    ordered_attributes.sort_by_key(|attribute| (attribute.buffer_index, attribute.offset));
    for attribute in ordered_attributes {
        let buffer = buffers
            .get_mut(usize::from(attribute.buffer_index))
            .ok_or_else(|| invalid("template vertex attribute points at a missing buffer"))?;
        let format = u16::from_be_bytes(attribute.format);
        let stride = buffer.stride as usize;
        let name = attribute.name.as_str();
        let components = if name.starts_with("_p") || name.starts_with("_n") {
            3
        } else if name.starts_with("_u") {
            2
        } else {
            4
        };
        for vertex in 0..CUBE_VERTEX_COUNT {
            let value: [f32; 4] = if name.starts_with("_p") {
                let p = positions[vertex];
                [p[0], p[1], p[2], 1.0]
            } else if name.starts_with("_n") {
                let n = normals[vertex];
                [n[0], n[1], n[2], 0.0]
            } else if name.starts_with("_t") {
                tangents[vertex]
            } else if name.starts_with("_b") {
                let n = normals[vertex];
                let t = tangents[vertex];
                [
                    n[1] * t[2] - n[2] * t[1],
                    n[2] * t[0] - n[0] * t[2],
                    n[0] * t[1] - n[1] * t[0],
                    1.0,
                ]
            } else if name.starts_with("_u") {
                let uv = uvs[vertex];
                [uv[0], uv[1], 0.0, 1.0]
            } else if name.starts_with("_i") {
                slot_indices
            } else if name.starts_with("_w") {
                slot_weights
            } else if name.starts_with("_c") {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.0; 4]
            };
            // Vanilla three-influence layouts pack `_i0`/`_w0` into three
            // bytes each although their format code is four bytes wide (the
            // GPU only reads `vertex_skin_count` lanes), so encode into a
            // scratch slot and clip to what fits before the next attribute.
            let mut scratch = [0u8; 16];
            let written = encode_vertex_attribute(&mut scratch, 0, format, value, components)
                .map_err(|error| {
                    invalid(format!(
                        "cannot encode attribute {name} (format 0x{format:04X}): {}",
                        error.message
                    ))
                })?;
            let start = vertex * stride + usize::from(attribute.offset);
            let available = stride.saturating_sub(usize::from(attribute.offset));
            let width = written.min(available);
            if width == 0 {
                return Err(invalid(format!(
                    "attribute {name} at offset {} does not fit stride {stride}",
                    attribute.offset
                )));
            }
            buffer.data[start..start + width].copy_from_slice(&scratch[..width]);
        }
    }
    let vertex_buffer = VertexBuffer {
        flags: template_buffer.flags,
        attributes: template_buffer.attributes.clone(),
        buffers,
        vertex_count: CUBE_VERTEX_COUNT as u32,
        vertex_skin_count: influences.len() as u16,
        gpu_alignment: template_buffer.gpu_alignment,
    };

    let index_data = match template_mesh.index_format {
        1 => indices
            .iter()
            .flat_map(|index| (*index as u16).to_le_bytes())
            .collect::<Vec<u8>>(),
        2 => indices
            .iter()
            .flat_map(|index| index.to_le_bytes())
            .collect::<Vec<u8>>(),
        other => {
            return Err(invalid(format!(
                "template mesh uses unsupported index format {other}"
            )))
        }
    };
    let mesh = Mesh {
        submeshes: vec![(0, CUBE_INDEX_COUNT as u32)],
        buffer_flag: template_mesh.buffer_flag,
        data: index_data,
        primitive_type: template_mesh.primitive_type,
        index_format: template_mesh.index_format,
        index_count: CUBE_INDEX_COUNT as u32,
        first_vertex: 0,
    };
    let mut skin_bone_indices: Vec<u16> =
        influence_bones.iter().map(|index| *index as u16).collect();
    skin_bone_indices.sort_unstable();
    skin_bone_indices.dedup();
    let radius = (half[0] * half[0] + half[1] * half[1] + half[2] * half[2]).sqrt() + 1.0;
    let bounding = [center[0], center[1], center[2], half[0], half[1], half[2]];
    let shape_name = format!("Cube__{}", material.name);
    let shape = Shape {
        flags: template_shape.flags,
        name: shape_name.clone(),
        vertex_buffer_index: 0,
        meshes: vec![mesh],
        skin_bone_indices: skin_bone_indices.clone(),
        boundings: vec![bounding; 2],
        radius_list: vec![[center[0], center[1], center[2], radius]; skin_bone_indices.len()],
        material_index: 0,
        bone_index: template_shape.bone_index,
        vertex_skin_count: influences.len() as u8,
        target_attrib_count: template_shape.target_attrib_count,
    };
    // Keep the template's body parts (skin, hair: materials textured with
    // `Link_*`) so hiding the body's own groups does not leave Link without
    // arms or hair. Everything else is the garment the cube replaces.
    let is_body_material = |material: &Material| {
        material
            .texture_refs
            .iter()
            .any(|texture| texture.starts_with("Link_"))
    };
    let kept_indices: Vec<usize> = (0..model.shapes.len())
        .filter(|&index| {
            index != template_index
                && model
                    .materials
                    .get(usize::from(model.shapes[index].material_index))
                    .is_some_and(is_body_material)
        })
        .collect();
    let mut materials = vec![material];
    let mut vertex_buffers = vec![vertex_buffer];
    let mut shapes = vec![shape];
    let mut kept_shapes = Vec::new();
    let mut material_slots: BTreeMap<u16, u16> = BTreeMap::new();
    let mut buffer_slots: BTreeMap<u16, u16> = BTreeMap::new();
    for index in kept_indices {
        let mut kept = model.shapes[index].clone();
        let material_slot = *material_slots
            .entry(kept.material_index)
            .or_insert_with(|| {
                materials.push(model.materials[usize::from(kept.material_index)].clone());
                (materials.len() - 1) as u16
            });
        let buffer_slot = match buffer_slots.get(&kept.vertex_buffer_index) {
            Some(slot) => *slot,
            None => {
                let mut buffer =
                    model.vertex_buffers[usize::from(kept.vertex_buffer_index)].clone();
                if !palette_added.is_empty() {
                    remap_skin_indices(&mut buffer, &palette_remap)?;
                }
                vertex_buffers.push(buffer);
                let slot = (vertex_buffers.len() - 1) as u16;
                buffer_slots.insert(kept.vertex_buffer_index, slot);
                slot
            }
        };
        kept.material_index = material_slot;
        kept.vertex_buffer_index = buffer_slot;
        kept_shapes.push(kept.name.clone());
        shapes.push(kept);
    }
    let material_name = materials[0].name.clone();
    model.materials = materials;
    model.vertex_buffers = vertex_buffers;
    model.shapes = shapes;

    Ok(CubeModelReport {
        center,
        size: spec.size,
        material: material_name,
        shape: shape_name,
        influences: influences
            .iter()
            .zip(&matrix_slots)
            .zip(&weight_bytes)
            .map(|(((bone, _), matrix), weight)| {
                (bone.clone(), *matrix, f32::from(*weight) / 255.0)
            })
            .collect(),
        palette_added,
        kept_shapes,
    })
}

/// Rewrites every `_i<N>` lane of a vertex buffer through `remap` (old matrix
/// palette slot -> new slot) after the skeleton palette was extended. Only the
/// 8-bit index formats used by TOTK armor are handled.
fn remap_skin_indices(buffer: &mut VertexBuffer, remap: &[u16]) -> io::Result<()> {
    let lanes = usize::from(buffer.vertex_skin_count).max(1);
    for attribute in &buffer.attributes {
        if !attribute.name.starts_with("_i") {
            continue;
        }
        let format = u16::from_be_bytes(attribute.format);
        let width = match format {
            0x030B => 4,
            0x0309 => 2,
            0x0302 => 1,
            other => {
                return Err(invalid(format!(
                    "cannot remap skin indices in vertex format 0x{other:04X}"
                )))
            }
        };
        let data = buffer
            .buffers
            .get_mut(usize::from(attribute.buffer_index))
            .ok_or_else(|| invalid("skin index attribute points at a missing buffer"))?;
        let stride = data.stride as usize;
        let available = stride.saturating_sub(usize::from(attribute.offset));
        let lanes = lanes.min(width).min(available);
        for vertex in 0..buffer.vertex_count as usize {
            let start = vertex * stride + usize::from(attribute.offset);
            for lane in 0..lanes {
                let byte = &mut data.data[start + lane];
                let new = remap.get(usize::from(*byte)).copied().ok_or_else(|| {
                    invalid(format!("template vertex references palette slot {byte} outside the skeleton palette"))
                })?;
                *byte = u8::try_from(new)
                    .map_err(|_| invalid("extended matrix palette exceeds 255 entries"))?;
            }
        }
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
