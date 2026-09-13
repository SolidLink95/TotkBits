//! Switch Toolbox's model replacement (`FMDL.AddOjects` with `Replace`)
//! applied to the Toolbox object model, so that `--fbx` produces the same
//! bytes the Toolbox CLI writes: the Assimp scene becomes `STGenericObject`s
//! (`AssimpData.CreateGenericObject`), every existing shape is dropped, the
//! skinning palette is regenerated, and each object is turned into an FSHP
//! with Toolbox's own attribute set, bone list, tangents, bounding boxes,
//! weight quantization and Syroot's per-attribute vertex buffers.
//!
//! The import settings are the ones the CLI applies headlessly: attribute
//! formats from `BfresModelImportSettings_Load`, "map original materials",
//! "reset UV params", tangents/bitangents whenever the FBX has UVs, no
//! skin-count limit, no dummy LODs, no extra rotation and no UV flip beyond
//! Assimp's own.

use super::model::{Material, Mesh, Model, Shape, VertexAttrib, VertexBuffer, VertexData};
use super::opentk::{quat_from_euler, Mat4, Quat, Vec3};
use super::skeleton_import::{self, SkeletonImportReport};
use super::BfresError;
use crate::parser::fbx::assimp::{self, AssimpMesh};
use crate::parser::fbx::toolbox_skeleton::{self, MeshBoneUsage};

/// Syroot `GFX.AttribFormat` values (big-endian in the attribute record).
mod format {
    pub const F8_UNORM: u16 = 0x0102;
    pub const F8_UINT: u16 = 0x0302;
    pub const F8_SNORM: u16 = 0x0202;
    pub const F8_SINT: u16 = 0x0402;
    pub const F8_UINT_TO_SINGLE: u16 = 0x0802;
    pub const F8_SINT_TO_SINGLE: u16 = 0x0A02;
    pub const F16_UNORM: u16 = 0x010A;
    pub const F16_UINT: u16 = 0x020A;
    pub const F16_SNORM: u16 = 0x030A;
    pub const F16_SINT: u16 = 0x040A;
    pub const F16_SINGLE: u16 = 0x050A;
    pub const F8_8_UNORM: u16 = 0x0109;
    pub const F8_8_UINT: u16 = 0x0309;
    pub const F8_8_SNORM: u16 = 0x0209;
    pub const F8_8_SINT: u16 = 0x0409;
    pub const F8_8_UINT_TO_SINGLE: u16 = 0x0804;
    pub const F8_8_SINT_TO_SINGLE: u16 = 0x0A04;
    pub const F16_16_UNORM: u16 = 0x0112;
    pub const F16_16_SNORM: u16 = 0x0212;
    pub const F16_16_UINT: u16 = 0x0312;
    pub const F16_16_SINT: u16 = 0x0412;
    pub const F16_16_SINGLE: u16 = 0x0512;
    pub const F8_8_8_8_UNORM: u16 = 0x010B;
    pub const F8_8_8_8_SNORM: u16 = 0x020B;
    pub const F8_8_8_8_UINT: u16 = 0x030B;
    pub const F8_8_8_8_SINT: u16 = 0x040B;
    pub const F8_8_8_8_UINT_TO_SINGLE: u16 = 0x080B;
    pub const F8_8_8_8_SINT_TO_SINGLE: u16 = 0x0A0B;
    pub const F10_10_10_2_UNORM: u16 = 0x000B;
    pub const F10_10_10_2_UINT: u16 = 0x090B;
    pub const F10_10_10_2_SNORM: u16 = 0x020E;
    pub const F10_10_10_2_SINT: u16 = 0x099B;
    pub const F16_16_16_16_UNORM: u16 = 0x0115;
    pub const F16_16_16_16_SNORM: u16 = 0x0215;
    pub const F16_16_16_16_UINT: u16 = 0x0315;
    pub const F16_16_16_16_SINT: u16 = 0x0415;
    pub const F16_16_16_16_SINGLE: u16 = 0x0515;
    pub const F32_UINT: u16 = 0x0314;
    pub const F32_SINT: u16 = 0x0416;
    pub const F32_SINGLE: u16 = 0x0516;
    pub const F32_32_UINT: u16 = 0x0317;
    pub const F32_32_SINT: u16 = 0x0417;
    pub const F32_32_SINGLE: u16 = 0x0517;
    pub const F32_32_32_UINT: u16 = 0x0318;
    pub const F32_32_32_SINT: u16 = 0x0418;
    pub const F32_32_32_SINGLE: u16 = 0x0518;
    pub const F32_32_32_32_UINT: u16 = 0x0319;
    pub const F32_32_32_32_SINT: u16 = 0x0419;
    pub const F32_32_32_32_SINGLE: u16 = 0x0519;
}

/// `ShapeFlags.HasVertexBuffer`.
const SHAPE_HAS_VERTEX_BUFFER: u32 = 2;
/// `GFX.PrimitiveType.Triangles`.
const PRIMITIVE_TRIANGLES: u32 = 3;
/// `GFX.IndexFormat.UInt16` / `UInt32`.
const INDEX_UINT16: u32 = 1;
const INDEX_UINT32: u32 = 2;
/// Syroot's `VertexBuffer` default alignment.
const GPU_ALIGNMENT: u16 = 8;

#[derive(Clone, Debug, Default)]
pub struct GeometryImportReport {
    /// Shape names in the order they were created.
    pub shapes: Vec<String>,
    pub shapes_before: usize,
    pub skeleton: SkeletonImportReport,
}

/// Toolbox's `Vertex` for one imported object.
#[derive(Clone, Debug)]
struct Vertex {
    pos: Vec3,
    nrm: Vec3,
    uv: [[f32; 2]; 4],
    col: [f32; 4],
    col2: [f32; 4],
    tan: [f32; 4],
    bitan: [f32; 4],
    bone_names: Vec<String>,
    bone_weights: Vec<f32>,
    bone_ids: Vec<i32>,
}

impl Default for Vertex {
    fn default() -> Self {
        Vertex {
            pos: Vec3::ZERO,
            nrm: Vec3::ZERO,
            uv: [[0.0; 2]; 4],
            col: [1.0; 4],
            col2: [1.0; 4],
            tan: [0.0; 4],
            bitan: [0.0; 4],
            bone_names: Vec::new(),
            bone_weights: Vec::new(),
            bone_ids: Vec::new(),
        }
    }
}

/// `STGenericObject` as `AssimpData.CreateGenericObject` fills it.
struct GenericObject {
    name: String,
    material_index: i32,
    bone_index: u16,
    vertex_skin_count: u8,
    vertices: Vec<Vertex>,
    faces: Vec<u32>,
    has_pos: bool,
    has_nrm: bool,
    has_uv0: bool,
    has_uv1: bool,
    has_uv2: bool,
    has_weights: bool,
    has_indices: bool,
    has_vertex_colors: bool,
    has_tangents: bool,
}

/// Toolbox's `FSHP.VertexAttribute`.
#[derive(Clone, Debug)]
struct Attribute {
    name: String,
    format: u16,
}

/// The settings `ApplyControlStates` leaves behind for a headless import.
struct Settings {
    enable_positions: bool,
    enable_normals: bool,
    enable_uv0: bool,
    enable_uv1: bool,
    enable_uv2: bool,
    enable_tangents: bool,
    enable_bitangents: bool,
    enable_weights: bool,
    enable_indices: bool,
    enable_vertex_colors: bool,
}

impl Settings {
    /// `SetModelAttributes`.
    fn from_objects(objects: &[GenericObject]) -> Settings {
        let any = |f: fn(&GenericObject) -> bool| objects.iter().any(f);
        Settings {
            enable_positions: any(|o| o.has_pos),
            enable_normals: any(|o| o.has_nrm),
            enable_uv0: any(|o| o.has_uv0),
            enable_uv1: any(|o| o.has_uv1),
            enable_uv2: any(|o| o.has_uv2),
            enable_tangents: any(|o| o.has_uv0),
            enable_bitangents: any(|o| o.has_uv0),
            enable_weights: any(|o| o.has_weights),
            enable_indices: any(|o| o.has_weights),
            enable_vertex_colors: any(|o| o.has_vertex_colors),
        }
    }

    /// `BfresModelImportSettings.CreateNewAttributes` with the `Default`
    /// game preset (one buffer per attribute).
    fn create_new_attributes(&self, material: &Material) -> Vec<Attribute> {
        let has_assign = |name: &str| {
            material
                .attrib_assign
                .iter()
                .any(|(_, value)| value == name)
        };
        let mut attributes: Vec<Attribute> = Vec::new();
        let mut push = |name: &str, format: u16| {
            attributes.push(Attribute {
                name: name.to_owned(),
                format,
            });
        };
        if self.enable_positions {
            push("_p0", format::F32_32_32_SINGLE);
        }
        if self.enable_normals {
            push("_n0", format::F10_10_10_2_SNORM);
        }
        if self.enable_vertex_colors {
            push("_c0", format::F16_16_16_16_SINGLE);
        }
        let mut packed_uv = false;
        if self.enable_uv0 {
            if has_assign("_g3d_02_u0_u1") {
                push("_g3d_02_u0_u1", format::F16_16_16_16_SINGLE);
                packed_uv = true;
            } else {
                push("_u0", format::F16_16_SINGLE);
            }
        }
        if self.enable_uv1 && self.enable_uv0 && !packed_uv {
            push("_u1", format::F16_16_SINGLE);
        }
        if self.enable_uv2 && self.enable_uv0 {
            if has_assign("_g3d_02_u2_u3") {
                push("_g3d_02_u2_u3", format::F16_16_16_16_SINGLE);
            } else {
                push("_u2", format::F16_16_SINGLE);
            }
        }
        if self.enable_tangents {
            push("_t0", format::F8_8_8_8_SNORM);
        }
        if self.enable_bitangents {
            push("_b0", format::F8_8_8_8_SNORM);
        }
        if self.enable_weights {
            push("_w0", format::F8_8_8_8_UNORM);
        }
        if self.enable_indices {
            push("_i0", format::F8_8_8_8_UINT);
        }
        if has_assign("_c0") && !self.enable_vertex_colors {
            push("_c0", format::F16_16_16_16_SINGLE);
        }
        for index in 1..6 {
            let name = format!("_c{index}");
            if has_assign(&name) {
                push(&name, format::F16_16_16_16_SINGLE);
            }
        }
        attributes
    }
}

/// `FMDL.AddOjects(path, resFile, null, Replace: true, headless settings)`.
pub fn import_model(
    model: &mut Model,
    fbx: &[u8],
    import_bones: bool,
) -> Result<GeometryImportReport, BfresError> {
    let scene = assimp::import_scene(fbx).map_err(|error| BfresError::new(0, error.to_string()))?;
    let mut objects: Vec<GenericObject> = Vec::new();
    for (index, mesh) in scene.meshes.iter().enumerate() {
        objects.push(create_generic_object(mesh, index)?);
    }
    if objects.is_empty() {
        return Err(BfresError::new(0, "No models found!"));
    }
    let mut report = GeometryImportReport {
        shapes_before: model.shapes.len(),
        ..Default::default()
    };
    let settings = Settings::from_objects(&objects);

    // Name matches against the shapes about to be replaced inherit the
    // material (and, without "Import Bones", the bone index).
    for object in &mut objects {
        object.bone_index = 0;
        if let Some(existing) = model.shapes.iter().find(|shape| shape.name == object.name) {
            object.material_index = i32::from(existing.material_index);
            if !import_bones {
                object.bone_index = existing.bone_index;
            }
        }
    }
    model.shapes.clear();
    model.vertex_buffers.clear();

    // Skeleton: optional smart merge, then the palette regeneration every
    // import performs.
    let mut added_bones: Vec<String> = Vec::new();
    if import_bones {
        let imported = toolbox_skeleton::import_skeleton_like_toolbox(fbx)
            .map_err(|error| BfresError::new(0, error.to_string()))?;
        if imported.bones.is_empty() {
            return Err(BfresError::new(0, "the FBX contains no skeleton nodes"));
        }
        let (_, skeleton_report) = skeleton_import::merge_imported_bones(model, &imported.bones);
        added_bones = skeleton_report.added.clone();
        report.skeleton = skeleton_report;
    } else {
        report.skeleton.bones_before = model.skeleton.bones.len();
        report.skeleton.bones_after = model.skeleton.bones.len();
    }
    let usage: Vec<MeshBoneUsage> = objects
        .iter()
        .map(|object| MeshBoneUsage {
            name: object.name.clone(),
            vertices: object
                .vertices
                .iter()
                .map(|vertex| vertex.bone_names.clone())
                .collect(),
        })
        .collect();
    skeleton_import::regenerate_skinning_palette(model, &usage, &mut report.skeleton);
    let transforms = bone_transforms(model, &added_bones)?;

    if model.materials.is_empty() {
        return Err(BfresError::new(
            0,
            "the model has no material to assign the imported meshes to",
        ));
    }

    for object in &mut objects {
        if object.vertices.is_empty() {
            return Err(BfresError::new(
                0,
                format!("0 vertices found on mesh {}", object.name),
            ));
        }
        let force_skin_max = object.vertex_skin_count;
        let name = rename_duplicate(
            &model
                .shapes
                .iter()
                .map(|shape| shape.name.clone())
                .collect::<Vec<_>>(),
            &object.name,
        );
        let material_index = if object.material_index > 0
            && (object.material_index as usize) < model.materials.len()
        {
            object.material_index as u16
        } else {
            0
        };
        let mut attributes =
            settings.create_new_attributes(&model.materials[usize::from(material_index)]);
        create_bone_list(object, model, force_skin_max, &mut attributes)?;
        // ApplyImportSettings: tangents from the material's normal-map UV
        // layer and the bake parameters reset.
        if settings.enable_tangents {
            let uv_index = normal_map_uv_index(&model.materials[usize::from(material_index)]);
            calculate_tangent_bitangent(object, uv_index);
        }
        reset_uv_params(&mut model.materials[usize::from(material_index)]);
        let skin_bone_indices = get_indices(object, model);
        let vertex_skin_count = object
            .vertices
            .iter()
            .map(|vertex| vertex.bone_weights.len())
            .max()
            .unwrap_or(0)
            .min(255) as u8;
        let (bounding, radius) =
            calculate_bounding_box(object, model, &transforms, vertex_skin_count)?;
        optimize_attribute_formats(&mut attributes, vertex_skin_count);

        let vertex_buffer_index = model.vertex_buffers.len() as u16;
        let mesh = save_mesh(&object.faces);
        let vertex_buffer = save_vertex_buffer(
            object,
            model,
            &transforms,
            &attributes,
            vertex_skin_count,
            object.bone_index,
        )?;
        model.vertex_buffers.push(vertex_buffer);
        model.shapes.push(Shape {
            flags: SHAPE_HAS_VERTEX_BUFFER,
            name: name.clone(),
            vertex_buffer_index,
            meshes: vec![mesh],
            skin_bone_indices,
            boundings: vec![bounding, bounding],
            radius_list: vec![[0.0, 0.0, 0.0, radius]],
            material_index,
            bone_index: object.bone_index,
            vertex_skin_count,
            target_attrib_count: 0,
        });
        report.shapes.push(name);
    }
    Ok(report)
}

/// `AssimpData.CreateGenericObject` + `GetVertices` with the node transform
/// applied (positions by the full matrix, normals by its rotation).
fn create_generic_object(mesh: &AssimpMesh, index: usize) -> Result<GenericObject, BfresError> {
    let transform = Mat4::from_assimp(&mesh.node_transform);
    let normals_transform = Mat4::create_from_quaternion(transform.extract_rotation());
    let has_tangents = mesh.has_tangent_basis();
    let mut vertices = Vec::with_capacity(mesh.positions.len());
    for v in 0..mesh.positions.len() {
        let mut vertex = Vertex {
            pos: Vec3::from_array(mesh.positions[v]).transform_position(&transform),
            ..Default::default()
        };
        if !mesh.normals.is_empty() {
            vertex.nrm = Vec3::from_array(mesh.normals[v])
                .transform_normal(&normals_transform)
                .map_err(|error| BfresError::new(0, error))?;
        }
        for (channel, uvs) in mesh.uvs.iter().take(3).enumerate() {
            vertex.uv[channel] = uvs[v];
        }
        if has_tangents {
            let t = mesh.tangents[v];
            let b = mesh.bitangents[v];
            vertex.tan = [t[0], t[1], t[2], 1.0];
            vertex.bitan = [b[0], b[1], b[2], 1.0];
        }
        if let Some(colors) = mesh.colors.first() {
            vertex.col = colors[v];
        }
        if let Some(colors) = mesh.colors.get(1) {
            vertex.col2 = colors[v];
        }
        vertices.push(vertex);
    }
    for bone in &mesh.bones {
        for &(vertex, weight) in &bone.weights {
            if let Some(target) = vertices.get_mut(vertex as usize) {
                target.bone_weights.push(weight);
                target.bone_names.push(bone.name.clone());
            }
        }
    }
    let vertex_skin_count = vertices
        .iter()
        .map(|vertex| vertex.bone_names.len())
        .max()
        .unwrap_or(0)
        .min(255) as u8;
    let faces = mesh.faces.iter().flatten().copied().collect();
    Ok(GenericObject {
        name: if mesh.name.is_empty() {
            format!("Mesh {index}")
        } else {
            mesh.name.clone()
        },
        material_index: mesh.material_index as i32,
        bone_index: 0,
        vertex_skin_count,
        vertices,
        faces,
        has_pos: !mesh.positions.is_empty(),
        has_nrm: !mesh.normals.is_empty(),
        has_uv0: !mesh.uvs.is_empty(),
        has_uv1: mesh.uvs.len() > 1,
        has_uv2: mesh.uvs.len() > 2,
        has_weights: mesh
            .bones
            .first()
            .is_some_and(|bone| !bone.weights.is_empty()),
        has_indices: !mesh.bones.is_empty(),
        has_vertex_colors: !mesh.colors.is_empty(),
        has_tangents,
    })
}

/// `Utils.RenameDuplicateString`: `_0`, `_1`, ... suffixes.
fn rename_duplicate(existing: &[String], name: &str) -> String {
    if !existing.iter().any(|value| value == name) {
        return name.to_owned();
    }
    let mut index = 0;
    loop {
        let candidate = format!("{name}_{index}");
        if !existing.iter().any(|value| *value == candidate) {
            return candidate;
        }
        index += 1;
    }
}

/// `FSHP.CreateBoneList` without a forced skin count: resolves every vertex
/// bone name to a palette slot and sorts the influences by slot.
fn create_bone_list(
    object: &mut GenericObject,
    model: &Model,
    forced_skin_amount: u8,
    attributes: &mut Vec<Attribute>,
) -> Result<(), BfresError> {
    if object.vertex_skin_count == 1 {
        let use_weights = object
            .vertices
            .iter()
            .any(|vertex| !vertex.bone_weights.is_empty());
        if !use_weights {
            for vertex in &mut object.vertices {
                vertex.bone_weights.clear();
            }
            if let Some(position) = attributes.iter().position(|a| a.name == "_w0") {
                attributes.remove(position);
            }
        }
    }
    let use_rigid_skinning = object.vertex_skin_count == 1;
    let node_array = &model.skeleton.matrix_to_bone;
    if node_array.is_empty() {
        return Ok(());
    }
    // First bone of each name in the palette wins.
    let mut palette_bones: Vec<(&str, usize)> = Vec::new();
    for slot in node_array {
        let Some(bone) = model.skeleton.bones.get(usize::from(*slot)) else {
            continue;
        };
        if !palette_bones.iter().any(|(name, _)| *name == bone.name) {
            palette_bones.push((bone.name.as_str(), usize::from(*slot)));
        }
    }
    let find_slot = |name: &str| -> Option<i32> {
        node_array
            .iter()
            .position(|slot| model.skeleton.bones[usize::from(*slot)].name == name)
            .map(|index| index as i32)
    };
    for vertex in &mut object.vertices {
        for bone_name in &vertex.bone_names {
            let Some((_, bone_index)) = palette_bones
                .iter()
                .find(|(name, _)| *name == bone_name.as_str())
            else {
                continue;
            };
            let bone = &model.skeleton.bones[*bone_index];
            if !use_rigid_skinning && bone.smooth_matrix_index != -1 {
                if vertex.bone_ids.len() < usize::from(forced_skin_amount) {
                    if let Some(slot) = find_slot(bone_name) {
                        vertex.bone_ids.push(slot);
                    }
                }
            } else if bone.rigid_matrix_index != -1 {
                vertex.bone_ids.push(i32::from(bone.rigid_matrix_index));
            } else if bone.smooth_matrix_index != -1 {
                vertex.bone_ids.push(i32::from(bone.smooth_matrix_index));
            } else if let Some(slot) = find_slot(bone_name) {
                vertex.bone_ids.push(slot);
            }
        }
        if !vertex.bone_weights.is_empty() {
            let mut envelopes: Vec<(i32, f32)> = Vec::with_capacity(vertex.bone_ids.len());
            for j in 0..vertex.bone_ids.len() {
                let weight = *vertex.bone_weights.get(j).ok_or_else(|| {
                    BfresError::new(
                        0,
                        format!("mesh {} skins to a bone the skeleton lacks", object.name),
                    )
                })?;
                envelopes.push((vertex.bone_ids[j], weight));
            }
            // Sort descending, then a stable ascending order: ascending by id.
            envelopes.sort_by(|a, b| b.0.cmp(&a.0));
            envelopes.sort_by_key(|(id, _)| *id);
            for (j, (id, weight)) in envelopes.into_iter().enumerate() {
                vertex.bone_ids[j] = id;
                vertex.bone_weights[j] = weight;
            }
        }
    }
    Ok(())
}

/// `FMAT.GetNormalMapUVIndex`.
fn normal_map_uv_index(material: &Material) -> usize {
    let option = |key: &str| {
        material
            .options
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    if let Some(value) = option("uking_texture2_texcoord") {
        return value
            .trim()
            .parse::<f32>()
            .map(|v| v as i32)
            .unwrap_or(0)
            .max(0) as usize;
    }
    if let Some(value) = option("o_texture2_texcoord") {
        return value.trim().parse::<i32>().unwrap_or(0).max(0) as usize;
    }
    if let Some(value) = option("cIsEnableNormalMap") {
        if value.trim().parse::<f32>().unwrap_or(0.0) == 1.0 {
            return 1;
        }
    }
    0
}

/// `ResetUVParams`: `gsys_bake_st0` / `gsys_bake_st1` become `{1, 1, 0, 0}`.
fn reset_uv_params(material: &mut Material) {
    for param in &material.shader_params {
        if param.name != "gsys_bake_st0" && param.name != "gsys_bake_st1" {
            continue;
        }
        let start = usize::from(param.data_offset);
        let values = [1.0f32, 1.0, 0.0, 0.0];
        for (index, value) in values.iter().enumerate() {
            let at = start + index * 4;
            if let Some(slot) = material.param_data.get_mut(at..at + 4) {
                slot.copy_from_slice(&value.to_le_bytes());
            }
        }
    }
}

/// `STGenericObject.CalculateTangentBitangent` (per-face accumulation,
/// Gram-Schmidt against the normal, bitangent negated).
fn calculate_tangent_bitangent(object: &mut GenericObject, uv_index: usize) {
    let count = object.vertices.len();
    if count < 3 {
        return;
    }
    let uv_index = uv_index.min(3);
    let mut tangents = vec![Vec3::ZERO; count];
    let mut bitangents = vec![Vec3::ZERO; count];
    let faces = &object.faces;
    let mut i = 0;
    while i < faces.len() {
        if faces.len() <= i + 2 {
            break;
        }
        let (a, b, c) = (
            faces[i] as usize,
            faces[i + 1] as usize,
            faces[i + 2] as usize,
        );
        i += 3;
        let v1 = &object.vertices[a];
        let v2 = &object.vertices[b];
        let v3 = &object.vertices[c];
        let x1 = v2.pos.x - v1.pos.x;
        let x2 = v3.pos.x - v1.pos.x;
        let y1 = v2.pos.y - v1.pos.y;
        let y2 = v3.pos.y - v1.pos.y;
        let z1 = v2.pos.z - v1.pos.z;
        let z2 = v3.pos.z - v1.pos.z;
        let (u1, u2, u3) = (v1.uv[uv_index], v2.uv[uv_index], v3.uv[uv_index]);
        let s1 = u2[0] - u1[0];
        let s2 = u3[0] - u1[0];
        let t1 = u2[1] - u1[1];
        let t2 = u3[1] - u1[1];
        let div = s1 * t2 - s2 * t1;
        let mut r = 1.0f32 / div;
        if r == f32::INFINITY || r == f32::NEG_INFINITY {
            r = 1.0;
        }
        let mut s = Vec3::new(t2 * x1 - t1 * x2, t2 * y1 - t1 * y2, t2 * z1 - t1 * z2).scale(r);
        let mut t = Vec3::new(s1 * x2 - s2 * x1, s1 * y2 - s2 * y1, s1 * z2 - s2 * z1).scale(r);
        let delta = 0.00075f32;
        let same_u = (u1[0] - u2[0]).abs() < delta && (u2[0] - u3[0]).abs() < delta;
        let same_v = (u1[1] - u2[1]).abs() < delta && (u2[1] - u3[1]).abs() < delta;
        if same_u || same_v {
            s = Vec3::new(1.0, 0.0, 0.0);
            t = Vec3::new(0.0, 1.0, 0.0);
        }
        for index in [a, b, c] {
            tangents[index] = Vec3::add(tangents[index], s);
            bitangents[index] = Vec3::add(bitangents[index], t);
        }
    }
    for (index, vertex) in object.vertices.iter_mut().enumerate() {
        let new_tan = tangents[index];
        let new_bitan = bitangents[index];
        let n = vertex.nrm;
        let tan = Vec3::sub(new_tan, n.scale(Vec3::dot(n, new_tan))).normalized();
        let bitan = Vec3::sub(new_bitan, n.scale(Vec3::dot(n, new_bitan))).normalized();
        vertex.tan = [tan.x, tan.y, tan.z, 1.0];
        vertex.bitan = [bitan.x * -1.0, bitan.y * -1.0, bitan.z * -1.0, -1.0];
    }
}

/// `FSHP.GetIndices`: the bones behind every palette slot a vertex uses.
fn get_indices(object: &GenericObject, model: &Model) -> Vec<u16> {
    let mut indices: Vec<u16> = Vec::new();
    for vertex in &object.vertices {
        for id in &vertex.bone_ids {
            let Some(bone) = usize::try_from(*id)
                .ok()
                .and_then(|slot| model.skeleton.matrix_to_bone.get(slot))
            else {
                continue;
            };
            if !indices.contains(bone) {
                indices.push(*bone);
            }
        }
    }
    indices.sort_unstable();
    indices
}

// ---- STBone transforms -----------------------------------------------------

/// `STSkeleton.reset()`: the transform of every bone after the final
/// `update()` (roots multiplied by an identity scale) and its inverse from
/// the reset pass, exactly as Toolbox keeps them.
struct BoneTransforms {
    transform: Vec<Mat4>,
    invert: Vec<Mat4>,
}

fn bone_transforms(model: &Model, added: &[String]) -> Result<BoneTransforms, BfresError> {
    let bones = &model.skeleton.bones;
    let mut locals = Vec::with_capacity(bones.len());
    for bone in bones {
        let rot = if bone.uses_euler() {
            let mut q = quat_from_euler(Vec3::new(
                bone.rotation[0],
                bone.rotation[1],
                bone.rotation[2],
            ));
            // `EulerRotation` setter used by `CloneBaseInstance` for new bones.
            if added.iter().any(|name| *name == bone.name) && q.w == 0.0 {
                q.w = 1.0;
            }
            q
        } else {
            Quat::new(
                bone.rotation[0],
                bone.rotation[1],
                bone.rotation[2],
                bone.rotation[3],
            )
        };
        let local = Mat4::mult(
            &Mat4::mult(
                &Mat4::create_scale(Vec3::from_array(bone.scale)),
                &Mat4::create_from_quaternion(rot),
            ),
            &Mat4::create_translation(Vec3::from_array(bone.position)),
        );
        locals.push(local);
    }
    let parent_of = |index: usize| -> Option<usize> {
        let parent = bones[index].parent_index;
        (parent >= 0 && (parent as usize) < bones.len() && parent as usize != index)
            .then_some(parent as usize)
    };
    // `SkeletonFlagsScaling.Maya` turns segment scale compensation on for
    // every bone read from the file; bones the merge created keep it off.
    let maya = model.skeleton.flags & 0x300 == 0x200;
    let compensate: Vec<bool> = bones
        .iter()
        .map(|bone| maya && !added.iter().any(|name| *name == bone.name))
        .collect();
    struct Chain<'a> {
        locals: &'a [Mat4],
        scales: &'a [[f32; 3]],
        compensate: &'a [bool],
        parent_of: &'a dyn Fn(usize) -> Option<usize>,
    }
    fn resolve(
        chain: &Chain<'_>,
        index: usize,
        cache: &mut Vec<Option<Mat4>>,
        root_scale: bool,
        depth: usize,
    ) -> Mat4 {
        if let Some(value) = cache[index] {
            return value;
        }
        let value = match (chain.parent_of)(index) {
            Some(parent) if depth < 512 => {
                let parent_transform = resolve(chain, parent, cache, root_scale, depth + 1);
                if chain.compensate[index] {
                    let scale = chain.scales[parent];
                    let inverse_scale = Mat4::create_scale(Vec3::new(
                        1.0f32 / scale[0],
                        1.0f32 / scale[1],
                        1.0f32 / scale[2],
                    ));
                    Mat4::mult(
                        &Mat4::mult(&chain.locals[index], &inverse_scale),
                        &parent_transform,
                    )
                } else {
                    Mat4::mult(&chain.locals[index], &parent_transform)
                }
            }
            _ => {
                if root_scale {
                    Mat4::mult(
                        &chain.locals[index],
                        &Mat4::create_scale(Vec3::new(1.0, 1.0, 1.0)),
                    )
                } else {
                    chain.locals[index]
                }
            }
        };
        cache[index] = Some(value);
        value
    }
    let scales: Vec<[f32; 3]> = bones.iter().map(|bone| bone.scale).collect();
    let chain = Chain {
        locals: &locals,
        scales: &scales,
        compensate: &compensate,
        parent_of: &parent_of,
    };
    // update(true): world transforms without the root scale pass.
    let mut reset: Vec<Option<Mat4>> = vec![None; bones.len()];
    let mut invert = Vec::with_capacity(bones.len());
    for index in 0..bones.len() {
        let transform = resolve(&chain, index, &mut reset, false, 0);
        invert.push(transform.inverted().unwrap_or(Mat4 { m: [[0.0; 4]; 4] }));
    }
    let mut cache: Vec<Option<Mat4>> = vec![None; bones.len()];
    let mut transform = Vec::with_capacity(bones.len());
    for index in 0..bones.len() {
        transform.push(resolve(&chain, index, &mut cache, true, 0));
    }
    Ok(BoneTransforms { transform, invert })
}

// ---- Bounding boxes ----------------------------------------------------------

/// .NET `Math.Min(float, float)` / `Math.Max`.
fn net_min(a: f32, b: f32) -> f32 {
    if a < b {
        return a;
    }
    if a.is_nan() {
        return a;
    }
    b
}

fn net_max(a: f32, b: f32) -> f32 {
    if a > b {
        return a;
    }
    if a.is_nan() {
        return a;
    }
    b
}

/// `FSHP.CalculateBoundingBox`: (center xyz, extent xyz) and the radius.
fn calculate_bounding_box(
    object: &GenericObject,
    model: &Model,
    transforms: &BoneTransforms,
    vertex_skin_count: u8,
) -> Result<([f32; 6], f32), BfresError> {
    if vertex_skin_count > 0 {
        // Per-bone boxes in bone space, in first-use order.
        let mut boxes: Vec<(usize, Vec3, Vec3)> = Vec::new();
        for vertex in &object.vertices {
            for id in &vertex.bone_ids {
                let Some(&bone) = usize::try_from(*id)
                    .ok()
                    .and_then(|slot| model.skeleton.matrix_to_bone.get(slot))
                else {
                    continue;
                };
                let bone = usize::from(bone);
                let position = if let Some(entry) = boxes.iter().position(|(b, _, _)| *b == bone) {
                    entry
                } else {
                    boxes.push((
                        bone,
                        Vec3::new(f32::MAX, f32::MAX, f32::MAX),
                        Vec3::new(f32::MIN, f32::MIN, f32::MIN),
                    ));
                    boxes.len() - 1
                };
                let inverted = transforms.transform[bone]
                    .inverted()
                    .map_err(|error| BfresError::new(0, error))?;
                let local = vertex.pos.transform_position(&inverted);
                let (_, min, max) = &mut boxes[position];
                min.x = net_min(min.x, local.x);
                min.y = net_min(min.y, local.y);
                min.z = net_min(min.z, local.z);
                max.x = net_max(max.x, local.x);
                max.y = net_max(max.y, local.y);
                max.z = net_max(max.z, local.z);
            }
        }
        let (min, max) = if boxes.is_empty() {
            (bb_min(object), bb_max(object))
        } else {
            let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
            let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
            for (_, bmin, bmax) in &boxes {
                min.x = net_min(bmin.x, min.x);
                min.y = net_min(bmin.y, min.y);
                min.z = net_min(bmin.z, min.z);
                max.x = net_max(bmax.x, max.x);
                max.y = net_max(bmax.y, max.y);
                max.z = net_max(bmax.z, max.z);
            }
            (min, max)
        };
        let c = Vec3::new(
            (min.x + max.x) / 2.0,
            (min.y + max.y) / 2.0,
            (min.z + max.z) / 2.0,
        );
        let e = Vec3::new(
            (max.x - min.x) / 2.0,
            (max.y - min.y) / 2.0,
            (max.z - min.z) / 2.0,
        );
        let radius = c.length() + e.length();
        return Ok(([c.x, c.y, c.z, e.x, e.y, e.z], radius));
    }
    let min = bb_min(object);
    let max = bb_max(object);
    let center = Vec3::add(max, min);
    let extent = Vec3::new(
        get_extent(max.x, min.x),
        get_extent(max.y, min.y),
        get_extent(max.z, min.z),
    );
    let radius = center.length() + extent.length();
    Ok((
        [center.x, center.y, center.z, extent.x, extent.y, extent.z],
        radius,
    ))
}

fn bb_min(object: &GenericObject) -> Vec3 {
    let mut minimum = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    for vertex in &object.vertices {
        if vertex.pos.x < minimum.x {
            minimum.x = vertex.pos.x;
        }
        if vertex.pos.y < minimum.y {
            minimum.y = vertex.pos.y;
        }
        if vertex.pos.z < minimum.z {
            minimum.z = vertex.pos.z;
        }
    }
    minimum
}

fn bb_max(object: &GenericObject) -> Vec3 {
    let mut maximum = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for vertex in &object.vertices {
        if vertex.pos.x > maximum.x {
            maximum.x = vertex.pos.x;
        }
        if vertex.pos.y > maximum.y {
            maximum.y = vertex.pos.y;
        }
        if vertex.pos.z > maximum.z {
            maximum.z = vertex.pos.z;
        }
    }
    maximum
}

/// `GetExtent`: `(float)Math.Max(Math.Sqrt(max * max), Math.Sqrt(min * min))`.
fn get_extent(max: f32, min: f32) -> f32 {
    let a = f64::from(max * max).sqrt();
    let b = f64::from(min * min).sqrt();
    let value = if a > b {
        a
    } else if a.is_nan() {
        a
    } else {
        b
    };
    value as f32
}

// ---- Attribute formats -------------------------------------------------------

/// `FSHP.OptmizeAttributeFormats`: `_w*` / `_i*` lanes follow the skin count.
fn optimize_attribute_formats(attributes: &mut [Attribute], vertex_skin_count: u8) {
    use format::*;
    for attribute in attributes.iter_mut() {
        let is_weight_or_index =
            attribute.name.starts_with("_w") || attribute.name.starts_with("_i");
        if !is_weight_or_index {
            continue;
        }
        let f = attribute.format;
        attribute.format = match vertex_skin_count {
            1 => match f {
                F32_32_32_32_SINGLE | F32_32_32_SINGLE | F32_32_SINGLE => F32_SINGLE,
                F32_32_32_32_SINT | F32_32_32_SINT | F32_32_SINT => F32_SINT,
                F32_32_32_32_UINT | F32_32_32_UINT | F32_32_UINT => F32_UINT,
                F16_16_16_16_SINGLE | F16_16_SINGLE => F16_SINGLE,
                F16_16_16_16_SINT | F16_16_SINT => F16_SINT,
                F16_16_16_16_UINT | F16_16_UINT => F16_UINT,
                F8_8_8_8_UINT | F8_8_UINT => F8_UINT,
                F8_8_8_8_SINT | F8_8_SINT => F8_SINT,
                F8_8_8_8_SNORM | F8_8_SNORM => F8_SNORM,
                F8_8_8_8_UNORM | F8_8_UNORM => F8_UNORM,
                F8_8_8_8_SINT_TO_SINGLE | F8_8_SINT_TO_SINGLE => F8_SINT_TO_SINGLE,
                F8_8_8_8_UINT_TO_SINGLE | F8_8_UINT_TO_SINGLE => F8_UINT_TO_SINGLE,
                other => other,
            },
            2 => match f {
                F32_32_32_32_SINGLE | F32_32_32_SINGLE | F32_SINGLE => F32_32_SINGLE,
                F32_32_32_32_SINT | F32_32_32_SINT | F32_SINT => F32_32_SINT,
                F32_32_32_32_UINT | F32_32_32_UINT | F32_UINT => F32_32_UINT,
                F16_16_16_16_SINGLE | F16_SINGLE => F16_16_SINGLE,
                F16_16_16_16_SINT | F16_SINT => F16_16_SINT,
                F16_16_16_16_UINT | F16_UINT => F16_16_UINT,
                F8_8_8_8_UINT | F8_UINT => F8_8_UINT,
                F8_8_8_8_SINT | F8_SINT => F8_8_SINT,
                F8_8_8_8_SNORM | F8_SNORM => F8_8_SNORM,
                F8_8_8_8_UNORM | F8_UNORM => F8_8_UNORM,
                F8_8_8_8_SINT_TO_SINGLE | F8_SINT_TO_SINGLE => F8_8_SINT_TO_SINGLE,
                F8_8_8_8_UINT_TO_SINGLE | F8_UINT_TO_SINGLE => F8_8_UINT_TO_SINGLE,
                other => other,
            },
            3 => match f {
                F32_32_32_32_SINGLE | F32_32_SINGLE | F32_SINGLE => F32_32_32_SINGLE,
                F32_32_32_32_SINT | F32_32_SINT | F32_SINT => F32_32_32_SINT,
                F32_32_32_32_UINT | F32_32_UINT | F32_UINT => F32_32_32_UINT,
                other => other,
            },
            0 => f,
            _ => match f {
                F32_32_32_32_SINGLE | F32_32_SINGLE | F32_SINGLE => F32_32_32_32_SINGLE,
                F32_32_32_SINT | F32_32_SINT | F32_SINT => F32_32_32_32_SINT,
                F32_32_32_UINT | F32_32_UINT | F32_UINT => F32_32_32_32_UINT,
                F16_16_SINGLE | F16_SINGLE => F16_16_16_16_SINGLE,
                F16_16_SINT | F16_SINT => F16_16_16_16_SINT,
                F16_16_UINT | F16_UINT => F16_16_16_16_UINT,
                F8_8_UINT | F8_UINT => F8_8_8_8_UINT,
                F8_8_SINT | F8_SINT => F8_8_8_8_SINT,
                F8_8_SNORM | F8_SNORM => F8_8_8_8_SNORM,
                F8_8_UNORM | F8_UNORM => F8_8_8_8_UNORM,
                F8_8_SINT_TO_SINGLE | F8_SINT_TO_SINGLE => F8_8_8_8_SINT_TO_SINGLE,
                F8_8_UINT_TO_SINGLE | F8_UINT_TO_SINGLE => F8_8_8_8_UINT_TO_SINGLE,
                other => other,
            },
        };
    }
}

// ---- Shape / vertex buffer serialization -------------------------------------

/// `BfresSwitch.SaveShape` for the single LOD Assimp delivers.
fn save_mesh(faces: &[u32]) -> Mesh {
    let (index_format, data) = if faces.len() > 65000 {
        (
            INDEX_UINT32,
            faces
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect::<Vec<u8>>(),
        )
    } else {
        (
            INDEX_UINT16,
            faces
                .iter()
                .flat_map(|f| (*f as u16).to_le_bytes())
                .collect::<Vec<u8>>(),
        )
    };
    Mesh {
        submeshes: vec![(0, faces.len() as u32)],
        buffer_flag: 0,
        data,
        primitive_type: PRIMITIVE_TRIANGLES,
        index_format,
        index_count: faces.len() as u32,
        first_vertex: 0,
    }
}

/// `FSHP.TransformLocal` for single-bind and unskinned shapes.
fn transform_local(
    value: Vec3,
    bone_slot: i32,
    is_single_bind: bool,
    is_position: bool,
    model: &Model,
    transforms: &BoneTransforms,
) -> Vec3 {
    let inverse = if is_single_bind {
        let Some(&bone) = usize::try_from(bone_slot)
            .ok()
            .and_then(|slot| model.skeleton.matrix_to_bone.get(slot))
        else {
            return value;
        };
        let bone = usize::from(bone);
        if model.skeleton.bones[bone].rigid_matrix_index == -1 {
            return value;
        }
        transforms.invert[bone]
    } else {
        let Some(bone) = usize::try_from(bone_slot)
            .ok()
            .filter(|b| *b < transforms.invert.len())
        else {
            return value;
        };
        transforms.invert[bone]
    };
    if inverse == (Mat4 { m: [[0.0; 4]; 4] }) {
        return value;
    }
    if is_position {
        value.transform_position(&inverse)
    } else {
        value.transform_normal(&inverse).unwrap_or(value)
    }
}

/// `FSHP.UpdateVertices` + `SaveVertexBuffer` + `VertexBufferHelper.ToVertexBuffer`.
fn save_vertex_buffer(
    object: &mut GenericObject,
    model: &Model,
    transforms: &BoneTransforms,
    attributes: &[Attribute],
    vertex_skin_count: u8,
    bone_index: u16,
) -> Result<VertexBuffer, BfresError> {
    let count = object.vertices.len();
    let skin = usize::from(vertex_skin_count);
    let list_count = skin.div_ceil(4);
    let target_skin = skin.max(4);
    let mut verts = Vec::with_capacity(count);
    let mut norms = Vec::with_capacity(count);
    let mut uv0 = Vec::with_capacity(count);
    let mut uv1 = Vec::with_capacity(count);
    let mut uv2 = Vec::with_capacity(count);
    let mut tans = Vec::with_capacity(count);
    let mut bitans = Vec::with_capacity(count);
    let mut colors = Vec::with_capacity(count);
    let mut colors1 = Vec::with_capacity(count);
    let colors2 = vec![[1.0f32; 4]; count];
    let colors3 = vec![[1.0f32; 4]; count];
    let mut weights: Vec<Vec<[f32; 4]>> = vec![Vec::with_capacity(count); list_count];
    let mut bone_indices: Vec<Vec<[f32; 4]>> = vec![Vec::with_capacity(count); list_count];

    for vertex in &mut object.vertices {
        if vertex_skin_count <= 1 {
            let mut bone_slot = i32::from(bone_index);
            if vertex_skin_count == 1 {
                if let Some(first) = vertex.bone_ids.first() {
                    bone_slot = *first;
                }
            }
            let single = vertex_skin_count == 1;
            vertex.pos = transform_local(vertex.pos, bone_slot, single, true, model, transforms);
            vertex.nrm = transform_local(vertex.nrm, bone_slot, single, false, model, transforms);
            let tan = transform_local(
                Vec3::new(vertex.tan[0], vertex.tan[1], vertex.tan[2]),
                bone_slot,
                single,
                false,
                model,
                transforms,
            );
            vertex.tan = [tan.x, tan.y, tan.z, vertex.tan[3]];
            let bitan = transform_local(
                Vec3::new(vertex.bitan[0], vertex.bitan[1], vertex.bitan[2]),
                bone_slot,
                single,
                false,
                model,
                transforms,
            );
            vertex.bitan = [bitan.x, bitan.y, bitan.z, vertex.bitan[3]];
        }
        verts.push([vertex.pos.x, vertex.pos.y, vertex.pos.z, 1.0]);
        norms.push([vertex.nrm.x, vertex.nrm.y, vertex.nrm.z, 0.0]);
        uv0.push([
            vertex.uv[0][0],
            vertex.uv[0][1],
            vertex.uv[1][0],
            vertex.uv[1][1],
        ]);
        uv1.push([vertex.uv[1][0], vertex.uv[1][1], 0.0, 0.0]);
        uv2.push([
            vertex.uv[2][0],
            vertex.uv[2][1],
            vertex.uv[3][0],
            vertex.uv[3][1],
        ]);
        tans.push(vertex.tan);
        bitans.push(vertex.bitan);
        colors.push(vertex.col);
        colors1.push(vertex.col2);

        // Weight quantization "identical to BFRES_Vertex.py".
        let mut weights_a = vec![0.0f32; target_skin];
        let mut indices_a = vec![0i32; target_skin];
        for i in 0..skin {
            if let Some(weight) = vertex.bone_weights.get(i) {
                weights_a[i] = *weight;
            }
        }
        let mut max_weight = 255i32;
        for (i, slot) in weights_a.iter_mut().enumerate() {
            if skin < i + 1 || vertex.bone_weights.len() < i + 1 {
                *slot = 0.0;
                max_weight = 0;
            } else {
                let mut weight = (vertex.bone_weights[i] * 255.0f32) as i32;
                if vertex.bone_weights.len() == i + 1 {
                    weight = max_weight;
                }
                if weight >= max_weight {
                    weight = max_weight;
                    max_weight = 0;
                } else {
                    max_weight -= weight;
                }
                *slot = weight as f32 / 255.0f32;
            }
        }
        for i in 0..skin {
            if let Some(id) = vertex.bone_ids.get(i) {
                indices_a[i] = *id;
            }
        }
        let mut list_index = 0usize;
        let mut lane = 0usize;
        let mut weight4 = [0.0f32; 4];
        let mut index4 = [0.0f32; 4];
        for i in 0..target_skin {
            weight4[lane] = weights_a[i];
            index4[lane] = indices_a[i] as f32;
            if lane == 3 || i == target_skin - 1 {
                if let Some(list) = weights.get_mut(list_index) {
                    list.push(weight4);
                }
                if let Some(list) = bone_indices.get_mut(list_index) {
                    list.push(index4);
                }
                weight4 = [0.0; 4];
                index4 = [0.0; 4];
                list_index += 1;
                lane = 0;
                continue;
            }
            lane += 1;
        }
    }

    let mut helper: Vec<(String, u16, &[[f32; 4]])> = Vec::new();
    for attribute in attributes {
        let data: Option<&[[f32; 4]]> = match attribute.name.as_str() {
            "_p0" => Some(&verts),
            "_n0" => Some(&norms),
            "_u0" | "_g3d_02_u0_u1" => Some(&uv0),
            "_u1" => Some(&uv1),
            "_u2" | "_g3d_02_u2_u3" => Some(&uv2),
            "_b0" => Some(&bitans),
            "_t0" => Some(&tans),
            "_c0" => Some(&colors),
            "_c1" => Some(&colors1),
            "_c2" => Some(&colors2),
            "_c3" => Some(&colors3),
            name => {
                let mut found = None;
                for i in 0..weights.len() {
                    if name == format!("_w{i}") {
                        found = Some(weights[i].as_slice());
                    }
                    if name == format!("_i{i}") {
                        found = Some(bone_indices[i].as_slice());
                    }
                }
                found
            }
        };
        if let Some(data) = data {
            helper.push((attribute.name.clone(), attribute.format, data));
        }
    }
    if helper.is_empty() {
        return Err(BfresError::new(0, "Attributes are empty?"));
    }
    let mut buffer = VertexBuffer {
        flags: 0,
        attributes: Vec::with_capacity(helper.len()),
        buffers: Vec::with_capacity(helper.len()),
        vertex_count: helper[0].2.len() as u32,
        vertex_skin_count: u16::from(vertex_skin_count),
        gpu_alignment: GPU_ALIGNMENT,
    };
    for (name, format, data) in helper {
        if data.len() != buffer.vertex_count as usize {
            return Err(BfresError::new(
                0,
                "Attribute data arrays have different sizes.",
            ));
        }
        let stride = format_size(format).ok_or_else(|| {
            BfresError::new(0, format!("unsupported vertex format 0x{format:04X}"))
        })?;
        let mut bytes = Vec::with_capacity(data.len() * stride);
        for value in data {
            encode_attribute(format, *value, &mut bytes)?;
        }
        buffer.attributes.push(VertexAttrib {
            name,
            format: format.to_be_bytes(),
            offset: 0,
            buffer_index: buffer.buffers.len() as u16,
        });
        buffer.buffers.push(VertexData {
            data: bytes,
            stride: stride as u32,
        });
    }
    Ok(buffer)
}

// ---- Syroot BinaryDataWriterExtensions ----------------------------------------

/// `VertexBufferHelperAttrib.FormatSize`.
fn format_size(format: u16) -> Option<usize> {
    use format::*;
    Some(match format {
        F8_UNORM | F8_UINT | F8_SNORM | F8_SINT | F8_UINT_TO_SINGLE | F8_SINT_TO_SINGLE => 1,
        F16_UNORM | F16_UINT | F16_SNORM | F16_SINT | F16_SINGLE => 2,
        F8_8_UNORM | F8_8_UINT | F8_8_SNORM | F8_8_SINT | F8_8_UINT_TO_SINGLE
        | F8_8_SINT_TO_SINGLE => 2,
        F16_16_UNORM | F16_16_SNORM | F16_16_UINT | F16_16_SINT | F16_16_SINGLE => 4,
        F8_8_8_8_UNORM
        | F8_8_8_8_SNORM
        | F8_8_8_8_UINT
        | F8_8_8_8_SINT
        | F8_8_8_8_UINT_TO_SINGLE
        | F8_8_8_8_SINT_TO_SINGLE => 4,
        F10_10_10_2_UNORM | F10_10_10_2_UINT | F10_10_10_2_SNORM | F10_10_10_2_SINT => 4,
        F32_UINT | F32_SINT | F32_SINGLE => 4,
        F16_16_16_16_UNORM | F16_16_16_16_SNORM | F16_16_16_16_UINT | F16_16_16_16_SINT
        | F16_16_16_16_SINGLE => 8,
        F32_32_UINT | F32_32_SINT | F32_32_SINGLE => 8,
        F32_32_32_UINT | F32_32_32_SINT | F32_32_32_SINGLE => 12,
        F32_32_32_32_UINT | F32_32_32_32_SINT | F32_32_32_32_SINGLE => 16,
        _ => return None,
    })
}

/// `Algebra.Clamp`.
fn clamp(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// `(int)value` / `(uint)value` in .NET: truncation toward zero.
fn to_int(value: f32) -> i32 {
    value as i32
}

fn to_int64(value: f32) -> i64 {
    value as i64
}

/// `Half` (Jeroen van der Zijp's table conversion, truncating).
pub fn float_to_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let index = ((bits >> 23) & 0x1ff) as usize;
    let exponent = (index & 0xff) as i32 - 127;
    let sign: u16 = if index & 0x100 != 0 { 0x8000 } else { 0 };
    let (base, shift): (u16, u32) = if exponent < -24 {
        (0, 24)
    } else if exponent < -14 {
        (
            (0x0400u32 >> (-exponent - 14)) as u16,
            (-exponent - 1) as u32,
        )
    } else if exponent <= 15 {
        (((exponent + 15) << 10) as u16, 13)
    } else if exponent < 128 {
        (0x7C00, 24)
    } else {
        (0x7C00, 13)
    };
    (base | sign).wrapping_add(((bits & 0x007f_ffff) >> shift) as u16)
}

/// `BinaryDataWriterExtensions.Write(Vector4F)` for one attribute format.
fn encode_attribute(format: u16, v: [f32; 4], out: &mut Vec<u8>) -> Result<(), BfresError> {
    use format::*;
    let unorm8 = |x: f32| (clamp(x, 0.0, 1.0) * 255.0f32) as u8;
    let uint8 = |x: f32| to_int64(x) as u8;
    let snorm8 = |x: f32| (clamp(x, -1.0, 1.0) * 127.0f32) as i8 as u8;
    let sint8 = |x: f32| to_int(x) as i8 as u8;
    let unorm16 = |x: f32| ((clamp(x, 0.0, 1.0) * 65535.0f32) as u16).to_le_bytes();
    let uint16 = |x: f32| (to_int64(x) as u16).to_le_bytes();
    let snorm16 = |x: f32| ((clamp(x, -1.0, 1.0) * 32767.0f32) as i16).to_le_bytes();
    let sint16 = |x: f32| (to_int(x) as i16).to_le_bytes();
    let half = |x: f32| float_to_half(x).to_le_bytes();
    let uint32 = |x: f32| (to_int64(x) as u32).to_le_bytes();
    let sint32 = |x: f32| to_int(x).to_le_bytes();
    let single = |x: f32| x.to_le_bytes();
    let lanes = |n: usize| &v[..n];
    match format {
        F8_UNORM => out.push(unorm8(v[0])),
        F8_UINT => out.push(uint8(v[0])),
        F8_SNORM => out.push(snorm8(v[0])),
        F8_SINT => out.push(sint8(v[0])),
        F8_8_UNORM => out.extend(lanes(2).iter().map(|x| unorm8(*x))),
        F8_8_UINT => out.extend(lanes(2).iter().map(|x| uint8(*x))),
        F8_8_SNORM => out.extend(lanes(2).iter().map(|x| snorm8(*x))),
        F8_8_SINT => out.extend(lanes(2).iter().map(|x| sint8(*x))),
        F8_8_8_8_UNORM => out.extend(lanes(4).iter().map(|x| unorm8(*x))),
        F8_8_8_8_UINT => out.extend(lanes(4).iter().map(|x| uint8(*x))),
        F8_8_8_8_SNORM => out.extend(lanes(4).iter().map(|x| snorm8(*x))),
        F8_8_8_8_SINT => out.extend(lanes(4).iter().map(|x| sint8(*x))),
        F16_UNORM => out.extend(unorm16(v[0])),
        F16_UINT => out.extend(uint16(v[0])),
        F16_SNORM => out.extend(snorm16(v[0])),
        F16_SINT => out.extend(sint16(v[0])),
        F16_SINGLE => out.extend(half(v[0])),
        F16_16_UNORM => lanes(2).iter().for_each(|x| out.extend(unorm16(*x))),
        F16_16_UINT => lanes(2).iter().for_each(|x| out.extend(uint16(*x))),
        F16_16_SNORM => lanes(2).iter().for_each(|x| out.extend(snorm16(*x))),
        F16_16_SINT => lanes(2).iter().for_each(|x| out.extend(sint16(*x))),
        F16_16_SINGLE => lanes(2).iter().for_each(|x| out.extend(half(*x))),
        F16_16_16_16_UNORM => lanes(4).iter().for_each(|x| out.extend(unorm16(*x))),
        F16_16_16_16_UINT => lanes(4).iter().for_each(|x| out.extend(uint16(*x))),
        F16_16_16_16_SNORM => lanes(4).iter().for_each(|x| out.extend(snorm16(*x))),
        F16_16_16_16_SINT => lanes(4).iter().for_each(|x| out.extend(sint16(*x))),
        F16_16_16_16_SINGLE => lanes(4).iter().for_each(|x| out.extend(half(*x))),
        F32_UINT => out.extend(uint32(v[0])),
        F32_SINT => out.extend(sint32(v[0])),
        F32_SINGLE => out.extend(single(v[0])),
        F32_32_UINT => lanes(2).iter().for_each(|x| out.extend(uint32(*x))),
        F32_32_SINT => lanes(2).iter().for_each(|x| out.extend(sint32(*x))),
        F32_32_SINGLE => lanes(2).iter().for_each(|x| out.extend(single(*x))),
        F32_32_32_UINT => lanes(3).iter().for_each(|x| out.extend(uint32(*x))),
        F32_32_32_SINT => lanes(3).iter().for_each(|x| out.extend(sint32(*x))),
        F32_32_32_SINGLE => lanes(3).iter().for_each(|x| out.extend(single(*x))),
        F32_32_32_32_UINT => lanes(4).iter().for_each(|x| out.extend(uint32(*x))),
        F32_32_32_32_SINT => lanes(4).iter().for_each(|x| out.extend(sint32(*x))),
        F32_32_32_32_SINGLE => lanes(4).iter().for_each(|x| out.extend(single(*x))),
        F10_10_10_2_SNORM => {
            // SingleToInt10: `(uint)value << 22 >> 22 & 0x3ff` on the
            // truncated value; SingleToInt2 on the 0..1 clamped W.
            let lane = |x: f32| (to_int64(clamp(x, -1.0, 1.0) * 511.0f32) as u32) & 0x3ff;
            let w = (to_int64(clamp(v[3], 0.0, 1.0)) as u32) & 0x3;
            let packed = lane(v[0]) | (lane(v[1]) << 10) | (lane(v[2]) << 20) | (w << 30);
            out.extend(packed.to_le_bytes());
        }
        F10_10_10_2_UNORM => {
            let lane = |x: f32| (to_int64(clamp(x, 0.0, 1.0) * 1023.0f32) as u32) & 0x3ff;
            let w = (to_int64(clamp(v[3], 0.0, 1.0) * 3.0f32) as u32) & 0x3;
            let packed = lane(v[0]) | (lane(v[1]) << 10) | (lane(v[2]) << 20) | (w << 30);
            out.extend(packed.to_le_bytes());
        }
        F10_10_10_2_UINT | F10_10_10_2_SINT => {
            let lane = |x: f32| (to_int64(x) as u32) & 0x3ff;
            let w = (to_int64(v[3]) as u32) & 0x3;
            let packed = lane(v[0]) | (lane(v[1]) << 10) | (lane(v[2]) << 20) | (w << 30);
            out.extend(packed.to_le_bytes());
        }
        other => {
            return Err(BfresError::new(
                0,
                format!("unsupported vertex format 0x{other:04X}"),
            ))
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_conversion_truncates_like_syroot() {
        assert_eq!(float_to_half(1.0), 0x3C00);
        assert_eq!(float_to_half(-2.0), 0xC000);
        assert_eq!(float_to_half(0.0), 0x0000);
        assert_eq!(float_to_half(65504.0), 0x7BFF);
        // 1 + 1/1024 + tiny is truncated, never rounded up.
        assert_eq!(float_to_half(1.000_976_6 + 1e-6), 0x3C01);
        assert_eq!(float_to_half(0.333_333_34), 0x3555);
    }

    #[test]
    fn rename_duplicate_appends_counters() {
        let existing = vec!["a".to_owned(), "a_0".to_owned()];
        assert_eq!(rename_duplicate(&existing, "a"), "a_1");
        assert_eq!(rename_duplicate(&existing, "b"), "b");
    }
}
