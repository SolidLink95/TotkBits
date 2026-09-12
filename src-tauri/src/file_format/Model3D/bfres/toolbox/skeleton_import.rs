//! Switch Toolbox's "Import Bones" model import option, applied to the
//! Toolbox object model: `FMDL.AddOjects` merges the FBX skeleton into the
//! existing one (bones re-ordered to the imported hierarchy, transforms only
//! replaced when they differ beyond a tolerance, new bones appended with
//! Euler rotations), then rebuilds every bone's flags through
//! `BfresBone.GenericToBfresBone` and regenerates the skinning palette from
//! the imported meshes' bone usage.

use super::matrix::{quat_from_euler, Quat};
use super::model::{Bone, Model};
use crate::parser::fbx::toolbox_skeleton::{MeshBoneUsage, ToolboxImportedBone};
use std::collections::{BTreeSet, HashMap};

/// `BoneFlags.Visible`.
const FLAG_VISIBLE: u32 = 0x0000_0001;
/// `BoneFlagsRotation.EulerXYZ` and the rotation field mask.
const FLAG_ROTATION_EULER: u32 = 0x0000_1000;
const FLAG_ROTATION_MASK: u32 = 0x0000_7000;
/// `BoneFlagsTransform` bits touched by `SetTransforms`.
const FLAG_SCALE_ONE: u32 = 0x0300_0000;
const FLAG_ROTATE_ZERO: u32 = 0x0400_0000;
const FLAG_TRANSLATE_ZERO: u32 = 0x0800_0000;

const TOLERANCE: f32 = 0.001;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkeletonImportReport {
    pub bones_before: usize,
    pub bones_after: usize,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub transforms_updated: Vec<String>,
    pub smooth_count: usize,
    pub rigid_count: usize,
}

/// Replaces `model.skeleton.bones` with the imported hierarchy, Toolbox
/// style, and returns the old-bone-index -> new-bone-index mapping (`None`
/// for bones that no longer exist).
pub fn merge_imported_bones(
    model: &mut Model,
    imported: &[ToolboxImportedBone],
) -> (Vec<Option<u16>>, SkeletonImportReport) {
    let existing = std::mem::take(&mut model.skeleton.bones);
    let mut report = SkeletonImportReport {
        bones_before: existing.len(),
        ..Default::default()
    };
    // `ExistingBones`: first bone of each name wins.
    let mut existing_by_name: HashMap<&str, usize> = HashMap::new();
    for (index, bone) in existing.iter().enumerate() {
        existing_by_name.entry(bone.name.as_str()).or_insert(index);
    }
    let mut old_to_new: Vec<Option<u16>> = vec![None; existing.len()];
    let mut bones = Vec::with_capacity(imported.len());
    for (new_index, source) in imported.iter().enumerate() {
        let mut bone = match existing_by_name.get(source.name.as_str()) {
            Some(&old_index) => {
                if old_to_new[old_index].is_none() {
                    old_to_new[old_index] = Some(new_index as u16);
                }
                let mut bone = existing[old_index].clone();
                let mut updated = false;
                if vector_length(sub3(bone.position, source.position)) > TOLERANCE {
                    bone.position = source.position;
                    updated = true;
                }
                if vector_length(sub3(bone.scale, source.scale)) > TOLERANCE {
                    bone.scale = source.scale;
                    updated = true;
                }
                let current = if bone.uses_euler() {
                    quat_from_euler_opentk(bone.rotation[0], bone.rotation[1], bone.rotation[2])
                } else {
                    Quat {
                        x: bone.rotation[0],
                        y: bone.rotation[1],
                        z: bone.rotation[2],
                        w: bone.rotation[3],
                    }
                };
                let dot = current.x * source.rotation[0]
                    + current.y * source.rotation[1]
                    + current.z * source.rotation[2]
                    + current.w * source.rotation[3];
                if dot.abs() < (1.0f32 - TOLERANCE) {
                    bone.rotation = if bone.uses_euler() {
                        [source.euler[0], source.euler[1], source.euler[2], 1.0]
                    } else {
                        source.rotation
                    };
                    updated = true;
                }
                if updated {
                    report.transforms_updated.push(source.name.clone());
                }
                bone
            }
            None => {
                // `new Bone()` + `CloneBaseInstance`: imported bones are Euler.
                report.added.push(source.name.clone());
                Bone {
                    name: source.name.clone(),
                    parent_index: -1,
                    smooth_matrix_index: -1,
                    rigid_matrix_index: -1,
                    billboard_index: -1,
                    flags: FLAG_VISIBLE | FLAG_ROTATION_EULER,
                    scale: source.scale,
                    rotation: [source.euler[0], source.euler[1], source.euler[2], 1.0],
                    position: source.position,
                    user_data: Vec::new(),
                }
            }
        };
        bone.name = source.name.clone();
        bone.parent_index = source.parent_index as i16;
        // GenericToBfresBone + SetTransforms.
        bone.flags = (bone.flags & !FLAG_VISIBLE) | FLAG_VISIBLE;
        let rotation_flag = if bone.uses_euler() {
            FLAG_ROTATION_EULER
        } else {
            0
        };
        bone.flags = (bone.flags & !FLAG_ROTATION_MASK) | rotation_flag;
        let rotate_zero = bone.rotation == [0.0, 0.0, 0.0, 1.0];
        let scale_one = bone.scale == [1.0, 1.0, 1.0];
        let translate_zero = bone.position == [0.0, 0.0, 0.0];
        bone.flags = set_bits(bone.flags, FLAG_ROTATE_ZERO, rotate_zero);
        bone.flags = set_bits(bone.flags, FLAG_SCALE_ONE, scale_one);
        bone.flags = set_bits(bone.flags, FLAG_TRANSLATE_ZERO, translate_zero);
        bones.push(bone);
    }
    for (old_index, mapping) in old_to_new.iter().enumerate() {
        if mapping.is_none() {
            report.removed.push(existing[old_index].name.clone());
        }
    }
    report.bones_after = bones.len();
    model.skeleton.bones = bones;
    (old_to_new, report)
}

/// The palette generation that follows every Toolbox model import: bones
/// used by single-influence meshes become rigid entries, every other used
/// bone a smooth entry, both lists sorted by bone index, smooth first.
pub fn regenerate_skinning_palette(
    model: &mut Model,
    mesh_bone_usage: &[MeshBoneUsage],
    report: &mut SkeletonImportReport,
) {
    let skeleton = &mut model.skeleton;
    for bone in &mut skeleton.bones {
        bone.smooth_matrix_index = -1;
        bone.rigid_matrix_index = -1;
    }
    let index_of =
        |name: &str| -> Option<usize> { skeleton.bones.iter().position(|bone| bone.name == name) };
    let mut smooth: Vec<usize> = Vec::new();
    let mut rigid: Vec<usize> = Vec::new();
    for mesh in mesh_bone_usage {
        let influences = mesh
            .vertices
            .iter()
            .map(|names| names.len())
            .max()
            .unwrap_or(0);
        for names in &mesh.vertices {
            for name in names {
                let Some(index) = index_of(name) else {
                    continue;
                };
                if influences == 1 {
                    if !rigid.contains(&index) {
                        rigid.push(index);
                    }
                } else if !smooth.contains(&index) {
                    smooth.push(index);
                }
            }
        }
    }
    smooth.sort_unstable();
    rigid.sort_unstable();
    for (slot, &index) in smooth.iter().enumerate() {
        skeleton.bones[index].smooth_matrix_index = slot as i16;
    }
    for (slot, &index) in rigid.iter().enumerate() {
        skeleton.bones[index].rigid_matrix_index = (smooth.len() + slot) as i16;
    }
    skeleton.matrix_to_bone = smooth
        .iter()
        .chain(rigid.iter())
        .map(|index| *index as u16)
        .collect();
    // Inverse matrices are regenerated by the saver for every smooth bone.
    skeleton.inverse_matrices.resize(
        smooth.len(),
        [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
    );
    report.smooth_count = smooth.len();
    report.rigid_count = rigid.len();
}

/// Points the model's shapes and vertex buffers at the regenerated palette:
/// `_i<N>` lanes are rewritten from the old matrix palette to the new one and
/// every shape takes the skin count and sorted bone list Toolbox derives from
/// the FBX mesh of the same name (`GetMaxSkinInfluenceCount` / `GetIndices`).
/// With `imported_bones` every shape also binds to bone 0, as Toolbox does
/// for imported objects when "Import Bones" is checked.
pub fn rebind_geometry(
    model: &mut Model,
    old_matrix_to_bone: &[u16],
    old_to_new: &[Option<u16>],
    mesh_bone_usage: &[MeshBoneUsage],
    imported_bones: bool,
) -> Result<(), String> {
    // Toolbox-style per-shape skinning, resolved before the palette lookup so
    // rigid/smooth slot preference follows the new skin count.
    let bone_index_of = |name: &str| -> Option<u16> {
        model
            .skeleton
            .bones
            .iter()
            .position(|bone| bone.name == name)
            .map(|index| index as u16)
    };
    let mut shape_skinning: Vec<Option<(u8, Vec<u16>)>> = Vec::with_capacity(model.shapes.len());
    for (shape_index, shape) in model.shapes.iter().enumerate() {
        let usage = mesh_bone_usage
            .iter()
            .find(|mesh| mesh.name == shape.name)
            .or_else(|| mesh_bone_usage.get(shape_index));
        shape_skinning.push(usage.map(|mesh| {
            let skin_count = mesh
                .vertices
                .iter()
                .map(|names| names.len())
                .max()
                .unwrap_or(0)
                .min(255) as u8;
            let mut indices: Vec<u16> = mesh
                .vertices
                .iter()
                .flatten()
                .filter_map(|name| bone_index_of(name))
                .collect();
            indices.sort_unstable();
            indices.dedup();
            (skin_count, indices)
        }));
    }
    for (shape, skinning) in model.shapes.iter_mut().zip(&shape_skinning) {
        if let Some((skin_count, _)) = skinning {
            shape.vertex_skin_count = *skin_count;
            if let Some(buffer) = model
                .vertex_buffers
                .get_mut(usize::from(shape.vertex_buffer_index))
            {
                buffer.vertex_skin_count = u16::from(*skin_count);
            }
        }
    }

    let skeleton = &model.skeleton;
    let root_slot = skeleton
        .bones
        .iter()
        .find(|bone| bone.smooth_matrix_index != -1 || bone.rigid_matrix_index != -1)
        .map(|bone| {
            if bone.smooth_matrix_index != -1 {
                bone.smooth_matrix_index as u16
            } else {
                bone.rigid_matrix_index as u16
            }
        })
        .unwrap_or(0);
    let slot_for = |old_slot: u8, prefer_rigid: bool| -> u16 {
        let Some(old_bone) = old_matrix_to_bone.get(usize::from(old_slot)) else {
            return root_slot;
        };
        let Some(Some(new_bone)) = old_to_new.get(usize::from(*old_bone)) else {
            return root_slot;
        };
        let bone = &skeleton.bones[usize::from(*new_bone)];
        let (first, second) = if prefer_rigid {
            (bone.rigid_matrix_index, bone.smooth_matrix_index)
        } else {
            (bone.smooth_matrix_index, bone.rigid_matrix_index)
        };
        if first != -1 {
            first as u16
        } else if second != -1 {
            second as u16
        } else {
            root_slot
        }
    };

    let mut remapped_buffers = BTreeSet::new();
    for shape in &model.shapes {
        let buffer_index = usize::from(shape.vertex_buffer_index);
        if !remapped_buffers.insert(buffer_index) {
            continue;
        }
        let prefer_rigid = shape.vertex_skin_count <= 1;
        let Some(buffer) = model.vertex_buffers.get(buffer_index) else {
            continue;
        };
        let lanes = usize::from(buffer.vertex_skin_count).max(1);
        let mut edits: Vec<(usize, usize, usize, usize)> = Vec::new();
        for attribute in &buffer.attributes {
            if !attribute.name.starts_with("_i") {
                continue;
            }
            let width = match u16::from_be_bytes(attribute.format) {
                0x030B => 4,
                0x0309 => 2,
                0x0302 => 1,
                other => {
                    return Err(format!(
                        "cannot rebind skin indices stored as vertex format 0x{other:04X}"
                    ))
                }
            };
            let Some(data) = buffer.buffers.get(usize::from(attribute.buffer_index)) else {
                continue;
            };
            let stride = data.stride as usize;
            let available = stride.saturating_sub(usize::from(attribute.offset));
            edits.push((
                usize::from(attribute.buffer_index),
                usize::from(attribute.offset),
                lanes.min(width).min(available),
                stride,
            ));
        }
        let vertex_count = buffer.vertex_count as usize;
        let buffer = &mut model.vertex_buffers[buffer_index];
        for (data_index, offset, lanes, stride) in edits {
            let data = &mut buffer.buffers[data_index].data;
            for vertex in 0..vertex_count {
                let start = vertex * stride + offset;
                for lane in 0..lanes {
                    let byte = &mut data[start + lane];
                    let slot = slot_for(*byte, prefer_rigid);
                    *byte = u8::try_from(slot)
                        .map_err(|_| "matrix palette exceeds 255 entries".to_owned())?;
                }
            }
        }
    }
    for (shape, skinning) in model.shapes.iter_mut().zip(&shape_skinning) {
        match skinning {
            Some((_, indices)) => shape.skin_bone_indices = indices.clone(),
            None => {
                let mut indices: Vec<u16> = shape
                    .skin_bone_indices
                    .iter()
                    .filter_map(|old| old_to_new.get(usize::from(*old)).copied().flatten())
                    .collect();
                indices.sort_unstable();
                indices.dedup();
                shape.skin_bone_indices = indices;
            }
        }
        if imported_bones {
            shape.bone_index = 0;
        } else if let Some(Some(new_index)) = old_to_new.get(usize::from(shape.bone_index)) {
            shape.bone_index = *new_index;
        }
    }
    Ok(())
}

fn set_bits(flags: u32, mask: u32, on: bool) -> u32 {
    if on {
        flags | mask
    } else {
        flags & !mask
    }
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// OpenTK `Vector3.Length`.
fn vector_length(v: [f32; 3]) -> f32 {
    f64::from(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() as f32
}

/// `STMath.FromEulerAngles` (OpenTK quaternions); only used for the
/// tolerance comparison, so the System.Numerics port is close enough.
fn quat_from_euler_opentk(x: f32, y: f32, z: f32) -> Quat {
    quat_from_euler(x, y, z)
}
