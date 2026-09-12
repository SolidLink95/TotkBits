//! Native implementation of the deterministic parts of Switch Toolbox's
//! `ResFileSwitchSaver`.
//!
//! The complete BFRES graph writer is built on these primitives. Keeping the
//! relocation and string-table rules isolated makes their byte layout directly
//! testable against files emitted by Toolbox.

use super::{align_up, BfresError};
use crate::parser::binary::{BinaryReader, BinaryWriter, Endian as BinaryEndian};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub(super) struct V10ModelGraph {
    pub header: usize,
    pub vertex_buffers: Vec<V10VertexBufferGraph>,
    pub materials: Vec<V10MaterialGraph>,
    pub shapes: Vec<V10ShapeGraph>,
    pub skeleton: V10SkeletonGraph,
}

#[derive(Clone, Debug)]
pub(super) struct V10MaterialGraph {
    pub header: usize,
    pub shader_info: V10ShaderInfoGraph,
    pub texture_names: Vec<usize>,
    pub samplers: Vec<usize>,
    pub render_info_data: Option<(usize, usize)>,
    pub render_info_counts: Option<(usize, usize)>,
    pub render_info_offsets: Option<(usize, usize)>,
    pub shader_parameter_data: Option<(usize, usize)>,
    pub shader_parameter_indices: Option<(usize, usize)>,
    pub sampler_slots: Option<(usize, usize)>,
    pub texture_slots: Option<(usize, usize)>,
}

#[derive(Clone, Debug)]
pub(super) struct V10ShaderInfoGraph {
    pub header: usize,
    pub assignment: V10ShaderAssignmentGraph,
    pub attribute_values: Vec<usize>,
    pub attribute_indices: Option<usize>,
    pub sampler_values: Vec<usize>,
    pub sampler_indices: Option<usize>,
    pub option_bitflags: Option<(usize, usize)>,
    pub option_values: Vec<usize>,
    pub option_indices: Option<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct V10ShaderAssignmentGraph {
    pub header: usize,
    pub render_info_records: Vec<usize>,
    pub render_info_dict: usize,
    pub shader_parameter_records: Vec<usize>,
    pub shader_parameter_dict: usize,
    pub attribute_dict: usize,
    pub sampler_dict: usize,
    pub option_dict: usize,
}

#[derive(Clone, Debug)]
pub(super) struct V10VertexBufferGraph {
    pub header: usize,
    pub attributes: Vec<usize>,
    pub buffer_sizes: Vec<usize>,
    pub buffer_strides: Vec<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct V10ShapeGraph {
    pub header: usize,
    pub meshes: Vec<usize>,
    pub skin_bones: Option<(usize, usize)>,
    pub bounds: Vec<usize>,
    pub radii: Vec<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct V10SkeletonGraph {
    pub header: usize,
    pub bones: Vec<usize>,
    pub matrix_to_bone: Option<(usize, usize)>,
    pub inverse_matrices: Vec<usize>,
    pub mirrored_bones: Option<(usize, usize)>,
}

#[derive(Clone, Debug)]
pub(super) struct V10ResourceGraph {
    pub models: Vec<V10ModelGraph>,
    pub buffer_info: usize,
    pub external_files: Vec<usize>,
}

impl V10ResourceGraph {
    pub fn parse(data: &[u8]) -> Result<Self, BfresError> {
        if data.get(..4) != Some(b"FRES") {
            return Err(error(0, "missing FRES signature"));
        }
        if data.get(10).copied().unwrap_or_default() < 10 {
            return Err(error(8, "native serializer requires BFRES v10"));
        }
        let model_count = read_u16(data, 0xdc)? as usize;
        let external_count = read_u16(data, 0xec)? as usize;
        let model_array = read_u64(data, 0x28)? as usize;
        let buffer_info = read_u64(data, 0xb0)? as usize;
        let external_array = read_u64(data, 0xb8)? as usize;
        let mut models = Vec::with_capacity(model_count);
        for index in 0..model_count {
            let header = model_array
                .checked_add(index * 120)
                .ok_or_else(|| error(model_array, "model array overflow"))?;
            if data.get(header..header + 4) != Some(b"FMDL") {
                return Err(error(header, "missing FMDL signature"));
            }
            let vertex_count = read_u16(data, header + 104)? as usize;
            let shape_count = read_u16(data, header + 106)? as usize;
            let material_count = read_u16(data, header + 108)? as usize;
            let skeleton = read_u64(data, header + 24)? as usize;
            let vertex_start = read_u64(data, header + 32)? as usize;
            let shape_start = read_u64(data, header + 40)? as usize;
            let material_start = read_u64(data, header + 56)? as usize;
            let vertex_headers = fixed_record_offsets(vertex_start, vertex_count, 88, data.len())?;
            let mut vertex_buffers = Vec::with_capacity(vertex_count);
            for vertex in vertex_headers {
                let attribute_count = data[vertex + 76] as usize;
                let buffer_count = data[vertex + 77] as usize;
                vertex_buffers.push(V10VertexBufferGraph {
                    header: vertex,
                    attributes: fixed_record_offsets(
                        read_u64(data, vertex + 8)? as usize,
                        attribute_count,
                        16,
                        data.len(),
                    )?,
                    buffer_sizes: fixed_record_offsets(
                        read_u64(data, vertex + 48)? as usize,
                        buffer_count,
                        16,
                        data.len(),
                    )?,
                    buffer_strides: fixed_record_offsets(
                        read_u64(data, vertex + 56)? as usize,
                        buffer_count,
                        16,
                        data.len(),
                    )?,
                });
            }
            let shape_headers = fixed_record_offsets(shape_start, shape_count, 96, data.len())?;
            let mut shapes = Vec::with_capacity(shape_count);
            for shape in shape_headers {
                let mesh_count = data[shape + 91] as usize;
                let skin_count = read_u16(data, shape + 88)? as usize;
                let meshes = fixed_record_offsets(
                    read_u64(data, shape + 24)? as usize,
                    mesh_count,
                    56,
                    data.len(),
                )?;
                let bounds_count = meshes.iter().try_fold(0usize, |count, mesh| {
                    Ok::<_, BfresError>(count + read_u16(data, mesh + 52)? as usize + 1)
                })?;
                let radius_count = if skin_count == 0 {
                    mesh_count
                } else {
                    skin_count
                };
                shapes.push(V10ShapeGraph {
                    header: shape,
                    meshes,
                    skin_bones: optional_array(
                        read_u64(data, shape + 32)? as usize,
                        skin_count,
                        2,
                        data.len(),
                    )?,
                    bounds: fixed_record_offsets(
                        read_u64(data, shape + 56)? as usize,
                        bounds_count,
                        24,
                        data.len(),
                    )?,
                    radii: fixed_record_offsets(
                        read_u64(data, shape + 64)? as usize,
                        radius_count,
                        16,
                        data.len(),
                    )?,
                });
            }
            shapes.sort_by_key(|shape| {
                let name = read_u64(data, shape.header + 8)
                    .ok()
                    .and_then(|offset| read_res_string(data, offset as usize).ok())
                    .unwrap_or_default();
                name.to_ascii_lowercase().contains("blade_hide")
            });
            let mut ordered_vertex_buffers = Vec::with_capacity(vertex_buffers.len());
            for shape in &shapes {
                let source = read_u64(data, shape.header + 16)? as usize;
                if let Some(position) = vertex_buffers
                    .iter()
                    .position(|vertex| vertex.header == source)
                {
                    ordered_vertex_buffers.push(vertex_buffers.remove(position));
                }
            }
            ordered_vertex_buffers.append(&mut vertex_buffers);
            vertex_buffers = ordered_vertex_buffers;
            let skeleton_header = read_u64(data, header + 24)? as usize;
            let bone_count = read_u16(data, skeleton_header + 56)? as usize;
            let smooth_count = read_u16(data, skeleton_header + 58)? as usize;
            let rigid_count = read_u16(data, skeleton_header + 60)? as usize;
            let skeleton = V10SkeletonGraph {
                header: skeleton_header,
                bones: fixed_record_offsets(
                    read_u64(data, skeleton_header + 16)? as usize,
                    bone_count,
                    88,
                    data.len(),
                )?,
                matrix_to_bone: optional_array(
                    read_u64(data, skeleton_header + 24)? as usize,
                    smooth_count + rigid_count,
                    2,
                    data.len(),
                )?,
                inverse_matrices: fixed_record_offsets(
                    read_u64(data, skeleton_header + 32)? as usize,
                    smooth_count,
                    48,
                    data.len(),
                )?,
                mirrored_bones: optional_array(
                    read_u64(data, skeleton_header + 48)? as usize,
                    bone_count,
                    2,
                    data.len(),
                )?,
            };
            let material_headers =
                fixed_record_offsets(material_start, material_count, 176, data.len())?;
            let mut materials = Vec::with_capacity(material_count);
            for material in material_headers {
                let texture_count = data[material + 162] as usize;
                let sampler_count = data[material + 163] as usize;
                let render_info_size = read_u16(data, material + 168)? as usize;
                let shader_info = read_u64(data, material + 16)? as usize;
                if shader_info == 0 || shader_info + 80 > data.len() {
                    return Err(error(shader_info, "invalid ShaderInfo pointer"));
                }
                let shader_assignment = read_u64(data, shader_info)? as usize;
                if shader_assignment == 0 || shader_assignment + 88 > data.len() {
                    return Err(error(shader_assignment, "invalid ShaderAssignV10 pointer"));
                }
                let render_count = read_u16(data, shader_assignment + 72)? as usize;
                let parameter_count = read_u16(data, shader_assignment + 74)? as usize;
                let parameter_data_size = read_u16(data, shader_assignment + 76)? as usize;
                let attribute_value_count = data[shader_info + 68] as usize;
                let sampler_value_count = data[shader_info + 69] as usize;
                let boolean_option_count = read_u16(data, shader_info + 70)? as usize;
                let option_count = read_u16(data, shader_info + 72)? as usize;
                let shader_info_graph = V10ShaderInfoGraph {
                    header: shader_info,
                    assignment: V10ShaderAssignmentGraph {
                        header: shader_assignment,
                        render_info_records: fixed_record_offsets(
                            read_u64(data, shader_assignment + 16)? as usize,
                            render_count,
                            16,
                            data.len(),
                        )?,
                        render_info_dict: checked_dictionary(
                            data,
                            read_u64(data, shader_assignment + 24)? as usize,
                        )?,
                        shader_parameter_records: fixed_record_offsets(
                            read_u64(data, shader_assignment + 32)? as usize,
                            parameter_count,
                            24,
                            data.len(),
                        )?,
                        shader_parameter_dict: checked_dictionary(
                            data,
                            read_u64(data, shader_assignment + 40)? as usize,
                        )?,
                        attribute_dict: checked_dictionary(
                            data,
                            read_u64(data, shader_assignment + 48)? as usize,
                        )?,
                        sampler_dict: checked_dictionary(
                            data,
                            read_u64(data, shader_assignment + 56)? as usize,
                        )?,
                        option_dict: checked_dictionary(
                            data,
                            read_u64(data, shader_assignment + 64)? as usize,
                        )?,
                    },
                    attribute_values: fixed_record_offsets(
                        read_u64(data, shader_info + 8)? as usize,
                        attribute_value_count,
                        8,
                        data.len(),
                    )?,
                    attribute_indices: optional_pointer(
                        data,
                        read_u64(data, shader_info + 16)? as usize,
                    )?,
                    sampler_values: fixed_record_offsets(
                        read_u64(data, shader_info + 24)? as usize,
                        sampler_value_count,
                        8,
                        data.len(),
                    )?,
                    sampler_indices: optional_pointer(
                        data,
                        read_u64(data, shader_info + 32)? as usize,
                    )?,
                    option_bitflags: optional_byte_range(
                        read_u64(data, shader_info + 40)? as usize,
                        (1 + boolean_option_count / 64) * 8,
                        data.len(),
                    )?,
                    option_values: fixed_record_offsets(
                        read_u64(data, shader_info + 48)? as usize,
                        option_count.saturating_sub(boolean_option_count),
                        8,
                        data.len(),
                    )?,
                    option_indices: optional_pointer(
                        data,
                        read_u64(data, shader_info + 56)? as usize,
                    )?,
                };
                materials.push(V10MaterialGraph {
                    header: material,
                    shader_info: shader_info_graph,
                    texture_names: fixed_record_offsets(
                        read_u64(data, material + 32)? as usize,
                        texture_count,
                        8,
                        data.len(),
                    )?,
                    samplers: fixed_record_offsets(
                        read_u64(data, material + 48)? as usize,
                        sampler_count,
                        32,
                        data.len(),
                    )?,
                    render_info_data: optional_byte_range(
                        read_u64(data, material + 64)? as usize,
                        render_info_size,
                        data.len(),
                    )?,
                    render_info_counts: optional_byte_range(
                        read_u64(data, material + 72)? as usize,
                        render_count * 2,
                        data.len(),
                    )?,
                    render_info_offsets: optional_byte_range(
                        read_u64(data, material + 80)? as usize,
                        render_count * 2,
                        data.len(),
                    )?,
                    shader_parameter_data: optional_byte_range(
                        read_u64(data, material + 88)? as usize,
                        parameter_data_size,
                        data.len(),
                    )?,
                    shader_parameter_indices: optional_byte_range(
                        read_u64(data, material + 96)? as usize,
                        parameter_count * 4,
                        data.len(),
                    )?,
                    sampler_slots: optional_byte_range(
                        read_u64(data, material + 144)? as usize,
                        sampler_count * 8,
                        data.len(),
                    )?,
                    texture_slots: optional_byte_range(
                        read_u64(data, material + 152)? as usize,
                        texture_count * 8,
                        data.len(),
                    )?,
                });
            }
            models.push(V10ModelGraph {
                header,
                vertex_buffers,
                materials,
                shapes,
                skeleton,
            });
        }
        Ok(Self {
            models,
            buffer_info,
            external_files: fixed_record_offsets(external_array, external_count, 16, data.len())?,
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct V10FixedPhase {
    pub bytes: Vec<u8>,
    pub source_to_output: HashMap<usize, usize>,
    synthetic_strings: Vec<(usize, String)>,
    synthetic_source_pointers: Vec<(usize, usize)>,
    expanded_relocations: HashMap<usize, u16>,
}

/// Places all fixed-size v10 objects in the same phase order as
/// `ResFileSwitchSaver.Save`. Pointer rebasing is deliberately deferred until
/// the variable child blocks have also been assigned output addresses.
pub(super) fn emit_v10_fixed_phase(
    data: &[u8],
    graph: &V10ResourceGraph,
) -> Result<V10FixedPhase, BfresError> {
    let mut bytes = Vec::new();
    let mut source_to_output = HashMap::new();
    append_mapped_block(data, 0, 0xf0, &mut bytes, &mut source_to_output)?;
    for model in &graph.models {
        append_mapped_block(data, model.header, 120, &mut bytes, &mut source_to_output)?;
    }
    append_mapped_block(
        data,
        graph.buffer_info,
        32,
        &mut bytes,
        &mut source_to_output,
    )?;
    for external in &graph.external_files {
        append_mapped_block(data, *external, 16, &mut bytes, &mut source_to_output)?;
    }
    for pointer_field in [0x30usize, 0xc0] {
        let dictionary = read_u64(data, pointer_field)? as usize;
        if dictionary != 0 {
            let node_count = read_u32(data, dictionary + 4)? as usize + 1;
            append_mapped_block(
                data,
                dictionary,
                8 + node_count * 16,
                &mut bytes,
                &mut source_to_output,
            )?;
        }
    }
    for model in &graph.models {
        for (index, vertex) in model.vertex_buffers.iter().enumerate() {
            let target =
                append_mapped_block(data, vertex.header, 88, &mut bytes, &mut source_to_output)?;
            patch_u16_at(&mut bytes, target + 78, index as u16)?;
        }
        for material in &model.materials {
            append_mapped_block(
                data,
                material.header,
                176,
                &mut bytes,
                &mut source_to_output,
            )?;
        }
        for (index, shape) in model.shapes.iter().enumerate() {
            let target =
                append_mapped_block(data, shape.header, 96, &mut bytes, &mut source_to_output)?;
            let index = index as u16;
            patch_u16_at(&mut bytes, target + 80, index)?;
            patch_u16_at(&mut bytes, target + 86, index)?;
        }
        append_mapped_block(
            data,
            model.skeleton.header,
            64,
            &mut bytes,
            &mut source_to_output,
        )?;
    }
    // Fixed records may be emitted in a different order than their source
    // arrays (notably shapes and their corresponding vertex buffers). The
    // model array pointers must address the first record in emitted order;
    // relocation rebasing of the old array base cannot express that reorder.
    for model in &graph.models {
        let output_model = source_to_output[&model.header];
        if let Some(vertex) = model.vertex_buffers.first() {
            let target = source_to_output[&vertex.header] as u64;
            patch_u64(&mut bytes, output_model + 32, target)?;
        }
        if let Some(shape) = model.shapes.first() {
            let target = source_to_output[&shape.header] as u64;
            patch_u64(&mut bytes, output_model + 40, target)?;
        }
    }
    Ok(V10FixedPhase {
        bytes,
        source_to_output,
        synthetic_strings: Vec::new(),
        synthetic_source_pointers: Vec::new(),
        expanded_relocations: HashMap::new(),
    })
}

pub(super) fn emit_v10_model_core_phase(
    data: &[u8],
    graph: &V10ResourceGraph,
    phase: &mut V10FixedPhase,
) -> Result<(), BfresError> {
    for model in &graph.models {
        let rigid_bones = model
            .shapes
            .iter()
            .filter_map(|shape| shape.skin_bones)
            .flat_map(|(source, count)| {
                (0..count).filter_map(move |index| read_u16(data, source + index * 2).ok())
            })
            .collect::<Vec<_>>();
        for bone in &model.skeleton.bones {
            let target = append_mapped_block(
                data,
                *bone,
                88,
                &mut phase.bytes,
                &mut phase.source_to_output,
            )?;
            let bone_index = read_u16(data, *bone + 32)?;
            if let Some(rigid_index) = rigid_bones.iter().position(|index| *index == bone_index) {
                let mut writer = BinaryWriter::from_vec(phase.bytes.clone(), BinaryEndian::Little);
                writer.seek(target + 38);
                writer.write_i16(rigid_index as i16);
                phase.bytes = writer.into_inner();
            }
        }
        if let Some((source, count)) = model.skeleton.matrix_to_bone {
            align_output(&mut phase.bytes, 8);
            append_mapped_block(
                data,
                source,
                count * 2,
                &mut phase.bytes,
                &mut phase.source_to_output,
            )?;
        } else if model.shapes.iter().any(|shape| shape.skin_bones.is_some()) {
            align_output(&mut phase.bytes, 8);
            let target = phase.bytes.len();
            put_u16(&mut phase.bytes, 0);
            let output_skeleton = phase.source_to_output[&model.skeleton.header];
            patch_u64(&mut phase.bytes, output_skeleton + 24, target as u64)?;
            patch_u16_at(&mut phase.bytes, output_skeleton + 60, 1)?;
        }
        for matrix in &model.skeleton.inverse_matrices {
            if matrix == model.skeleton.inverse_matrices.first().unwrap() {
                align_output(&mut phase.bytes, 8);
            }
            append_mapped_block(
                data,
                *matrix,
                48,
                &mut phase.bytes,
                &mut phase.source_to_output,
            )?;
        }
        if let Some((source, count)) = model.skeleton.mirrored_bones {
            align_output(&mut phase.bytes, 8);
            append_mapped_block(
                data,
                source,
                count * 2,
                &mut phase.bytes,
                &mut phase.source_to_output,
            )?;
        }
        append_source_dictionary(
            data,
            read_u64(data, model.skeleton.header + 8)? as usize,
            &mut phase.bytes,
            &mut phase.source_to_output,
        )?;
        let shape_dictionary = read_u64(data, model.header + 48)? as usize;
        let dictionary_nodes = read_dictionary(data, shape_dictionary)?;
        let shape_names = model
            .shapes
            .iter()
            .map(|shape| read_res_string(data, read_u64(data, shape.header + 8)? as usize))
            .collect::<Result<Vec<_>, _>>()?;
        if dictionary_nodes.len() == 3
            && shape_names.len() == 2
            && dictionary_nodes[1].key != shape_names[0]
        {
            align_output(&mut phase.bytes, 8);
            let target = phase.bytes.len();
            phase.source_to_output.insert(shape_dictionary, target);
            put_u32(&mut phase.bytes, 0);
            put_u32(&mut phase.bytes, 2);
            let ordered = [
                (u32::MAX, 2u16, 0u16, 0usize),
                (dictionary_nodes[2].reference, 0, 1, 2),
                (dictionary_nodes[1].reference, 1, 2, 1),
            ];
            for (reference, left, right, source_index) in ordered {
                put_u32(&mut phase.bytes, reference);
                put_u16(&mut phase.bytes, left);
                put_u16(&mut phase.bytes, right);
                let field = phase.bytes.len();
                phase.bytes.extend_from_slice(&[0; 8]);
                phase.synthetic_source_pointers.push((
                    field,
                    read_u64(data, shape_dictionary + 16 + source_index * 16)? as usize,
                ));
            }
        } else {
            append_source_dictionary(
                data,
                shape_dictionary,
                &mut phase.bytes,
                &mut phase.source_to_output,
            )?;
        }
        append_source_dictionary(
            data,
            read_u64(data, model.header + 64)? as usize,
            &mut phase.bytes,
            &mut phase.source_to_output,
        )?;
    }
    Ok(())
}

pub(super) fn emit_v10_shape_phase(
    data: &[u8],
    graph: &V10ResourceGraph,
    phase: &mut V10FixedPhase,
) -> Result<(), BfresError> {
    for model in &graph.models {
        for shape in &model.shapes {
            if data[shape.header + 92] != 0 {
                return Err(error(
                    shape.header + 92,
                    "key shapes are not implemented by the native v10 emitter",
                ));
            }
            for mesh in &shape.meshes {
                append_mapped_block(
                    data,
                    *mesh,
                    56,
                    &mut phase.bytes,
                    &mut phase.source_to_output,
                )?;
            }
            if let Some((source, count)) = shape.skin_bones {
                append_mapped_block(
                    data,
                    source,
                    count * 2,
                    &mut phase.bytes,
                    &mut phase.source_to_output,
                )?;
            }
            if !shape.bounds.is_empty() {
                align_output(&mut phase.bytes, 8);
                for bounding in &shape.bounds {
                    append_mapped_block(
                        data,
                        *bounding,
                        24,
                        &mut phase.bytes,
                        &mut phase.source_to_output,
                    )?;
                }
            }
            for radius in &shape.radii {
                append_mapped_block(
                    data,
                    *radius,
                    16,
                    &mut phase.bytes,
                    &mut phase.source_to_output,
                )?;
            }
            for mesh in &shape.meshes {
                align_output(&mut phase.bytes, 8);
                let submesh_count = read_u16(data, mesh + 52)? as usize;
                let submeshes = read_u64(data, *mesh)? as usize;
                for submesh in fixed_record_offsets(submeshes, submesh_count, 8, data.len())? {
                    append_mapped_block(
                        data,
                        submesh,
                        8,
                        &mut phase.bytes,
                        &mut phase.source_to_output,
                    )?;
                }
                append_mapped_block(
                    data,
                    read_u64(data, mesh + 16)? as usize,
                    72,
                    &mut phase.bytes,
                    &mut phase.source_to_output,
                )?;
                append_mapped_block(
                    data,
                    read_u64(data, mesh + 24)? as usize,
                    16,
                    &mut phase.bytes,
                    &mut phase.source_to_output,
                )?;
            }
        }
    }
    Ok(())
}

pub(super) fn emit_v10_vertex_phase(
    data: &[u8],
    graph: &V10ResourceGraph,
    phase: &mut V10FixedPhase,
) -> Result<(), BfresError> {
    for model in &graph.models {
        for vertex in &model.vertex_buffers {
            let add_rigid_skin = read_u16(data, vertex.header + 84)? != 0
                && !vertex.attributes.iter().any(|attribute| {
                    read_u64(data, *attribute)
                        .ok()
                        .and_then(|name| read_res_string(data, name as usize).ok())
                        .is_some_and(|name| name == "_w0")
                });
            if !vertex.attributes.is_empty() {
                align_output(&mut phase.bytes, 8);
                let source_attributes = read_u64(data, vertex.header + 8)? as usize;
                for attribute in &vertex.attributes {
                    append_mapped_block(
                        data,
                        *attribute,
                        16,
                        &mut phase.bytes,
                        &mut phase.source_to_output,
                    )?;
                }
                if add_rigid_skin {
                    for (name, format, buffer_index) in
                        [("_w0", 0x0201u16, 6u8), ("_i0", 0x0203u16, 7u8)]
                    {
                        let field = phase.bytes.len();
                        phase.bytes.extend_from_slice(&[0; 8]);
                        put_u16(&mut phase.bytes, format);
                        phase.bytes.extend_from_slice(&[0; 4]);
                        phase.bytes.push(buffer_index);
                        phase.bytes.push(0);
                        phase.synthetic_strings.push((field, name.to_owned()));
                    }
                    let output_header = phase.source_to_output[&vertex.header];
                    phase.bytes[output_header + 76] = 8;
                    phase.expanded_relocations.insert(source_attributes, 8);
                }
            }
            let source_buffer_count = vertex.buffer_sizes.len();
            let buffer_count = source_buffer_count + usize::from(add_rigid_skin) * 2;
            if buffer_count != 0 {
                let runtime = read_u64(data, vertex.header + 32)? as usize;
                append_mapped_block(
                    data,
                    runtime,
                    source_buffer_count * 9 * 8,
                    &mut phase.bytes,
                    &mut phase.source_to_output,
                )?;
                if add_rigid_skin {
                    phase.bytes.extend_from_slice(&[0; 2 * 9 * 8]);
                    let output_header = phase.source_to_output[&vertex.header];
                    phase.bytes[output_header + 77] = 8;
                }
                for size in &vertex.buffer_sizes {
                    append_mapped_block(
                        data,
                        *size,
                        16,
                        &mut phase.bytes,
                        &mut phase.source_to_output,
                    )?;
                }
                if add_rigid_skin {
                    let vertex_count = read_u32(data, vertex.header + 80)?;
                    for _ in 0..2 {
                        put_u32(&mut phase.bytes, vertex_count);
                        phase.bytes.extend_from_slice(&[0; 12]);
                    }
                }
                for stride in &vertex.buffer_strides {
                    append_mapped_block(
                        data,
                        *stride,
                        16,
                        &mut phase.bytes,
                        &mut phase.source_to_output,
                    )?;
                }
                if add_rigid_skin {
                    for _ in 0..2 {
                        put_u32(&mut phase.bytes, 1);
                        phase.bytes.extend_from_slice(&[0; 12]);
                    }
                }
                let offsets = read_u64(data, vertex.header + 40)? as usize;
                append_mapped_block(
                    data,
                    offsets,
                    source_buffer_count * 8,
                    &mut phase.bytes,
                    &mut phase.source_to_output,
                )?;
                if add_rigid_skin {
                    phase.bytes.extend_from_slice(&[0; 16]);
                }
            }
            let source_dictionary = read_u64(data, vertex.header + 16)? as usize;
            if add_rigid_skin && vertex.attributes.len() == 6 {
                align_output(&mut phase.bytes, 8);
                let output_dictionary = phase.bytes.len();
                phase
                    .source_to_output
                    .insert(source_dictionary, output_dictionary);
                put_u32(&mut phase.bytes, 0);
                put_u32(&mut phase.bytes, 8);
                let topology = [
                    (u32::MAX, 1u16, 0u16),
                    (4, 0, 3),
                    (9, 5, 6),
                    (8, 2, 4),
                    (9, 8, 7),
                    (10, 1, 5),
                    (10, 6, 2),
                    (10, 3, 7),
                    (10, 8, 4),
                ];
                for (index, (reference, left, right)) in topology.into_iter().enumerate() {
                    put_u32(&mut phase.bytes, reference);
                    put_u16(&mut phase.bytes, left);
                    put_u16(&mut phase.bytes, right);
                    let field = phase.bytes.len();
                    phase.bytes.extend_from_slice(&[0; 8]);
                    if index < 7 {
                        let source_field = source_dictionary + 16 + index * 16;
                        phase
                            .synthetic_source_pointers
                            .push((field, read_u64(data, source_field)? as usize));
                    } else {
                        phase
                            .synthetic_strings
                            .push((field, if index == 7 { "_w0" } else { "_i0" }.to_owned()));
                    }
                }
                phase.expanded_relocations.insert(source_dictionary + 16, 9);
            } else {
                append_source_dictionary(
                    data,
                    source_dictionary,
                    &mut phase.bytes,
                    &mut phase.source_to_output,
                )?;
            }
        }
    }
    Ok(())
}

pub(super) fn emit_v10_material_base_phase(
    data: &[u8],
    material: &V10MaterialGraph,
    phase: &mut V10FixedPhase,
) -> Result<(), BfresError> {
    append_aligned_block(data, material.shader_info.header, 80, phase, 8)?;
    let texture_count = material.texture_names.len();
    let sampler_count = material.samplers.len();
    append_aligned_block(
        data,
        read_u64(data, material.header + 24)? as usize,
        texture_count * 8,
        phase,
        8,
    )?;
    append_aligned_records(data, &material.texture_names, 8, phase)?;
    append_aligned_block(
        data,
        read_u64(data, material.header + 40)? as usize,
        texture_count * 15 * 8,
        phase,
        8,
    )?;
    append_aligned_records(data, &material.samplers, 32, phase)?;
    append_aligned_dictionary(data, read_u64(data, material.header + 56)? as usize, phase)?;
    append_optional_range(data, material.render_info_data, phase, 8)?;
    append_optional_range(data, material.render_info_counts, phase, 8)?;
    append_optional_range(data, material.render_info_offsets, phase, 8)?;
    append_optional_range(data, material.shader_parameter_data, phase, 8)?;
    align_output(&mut phase.bytes, 128);
    append_optional_range(data, material.shader_parameter_indices, phase, 8)?;
    append_aligned_block(
        data,
        read_u64(data, material.header + 128)? as usize,
        32,
        phase,
        8,
    )?;
    append_optional_range(data, material.sampler_slots, phase, 8)?;
    append_optional_range(data, material.texture_slots, phase, 8)?;
    if sampler_count == 0 && read_u64(data, material.header + 40)? != 0 {
        return Err(error(
            material.header + 40,
            "unexpected sampler runtime table",
        ));
    }
    Ok(())
}

pub(super) fn emit_v10_material_initial_queue_phase(
    data: &[u8],
    model: &V10ModelGraph,
    phase: &mut V10FixedPhase,
) -> Result<(), BfresError> {
    let assignment_count = read_u16(data, model.header + 110)? as usize;
    let assignment_start = read_u64(data, model.header + 72)? as usize;
    for assignment in fixed_record_offsets(assignment_start, assignment_count, 88, data.len())? {
        append_aligned_block(data, assignment, 88, phase, 8)?;
    }
    for material in &model.materials {
        emit_v10_material_base_phase(data, material, phase)?;
    }
    Ok(())
}

pub(super) fn emit_v10_shader_assignment_children_phase(
    data: &[u8],
    model: &V10ModelGraph,
    phase: &mut V10FixedPhase,
) -> Result<(), BfresError> {
    let mut emitted = std::collections::HashSet::new();
    let has_rigid_skin_attributes = model
        .vertex_buffers
        .iter()
        .any(|vertex| read_u16(data, vertex.header + 84).is_ok_and(|count| count != 0));
    for material in &model.materials {
        let assignment = &material.shader_info.assignment;
        if !emitted.insert(assignment.header) {
            continue;
        }
        append_aligned_records(data, &assignment.render_info_records, 16, phase)?;
        append_aligned_dictionary(data, assignment.render_info_dict, phase)?;
        append_aligned_records(data, &assignment.shader_parameter_records, 24, phase)?;
        append_aligned_dictionary(data, assignment.shader_parameter_dict, phase)?;
        if has_rigid_skin_attributes
            && dictionary_entry_count(data, assignment.attribute_dict)? == 6
        {
            align_output(&mut phase.bytes, 8);
            let source_dictionary = assignment.attribute_dict;
            let output_dictionary = phase.bytes.len();
            phase
                .source_to_output
                .insert(source_dictionary, output_dictionary);
            put_u32(&mut phase.bytes, 0);
            put_u32(&mut phase.bytes, 8);
            let topology = [
                (u32::MAX, 1u16, 0u16),
                (4, 0, 4),
                (9, 3, 5),
                (10, 1, 3),
                (8, 2, 6),
                (10, 5, 2),
                (9, 8, 7),
                (10, 6, 7),
                (10, 8, 4),
            ];
            for (index, (reference, left, right)) in topology.into_iter().enumerate() {
                put_u32(&mut phase.bytes, reference);
                put_u16(&mut phase.bytes, left);
                put_u16(&mut phase.bytes, right);
                let field = phase.bytes.len();
                phase.bytes.extend_from_slice(&[0; 8]);
                if index < 7 {
                    phase.synthetic_source_pointers.push((
                        field,
                        read_u64(data, source_dictionary + 16 + index * 16)? as usize,
                    ));
                } else {
                    phase
                        .synthetic_strings
                        .push((field, if index == 7 { "_w0" } else { "_i0" }.to_owned()));
                }
            }
            phase.expanded_relocations.insert(source_dictionary + 16, 9);
        } else {
            append_aligned_dictionary(data, assignment.attribute_dict, phase)?;
        }
        append_aligned_dictionary(data, assignment.sampler_dict, phase)?;
        append_aligned_dictionary(data, assignment.option_dict, phase)?;
    }
    Ok(())
}

pub(super) fn emit_v10_shader_info_children_phase(
    data: &[u8],
    model: &V10ModelGraph,
    phase: &mut V10FixedPhase,
) -> Result<(), BfresError> {
    for material in &model.materials {
        let info = &material.shader_info;
        let assignment = &info.assignment;
        append_aligned_records(data, &info.attribute_values, 8, phase)?;
        let has_rigid_skin_attributes = model
            .vertex_buffers
            .iter()
            .any(|vertex| read_u16(data, vertex.header + 84).is_ok_and(|count| count != 0));
        if has_rigid_skin_attributes && info.attribute_values.len() == 6 {
            for name in ["_w0", "_i0"] {
                let field = phase.bytes.len();
                phase.bytes.extend_from_slice(&[0; 8]);
                phase.synthetic_strings.push((field, name.to_owned()));
            }
            let output_info = phase.source_to_output[&info.header];
            phase.bytes[output_info + 68] = 8;
            if let Some(&source_values) = info.attribute_values.first() {
                phase.expanded_relocations.insert(source_values, 8);
            }
        }
        let attribute_total = dictionary_entry_count(data, assignment.attribute_dict)?;
        if let Some(source) = info.attribute_indices {
            append_aligned_block(
                data,
                source,
                align_up(info.attribute_values.len() + attribute_total, 8),
                phase,
                8,
            )?;
        }
        append_aligned_records(data, &info.sampler_values, 8, phase)?;
        let sampler_total = dictionary_entry_count(data, assignment.sampler_dict)?;
        if let Some(source) = info.sampler_indices {
            append_aligned_block(
                data,
                source,
                align_up(info.sampler_values.len() + sampler_total, 8),
                phase,
                8,
            )?;
        }
        append_optional_range(data, info.option_bitflags, phase, 8)?;
        append_aligned_records(data, &info.option_values, 8, phase)?;
        let option_total = dictionary_entry_count(data, assignment.option_dict)?;
        let option_used = read_u16(data, info.header + 72)? as usize;
        if let Some(source) = info.option_indices {
            append_aligned_block(
                data,
                source,
                align_up((option_used + option_total) * 2, 8),
                phase,
                8,
            )?;
        }
    }
    Ok(())
}

fn dictionary_entry_count(data: &[u8], source: usize) -> Result<usize, BfresError> {
    if source == 0 {
        Ok(0)
    } else {
        Ok(read_u32(data, source + 4)? as usize)
    }
}

fn string_table_end(data: &[u8], start: usize) -> Result<usize, BfresError> {
    if data.get(start..start + 4) != Some(b"_STR") {
        return Err(error(start, "missing _STR signature"));
    }
    let count = read_u32(data, start + 16)? as usize;
    let mut position = start + 20;
    for _ in 0..count {
        let len = read_u16(data, position)? as usize;
        position = align_up(
            position
                .checked_add(2 + len + 1)
                .ok_or_else(|| error(position, "string table overflow"))?,
            2,
        );
        if position > data.len() {
            return Err(error(position, "string table lies outside BFRES"));
        }
    }
    Ok(position)
}

pub(super) fn emit_v10_canonical_copy(data: &[u8]) -> Result<V10FixedPhase, BfresError> {
    let graph = V10ResourceGraph::parse(data)?;
    let mut phase = emit_v10_fixed_phase(data, &graph)?;
    emit_v10_model_core_phase(data, &graph, &mut phase)?;
    emit_v10_shape_phase(data, &graph, &mut phase)?;
    emit_v10_vertex_phase(data, &graph, &mut phase)?;
    for model in &graph.models {
        emit_v10_material_initial_queue_phase(data, model, &mut phase)?;
        emit_v10_shader_assignment_children_phase(data, model, &mut phase)?;
        emit_v10_shader_info_children_phase(data, model, &mut phase)?;
    }
    let string_table = data
        .windows(4)
        .enumerate()
        .skip(0xf0)
        .find_map(|(offset, value)| (value == b"_STR").then_some(offset))
        .ok_or_else(|| error(0, "missing BFRES _STR section"))?;
    let string_end = string_table_end(data, string_table)?;
    align_output(&mut phase.bytes, 4);
    let output_string_table = append_mapped_block(
        data,
        string_table,
        string_end - string_table,
        &mut phase.bytes,
        &mut phase.source_to_output,
    )?;
    let block_offset = u16::try_from(output_string_table)
        .map_err(|_| error(output_string_table, "BFRES block offset exceeds u16"))?;
    patch_u16_at(&mut phase.bytes, 0x16, block_offset)?;
    if !phase.synthetic_strings.is_empty() {
        let synthetic_start = phase.bytes.len();
        let mut positions = HashMap::<String, usize>::new();
        let mut synthetic_values = phase
            .synthetic_strings
            .iter()
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>();
        synthetic_values.sort();
        synthetic_values.dedup();
        for value in &synthetic_values {
            if !positions.contains_key(value) {
                let position = phase.bytes.len();
                put_u16(
                    &mut phase.bytes,
                    u16::try_from(value.len())
                        .map_err(|_| error(position, "synthetic BFRES string is too long"))?,
                );
                phase.bytes.extend_from_slice(value.as_bytes());
                phase.bytes.push(0);
                phase.bytes.resize(align_up(phase.bytes.len(), 2), 0);
                positions.insert(value.clone(), position);
            }
        }
        let source_count = read_u32(data, string_table + 16)?;
        patch_u32_at(
            &mut phase.bytes,
            output_string_table + 16,
            source_count + positions.len() as u32,
        )?;
        for (field, value) in &phase.synthetic_strings {
            patch_u64(&mut phase.bytes, *field, positions[value] as u64)?;
        }
        let added_size = (phase.bytes.len() - synthetic_start) as u32;
        patch_u32_at(
            &mut phase.bytes,
            0xd8,
            read_u32(data, 0xd8)?.saturating_add(added_size),
        )?;
    }
    align_output(&mut phase.bytes, 4096);
    let source_buffer = read_u64(data, graph.buffer_info + 8)? as usize;
    let output_buffer = phase.bytes.len();
    phase.source_to_output.insert(source_buffer, output_buffer);
    for model in &graph.models {
        for shape in &model.shapes {
            for mesh in &shape.meshes {
                align_output(&mut phase.bytes, 8);
                let source_data = source_buffer + read_u32(data, mesh + 32)? as usize;
                let size_record = read_u64(data, mesh + 24)? as usize;
                let size = read_u32(data, size_record)? as usize;
                let target = phase.bytes.len();
                append_mapped_block(
                    data,
                    source_data,
                    size,
                    &mut phase.bytes,
                    &mut phase.source_to_output,
                )?;
                let output_mesh = *phase
                    .source_to_output
                    .get(mesh)
                    .ok_or_else(|| error(*mesh, "mesh header was not emitted"))?;
                patch_u32_at(
                    &mut phase.bytes,
                    output_mesh + 32,
                    (target - output_buffer) as u32,
                )?;
            }
        }
    }
    for model in &graph.models {
        for vertex in &model.vertex_buffers {
            let alignment = read_u16(data, vertex.header + 86)? as usize;
            let mut source_data = source_buffer + read_u32(data, vertex.header + 72)? as usize;
            let mut first_target = None;
            for size_record in &vertex.buffer_sizes {
                source_data = align_up(source_data, alignment);
                align_output(&mut phase.bytes, alignment);
                first_target.get_or_insert(phase.bytes.len());
                let size = read_u32(data, *size_record)? as usize;
                append_mapped_block(
                    data,
                    source_data,
                    size,
                    &mut phase.bytes,
                    &mut phase.source_to_output,
                )?;
                source_data += size;
            }
            let add_rigid_skin = read_u16(data, vertex.header + 84)? != 0
                && !vertex.attributes.iter().any(|attribute| {
                    read_u64(data, *attribute)
                        .ok()
                        .and_then(|name| read_res_string(data, name as usize).ok())
                        .is_some_and(|name| name == "_w0")
                });
            if add_rigid_skin {
                let vertex_count = read_u32(data, vertex.header + 80)? as usize;
                align_output(&mut phase.bytes, alignment);
                first_target.get_or_insert(phase.bytes.len());
                phase
                    .bytes
                    .resize(phase.bytes.len() + vertex_count, u8::MAX);
                align_output(&mut phase.bytes, alignment);
                phase.bytes.resize(phase.bytes.len() + vertex_count, 0);
            }
            if let Some(target) = first_target {
                let output_vertex = *phase
                    .source_to_output
                    .get(&vertex.header)
                    .ok_or_else(|| error(vertex.header, "vertex header was not emitted"))?;
                patch_u32_at(
                    &mut phase.bytes,
                    output_vertex + 72,
                    (target - output_buffer) as u32,
                )?;
            }
        }
    }
    align_output(&mut phase.bytes, 4096);
    let buffer_total_size = (phase.bytes.len() - output_buffer) as u32;
    let source_memory = read_u64(data, 0xa8)? as usize;
    append_mapped_block(
        data,
        source_memory,
        288,
        &mut phase.bytes,
        &mut phase.source_to_output,
    )?;
    let output_buffer_info = *phase
        .source_to_output
        .get(&graph.buffer_info)
        .ok_or_else(|| error(graph.buffer_info, "buffer info was not emitted"))?;
    patch_u32_at(&mut phase.bytes, output_buffer_info + 4, buffer_total_size)?;
    for external in &graph.external_files {
        let source_data = read_u64(data, *external)? as usize;
        let size = read_u64(data, *external + 8)? as usize;
        if size != 0 {
            append_mapped_block(
                data,
                source_data,
                size,
                &mut phase.bytes,
                &mut phase.source_to_output,
            )?;
        }
    }
    align_output(&mut phase.bytes, 256);
    let source_rlt = read_u32(data, 0x18)? as usize;
    let source_file_size = read_u32(data, 0x1c)? as usize;
    append_mapped_block(
        data,
        source_rlt,
        source_file_size - source_rlt,
        &mut phase.bytes,
        &mut phase.source_to_output,
    )?;
    // Switch Toolbox pads the complete BFRES (including `_RLT`) to the file
    // alignment, and records that padded length in the root header.
    align_output(&mut phase.bytes, 4096);
    Ok(phase)
}

pub(super) fn rebase_v10_relocations(
    source: &[u8],
    phase: &mut V10FixedPhase,
) -> Result<(), BfresError> {
    if phase.bytes.as_slice() == source.get(..phase.bytes.len()).unwrap_or_default() {
        return Ok(());
    }
    let source_rlt = read_u32(source, 0x18)? as usize;
    if source.get(source_rlt..source_rlt + 4) != Some(b"_RLT") {
        return Err(error(source_rlt, "missing source _RLT section"));
    }
    let output_rlt = translate_source_offset(phase, source_rlt)?;
    patch_u32_at(&mut phase.bytes, 0x18, output_rlt as u32)?;
    patch_u32_at(&mut phase.bytes, output_rlt + 4, output_rlt as u32)?;
    if let Some(output_string_table) = phase.bytes.windows(4).position(|value| value == b"_STR") {
        patch_u32_at(
            &mut phase.bytes,
            output_string_table + 8,
            (output_rlt - output_string_table) as u32,
        )?;
    }
    // Toolbox leaves the final alignment padding outside the logical size
    // recorded in the BFRES header.
    let output_size = output_rlt
        .checked_add(read_u32(source, 0x1c)? as usize - source_rlt)
        .ok_or_else(|| error(output_rlt, "BFRES logical size overflow"))?
        as u32;
    patch_u32_at(&mut phase.bytes, 0x1c, output_size)?;
    let file_name = read_u32(source, 0x10)? as usize;
    if file_name != 0 {
        let output_file_name = translate_source_offset(phase, file_name)? as u32;
        patch_u32_at(&mut phase.bytes, 0x10, output_file_name)?;
    }

    let section_count = read_u32(source, source_rlt + 8)? as usize;
    let section_headers = source_rlt + 16;
    let entries_start = section_headers + section_count * 24;
    for section in 0..section_count {
        let source_header = section_headers + section * 24;
        let output_header = output_rlt + 16 + section * 24;
        let source_start = read_u32(source, source_header + 8)? as usize;
        let source_size = read_u32(source, source_header + 12)? as usize;
        let translated_start = translate_source_offset(phase, source_start)?;
        let translated_end = translate_source_offset(phase, source_start + source_size)?;
        patch_u32_at(&mut phase.bytes, output_header + 8, translated_start as u32)?;
        patch_u32_at(
            &mut phase.bytes,
            output_header + 12,
            translated_end.saturating_sub(translated_start) as u32,
        )?;
        let entry_index = read_u32(source, source_header + 16)? as usize;
        let entry_count = read_u32(source, source_header + 20)? as usize;
        for index in entry_index..entry_index + entry_count {
            let source_entry = entries_start + index * 8;
            let output_entry = output_rlt + (entries_start - source_rlt) + index * 8;
            let position = read_u32(source, source_entry)? as usize;
            let struct_count = read_u16(source, source_entry + 4)? as usize;
            let offset_count = source[source_entry + 6] as usize;
            let padding_count = source[source_entry + 7] as usize;
            let translated_position = translate_source_offset(phase, position)?;
            patch_u32_at(&mut phase.bytes, output_entry, translated_position as u32)?;
            if struct_count == 8 && offset_count == 6 {
                patch_u16_at(&mut phase.bytes, output_entry + 4, 1)?;
                phase.bytes[output_entry + 6] = 8;
            }
            if let Some(&expanded_count) = phase.expanded_relocations.get(&position) {
                // Toolbox rewrites an expanded pointer array as one structure
                // containing every pointer, rather than preserving the source
                // array's structure count and stride.
                if !(expanded_count == 8 && struct_count == 8 && offset_count == 6) {
                    patch_u16_at(&mut phase.bytes, output_entry + 4, expanded_count)?;
                }
            }
            for structure in 0..struct_count {
                let structure_start = position + structure * (offset_count + padding_count) * 8;
                for pointer in 0..offset_count {
                    let source_field = structure_start + pointer * 8;
                    let output_field = translate_source_offset(phase, source_field)?;
                    let source_target = read_u64(source, source_field)? as usize;
                    if source_target != 0 {
                        let output_target = translate_source_offset(phase, source_target)? as u64;
                        patch_u64(&mut phase.bytes, output_field, output_target)?;
                    }
                }
            }
        }
        let output_entries = output_rlt + (entries_start - source_rlt) + entry_index * 8;
        let mut records = phase.bytes[output_entries..output_entries + entry_count * 8]
            .chunks_exact(8)
            .map(<[u8]>::to_vec)
            .collect::<Vec<_>>();
        for record in &mut records {
            if BinaryReader::new(record).read_u16_at(4).unwrap() == 8 && record[6] == 6 {
                let mut writer = BinaryWriter::from_vec(record.clone(), BinaryEndian::Little);
                writer.write_u16_at(4, 1);
                *record = writer.into_inner();
                record[6] = 8;
            }
        }
        records.sort_by_key(|record| BinaryReader::new(record).read_u32_at(0).unwrap());
        for (index, record) in records.into_iter().enumerate() {
            phase.bytes[output_entries + index * 8..output_entries + (index + 1) * 8]
                .copy_from_slice(&record);
        }
    }
    // ResFileSwitchSaver describes all pointer fields in the v10 root header.
    // Older source files can carry a shorter root relocation record.
    *phase
        .bytes
        .get_mut(output_rlt + (entries_start - source_rlt) + 6)
        .ok_or_else(|| error(output_rlt, "root relocation record lies outside output"))? = 17;
    for &(field, source_target) in &phase.synthetic_source_pointers {
        let output_target = translate_source_offset(phase, source_target)? as u64;
        patch_u64(&mut phase.bytes, field, output_target)?;
    }
    let graph = V10ResourceGraph::parse(source)?;
    for model in &graph.models {
        let output_model = translate_source_offset(phase, model.header)?;
        if let Some(vertex) = model.vertex_buffers.first() {
            let output_vertex = translate_source_offset(phase, vertex.header)? as u64;
            patch_u64(&mut phase.bytes, output_model + 32, output_vertex)?;
        }
        if let Some(shape) = model.shapes.first() {
            let output_shape = translate_source_offset(phase, shape.header)? as u64;
            patch_u64(&mut phase.bytes, output_model + 40, output_shape)?;
        }
    }
    // Toolbox derives the first three section ranges from the blocks it has
    // just serialized, rather than translating the stale ranges in the input.
    let output_graph = V10ResourceGraph::parse(&phase.bytes)?;
    let output_buffer = read_u64(&phase.bytes, output_graph.buffer_info + 8)? as usize;
    let first_vertex_data = output_graph
        .models
        .iter()
        .flat_map(|model| &model.vertex_buffers)
        .map(|vertex| {
            read_u32(&phase.bytes, vertex.header + 72).map(|offset| output_buffer + offset as usize)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or_else(|| error(output_buffer, "BFRES has no vertex buffers"))?;
    let string_table = phase
        .bytes
        .windows(4)
        .position(|value| value == b"_STR")
        .ok_or_else(|| error(0, "missing output string table"))?;
    let section_zero_size = string_table + read_u32(&phase.bytes, 0xd8)? as usize;
    let section_one_size = first_vertex_data.saturating_sub(output_buffer);
    let section_two_start = first_vertex_data.saturating_sub(2);
    let mut last_vertex_end = first_vertex_data;
    for vertex in output_graph
        .models
        .iter()
        .flat_map(|model| &model.vertex_buffers)
    {
        let alignment = read_u16(&phase.bytes, vertex.header + 86)? as usize;
        let mut cursor = output_buffer + read_u32(&phase.bytes, vertex.header + 72)? as usize;
        for size_record in &vertex.buffer_sizes {
            cursor = align_up(cursor, alignment);
            cursor += read_u32(&phase.bytes, *size_record)? as usize;
        }
        last_vertex_end = last_vertex_end.max(cursor);
    }
    // The buffer section includes the five-byte trailing guard written by
    // ResFileSwitchSaver after the final vertex stream.
    let buffer_section_size = last_vertex_end + 5 - section_two_start;
    for (section, start, size) in [
        (0usize, 0usize, section_zero_size),
        (1, output_buffer, section_one_size),
        (2, section_two_start, buffer_section_size),
    ] {
        let header = output_rlt + 16 + section * 24;
        patch_u32_at(&mut phase.bytes, header + 8, start as u32)?;
        patch_u32_at(&mut phase.bytes, header + 12, size as u32)?;
    }
    Ok(())
}

fn translate_source_offset(phase: &V10FixedPhase, source: usize) -> Result<usize, BfresError> {
    let (block_source, block_target) = phase
        .source_to_output
        .iter()
        .filter(|(candidate, _)| **candidate <= source)
        .max_by_key(|(candidate, _)| **candidate)
        .ok_or_else(|| error(source, "source offset has no emitted block"))?;
    block_target
        .checked_add(source - block_source)
        .ok_or_else(|| error(source, "translated offset overflow"))
}

fn patch_u32_at(output: &mut [u8], offset: usize, value: u32) -> Result<(), BfresError> {
    if offset.checked_add(4).is_none_or(|end| end > output.len()) {
        return Err(error(offset, "BFRES u32 field lies outside output"));
    }
    let mut writer = BinaryWriter::from_vec(output.to_vec(), BinaryEndian::Little);
    writer.write_u32_at(offset, value);
    output.copy_from_slice(&writer.into_inner());
    Ok(())
}

fn append_optional_range(
    data: &[u8],
    range: Option<(usize, usize)>,
    phase: &mut V10FixedPhase,
    alignment: usize,
) -> Result<(), BfresError> {
    if let Some((source, len)) = range {
        append_aligned_block(data, source, len, phase, alignment)?;
    }
    Ok(())
}

fn append_aligned_records(
    data: &[u8],
    records: &[usize],
    stride: usize,
    phase: &mut V10FixedPhase,
) -> Result<(), BfresError> {
    if records.is_empty() {
        return Ok(());
    }
    align_output(&mut phase.bytes, 8);
    for source in records {
        append_mapped_block(
            data,
            *source,
            stride,
            &mut phase.bytes,
            &mut phase.source_to_output,
        )?;
    }
    Ok(())
}

fn append_aligned_dictionary(
    data: &[u8],
    source: usize,
    phase: &mut V10FixedPhase,
) -> Result<(), BfresError> {
    append_source_dictionary(data, source, &mut phase.bytes, &mut phase.source_to_output)
}

fn append_aligned_block(
    data: &[u8],
    source: usize,
    len: usize,
    phase: &mut V10FixedPhase,
    alignment: usize,
) -> Result<(), BfresError> {
    if len == 0 {
        return Ok(());
    }
    align_output(&mut phase.bytes, alignment);
    append_mapped_block(
        data,
        source,
        len,
        &mut phase.bytes,
        &mut phase.source_to_output,
    )?;
    Ok(())
}

fn append_source_dictionary(
    data: &[u8],
    source: usize,
    output: &mut Vec<u8>,
    source_to_output: &mut HashMap<usize, usize>,
) -> Result<(), BfresError> {
    if source == 0 {
        return Ok(());
    }
    align_output(output, 8);
    let node_count = read_u32(data, source + 4)? as usize + 1;
    append_mapped_block(data, source, 8 + node_count * 16, output, source_to_output)?;
    Ok(())
}

fn align_output(output: &mut Vec<u8>, alignment: usize) {
    output.resize(align_up(output.len(), alignment), 0);
}

fn append_mapped_block(
    data: &[u8],
    source: usize,
    len: usize,
    output: &mut Vec<u8>,
    source_to_output: &mut HashMap<usize, usize>,
) -> Result<usize, BfresError> {
    let end = source
        .checked_add(len)
        .ok_or_else(|| error(source, "block overflow"))?;
    let block = data
        .get(source..end)
        .ok_or_else(|| error(source, "block lies outside BFRES"))?;
    let target = output.len();
    output.extend_from_slice(block);
    source_to_output.insert(source, target);
    Ok(target)
}

fn optional_pointer(data: &[u8], offset: usize) -> Result<Option<usize>, BfresError> {
    if offset == 0 {
        return Ok(None);
    }
    if offset >= data.len() {
        return Err(error(offset, "pointer lies outside BFRES"));
    }
    Ok(Some(offset))
}

fn checked_dictionary(data: &[u8], offset: usize) -> Result<usize, BfresError> {
    if offset == 0 {
        return Ok(0);
    }
    read_dictionary(data, offset)?;
    Ok(offset)
}

fn optional_byte_range(
    start: usize,
    len: usize,
    file_len: usize,
) -> Result<Option<(usize, usize)>, BfresError> {
    if start == 0 || len == 0 {
        return Ok(None);
    }
    let end = start
        .checked_add(len)
        .ok_or_else(|| error(start, "byte range overflow"))?;
    if end > file_len {
        return Err(error(start, "byte range lies outside BFRES"));
    }
    Ok(Some((start, len)))
}

fn optional_array(
    start: usize,
    count: usize,
    stride: usize,
    file_len: usize,
) -> Result<Option<(usize, usize)>, BfresError> {
    if count == 0 || start == 0 {
        return Ok(None);
    }
    fixed_record_offsets(start, count, stride, file_len)?;
    Ok(Some((start, count)))
}

fn fixed_record_offsets(
    start: usize,
    count: usize,
    stride: usize,
    file_len: usize,
) -> Result<Vec<usize>, BfresError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let end = start
        .checked_add(
            count
                .checked_mul(stride)
                .ok_or_else(|| error(start, "array overflow"))?,
        )
        .ok_or_else(|| error(start, "array overflow"))?;
    if start == 0 || end > file_len {
        return Err(error(start, "array lies outside BFRES"));
    }
    Ok((0..count).map(|index| start + index * stride).collect())
}

const RLT_ALIGNMENT: usize = 0x100;
const RLT_SECTION_COUNT: usize = 5;

#[derive(Clone, Debug)]
pub(super) struct ResFileHeaderSpec<'a> {
    pub version: [u8; 4],
    pub alignment: u8,
    pub target_address_size: u8,
    pub name: &'a str,
    pub flags: u16,
    pub model_count: u16,
    pub external_file_count: u16,
}

#[derive(Clone, Debug)]
pub(super) struct ResFileHeaderLayout {
    pub file_name_field: usize,
    pub relocation_field: usize,
    pub file_size_field: usize,
    pub name_field: usize,
    pub model_array_field: usize,
    pub model_dict_field: usize,
    pub memory_pool_field: usize,
    pub buffer_info_field: usize,
    pub string_pool_field: usize,
}

#[derive(Clone, Debug)]
pub(super) struct ModelHeaderSpec<'a> {
    pub flags: u32,
    pub name: &'a str,
    pub path: &'a str,
    pub vertex_buffer_count: u16,
    pub shape_count: u16,
    pub material_count: u16,
    pub shader_assign_count: u16,
    pub user_data_count: u16,
}

#[derive(Clone, Debug)]
pub(super) struct ModelHeaderLayout {
    pub start: usize,
    pub skeleton_field: usize,
    pub vertex_buffer_field: usize,
    pub shape_array_field: usize,
    pub shape_dict_field: usize,
    pub material_array_field: usize,
    pub material_dict_field: usize,
    pub shader_assign_field: usize,
    pub user_data_array_field: usize,
    pub user_data_dict_field: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DictionaryNode {
    pub reference: u32,
    pub left: u16,
    pub right: u16,
    pub key: String,
}

#[derive(Clone, Debug)]
pub(super) struct BufferInfoLayout {
    pub start: usize,
    pub total_size_field: usize,
    pub index_buffer_field: usize,
}

pub(super) fn write_buffer_info(
    output: &mut Vec<u8>,
    unknown: u32,
    total_size: u32,
    relocation: &mut RelocationTableBuilder,
) -> Result<BufferInfoLayout, BfresError> {
    let start = output.len();
    put_u32(output, unknown);
    let total_size_field = output.len();
    put_u32(output, total_size);
    let index_buffer_field = reserve(output, 8);
    output.extend_from_slice(&[0; 16]);
    relocation.add(2, index_buffer_field as u32, 1, 1, 0)?;
    Ok(BufferInfoLayout {
        start,
        total_size_field,
        index_buffer_field,
    })
}

#[derive(Clone, Debug)]
pub(super) struct ExternalFileLayout {
    pub start: usize,
    pub data_field: usize,
}

#[derive(Clone, Debug)]
pub(super) struct VertexBufferHeaderSpec {
    pub flags: u32,
    pub buffer_offset: u32,
    pub attribute_count: u8,
    pub buffer_count: u8,
    pub index: u16,
    pub vertex_count: u32,
    pub vertex_skin_count: u16,
    pub gpu_alignment: u16,
}

#[derive(Clone, Debug)]
pub(super) struct VertexBufferHeaderLayout {
    pub start: usize,
    pub attribute_array_field: usize,
    pub attribute_dict_field: usize,
    pub memory_pool_field: usize,
    pub unknown_buffer_field: usize,
    pub unknown_buffer2_field: usize,
    pub buffer_size_array_field: usize,
    pub stride_array_field: usize,
}

#[derive(Clone, Debug)]
pub(super) struct MaterialHeaderSpec<'a> {
    pub flags: u32,
    pub name: &'a str,
    pub material_index: u16,
    pub texture_count: u8,
    pub sampler_count: u8,
    pub volatile_shader_param_count: u16,
    pub user_data_count: u16,
    pub render_info_data_size: u16,
}

#[derive(Clone, Debug)]
pub(super) struct MaterialHeaderLayout {
    pub start: usize,
    pub shader_info_field: usize,
    pub texture_runtime_field: usize,
    pub texture_names_field: usize,
    pub sampler_runtime_field: usize,
    pub sampler_array_field: usize,
    pub sampler_dict_field: usize,
    pub render_info_data_field: usize,
    pub render_info_counts_field: usize,
    pub render_info_offsets_field: usize,
    pub shader_parameter_data_field: usize,
    pub shader_parameter_indices_field: usize,
    pub user_data_array_field: usize,
    pub user_data_dict_field: usize,
    pub volatile_flags_field: usize,
    pub sampler_slots_field: usize,
    pub texture_slots_field: usize,
}

#[derive(Clone, Debug)]
pub(super) struct ShapeHeaderSpec<'a> {
    pub flags: u32,
    pub name: &'a str,
    pub vertex_buffer_offset: u64,
    pub shape_index: u16,
    pub material_index: u16,
    pub bone_index: u16,
    pub vertex_buffer_index: u16,
    pub skin_bone_count: u16,
    pub vertex_skin_count: u8,
    pub mesh_count: u8,
    pub key_shape_count: u8,
    pub target_attribute_count: u8,
}

#[derive(Clone, Debug)]
pub(super) struct ShapeHeaderLayout {
    pub start: usize,
    pub mesh_array_field: usize,
    pub skin_bones_field: usize,
    pub key_shape_array_field: usize,
    pub key_shape_dict_field: usize,
    pub bounds_field: usize,
    pub radius_field: usize,
}

#[derive(Clone, Debug)]
pub(super) struct SkeletonHeaderSpec {
    pub flags: u32,
    pub bone_count: u16,
    pub smooth_matrix_count: u16,
    pub rigid_matrix_count: u16,
}

#[derive(Clone, Debug)]
pub(super) struct SkeletonHeaderLayout {
    pub start: usize,
    pub bone_dict_field: usize,
    pub bone_array_field: usize,
    pub matrix_to_bone_field: usize,
    pub inverse_matrices_field: usize,
    pub mirrored_indices_field: usize,
}

#[derive(Clone, Debug)]
pub(super) struct MeshHeaderSpec {
    pub memory_pool_offset: u64,
    pub face_buffer_offset: u32,
    pub primitive_type: u32,
    pub index_format: u32,
    pub index_count: u32,
    pub first_vertex: u32,
    pub submesh_count: u16,
}

#[derive(Clone, Debug)]
pub(super) struct MeshHeaderLayout {
    pub start: usize,
    pub submesh_array_field: usize,
    pub unknown_buffer_field: usize,
    pub buffer_size_field: usize,
}

#[derive(Clone, Debug)]
pub(super) struct BoneHeaderSpec<'a> {
    pub name: &'a str,
    pub index: u16,
    pub parent_index: i16,
    pub smooth_matrix_index: i16,
    pub rigid_matrix_index: i16,
    pub billboard_index: i16,
    pub user_data_count: u16,
    pub flags: u32,
    pub scale: [f32; 3],
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
}

#[derive(Clone, Debug)]
pub(super) struct BoneHeaderLayout {
    pub start: usize,
    pub user_data_array_field: usize,
    pub user_data_dict_field: usize,
}

pub(super) fn write_v10_bone_header(
    output: &mut Vec<u8>,
    spec: &BoneHeaderSpec<'_>,
    strings: &mut StringTableBuilder,
) -> BoneHeaderLayout {
    let start = output.len();
    let name_field = reserve(output, 8);
    strings.register(spec.name, name_field);
    let user_data_array_field = reserve(output, 8);
    let user_data_dict_field = reserve(output, 8);
    output.extend_from_slice(&[0; 8]);
    put_u16(output, spec.index);
    put_i16(output, spec.parent_index);
    put_i16(output, spec.smooth_matrix_index);
    put_i16(output, spec.rigid_matrix_index);
    put_i16(output, spec.billboard_index);
    put_u16(output, spec.user_data_count);
    put_u32(output, spec.flags);
    for value in spec.scale {
        put_f32(output, value);
    }
    for value in spec.rotation {
        put_f32(output, value);
    }
    for value in spec.translation {
        put_f32(output, value);
    }
    BoneHeaderLayout {
        start,
        user_data_array_field,
        user_data_dict_field,
    }
}

pub(super) fn register_v10_bone_array(
    relocation: &mut RelocationTableBuilder,
    start: usize,
    count: usize,
) -> Result<(), BfresError> {
    if count != 0 {
        relocation.add(
            1,
            start as u32,
            3,
            u16::try_from(count).map_err(|_| error(start, "too many BFRES bones"))?,
            8,
        )?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub(super) struct VertexAttributeSpec<'a> {
    pub name: &'a str,
    pub format: u16,
    pub offset: u16,
    pub buffer_index: u8,
}

#[derive(Clone, Debug)]
pub(super) struct ShaderInfoSpec {
    pub attribute_assign_count: u8,
    pub sampler_assign_count: u8,
    pub boolean_option_count: u16,
    pub option_count: u16,
}

#[derive(Clone, Debug)]
pub(super) struct ShaderInfoLayout {
    pub start: usize,
    pub shader_assign_field: usize,
    pub attribute_values_field: usize,
    pub attribute_indices_field: usize,
    pub sampler_values_field: usize,
    pub sampler_indices_field: usize,
    pub option_toggles_field: usize,
    pub option_values_field: usize,
    pub option_indices_field: usize,
}

pub(super) fn write_shader_info(
    output: &mut Vec<u8>,
    spec: &ShaderInfoSpec,
    relocation: &mut RelocationTableBuilder,
) -> Result<ShaderInfoLayout, BfresError> {
    let start = output.len();
    let shader_assign_field = reserve(output, 8);
    let attribute_values_field = reserve(output, 8);
    let attribute_indices_field = reserve(output, 8);
    let sampler_values_field = reserve(output, 8);
    let sampler_indices_field = reserve(output, 8);
    let option_toggles_field = reserve(output, 8);
    let option_values_field = reserve(output, 8);
    let option_indices_field = reserve(output, 8);
    put_u32(output, 0);
    output.push(spec.attribute_assign_count);
    output.push(spec.sampler_assign_count);
    put_u16(output, spec.boolean_option_count);
    put_u16(output, spec.option_count);
    put_u16(output, 0);
    put_u32(output, 0);
    relocation.add(1, start as u32, 8, 1, 0)?;
    Ok(ShaderInfoLayout {
        start,
        shader_assign_field,
        attribute_values_field,
        attribute_indices_field,
        sampler_values_field,
        sampler_indices_field,
        option_toggles_field,
        option_values_field,
        option_indices_field,
    })
}

#[derive(Clone, Debug)]
pub(super) struct ShaderAssignSpec<'a> {
    pub shader_archive_name: &'a str,
    pub shading_model_name: &'a str,
    pub render_info_count: u16,
    pub shader_parameter_count: u16,
    pub shader_parameter_data_size: u16,
}

#[derive(Clone, Debug)]
pub(super) struct ShaderAssignLayout {
    pub start: usize,
    pub render_info_list_field: usize,
    pub render_info_dict_field: usize,
    pub shader_parameter_list_field: usize,
    pub shader_parameter_dict_field: usize,
    pub attribute_dict_field: usize,
    pub sampler_dict_field: usize,
    pub option_dict_field: usize,
}

pub(super) fn write_shader_assign(
    output: &mut Vec<u8>,
    spec: &ShaderAssignSpec<'_>,
    strings: &mut StringTableBuilder,
    relocation: &mut RelocationTableBuilder,
) -> Result<ShaderAssignLayout, BfresError> {
    let start = output.len();
    let archive_field = reserve(output, 8);
    strings.register(spec.shader_archive_name, archive_field);
    let model_field = reserve(output, 8);
    strings.register(spec.shading_model_name, model_field);
    let render_info_list_field = reserve(output, 8);
    let render_info_dict_field = reserve(output, 8);
    let shader_parameter_list_field = reserve(output, 8);
    let shader_parameter_dict_field = reserve(output, 8);
    let attribute_dict_field = reserve(output, 8);
    let sampler_dict_field = reserve(output, 8);
    let option_dict_field = reserve(output, 8);
    put_u16(output, spec.render_info_count);
    put_u16(output, spec.shader_parameter_count);
    put_u16(output, spec.shader_parameter_data_size);
    put_u16(output, 0);
    output.extend_from_slice(&[0; 8]);
    relocation.add(1, start as u32, 9, 1, 0)?;
    Ok(ShaderAssignLayout {
        start,
        render_info_list_field,
        render_info_dict_field,
        shader_parameter_list_field,
        shader_parameter_dict_field,
        attribute_dict_field,
        sampler_dict_field,
        option_dict_field,
    })
}

#[derive(Clone, Debug)]
pub(super) struct RenderInfoRecordSpec<'a> {
    pub name: &'a str,
    pub value_type: u8,
}

pub(super) fn write_render_info_record(
    output: &mut Vec<u8>,
    spec: &RenderInfoRecordSpec<'_>,
    strings: &mut StringTableBuilder,
) -> usize {
    let start = output.len();
    let name_field = output.len();
    put_u64(output, 0);
    strings.register(spec.name, name_field);
    output.push(spec.value_type);
    output.extend_from_slice(&[0; 7]);
    start
}

pub(super) fn write_shader_parameter_record(
    output: &mut Vec<u8>,
    name: &str,
    data_offset: u16,
    parameter_type: u16,
    strings: &mut StringTableBuilder,
) -> usize {
    let start = output.len();
    put_u64(output, 0);
    let name_field = output.len();
    put_u64(output, 0);
    strings.register(name, name_field);
    put_u16(output, data_offset);
    put_u16(output, parameter_type);
    put_u32(output, 0);
    start
}

pub(super) fn write_bounding(output: &mut Vec<u8>, center: [f32; 3], extent: [f32; 3]) -> usize {
    let start = output.len();
    for value in center.into_iter().chain(extent) {
        put_f32(output, value);
    }
    start
}

pub(super) fn write_bounding_radius(output: &mut Vec<u8>, radius: [f32; 4]) -> usize {
    let start = output.len();
    for value in radius {
        put_f32(output, value);
    }
    start
}

#[derive(Clone, Debug)]
pub(super) struct SamplerSwitchSpec {
    pub wrap_u: u8,
    pub wrap_v: u8,
    pub wrap_w: u8,
    pub compare_function: u8,
    pub border_color_type: u8,
    pub anisotropic: u8,
    pub filter_flags: u16,
    pub min_lod: f32,
    pub max_lod: f32,
    pub lod_bias: f32,
}

pub(super) fn write_sampler_switch(output: &mut Vec<u8>, spec: &SamplerSwitchSpec) -> usize {
    let start = output.len();
    output.extend_from_slice(&[
        spec.wrap_u,
        spec.wrap_v,
        spec.wrap_w,
        spec.compare_function,
        spec.border_color_type,
        spec.anisotropic,
    ]);
    put_u16(output, spec.filter_flags);
    for value in [spec.min_lod, spec.max_lod, spec.lod_bias] {
        put_f32(output, value);
    }
    output.extend_from_slice(&[0; 12]);
    start
}

pub(super) fn write_string_pointer_array(
    output: &mut Vec<u8>,
    values: &[String],
    strings: &mut StringTableBuilder,
    relocation: &mut RelocationTableBuilder,
    label_section: u8,
) -> Result<usize, BfresError> {
    let start = output.len();
    if !values.is_empty() {
        relocation.add(
            label_section as usize,
            start as u32,
            values.len() as u32,
            1,
            0,
        )?;
    }
    for value in values {
        let field = reserve(output, 8);
        strings.register(value, field);
    }
    Ok(start)
}

pub(super) fn write_u16_array(output: &mut Vec<u8>, values: &[u16]) -> usize {
    let start = output.len();
    for value in values {
        put_u16(output, *value);
    }
    start
}

pub(super) fn write_matrix_3x4(output: &mut Vec<u8>, values: [f32; 12]) -> usize {
    let start = output.len();
    for value in values {
        put_f32(output, value);
    }
    start
}

pub(super) fn write_mesh_runtime_buffer(output: &mut Vec<u8>) -> usize {
    let start = output.len();
    output.extend_from_slice(&[0; 9 * 8]);
    start
}

pub(super) fn write_vertex_attribute(
    output: &mut Vec<u8>,
    spec: &VertexAttributeSpec<'_>,
    strings: &mut StringTableBuilder,
) -> usize {
    let start = output.len();
    let name_field = reserve(output, 8);
    strings.register(spec.name, name_field);
    put_u16_be(output, spec.format);
    output.extend_from_slice(&[0; 2]);
    put_u16(output, spec.offset);
    output.push(spec.buffer_index);
    output.push(0);
    start
}

pub(super) fn register_vertex_attribute_array(
    relocation: &mut RelocationTableBuilder,
    start: usize,
    count: usize,
) -> Result<(), BfresError> {
    if count != 0 {
        relocation.add(
            1,
            start as u32,
            1,
            u16::try_from(count).map_err(|_| error(start, "too many vertex attributes"))?,
            1,
        )?;
    }
    Ok(())
}

pub(super) fn write_vertex_buffer_size(output: &mut Vec<u8>, size: u32, gpu_flags: u32) -> usize {
    let start = output.len();
    put_u32(output, size);
    put_u32(output, gpu_flags);
    output.extend_from_slice(&[0; 8]);
    start
}

pub(super) fn write_vertex_buffer_stride(output: &mut Vec<u8>, stride: u32) -> usize {
    let start = output.len();
    put_u32(output, stride);
    output.extend_from_slice(&[0; 12]);
    start
}

pub(super) fn write_mesh_header(
    output: &mut Vec<u8>,
    spec: &MeshHeaderSpec,
    relocation: &mut RelocationTableBuilder,
) -> Result<MeshHeaderLayout, BfresError> {
    let start = output.len();
    let submesh_array_field = reserve(output, 8);
    put_u64(output, spec.memory_pool_offset);
    let unknown_buffer_field = reserve(output, 8);
    let buffer_size_field = reserve(output, 8);
    put_u32(output, spec.face_buffer_offset);
    put_u32(output, spec.primitive_type);
    put_u32(output, spec.index_format);
    put_u32(output, spec.index_count);
    put_u32(output, spec.first_vertex);
    put_u16(output, spec.submesh_count);
    output.extend_from_slice(&[0; 2]);
    relocation.add(1, submesh_array_field as u32, 1, 1, 0)?;
    relocation.add(4, (start + 8) as u32, 1, 1, 0)?;
    relocation.add(1, unknown_buffer_field as u32, 2, 1, 0)?;
    Ok(MeshHeaderLayout {
        start,
        submesh_array_field,
        unknown_buffer_field,
        buffer_size_field,
    })
}

pub(super) fn write_submesh(output: &mut Vec<u8>, offset: u32, count: u32) -> usize {
    let start = output.len();
    put_u32(output, offset);
    put_u32(output, count);
    start
}

pub(super) fn write_buffer_size(output: &mut Vec<u8>, size: u32, flags: u32) -> usize {
    let start = output.len();
    put_u32(output, size);
    put_u32(output, flags);
    output.extend_from_slice(&[0; 8]);
    start
}

pub(super) fn write_v10_skeleton_header(
    output: &mut Vec<u8>,
    spec: &SkeletonHeaderSpec,
    relocation: &mut RelocationTableBuilder,
) -> Result<SkeletonHeaderLayout, BfresError> {
    let start = output.len();
    output.extend_from_slice(b"FSKL");
    put_u32(output, spec.flags);
    let bone_dict_field = reserve(output, 8);
    let bone_array_field = reserve(output, 8);
    let matrix_to_bone_field = reserve(output, 8);
    let inverse_matrices_field = reserve(output, 8);
    output.extend_from_slice(&[0; 8]);
    let mirrored_indices_field = reserve(output, 8);
    put_u16(output, spec.bone_count);
    put_u16(output, spec.smooth_matrix_count);
    put_u16(output, spec.rigid_matrix_count);
    output.extend_from_slice(&[0; 2]);
    relocation.add(1, bone_dict_field as u32, 4, 1, 0)?;
    relocation.add(1, mirrored_indices_field as u32, 1, 1, 0)?;
    Ok(SkeletonHeaderLayout {
        start,
        bone_dict_field,
        bone_array_field,
        matrix_to_bone_field,
        inverse_matrices_field,
        mirrored_indices_field,
    })
}

pub(super) fn write_v10_shape_header(
    output: &mut Vec<u8>,
    spec: &ShapeHeaderSpec<'_>,
    strings: &mut StringTableBuilder,
    relocation: &mut RelocationTableBuilder,
) -> Result<ShapeHeaderLayout, BfresError> {
    let start = output.len();
    output.extend_from_slice(b"FSHP");
    put_u32(output, spec.flags);
    let name_field = reserve(output, 8);
    strings.register(spec.name, name_field);
    put_u64(output, spec.vertex_buffer_offset);
    let mesh_array_field = reserve(output, 8);
    let skin_bones_field = reserve(output, 8);
    let key_shape_array_field = reserve(output, 8);
    let key_shape_dict_field = reserve(output, 8);
    let bounds_field = reserve(output, 8);
    let radius_field = reserve(output, 8);
    output.extend_from_slice(&[0; 8]);
    put_u16(output, spec.shape_index);
    put_u16(output, spec.material_index);
    put_u16(output, spec.bone_index);
    put_u16(output, spec.vertex_buffer_index);
    put_u16(output, spec.skin_bone_count);
    output.push(spec.vertex_skin_count);
    output.push(spec.mesh_count);
    output.push(spec.key_shape_count);
    output.push(spec.target_attribute_count);
    output.extend_from_slice(&[0; 2]);
    if output.len() - start != 96 {
        return Err(error(start, "incorrect BFRES v10 shape size"));
    }
    relocation.add(1, name_field as u32, 8, 1, 0)?;
    Ok(ShapeHeaderLayout {
        start,
        mesh_array_field,
        skin_bones_field,
        key_shape_array_field,
        key_shape_dict_field,
        bounds_field,
        radius_field,
    })
}

pub(super) fn write_v10_material_header(
    output: &mut Vec<u8>,
    spec: &MaterialHeaderSpec<'_>,
    strings: &mut StringTableBuilder,
    relocation: &mut RelocationTableBuilder,
) -> Result<MaterialHeaderLayout, BfresError> {
    let start = output.len();
    output.extend_from_slice(b"FMAT");
    put_u32(output, spec.flags);
    let name_field = reserve(output, 8);
    strings.register(spec.name, name_field);
    let shader_info_field = reserve(output, 8);
    let texture_runtime_field = reserve(output, 8);
    let texture_names_field = reserve(output, 8);
    let sampler_runtime_field = reserve(output, 8);
    let sampler_array_field = reserve(output, 8);
    let sampler_dict_field = reserve(output, 8);
    let render_info_data_field = reserve(output, 8);
    let render_info_counts_field = reserve(output, 8);
    let render_info_offsets_field = reserve(output, 8);
    let shader_parameter_data_field = reserve(output, 8);
    let shader_parameter_indices_field = reserve(output, 8);
    output.extend_from_slice(&[0; 8]);
    let user_data_array_field = reserve(output, 8);
    let user_data_dict_field = reserve(output, 8);
    let volatile_flags_field = reserve(output, 8);
    output.extend_from_slice(&[0; 8]);
    let sampler_slots_field = reserve(output, 8);
    let texture_slots_field = reserve(output, 8);
    put_u16(output, spec.material_index);
    output.push(spec.texture_count);
    output.push(spec.sampler_count);
    put_u16(output, spec.volatile_shader_param_count);
    put_u16(output, spec.user_data_count);
    put_u16(output, spec.render_info_data_size);
    output.extend_from_slice(&[0; 6]);
    if output.len() - start != 176 {
        return Err(error(start, "incorrect BFRES v10 material size"));
    }
    relocation.add(1, name_field as u32, 12, 1, 0)?;
    relocation.add(1, user_data_array_field as u32, 3, 1, 0)?;
    relocation.add(1, sampler_slots_field as u32, 2, 1, 0)?;
    Ok(MaterialHeaderLayout {
        start,
        shader_info_field,
        texture_runtime_field,
        texture_names_field,
        sampler_runtime_field,
        sampler_array_field,
        sampler_dict_field,
        render_info_data_field,
        render_info_counts_field,
        render_info_offsets_field,
        shader_parameter_data_field,
        shader_parameter_indices_field,
        user_data_array_field,
        user_data_dict_field,
        volatile_flags_field,
        sampler_slots_field,
        texture_slots_field,
    })
}

pub(super) fn write_v10_vertex_buffer_header(
    output: &mut Vec<u8>,
    spec: &VertexBufferHeaderSpec,
    relocation: &mut RelocationTableBuilder,
) -> Result<VertexBufferHeaderLayout, BfresError> {
    let start = output.len();
    output.extend_from_slice(b"FVTX");
    put_u32(output, spec.flags);
    let attribute_array_field = reserve(output, 8);
    let attribute_dict_field = reserve(output, 8);
    let memory_pool_field = reserve(output, 8);
    let unknown_buffer_field = reserve(output, 8);
    let unknown_buffer2_field = reserve(output, 8);
    let buffer_size_array_field = reserve(output, 8);
    let stride_array_field = reserve(output, 8);
    output.extend_from_slice(&[0; 8]);
    put_u32(output, spec.buffer_offset);
    output.push(spec.attribute_count);
    output.push(spec.buffer_count);
    put_u16(output, spec.index);
    put_u32(output, spec.vertex_count);
    put_u16(output, spec.vertex_skin_count);
    put_u16(output, spec.gpu_alignment);
    relocation.add(1, attribute_array_field as u32, 2, 1, 0)?;
    relocation.add(4, memory_pool_field as u32, 1, 1, 0)?;
    relocation.add(1, unknown_buffer_field as u32, 4, 1, 0)?;
    Ok(VertexBufferHeaderLayout {
        start,
        attribute_array_field,
        attribute_dict_field,
        memory_pool_field,
        unknown_buffer_field,
        unknown_buffer2_field,
        buffer_size_array_field,
        stride_array_field,
    })
}

pub(super) fn write_external_file_header(
    output: &mut Vec<u8>,
    size: u64,
    relocation: &mut RelocationTableBuilder,
) -> Result<ExternalFileLayout, BfresError> {
    let start = output.len();
    let data_field = reserve(output, 8);
    put_u64(output, size);
    relocation.add(5, data_field as u32, 1, 1, 0)?;
    Ok(ExternalFileLayout { start, data_field })
}

pub(super) fn read_dictionary(
    data: &[u8],
    offset: usize,
) -> Result<Vec<DictionaryNode>, BfresError> {
    if offset == 0 {
        return Ok(Vec::new());
    }
    let count = read_u32(data, offset + 4)? as usize;
    let mut nodes = Vec::with_capacity(count + 1);
    for index in 0..=count {
        let entry = offset + 8 + index * 16;
        let key_offset = read_u64(data, entry + 8)? as usize;
        nodes.push(DictionaryNode {
            reference: read_u32(data, entry)?,
            left: read_u16(data, entry + 4)?,
            right: read_u16(data, entry + 6)?,
            key: if index == 0 || key_offset == 0 {
                String::new()
            } else {
                read_res_string(data, key_offset)?
            },
        });
    }
    Ok(nodes)
}

/// Emits the Switch `_DIC` payload. The Patricia scalars are retained from the
/// loaded graph when keys are unchanged, exactly as Toolbox does after its
/// `UpdateNodes` pass for an equivalent key sequence.
pub(super) fn write_dictionary(
    output: &mut Vec<u8>,
    nodes: &[DictionaryNode],
    strings: &mut StringTableBuilder,
    relocation: &mut RelocationTableBuilder,
) -> Result<usize, BfresError> {
    if nodes.is_empty() {
        return Err(error(output.len(), "BFRES dictionary has no root node"));
    }
    let start = output.len();
    put_u32(output, 0);
    put_u32(
        output,
        u32::try_from(nodes.len() - 1)
            .map_err(|_| error(start, "BFRES dictionary is too large"))?,
    );
    let first_key_field = start + 16;
    for node in nodes {
        put_u32(output, node.reference);
        put_u16(output, node.left);
        put_u16(output, node.right);
        let key_field = reserve(output, 8);
        strings.register(&node.key, key_field);
    }
    relocation.add(
        1,
        first_key_field as u32,
        1,
        u16::try_from(nodes.len()).map_err(|_| error(start, "too many dictionary nodes"))?,
        1,
    )?;
    Ok(start)
}

/// Writes Toolbox's BFRES v10 `FMDL` structure. The section signature is
/// intentionally part of the flags word in v9+ files and is not emitted as a
/// separate header block.
pub(super) fn write_v10_model_header(
    output: &mut Vec<u8>,
    spec: &ModelHeaderSpec<'_>,
    strings: &mut StringTableBuilder,
    relocation: &mut RelocationTableBuilder,
) -> Result<ModelHeaderLayout, BfresError> {
    let start = output.len();
    put_u32(output, spec.flags);
    output.resize(align_up(output.len(), 8), 0);
    let name_field = reserve(output, 8);
    strings.register(spec.name, name_field);
    let path_field = reserve(output, 8);
    strings.register(spec.path, path_field);
    let skeleton_field = reserve(output, 8);
    let vertex_buffer_field = reserve(output, 8);
    let shape_array_field = reserve(output, 8);
    let shape_dict_field = reserve(output, 8);
    let material_array_field = reserve(output, 8);
    let material_dict_field = reserve(output, 8);
    let shader_assign_field = reserve(output, 8);
    let user_data_array_field = reserve(output, 8);
    let user_data_dict_field = reserve(output, 8);
    output.extend_from_slice(&[0; 8]);
    put_u16(output, spec.vertex_buffer_count);
    put_u16(output, spec.shape_count);
    put_u16(output, spec.material_count);
    put_u16(output, spec.shader_assign_count);
    put_u16(output, spec.user_data_count);
    put_u16(output, 0);
    put_u32(output, 0);
    relocation.add(1, name_field as u32, 11, 1, 0)?;
    Ok(ModelHeaderLayout {
        start,
        skeleton_field,
        vertex_buffer_field,
        shape_array_field,
        shape_dict_field,
        material_array_field,
        material_dict_field,
        shader_assign_field,
        user_data_array_field,
        user_data_dict_field,
    })
}

/// Writes the fixed 0xF0-byte BFRES v10 root used by Toolbox. Pointer fields
/// are left zero and returned for the later object/string/finalization passes.
pub(super) fn write_v10_resfile_header(
    output: &mut Vec<u8>,
    spec: &ResFileHeaderSpec<'_>,
    strings: &mut StringTableBuilder,
    relocation: &mut RelocationTableBuilder,
) -> Result<ResFileHeaderLayout, BfresError> {
    if output.len() != 0 {
        return Err(error(0, "BFRES root header must start at offset zero"));
    }
    output.extend_from_slice(b"FRES    ");
    output.extend_from_slice(&spec.version);
    // BinaryData's little-endian ByteOrder marker serializes as FF FE.
    output.extend_from_slice(&[0xff, 0xfe, spec.alignment, spec.target_address_size]);
    let file_name_field = reserve(output, 4);
    put_u16(output, spec.flags);
    output.extend_from_slice(&[0; 2]); // binary header-block size
    let relocation_field = reserve(output, 4);
    let file_size_field = reserve(output, 4);

    let name_field = reserve(output, 8);
    strings.register(spec.name, name_field);
    let model_array_field = reserve(output, 8);
    let model_dict_field = reserve(output, 8);
    // v9+ added two currently-unused resource pairs.
    output.extend_from_slice(&[0; 32]);
    // Five animation array/dictionary pairs.
    output.extend_from_slice(&[0; 80]);
    let memory_pool_field = reserve(output, 8);
    let buffer_info_field = reserve(output, 8);
    // External file array, dictionary, and padding.
    output.extend_from_slice(&[0; 24]);
    // Absolute pool pointer, four bytes of padding, then its u32 size.
    let string_pool_field = reserve(output, 12);
    put_u16(output, spec.model_count);
    // Two v9+ counts and five animation counts.
    output.extend_from_slice(&[0; 4 + 10]);
    put_u16(output, spec.external_file_count);
    output.push(0);
    output.push(1);
    if output.len() != 0xf0 {
        return Err(error(output.len(), "incorrect BFRES v10 root size"));
    }

    relocation.add(1, name_field as u32, 17, 1, 0)?;
    relocation.add(1, string_pool_field as u32, 1, 1, 0)?;
    Ok(ResFileHeaderLayout {
        file_name_field,
        relocation_field,
        file_size_field,
        name_field,
        model_array_field,
        model_dict_field,
        memory_pool_field,
        buffer_info_field,
        string_pool_field,
    })
}

#[derive(Clone, Debug)]
struct StringEntry {
    value: String,
    pointer_fields: Vec<usize>,
}

/// Toolbox retains strings from the loaded `_STR` dictionary in their source
/// order, then appends newly registered strings in first-use order.
#[derive(Clone, Debug, Default)]
pub(super) struct StringTableBuilder {
    original_order: Vec<String>,
    entries: Vec<StringEntry>,
    indices: HashMap<String, usize>,
}

impl StringTableBuilder {
    pub fn with_original_order(strings: impl IntoIterator<Item = String>) -> Self {
        Self {
            original_order: strings.into_iter().collect(),
            ..Self::default()
        }
    }

    pub fn register(&mut self, value: &str, pointer_field: usize) {
        let index = if let Some(&index) = self.indices.get(value) {
            index
        } else {
            let index = self.entries.len();
            self.entries.push(StringEntry {
                value: value.to_owned(),
                pointer_fields: Vec::new(),
            });
            self.indices.insert(value.to_owned(), index);
            index
        };
        self.entries[index].pointer_fields.push(pointer_field);
    }

    pub fn write(&self, output: &mut Vec<u8>) -> Result<StringTableLayout, BfresError> {
        let start = align_up(output.len(), 4);
        output.resize(start, 0);
        output.extend_from_slice(b"_STR");
        // Header block is patched once the following relocation block exists.
        output.extend_from_slice(&[0; 12]);
        put_u32(
            output,
            u32::try_from(self.entries.len())
                .map_err(|_| error(start, "too many BFRES strings"))?,
        );
        let values_offset = output.len();

        let mut ordered = Vec::with_capacity(self.entries.len());
        for value in &self.original_order {
            if let Some(&index) = self.indices.get(value) {
                if !ordered.contains(&index) {
                    ordered.push(index);
                }
            }
        }
        for index in 0..self.entries.len() {
            if !ordered.contains(&index) {
                ordered.push(index);
            }
        }

        let mut positions = HashMap::with_capacity(ordered.len());
        for index in ordered {
            let entry = &self.entries[index];
            let position = output.len();
            let len = u16::try_from(entry.value.len())
                .map_err(|_| error(position, "BFRES string exceeds 65535 bytes"))?;
            put_u16(output, len);
            output.extend_from_slice(entry.value.as_bytes());
            output.push(0);
            output.resize(align_up(output.len(), 2), 0);
            positions.insert(entry.value.as_str(), position);
        }

        for entry in &self.entries {
            let target = *positions
                .get(entry.value.as_str())
                .ok_or_else(|| error(start, "registered BFRES string was not emitted"))?;
            for &field in &entry.pointer_fields {
                patch_u64(output, field, target as u64)?;
            }
        }
        Ok(StringTableLayout {
            start,
            values_offset,
            end: output.len(),
            positions: positions
                .into_iter()
                .map(|(value, position)| (value.to_owned(), position))
                .collect(),
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct StringTableLayout {
    pub start: usize,
    pub values_offset: usize,
    pub end: usize,
    pub positions: HashMap<String, usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RelocationEntry {
    pub position: u32,
    pub struct_count: u16,
    pub offset_count: u8,
    pub padding_count: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RelocationSection {
    pub position: u32,
    pub size: u32,
    pub entries: Vec<RelocationEntry>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RelocationTableBuilder {
    entries: [Vec<RelocationEntry>; RLT_SECTION_COUNT],
}

impl RelocationTableBuilder {
    pub fn add(
        &mut self,
        section: usize,
        position: u32,
        offset_count: u32,
        struct_count: u16,
        padding_count: u8,
    ) -> Result<(), BfresError> {
        if !(1..=RLT_SECTION_COUNT).contains(&section) {
            return Err(error(position as usize, "invalid relocation section"));
        }
        if struct_count == 0 || offset_count == 0 {
            return Err(error(
                position as usize,
                "relocation dimensions must be non-zero",
            ));
        }

        // ResFileSwitchSaver recursively splits pointer arrays because the RLT
        // stores OffsetCount in one byte.
        let mut remaining = offset_count;
        let mut next = position;
        while remaining != 0 {
            let count = remaining.min(u8::MAX as u32) as u8;
            self.entries[section - 1].push(RelocationEntry {
                position: next,
                struct_count,
                offset_count: count,
                padding_count,
            });
            remaining -= u32::from(count);
            next = next
                .checked_add(u32::from(count) * 8)
                .ok_or_else(|| error(position as usize, "relocation position overflow"))?;
        }
        Ok(())
    }

    pub fn finish(
        mut self,
        section_positions: [u32; RLT_SECTION_COUNT],
        section_sizes: [u32; RLT_SECTION_COUNT],
    ) -> [RelocationSection; RLT_SECTION_COUNT] {
        // Toolbox leaves section 1 in registration order and sorts the four
        // buffer/external sections by pointer position.
        for entries in &mut self.entries[1..] {
            entries.sort_by_key(|entry| entry.position);
        }
        std::array::from_fn(|index| RelocationSection {
            position: section_positions[index],
            size: section_sizes[index],
            entries: std::mem::take(&mut self.entries[index]),
        })
    }
}

/// Appends a Toolbox-compatible `_RLT` block and returns its aligned offset.
pub(super) fn write_relocation_table(
    output: &mut Vec<u8>,
    sections: &[RelocationSection; RLT_SECTION_COUNT],
) -> Result<usize, BfresError> {
    let offset = align_up(output.len(), RLT_ALIGNMENT);
    output.resize(offset, 0);
    output.extend_from_slice(b"_RLT");
    put_u32(output, offset as u32);
    put_u32(output, RLT_SECTION_COUNT as u32);
    put_u32(output, 0);

    let mut entry_index = 0u32;
    for section in sections {
        output.extend_from_slice(&[0; 8]);
        put_u32(output, section.position);
        put_u32(output, section.size);
        put_u32(output, entry_index);
        put_u32(
            output,
            u32::try_from(section.entries.len())
                .map_err(|_| error(offset, "too many relocation entries"))?,
        );
        entry_index = entry_index
            .checked_add(section.entries.len() as u32)
            .ok_or_else(|| error(offset, "relocation entry index overflow"))?;
    }
    for section in sections {
        for entry in &section.entries {
            put_u32(output, entry.position);
            put_u16(output, entry.struct_count);
            output.push(entry.offset_count);
            output.push(entry.padding_count);
        }
    }
    Ok(offset)
}

fn put_u16(output: &mut Vec<u8>, value: u16) {
    let len = output.len();
    let mut writer = BinaryWriter::from_vec(std::mem::take(output), BinaryEndian::Little);
    writer.seek(len);
    writer.write_u16(value);
    *output = writer.into_inner();
}

fn put_i16(output: &mut Vec<u8>, value: i16) {
    let len = output.len();
    let mut writer = BinaryWriter::from_vec(std::mem::take(output), BinaryEndian::Little);
    writer.seek(len);
    writer.write_i16(value);
    *output = writer.into_inner();
}

fn put_f32(output: &mut Vec<u8>, value: f32) {
    let len = output.len();
    let mut writer = BinaryWriter::from_vec(std::mem::take(output), BinaryEndian::Little);
    writer.seek(len);
    writer.write_f32(value);
    *output = writer.into_inner();
}

fn reserve(output: &mut Vec<u8>, size: usize) -> usize {
    let offset = output.len();
    output.resize(offset + size, 0);
    offset
}

fn put_u32(output: &mut Vec<u8>, value: u32) {
    let len = output.len();
    let mut writer = BinaryWriter::from_vec(std::mem::take(output), BinaryEndian::Little);
    writer.seek(len);
    writer.write_u32(value);
    *output = writer.into_inner();
}

fn put_u64(output: &mut Vec<u8>, value: u64) {
    let len = output.len();
    let mut writer = BinaryWriter::from_vec(std::mem::take(output), BinaryEndian::Little);
    writer.seek(len);
    writer.write_u64(value);
    *output = writer.into_inner();
}

fn put_u16_be(output: &mut Vec<u8>, value: u16) {
    let len = output.len();
    let mut writer = BinaryWriter::from_vec(std::mem::take(output), BinaryEndian::Big);
    writer.seek(len);
    writer.write_u16(value);
    *output = writer.into_inner();
}

fn patch_u16_at(output: &mut [u8], offset: usize, value: u16) -> Result<(), BfresError> {
    if offset.checked_add(2).is_none_or(|end| end > output.len()) {
        return Err(error(offset, "BFRES u16 field lies outside output"));
    }
    let mut writer = BinaryWriter::from_vec(output.to_vec(), BinaryEndian::Little);
    writer.write_u16_at(offset, value);
    output.copy_from_slice(&writer.into_inner());
    Ok(())
}

fn patch_u64(output: &mut [u8], offset: usize, value: u64) -> Result<(), BfresError> {
    if offset.checked_add(8).is_none_or(|end| end > output.len()) {
        return Err(error(offset, "BFRES pointer field lies outside output"));
    }
    let mut writer = BinaryWriter::from_vec(output.to_vec(), BinaryEndian::Little);
    writer.write_u64_at(offset, value);
    output.copy_from_slice(&writer.into_inner());
    Ok(())
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, BfresError> {
    BinaryReader::new(data)
        .read_u16_at(offset)
        .map_err(|_| error(offset, "truncated BFRES u16"))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, BfresError> {
    BinaryReader::new(data)
        .read_u32_at(offset)
        .map_err(|_| error(offset, "truncated BFRES u32"))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, BfresError> {
    BinaryReader::new(data)
        .read_u64_at(offset)
        .map_err(|_| error(offset, "truncated BFRES u64"))
}

fn read_i16(data: &[u8], offset: usize) -> Result<i16, BfresError> {
    let mut reader = BinaryReader::new(data);
    reader
        .seek(offset)
        .map_err(|_| error(offset, "truncated BFRES i16"))?;
    reader
        .read_i16()
        .map_err(|_| error(offset, "truncated BFRES i16"))
}

fn read_f32(data: &[u8], offset: usize) -> Result<f32, BfresError> {
    let mut reader = BinaryReader::new(data);
    reader
        .seek(offset)
        .map_err(|_| error(offset, "truncated BFRES f32"))?;
    reader
        .read_f32()
        .map_err(|_| error(offset, "truncated BFRES f32"))
}

fn read_res_string(data: &[u8], offset: usize) -> Result<String, BfresError> {
    let len = read_u16(data, offset)? as usize;
    let bytes = data
        .get(offset + 2..offset + 2 + len)
        .ok_or_else(|| error(offset, "truncated BFRES ResString"))?;
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| error(offset, "BFRES ResString is not UTF-8"))
}

fn error(offset: usize, message: impl Into<String>) -> BfresError {
    BfresError::new(offset, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_large_pointer_arrays_like_toolbox() {
        let mut builder = RelocationTableBuilder::default();
        builder.add(1, 0x100, 300, 1, 0).unwrap();
        let sections = builder.finish([0; 5], [0; 5]);
        assert_eq!(sections[0].entries.len(), 2);
        assert_eq!(sections[0].entries[0].offset_count, 255);
        assert_eq!(sections[0].entries[1].position, 0x100 + 255 * 8);
        assert_eq!(sections[0].entries[1].offset_count, 45);
    }

    #[test]
    fn writes_switch_rlt_header_sections_and_entries() {
        let mut builder = RelocationTableBuilder::default();
        builder.add(1, 0x20, 2, 1, 3).unwrap();
        builder.add(3, 0x900, 1, 4, 0).unwrap();
        let sections = builder.finish([0, 0x800, 0x900, 0xa00, 0xa00], [0x800, 0x100, 0x100, 0, 0]);
        let mut output = vec![0xaa; 17];
        let offset = write_relocation_table(&mut output, &sections).unwrap();
        assert_eq!(offset, 0x100);
        assert_eq!(&output[offset..offset + 4], b"_RLT");
        assert_eq!(
            BinaryReader::new(&output).read_u32_at(offset + 4).unwrap(),
            0x100
        );
        assert_eq!(
            BinaryReader::new(&output).read_u32_at(offset + 8).unwrap(),
            5
        );
        assert_eq!(
            BinaryReader::new(&output).read_u32_at(offset + 36).unwrap(),
            1
        );
    }

    #[test]
    fn writes_strings_in_original_then_first_use_order_and_patches_pointers() {
        let mut strings = StringTableBuilder::with_original_order([
            "retained_b".to_owned(),
            "retained_a".to_owned(),
        ]);
        let mut output = vec![0; 24];
        strings.register("new", 0);
        strings.register("retained_a", 8);
        strings.register("retained_b", 16);
        let layout = strings.write(&mut output).unwrap();
        assert_eq!(layout.start, 24);
        assert_eq!(&output[layout.start..layout.start + 4], b"_STR");
        assert!(layout.positions["retained_b"] < layout.positions["retained_a"]);
        assert!(layout.positions["retained_a"] < layout.positions["new"]);
        assert_eq!(
            BinaryReader::new(&output).read_u64_at(0).unwrap() as usize,
            layout.positions["new"]
        );
        assert_eq!(layout.values_offset, layout.start + 20);
        assert_eq!(layout.end % 2, 0);
    }

    #[test]
    fn writes_the_toolbox_v10_root_shape() {
        let mut output = Vec::new();
        let mut strings = StringTableBuilder::default();
        let mut relocation = RelocationTableBuilder::default();
        let layout = write_v10_resfile_header(
            &mut output,
            &ResFileHeaderSpec {
                version: [0, 0, 10, 0],
                alignment: 3,
                target_address_size: 0,
                name: "Weapon_Lsword_005",
                flags: 0,
                model_count: 1,
                external_file_count: 0,
            },
            &mut strings,
            &mut relocation,
        )
        .unwrap();
        assert_eq!(output.len(), 0xf0);
        assert_eq!(&output[..8], b"FRES    ");
        assert_eq!(layout.file_name_field, 0x10);
        assert_eq!(layout.relocation_field, 0x18);
        assert_eq!(layout.file_size_field, 0x1c);
        assert_eq!(layout.name_field, 0x20);
        assert_eq!(layout.model_array_field, 0x28);
        assert_eq!(layout.model_dict_field, 0x30);
        assert_eq!(layout.memory_pool_field, 0xa8);
        assert_eq!(layout.buffer_info_field, 0xb0);
        assert_eq!(layout.string_pool_field, 0xd0);
        assert_eq!(BinaryReader::new(&output).read_u16_at(0xdc).unwrap(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn v10_root_scalars_match_the_working_toolbox_weapon() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let mut output = Vec::new();
        let mut strings = StringTableBuilder::default();
        let mut relocation = RelocationTableBuilder::default();
        write_v10_resfile_header(
            &mut output,
            &ResFileHeaderSpec {
                version: raw[8..12].try_into().unwrap(),
                alignment: raw[14],
                target_address_size: raw[15],
                name: "Weapon_Lsword_005",
                flags: BinaryReader::new(&raw).read_u16_at(20).unwrap(),
                model_count: 1,
                external_file_count: BinaryReader::new(&raw).read_u16_at(236).unwrap(),
            },
            &mut strings,
            &mut relocation,
        )
        .unwrap();
        for range in [0..16, 20..22, 220..240] {
            assert_eq!(&output[range.clone()], &raw[range]);
        }
    }

    #[cfg(windows)]
    #[test]
    fn v10_model_scalars_match_the_working_toolbox_weapon() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let source = 0xf0;
        let mut output = Vec::new();
        let mut strings = StringTableBuilder::default();
        let mut relocation = RelocationTableBuilder::default();
        let layout = write_v10_model_header(
            &mut output,
            &ModelHeaderSpec {
                flags: BinaryReader::new(&raw).read_u32_at(source).unwrap(),
                name: "Weapon_Lsword_005",
                path: "",
                vertex_buffer_count: BinaryReader::new(&raw).read_u16_at(source + 104).unwrap(),
                shape_count: BinaryReader::new(&raw).read_u16_at(source + 106).unwrap(),
                material_count: BinaryReader::new(&raw).read_u16_at(source + 108).unwrap(),
                shader_assign_count: BinaryReader::new(&raw).read_u16_at(source + 110).unwrap(),
                user_data_count: BinaryReader::new(&raw).read_u16_at(source + 112).unwrap(),
            },
            &mut strings,
            &mut relocation,
        )
        .unwrap();
        assert_eq!(layout.start, 0);
        assert_eq!(output.len(), 120);
        assert_eq!(&output[..4], &raw[source..source + 4]);
        assert_eq!(&output[96..], &raw[source + 96..source + 120]);
    }

    #[cfg(windows)]
    #[test]
    fn dictionaries_roundtrip_toolbox_patricia_scalars() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        for source in [408usize, 448] {
            let nodes = read_dictionary(&raw, source).unwrap();
            let mut output = Vec::new();
            let mut strings = StringTableBuilder::default();
            let mut relocation = RelocationTableBuilder::default();
            write_dictionary(&mut output, &nodes, &mut strings, &mut relocation).unwrap();
            assert_eq!(&output[..8], &raw[source..source + 8]);
            for index in 0..nodes.len() {
                let target = 8 + index * 16;
                let expected = source + target;
                assert_eq!(&output[target..target + 8], &raw[expected..expected + 8]);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn buffer_and_external_headers_match_the_working_toolbox_weapon() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let mut output = Vec::new();
        let mut relocation = RelocationTableBuilder::default();
        let buffer = write_buffer_info(
            &mut output,
            read_u32(&raw, 360).unwrap(),
            read_u32(&raw, 364).unwrap(),
            &mut relocation,
        )
        .unwrap();
        let external =
            write_external_file_header(&mut output, read_u64(&raw, 400).unwrap(), &mut relocation)
                .unwrap();
        assert_eq!(buffer.start, 0);
        assert_eq!(external.start, 32);
        assert_eq!(&output[..8], &raw[360..368]);
        assert_eq!(&output[16..32], &raw[376..392]);
        assert_eq!(&output[40..48], &raw[400..408]);
    }

    #[cfg(windows)]
    #[test]
    fn vertex_buffer_headers_match_the_working_toolbox_weapon() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        for source in [488usize, 576] {
            let mut output = Vec::new();
            let mut relocation = RelocationTableBuilder::default();
            write_v10_vertex_buffer_header(
                &mut output,
                &VertexBufferHeaderSpec {
                    flags: read_u32(&raw, source + 4).unwrap(),
                    buffer_offset: read_u32(&raw, source + 72).unwrap(),
                    attribute_count: raw[source + 76],
                    buffer_count: raw[source + 77],
                    index: read_u16(&raw, source + 78).unwrap(),
                    vertex_count: read_u32(&raw, source + 80).unwrap(),
                    vertex_skin_count: read_u16(&raw, source + 84).unwrap(),
                    gpu_alignment: read_u16(&raw, source + 86).unwrap(),
                },
                &mut relocation,
            )
            .unwrap();
            assert_eq!(output.len(), 88);
            assert_eq!(&output[..8], &raw[source..source + 8]);
            assert_eq!(&output[64..88], &raw[source + 64..source + 88]);
        }
    }

    #[cfg(windows)]
    #[test]
    fn material_headers_match_the_working_toolbox_weapon() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        for source in [664usize, 840] {
            let mut output = Vec::new();
            let mut strings = StringTableBuilder::default();
            let mut relocation = RelocationTableBuilder::default();
            write_v10_material_header(
                &mut output,
                &MaterialHeaderSpec {
                    flags: read_u32(&raw, source + 4).unwrap(),
                    name: if source == 664 {
                        "Mt_Blade_Hide"
                    } else {
                        "Mt_Lsword_108"
                    },
                    material_index: read_u16(&raw, source + 160).unwrap(),
                    texture_count: raw[source + 162],
                    sampler_count: raw[source + 163],
                    volatile_shader_param_count: read_u16(&raw, source + 164).unwrap(),
                    user_data_count: read_u16(&raw, source + 166).unwrap(),
                    render_info_data_size: read_u16(&raw, source + 168).unwrap(),
                },
                &mut strings,
                &mut relocation,
            )
            .unwrap();
            assert_eq!(&output[..8], &raw[source..source + 8]);
            assert_eq!(&output[104..112], &raw[source + 104..source + 112]);
            assert_eq!(&output[136..144], &raw[source + 136..source + 144]);
            assert_eq!(&output[160..176], &raw[source + 160..source + 176]);
        }
    }

    #[cfg(windows)]
    #[test]
    fn shape_headers_match_the_working_toolbox_weapon() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        for (source, name) in [
            (1016usize, "Weapon_Lsword_108__Mt_Lsword_108"),
            (1112, "Weapon_Lsword_108_Blade__Mt_Blade_Hide"),
        ] {
            let mut output = Vec::new();
            let mut strings = StringTableBuilder::default();
            let mut relocation = RelocationTableBuilder::default();
            write_v10_shape_header(
                &mut output,
                &ShapeHeaderSpec {
                    flags: read_u32(&raw, source + 4).unwrap(),
                    name,
                    vertex_buffer_offset: read_u64(&raw, source + 16).unwrap(),
                    shape_index: read_u16(&raw, source + 80).unwrap(),
                    material_index: read_u16(&raw, source + 82).unwrap(),
                    bone_index: read_u16(&raw, source + 84).unwrap(),
                    vertex_buffer_index: read_u16(&raw, source + 86).unwrap(),
                    skin_bone_count: read_u16(&raw, source + 88).unwrap(),
                    vertex_skin_count: raw[source + 90],
                    mesh_count: raw[source + 91],
                    key_shape_count: raw[source + 92],
                    target_attribute_count: raw[source + 93],
                },
                &mut strings,
                &mut relocation,
            )
            .unwrap();
            assert_eq!(&output[..8], &raw[source..source + 8]);
            assert_eq!(&output[16..24], &raw[source + 16..source + 24]);
            assert_eq!(&output[72..96], &raw[source + 72..source + 96]);
            assert_eq!(output[91], 1, "Toolbox replacement must emit one LOD");
            assert_eq!(output[90], 1, "weapon shapes must retain rigid skinning");
        }
    }

    #[cfg(windows)]
    #[test]
    fn skeleton_header_matches_the_working_toolbox_weapon() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let source = 1208usize;
        let mut output = Vec::new();
        let mut relocation = RelocationTableBuilder::default();
        write_v10_skeleton_header(
            &mut output,
            &SkeletonHeaderSpec {
                flags: read_u32(&raw, source + 4).unwrap(),
                bone_count: read_u16(&raw, source + 56).unwrap(),
                smooth_matrix_count: read_u16(&raw, source + 58).unwrap(),
                rigid_matrix_count: read_u16(&raw, source + 60).unwrap(),
            },
            &mut relocation,
        )
        .unwrap();
        assert_eq!(output.len(), 64);
        assert_eq!(&output[..8], &raw[source..source + 8]);
        assert_eq!(&output[40..48], &raw[source + 40..source + 48]);
        assert_eq!(&output[56..64], &raw[source + 56..source + 64]);
    }

    #[cfg(windows)]
    #[test]
    fn one_lod_mesh_headers_and_children_match_toolbox() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        for shape in [1016usize, 1112] {
            assert_eq!(raw[shape + 91], 1);
            let source = read_u64(&raw, shape + 24).unwrap() as usize;
            let mut output = Vec::new();
            let mut relocation = RelocationTableBuilder::default();
            write_mesh_header(
                &mut output,
                &MeshHeaderSpec {
                    memory_pool_offset: read_u64(&raw, source + 8).unwrap(),
                    face_buffer_offset: read_u32(&raw, source + 32).unwrap(),
                    primitive_type: read_u32(&raw, source + 36).unwrap(),
                    index_format: read_u32(&raw, source + 40).unwrap(),
                    index_count: read_u32(&raw, source + 44).unwrap(),
                    first_vertex: read_u32(&raw, source + 48).unwrap(),
                    submesh_count: read_u16(&raw, source + 52).unwrap(),
                },
                &mut relocation,
            )
            .unwrap();
            assert_eq!(&output[8..16], &raw[source + 8..source + 16]);
            assert_eq!(&output[32..56], &raw[source + 32..source + 56]);

            let submesh = read_u64(&raw, source).unwrap() as usize;
            let child = write_submesh(
                &mut output,
                read_u32(&raw, submesh).unwrap(),
                read_u32(&raw, submesh + 4).unwrap(),
            );
            assert_eq!(&output[child..child + 8], &raw[submesh..submesh + 8]);

            let buffer_size = read_u64(&raw, source + 24).unwrap() as usize;
            let size = write_buffer_size(
                &mut output,
                read_u32(&raw, buffer_size).unwrap(),
                read_u32(&raw, buffer_size + 4).unwrap(),
            );
            assert_eq!(&output[size..size + 8], &raw[buffer_size..buffer_size + 8]);
        }
    }

    #[cfg(windows)]
    #[test]
    fn bone_array_matches_the_working_toolbox_weapon() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let skeleton = 1208usize;
        let count = read_u16(&raw, skeleton + 56).unwrap() as usize;
        let source = read_u64(&raw, skeleton + 16).unwrap() as usize;
        let mut output = Vec::new();
        let mut strings = StringTableBuilder::default();
        let mut relocation = RelocationTableBuilder::default();
        register_v10_bone_array(&mut relocation, 0, count).unwrap();
        for index in 0..count {
            let bone = source + index * 88;
            let name_offset = read_u64(&raw, bone).unwrap() as usize;
            let name = read_res_string(&raw, name_offset).unwrap();
            let start = output.len();
            write_v10_bone_header(
                &mut output,
                &BoneHeaderSpec {
                    name: &name,
                    index: read_u16(&raw, bone + 32).unwrap(),
                    parent_index: read_i16(&raw, bone + 34).unwrap(),
                    smooth_matrix_index: read_i16(&raw, bone + 36).unwrap(),
                    rigid_matrix_index: read_i16(&raw, bone + 38).unwrap(),
                    billboard_index: read_i16(&raw, bone + 40).unwrap(),
                    user_data_count: read_u16(&raw, bone + 42).unwrap(),
                    flags: read_u32(&raw, bone + 44).unwrap(),
                    scale: std::array::from_fn(|slot| {
                        read_f32(&raw, bone + 48 + slot * 4).unwrap()
                    }),
                    rotation: std::array::from_fn(|slot| {
                        read_f32(&raw, bone + 60 + slot * 4).unwrap()
                    }),
                    translation: std::array::from_fn(|slot| {
                        read_f32(&raw, bone + 76 + slot * 4).unwrap()
                    }),
                },
                &mut strings,
            );
            assert_eq!(output.len() - start, 88);
            assert_eq!(&output[start + 24..start + 88], &raw[bone + 24..bone + 88]);
        }
    }

    #[cfg(windows)]
    #[test]
    fn vertex_attribute_and_buffer_records_match_toolbox() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        for vertex in [488usize, 576] {
            let count = raw[vertex + 76] as usize;
            let attributes = read_u64(&raw, vertex + 8).unwrap() as usize;
            let mut output = Vec::new();
            let mut strings = StringTableBuilder::default();
            let mut relocation = RelocationTableBuilder::default();
            register_vertex_attribute_array(&mut relocation, 0, count).unwrap();
            for index in 0..count {
                let source = attributes + index * 16;
                let name = read_res_string(&raw, read_u64(&raw, source).unwrap() as usize).unwrap();
                let start = write_vertex_attribute(
                    &mut output,
                    &VertexAttributeSpec {
                        name: &name,
                        format: BinaryReader::with_endian(&raw, BinaryEndian::Big)
                            .read_u16_at(source + 8)
                            .unwrap(),
                        offset: read_u16(&raw, source + 12).unwrap(),
                        buffer_index: raw[source + 14],
                    },
                    &mut strings,
                );
                assert_eq!(
                    &output[start + 8..start + 16],
                    &raw[source + 8..source + 16]
                );
            }

            let sizes = read_u64(&raw, vertex + 48).unwrap() as usize;
            let strides = read_u64(&raw, vertex + 56).unwrap() as usize;
            let buffers = raw[vertex + 77] as usize;
            for index in 0..buffers {
                let size_source = sizes + index * 16;
                let size = write_vertex_buffer_size(
                    &mut output,
                    read_u32(&raw, size_source).unwrap(),
                    read_u32(&raw, size_source + 4).unwrap(),
                );
                assert_eq!(
                    &output[size..size + 16],
                    &raw[size_source..size_source + 16]
                );
                let stride_source = strides + index * 16;
                let stride =
                    write_vertex_buffer_stride(&mut output, read_u32(&raw, stride_source).unwrap());
                assert_eq!(
                    &output[stride..stride + 16],
                    &raw[stride_source..stride_source + 16]
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn shader_info_and_deduplicated_assignment_match_toolbox() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let mut assignments = std::collections::BTreeSet::new();
        for material in [664usize, 840] {
            let source = read_u64(&raw, material + 16).unwrap() as usize;
            let mut output = Vec::new();
            let mut relocation = RelocationTableBuilder::default();
            write_shader_info(
                &mut output,
                &ShaderInfoSpec {
                    attribute_assign_count: raw[source + 68],
                    sampler_assign_count: raw[source + 69],
                    boolean_option_count: read_u16(&raw, source + 70).unwrap(),
                    option_count: read_u16(&raw, source + 72).unwrap(),
                },
                &mut relocation,
            )
            .unwrap();
            assert_eq!(output.len(), 80);
            assert_eq!(&output[64..80], &raw[source + 64..source + 80]);
            assignments.insert(read_u64(&raw, source).unwrap() as usize);
        }

        // Both materials in the supplied weapon share one assignment instance,
        // matching Toolbox's hash-based deduplication pass.
        assert_eq!(assignments.len(), 1);
        let source = *assignments.first().unwrap();
        let archive = read_res_string(&raw, read_u64(&raw, source).unwrap() as usize).unwrap();
        let model = read_res_string(&raw, read_u64(&raw, source + 8).unwrap() as usize).unwrap();
        let mut output = Vec::new();
        let mut strings = StringTableBuilder::default();
        let mut relocation = RelocationTableBuilder::default();
        write_shader_assign(
            &mut output,
            &ShaderAssignSpec {
                shader_archive_name: &archive,
                shading_model_name: &model,
                render_info_count: read_u16(&raw, source + 72).unwrap(),
                shader_parameter_count: read_u16(&raw, source + 74).unwrap(),
                shader_parameter_data_size: read_u16(&raw, source + 76).unwrap(),
            },
            &mut strings,
            &mut relocation,
        )
        .unwrap();
        assert_eq!(output.len(), 88);
        assert_eq!(&output[72..88], &raw[source + 72..source + 88]);
    }

    #[cfg(windows)]
    #[test]
    fn material_metadata_and_shape_bounds_match_toolbox() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let shader_info = read_u64(&raw, 664 + 16).unwrap() as usize;
        let assignment = read_u64(&raw, shader_info).unwrap() as usize;
        let render_count = read_u16(&raw, assignment + 72).unwrap() as usize;
        let render_records = read_u64(&raw, assignment + 16).unwrap() as usize;
        let parameter_count = read_u16(&raw, assignment + 74).unwrap() as usize;
        let parameter_records = read_u64(&raw, assignment + 32).unwrap() as usize;
        let mut output = Vec::new();
        let mut strings = StringTableBuilder::default();
        for index in 0..render_count {
            let source = render_records + index * 16;
            let name = read_res_string(&raw, read_u64(&raw, source).unwrap() as usize).unwrap();
            let start = write_render_info_record(
                &mut output,
                &RenderInfoRecordSpec {
                    name: &name,
                    value_type: raw[source + 8],
                },
                &mut strings,
            );
            assert_eq!(
                &output[start + 8..start + 16],
                &raw[source + 8..source + 16]
            );
        }
        for index in 0..parameter_count {
            let source = parameter_records + index * 24;
            let name = read_res_string(&raw, read_u64(&raw, source + 8).unwrap() as usize).unwrap();
            let start = write_shader_parameter_record(
                &mut output,
                &name,
                read_u16(&raw, source + 16).unwrap(),
                read_u16(&raw, source + 18).unwrap(),
                &mut strings,
            );
            assert_eq!(&output[start..start + 8], &raw[source..source + 8]);
            assert_eq!(
                &output[start + 16..start + 24],
                &raw[source + 16..source + 24]
            );
        }

        for shape in [1016usize, 1112] {
            let bounds = read_u64(&raw, shape + 56).unwrap() as usize;
            let radius = read_u64(&raw, shape + 64).unwrap() as usize;
            let bound_start = write_bounding(
                &mut output,
                std::array::from_fn(|i| read_f32(&raw, bounds + i * 4).unwrap()),
                std::array::from_fn(|i| read_f32(&raw, bounds + 12 + i * 4).unwrap()),
            );
            assert_eq!(
                &output[bound_start..bound_start + 24],
                &raw[bounds..bounds + 24]
            );
            let radius_start = write_bounding_radius(
                &mut output,
                std::array::from_fn(|i| read_f32(&raw, radius + i * 4).unwrap()),
            );
            assert_eq!(
                &output[radius_start..radius_start + 16],
                &raw[radius..radius + 16]
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn sampler_and_texture_name_arrays_match_toolbox() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        for material in [664usize, 840] {
            let mut output = Vec::new();
            let sampler_count = raw[material + 163] as usize;
            let sampler_array = read_u64(&raw, material + 48).unwrap() as usize;
            for index in 0..sampler_count {
                let source = sampler_array + index * 32;
                let start = write_sampler_switch(
                    &mut output,
                    &SamplerSwitchSpec {
                        wrap_u: raw[source],
                        wrap_v: raw[source + 1],
                        wrap_w: raw[source + 2],
                        compare_function: raw[source + 3],
                        border_color_type: raw[source + 4],
                        anisotropic: raw[source + 5],
                        filter_flags: read_u16(&raw, source + 6).unwrap(),
                        min_lod: read_f32(&raw, source + 8).unwrap(),
                        max_lod: read_f32(&raw, source + 12).unwrap(),
                        lod_bias: read_f32(&raw, source + 16).unwrap(),
                    },
                );
                assert_eq!(&output[start..start + 32], &raw[source..source + 32]);
            }

            let texture_count = raw[material + 162] as usize;
            let names = read_u64(&raw, material + 32).unwrap() as usize;
            let values: Vec<String> = (0..texture_count)
                .map(|index| {
                    read_res_string(&raw, read_u64(&raw, names + index * 8).unwrap() as usize)
                        .unwrap()
                })
                .collect();
            let mut pointers = Vec::new();
            let mut strings = StringTableBuilder::default();
            let mut relocation = RelocationTableBuilder::default();
            write_string_pointer_array(&mut pointers, &values, &mut strings, &mut relocation, 1)
                .unwrap();
            assert_eq!(pointers.len(), texture_count * 8);
            assert_eq!(
                strings
                    .entries
                    .iter()
                    .map(|entry| entry.pointer_fields.len())
                    .sum::<usize>(),
                texture_count
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn parses_the_working_toolbox_resource_graph() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let graph = V10ResourceGraph::parse(&raw).unwrap();
        assert_eq!(graph.models.len(), 1);
        assert_eq!(graph.models[0].header, 0xf0);
        assert_eq!(
            graph.models[0]
                .vertex_buffers
                .iter()
                .map(|vertex| vertex.header)
                .collect::<Vec<_>>(),
            vec![488, 576]
        );
        assert_eq!(
            graph.models[0]
                .materials
                .iter()
                .map(|material| material.header)
                .collect::<Vec<_>>(),
            vec![664, 840]
        );
        assert!(graph.models[0].materials.iter().all(|material| {
            material.shader_info.header != 0
                && material.shader_info.assignment.header != 0
                && !material
                    .shader_info
                    .assignment
                    .render_info_records
                    .is_empty()
                && material.shader_info.assignment.render_info_dict != 0
                && !material
                    .shader_info
                    .assignment
                    .shader_parameter_records
                    .is_empty()
                && material.shader_info.assignment.shader_parameter_dict != 0
                && material.shader_info.assignment.attribute_dict != 0
                && material.shader_info.assignment.sampler_dict != 0
                && material.shader_info.assignment.option_dict != 0
                && material.shader_info.option_bitflags.is_some()
                && !material.texture_names.is_empty()
                && !material.samplers.is_empty()
                && material.render_info_data.is_some()
                && material.render_info_counts.is_some()
                && material.render_info_offsets.is_some()
                && material.shader_parameter_data.is_some()
                && material.shader_parameter_indices.is_some()
                && material.sampler_slots.is_some()
                && material.texture_slots.is_some()
        }));
        assert_eq!(
            graph.models[0].materials[0].shader_info.assignment.header,
            graph.models[0].materials[1].shader_info.assignment.header
        );
        assert_eq!(
            graph.models[0]
                .shapes
                .iter()
                .map(|shape| shape.header)
                .collect::<Vec<_>>(),
            vec![1016, 1112]
        );
        assert_eq!(graph.models[0].skeleton.header, 1208);
        assert!(graph.models[0]
            .vertex_buffers
            .iter()
            .all(|vertex| !vertex.attributes.is_empty()
                && !vertex.buffer_sizes.is_empty()
                && vertex.buffer_sizes.len() == vertex.buffer_strides.len()));
        assert!(graph.models[0]
            .shapes
            .iter()
            .all(|shape| shape.meshes.len() == 1
                && shape.skin_bones
                    == Some((read_u64(&raw, shape.header + 32).unwrap() as usize, 1))
                && !shape.bounds.is_empty()
                && shape.radii.len() == 1));
        assert!(!graph.models[0].skeleton.bones.is_empty());
        assert!(graph.models[0].skeleton.matrix_to_bone.is_some());
        assert!(graph.models[0].skeleton.inverse_matrices.is_empty());
        assert!(graph.models[0].skeleton.mirrored_bones.is_none());
        assert_eq!(graph.buffer_info, 360);
        assert_eq!(graph.external_files, vec![392]);
    }

    #[cfg(windows)]
    #[test]
    fn skin_matrix_and_mesh_runtime_arrays_match_toolbox() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let graph = V10ResourceGraph::parse(&raw).unwrap();
        let model = &graph.models[0];
        let mut output = Vec::new();
        for shape in &model.shapes {
            let (source, count) = shape.skin_bones.unwrap();
            let values: Vec<u16> = (0..count)
                .map(|index| read_u16(&raw, source + index * 2).unwrap())
                .collect();
            let start = write_u16_array(&mut output, &values);
            assert_eq!(
                &output[start..start + count * 2],
                &raw[source..source + count * 2]
            );
            for mesh in &shape.meshes {
                let runtime = read_u64(&raw, mesh + 16).unwrap() as usize;
                let start = write_mesh_runtime_buffer(&mut output);
                assert_eq!(&output[start..start + 72], &raw[runtime..runtime + 72]);
            }
        }
        if let Some((source, count)) = model.skeleton.matrix_to_bone {
            let values: Vec<u16> = (0..count)
                .map(|index| read_u16(&raw, source + index * 2).unwrap())
                .collect();
            let start = write_u16_array(&mut output, &values);
            assert_eq!(
                &output[start..start + count * 2],
                &raw[source..source + count * 2]
            );
        }
        for source in &model.skeleton.inverse_matrices {
            let values = std::array::from_fn(|index| read_f32(&raw, source + index * 4).unwrap());
            let start = write_matrix_3x4(&mut output, values);
            assert_eq!(&output[start..start + 48], &raw[*source..*source + 48]);
        }
    }

    #[cfg(windows)]
    #[test]
    fn canonical_fixed_phase_matches_toolbox_prefix() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let graph = V10ResourceGraph::parse(&raw).unwrap();
        let phase = emit_v10_fixed_phase(&raw, &graph).unwrap();
        assert_eq!(phase.bytes.len(), 1272);
        assert_eq!(
            phase
                .bytes
                .iter()
                .zip(&raw)
                .position(|(left, right)| left != right),
            None,
            "canonical model-core prefix differs; emitted length {}",
            phase.bytes.len()
        );
        assert_eq!(phase.source_to_output.get(&0xf0), Some(&0xf0));
        assert_eq!(phase.source_to_output.get(&1208), Some(&1208));
    }

    #[cfg(windows)]
    #[test]
    fn canonical_model_core_phase_matches_toolbox_prefix() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let graph = V10ResourceGraph::parse(&raw).unwrap();
        let mut phase = emit_v10_fixed_phase(&raw, &graph).unwrap();
        emit_v10_model_core_phase(&raw, &graph, &mut phase).unwrap();
        assert!(phase.bytes.len() > 1800);
        assert_eq!(
            phase
                .bytes
                .iter()
                .zip(&raw)
                .position(|(left, right)| left != right),
            None,
            "canonical model-core prefix differs; emitted length {}",
            phase.bytes.len()
        );
        for source in &graph.models[0].skeleton.bones {
            assert_eq!(phase.source_to_output.get(source), Some(source));
        }
    }

    #[cfg(windows)]
    #[test]
    fn canonical_shape_phase_matches_toolbox_prefix() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let graph = V10ResourceGraph::parse(&raw).unwrap();
        let mut phase = emit_v10_fixed_phase(&raw, &graph).unwrap();
        emit_v10_model_core_phase(&raw, &graph, &mut phase).unwrap();
        emit_v10_shape_phase(&raw, &graph, &mut phase).unwrap();
        assert!(phase.bytes.len() > 2400);
        assert_eq!(
            phase
                .bytes
                .iter()
                .zip(&raw)
                .position(|(left, right)| left != right),
            None,
            "canonical shape prefix differs; emitted length {}",
            phase.bytes.len()
        );
    }

    #[cfg(windows)]
    #[test]
    fn canonical_vertex_phase_matches_toolbox_prefix() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let graph = V10ResourceGraph::parse(&raw).unwrap();
        let mut phase = emit_v10_fixed_phase(&raw, &graph).unwrap();
        emit_v10_model_core_phase(&raw, &graph, &mut phase).unwrap();
        emit_v10_shape_phase(&raw, &graph, &mut phase).unwrap();
        emit_v10_vertex_phase(&raw, &graph, &mut phase).unwrap();
        assert!(phase.bytes.len() > 4700);
        assert_eq!(
            phase
                .bytes
                .iter()
                .zip(&raw)
                .position(|(left, right)| left != right),
            None,
            "canonical vertex prefix differs; emitted length {}",
            phase.bytes.len()
        );
    }

    #[cfg(windows)]
    #[test]
    fn material_initial_queue_matches_toolbox_prefix() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let graph = V10ResourceGraph::parse(&raw).unwrap();
        let mut phase = emit_v10_fixed_phase(&raw, &graph).unwrap();
        emit_v10_model_core_phase(&raw, &graph, &mut phase).unwrap();
        emit_v10_shape_phase(&raw, &graph, &mut phase).unwrap();
        emit_v10_vertex_phase(&raw, &graph, &mut phase).unwrap();
        emit_v10_material_initial_queue_phase(&raw, &graph.models[0], &mut phase).unwrap();
        assert!(phase.bytes.len() > 8000);
        assert_eq!(
            phase
                .bytes
                .iter()
                .zip(&raw)
                .position(|(left, right)| left != right),
            None,
            "material initial queue differs; emitted length {}",
            phase.bytes.len(),
        );
    }

    #[cfg(windows)]
    #[test]
    fn shader_assignment_children_match_toolbox_prefix() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let graph = V10ResourceGraph::parse(&raw).unwrap();
        let mut phase = emit_v10_fixed_phase(&raw, &graph).unwrap();
        emit_v10_model_core_phase(&raw, &graph, &mut phase).unwrap();
        emit_v10_shape_phase(&raw, &graph, &mut phase).unwrap();
        emit_v10_vertex_phase(&raw, &graph, &mut phase).unwrap();
        emit_v10_material_initial_queue_phase(&raw, &graph.models[0], &mut phase).unwrap();
        emit_v10_shader_assignment_children_phase(&raw, &graph.models[0], &mut phase).unwrap();
        assert!(phase.bytes.len() > 9500);
        assert_eq!(
            phase
                .bytes
                .iter()
                .zip(&raw)
                .position(|(left, right)| left != right),
            None,
            "shader-assignment child queue differs; emitted length {}",
            phase.bytes.len()
        );
    }

    #[cfg(windows)]
    #[test]
    fn complete_material_children_match_toolbox_prefix() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let graph = V10ResourceGraph::parse(&raw).unwrap();
        let mut phase = emit_v10_fixed_phase(&raw, &graph).unwrap();
        emit_v10_model_core_phase(&raw, &graph, &mut phase).unwrap();
        emit_v10_shape_phase(&raw, &graph, &mut phase).unwrap();
        emit_v10_vertex_phase(&raw, &graph, &mut phase).unwrap();
        emit_v10_material_initial_queue_phase(&raw, &graph.models[0], &mut phase).unwrap();
        emit_v10_shader_assignment_children_phase(&raw, &graph.models[0], &mut phase).unwrap();
        emit_v10_shader_info_children_phase(&raw, &graph.models[0], &mut phase).unwrap();
        assert!(phase.bytes.len() > 11000);
        assert_eq!(&raw[phase.bytes.len()..phase.bytes.len() + 4], b"_STR");
        assert_eq!(
            phase
                .bytes
                .iter()
                .zip(&raw)
                .position(|(left, right)| left != right),
            None,
            "complete material child queue differs; emitted length {}",
            phase.bytes.len()
        );
    }

    #[cfg(windows)]
    #[test]
    fn canonical_copy_matches_complete_toolbox_bfres() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let raw =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(path).unwrap())
                .unwrap();
        let mut emitted = emit_v10_canonical_copy(&raw).unwrap();
        rebase_v10_relocations(&raw, &mut emitted).unwrap();
        let expected_size = read_u32(&raw, 0x1c).unwrap() as usize;
        assert_eq!(
            emitted
                .bytes
                .iter()
                .zip(&raw)
                .position(|(left, right)| left != right),
            None,
            "canonical file differs; emitted={}, expected file size={expected_size}, buffer totals={}/{}",
            emitted.bytes.len(),
            read_u32(&emitted.bytes, 364).unwrap(),
            read_u32(&raw, 364).unwrap(),
        );
        assert_eq!(emitted.bytes.len(), expected_size);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "diagnostic for the supplied TotkBits and Toolbox comparison fixtures"]
    fn canonicalizes_totkbits_fixture_and_reports_raw_difference() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/BotW Weapon Restoration/romfs/_model");
        let totkbits = root.join("totkbits/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc");
        let toolbox = root.join("toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc");
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/works/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc");
        if !totkbits.is_file() || !toolbox.is_file() {
            return;
        }
        let source =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(totkbits).unwrap())
                .unwrap();
        let expected =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(toolbox).unwrap())
                .unwrap();
        let base =
            crate::compression::meshcodec::MeshCodec::decompress(&std::fs::read(base).unwrap())
                .unwrap();
        let mut emitted = emit_v10_canonical_copy(&source).unwrap();
        rebase_v10_relocations(&source, &mut emitted).unwrap();
        V10ResourceGraph::parse(&emitted.bytes).unwrap();
        let first = emitted
            .bytes
            .iter()
            .zip(&expected)
            .position(|(left, right)| left != right);
        let string_table = source
            .windows(4)
            .position(|value| value == b"_STR")
            .unwrap();
        let expected_string_table = expected
            .windows(4)
            .position(|value| value == b"_STR")
            .unwrap();
        let lengths = |bytes: &[u8]| {
            let graph = V10ResourceGraph::parse(bytes).unwrap();
            let mut phase = emit_v10_fixed_phase(bytes, &graph).unwrap();
            let mut values = vec![phase.bytes.len()];
            emit_v10_model_core_phase(bytes, &graph, &mut phase).unwrap();
            values.push(phase.bytes.len());
            emit_v10_shape_phase(bytes, &graph, &mut phase).unwrap();
            values.push(phase.bytes.len());
            emit_v10_vertex_phase(bytes, &graph, &mut phase).unwrap();
            values.push(phase.bytes.len());
            for model in &graph.models {
                emit_v10_material_initial_queue_phase(bytes, model, &mut phase).unwrap();
                values.push(phase.bytes.len());
                emit_v10_shader_assignment_children_phase(bytes, model, &mut phase).unwrap();
                values.push(phase.bytes.len());
                emit_v10_shader_info_children_phase(bytes, model, &mut phase).unwrap();
                values.push(phase.bytes.len());
            }
            values
        };
        eprintln!(
            "canonical raw lengths: emitted={} toolbox={}, first difference={first:?}; STR={string_table} end={} buffer={} memory={} ext={:?} rlt={}; toolbox STR={} end={} buffer={} memory={} ext={:?} rlt={}",
            emitted.bytes.len(),
            expected.len(),
            string_table_end(&source, string_table).unwrap(),
            read_u64(&source, V10ResourceGraph::parse(&source).unwrap().buffer_info + 8).unwrap(),
            read_u64(&source, 0xa8).unwrap(),
            V10ResourceGraph::parse(&source).unwrap().external_files.iter().map(|entry| (read_u64(&source, *entry).unwrap(), read_u64(&source, *entry + 8).unwrap())).collect::<Vec<_>>(),
            read_u32(&source, 0x18).unwrap(),
            expected_string_table,
            string_table_end(&expected, expected_string_table).unwrap(),
            read_u64(&expected, V10ResourceGraph::parse(&expected).unwrap().buffer_info + 8).unwrap(),
            read_u64(&expected, 0xa8).unwrap(),
            V10ResourceGraph::parse(&expected).unwrap().external_files.iter().map(|entry| (read_u64(&expected, *entry).unwrap(), read_u64(&expected, *entry + 8).unwrap())).collect::<Vec<_>>(),
            read_u32(&expected, 0x18).unwrap(),
        );
        eprintln!(
            "root pool source=({}, {}) toolbox=({}, {})",
            read_u64(&source, 0xd8).unwrap(),
            read_u32(&source, 0xe4).unwrap(),
            read_u64(&expected, 0xd8).unwrap(),
            read_u32(&expected, 0xe4).unwrap()
        );
        let source_rlt = read_u32(&source, 0x18).unwrap() as usize;
        let toolbox_rlt = read_u32(&expected, 0x18).unwrap() as usize;
        for section in 0..6 {
            let a = read_u32(&source, source_rlt + 24 + section * 24).unwrap();
            let az = read_u32(&source, source_rlt + 28 + section * 24).unwrap();
            let b = read_u32(&expected, toolbox_rlt + 24 + section * 24).unwrap();
            let bz = read_u32(&expected, toolbox_rlt + 28 + section * 24).unwrap();
            eprintln!("rlt section={section} source={a}/{az} toolbox={b}/{bz}");
        }
        let entry_start = source_rlt + 16 + 5 * 24;
        for index in [49usize, 52] {
            eprintln!(
                "rlt entry={index} source={:?} toolbox={:?}",
                &source[entry_start + index * 8..entry_start + (index + 1) * 8],
                &expected[entry_start + index * 8..entry_start + (index + 1) * 8]
            );
        }
        eprintln!(
            "phase lengths source={:?} toolbox={:?}",
            lengths(&source),
            lengths(&expected)
        );
        let compared = read_u32(&expected, 0x1c).unwrap() as usize;
        let mut runs = Vec::new();
        let mut offset = 0usize;
        while offset < compared {
            if source[offset] == expected[offset] {
                offset += 1;
                continue;
            }
            let start = offset;
            while offset < compared && source[offset] != expected[offset] {
                offset += 1;
            }
            runs.push((start, offset - start));
        }
        eprintln!(
            "raw difference runs={} differing_bytes={} first_runs={:?}",
            runs.len(),
            runs.iter().map(|(_, len)| len).sum::<usize>(),
            &runs[..runs.len().min(40)]
        );
        eprintln!(
            "first byte differences={:?}",
            source
                .iter()
                .zip(&expected)
                .enumerate()
                .filter_map(|(offset, (left, right))| {
                    (left != right).then_some((offset, *left, *right))
                })
                .take(80)
                .collect::<Vec<_>>()
        );
        let source_graph = V10ResourceGraph::parse(&source).unwrap();
        let toolbox_graph = V10ResourceGraph::parse(&expected).unwrap();
        let source_buffer = read_u64(&source, source_graph.buffer_info + 8).unwrap() as usize;
        let toolbox_buffer = read_u64(&expected, toolbox_graph.buffer_info + 8).unwrap() as usize;
        for (vertex_index, (left, right)) in source_graph.models[0]
            .vertex_buffers
            .iter()
            .zip(&toolbox_graph.models[0].vertex_buffers)
            .enumerate()
        {
            let mut left_cursor =
                source_buffer + read_u32(&source, left.header + 72).unwrap() as usize;
            let mut right_cursor =
                toolbox_buffer + read_u32(&expected, right.header + 72).unwrap() as usize;
            for (buffer_index, (left_size, right_size)) in left
                .buffer_sizes
                .iter()
                .zip(&right.buffer_sizes)
                .enumerate()
            {
                let left_len = read_u32(&source, *left_size).unwrap() as usize;
                let right_len = read_u32(&expected, *right_size).unwrap() as usize;
                let count = source[left_cursor..left_cursor + left_len.min(right_len)]
                    .iter()
                    .zip(&expected[right_cursor..right_cursor + left_len.min(right_len)])
                    .filter(|(a, b)| a != b)
                    .count();
                eprintln!("gpu vertex={vertex_index} buffer={buffer_index} offsets={left_cursor}/{right_cursor} lengths={left_len}/{right_len} differences={count}");
                if matches!(buffer_index, 4 | 5) {
                    let mut deltas = std::collections::BTreeMap::<i16, usize>::new();
                    for (&a, &b) in source[left_cursor..left_cursor + left_len]
                        .iter()
                        .zip(&expected[right_cursor..right_cursor + right_len])
                    {
                        *deltas
                            .entry((a as i8 as i16) - (b as i8 as i16))
                            .or_default() += 1;
                    }
                    eprintln!("gpu vertex={vertex_index} buffer={buffer_index} signed_byte_deltas={deltas:?}");
                }
                if matches!(buffer_index, 1 | 4 | 5) {
                    let samples = (0..left_len.min(right_len) / 4)
                        .filter_map(|index| {
                            let a = read_u32(&source, left_cursor + index * 4).unwrap();
                            let b = read_u32(&expected, right_cursor + index * 4).unwrap();
                            (a != b).then_some((index, a, b))
                        })
                        .take(12)
                        .collect::<Vec<_>>();
                    eprintln!("gpu vertex={vertex_index} buffer={buffer_index} words={samples:?}");
                }
                if buffer_index == 0 {
                    let samples = (0..(left_len / 12))
                        .filter_map(|index| {
                            let left_value = (0..3)
                                .map(|component| {
                                    read_f32(&source, left_cursor + index * 12 + component * 4)
                                        .unwrap()
                                })
                                .collect::<Vec<_>>();
                            let right_value = (0..3)
                                .map(|component| {
                                    read_f32(&expected, right_cursor + index * 12 + component * 4)
                                        .unwrap()
                                })
                                .collect::<Vec<_>>();
                            (left_value != right_value).then_some((index, left_value, right_value))
                        })
                        .take(8)
                        .collect::<Vec<_>>();
                    eprintln!("gpu vertex={vertex_index} position_samples={samples:?}");
                }
                left_cursor = align_up(
                    left_cursor + left_len,
                    read_u16(&source, left.header + 86).unwrap() as usize,
                );
                right_cursor = align_up(
                    right_cursor + right_len,
                    read_u16(&expected, right.header + 86).unwrap() as usize,
                );
            }
        }
        for (label, bytes) in [
            ("base", base.as_slice()),
            ("source", source.as_slice()),
            ("toolbox", expected.as_slice()),
        ] {
            let graph = V10ResourceGraph::parse(bytes).unwrap();
            for (model_index, model) in graph.models.iter().enumerate() {
                eprintln!("{label} model={model_index} skeleton bones={} matrix_to_bone={:?} inverse={} mirrored={:?} counts=({},{},{})", model.skeleton.bones.len(), model.skeleton.matrix_to_bone.map(|(_, count)| count), model.skeleton.inverse_matrices.len(), model.skeleton.mirrored_bones.map(|(_, count)| count), read_u16(bytes, model.skeleton.header + 56).unwrap(), read_u16(bytes, model.skeleton.header + 58).unwrap(), read_u16(bytes, model.skeleton.header + 60).unwrap());
                eprintln!(
                    "{label} model={model_index} shape_dict={:?}",
                    read_dictionary(bytes, read_u64(bytes, model.header + 48).unwrap() as usize)
                        .unwrap()
                );
                for (vertex_index, vertex) in model.vertex_buffers.iter().enumerate() {
                    let attributes = vertex
                        .attributes
                        .iter()
                        .map(|offset| {
                            let name =
                                read_res_string(bytes, read_u64(bytes, *offset).unwrap() as usize)
                                    .unwrap();
                            (
                                name,
                                read_u16(bytes, *offset + 8).unwrap(),
                                read_u16(bytes, *offset + 12).unwrap(),
                                bytes[*offset + 14],
                            )
                        })
                        .collect::<Vec<_>>();
                    let sizes = vertex
                        .buffer_sizes
                        .iter()
                        .map(|offset| {
                            (
                                read_u32(bytes, *offset).unwrap(),
                                read_u32(bytes, *offset + 4).unwrap(),
                            )
                        })
                        .collect::<Vec<_>>();
                    let strides = vertex
                        .buffer_strides
                        .iter()
                        .map(|offset| {
                            (
                                read_u32(bytes, *offset).unwrap(),
                                read_u32(bytes, *offset + 4).unwrap(),
                            )
                        })
                        .collect::<Vec<_>>();
                    eprintln!("{label} model={model_index} vertex={vertex_index} header={} attrs={attributes:?} sizes={sizes:?} strides={strides:?}", vertex.header);
                    eprintln!(
                        "{label} vertex={vertex_index} dict={:?}",
                        read_dictionary(
                            bytes,
                            read_u64(bytes, vertex.header + 16).unwrap() as usize
                        )
                        .unwrap()
                    );
                }
                for (shape_index, shape) in model.shapes.iter().enumerate() {
                    let meshes = shape
                        .meshes
                        .iter()
                        .map(|offset| {
                            (
                                read_u16(bytes, *offset + 52).unwrap(),
                                read_u32(bytes, *offset + 48).unwrap(),
                            )
                        })
                        .collect::<Vec<_>>();
                    eprintln!("{label} model={model_index} shape={shape_index} header={} meshes={meshes:?} skin={:?} bounds={} radii={}", shape.header, shape.skin_bones.map(|(_, count)| count), shape.bounds.len(), shape.radii.len());
                    eprintln!(
                        "{label} shape={shape_index} bound_values={:?} radius_values={:?}",
                        shape
                            .bounds
                            .iter()
                            .map(|offset| (0..6)
                                .map(|i| read_f32(bytes, *offset + i * 4).unwrap())
                                .collect::<Vec<_>>())
                            .collect::<Vec<_>>(),
                        shape
                            .radii
                            .iter()
                            .map(|offset| (0..4)
                                .map(|i| read_f32(bytes, *offset + i * 4).unwrap())
                                .collect::<Vec<_>>())
                            .collect::<Vec<_>>()
                    );
                }
                for (material_index, material) in model.materials.iter().enumerate() {
                    let info = &material.shader_info;
                    let attribute_values = info
                        .attribute_values
                        .iter()
                        .map(|offset| {
                            read_res_string(bytes, read_u64(bytes, *offset).unwrap() as usize)
                                .unwrap()
                        })
                        .collect::<Vec<_>>();
                    let attribute_dict =
                        read_dictionary(bytes, info.assignment.attribute_dict).unwrap();
                    eprintln!("{label} material={material_index} header={} textures={} samplers={} ranges={:?}/{:?}/{:?}/{:?} shader attrs={} attr_idx={:?} sampler_values={} sampler_idx={:?} option_flags={:?} option_values={} option_idx={:?}", material.header, material.texture_names.len(), material.samplers.len(), material.render_info_data, material.render_info_counts, material.render_info_offsets, material.shader_parameter_data, info.attribute_values.len(), info.attribute_indices, info.sampler_values.len(), info.sampler_indices, info.option_bitflags, info.option_values.len(), info.option_indices);
                    eprintln!("{label} material={material_index} shader_attribute_values={attribute_values:?} shader_attribute_dict={attribute_dict:?}");
                }
            }
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "diagnostic: compares canonical copies of tmp/bfres originals with Toolbox resaves"]
    fn canonical_copies_match_toolbox_resave_corpus() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/_CLAUDE/parity/toolbox_raw");
        let Ok(entries) = std::fs::read_dir(&root) else {
            return;
        };
        let mut failures = Vec::new();
        for entry in entries {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            let original_name = name.replace(".resave.bfres", ".bfres");
            let original = std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../tmp/bfres")
                    .join(&original_name),
            )
            .unwrap();
            let expected = std::fs::read(&path).unwrap();
            let mut emitted = emit_v10_canonical_copy(&original).unwrap();
            rebase_v10_relocations(&original, &mut emitted).unwrap();
            let first = emitted
                .bytes
                .iter()
                .zip(&expected)
                .position(|(left, right)| left != right);
            let differing = emitted
                .bytes
                .iter()
                .zip(&expected)
                .filter(|(left, right)| left != right)
                .count();
            eprintln!(
                "{name}: emitted={} expected={} first_diff={first:?} differing_bytes={differing}",
                emitted.bytes.len(),
                expected.len()
            );
            std::fs::write(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../tmp/_CLAUDE/parity")
                    .join(format!("{original_name}.totkbits_canonical.bfres")),
                &emitted.bytes,
            )
            .unwrap();
            if first.is_some() || emitted.bytes.len() != expected.len() {
                failures.push(name);
            }
        }
        assert!(
            failures.is_empty(),
            "canonical copies differ from Toolbox: {failures:?}"
        );
    }
}
