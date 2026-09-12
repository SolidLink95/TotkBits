//! The skeleton Switch Toolbox builds from an FBX when "Import Bones" is
//! checked, reproduced step by step: Assimp's node transformation chain,
//! Toolbox's `AssimpData.BuildSkeletonNodes` / `CreateByNode` traversal and
//! `STBone.FromTransform` (OpenTK decomposition + `STMath.ToEulerAngles`).
//! Every float operation keeps the precision and association order of the
//! C++/C# originals so the resulting bone values match bit for bit.

use fbxcel::tree::v7400::NodeHandle;
use fbxcel_dom::{
    any::AnyDocument,
    v7400::object::{geometry::TypedGeometryHandle, model::TypedModelHandle, TypedObjectHandle},
};
use std::{
    collections::{HashMap, HashSet},
    io,
};

/// One bone exactly as Toolbox's `STBone` holds it after the import.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolboxImportedBone {
    pub name: String,
    /// Index into the same list, `-1` for the root.
    pub parent_index: i32,
    /// `Matrix4.ExtractTranslation()`.
    pub position: [f32; 3],
    /// `Matrix4.ExtractScale()`.
    pub scale: [f32; 3],
    /// `Matrix4.ExtractRotation()` as (x, y, z, w).
    pub rotation: [f32; 4],
    /// `STMath.ToEulerAngles(rotation)`.
    pub euler: [f32; 3],
}

#[derive(Clone, Debug, Default)]
pub struct ToolboxImportedSkeleton {
    /// Bones in Toolbox's traversal order (depth first, children in FBX
    /// connection order).
    pub bones: Vec<ToolboxImportedBone>,
    /// One entry per FBX mesh, in file order.
    pub mesh_bone_usage: Vec<MeshBoneUsage>,
}

/// The bone names Assimp reports for every control point of one mesh after
/// `LimitBoneWeights` (at most four, heaviest first).
#[derive(Clone, Debug, Default)]
pub struct MeshBoneUsage {
    /// Assimp's mesh name: the geometry name, or the model node's name when
    /// the geometry is unnamed.
    pub name: String,
    pub vertices: Vec<Vec<String>>,
}

type Matrix = [[f32; 4]; 4];

const IDENTITY: Matrix = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// `ai_epsilon`.
const ZERO_EPSILON: f32 = 1e-6;
/// `AI_DEG_TO_RAD`.
const DEG_TO_RAD: f32 = 0.017_453_292_5;
/// Assimp's `LimitBoneWeights` default.
const MAX_WEIGHTS: usize = 4;

struct FbxModel<'a> {
    name: String,
    node: NodeHandle<'a>,
}

pub fn import_skeleton_like_toolbox(data: &[u8]) -> io::Result<ToolboxImportedSkeleton> {
    let document = AnyDocument::from_seekable_reader(std::io::Cursor::new(data))
        .map_err(|error| invalid(error.to_string()))?;
    let AnyDocument::V7400(_, document) = document else {
        return Err(invalid("unsupported FBX document version"));
    };

    // Every model node (limbs, nulls, meshes) keyed by object id, plus the
    // parent link Assimp uses to build the node tree.
    let mut models: HashMap<i64, FbxModel<'_>> = HashMap::new();
    let mut model_order = Vec::new();
    let mut parent_of: HashMap<i64, i64> = HashMap::new();
    for object in document.objects() {
        let TypedObjectHandle::Model(model) = object.get_typed() else {
            continue;
        };
        let id = object.object_id().raw();
        models.insert(
            id,
            FbxModel {
                name: model.name().unwrap_or("").to_string(),
                node: object.node(),
            },
        );
        model_order.push(id);
        if let Some(parent) = model.parent_model() {
            parent_of.insert(id, parent.object_id().raw());
        }
    }
    // Children in the order of the file's Connections section, which is how
    // Assimp sequences a node's children.
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut seen_child = HashSet::new();
    if let Some(connections) = document
        .tree()
        .root()
        .children_by_name("Connections")
        .next()
    {
        for connection in connections.children_by_name("C") {
            let attributes = connection.attributes();
            let kind = attributes.first().and_then(|value| value.get_string());
            if kind != Some("OO") {
                continue;
            }
            let (Some(child), Some(parent)) = (
                attributes.get(1).and_then(|value| value.get_i64()),
                attributes.get(2).and_then(|value| value.get_i64()),
            ) else {
                continue;
            };
            if !models.contains_key(&child) || (parent != 0 && !models.contains_key(&parent)) {
                continue;
            }
            if seen_child.insert(child) {
                children.entry(parent).or_default().push(child);
            }
        }
    }
    for id in &model_order {
        if !seen_child.contains(id) {
            let parent = parent_of.get(id).copied().unwrap_or(0);
            children.entry(parent).or_default().push(*id);
        }
    }

    // `GetSklRoot`: the first node (depth first) that is called Root or looks
    // like a skeleton root. Without one the whole scene becomes the skeleton.
    let mut bones = Vec::new();
    let root_children = children.get(&0).cloned().unwrap_or_default();
    match find_skeleton_root(&root_children, &children, &models) {
        Some(root) => {
            build_skeleton_nodes(root, &children, &models, IDENTITY, &mut bones)?;
        }
        None => {
            // Assimp's root node is called "RootNode", which Toolbox treats
            // as a bone, so every model node becomes part of the skeleton.
            push_bone("RootNode".to_owned(), -1, IDENTITY, &mut bones);
            for child in root_children {
                create_by_node(child, 0, &children, &models, &mut bones)?;
            }
        }
    }

    let mesh_bone_usage = mesh_bone_usage(&document, &models)?;
    Ok(ToolboxImportedSkeleton {
        bones,
        mesh_bone_usage,
    })
}

fn find_skeleton_root(
    nodes: &[i64],
    children: &HashMap<i64, Vec<i64>>,
    models: &HashMap<i64, FbxModel<'_>>,
) -> Option<i64> {
    for id in nodes {
        let name = models
            .get(id)
            .map(|model| model.name.as_str())
            .unwrap_or("");
        if name.contains("Skl_Root")
            || name.contains("nw4f_root")
            || name.contains("skl_root")
            || name == "Root"
        {
            return Some(*id);
        }
        if let Some(found) = children
            .get(id)
            .and_then(|next| find_skeleton_root(next, children, models))
        {
            return Some(found);
        }
    }
    None
}

/// `BuildSkeletonNodes`: `root_transform` is the scene root transform, which
/// Toolbox passes down unchanged until it meets the first bone.
fn build_skeleton_nodes(
    id: i64,
    children: &HashMap<i64, Vec<i64>>,
    models: &HashMap<i64, FbxModel<'_>>,
    root_transform: Matrix,
    bones: &mut Vec<ToolboxImportedBone>,
) -> io::Result<()> {
    let model = models
        .get(&id)
        .ok_or_else(|| invalid("FBX connection references a missing model"))?;
    let local = node_transform(&model.node)?;
    let world = multiply(&root_transform, &local);
    let name = model.name.as_str();
    let is_bone = name.contains("Skl_Root")
        || name.contains("nw4f_root")
        || name.contains("skl_root")
        || name.contains("all_root")
        || name.contains("_root")
        || name.contains("Root");
    if is_bone {
        push_bone(model.name.clone(), -1, world, bones);
        let index = bones.len() - 1;
        for child in children.get(&id).cloned().unwrap_or_default() {
            create_by_node(child, index, children, models, bones)?;
        }
    } else if name == "skeleton_root" {
        if let Some(first) = children.get(&id).and_then(|next| next.first()).copied() {
            let first_model = models
                .get(&first)
                .ok_or_else(|| invalid("FBX connection references a missing model"))?;
            let first_world = multiply(&world, &node_transform(&first_model.node)?);
            push_bone(first_model.name.clone(), -1, first_world, bones);
            let index = bones.len() - 1;
            for child in children.get(&first).cloned().unwrap_or_default() {
                create_by_node(child, index, children, models, bones)?;
            }
        }
    } else {
        for child in children.get(&id).cloned().unwrap_or_default() {
            build_skeleton_nodes(child, children, models, world, bones)?;
        }
    }
    Ok(())
}

/// `CreateByNode` for a non-root bone: the local node transform only.
fn create_by_node(
    id: i64,
    parent_index: usize,
    children: &HashMap<i64, Vec<i64>>,
    models: &HashMap<i64, FbxModel<'_>>,
    bones: &mut Vec<ToolboxImportedBone>,
) -> io::Result<()> {
    let model = models
        .get(&id)
        .ok_or_else(|| invalid("FBX connection references a missing model"))?;
    if bones.iter().any(|bone| bone.name == model.name) {
        // "Duplicate node name found for bone": Toolbox skips the node but
        // still descends into its children, which then hang off it by name.
        for child in children.get(&id).cloned().unwrap_or_default() {
            create_by_node(child, parent_index, children, models, bones)?;
        }
        return Ok(());
    }
    let local = node_transform(&model.node)?;
    push_bone(model.name.clone(), parent_index as i32, local, bones);
    let index = bones.len() - 1;
    for child in children.get(&id).cloned().unwrap_or_default() {
        create_by_node(child, index, children, models, bones)?;
    }
    Ok(())
}

fn push_bone(name: String, parent_index: i32, world: Matrix, bones: &mut Vec<ToolboxImportedBone>) {
    let (position, scale, rotation, euler) = from_transform(&world);
    bones.push(ToolboxImportedBone {
        name,
        parent_index,
        position,
        scale,
        rotation,
        euler,
    });
}

// ---- Assimp: FBXConverter::GenerateTransformationChain ---------------------

fn node_transform(node: &NodeHandle<'_>) -> io::Result<Matrix> {
    let properties = read_properties(node);
    let vector = |name: &str| properties.get(name).copied();
    let rotation_order = properties
        .get("RotationOrder")
        .map(|value| value[0] as i32)
        .unwrap_or(0);

    // Chain order: Translation, RotationOffset, RotationPivot, PreRotation,
    // Rotation, PostRotation, RotationPivotInverse, ScalingOffset,
    // ScalingPivot, Scaling, ScalingPivotInverse.
    let mut chain: Vec<Matrix> = Vec::new();
    if let Some(value) = vector("Lcl Translation") {
        if square_length(value) > ZERO_EPSILON {
            chain.push(translation(value));
        }
    }
    if let Some(value) = vector("RotationOffset") {
        if square_length(value) > ZERO_EPSILON {
            chain.push(translation(value));
        }
    }
    let rotation_pivot = vector("RotationPivot").filter(|v| square_length(*v) > ZERO_EPSILON);
    if let Some(value) = rotation_pivot {
        chain.push(translation(value));
    }
    if let Some(value) = vector("PreRotation") {
        if square_length(value) > ZERO_EPSILON {
            chain.push(rotation_matrix(0, value));
        }
    }
    if let Some(value) = vector("Lcl Rotation") {
        if square_length(value) > ZERO_EPSILON {
            chain.push(rotation_matrix(rotation_order, value));
        }
    }
    if let Some(value) = vector("PostRotation") {
        if square_length(value) > ZERO_EPSILON {
            chain.push(rotation_matrix(0, value));
        }
    }
    if let Some(value) = rotation_pivot {
        chain.push(translation([-value[0], -value[1], -value[2]]));
    }
    if let Some(value) = vector("ScalingOffset") {
        if square_length(value) > ZERO_EPSILON {
            chain.push(translation(value));
        }
    }
    let scaling_pivot = vector("ScalingPivot").filter(|v| square_length(*v) > ZERO_EPSILON);
    if let Some(value) = scaling_pivot {
        chain.push(translation(value));
    }
    if let Some(value) = vector("Lcl Scaling") {
        if (square_length(value) - 3.0f32).abs() > ZERO_EPSILON {
            chain.push(scaling(value));
        }
    }
    if let Some(value) = scaling_pivot {
        chain.push(translation([-value[0], -value[1], -value[2]]));
    }
    let mut transform = IDENTITY;
    for link in &chain {
        transform = multiply(&transform, link);
    }
    Ok(transform)
}

/// `Properties70/P` entries with three numeric values, read as `aiVector3D`.
fn read_properties(node: &NodeHandle<'_>) -> HashMap<String, [f32; 3]> {
    let mut properties = HashMap::new();
    let Some(block) = node.children_by_name("Properties70").next() else {
        return properties;
    };
    for property in block.children_by_name("P") {
        let attributes = property.attributes();
        let Some(name) = attributes.first().and_then(|value| value.get_string()) else {
            continue;
        };
        let number = |index: usize| -> Option<f32> {
            attributes.get(index).and_then(|value| {
                value
                    .get_f64()
                    .map(|v| v as f32)
                    .or_else(|| value.get_i32().map(|v| v as f32))
                    .or_else(|| value.get_i64().map(|v| v as f32))
            })
        };
        match (number(4), number(5), number(6)) {
            (Some(x), Some(y), Some(z)) => {
                properties.insert(name.to_owned(), [x, y, z]);
            }
            (Some(x), None, None) => {
                properties.insert(name.to_owned(), [x, 0.0, 0.0]);
            }
            _ => {}
        }
    }
    properties
}

fn square_length(v: [f32; 3]) -> f32 {
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
}

fn translation(v: [f32; 3]) -> Matrix {
    let mut m = IDENTITY;
    m[0][3] = v[0];
    m[1][3] = v[1];
    m[2][3] = v[2];
    m
}

fn scaling(v: [f32; 3]) -> Matrix {
    let mut m = IDENTITY;
    m[0][0] = v[0];
    m[1][1] = v[1];
    m[2][2] = v[2];
    m
}

fn rotation_x(angle: f32) -> Matrix {
    let (s, c) = (angle.sin(), angle.cos());
    let mut m = IDENTITY;
    m[1][1] = c;
    m[1][2] = -s;
    m[2][1] = s;
    m[2][2] = c;
    m
}

fn rotation_y(angle: f32) -> Matrix {
    let (s, c) = (angle.sin(), angle.cos());
    let mut m = IDENTITY;
    m[0][0] = c;
    m[0][2] = s;
    m[2][0] = -s;
    m[2][2] = c;
    m
}

fn rotation_z(angle: f32) -> Matrix {
    let (s, c) = (angle.sin(), angle.cos());
    let mut m = IDENTITY;
    m[0][0] = c;
    m[0][1] = -s;
    m[1][0] = s;
    m[1][1] = c;
    m
}

/// `FBXConverter::GetRotationMatrix`: degrees in, the three axis matrices
/// multiplied in the order the rotation mode dictates.
fn rotation_matrix(order: i32, degrees: [f32; 3]) -> Matrix {
    let angle_epsilon = ZERO_EPSILON;
    let mut temp = [IDENTITY; 3];
    let mut is_identity = [true; 3];
    if degrees[2].abs() > angle_epsilon {
        temp[2] = rotation_z(degrees[2] * DEG_TO_RAD);
        is_identity[2] = false;
    }
    if degrees[1].abs() > angle_epsilon {
        temp[1] = rotation_y(degrees[1] * DEG_TO_RAD);
        is_identity[1] = false;
    }
    if degrees[0].abs() > angle_epsilon {
        temp[0] = rotation_x(degrees[0] * DEG_TO_RAD);
        is_identity[0] = false;
    }
    let sequence: [usize; 3] = match order {
        1 => [1, 2, 0], // XZY
        2 => [0, 2, 1], // YZX
        3 => [2, 0, 1], // YXZ
        4 => [1, 0, 2], // ZXY
        5 => [0, 1, 2], // ZYX
        _ => [2, 1, 0], // XYZ
    };
    let mut out = IDENTITY;
    for axis in sequence {
        if !is_identity[axis] {
            out = multiply(&out, &temp[axis]);
        }
    }
    out
}

/// `aiMatrix4x4 * aiMatrix4x4` (row major, column vectors).
fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [[0.0f32; 4]; 4];
    for (row, out_row) in out.iter_mut().enumerate() {
        for (column, cell) in out_row.iter_mut().enumerate() {
            *cell = a[row][0] * b[0][column]
                + a[row][1] * b[1][column]
                + a[row][2] * b[2][column]
                + a[row][3] * b[3][column];
        }
    }
    out
}

// ---- Toolbox: AssimpHelper.TKMatrix + STBone.FromTransform -----------------

/// The OpenTK matrix Toolbox works on is the transposed Assimp matrix, so
/// its rows are the Assimp columns and the translation sits in row 3.
fn from_transform(assimp: &Matrix) -> ([f32; 3], [f32; 3], [f32; 4], [f32; 3]) {
    let row = |i: usize| [assimp[0][i], assimp[1][i], assimp[2][i]];
    let row0 = row(0);
    let row1 = row(1);
    let row2 = row(2);
    let position = [assimp[0][3], assimp[1][3], assimp[2][3]];
    let scale = [length(row0), length(row1), length(row2)];
    let rotation = extract_rotation(normalized(row0), normalized(row1), normalized(row2));
    let euler = to_euler_angles(rotation);
    (position, scale, rotation, euler)
}

/// OpenTK `Vector3.Length`: float squares, double sqrt, float result.
fn length(v: [f32; 3]) -> f32 {
    (f64::from(v[0] * v[0] + v[1] * v[1] + v[2] * v[2])).sqrt() as f32
}

fn normalized(v: [f32; 3]) -> [f32; 3] {
    let scale = 1.0f32 / length(v);
    [v[0] * scale, v[1] * scale, v[2] * scale]
}

/// OpenTK 3 `Matrix4.ExtractRotation(row_normalise: true)`.
fn extract_rotation(row0: [f32; 3], row1: [f32; 3], row2: [f32; 3]) -> [f32; 4] {
    let (mut x, mut y, mut z, mut w) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let trace = 0.25 * (f64::from(row0[0] + row1[1] + row2[2]) + 1.0);
    if trace > 0.0 {
        let mut sq = trace.sqrt();
        w = sq as f32;
        sq = 1.0 / (4.0 * sq);
        x = (f64::from(row1[2] - row2[1]) * sq) as f32;
        y = (f64::from(row2[0] - row0[2]) * sq) as f32;
        z = (f64::from(row0[1] - row1[0]) * sq) as f32;
    } else if row0[0] > row1[1] && row0[0] > row2[2] {
        let mut sq =
            2.0 * (1.0 + f64::from(row0[0]) - f64::from(row1[1]) - f64::from(row2[2])).sqrt();
        x = (0.25 * sq) as f32;
        sq = 1.0 / sq;
        w = (f64::from(row2[1] - row1[2]) * sq) as f32;
        y = (f64::from(row1[0] + row0[1]) * sq) as f32;
        z = (f64::from(row2[0] + row0[2]) * sq) as f32;
    } else if row1[1] > row2[2] {
        let mut sq =
            2.0 * (1.0 + f64::from(row1[1]) - f64::from(row0[0]) - f64::from(row2[2])).sqrt();
        y = (0.25 * sq) as f32;
        sq = 1.0 / sq;
        w = (f64::from(row2[0] - row0[2]) * sq) as f32;
        x = (f64::from(row1[0] + row0[1]) * sq) as f32;
        z = (f64::from(row2[1] + row1[2]) * sq) as f32;
    } else {
        let mut sq =
            2.0 * (1.0 + f64::from(row2[2]) - f64::from(row0[0]) - f64::from(row1[1])).sqrt();
        z = (0.25 * sq) as f32;
        sq = 1.0 / sq;
        w = (f64::from(row1[0] - row0[1]) * sq) as f32;
        x = (f64::from(row2[0] + row0[2]) * sq) as f32;
        y = (f64::from(row2[1] + row1[2]) * sq) as f32;
    }
    // Quaternion.Normalize()
    let scale = 1.0f32 / (f64::from(w * w + (x * x + y * y + z * z)).sqrt() as f32);
    [x * scale, y * scale, z * scale, w * scale]
}

/// `STMath.ToEulerAngles`: OpenTK 3 builds the matrix through axis/angle.
fn to_euler_angles(q: [f32; 4]) -> [f32; 3] {
    let (mut qx, mut qy, mut qz, mut qw) = (q[0], q[1], q[2], q[3]);
    if qw.abs() > 1.0 {
        let scale = 1.0f32 / (f64::from(qw * qw + (qx * qx + qy * qy + qz * qz)).sqrt() as f32);
        qx *= scale;
        qy *= scale;
        qz *= scale;
        qw *= scale;
    }
    let angle = 2.0f32 * (f64::from(qw).acos() as f32);
    let den = (1.0 - f64::from(qw * qw)).sqrt() as f32;
    let mut axis = if den > 0.0001 {
        [qx / den, qy / den, qz / den]
    } else {
        [1.0, 0.0, 0.0]
    };
    // CreateFromAxisAngle
    axis = normalized(axis);
    let (ax, ay, az) = (axis[0], axis[1], axis[2]);
    let cos = f64::from(-angle).cos() as f32;
    let sin = f64::from(-angle).sin() as f32;
    let t = 1.0f32 - cos;
    let t_xx = t * ax * ax;
    let t_xy = t * ax * ay;
    let t_xz = t * ax * az;
    let t_yy = t * ay * ay;
    let t_yz = t * ay * az;
    let t_zz = t * az * az;
    let sin_x = sin * ax;
    let sin_y = sin * ay;
    let sin_z = sin * az;
    let m11 = t_xx + cos;
    let m12 = t_xy - sin_z;
    let m13 = t_xz + sin_y;
    let m22 = t_yy + cos;
    let m23 = t_yz - sin_x;
    let m32 = t_yz + sin_x;
    let m33 = t_zz + cos;
    let y = f64::from(m13.clamp(-1.0, 1.0)).asin() as f32;
    let (x, z) = if m13.abs() < 0.99999 {
        (
            f64::from(-m23).atan2(f64::from(m33)) as f32,
            f64::from(-m12).atan2(f64::from(m11)) as f32,
        )
    } else {
        (f64::from(m32).atan2(f64::from(m22)) as f32, 0.0)
    };
    [-x, -y, -z]
}

// ---- Assimp mesh bones -> Toolbox vertex bone names -------------------------

fn mesh_bone_usage(
    document: &fbxcel_dom::v7400::Document,
    models: &HashMap<i64, FbxModel<'_>>,
) -> io::Result<Vec<MeshBoneUsage>> {
    let mut usage = Vec::new();
    for object in document.objects() {
        let TypedObjectHandle::Geometry(TypedGeometryHandle::Mesh(geometry)) = object.get_typed()
        else {
            continue;
        };
        let mut name = geometry.name().unwrap_or("").to_string();
        if name.is_empty() {
            name = geometry
                .models()
                .next()
                .and_then(|model| models.get(&model.object_id().raw()))
                .map(|model| model.name.clone())
                .unwrap_or_default();
        }
        let control_points = geometry
            .node()
            .children_by_name("Vertices")
            .next()
            .and_then(|node| {
                node.attributes()
                    .first()
                    .and_then(|value| value.get_arr_f64())
            })
            .map(|values| values.len() / 3)
            .unwrap_or(0);
        let mut weights: Vec<Vec<(String, f64)>> = vec![Vec::new(); control_points];
        for skin in geometry.skins() {
            for cluster in skin.clusters() {
                let node = cluster.node();
                let indices = node
                    .children_by_name("Indexes")
                    .next()
                    .and_then(|v| v.attributes().first().and_then(|a| a.get_arr_i32()))
                    .unwrap_or(&[]);
                if indices.is_empty() {
                    // Assimp only creates a mesh bone for clusters that
                    // address at least one vertex.
                    continue;
                }
                let cluster_weights = node
                    .children_by_name("Weights")
                    .next()
                    .and_then(|v| v.attributes().first().and_then(|a| a.get_arr_f64()))
                    .unwrap_or(&[]);
                let bone_name = cluster
                    .source_objects()
                    .filter_map(|v| v.object_handle())
                    .find_map(|v| match v.get_typed() {
                        TypedObjectHandle::Model(_) => models
                            .get(&v.object_id().raw())
                            .map(|model| model.name.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| invalid("skin cluster is not linked to a model node"))?;
                for (slot, &index) in indices.iter().enumerate() {
                    let weight = cluster_weights.get(slot).copied().unwrap_or(0.0);
                    if let Some(entry) = usize::try_from(index)
                        .ok()
                        .and_then(|index| weights.get_mut(index))
                    {
                        entry.push((bone_name.clone(), weight));
                    }
                }
            }
        }
        // LimitBoneWeights: keep the four heaviest influences per vertex.
        let vertices = weights
            .into_iter()
            .map(|mut entries| {
                if entries.len() > MAX_WEIGHTS {
                    entries
                        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    entries.truncate(MAX_WEIGHTS);
                }
                entries.into_iter().map(|(name, _)| name).collect()
            })
            .collect();
        usage.push(MeshBoneUsage { name, vertices });
    }
    Ok(usage)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
