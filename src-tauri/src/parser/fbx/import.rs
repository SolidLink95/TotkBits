use fbxcel::tree::v7400::NodeHandle;
use fbxcel_dom::{
    any::AnyDocument,
    v7400::data::mesh::layer::TypedLayerElementHandle,
    v7400::object::{geometry::TypedGeometryHandle, model::TypedModelHandle, TypedObjectHandle},
};
use std::{cell::RefCell, collections::HashMap, io};

#[derive(Debug, Clone)]
pub struct ImportedMesh {
    pub name: String,
    pub material: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub bitangents: Vec<[f32; 4]>,
    pub uv_maps: Vec<Vec<[f32; 2]>>,
    pub colors: Vec<Vec<[f32; 4]>>,
    pub bone_indices: Vec<[u16; 8]>,
    pub bone_weights: Vec<[f32; 8]>,
    pub palette_bones: Vec<u16>,
    pub source_vertices: Vec<usize>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct ImportedFbx {
    pub meshes: Vec<ImportedMesh>,
    pub bones: Vec<(String, Option<String>)>,
}

pub fn import_for_g1m(data: &[u8]) -> io::Result<ImportedFbx> {
    import_meshes(data, true)
}

/// Imports only polygon meshes and the skin data needed by BFRES. Materials,
/// cameras, lights and every other FBX object type are intentionally ignored.
pub fn import_for_bfres(data: &[u8]) -> io::Result<ImportedFbx> {
    import_meshes(data, false)
}

fn import_meshes(data: &[u8], strict_g1m: bool) -> io::Result<ImportedFbx> {
    let document = AnyDocument::from_seekable_reader(std::io::Cursor::new(data))
        .map_err(|error| invalid(error.to_string()))?;
    let AnyDocument::V7400(_, document) = document else {
        return Err(invalid("unsupported FBX document version"));
    };
    let mut bone_names = Vec::new();
    let mut bone_by_id = HashMap::new();
    for object in document.objects() {
        if let TypedObjectHandle::Model(TypedModelHandle::LimbNode(model)) = object.get_typed() {
            let name = model.name().unwrap_or("").to_string();
            if name.is_empty() {
                return Err(invalid("FBX armature contains an unnamed bone"));
            }
            bone_by_id.insert(model.object_id(), bone_names.len() as u16);
            bone_names.push((model.object_id(), name));
        }
    }
    if strict_g1m && bone_names.is_empty() {
        return Err(invalid("FBX contains no armature bones"));
    }
    let bones = bone_names
        .iter()
        .map(|(id, name)| {
            let parent = document
                .objects()
                .find(|object| object.object_id() == *id)
                .and_then(|object| match object.get_typed() {
                    TypedObjectHandle::Model(model) => model.parent_model(),
                    _ => None,
                })
                .and_then(|parent| match parent {
                    TypedModelHandle::LimbNode(parent) => parent.name().map(str::to_owned),
                    _ => None,
                });
            (name.clone(), parent)
        })
        .collect();

    let mut meshes = Vec::new();
    for object in document.objects() {
        let TypedObjectHandle::Geometry(TypedGeometryHandle::Mesh(geometry)) = object.get_typed()
        else {
            continue;
        };
        let mut models = geometry.models();
        let Some(model) = models.next() else { continue };
        if models.next().is_some() {
            return Err(invalid(format!(
                "FBX geometry {} is instanced by multiple mesh nodes",
                geometry.name().unwrap_or("<unnamed>")
            )));
        }
        let skins: Vec<_> = geometry.skins().collect();
        if strict_g1m && skins.is_empty() {
            continue;
        }
        if skins.len() > 1 || (strict_g1m && skins.len() != 1) {
            return Err(invalid(
                "each model mesh must have exactly one skin deformer",
            ));
        }
        let materials: Vec<_> = model.materials().collect();
        if strict_g1m && materials.len() != 1 {
            return Err(invalid(format!(
                "mesh {} must have exactly one material (found {})",
                model.name().unwrap_or("<unnamed>"),
                materials.len()
            )));
        }
        let name = model.name().unwrap_or("").to_string();
        let material = materials
            .first()
            .and_then(|material| material.name())
            .unwrap_or("")
            .to_string();
        if name.is_empty() || (strict_g1m && material.is_empty()) {
            return Err(invalid("FBX mesh and material names must not be empty"));
        }
        let polygons = geometry
            .polygon_vertices()
            .map_err(|error| invalid(error.to_string()))?;
        let control: Vec<[f32; 3]> = polygons
            .raw_control_points()
            .map_err(|error| invalid(error.to_string()))?
            .map(|p| [p.x as f32, p.y as f32, p.z as f32])
            .collect();
        let polygon_vertex_ordinals = RefCell::new(HashMap::new());
        let triangles = polygons
            .triangulate_each(|_, polygon, output| {
                let mut polygon_vertex_ordinals = polygon_vertex_ordinals.borrow_mut();
                for &index in polygon {
                    let ordinal = polygon_vertex_ordinals.len();
                    polygon_vertex_ordinals.entry(index).or_insert(ordinal);
                }
                if polygon.len() >= 3 {
                    for index in 1..polygon.len() - 1 {
                        output.push([polygon[0], polygon[index], polygon[index + 1]]);
                    }
                }
                Ok(())
            })
            .map_err(|error| invalid(error.to_string()))?;
        let polygon_vertex_ordinals = polygon_vertex_ordinals.into_inner();
        let mut control_weights = vec![Vec::<(u16, f32)>::new(); control.len()];
        let mut palette_bones = Vec::new();
        for cluster in skins.first().into_iter().flat_map(|skin| skin.clusters()) {
            let bone = cluster
                .source_objects()
                .filter_map(|v| v.object_handle())
                .find_map(|v| match v.get_typed() {
                    TypedObjectHandle::Model(TypedModelHandle::LimbNode(_)) => {
                        bone_by_id.get(&v.object_id()).copied()
                    }
                    _ => None,
                })
                .ok_or_else(|| invalid("skin cluster is not linked to an armature bone"))?;
            let node = cluster.node();
            let cluster_indices_node = child_i32_optional(&node, "Indexes");
            let cluster_indices = cluster_indices_node.unwrap_or(&[]);
            let cluster_weights = child_f64_optional(&node, "Weights").unwrap_or(&[]);
            if cluster_indices.len() != cluster_weights.len() {
                return Err(invalid("skin cluster index/weight counts differ"));
            }
            if cluster_weights.iter().any(|weight| *weight > 0.0) && !palette_bones.contains(&bone)
            {
                palette_bones.push(bone);
            }
            for (&index, &weight) in cluster_indices.iter().zip(cluster_weights) {
                let target = usize::try_from(index).map_err(|_| invalid("negative skin index"))?;
                let values = control_weights
                    .get_mut(target)
                    .ok_or_else(|| invalid("skin index is out of range"))?;
                if weight > 0.0 {
                    values.push((bone, weight as f32));
                }
            }
        }
        let mut normal_layer = None;
        let mut uv_layers = Vec::new();
        let mut color_layers = Vec::new();
        let mut raw_color_layers = Vec::new();
        for layer in geometry.layers() {
            for entry in layer.layer_element_entries() {
                match entry
                    .typed_layer_element()
                    .map_err(|error| invalid(error.to_string()))?
                {
                    TypedLayerElementHandle::Normal(handle) if normal_layer.is_none() => {
                        normal_layer = Some(
                            handle
                                .normals()
                                .map_err(|error| invalid(error.to_string()))?,
                        );
                    }
                    TypedLayerElementHandle::Uv(handle) => {
                        uv_layers.push(handle.uv().map_err(|error| invalid(error.to_string()))?)
                    }
                    TypedLayerElementHandle::Color(handle) => {
                        if let Ok(layer) = handle.color() {
                            color_layers.push(layer);
                        } else {
                            let node = handle.node();
                            let direct = child_f64_optional(&node, "Colors")
                                .ok_or_else(|| invalid("FBX color layer has no Colors array"))?;
                            if direct.len() % 4 != 0 {
                                return Err(invalid(
                                    "FBX Colors array length is not divisible by 4",
                                ));
                            }
                            let direct = direct
                                .chunks_exact(4)
                                .map(|value| {
                                    [
                                        value[0] as f32,
                                        value[1] as f32,
                                        value[2] as f32,
                                        value[3] as f32,
                                    ]
                                })
                                .collect::<Vec<_>>();
                            let indices = child_i32_optional(&node, "ColorIndex")
                                .or_else(|| child_i32_optional(&node, "ColorsIndex"))
                                .map(<[i32]>::to_vec);
                            raw_color_layers.push((direct, indices));
                        }
                    }
                    _ => {}
                }
            }
        }
        let normal_layer =
            normal_layer.ok_or_else(|| invalid(format!("mesh {name} has no FBX normal layer")))?;
        if uv_layers.is_empty() {
            return Err(invalid(format!("mesh {name} has no FBX UV layer")));
        }
        let corner_count = triangles.len();
        if corner_count == 0 {
            // Meshes without polygons (empty objects, stray control points)
            // have nothing to replace a BFRES shape with.
            continue;
        }
        let mut positions = Vec::with_capacity(corner_count);
        let mut normals = Vec::with_capacity(corner_count);
        let mut uv_maps = vec![Vec::with_capacity(corner_count); uv_layers.len()];
        let mut colors =
            vec![Vec::with_capacity(corner_count); color_layers.len() + raw_color_layers.len()];
        let mut bone_indices = Vec::with_capacity(corner_count);
        let mut bone_weights = Vec::with_capacity(corner_count);
        let mut indices = Vec::with_capacity(corner_count);
        let mut source_control_indices = Vec::with_capacity(corner_count);
        for (triangle_corner, triangle_vertex) in triangles.triangle_vertex_indices().enumerate() {
            let control_index = triangles
                .control_point_index(triangle_vertex)
                .ok_or_else(|| invalid("polygon index is out of range"))?
                .to_u32() as usize;
            positions.push(control[control_index]);
            source_control_indices.push(control_index);
            let normal = normal_layer
                .normal(&triangles, triangle_vertex)
                .map_err(|error| invalid(error.to_string()))?;
            let mut normal = [normal.x as f32, normal.y as f32, normal.z as f32];
            if !normal.iter().all(|value| value.is_finite())
                || normal.iter().map(|value| value * value).sum::<f32>() <= f32::EPSILON
            {
                return Err(invalid(format!("mesh {name} contains an invalid normal")));
            }
            let inverse_length = 1.0
                / (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            for component in &mut normal {
                *component *= inverse_length;
            }
            normals.push(normal);
            for (layer, values) in uv_layers.iter().enumerate() {
                let uv = values
                    .uv(&triangles, triangle_vertex)
                    .map_err(|error| invalid(error.to_string()))?;
                if !uv.x.is_finite() || !uv.y.is_finite() {
                    return Err(invalid(format!("mesh {name} contains an invalid UV")));
                }
                // FBX uses a bottom-left UV origin while G1M stores V from the top.
                uv_maps[layer].push([uv.x as f32, 1.0 - uv.y as f32]);
            }
            for (layer, values) in color_layers.iter().enumerate() {
                let color = values
                    .color(&triangles, triangle_vertex)
                    .map_err(|error| invalid(error.to_string()))?;
                colors[layer].push([
                    color[0] as f32,
                    color[1] as f32,
                    color[2] as f32,
                    color[3] as f32,
                ]);
            }
            for (raw_layer, (direct, color_indices)) in raw_color_layers.iter().enumerate() {
                let polygon_vertex = if color_indices
                    .as_ref()
                    .is_some_and(|indices| indices.len() == control.len())
                {
                    control_index
                } else {
                    let source = triangles
                        .polygon_vertex_index(triangle_vertex)
                        .ok_or_else(|| invalid("triangle vertex has no source polygon vertex"))?;
                    *polygon_vertex_ordinals
                        .get(&source)
                        .ok_or_else(|| invalid("source polygon vertex is out of range"))?
                };
                let direct_index = match color_indices {
                    Some(indices) => *indices.get(polygon_vertex).ok_or_else(|| {
                        invalid(format!(
                            "FBX ColorIndex is shorter than its polygon vertex data ({} indices, {} controls, corner {}, control {})",
                            indices.len(), control.len(), triangle_corner, control_index
                        ))
                    })? as usize,
                    None => polygon_vertex,
                };
                let color = direct
                    .get(direct_index)
                    .ok_or_else(|| invalid("FBX color index is out of range"))?;
                colors[color_layers.len() + raw_layer].push(*color);
            }
            let mut influences = control_weights[control_index].clone();
            influences.sort_by(|a, b| b.1.total_cmp(&a.1));
            influences.truncate(8);
            let sum: f32 = influences.iter().map(|v| v.1).sum();
            if strict_g1m && sum <= f32::EPSILON {
                return Err(invalid(format!(
                    "mesh {name} contains an unweighted vertex"
                )));
            }
            let mut joints = [0; 8];
            let mut weights = [0.0; 8];
            for (slot, &(joint, weight)) in influences.iter().enumerate() {
                joints[slot] = joint;
                weights[slot] = if sum > f32::EPSILON {
                    weight / sum
                } else {
                    0.0
                };
            }
            bone_indices.push(joints);
            bone_weights.push(weights);
            indices.push(indices.len() as u32);
        }
        let mut mesh = ImportedMesh {
            name,
            material,
            positions,
            normals,
            tangents: Vec::new(),
            bitangents: Vec::new(),
            uv_maps,
            colors,
            bone_indices,
            bone_weights,
            palette_bones,
            source_vertices: source_control_indices.clone(),
            indices,
        };
        deduplicate_vertices(&mut mesh, &source_control_indices);
        if mesh.positions.is_empty() || mesh.indices.is_empty() {
            continue;
        }
        calculate_tangents(&mut mesh)?;
        meshes.push(mesh);
    }
    if meshes.is_empty() {
        return Err(invalid(if strict_g1m {
            "FBX contains no armature-bound meshes"
        } else {
            "FBX contains no polygon meshes"
        }));
    }
    Ok(ImportedFbx { meshes, bones })
}

pub(crate) fn calculate_tangents(mesh: &mut ImportedMesh) -> io::Result<()> {
    if mesh.uv_maps.is_empty() {
        return Err(invalid("cannot calculate tangents without UVs"));
    }
    let mut tangents = vec![[0.0f32; 3]; mesh.positions.len()];
    let mut bitangents = vec![[0.0f32; 3]; mesh.positions.len()];
    for face in mesh.indices.chunks_exact(3) {
        let indices = [face[0] as usize, face[1] as usize, face[2] as usize];
        let [p1, p2, p3] = indices.map(|index| mesh.positions[index]);
        let [uv1, uv2, uv3] = indices.map(|index| mesh.uv_maps[0][index]);
        let edge1 = [p2[0] - p1[0], p2[1] - p1[1], p2[2] - p1[2]];
        let edge2 = [p3[0] - p1[0], p3[1] - p1[1], p3[2] - p1[2]];
        let s1 = uv2[0] - uv1[0];
        let s2 = uv3[0] - uv1[0];
        let t1 = uv2[1] - uv1[1];
        let t2 = uv3[1] - uv1[1];
        let divisor = s1 * t2 - s2 * t1;
        let reciprocal = 1.0 / divisor;
        let r = if reciprocal.is_infinite() {
            1.0
        } else {
            reciprocal
        };
        let mut tangent = [
            (t2 * edge1[0] - t1 * edge2[0]) * r,
            (t2 * edge1[1] - t1 * edge2[1]) * r,
            (t2 * edge1[2] - t1 * edge2[2]) * r,
        ];
        let mut bitangent = [
            (s1 * edge2[0] - s2 * edge1[0]) * r,
            (s1 * edge2[1] - s2 * edge1[1]) * r,
            (s1 * edge2[2] - s2 * edge1[2]) * r,
        ];
        if ((uv1[0] - uv2[0]).abs() < 0.00075 && (uv2[0] - uv3[0]).abs() < 0.00075)
            || ((uv1[1] - uv2[1]).abs() < 0.00075 && (uv2[1] - uv3[1]).abs() < 0.00075)
        {
            tangent = [1.0, 0.0, 0.0];
            bitangent = [0.0, 1.0, 0.0];
        }
        for index in indices {
            for axis in 0..3 {
                tangents[index][axis] += tangent[axis];
                bitangents[index][axis] += bitangent[axis];
            }
        }
    }
    let orthogonal = |value: [f32; 3], normal: [f32; 3]| {
        let dot = normal[0] * value[0] + normal[1] * value[1] + normal[2] * value[2];
        let mut result = [
            value[0] - normal[0] * dot,
            value[1] - normal[1] * dot,
            value[2] - normal[2] * dot,
        ];
        let length = ((result[0] * result[0] + result[1] * result[1] + result[2] * result[2])
            as f64)
            .sqrt() as f32;
        if length != 0.0 {
            let scale = 1.0 / length;
            for value in &mut result {
                *value *= scale;
            }
        }
        result
    };
    mesh.tangents = tangents
        .iter()
        .zip(&mesh.normals)
        .map(|(&value, &normal)| {
            let v = orthogonal(value, normal);
            [v[0], v[1], v[2], 1.0]
        })
        .collect();
    mesh.bitangents = bitangents
        .iter()
        .zip(&mesh.normals)
        .map(|(&value, &normal)| {
            let v = orthogonal(value, normal);
            [-v[0], -v[1], -v[2], -1.0]
        })
        .collect();
    Ok(())
}

fn deduplicate_vertices(mesh: &mut ImportedMesh, source_control_indices: &[usize]) {
    let mut unique = HashMap::<Vec<u32>, u32>::new();
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uv_maps = vec![Vec::new(); mesh.uv_maps.len()];
    let mut colors = vec![Vec::new(); mesh.colors.len()];
    let mut bone_indices = Vec::new();
    let mut bone_weights = Vec::new();
    let mut indices = Vec::with_capacity(mesh.positions.len());
    let mut source_vertices = Vec::new();

    for vertex in 0..mesh.positions.len() {
        let mut key = Vec::with_capacity(19 + mesh.uv_maps.len() * 2 + mesh.colors.len() * 4);
        key.push(source_control_indices[vertex] as u32);
        key.extend(mesh.positions[vertex].map(f32::to_bits));
        key.extend(mesh.normals[vertex].map(f32::to_bits));
        for uvs in &mesh.uv_maps {
            key.extend(uvs[vertex].map(f32::to_bits));
        }
        for values in &mesh.colors {
            key.extend(values[vertex].map(f32::to_bits));
        }
        key.extend(mesh.bone_indices[vertex].map(u32::from));
        key.extend(mesh.bone_weights[vertex].map(f32::to_bits));

        let index = *unique.entry(key).or_insert_with(|| {
            let index = positions.len() as u32;
            positions.push(mesh.positions[vertex]);
            normals.push(mesh.normals[vertex]);
            for (layer, source) in mesh.uv_maps.iter().enumerate() {
                uv_maps[layer].push(source[vertex]);
            }
            for (layer, source) in mesh.colors.iter().enumerate() {
                colors[layer].push(source[vertex]);
            }
            bone_indices.push(mesh.bone_indices[vertex]);
            bone_weights.push(mesh.bone_weights[vertex]);
            source_vertices.push(source_control_indices[vertex]);
            index
        });
        indices.push(index);
    }

    mesh.positions = positions;
    mesh.normals = normals;
    mesh.uv_maps = uv_maps;
    mesh.colors = colors;
    mesh.bone_indices = bone_indices;
    mesh.bone_weights = bone_weights;
    mesh.source_vertices = source_vertices;
    mesh.indices = indices;
}

fn child_f64_optional<'a>(node: &NodeHandle<'a>, name: &str) -> Option<&'a [f64]> {
    node.children_by_name(name)
        .next()
        .and_then(|v| v.attributes().get(0))
        .and_then(|v| v.get_arr_f64())
}
fn child_i32_optional<'a>(node: &NodeHandle<'a>, name: &str) -> Option<&'a [i32]> {
    node.children_by_name(name)
        .next()
        .and_then(|v| v.attributes().get(0))
        .and_then(|v| v.get_arr_i32())
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn imports_supplied_armature_bound_meshes() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/1/f23c0538.fbx");
        if !path.is_file() {
            return;
        }
        let imported = import_for_g1m(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(imported.meshes.len(), 10);
        assert_eq!(imported.bones.len(), 217);
        assert!(imported
            .meshes
            .iter()
            .all(|mesh| !mesh.positions.is_empty()));
    }
}
