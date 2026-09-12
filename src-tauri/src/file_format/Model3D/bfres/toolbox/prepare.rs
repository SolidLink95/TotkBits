//! Everything Switch Toolbox does to the loaded object graph between "open"
//! and the actual serializer: `BfresSwitch.SetModel`/`SetMaterial`,
//! `BFRES.SetShaderAssignAttributes`, `FSKL.CalculateIndices`,
//! `MaterialParserV10.PrepareSave`, the shader-assign de-duplication of
//! `Model.Save` and the dictionary rebuild of `ResFile.PreSave`.

use super::super::BfresError;
use super::matrix::inverse_bind_matrix;
use super::model::*;

pub const DEFAULT_VALUE: &str = "<Default Value>";

/// `MaterialParserV10.ShaderInfo` as built by `PrepareSave`.
#[derive(Clone, Debug, Default)]
pub struct ShaderInfoV10 {
    pub attrib_assigns: Vec<String>,
    pub sampler_assigns: Vec<String>,
    pub option_values: Vec<String>,
    pub option_toggles: Vec<bool>,
    pub option_indices: Vec<i16>,
    pub attribute_indices: Option<Vec<i8>>,
    pub sampler_indices: Option<Vec<i8>>,
    /// Index into `PreparedModel::shader_assigns`.
    pub assign: usize,
}

#[derive(Clone, Debug, Default)]
pub struct PreparedModel {
    pub model: Model,
    pub infos: Vec<ShaderInfoV10>,
    /// Material index whose data backs each de-duplicated `ShaderAssignV10`.
    pub shader_assigns: Vec<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Prepared {
    pub file: ResFile,
    pub models: Vec<PreparedModel>,
}

/// .NET Framework (64-bit) `string.GetHashCode`.
pub fn dotnet_string_hash(value: &str) -> i32 {
    let chars: Vec<u16> = value.encode_utf16().collect();
    let mut hash1: i32 = 5381;
    let mut hash2: i32 = 5381;
    let mut i = 0;
    while i < chars.len() {
        hash1 = (hash1.wrapping_shl(5).wrapping_add(hash1)) ^ i32::from(chars[i]);
        if i + 1 >= chars.len() {
            break;
        }
        hash2 = (hash2.wrapping_shl(5).wrapping_add(hash2)) ^ i32::from(chars[i + 1]);
        i += 2;
    }
    hash1.wrapping_add(hash2.wrapping_mul(1566083941))
}

fn shader_assign_hash(material: &Material) -> i32 {
    let mut hash = dotnet_string_hash(&material.shader_archive);
    hash = hash.wrapping_add(dotnet_string_hash(&material.shading_model));
    for info in &material.render_infos {
        hash = hash.wrapping_add(dotnet_string_hash(&info.name));
        hash = hash.wrapping_add(i32::from(info.kind));
    }
    for param in &material.shader_params {
        hash = hash.wrapping_add(dotnet_string_hash(&param.name));
        hash = hash.wrapping_add(i32::from(param.data_offset));
        hash = hash.wrapping_add(i32::from(param.param_type));
    }
    for (key, _) in &material.options {
        hash = hash.wrapping_add(dotnet_string_hash(key));
    }
    for (key, _) in &material.attrib_assign {
        hash = hash.wrapping_add(dotnet_string_hash(key));
    }
    for (key, _) in &material.sampler_assign {
        hash = hash.wrapping_add(dotnet_string_hash(key));
    }
    hash
}

/// `BfresSwitch.WriteShaderParams`: the parameter block is re-packed with
/// consecutive offsets in list order using Syroot's `DataSize` per type.
fn repack_shader_params(material: &mut Material) -> Result<(), BfresError> {
    let mut packed = Vec::with_capacity(material.param_data.len());
    let mut offset = 0u32;
    for param in &mut material.shader_params {
        let size = param.data_size().ok_or_else(|| {
            BfresError::new(
                0,
                format!(
                    "unknown shader parameter type {} for {}",
                    param.param_type, param.name
                ),
            )
        })? as usize;
        let start = usize::from(param.data_offset);
        let source = material
            .param_data
            .get(start..start + size)
            .ok_or_else(|| {
                BfresError::new(
                    start,
                    format!(
                        "shader parameter {} lies outside the parameter block",
                        param.name
                    ),
                )
            })?;
        packed.extend_from_slice(source);
        param.data_offset = offset as u16;
        offset += size as u32;
    }
    material.param_data = packed;
    Ok(())
}

/// `BFRES.SetShaderAssignAttributes`: links every vertex attribute and
/// sampler of a shape's material that the shader assign does not mention.
fn set_shader_assign_attributes(model: &mut Model) {
    for shape_index in 0..model.shapes.len() {
        let shape = &model.shapes[shape_index];
        let material_index = usize::from(shape.material_index);
        let Some(vertex_buffer) = model
            .vertex_buffers
            .get(usize::from(shape.vertex_buffer_index))
        else {
            continue;
        };
        let attribute_names: Vec<String> = vertex_buffer
            .attributes
            .iter()
            .map(|attribute| attribute.name.clone())
            .collect();
        let Some(material) = model.materials.get_mut(material_index) else {
            continue;
        };
        for name in attribute_names {
            if !material
                .attrib_assign
                .iter()
                .any(|(_, value)| *value == name)
                && !material.attrib_assign.iter().any(|(key, _)| *key == name)
            {
                material.attrib_assign.push((name.clone(), name));
            }
        }
        let sampler_names: Vec<String> = material
            .texture_refs
            .iter()
            .enumerate()
            .filter_map(|(index, _)| material.samplers.get(index).map(|s| s.name.clone()))
            .collect();
        for name in sampler_names {
            if !material
                .sampler_assign
                .iter()
                .any(|(_, value)| *value == name)
                && !material.sampler_assign.iter().any(|(key, _)| *key == name)
            {
                material.sampler_assign.push((name.clone(), name));
            }
        }
    }
}

/// `FSKL.CalculateIndices`: the inverse model matrices are recomputed from
/// the bone transforms for every bone that owns a smooth matrix.
fn recalculate_inverse_matrices(skeleton: &mut Skeleton) {
    let mut matrices = Vec::new();
    for index in 0..skeleton.bones.len() {
        if skeleton.bones[index].smooth_matrix_index != -1 {
            matrices.push(inverse_bind_matrix(&skeleton.bones, index));
        }
    }
    skeleton.inverse_matrices = matrices;
}

/// `MaterialParserV10.PrepareSave`.
fn prepare_material(material: &mut Material) -> ShaderInfoV10 {
    let mut info = ShaderInfoV10::default();
    let mut sampler_indices: Vec<i8> = Vec::new();
    let mut attribute_indices: Vec<i8> = Vec::new();
    let mut option_indices: Vec<i16> = Vec::new();

    for (_, value) in &material.sampler_assign {
        if value == DEFAULT_VALUE {
            sampler_indices.push(-1);
            continue;
        }
        info.sampler_assigns.push(value.clone());
        let index = info
            .sampler_assigns
            .iter()
            .position(|v| v == value)
            .unwrap();
        sampler_indices.push(index as i8);
    }
    for (_, value) in &material.attrib_assign {
        if value == DEFAULT_VALUE {
            attribute_indices.push(-1);
            continue;
        }
        info.attrib_assigns.push(value.clone());
        let index = info.attrib_assigns.iter().position(|v| v == value).unwrap();
        attribute_indices.push(index as i8);
    }
    let mut choice = 0i16;
    let mut toggles = Vec::new();
    for (_, value) in &material.options {
        if value == DEFAULT_VALUE {
            option_indices.push(-1);
            continue;
        }
        if value == "True" {
            toggles.push(true);
        } else if value == "False" {
            toggles.push(false);
        } else {
            info.option_values.push(value.clone());
        }
        option_indices.push(choice);
        choice += 1;
    }
    info.option_toggles = toggles;
    if sampler_indices.iter().any(|i| *i == -1) {
        info.sampler_indices = Some(sampler_indices);
    }
    if attribute_indices.iter().any(|i| *i == -1) {
        info.attribute_indices = Some(attribute_indices);
    }
    info.option_indices = option_indices;

    // Render infos are re-ordered by type: strings, floats, ints.
    let mut ordered = Vec::with_capacity(material.render_infos.len());
    for kind in [2u8, 1, 0] {
        ordered.extend(
            material
                .render_infos
                .iter()
                .filter(|ri| ri.kind == kind)
                .cloned(),
        );
    }
    material.render_infos = ordered;
    info
}

pub fn prepare(source: &ResFile) -> Result<Prepared, BfresError> {
    let mut file = source.clone();
    let mut models = Vec::with_capacity(file.models.len());
    for model in file.models.drain(..) {
        let mut model = model;

        // BfresSwitch.SetModel: one vertex buffer per shape, in shape order.
        let mut vertex_buffers = Vec::with_capacity(model.shapes.len());
        for shape in &mut model.shapes {
            let buffer = model
                .vertex_buffers
                .get(usize::from(shape.vertex_buffer_index))
                .cloned()
                .ok_or_else(|| {
                    BfresError::new(
                        0,
                        format!("shape {} references a missing vertex buffer", shape.name),
                    )
                })?;
            vertex_buffers.push(buffer);
            shape.vertex_buffer_index = (vertex_buffers.len() - 1) as u16;
        }
        model.vertex_buffers = vertex_buffers;
        model.path = String::new();

        set_shader_assign_attributes(&mut model);
        recalculate_inverse_matrices(&mut model.skeleton);
        for material in &mut model.materials {
            repack_shader_params(material)?;
        }

        // MaterialParserV10.PrepareSave + Model.Save de-duplication.
        let mut infos = Vec::with_capacity(model.materials.len());
        for material in &mut model.materials {
            infos.push(prepare_material(material));
        }
        let mut shader_assigns: Vec<usize> = Vec::new();
        let mut hashes: Vec<i32> = Vec::new();
        for (index, material) in model.materials.iter().enumerate() {
            let hash = shader_assign_hash(material);
            let position = match hashes.iter().position(|h| *h == hash) {
                Some(position) => position,
                None => {
                    hashes.push(hash);
                    shader_assigns.push(index);
                    shader_assigns.len() - 1
                }
            };
            infos[index].assign = position;
        }
        models.push(PreparedModel {
            model,
            infos,
            shader_assigns,
        });
    }
    Ok(Prepared { file, models })
}
