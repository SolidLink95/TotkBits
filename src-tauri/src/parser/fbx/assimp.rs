//! The meshes Assimp 4.1 hands Switch Toolbox for an FBX file, reproduced
//! step by step: the FBX importer's per-corner vertex expansion
//! (`MeshGeometry`), the split of multi-material meshes into one `aiMesh`
//! per material (`FBXConverter::ConvertMeshMultiMaterial`), the cluster to
//! bone conversion (`ConvertWeights`), and the post-processing steps Toolbox
//! requests: `FlipUVs`, `Triangulate`, `GenNormals`, `JoinIdenticalVertices`
//! and `LimitBoneWeights`, in Assimp's own pipeline order. Every float
//! operation keeps the precision of the C++ original (`ai_real` = `float`).

use super::toolbox_skeleton::{multiply, node_transform, Matrix, IDENTITY};
use fbxcel::tree::v7400::NodeHandle;
use fbxcel_dom::{
    any::AnyDocument,
    v7400::object::{
        deformer::{TypedDeformerHandle, TypedSubDeformerHandle},
        geometry::TypedGeometryHandle,
        TypedObjectHandle,
    },
};
use std::{
    collections::{HashMap, HashSet},
    io,
};

/// One `aiBone`: the cluster's target node name and its vertex weights in
/// Assimp's order (cluster index order, then output vertex order).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssimpBone {
    pub name: String,
    pub weights: Vec<(u32, f32)>,
}

/// One `aiMesh` after post-processing.
#[derive(Clone, Debug, Default)]
pub struct AssimpMesh {
    /// `aiMesh::mName`: the geometry name, or the node name when unnamed.
    pub name: String,
    /// Index into the converted scene materials (`aiMesh::mMaterialIndex`).
    pub material_index: u32,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Present only when the FBX carries both tangents and binormals.
    pub tangents: Vec<[f32; 3]>,
    pub bitangents: Vec<[f32; 3]>,
    pub uvs: Vec<Vec<[f32; 2]>>,
    pub colors: Vec<Vec<[f32; 4]>>,
    pub faces: Vec<[u32; 3]>,
    pub bones: Vec<AssimpBone>,
    /// The node's global transformation (Assimp matrix, column vectors).
    pub node_transform: Matrix,
}

impl AssimpMesh {
    pub fn has_tangent_basis(&self) -> bool {
        !self.tangents.is_empty() && !self.bitangents.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub struct AssimpScene {
    /// Meshes in Toolbox's `BuildNode` order: depth-first over the node
    /// tree, meshes of a node before its children.
    pub meshes: Vec<AssimpMesh>,
}

/// `AI_MAX_NUMBER_OF_TEXTURECOORDS` / `AI_MAX_NUMBER_OF_COLOR_SETS`.
const MAX_CHANNELS: usize = 8;
/// `LimitBoneWeightsProcess` default.
const MAX_BONE_WEIGHTS: usize = 4;
/// `JoinVerticesProcess` attribute epsilon (squared).
const JOIN_SQUARE_EPSILON: f32 = 1e-5 * 1e-5;

struct Object<'a> {
    name: String,
    node: NodeHandle<'a>,
    kind: ObjectKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ObjectKind {
    Model,
    MeshGeometry,
    Skin,
    Cluster,
    Material,
    Other,
}

/// `OO` connections keyed by destination, in file order (Assimp's
/// "sequenced" connection lookups sort by insertion order).
struct Connections {
    by_destination: HashMap<i64, Vec<i64>>,
}

impl Connections {
    fn sources(&self, destination: i64) -> &[i64] {
        self.by_destination
            .get(&destination)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

pub fn import_scene(data: &[u8]) -> io::Result<AssimpScene> {
    let document = AnyDocument::from_seekable_reader(std::io::Cursor::new(data))
        .map_err(|error| invalid(error.to_string()))?;
    let AnyDocument::V7400(_, document) = document else {
        return Err(invalid("unsupported FBX document version"));
    };

    let mut objects: HashMap<i64, Object<'_>> = HashMap::new();
    let mut model_order = Vec::new();
    for object in document.objects() {
        let kind = match object.get_typed() {
            TypedObjectHandle::Model(_) => ObjectKind::Model,
            TypedObjectHandle::Geometry(TypedGeometryHandle::Mesh(_)) => ObjectKind::MeshGeometry,
            TypedObjectHandle::Deformer(TypedDeformerHandle::Skin(_)) => ObjectKind::Skin,
            TypedObjectHandle::SubDeformer(TypedSubDeformerHandle::Cluster(_)) => {
                ObjectKind::Cluster
            }
            TypedObjectHandle::Material(_) => ObjectKind::Material,
            _ => ObjectKind::Other,
        };
        let id = object.object_id().raw();
        if kind == ObjectKind::Model {
            model_order.push(id);
        }
        objects.insert(
            id,
            Object {
                name: object.name().unwrap_or("").to_string(),
                node: object.node(),
                kind,
            },
        );
    }

    let mut by_destination: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut seen: HashSet<(i64, i64)> = HashSet::new();
    if let Some(connections) = document
        .tree()
        .root()
        .children_by_name("Connections")
        .next()
    {
        for connection in connections.children_by_name("C") {
            let attributes = connection.attributes();
            if attributes.first().and_then(|value| value.get_string()) != Some("OO") {
                continue;
            }
            let (Some(source), Some(destination)) = (
                attributes.get(1).and_then(|value| value.get_i64()),
                attributes.get(2).and_then(|value| value.get_i64()),
            ) else {
                continue;
            };
            if seen.insert((source, destination)) {
                by_destination.entry(destination).or_default().push(source);
            }
        }
    }
    let connections = Connections { by_destination };

    // Model nodes without a recorded parent connection hang off the root.
    let mut has_parent: HashSet<i64> = HashSet::new();
    for (destination, sources) in &connections.by_destination {
        let parent_is_model = *destination == 0
            || objects
                .get(destination)
                .is_some_and(|object| object.kind == ObjectKind::Model);
        if parent_is_model {
            for source in sources {
                if objects
                    .get(source)
                    .is_some_and(|object| object.kind == ObjectKind::Model)
                {
                    has_parent.insert(*source);
                }
            }
        }
    }
    let mut root_children: Vec<i64> = connections
        .sources(0)
        .iter()
        .copied()
        .filter(|id| {
            objects
                .get(id)
                .is_some_and(|object| object.kind == ObjectKind::Model)
        })
        .collect();
    for id in &model_order {
        if !has_parent.contains(id) && !root_children.contains(id) {
            root_children.push(*id);
        }
    }

    let mut converter = Converter {
        objects: &objects,
        connections: &connections,
        meshes: Vec::new(),
        materials_converted: Vec::new(),
        default_material: None,
    };
    for child in root_children {
        converter.convert_nodes(child, IDENTITY)?;
    }
    let mut meshes = converter.meshes;
    for mesh in &mut meshes {
        post_process(mesh);
    }
    Ok(AssimpScene { meshes })
}

struct Converter<'a, 'b> {
    objects: &'b HashMap<i64, Object<'a>>,
    connections: &'b Connections,
    meshes: Vec<AssimpMesh>,
    /// FBX material ids in `materials_converted` order.
    materials_converted: Vec<i64>,
    default_material: Option<u32>,
}

impl Converter<'_, '_> {
    /// `FBXConverter::ConvertNodes` + `ConvertModel`, walking children in
    /// connection order like Assimp and Toolbox's `BuildNode` do.
    fn convert_nodes(&mut self, id: i64, parent_transform: Matrix) -> io::Result<()> {
        let Some(model) = self.objects.get(&id) else {
            return Ok(());
        };
        if model.kind != ObjectKind::Model {
            return Ok(());
        }
        let world = multiply(&parent_transform, &node_transform(&model.node)?);
        for source in self.connections.sources(id) {
            let Some(object) = self.objects.get(source) else {
                continue;
            };
            if object.kind == ObjectKind::MeshGeometry {
                self.convert_mesh(*source, id, &world)?;
            }
        }
        for source in self.connections.sources(id).to_vec() {
            if self
                .objects
                .get(&source)
                .is_some_and(|object| object.kind == ObjectKind::Model)
            {
                self.convert_nodes(source, world)?;
            }
        }
        Ok(())
    }

    fn model_materials(&self, model_id: i64) -> Vec<i64> {
        self.connections
            .sources(model_id)
            .iter()
            .copied()
            .filter(|id| {
                self.objects
                    .get(id)
                    .is_some_and(|object| object.kind == ObjectKind::Material)
            })
            .collect()
    }

    /// `FBXConverter::ConvertMaterialForMesh`.
    fn material_for_mesh(&mut self, model_id: i64, index: i32) -> u32 {
        let materials = self.model_materials(model_id);
        let Some(&material) = usize::try_from(index)
            .ok()
            .and_then(|index| materials.get(index))
        else {
            return self.default_material();
        };
        if let Some(position) = self
            .materials_converted
            .iter()
            .position(|id| *id == material)
        {
            return position as u32;
        }
        self.materials_converted.push(material);
        (self.materials_converted.len() - 1) as u32
    }

    fn default_material(&mut self) -> u32 {
        if let Some(index) = self.default_material {
            return index;
        }
        // `GetDefaultMaterial` appends an extra material to the scene.
        let index = self.materials_converted.len() as u32;
        self.materials_converted.push(i64::MIN);
        self.default_material = Some(index);
        index
    }

    fn convert_mesh(&mut self, geometry_id: i64, model_id: i64, world: &Matrix) -> io::Result<()> {
        let geometry = &self.objects[&geometry_id];
        let mesh = MeshGeometry::read(&geometry.node)?;
        if mesh.faces.is_empty() {
            return Ok(());
        }
        let mut name = geometry.name.clone();
        if name.is_empty() {
            name = self.objects[&model_id].name.clone();
        }
        let skin = self.skin_clusters(geometry_id);

        // One material per mesh maps to one aiMesh; several split the mesh.
        let multi_material = mesh
            .materials
            .first()
            .is_some_and(|base| mesh.materials.iter().any(|index| index != base));
        if multi_material {
            let mut had: Vec<i32> = Vec::new();
            for index in mesh.materials.clone() {
                if had.contains(&index) {
                    continue;
                }
                had.push(index);
                let mut out = mesh.convert(Some(index), &skin, &name, world);
                out.material_index = self.material_for_mesh(model_id, index);
                self.meshes.push(out);
            }
        } else {
            let mut out = mesh.convert(None, &skin, &name, world);
            let index = mesh.materials.first().copied().unwrap_or(-1);
            out.material_index = if mesh.materials.is_empty() {
                self.default_material()
            } else {
                self.material_for_mesh(model_id, index)
            };
            self.meshes.push(out);
        }
        Ok(())
    }

    /// The clusters of the geometry's skin deformer, in connection order,
    /// each with its target bone name, control point indices and weights.
    fn skin_clusters(&self, geometry_id: i64) -> Vec<ClusterData> {
        let mut clusters = Vec::new();
        let Some(skin) = self.connections.sources(geometry_id).iter().find(|id| {
            self.objects
                .get(id)
                .is_some_and(|object| object.kind == ObjectKind::Skin)
        }) else {
            return clusters;
        };
        for cluster_id in self.connections.sources(*skin) {
            let Some(cluster) = self.objects.get(cluster_id) else {
                continue;
            };
            if cluster.kind != ObjectKind::Cluster {
                continue;
            }
            let target = self
                .connections
                .sources(*cluster_id)
                .iter()
                .find_map(|id| {
                    self.objects
                        .get(id)
                        .filter(|object| object.kind == ObjectKind::Model)
                        .map(|object| object.name.clone())
                })
                .unwrap_or_default();
            let indices = child_i32(&cluster.node, "Indexes")
                .map(<[i32]>::to_vec)
                .unwrap_or_default();
            let weights = child_f64(&cluster.node, "Weights")
                .map(|values| values.iter().map(|value| *value as f32).collect())
                .unwrap_or_default();
            clusters.push(ClusterData {
                bone: target,
                indices,
                weights,
            });
        }
        clusters
    }
}

struct ClusterData {
    bone: String,
    indices: Vec<i32>,
    weights: Vec<f32>,
}

/// `Assimp::FBX::MeshGeometry`: every attribute expanded per polygon corner.
struct MeshGeometry {
    control_points: Vec<[f32; 3]>,
    /// Per-corner positions in `PolygonVertexIndex` order.
    vertices: Vec<[f32; 3]>,
    /// Corner count of every polygon.
    faces: Vec<u32>,
    normals: Vec<[f32; 3]>,
    tangents: Vec<[f32; 3]>,
    binormals: Vec<[f32; 3]>,
    uvs: Vec<Vec<[f32; 2]>>,
    colors: Vec<Vec<[f32; 4]>>,
    /// Material index per face (empty when the mesh has no assignments).
    materials: Vec<i32>,
    /// Corner indices per control point (`ToOutputVertexIndex`).
    mapping_offsets: Vec<usize>,
    mapping_counts: Vec<usize>,
    mappings: Vec<u32>,
    /// Face index per corner (`FaceForVertexIndex`).
    face_for_corner: Vec<usize>,
}

impl MeshGeometry {
    fn read(node: &NodeHandle<'_>) -> io::Result<Self> {
        let raw_vertices = child_f64(node, "Vertices").unwrap_or(&[]);
        let control_points: Vec<[f32; 3]> = raw_vertices
            .chunks_exact(3)
            .map(|v| [v[0] as f32, v[1] as f32, v[2] as f32])
            .collect();
        let polygon_indices = child_i32(node, "PolygonVertexIndex").unwrap_or(&[]);
        let mut vertices = Vec::with_capacity(polygon_indices.len());
        let mut faces = Vec::new();
        let mut mapping_counts = vec![0usize; control_points.len()];
        let mut count = 0u32;
        for &index in polygon_indices {
            let absolute = if index < 0 { -index - 1 } else { index } as usize;
            let point = control_points
                .get(absolute)
                .ok_or_else(|| invalid("polygon vertex index out of range"))?;
            vertices.push(*point);
            count += 1;
            mapping_counts[absolute] += 1;
            if index < 0 {
                faces.push(count);
                count = 0;
            }
        }
        let mut mapping_offsets = vec![0usize; control_points.len()];
        let mut cursor = 0usize;
        for (offset, count) in mapping_offsets.iter_mut().zip(&mut mapping_counts) {
            *offset = cursor;
            cursor += *count;
            *count = 0;
        }
        let mut mappings = vec![0u32; polygon_indices.len()];
        for (corner, &index) in polygon_indices.iter().enumerate() {
            let absolute = if index < 0 { -index - 1 } else { index } as usize;
            mappings[mapping_offsets[absolute] + mapping_counts[absolute]] = corner as u32;
            mapping_counts[absolute] += 1;
        }
        let mut face_for_corner = Vec::with_capacity(vertices.len());
        for (face, count) in faces.iter().enumerate() {
            face_for_corner.extend(std::iter::repeat(face).take(*count as usize));
        }

        let mut mesh = MeshGeometry {
            control_points,
            vertices,
            faces,
            normals: Vec::new(),
            tangents: Vec::new(),
            binormals: Vec::new(),
            uvs: Vec::new(),
            colors: Vec::new(),
            materials: Vec::new(),
            mapping_offsets,
            mapping_counts,
            mappings,
            face_for_corner,
        };
        mesh.read_layers(node)?;
        Ok(mesh)
    }

    /// `MeshGeometry::ReadLayer` / `ReadLayerElement`: every element the
    /// `Layer` nodes reference, in layer order.
    fn read_layers(&mut self, node: &NodeHandle<'_>) -> io::Result<()> {
        let mut uvs: Vec<(usize, Vec<[f32; 2]>)> = Vec::new();
        let mut colors: Vec<(usize, Vec<[f32; 4]>)> = Vec::new();
        for layer in node.children_by_name("Layer") {
            for element in layer.children_by_name("LayerElement") {
                let kind = element
                    .children_by_name("Type")
                    .next()
                    .and_then(|v| v.attributes().first().and_then(|a| a.get_string()))
                    .unwrap_or("")
                    .to_string();
                let typed_index = element
                    .children_by_name("TypedIndex")
                    .next()
                    .and_then(|v| v.attributes().first().and_then(|a| a.get_i32()))
                    .unwrap_or(0);
                let Some(source) = node.children_by_name(&kind).find(|candidate| {
                    candidate
                        .attributes()
                        .first()
                        .and_then(|a| a.get_i32())
                        .unwrap_or(0)
                        == typed_index
                }) else {
                    continue;
                };
                match kind.as_str() {
                    "LayerElementNormal" => {
                        if self.normals.is_empty() {
                            self.normals = self.resolve_vec3(&source, "Normals", "NormalsIndex")?;
                        }
                    }
                    "LayerElementTangent" => {
                        if self.tangents.is_empty() {
                            self.tangents =
                                self.resolve_vec3(&source, "Tangents", "TangentsIndex")?;
                        }
                    }
                    "LayerElementBinormal" => {
                        if self.binormals.is_empty() {
                            self.binormals =
                                self.resolve_vec3(&source, "Binormals", "BinormalsIndex")?;
                        }
                    }
                    "LayerElementUV" => {
                        let index = typed_index.max(0) as usize;
                        if index < MAX_CHANNELS && !uvs.iter().any(|(i, _)| *i == index) {
                            let data = self.resolve_vec2(&source, "UV", "UVIndex")?;
                            uvs.push((index, data));
                        }
                    }
                    "LayerElementColor" => {
                        let index = typed_index.max(0) as usize;
                        if index < MAX_CHANNELS && !colors.iter().any(|(i, _)| *i == index) {
                            let data = self.resolve_color(&source, "Colors", "ColorIndex")?;
                            colors.push((index, data));
                        }
                    }
                    "LayerElementMaterial" => {
                        if self.materials.is_empty() {
                            self.read_materials(&source)?;
                        }
                    }
                    _ => {}
                }
            }
        }
        // Assimp stores channels by their typed index and stops at the
        // first empty one, so gaps cut the list short.
        self.uvs = collect_channels(uvs);
        self.colors = collect_channels(colors);
        Ok(())
    }

    /// `ReadVertexDataMaterials`.
    fn read_materials(&mut self, source: &NodeHandle<'_>) -> io::Result<()> {
        let mapping = child_string(source, "MappingInformationType").unwrap_or("");
        let reference = child_string(source, "ReferenceInformationType").unwrap_or("");
        let materials = child_i32(source, "Materials").unwrap_or(&[]);
        if mapping == "AllSame" {
            if let Some(first) = materials.first() {
                self.materials = vec![*first; self.faces.len()];
            }
        } else if mapping == "ByPolygon" && reference == "IndexToDirect" {
            let mut values = materials.to_vec();
            values.resize(self.faces.len(), 0);
            self.materials = values;
        }
        Ok(())
    }

    /// `ResolveVertexDataArray` for the mapping/reference combinations
    /// Assimp 4.1 implements; anything else leaves the channel empty.
    fn resolve<T: Copy + Default>(
        &self,
        source: &NodeHandle<'_>,
        data: &[T],
        index_name: &str,
    ) -> io::Result<Vec<T>> {
        let mapping = child_string(source, "MappingInformationType").unwrap_or("");
        let reference = child_string(source, "ReferenceInformationType").unwrap_or("");
        let indices = child_i32(source, index_name);
        let corner_count = self.vertices.len();
        let mut direct = reference == "Direct";
        let mut index_to_direct = reference == "IndexToDirect";
        if index_to_direct && indices.is_none() {
            direct = true;
            index_to_direct = false;
        }
        let mut out = Vec::new();
        if mapping == "ByVertice" && direct {
            out = vec![T::default(); corner_count];
            for (point, value) in data.iter().enumerate() {
                let Some((&offset, &count)) = self
                    .mapping_offsets
                    .get(point)
                    .zip(self.mapping_counts.get(point))
                else {
                    break;
                };
                for corner in &self.mappings[offset..offset + count] {
                    out[*corner as usize] = *value;
                }
            }
        } else if mapping == "ByVertice" && index_to_direct {
            out = vec![T::default(); corner_count];
            for (point, index) in indices.unwrap_or(&[]).iter().enumerate() {
                let Some((&offset, &count)) = self
                    .mapping_offsets
                    .get(point)
                    .zip(self.mapping_counts.get(point))
                else {
                    break;
                };
                let value = usize::try_from(*index)
                    .ok()
                    .and_then(|index| data.get(index))
                    .ok_or_else(|| invalid("FBX layer index out of range"))?;
                for corner in &self.mappings[offset..offset + count] {
                    out[*corner as usize] = *value;
                }
            }
        } else if mapping == "ByPolygonVertex" && direct {
            if data.len() == corner_count {
                out = data.to_vec();
            }
        } else if mapping == "ByPolygonVertex" && index_to_direct {
            let indices = indices.unwrap_or(&[]);
            if indices.len() == corner_count {
                out = Vec::with_capacity(corner_count);
                for index in indices {
                    if *index == -1 {
                        out.push(T::default());
                        continue;
                    }
                    let value = usize::try_from(*index)
                        .ok()
                        .and_then(|index| data.get(index))
                        .ok_or_else(|| invalid("FBX layer index out of range"))?;
                    out.push(*value);
                }
            }
        }
        Ok(out)
    }

    fn resolve_vec3(
        &self,
        source: &NodeHandle<'_>,
        name: &str,
        index_name: &str,
    ) -> io::Result<Vec<[f32; 3]>> {
        let data: Vec<[f32; 3]> = child_f64(source, name)
            .unwrap_or(&[])
            .chunks_exact(3)
            .map(|v| [v[0] as f32, v[1] as f32, v[2] as f32])
            .collect();
        self.resolve(source, &data, index_name)
    }

    fn resolve_vec2(
        &self,
        source: &NodeHandle<'_>,
        name: &str,
        index_name: &str,
    ) -> io::Result<Vec<[f32; 2]>> {
        let data: Vec<[f32; 2]> = child_f64(source, name)
            .unwrap_or(&[])
            .chunks_exact(2)
            .map(|v| [v[0] as f32, v[1] as f32])
            .collect();
        self.resolve(source, &data, index_name)
    }

    fn resolve_color(
        &self,
        source: &NodeHandle<'_>,
        name: &str,
        index_name: &str,
    ) -> io::Result<Vec<[f32; 4]>> {
        let data: Vec<[f32; 4]> = child_f64(source, name)
            .unwrap_or(&[])
            .chunks_exact(4)
            .map(|v| [v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32])
            .collect();
        self.resolve(source, &data, index_name)
    }

    /// `ConvertMeshSingleMaterial` (`material` = `None`) or the per-material
    /// half of `ConvertMeshMultiMaterial`, followed by `ConvertWeights`.
    fn convert(
        &self,
        material: Option<i32>,
        clusters: &[ClusterData],
        name: &str,
        world: &Matrix,
    ) -> AssimpMesh {
        let has_tangents = !self.tangents.is_empty();
        let binormals: Vec<[f32; 3]> = if has_tangents && self.binormals.is_empty() {
            if self.normals.is_empty() {
                Vec::new()
            } else {
                // Assimp derives missing binormals as normal x tangent.
                self.normals
                    .iter()
                    .zip(&self.tangents)
                    .map(|(n, t)| cross(*n, *t))
                    .collect()
            }
        } else {
            self.binormals.clone()
        };
        let use_tangents = has_tangents && !binormals.is_empty();

        let mut out = AssimpMesh {
            name: name.to_owned(),
            node_transform: *world,
            uvs: vec![Vec::new(); self.uvs.len()],
            colors: vec![Vec::new(); self.colors.len()],
            ..Default::default()
        };
        // Corner index in the source geometry for every output vertex.
        let mut reverse_mapping: Vec<u32> = Vec::new();
        let mut polygons: Vec<Vec<u32>> = Vec::new();
        let mut in_cursor = 0usize;
        let mut cursor = 0u32;
        for (face, &count) in self.faces.iter().enumerate() {
            let count = count as usize;
            if material.is_some_and(|index| self.materials.get(face) != Some(&index)) {
                in_cursor += count;
                continue;
            }
            let mut indices = Vec::with_capacity(count);
            for _ in 0..count {
                indices.push(cursor);
                reverse_mapping.push(in_cursor as u32);
                out.positions.push(self.vertices[in_cursor]);
                if !self.normals.is_empty() {
                    out.normals.push(self.normals[in_cursor]);
                }
                if use_tangents {
                    out.tangents.push(self.tangents[in_cursor]);
                    out.bitangents.push(binormals[in_cursor]);
                }
                for (channel, uvs) in self.uvs.iter().enumerate() {
                    out.uvs[channel].push(uvs[in_cursor]);
                }
                for (channel, colors) in self.colors.iter().enumerate() {
                    out.colors[channel].push(colors[in_cursor]);
                }
                cursor += 1;
                in_cursor += 1;
            }
            polygons.push(indices);
        }
        // `FlipUVs` precedes `Triangulate` in Assimp's pipeline but does not
        // touch positions, so the polygons can be split right away.
        out.faces = triangulate(&out.positions, polygons);
        out.bones = self.convert_weights(material, clusters, &reverse_mapping);
        out
    }

    /// `FBXConverter::ConvertWeights` + `ConvertCluster`.
    fn convert_weights(
        &self,
        material: Option<i32>,
        clusters: &[ClusterData],
        reverse_mapping: &[u32],
    ) -> Vec<AssimpBone> {
        let mut bones = Vec::new();
        for cluster in clusters {
            if cluster.indices.is_empty() {
                continue;
            }
            let mut weights = Vec::new();
            let mut ok = false;
            for (slot, &index) in cluster.indices.iter().enumerate() {
                let Some(point) = usize::try_from(index).ok() else {
                    continue;
                };
                let (Some(&offset), Some(&count)) = (
                    self.mapping_offsets.get(point),
                    self.mapping_counts.get(point),
                ) else {
                    continue;
                };
                let weight = cluster.weights.get(slot).copied().unwrap_or(0.0);
                for corner in &self.mappings[offset..offset + count] {
                    let corner = *corner as usize;
                    let keep = match material {
                        None => true,
                        Some(index) => {
                            self.materials.get(self.face_for_corner[corner]) == Some(&index)
                        }
                    };
                    if !keep {
                        continue;
                    }
                    let output = match material {
                        None => corner as u32,
                        // std::lower_bound over the sorted corner list.
                        Some(_) => reverse_mapping.partition_point(|c| *c < corner as u32) as u32,
                    };
                    weights.push((output, weight));
                    ok = true;
                }
            }
            if ok {
                bones.push(AssimpBone {
                    name: cluster.bone.clone(),
                    weights,
                });
            }
        }
        bones
    }
}

fn collect_channels<T>(mut channels: Vec<(usize, Vec<T>)>) -> Vec<Vec<T>> {
    channels.sort_by_key(|(index, _)| *index);
    let mut out = Vec::new();
    for expected in 0..MAX_CHANNELS {
        match channels.iter().position(|(index, _)| *index == expected) {
            Some(position) => {
                let (_, data) = channels.swap_remove(position);
                if data.is_empty() {
                    break;
                }
                out.push(data);
            }
            None => break,
        }
    }
    out
}

// ---- Post-processing --------------------------------------------------------

/// Assimp's pipeline for Toolbox's flags, in `PostStepRegistry` order:
/// `FlipUVs`, `Triangulate` (done while converting), `GenFaceNormals`,
/// `JoinVertices`, `LimitBoneWeights`.
fn post_process(mesh: &mut AssimpMesh) {
    flip_uvs(mesh);
    if mesh.normals.is_empty() {
        gen_face_normals(mesh);
    }
    join_vertices(mesh);
    limit_bone_weights(mesh);
}

/// `FlipUVsProcess`: `y = 1 - y` on every channel.
fn flip_uvs(mesh: &mut AssimpMesh) {
    for channel in &mut mesh.uvs {
        for uv in channel {
            uv[1] = 1.0f32 - uv[1];
        }
    }
}

/// `TriangulateProcess`: triangles pass through, quads split on the concave
/// corner when there is one, larger polygons fan from the first vertex.
fn triangulate(positions: &[[f32; 3]], polygons: Vec<Vec<u32>>) -> Vec<[u32; 3]> {
    let mut faces = Vec::with_capacity(polygons.len());
    for polygon in polygons {
        match polygon.len() {
            0..=2 => {}
            3 => faces.push([polygon[0], polygon[1], polygon[2]]),
            4 => {
                let mut start = 0usize;
                for i in 0..4 {
                    let v0 = positions[polygon[(i + 3) % 4] as usize];
                    let v1 = positions[polygon[(i + 2) % 4] as usize];
                    let v2 = positions[polygon[(i + 1) % 4] as usize];
                    let v = positions[polygon[i] as usize];
                    let left = normalize(sub(v0, v));
                    let diag = normalize(sub(v1, v));
                    let right = normalize(sub(v2, v));
                    let angle = dot(left, diag).acos() + dot(right, diag).acos();
                    if angle > std::f32::consts::PI {
                        start = i;
                        break;
                    }
                }
                faces.push([
                    polygon[start],
                    polygon[(start + 1) % 4],
                    polygon[(start + 2) % 4],
                ]);
                faces.push([
                    polygon[start],
                    polygon[(start + 2) % 4],
                    polygon[(start + 3) % 4],
                ]);
            }
            _ => {
                // Assimp ear-clips concave n-gons; convex fans match it.
                for i in 1..polygon.len() - 1 {
                    faces.push([polygon[0], polygon[i], polygon[i + 1]]);
                }
            }
        }
    }
    faces
}

/// `GenFaceNormalsProcess`: every corner takes its face normal.
fn gen_face_normals(mesh: &mut AssimpMesh) {
    let mut normals = vec![[0.0f32; 3]; mesh.positions.len()];
    for face in &mesh.faces {
        let p1 = mesh.positions[face[0] as usize];
        let p2 = mesh.positions[face[1] as usize];
        let p3 = mesh.positions[face[2] as usize];
        let normal = normalize_safe(cross(sub(p2, p1), sub(p3, p1)));
        for index in face {
            normals[*index as usize] = normal;
        }
    }
    mesh.normals = normals;
}

/// `JoinVerticesProcess::ProcessMesh`: vertices are unique when their
/// positions are (bitwise) identical and every other attribute lies within
/// `1e-5`; the first occurrence wins and keeps its bone weights.
fn join_vertices(mesh: &mut AssimpMesh) {
    let count = mesh.positions.len();
    let mut by_position: HashMap<[u32; 3], Vec<u32>> = HashMap::new();
    let mut replace_index: Vec<u32> = vec![u32::MAX; count];
    let mut unique: Vec<u32> = Vec::with_capacity(count);
    let key = |p: [f32; 3]| p.map(|v| if v == 0.0 { 0 } else { v.to_bits() });
    for vertex in 0..count {
        let candidates = by_position.entry(key(mesh.positions[vertex])).or_default();
        let mut matched = None;
        for &candidate in candidates.iter() {
            let unique_index = replace_index[candidate as usize];
            if unique_index & 0x8000_0000 != 0 {
                continue;
            }
            if vertices_match(mesh, candidate as usize, vertex) {
                matched = Some(unique_index);
                break;
            }
        }
        match matched {
            Some(unique_index) => replace_index[vertex] = unique_index | 0x8000_0000,
            None => {
                replace_index[vertex] = unique.len() as u32;
                unique.push(vertex as u32);
                candidates.push(vertex as u32);
            }
        }
    }
    if unique.len() == count {
        return;
    }
    let pick = |values: &Vec<[f32; 3]>| -> Vec<[f32; 3]> {
        unique.iter().map(|v| values[*v as usize]).collect()
    };
    mesh.positions = pick(&mesh.positions);
    if !mesh.normals.is_empty() {
        mesh.normals = pick(&mesh.normals);
    }
    if !mesh.tangents.is_empty() {
        mesh.tangents = pick(&mesh.tangents);
        mesh.bitangents = pick(&mesh.bitangents);
    }
    for channel in &mut mesh.uvs {
        *channel = unique.iter().map(|v| channel[*v as usize]).collect();
    }
    for channel in &mut mesh.colors {
        *channel = unique.iter().map(|v| channel[*v as usize]).collect();
    }
    for face in &mut mesh.faces {
        for index in face {
            *index = replace_index[*index as usize] & !0x8000_0000;
        }
    }
    for bone in &mut mesh.bones {
        bone.weights.retain_mut(|(vertex, _)| {
            let replacement = replace_index[*vertex as usize];
            if replacement & 0x8000_0000 != 0 {
                false
            } else {
                *vertex = replacement;
                true
            }
        });
    }
}

fn vertices_match(mesh: &AssimpMesh, a: usize, b: usize) -> bool {
    let close3 = |x: [f32; 3], y: [f32; 3]| square_length(sub(x, y)) <= JOIN_SQUARE_EPSILON;
    if !mesh.normals.is_empty() && !close3(mesh.normals[a], mesh.normals[b]) {
        return false;
    }
    if !mesh.tangents.is_empty() {
        if !close3(mesh.tangents[a], mesh.tangents[b]) {
            return false;
        }
        if !close3(mesh.bitangents[a], mesh.bitangents[b]) {
            return false;
        }
    }
    for channel in &mesh.uvs {
        let (u, v) = (channel[a], channel[b]);
        let d = [u[0] - v[0], u[1] - v[1], 0.0];
        if square_length(d) > JOIN_SQUARE_EPSILON {
            return false;
        }
    }
    for channel in &mesh.colors {
        let (u, v) = (channel[a], channel[b]);
        let d = [u[0] - v[0], u[1] - v[1], u[2] - v[2], u[3] - v[3]];
        let difference = d[0] * d[0] + d[1] * d[1] + d[2] * d[2] + d[3] * d[3];
        if difference > JOIN_SQUARE_EPSILON {
            return false;
        }
    }
    true
}

/// `LimitBoneWeightsProcess`: vertices with more than four influences keep
/// the four heaviest (stable order for ties) and are renormalized; the bone
/// weight lists are rebuilt in vertex order.
fn limit_bone_weights(mesh: &mut AssimpMesh) {
    if mesh.bones.is_empty() {
        return;
    }
    let count = mesh.positions.len();
    let mut per_vertex: Vec<Vec<(usize, f32)>> = vec![Vec::new(); count];
    for (bone_index, bone) in mesh.bones.iter().enumerate() {
        for &(vertex, weight) in &bone.weights {
            if let Some(list) = per_vertex.get_mut(vertex as usize) {
                list.push((bone_index, weight));
            }
        }
    }
    let mut changed = false;
    for list in &mut per_vertex {
        if list.len() <= MAX_BONE_WEIGHTS {
            continue;
        }
        changed = true;
        list.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        list.truncate(MAX_BONE_WEIGHTS);
        let mut sum = 0.0f32;
        for (_, weight) in list.iter() {
            sum += *weight;
        }
        if sum != 0.0 {
            let inverse = 1.0f32 / sum;
            for (_, weight) in list.iter_mut() {
                *weight *= inverse;
            }
        }
    }
    if !changed {
        return;
    }
    let mut rebuilt: Vec<Vec<(u32, f32)>> = vec![Vec::new(); mesh.bones.len()];
    for (vertex, list) in per_vertex.iter().enumerate() {
        for &(bone, weight) in list {
            rebuilt[bone].push((vertex as u32, weight));
        }
    }
    for (bone, weights) in mesh.bones.iter_mut().zip(rebuilt) {
        bone.weights = weights;
    }
}

// ---- aiVector3D helpers (float precision) -----------------------------------

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn square_length(v: [f32; 3]) -> f32 {
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
}

/// `aiVector3D::Normalize`: divides by `std::sqrt` of the float length.
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = square_length(v).sqrt();
    [v[0] / length, v[1] / length, v[2] / length]
}

/// `aiVector3D::NormalizeSafe`.
fn normalize_safe(v: [f32; 3]) -> [f32; 3] {
    let length = square_length(v).sqrt();
    if length > 0.0 {
        [v[0] / length, v[1] / length, v[2] / length]
    } else {
        v
    }
}

// ---- FBX node helpers -------------------------------------------------------

fn child_f64<'a>(node: &NodeHandle<'a>, name: &str) -> Option<&'a [f64]> {
    node.children_by_name(name)
        .next()
        .and_then(|v| v.attributes().first())
        .and_then(|v| v.get_arr_f64())
}

fn child_i32<'a>(node: &NodeHandle<'a>, name: &str) -> Option<&'a [i32]> {
    node.children_by_name(name)
        .next()
        .and_then(|v| v.attributes().first())
        .and_then(|v| v.get_arr_i32())
}

fn child_string<'a>(node: &NodeHandle<'a>, name: &str) -> Option<&'a str> {
    node.children_by_name(name)
        .next()
        .and_then(|v| v.attributes().first())
        .and_then(|v| v.get_string())
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
