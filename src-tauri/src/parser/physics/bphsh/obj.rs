//! Geometry exchange for mesh shapes: the triangle soup a shape is built
//! from ([`Geometry`]), its extraction from a parsed shape (Havok's
//! `buildSurfaceGeometry`), and Wavefront OBJ + material JSON files in the
//! layout PhiveConverter established:
//!
//! ```text
//! v x y z            (every triangle written with its own three vertices)
//! o Stone00
//! usemtl Stone00
//! f 1 2 3
//! ```
//!
//! ```json
//! { "Stone00": { "mat_name": "Stone",
//!                "mat_flags": ["NoClimb"],
//!                "col_disable_flags": ["Player"] } }
//! ```
//!
//! `mat_name` is a TOTK `MaterialCollection` name (or a number), `mat_flags`
//! lists user shape tags (names from PhiveConfig or hex literals) and
//! `col_disable_flags` the collision layers the material ignores.

use super::materials;
use super::shape::{BphshShape, MaterialEntry, MeshShape};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, ErrorKind};

/// A triangle of `Geometry`, indices into `vertices`, `material` indexing
/// [`Geometry::materials`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Triangle {
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub material: u32,
}

/// One material of a shape: the row of the Phive material tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Material {
    pub material_id: i32,
    pub flags: u64,
    pub collision_mask: u64,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            material_id: 0,
            flags: 0,
            collision_mask: u64::MAX,
        }
    }
}

/// A triangle soup with per-triangle materials (Havok's `hkGeometry`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<Triangle>,
    pub materials: Vec<Material>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message.into())
}

/// Material tables from a geometry's materials.
pub fn material_tables(geometry: &Geometry) -> (Vec<MaterialEntry>, Vec<u64>) {
    let materials: Vec<Material> = if geometry.materials.is_empty() {
        vec![Material::default()]
    } else {
        geometry.materials.clone()
    };
    (
        materials
            .iter()
            .map(|m| MaterialEntry {
                material_id: m.material_id,
                reserved: 0,
                flags: m.flags,
            })
            .collect(),
        materials.iter().map(|m| m.collision_mask).collect(),
    )
}

/// One material's description in the JSON side file.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct MaterialInfo {
    #[serde(default)]
    pub mat_name: Option<String>,
    #[serde(default)]
    pub mat_flags: Vec<String>,
    #[serde(default)]
    pub col_disable_flags: Vec<String>,
}

impl MaterialInfo {
    pub fn from_material(material: &Material) -> Self {
        Self {
            mat_name: Some(
                materials::material_name(material.material_id)
                    .map(str::to_owned)
                    .unwrap_or_else(|| material.material_id.to_string()),
            ),
            mat_flags: materials::flag_names(material.flags),
            col_disable_flags: materials::disabled_layer_names(material.collision_mask),
        }
    }

    pub fn to_material(&self) -> Result<Material, String> {
        let material_id = match self.mat_name.as_deref().map(str::trim) {
            None | Some("") => 0,
            Some(name) => match name.parse::<i32>() {
                Ok(id) => id,
                Err(_) => materials::material_id(name)
                    .ok_or_else(|| format!("unknown material {name}"))?,
            },
        };
        Ok(Material {
            material_id,
            flags: materials::flags_from_names(self.mat_flags.iter().map(String::as_str))?,
            collision_mask: materials::mask_from_disabled_names(
                self.col_disable_flags.iter().map(String::as_str),
            )?,
        })
    }
}

/// The OBJ group name of material `index`: `<name><two-digit ordinal>`.
fn group_name(material: &Material, used: &mut BTreeMap<String, usize>) -> String {
    let name = materials::material_name(material.material_id)
        .map(str::to_owned)
        .unwrap_or_else(|| "UNDEFINED".to_owned());
    let count = used.entry(name.clone()).or_default();
    let group = format!("{name}{count:02}");
    *count += 1;
    group
}

/// The OBJ group names of `materials`, in table order (`Stone00`, `Stone01`, …).
pub fn group_names(materials: &[Material]) -> Vec<String> {
    let mut used = BTreeMap::new();
    materials
        .iter()
        .map(|material| group_name(material, &mut used))
        .collect()
}

/// Writes `geometry` as OBJ text plus the material JSON (pretty printed).
pub fn to_obj(geometry: &Geometry) -> io::Result<(String, String)> {
    use std::fmt::Write;
    let mut obj = String::with_capacity(geometry.vertices.len() * 32);
    for v in &geometry.vertices {
        writeln!(obj, "v {:.6} {:.6} {:.6}", v[0], v[1], v[2]).unwrap();
    }
    let materials: Vec<Material> = if geometry.materials.is_empty() {
        vec![Material::default()]
    } else {
        geometry.materials.clone()
    };
    let mut used = BTreeMap::new();
    let mut infos: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    for (m, material) in materials.iter().enumerate() {
        let group = group_name(material, &mut used);
        writeln!(obj, "o {group}").unwrap();
        writeln!(obj, "usemtl {group}").unwrap();
        for t in geometry
            .triangles
            .iter()
            .filter(|t| t.material as usize == m)
        {
            writeln!(obj, "f {} {} {}", t.a + 1, t.b + 1, t.c + 1).unwrap();
        }
        infos.insert(
            group,
            serde_json::to_value(MaterialInfo::from_material(material))
                .map_err(|e| invalid(e.to_string()))?,
        );
    }
    let json = serde_json::to_string_pretty(&serde_json::Value::Object(infos))
        .map_err(|e| invalid(e.to_string()))?;
    Ok((obj, json))
}

/// Parses OBJ text (with `usemtl` groups) and the optional material JSON
/// into a geometry. Polygons are fan-triangulated; a group without a JSON
/// entry gets the default material.
pub fn from_obj(obj: &str, json: Option<&str>) -> io::Result<Geometry> {
    let infos: BTreeMap<String, MaterialInfo> = match json {
        Some(text) if !text.trim().is_empty() => {
            serde_json::from_str(text).map_err(|e| invalid(format!("material json: {e}")))?
        }
        _ => BTreeMap::new(),
    };
    let mut geometry = Geometry::default();
    let mut groups: BTreeMap<String, u32> = BTreeMap::new();
    let mut current = 0u32;
    for (line_number, raw) in obj.lines().enumerate() {
        let line = raw.trim();
        let mut parts = line.split_whitespace();
        let Some(first) = parts.next() else { continue };
        match first {
            "usemtl" => {
                let name = line[6..].trim().to_owned();
                current = match groups.get(&name) {
                    Some(index) => *index,
                    None => {
                        let material = match infos.get(&name) {
                            Some(info) => info
                                .to_material()
                                .map_err(|e| invalid(format!("material {name}: {e}")))?,
                            None => Material::default(),
                        };
                        geometry.materials.push(material);
                        let index = geometry.materials.len() as u32 - 1;
                        groups.insert(name, index);
                        index
                    }
                };
            }
            "v" => {
                let mut xyz = [0f32; 3];
                for (i, value) in parts.take(3).enumerate() {
                    xyz[i] = value.parse().map_err(|_| {
                        invalid(format!("line {}: bad vertex {line}", line_number + 1))
                    })?;
                }
                geometry.vertices.push(xyz);
            }
            "f" => {
                if geometry.materials.is_empty() {
                    geometry.materials.push(Material::default());
                }
                let indices: Vec<u32> = parts
                    .map(|corner| {
                        let index_text = corner.split('/').next().unwrap_or("");
                        let index: i64 = index_text.parse().map_err(|_| {
                            invalid(format!("line {}: bad face {line}", line_number + 1))
                        })?;
                        let resolved = if index < 0 {
                            geometry.vertices.len() as i64 + index
                        } else {
                            index - 1
                        };
                        if resolved < 0 || resolved >= geometry.vertices.len() as i64 {
                            return Err(invalid(format!(
                                "line {}: vertex index out of range in {line}",
                                line_number + 1
                            )));
                        }
                        Ok(resolved as u32)
                    })
                    .collect::<io::Result<_>>()?;
                for i in 1..indices.len().saturating_sub(1) {
                    geometry.triangles.push(Triangle {
                        a: indices[0],
                        b: indices[i],
                        c: indices[i + 1],
                        material: current,
                    });
                }
            }
            _ => {}
        }
    }
    if geometry.vertices.is_empty() || geometry.triangles.is_empty() {
        return Err(invalid("the OBJ holds no faces"));
    }
    Ok(geometry)
}

/// Unpacks `shape` into an *indexed* triangle soup that keeps the vertex
/// identity TOTK's builder saw: every entry of a section vertex buffer is
/// its own vertex, and entries of different sections are merged when they
/// hold the same position and that position occurs once in each section
/// (a position duplicated inside a section came from distinct source
/// vertices, so its copies stay apart).
pub fn indexed_geometry(shape: &BphshShape) -> Geometry {
    use std::collections::HashMap;
    let mesh: &MeshShape = &shape.shape;
    let mut geometry = Geometry::default();
    let mut shared: HashMap<[i32; 3], u32> = HashMap::new();
    // 16-bit sections first: a float-section vertex whose grid point already
    // exists joins it (keeping the snapped position), so the merge does not
    // depend on section order and a rebuild of the result is stable.
    let mut order: Vec<usize> = (0..mesh.sections.len())
        .filter(|s| !mesh.sections[*s].has_float_vertices())
        .collect();
    order.extend((0..mesh.sections.len()).filter(|s| mesh.sections[*s].has_float_vertices()));
    for s in order {
        let section = &mesh.sections[s];
        let positions: Vec<[f32; 3]> = (0..section.vertex_count())
            .map(|i| mesh.vertex_position(section, i))
            .collect();
        // Merge key: the quantized grid coordinate, for float sections too.
        let keys: Vec<[i32; 3]> = if section.has_float_vertices() {
            positions
                .iter()
                .map(|p| {
                    [
                        (p[0] * mesh.bit_scale16[0]).round() as i32,
                        (p[1] * mesh.bit_scale16[1]).round() as i32,
                        (p[2] * mesh.bit_scale16[2]).round() as i32,
                    ]
                })
                .collect()
        } else {
            section
                .vertices
                .iter()
                .map(|v| {
                    [
                        (v[0] as i32).wrapping_add(section.section_offset[0] as i32),
                        (v[1] as i32).wrapping_add(section.section_offset[1] as i32),
                        (v[2] as i32).wrapping_add(section.section_offset[2] as i32),
                    ]
                })
                .collect()
        };
        let mut counts: HashMap<[i32; 3], u32> = HashMap::new();
        for key in &keys {
            *counts.entry(*key).or_default() += 1;
        }
        let local: Vec<u32> = keys
            .iter()
            .enumerate()
            .map(|(i, key)| {
                let position = positions[i];
                if counts[key] == 1 {
                    if let Some(index) = shared.get(key) {
                        return *index;
                    }
                    geometry.vertices.push(position);
                    let index = geometry.vertices.len() as u32 - 1;
                    shared.insert(*key, index);
                    index
                } else {
                    geometry.vertices.push(position);
                    geometry.vertices.len() as u32 - 1
                }
            })
            .collect();
        for (p, primitive) in section.primitives.iter().enumerate() {
            let material = mesh.shape_tag_of(s, p) as u32;
            let ids = primitive.ids();
            let corner = |id: u8| local.get(id as usize).copied().unwrap_or(0);
            geometry.triangles.push(Triangle {
                a: corner(ids[0]),
                b: corner(ids[1]),
                c: corner(ids[2]),
                material,
            });
            if !primitive.is_triangle() {
                geometry.triangles.push(Triangle {
                    a: corner(ids[0]),
                    b: corner(ids[2]),
                    c: corner(ids[3]),
                    material,
                });
            }
        }
    }
    geometry.materials = surface_materials(shape);
    geometry
}

fn surface_materials(shape: &BphshShape) -> Vec<Material> {
    shape
        .materials
        .iter()
        .enumerate()
        .map(|(i, m)| Material {
            material_id: m.material_id,
            flags: m.flags,
            collision_mask: shape.collision_masks.get(i).copied().unwrap_or(u64::MAX),
        })
        .collect()
}
