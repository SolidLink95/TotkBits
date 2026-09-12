use super::{align_up, BfresError, BfresFile, BfresSection, Endian};
use crate::parser::{
    binary::{BinaryReader, BinaryWriter, Endian as BinaryEndian},
    fbx::import::{calculate_tangents, import_for_bfres, ImportedMesh},
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone)]
struct Attribute {
    name: String,
    format: u16,
    offset: usize,
    buffer: usize,
}

pub(super) fn replace_geometry_from_fbx(data: &[u8], fbx: &[u8]) -> Result<Vec<u8>, BfresError> {
    let parsed = BfresFile::from_bytes(data)
        .map_err(|e| error(e.offset, format!("while parsing source BFRES: {e}")))?;
    if parsed.header.version[2] != 10 || parsed.header.endian != Endian::Little {
        return Err(error(
            0,
            "FBX geometry replacement currently requires little-endian BFRES v10",
        ));
    }
    let mut imported = import_for_bfres(fbx).map_err(|e| error(0, e.to_string()))?;
    validate_compatible_skeleton(&parsed, &imported.bones, &imported.meshes)?;
    // `BfresFile` deliberately keeps a broad signature index, which can also
    // find the bytes "FSHP" inside payload data. Geometry replacement needs
    // actual resource headers, so reject candidates whose mandatory pointers
    // do not address the fixed BFRES region.
    let shape_reader = BinaryReader::with_endian(data, BinaryEndian::Little);
    let mut shapes: Vec<_> = parsed
        .sections_with_signature(b"FSHP")
        .filter(|shape| {
            let offset = shape.offset as usize;
            offset.checked_add(96).is_some_and(|end| end <= data.len())
                && read_u64(&shape_reader, offset + 16)
                    .is_ok_and(|pointer| pointer != 0 && (pointer as usize) < data.len())
                && read_u64(&shape_reader, offset + 24)
                    .is_ok_and(|pointer| pointer != 0 && (pointer as usize) < data.len())
        })
        .cloned()
        .collect();
    shapes.sort_by_key(|shape| shape.offset);
    let template_has_skin = shapes.iter().any(|shape| {
        read_u16(&shape_reader, shape.offset as usize + 88).is_ok_and(|count| count != 0)
    });
    if imported.meshes.len() > shapes.len() {
        return Err(error(
            0,
            format!(
                "FBX has {} meshes but the BFRES has only {} shape slots",
                imported.meshes.len(),
                shapes.len()
            ),
        ));
    }
    orient_meshes_to_source_bind_space(&mut imported.meshes, &parsed.render.meshes)
        .map_err(|e| error(e.offset, format!("while orienting imported meshes: {e}")))?;
    for mesh in &mut imported.meshes {
        weld_mesh_vertices(mesh)?;
        calculate_tangents(mesh).map_err(|failure| {
            error(
                0,
                format!("failed to rebuild welded tangent space: {failure}"),
            )
        })?;
    }
    order_meshes_for_shapes(&mut imported.meshes, &shapes);
    if imported.meshes.is_empty() || shapes.is_empty() {
        return Err(error(0, "model contains no replaceable meshes"));
    }
    let material_names: Vec<_> = parsed
        .materials
        .iter()
        .map(|material| material.name.clone())
        .collect();
    if material_names.is_empty() {
        return Err(error(0, "BFRES model contains no materials"));
    }

    let reader = BinaryReader::with_endian(data, BinaryEndian::Little);
    let buffer_info = read_u64(&reader, 0xB0)? as usize;
    let buffer_base = if buffer_info != 0 && buffer_info + 16 <= data.len() {
        read_u64(&reader, buffer_info + 8)? as usize
    } else if read_u8(&reader, 0xEE)? & 1 != 0 || read_u8(&reader, 0xEF)? != 0 {
        parsed.header.file_size as usize + 288
    } else {
        return Err(error(0xB0, "BFRES has no external geometry buffer"));
    };
    let root = parsed
        .render
        .bones
        .iter()
        .position(|b| b.name == "Root")
        .or_else(|| parsed.render.bones.iter().position(|b| b.parent_index < 0))
        // Rigid weapon BFRES files can omit the decoded FSKL graph while their
        // shapes still conventionally address bone/matrix slot zero.
        .unwrap_or(0) as u16;
    let matrix_by_bone: HashMap<u16, u16> = parsed
        .render
        .matrix_to_bone
        .iter()
        .enumerate()
        .filter_map(|(matrix, &bone)| u16::try_from(matrix).ok().map(|m| (bone, m)))
        .collect();
    let imported_to_bfres: Vec<u16> = imported
        .bones
        .iter()
        .map(|(name, _)| {
            parsed
                .render
                .bones
                .iter()
                .position(|b| b.name == *name)
                .map(|v| v as u16)
                .unwrap_or(root)
        })
        .collect();

    for mesh in &mut imported.meshes {
        remap_weights(mesh, &imported_to_bfres, root)?;
        if !template_has_skin {
            for joints in &mut mesh.bone_indices {
                *joints = [0; 8];
            }
            for weights in &mut mesh.bone_weights {
                *weights = [0.0; 8];
            }
        }
    }

    let mut writer = BinaryWriter::from_vec(data.to_vec(), BinaryEndian::Little);
    writer.seek(align_up(data.len().max(buffer_base), 256));
    let mut used_streams = BTreeSet::new();
    let mut added_skin_palette = false;
    for (shape_index, shape) in shapes.iter().enumerate() {
        let shape_offset = shape.offset as usize;
        let stream_offset = read_u64(&reader, shape_offset + 16)? as usize;
        if !used_streams.insert(stream_offset) && shape_index < imported.meshes.len() {
            return Err(error(
                shape_offset,
                "multiple BFRES shapes share one vertex stream",
            ));
        }
        let mesh_offset = read_u64(&reader, shape_offset + 24)? as usize;
        let mesh_entry = mesh_offset;
        if shape_index >= imported.meshes.len() {
            put_u32(&mut writer, mesh_entry + 44, 0);
            continue;
        }
        let mut skin_offset = read_u64(&reader, shape_offset + 32)? as usize;
        let mut skin_capacity = read_u16(&reader, shape_offset + 88)
            .map_err(|e| error(e.offset, format!("FSHP skin count: {e}")))?
            as usize;
        let mesh = &mut imported.meshes[shape_index];
        let has_active_skin = mesh
            .bone_weights
            .iter()
            .any(|weights| weights.iter().take(4).any(|weight| *weight > 0.0));
        if skin_capacity == 0 && has_active_skin {
            writer.align(2).map_err(|error| {
                BfresError::new(
                    shape_offset,
                    format!("skin palette alignment failed: {error}"),
                )
            })?;
            skin_offset = writer.position();
            writer.write_u16(root);
            skin_capacity = 1;
            added_skin_palette = true;
            put_u64(&mut writer, shape_offset + 32, skin_offset as u64);
        }
        constrain_skin_palette(mesh, skin_capacity, root);
        validate_mesh(mesh)?;
        let (minimum, maximum) = bounds(&mesh.positions)?;
        let mut center = [
            (minimum[0] + maximum[0]) * 0.5,
            (minimum[1] + maximum[1]) * 0.5,
            (minimum[2] + maximum[2]) * 0.5,
        ];
        for value in &mut center {
            if *value != 0.0 && value.abs() < 1.0e-6 {
                *value = -value.abs();
            }
        }
        let extent = [
            (maximum[0] - minimum[0]) * 0.5,
            (maximum[1] - minimum[1]) * 0.5,
            (maximum[2] - minimum[2]) * 0.5,
        ];
        let length = |value: [f32; 3]| {
            (f64::from(value[0]).powi(2)
                + f64::from(value[1]).powi(2)
                + f64::from(value[2]).powi(2))
            .sqrt()
        };
        let radius = (length(center) + length(extent)) as f32;
        let bounds_offset = read_u64(&reader, shape_offset + 56)? as usize;
        let submesh_count = read_u16(&reader, mesh_entry + 52)
            .map_err(|e| error(e.offset, format!("FMES submesh count: {e}")))?
            as usize;
        for bound_index in 0..=submesh_count {
            let target = bounds_offset + bound_index * 24;
            for (component, value) in center.into_iter().chain(extent).enumerate() {
                put_f32(&mut writer, target + component * 4, value);
            }
        }
        let radius_offset = read_u64(&reader, shape_offset + 64)? as usize;
        if radius_offset != 0 {
            put_f32(&mut writer, radius_offset + 12, radius);
        }
        let mut skin_bones = BTreeSet::new();
        let mut vertex_skin_count = 0usize;
        for (joints, weights) in mesh.bone_indices.iter().zip(&mesh.bone_weights) {
            let active = weights
                .iter()
                .take(4)
                .filter(|weight| **weight > 0.0)
                .count();
            vertex_skin_count = vertex_skin_count.max(active);
            for slot in 0..active {
                skin_bones.insert(joints[slot]);
            }
        }
        if skin_capacity == 0 {
            skin_bones.clear();
            vertex_skin_count = 0;
        }
        for (index, bone) in skin_bones.iter().enumerate() {
            put_u16(&mut writer, skin_offset + index * 2, *bone);
        }
        put_u16(&mut writer, shape_offset + 88, skin_bones.len() as u16);
        put_u8(
            &mut writer,
            shape_offset + 90,
            vertex_skin_count.min(4) as u8,
        );
        put_u16(
            &mut writer,
            stream_offset + 84,
            vertex_skin_count.min(4) as u16,
        );
        // Replacing a shape in Toolbox replaces its complete LOD mesh list
        // with the single mesh imported from the FBX. Do not leave stale lower
        // detail meshes from the template attached to the new vertex stream.
        put_u8(&mut writer, shape_offset + 91, 1);
        let attr_offset = read_u64(&reader, stream_offset + 8)? as usize;
        let sizes_offset = read_u64(&reader, stream_offset + 48)? as usize;
        let strides_offset = read_u64(&reader, stream_offset + 56)? as usize;
        let attr_count = read_u8(&reader, stream_offset + 76)? as usize;
        let buffer_count = read_u8(&reader, stream_offset + 77)? as usize;
        let alignment = read_u16(&reader, stream_offset + 86)
            .map_err(|e| error(e.offset, format!("FVTX alignment: {e}")))?
            as usize;
        let attributes = (0..attr_count)
            .map(|i| {
                let entry = attr_offset + i * 16;
                Ok(Attribute {
                    name: super::read_string(data, read_u64(&reader, entry)?)
                        .unwrap_or_else(|| format!("attribute_{i}")),
                    format: BinaryReader::with_endian(data, BinaryEndian::Big)
                        .read_u16_at(entry + 8)
                        .map_err(io_error(entry + 8))?,
                    offset: read_u16(&reader, entry + 12)
                        .map_err(|e| error(e.offset, format!("FVTX attribute {i}: {e}")))?
                        as usize,
                    buffer: read_u8(&reader, entry + 14)? as usize,
                })
            })
            .collect::<Result<Vec<_>, BfresError>>()?;
        for buffer in 0..buffer_count {
            let stride = read_u32(&reader, strides_offset + buffer * 16)? as usize;
            let mut bytes =
                vec![
                    0;
                    stride
                        .checked_mul(mesh.positions.len())
                        .ok_or_else(|| error(stream_offset, "vertex buffer size overflow"))?
                ];
            for attribute in attributes.iter().filter(|a| a.buffer == buffer) {
                for vertex in 0..mesh.positions.len() {
                    let value = attribute_value(attribute, mesh, vertex, &matrix_by_bone, root)?;
                    encode(
                        &mut bytes,
                        vertex * stride + attribute.offset,
                        attribute.format,
                        value,
                        attribute_component_count(&attribute.name),
                    )
                    .map_err(|failure| {
                        error(
                            failure.offset,
                            format!(
                                "{} (attribute {}, format 0x{:04X}, buffer {}, stride {}, offset {}, vertex {})",
                                failure.message,
                                attribute.name,
                                attribute.format,
                                buffer,
                                stride,
                                attribute.offset,
                                vertex
                            ),
                        )
                    })?;
                }
            }
            writer
                .align(alignment.max(1))
                .map_err(|e| error(stream_offset, e.to_string()))?;
            if buffer == 0 {
                let buffer_position = writer.position();
                put_u32(
                    &mut writer,
                    stream_offset + 72,
                    relative(buffer_position, buffer_base)?,
                )
            }
            put_u32(&mut writer, sizes_offset + buffer * 16, bytes.len() as u32);
            writer.write_bytes(&bytes);
        }
        put_u32(&mut writer, stream_offset + 80, mesh.positions.len() as u32);

        writer
            .align(4)
            .map_err(|e| error(mesh_entry, e.to_string()))?;
        let face_position = writer.position();
        let use_u32 = mesh.positions.len() > u16::MAX as usize;
        for &index in &mesh.indices {
            if use_u32 {
                writer.write_u32(index);
            } else {
                writer.write_u16(index as u16);
            }
        }
        let face_size = writer.position() - face_position;
        let size_offset = read_u64(&reader, mesh_entry + 24)? as usize;
        put_u32(&mut writer, size_offset, face_size as u32);
        put_u32(
            &mut writer,
            mesh_entry + 32,
            relative(face_position, buffer_base)?,
        );
        put_u32(&mut writer, mesh_entry + 36, 3);
        put_u32(&mut writer, mesh_entry + 40, if use_u32 { 2 } else { 1 });
        put_u32(&mut writer, mesh_entry + 44, mesh.indices.len() as u32);
        put_u32(&mut writer, mesh_entry + 48, 0);
        let submesh_offset = read_u64(&reader, mesh_entry)? as usize;
        put_u32(&mut writer, submesh_offset, 0);
        put_u32(&mut writer, submesh_offset + 4, mesh.indices.len() as u32);
        put_u16(
            &mut writer,
            shape_offset + 82,
            material_index_for_mesh(&mesh.name, &material_names)?,
        );
        put_u16(&mut writer, shape_offset + 84, root);
    }
    let output = writer.into_inner();
    let reparsed = BfresFile::from_bytes(&output)
        .map_err(|e| error(e.offset, format!("while reopening rebuilt BFRES: {e}")))?;
    let expected_vertices: usize = imported
        .meshes
        .iter()
        .map(|mesh| mesh.positions.len())
        .sum();
    let expected_indices: usize = imported.meshes.iter().map(|mesh| mesh.indices.len()).sum();
    let actual_vertices = reparsed
        .render
        .meshes
        .iter()
        .map(|mesh| mesh.positions.len())
        .sum::<usize>();
    let actual_indices = reparsed
        .render
        .meshes
        .iter()
        .map(|mesh| mesh.indices.len())
        .sum::<usize>();
    if reparsed.render.meshes.len() != shapes.len()
        || actual_vertices != expected_vertices
        || actual_indices != expected_indices
    {
        return Err(error(
            0,
            format!(
                "rebuilt BFRES geometry failed validation (meshes {} != {}, vertices {actual_vertices} != {expected_vertices}, indices {actual_indices} != {expected_indices})",
                reparsed.render.meshes.len(),
                shapes.len()
            ),
        ));
    }
    // Appended vertex/index buffers are addressed by offsets relative to the
    // external buffer and do not add relocation entries. Preserve the source
    // resource graph byte-for-byte when no new absolute skin pointer was made.
    if !added_skin_palette {
        return Ok(output);
    }
    let mut canonical = super::serializer::emit_v10_canonical_copy(&output)
        .map_err(|e| error(e.offset, format!("while canonicalizing rebuilt BFRES: {e}")))?;
    super::serializer::rebase_v10_relocations(&output, &mut canonical)
        .map_err(|e| error(e.offset, format!("while rebasing rebuilt BFRES: {e}")))?;
    BfresFile::from_bytes(&canonical.bytes)
        .map_err(|e| error(e.offset, format!("while validating canonical BFRES: {e}")))?;
    Ok(canonical.bytes)
}

fn weld_mesh_vertices(mesh: &mut ImportedMesh) -> Result<(), BfresError> {
    let original = mesh.clone();
    let mut remap = HashMap::<Vec<u32>, Vec<u32>>::new();
    let mut rebuilt = original.clone();
    rebuilt.positions.clear();
    rebuilt.normals.clear();
    rebuilt.tangents.clear();
    rebuilt.bitangents.clear();
    rebuilt.uv_maps = vec![Vec::new(); original.uv_maps.len()];
    rebuilt.colors = vec![Vec::new(); original.colors.len()];
    rebuilt.bone_indices.clear();
    rebuilt.bone_weights.clear();
    rebuilt.source_vertices.clear();
    rebuilt.indices.clear();

    let mut old_to_new = vec![u32::MAX; original.positions.len()];
    for index in 0..original.positions.len() {
        let position = *original
            .positions
            .get(index)
            .ok_or_else(|| error(0, format!("mesh {:?} has an invalid index", original.name)))?;
        let mut key = Vec::new();
        key.extend(position.map(f32::to_bits));
        // Toolbox's Assimp import joins the FBX vertices before BFRES tangent
        // generation. Our importer creates MikkTSpace tangents eagerly, so using
        // those generated values as part of the identity would retain artificial
        // per-corner splits which Toolbox has already removed.
        for channel in &original.uv_maps {
            if let Some(value) = channel.get(index) {
                key.extend(value.map(f32::to_bits));
            }
        }
        for channel in &original.colors {
            if let Some(value) = channel.get(index) {
                key.extend(value.map(f32::to_bits));
            }
        }
        if let Some(value) = original.bone_indices.get(index) {
            key.extend(value.map(u32::from));
        }
        if let Some(value) = original.bone_weights.get(index) {
            key.extend(value.map(f32::to_bits));
        }

        let matching = remap.get(&key).and_then(|candidates| {
            candidates.iter().copied().find(|candidate| {
                match (
                    original.normals.get(index),
                    rebuilt.normals.get(*candidate as usize),
                ) {
                    (Some(left), Some(right)) => {
                        left.iter()
                            .zip(right)
                            .map(|(left, right)| (left - right) * (left - right))
                            .sum::<f32>()
                            <= 1.0e-10
                    }
                    (None, None) => true,
                    _ => false,
                }
            })
        });
        let new_index = if let Some(existing) = matching {
            existing
        } else {
            let next = u32::try_from(rebuilt.positions.len())
                .map_err(|_| error(0, "welded BFRES mesh has too many vertices"))?;
            remap.entry(key).or_default().push(next);
            rebuilt.positions.push(position);
            if let Some(value) = original.normals.get(index) {
                rebuilt.normals.push(*value);
            }
            if let Some(value) = original.tangents.get(index) {
                rebuilt.tangents.push(*value);
            }
            if let Some(value) = original.bitangents.get(index) {
                rebuilt.bitangents.push(*value);
            }
            for (target, source) in rebuilt.uv_maps.iter_mut().zip(&original.uv_maps) {
                if let Some(value) = source.get(index) {
                    target.push(*value);
                }
            }
            for (target, source) in rebuilt.colors.iter_mut().zip(&original.colors) {
                if let Some(value) = source.get(index) {
                    target.push(*value);
                }
            }
            if let Some(value) = original.bone_indices.get(index) {
                rebuilt.bone_indices.push(*value);
            }
            if let Some(value) = original.bone_weights.get(index) {
                rebuilt.bone_weights.push(*value);
            }
            rebuilt
                .source_vertices
                .push(*original.source_vertices.get(index).unwrap_or(&index));
            next
        };
        old_to_new[index] = new_index;
    }
    rebuilt.indices = original
        .indices
        .iter()
        .map(|index| old_to_new[*index as usize])
        .collect();
    *mesh = rebuilt;
    Ok(())
}

fn validate_compatible_skeleton(
    bfres: &BfresFile,
    imported: &[(String, Option<String>)],
    meshes: &[ImportedMesh],
) -> Result<(), BfresError> {
    let expected: Vec<_> = bfres
        .render
        .bones
        .iter()
        .enumerate()
        .map(|(index, bone)| {
            let parent = if bone.parent_index < 0 {
                None
            } else {
                let parent_index = bone.parent_index as usize;
                Some(
                    bfres
                        .render
                        .bones
                        .get(parent_index)
                        .ok_or_else(|| {
                            error(
                                0,
                                format!(
                                    "BFRES bone {index} ({}) has invalid parent index {}",
                                    bone.name, bone.parent_index
                                ),
                            )
                        })?
                        .name
                        .clone(),
                )
            };
            Ok((bone.name.clone(), parent))
        })
        .collect::<Result<_, BfresError>>()?;

    if imported.len() < expected.len() {
        return Err(error(
            0,
            format!(
                "FBX skeleton is missing BFRES bones: expected at least {}, found {}",
                expected.len(),
                imported.len()
            ),
        ));
    }
    let expected_by_name: BTreeMap<_, _> = expected.iter().cloned().collect();
    let imported_by_name: BTreeMap<_, _> = imported.iter().cloned().collect();
    if expected_by_name.len() != expected.len() {
        return Err(error(0, "BFRES skeleton contains duplicate bone names"));
    }
    if imported_by_name.len() != imported.len() {
        return Err(error(0, "FBX skeleton contains duplicate bone names"));
    }
    for (name, required_parent) in &expected_by_name {
        let Some(actual_parent) = imported_by_name.get(name) else {
            return Err(error(
                0,
                format!("FBX skeleton is missing BFRES bone {name:?}"),
            ));
        };
        if actual_parent != required_parent {
            return Err(error(
                0,
                format!(
                    "FBX skeleton parent mismatch for bone {name:?}: expected {required_parent:?}, found {actual_parent:?}"
                ),
            ));
        }
    }
    let extra_indices: BTreeSet<u16> = imported
        .iter()
        .enumerate()
        // FBX exporters commonly add a synthetic `Root` above the actual
        // skeleton. The remapping phase intentionally maps unknown bones to
        // the BFRES root, so this conventional wrapper is safe to accept.
        .filter(|(_, (name, _))| name != "Root" && !expected_by_name.contains_key(name))
        .filter_map(|(index, _)| u16::try_from(index).ok())
        .collect();
    for mesh in meshes {
        for (joints, weights) in mesh.bone_indices.iter().zip(&mesh.bone_weights) {
            for (&joint, &weight) in joints.iter().zip(weights) {
                if weight > 0.0 && extra_indices.contains(&joint) {
                    let name = imported
                        .get(joint as usize)
                        .map(|(name, _)| name.as_str())
                        .unwrap_or("<invalid>");
                    return Err(error(
                        0,
                        format!(
                            "FBX mesh {:?} uses extra bone {name:?}, which is absent from the BFRES skeleton",
                            mesh.name
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Aligns FBX meshes with BFRES shape slots by semantic role before falling
/// back to their stable source order. Weapon files commonly store the hidden
/// blade shape before the main shape while FBX exporters emit the reverse.
fn order_meshes_for_shapes(meshes: &mut Vec<ImportedMesh>, shapes: &[BfresSection]) {
    let mut remaining = std::mem::take(meshes);
    let mut ordered = Vec::with_capacity(remaining.len());
    for shape in shapes.iter().take(remaining.len()) {
        let shape_is_blade = shape.name.as_deref().is_some_and(contains_blade_hide);
        let matching = remaining
            .iter()
            .position(|mesh| contains_blade_hide(&mesh.name) == shape_is_blade);
        let index = matching.unwrap_or(0);
        ordered.push(remaining.remove(index));
    }
    ordered.append(&mut remaining);
    *meshes = ordered;
}

fn contains_blade_hide(name: &str) -> bool {
    name.to_ascii_lowercase().contains("blade_hide")
}

fn orient_meshes_to_source_bind_space(
    meshes: &mut [ImportedMesh],
    sources: &[super::BfresMesh],
) -> Result<(), BfresError> {
    let mut remaining: Vec<usize> = (0..sources.len()).collect();
    for mesh in meshes {
        // Some BFRES templates expose fewer decoded render meshes than their
        // FSHP slot table (notably the hidden-blade slot). Such an extra FBX
        // mesh still has a valid destination; leave it in FBX bind space.
        if remaining.is_empty() {
            continue;
        }
        let wants_blade = contains_blade_hide(&mesh.name);
        let position = remaining
            .iter()
            .position(|index| contains_blade_hide(&sources[*index].name) == wants_blade)
            .unwrap_or(0);
        let source_index = remaining[position];
        remaining.remove(position);
        let source = &sources[source_index];
        let (permutation, mut signs) =
            best_signed_axis_orientation(&mesh.positions, &source.positions)?;
        if wants_blade {
            signs[1] = -signs[1];
        }
        let convert =
            |value: [f32; 3]| [0, 1, 2].map(|axis| value[permutation[axis]] * signs[axis]);
        for position in &mut mesh.positions {
            *position = convert(*position);
        }
        for normal in &mut mesh.normals {
            *normal = convert(*normal);
        }
        calculate_tangents(mesh)
            .map_err(|failure| error(0, format!("failed to rebuild tangent space: {failure}")))?;
    }
    Ok(())
}

fn best_signed_axis_orientation(
    input: &[[f32; 3]],
    target: &[[f32; 3]],
) -> Result<([usize; 3], [f32; 3]), BfresError> {
    const PERMUTATIONS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let target_bounds = bounds(target)?;
    let mut best = ([0, 1, 2], [1.0; 3]);
    let mut best_score = f64::INFINITY;
    for permutation in PERMUTATIONS {
        for mask in 0..8u8 {
            let signs = [0, 1, 2].map(|axis| if mask & (1 << axis) == 0 { 1.0 } else { -1.0 });
            let transformed: Vec<_> = input
                .iter()
                .map(|value| [0, 1, 2].map(|axis| value[permutation[axis]] * signs[axis]))
                .collect();
            let candidate = bounds(&transformed)?;
            let score = (0..3)
                .flat_map(|axis| {
                    [
                        f64::from(candidate.0[axis] - target_bounds.0[axis]).powi(2),
                        f64::from(candidate.1[axis] - target_bounds.1[axis]).powi(2),
                    ]
                })
                .sum::<f64>();
            if score < best_score {
                best_score = score;
                best = (permutation, signs);
            }
        }
    }
    Ok(best)
}

fn bone_world_position(
    mut value: [f32; 3],
    bone_index: u16,
    bones: &[super::BfresBone],
) -> Result<[f32; 3], BfresError> {
    let mut current = Some(bone_index as usize);
    let mut lineage = Vec::new();
    while let Some(index) = current {
        let bone = bones
            .get(index)
            .ok_or_else(|| error(index, "bone index is out of range"))?;
        lineage.push(index);
        current = (bone.parent_index >= 0).then_some(bone.parent_index as usize);
    }
    for index in lineage {
        let bone = &bones[index];
        for axis in 0..3 {
            value[axis] *= bone.scale[axis];
        }
        let quaternion = if bone.rotation_mode == "euler_xyz" {
            euler_xyz_quaternion(bone.rotation)
        } else {
            normalize_quaternion(bone.rotation)
        };
        value = rotate_by_quaternion(value, quaternion);
        for axis in 0..3 {
            value[axis] += bone.translation[axis];
        }
    }
    Ok(value)
}

fn euler_xyz_quaternion(rotation: [f32; 4]) -> [f32; 4] {
    let axis = |axis: usize, angle: f32| {
        let half = angle * 0.5;
        let mut result = [0.0, 0.0, 0.0, half.cos()];
        result[axis] = half.sin();
        result
    };
    quaternion_multiply(
        quaternion_multiply(axis(2, rotation[2]), axis(1, rotation[1])),
        axis(0, rotation[0]),
    )
}

fn quaternion_multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn normalize_quaternion(value: [f32; 4]) -> [f32; 4] {
    let length = value.iter().map(|v| v * v).sum::<f32>().sqrt();
    if length <= f32::EPSILON {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        value.map(|v| v / length)
    }
}

fn rotate_by_quaternion(value: [f32; 3], quaternion: [f32; 4]) -> [f32; 3] {
    let q = normalize_quaternion(quaternion);
    let vector = [value[0], value[1], value[2], 0.0];
    let inverse = [-q[0], -q[1], -q[2], q[3]];
    let result = quaternion_multiply(quaternion_multiply(q, vector), inverse);
    [result[0], result[1], result[2]]
}

fn bounds(points: &[[f32; 3]]) -> Result<([f32; 3], [f32; 3]), BfresError> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for point in points {
        if !point.iter().all(|value| value.is_finite()) {
            return Err(error(0, "mesh contains non-finite positions"));
        }
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    Ok((min, max))
}

fn material_index_for_mesh(mesh_name: &str, material_names: &[String]) -> Result<u16, BfresError> {
    if material_names.is_empty() {
        return Err(error(0, "BFRES model contains no materials"));
    }
    if mesh_name.to_ascii_lowercase().contains("blade_hide") {
        return Ok(material_names
            .iter()
            .position(|name| name.to_ascii_lowercase().contains("blade_hide"))
            .and_then(|index| u16::try_from(index).ok())
            .unwrap_or(0));
    }
    Ok(u16::from(material_names.len() > 1))
}

fn constrain_skin_palette(mesh: &mut ImportedMesh, capacity: usize, root: u16) {
    if capacity == 0 {
        for (joints, weights) in mesh.bone_indices.iter_mut().zip(&mut mesh.bone_weights) {
            *joints = [root; 8];
            *weights = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        }
        return;
    }
    let mut totals = HashMap::<u16, f32>::new();
    for (joints, weights) in mesh.bone_indices.iter().zip(&mesh.bone_weights) {
        for slot in 0..4 {
            *totals.entry(joints[slot]).or_default() += weights[slot].max(0.0);
        }
    }
    let mut ranked: Vec<_> = totals.into_iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let keep_non_root = capacity.saturating_sub(1);
    let kept: BTreeSet<_> = ranked
        .into_iter()
        .filter(|(bone, _)| *bone != root)
        .take(keep_non_root)
        .map(|(bone, _)| bone)
        .collect();
    for (joints, weights) in mesh.bone_indices.iter_mut().zip(&mut mesh.bone_weights) {
        let mut combined = HashMap::<u16, f32>::new();
        for slot in 0..4 {
            let bone = if kept.contains(&joints[slot]) {
                joints[slot]
            } else {
                root
            };
            *combined.entry(bone).or_default() += weights[slot].max(0.0);
        }
        let mut values: Vec<_> = combined.into_iter().filter(|(_, w)| *w > 0.0).collect();
        values.sort_by(|a, b| b.1.total_cmp(&a.1));
        let sum: f32 = values.iter().map(|v| v.1).sum();
        *joints = [root; 8];
        *weights = [0.0; 8];
        for (slot, (bone, weight)) in values.into_iter().take(4).enumerate() {
            joints[slot] = bone;
            weights[slot] = weight / sum;
        }
    }
}

fn remap_weights(mesh: &mut ImportedMesh, remap: &[u16], root: u16) -> Result<(), BfresError> {
    for vertex in 0..mesh.positions.len() {
        let mut combined = HashMap::<u16, f32>::new();
        for slot in 0..8 {
            let weight = mesh
                .bone_weights
                .get(vertex)
                .map(|v| v[slot])
                .unwrap_or(0.0);
            if weight > 0.0 && weight.is_finite() {
                let imported = mesh
                    .bone_indices
                    .get(vertex)
                    .map(|v| v[slot] as usize)
                    .unwrap_or(usize::MAX);
                *combined
                    .entry(remap.get(imported).copied().unwrap_or(root))
                    .or_default() += weight;
            }
        }
        if combined.is_empty() {
            combined.insert(root, 1.0);
        }
        let mut values: Vec<_> = combined.into_iter().collect();
        values.sort_by(|a, b| b.1.total_cmp(&a.1));
        values.truncate(4);
        let sum: f32 = values.iter().map(|v| v.1).sum();
        let mut joints = [root; 8];
        let mut weights = [0.0; 8];
        for (slot, (joint, weight)) in values.into_iter().enumerate() {
            joints[slot] = joint;
            weights[slot] = weight / sum;
        }
        if vertex < mesh.bone_indices.len() {
            mesh.bone_indices[vertex] = joints;
        } else {
            mesh.bone_indices.push(joints);
        }
        if vertex < mesh.bone_weights.len() {
            mesh.bone_weights[vertex] = weights;
        } else {
            mesh.bone_weights.push(weights);
        }
    }
    Ok(())
}

fn validate_mesh(mesh: &ImportedMesh) -> Result<(), BfresError> {
    if mesh.positions.is_empty() || mesh.indices.is_empty() || mesh.indices.len() % 3 != 0 {
        return Err(error(
            0,
            format!("FBX mesh {} has invalid triangles", mesh.name),
        ));
    }
    if mesh.normals.len() != mesh.positions.len() {
        return Err(error(
            0,
            format!("FBX mesh {} requires one normal per vertex", mesh.name),
        ));
    }
    if mesh
        .uv_maps
        .first()
        .is_none_or(|uv| uv.len() != mesh.positions.len())
    {
        return Err(error(
            0,
            format!("FBX mesh {} requires a complete UV map", mesh.name),
        ));
    }
    if mesh
        .indices
        .iter()
        .any(|&i| i as usize >= mesh.positions.len())
    {
        return Err(error(
            0,
            format!("FBX mesh {} index exceeds vertex count", mesh.name),
        ));
    }
    Ok(())
}

fn attribute_value(
    a: &Attribute,
    mesh: &ImportedMesh,
    v: usize,
    matrix: &HashMap<u16, u16>,
    root: u16,
) -> Result<[f32; 4], BfresError> {
    Ok(if a.name.starts_with("_p") {
        let x = mesh.positions[v];
        [x[0], x[1], x[2], 1.0]
    } else if a.name.starts_with("_n") {
        let x = mesh.normals[v];
        [x[0], x[1], x[2], 0.0]
    } else if a.name.starts_with("_t") {
        mesh.tangents[v]
    } else if a.name.starts_with("_b") {
        mesh.bitangents[v]
    } else if a.name.starts_with("_u") {
        let layer = a
            .name
            .trim_start_matches("_u")
            .chars()
            .next()
            .and_then(|c| c.to_digit(10))
            .unwrap_or(0) as usize;
        let x = mesh
            .uv_maps
            .get(layer)
            .or_else(|| mesh.uv_maps.first())
            .and_then(|values| values.get(v))
            .ok_or_else(|| {
                error(
                    v,
                    format!("mesh {} has no UV value for vertex {v}", mesh.name),
                )
            })?;
        [x[0], x[1], 0.0, 1.0]
    } else if a.name.starts_with("_i") {
        let x = mesh.bone_indices[v];
        [0, 1, 2, 3].map(|i| {
            *matrix
                .get(&x[i])
                .or_else(|| matrix.get(&root))
                .unwrap_or(&0) as f32
        })
    } else if a.name.starts_with("_w") {
        let x = mesh.bone_weights[v];
        [x[0], x[1], x[2], x[3]]
    } else if a.name.starts_with("_c") {
        let layer = a
            .name
            .trim_start_matches("_c")
            .chars()
            .next()
            .and_then(|c| c.to_digit(10))
            .unwrap_or(0) as usize;
        mesh.colors
            .get(layer)
            .or_else(|| mesh.colors.first())
            .and_then(|values| values.get(v))
            .copied()
            .unwrap_or([1.0; 4])
    } else {
        [0.0; 4]
    })
}

fn attribute_component_count(name: &str) -> usize {
    if name.starts_with("_p") || name.starts_with("_n") {
        3
    } else if name.starts_with("_u") {
        2
    } else {
        4
    }
}

pub(super) fn encode(
    out: &mut [u8],
    o: usize,
    f: u16,
    v: [f32; 4],
    components: usize,
) -> Result<usize, BfresError> {
    let floats = |values: &[f32]| {
        let mut writer = BinaryWriter::new();
        for value in values {
            writer.write_f32(*value);
        }
        writer.into_inner()
    };
    let unsigned = |values: Vec<u16>| {
        let mut writer = BinaryWriter::new();
        for value in values {
            writer.write_u16(value);
        }
        writer.into_inner()
    };
    let signed = |values: Vec<i16>| {
        let mut writer = BinaryWriter::new();
        for value in values {
            writer.write_i16(value);
        }
        writer.into_inner()
    };
    let bytes: Vec<u8> = match f {
        0x0518 => floats(&v[..3]),
        0x0519 => floats(&v),
        0x0517 => floats(&v[..2]),
        0x0516 => floats(&v[..1]),
        0x0512 => unsigned(v[..2].iter().map(|x| f32_to_half(*x)).collect()),
        0x0515 => unsigned(
            v[..components.min(4)]
                .iter()
                .map(|x| f32_to_half(*x))
                .collect(),
        ),
        0x010B => v
            .map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8)
            .to_vec(),
        0x020B => v.map(|x| (x.clamp(-1.0, 1.0) * 127.0) as i8 as u8).to_vec(),
        0x030B => v.map(|x| x.round().clamp(0.0, 255.0) as u8).to_vec(),
        0x0302 => vec![v[0].round().clamp(0.0, 255.0) as u8],
        0x0115 => unsigned(
            v.iter()
                .map(|x| (x.clamp(0.0, 1.0) * 65535.0).round() as u16)
                .collect(),
        ),
        0x0215 => signed(
            v.iter()
                .map(|x| (x.clamp(-1.0, 1.0) * 32767.0).round() as i16)
                .collect(),
        ),
        0x0315 => unsigned(
            v.iter()
                .map(|x| x.round().clamp(0.0, 65535.0) as u16)
                .collect(),
        ),
        0x0109 => v[..2]
            .iter()
            .map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect(),
        0x0309 => v[..2]
            .iter()
            .map(|x| x.round().clamp(0.0, 255.0) as u8)
            .collect(),
        0x0112 => unsigned(
            v[..2]
                .iter()
                .map(|x| (x.clamp(0.0, 1.0) * 65535.0).round() as u16)
                .collect(),
        ),
        0x0212 => signed(
            v[..2]
                .iter()
                .map(|x| (x.clamp(-1.0, 1.0) * 32767.0).round() as i16)
                .collect(),
        ),
        0x020E => {
            let p = |x: f32| ((x.clamp(-1.0, 1.0) * 511.0) as i32 as u32) & 0x3ff;
            let w = ((v[3].clamp(0.0, 1.0) as i32 as u32) & 0x3) << 30;
            let mut writer = BinaryWriter::new();
            writer.write_u32(p(v[0]) | (p(v[1]) << 10) | (p(v[2]) << 20) | w);
            writer.into_inner()
        }
        _ => {
            return Err(error(
                o,
                format!("cannot encode BFRES vertex format 0x{f:04X}"),
            ))
        }
    };
    let dst = out
        .get_mut(o..o + bytes.len())
        .ok_or_else(|| error(o, "vertex attribute exceeds stride"))?;
    dst.copy_from_slice(&bytes);
    Ok(bytes.len())
}

fn f32_to_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x7fffff;
    if exponent <= 0 {
        if exponent < -10 {
            return sign;
        }
        let mantissa = (mantissa | 0x800000) >> (1 - exponent);
        return sign | ((mantissa + 0x1000) >> 13) as u16;
    }
    if exponent >= 31 {
        return sign | 0x7c00 | u16::from(mantissa != 0);
    }
    sign | ((exponent as u16) << 10) | ((mantissa + 0x1000) >> 13) as u16
}

fn read_u8(r: &BinaryReader<'_>, o: usize) -> Result<u8, BfresError> {
    r.read_u8_at(o).map_err(io_error(o))
}
fn read_u16(r: &BinaryReader<'_>, o: usize) -> Result<u16, BfresError> {
    r.read_u16_at(o).map_err(io_error(o))
}
fn read_u32(r: &BinaryReader<'_>, o: usize) -> Result<u32, BfresError> {
    r.read_u32_at(o).map_err(io_error(o))
}
fn read_u64(r: &BinaryReader<'_>, o: usize) -> Result<u64, BfresError> {
    r.read_u64_at(o).map_err(io_error(o))
}
fn put_u16(w: &mut BinaryWriter, o: usize, v: u16) {
    let p = w.position();
    w.seek(o);
    w.write_u16(v);
    w.seek(p)
}
fn put_u8(w: &mut BinaryWriter, o: usize, v: u8) {
    let p = w.position();
    w.seek(o);
    w.write_u8(v);
    w.seek(p)
}
fn put_u32(w: &mut BinaryWriter, o: usize, v: u32) {
    let p = w.position();
    w.seek(o);
    w.write_u32(v);
    w.seek(p)
}

fn put_f32(w: &mut BinaryWriter, o: usize, v: f32) {
    put_u32(w, o, v.to_bits());
}
fn put_u64(w: &mut BinaryWriter, o: usize, v: u64) {
    let p = w.position();
    w.seek(o);
    w.write_u64(v);
    w.seek(p)
}
fn relative(p: usize, b: usize) -> Result<u32, BfresError> {
    u32::try_from(
        p.checked_sub(b)
            .ok_or_else(|| error(p, "buffer precedes BFRES buffer base"))?,
    )
    .map_err(|_| error(p, "buffer offset exceeds u32"))
}
fn error(offset: usize, message: impl Into<String>) -> BfresError {
    BfresError::new(offset, message)
}
fn io_error(offset: usize) -> impl FnOnce(std::io::Error) -> BfresError {
    move |e| error(offset, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    fn imported_mesh(name: &str) -> ImportedMesh {
        ImportedMesh {
            name: name.into(),
            material: String::new(),
            positions: Vec::new(),
            normals: Vec::new(),
            tangents: Vec::new(),
            bitangents: Vec::new(),
            uv_maps: Vec::new(),
            colors: Vec::new(),
            bone_indices: Vec::new(),
            bone_weights: Vec::new(),
            palette_bones: Vec::new(),
            source_vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    #[test]
    fn aligns_blade_hide_mesh_with_its_bfres_shape_slot() {
        let mut meshes = vec![imported_mesh("Main"), imported_mesh("BLADE_HIDE")];
        let shapes = vec![
            BfresSection {
                signature: *b"FSHP",
                offset: 1,
                name: Some("Weapon_Blade_Hide".into()),
            },
            BfresSection {
                signature: *b"FSHP",
                offset: 2,
                name: Some("Weapon_Main".into()),
            },
        ];
        order_meshes_for_shapes(&mut meshes, &shapes);
        assert_eq!(meshes[0].name, "BLADE_HIDE");
        assert_eq!(meshes[1].name, "Main");
    }

    #[test]
    fn applies_one_axis_conversion_to_all_supplied_weapon_meshes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let source_path = root.join("bfres/Weapon_Sword_022.Weapon_Sword_022.bfres");
        let fbx_path = root.join("Weapon_Sword_022.fbx");
        if !source_path.is_file() || !fbx_path.is_file() {
            return;
        }
        let mut imported = import_for_bfres(&fs::read(fbx_path).unwrap()).unwrap();
        let source = BfresFile::from_path(source_path).unwrap();
        orient_meshes_to_source_bind_space(&mut imported.meshes, &source.render.meshes).unwrap();
        assert!(imported.meshes.iter().all(|mesh| mesh
            .positions
            .iter()
            .flatten()
            .all(|value| value.is_finite())));

        let raw = fs::read(root.join("bfres/Weapon_Sword_022.Weapon_Sword_022.bfres")).unwrap();
        let renamed = BfresFile::rename_first_model_and_container(
            &raw,
            "Weapon_Lsword_005",
            "Weapon_Lsword_005.Weapon_Lsword_005",
        )
        .unwrap();
        let replaced = replace_geometry_from_fbx(
            &renamed,
            &fs::read(root.join("Weapon_Sword_022.fbx")).unwrap(),
        )
        .unwrap();
        let reopened = BfresFile::from_bytes(&replaced).unwrap();
        let blade = reopened
            .render
            .meshes
            .iter()
            .find(|mesh| contains_blade_hide(&mesh.name))
            .unwrap();
        assert!(blade
            .positions
            .iter()
            .flatten()
            .all(|value| value.is_finite()));
    }

    #[test]
    fn replaces_real_weapon_geometry_and_reopens() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let bfres_path = root.join("bfres/Weapon_Sword_022.Weapon_Sword_022.bfres");
        let fbx_path = root.join("Weapon_Sword_022.fbx");
        if !bfres_path.is_file() || !fbx_path.is_file() {
            return;
        }
        let source = fs::read(bfres_path).unwrap();
        let before = BfresFile::from_bytes(&source).unwrap();
        let imported = import_for_bfres(&fs::read(fbx_path).unwrap()).unwrap();
        let replaced = replace_geometry_from_fbx(
            &source,
            &fs::read(root.join("Weapon_Sword_022.fbx")).unwrap(),
        )
        .unwrap();
        let after = BfresFile::from_bytes(&replaced).unwrap();
        let blade = after
            .render
            .meshes
            .iter()
            .find(|mesh| contains_blade_hide(&mesh.name))
            .unwrap();
        let (blade_min, blade_max) = bounds(&blade.positions).unwrap();
        assert!(
            (blade_min[0] - -0.11383057).abs() < 0.001,
            "{blade_min:?} {blade_max:?}"
        );
        assert!(
            (blade_max[1] - 0.7421875).abs() < 0.01,
            "{blade_min:?} {blade_max:?}"
        );
        assert_eq!(before.materials, after.materials);
        assert_eq!(before.render.bones, after.render.bones);
        assert_eq!(
            after
                .render
                .meshes
                .iter()
                .map(|mesh| mesh.positions.len())
                .sum::<usize>(),
            imported
                .meshes
                .iter()
                .map(|mesh| mesh.positions.len())
                .sum::<usize>()
        );
        assert_eq!(
            after
                .render
                .meshes
                .iter()
                .map(|mesh| mesh.indices.len())
                .sum::<usize>(),
            imported
                .meshes
                .iter()
                .map(|mesh| mesh.indices.len())
                .sum::<usize>()
        );
        let material_names: Vec<_> = after
            .materials
            .iter()
            .map(|material| material.name.clone())
            .collect();
        let mut expected_materials: Vec<_> = imported
            .meshes
            .iter()
            .map(|mesh| material_index_for_mesh(&mesh.name, &material_names).unwrap())
            .collect();
        let mut actual_materials: Vec<_> = after
            .render
            .meshes
            .iter()
            .map(|mesh| mesh.material_index)
            .collect();
        expected_materials.sort_unstable();
        actual_materials.sort_unstable();
        assert_eq!(actual_materials, expected_materials);

        let mut obj = String::new();
        let mut vertex_base = 1u32;
        for mesh in &after.render.meshes {
            obj.push_str(&format!("o {}\n", mesh.name));
            for position in &mesh.positions {
                obj.push_str(&format!(
                    "v {} {} {}\n",
                    position[0], position[1], position[2]
                ));
            }
            for triangle in mesh.indices.chunks_exact(3) {
                obj.push_str(&format!(
                    "f {} {} {}\n",
                    triangle[0] + vertex_base,
                    triangle[1] + vertex_base,
                    triangle[2] + vertex_base
                ));
            }
            vertex_base += mesh.positions.len() as u32;
        }
        let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
        fs::write(target.join("bfres_replace_render.obj"), obj).unwrap();
        fs::write(target.join("bfres_replace_render.bfres"), replaced).unwrap();
    }

    #[test]
    #[ignore = "writes a visual-validation OBJ for the generated BFRES"]
    fn exports_generated_weapon_visual_obj() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/test_sic");
        let inputs = [
            (
                root.join("romfs/Model/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc"),
                root.join("Weapon_Lsword_005.visual.obj"),
            ),
            (
                root.join("../bfres/Weapon_Sword_022.Weapon_Sword_022.bfres"),
                root.join("Weapon_Sword_022.source_bfres.visual.obj"),
            ),
        ];
        for (input, output) in inputs.into_iter().filter(|(input, _)| input.is_file()) {
            let bfres = BfresFile::from_path(input).unwrap();
            let mut obj = String::new();
            let mut vertex_base = 1u32;
            for mesh in &bfres.render.meshes {
                obj.push_str(&format!("o {}\n", mesh.name.replace(' ', "_")));
                for (index, position) in mesh.positions.iter().enumerate() {
                    let position = if mesh.vertex_skin_count == 1 {
                        let bone = mesh
                            .bone_indices
                            .get(index)
                            .map(|indices| indices[0])
                            .unwrap_or(mesh.bone_index);
                        bone_world_position(*position, bone, &bfres.render.bones).unwrap()
                    } else {
                        *position
                    };
                    obj.push_str(&format!(
                        "v {} {} {}\n",
                        position[0], position[1], position[2]
                    ));
                }
                for triangle in mesh.indices.chunks_exact(3) {
                    obj.push_str(&format!(
                        "f {} {} {}\n",
                        triangle[0] + vertex_base,
                        triangle[1] + vertex_base,
                        triangle[2] + vertex_base
                    ));
                }
                vertex_base += mesh.positions.len() as u32;
            }
            fs::write(output, obj).unwrap();
        }
    }

    #[test]
    fn assigns_blade_hide_and_default_materials_case_insensitively() {
        let materials = vec![
            "First".to_string(),
            "Weapon_BLADE_HIDE_Material".to_string(),
            "Third".to_string(),
        ];
        assert_eq!(
            material_index_for_mesh("Blade_Hide.001", &materials).unwrap(),
            1
        );
        assert_eq!(material_index_for_mesh("Blade", &materials).unwrap(), 1);
        assert_eq!(
            material_index_for_mesh("Handle", &["Only".to_string()]).unwrap(),
            0
        );
        assert_eq!(
            material_index_for_mesh("blade_hide", &["Only".to_string()]).unwrap(),
            0
        );
    }
}
