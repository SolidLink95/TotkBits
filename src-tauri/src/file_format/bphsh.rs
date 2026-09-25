//! `.bphsh` documents: TOTK Phive mesh shapes (`Phive/Shape/Dcc/*.bphsh.zs`)
//! opened in the 3D tab. The shape is parsed by `parser::physics::bphsh`,
//! previewed as a binary glTF with one mesh per material (so the viewer can
//! hide or select each material group), and written back through the
//! byte-exact writer. The material tables (material id, user shape tags,
//! collision mask) are editable in place; the geometry can be exported to
//! OBJ (plus the material JSON side file) or replaced from an OBJ, which
//! rebuilds the shape with Havok's mesh builder.

use crate::file_format::BinTextFile::OpenedFile;
use crate::parser::physics::bphsh::{
    builder, materials, obj,
    reader::{self, TOTK_SDK_VERSION},
    shape::{BphshShape, MaterialEntry},
    writer,
};
use crate::Open_and_Save::SendData;
use crate::Settings::Pathlib;
use crate::Zstd::{TotkFileType, TotkZstd, ZstdDictionary};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

/// One row of the editable material table, keyed by the OBJ group name
/// (`Stone00`, `Stone01`, …) the OBJ exchange uses.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BphshMaterialRow {
    pub key: String,
    #[serde(default)]
    pub mat_name: String,
    #[serde(default)]
    pub mat_flags: Vec<String>,
    #[serde(default)]
    pub col_disable_flags: Vec<String>,
    #[serde(default)]
    pub triangles: usize,
    #[serde(default)]
    pub material_id: i32,
    #[serde(default)]
    pub flags_hex: String,
    #[serde(default)]
    pub collision_mask_hex: String,
}

/// What the material panel needs: the rows plus the name tables to offer.
#[derive(Clone, Debug, Serialize)]
pub struct BphshInfo {
    pub materials: Vec<BphshMaterialRow>,
    pub material_names: Vec<&'static str>,
    pub flag_names: Vec<&'static str>,
    pub layer_names: Vec<&'static str>,
    pub vertices: usize,
    pub triangles: usize,
    pub sections: usize,
    pub primitives: usize,
}

#[derive(Clone, Debug)]
pub struct BphshFile {
    pub shape: BphshShape,
    type_section: Vec<u8>,
    sdk_version: [u8; 8],
}

impl BphshFile {
    pub fn from_binary(bytes: &[u8]) -> io::Result<Self> {
        let parsed = reader::parse(bytes)?;
        let mut sdk_version = *TOTK_SDK_VERSION;
        if parsed.sdk_version.len() == sdk_version.len() {
            sdk_version.copy_from_slice(&parsed.sdk_version);
        }
        Ok(Self {
            shape: parsed.shape,
            type_section: parsed.type_section,
            sdk_version,
        })
    }

    /// The uncompressed file (byte for byte for an unmodified shape).
    pub fn raw_binary(&self) -> io::Result<Vec<u8>> {
        writer::write_with_type_section(&self.shape, &self.type_section, &self.sdk_version)
    }

    fn geometry(&self) -> obj::Geometry {
        obj::indexed_geometry(&self.shape)
    }

    fn group_keys(&self) -> Vec<String> {
        obj::group_names(&self.geometry().materials)
    }

    pub fn info(&self) -> BphshInfo {
        let geometry = self.geometry();
        let keys = obj::group_names(&geometry.materials);
        let mut counts = vec![0usize; geometry.materials.len()];
        for triangle in &geometry.triangles {
            if let Some(count) = counts.get_mut(triangle.material as usize) {
                *count += 1;
            }
        }
        let rows = geometry
            .materials
            .iter()
            .zip(keys)
            .zip(counts)
            .map(|((material, key), triangles)| {
                let info = obj::MaterialInfo::from_material(material);
                BphshMaterialRow {
                    key,
                    mat_name: info.mat_name.unwrap_or_default(),
                    mat_flags: info.mat_flags,
                    col_disable_flags: info.col_disable_flags,
                    triangles,
                    material_id: material.material_id,
                    flags_hex: format!("{:#x}", material.flags),
                    collision_mask_hex: format!("{:#x}", material.collision_mask),
                }
            })
            .collect();
        BphshInfo {
            materials: rows,
            material_names: materials::MATERIAL_NAMES.to_vec(),
            flag_names: materials::USER_SHAPE_TAGS
                .iter()
                .map(|(name, _)| *name)
                .collect(),
            layer_names: materials::LAYER_ENTITY_NAMES.to_vec(),
            vertices: geometry.vertices.len(),
            triangles: geometry.triangles.len(),
            sections: self.shape.shape.sections.len(),
            primitives: self
                .shape
                .shape
                .sections
                .iter()
                .map(|section| section.primitives.len())
                .sum(),
        }
    }

    /// Rewrites the material tables from the panel rows. Rows are matched by
    /// key, so reordering is harmless; every existing material must be
    /// present because the primitives index the table by position.
    pub fn apply_materials(&mut self, rows: &[BphshMaterialRow]) -> Result<(), String> {
        let keys = self.group_keys();
        let by_key: BTreeMap<&str, &BphshMaterialRow> =
            rows.iter().map(|row| (row.key.as_str(), row)).collect();
        let mut entries = Vec::with_capacity(keys.len());
        let mut masks = Vec::with_capacity(keys.len());
        for (index, key) in keys.iter().enumerate() {
            let row = by_key
                .get(key.as_str())
                .ok_or_else(|| format!("material {key} is missing from the table"))?;
            let info = obj::MaterialInfo {
                mat_name: Some(row.mat_name.clone()),
                mat_flags: row.mat_flags.clone(),
                col_disable_flags: row.col_disable_flags.clone(),
            };
            let material = info
                .to_material()
                .map_err(|error| format!("material {key}: {error}"))?;
            entries.push(MaterialEntry {
                material_id: material.material_id,
                reserved: self
                    .shape
                    .materials
                    .get(index)
                    .map(|entry| entry.reserved)
                    .unwrap_or(0),
                flags: material.flags,
            });
            masks.push(material.collision_mask);
        }
        self.shape.materials = entries;
        self.shape.collision_masks = masks;
        Ok(())
    }

    /// Writes `<output>.obj` and the material JSON next to it; returns both paths.
    pub fn export_obj(&self, output: &Path) -> io::Result<(PathBuf, PathBuf)> {
        let obj_path = if output
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("obj"))
        {
            output.to_path_buf()
        } else {
            output.with_extension("obj")
        };
        let json_path = obj_path.with_extension("json");
        let (text, json) = obj::to_obj(&self.geometry())?;
        if let Some(parent) = obj_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(&obj_path, text)?;
        fs::write(&json_path, json)?;
        Ok((obj_path, json_path))
    }

    /// Rebuilds the shape from an OBJ. Materials come from `<obj>.json` when
    /// it exists; groups it does not describe keep the current material of
    /// the same name (so a re-import of an export needs no side file) and
    /// otherwise get the default material.
    pub fn replace_from_obj(&mut self, obj_path: &Path) -> Result<usize, String> {
        let text = fs::read_to_string(obj_path)
            .map_err(|error| format!("failed to read {}: {error}", obj_path.display()))?;
        let (_, current_json) = obj::to_obj(&self.geometry()).map_err(|e| e.to_string())?;
        let mut infos: BTreeMap<String, serde_json::Value> =
            serde_json::from_str(&current_json).map_err(|e| e.to_string())?;
        let sidecar = obj_path.with_extension("json");
        if sidecar.is_file() {
            let text = fs::read_to_string(&sidecar)
                .map_err(|error| format!("failed to read {}: {error}", sidecar.display()))?;
            let overlay: BTreeMap<String, serde_json::Value> = serde_json::from_str(&text)
                .map_err(|error| format!("{}: {error}", sidecar.display()))?;
            infos.extend(overlay);
        }
        let json = serde_json::to_string(&infos).map_err(|e| e.to_string())?;
        let geometry = obj::from_obj(&text, Some(&json)).map_err(|e| e.to_string())?;
        let options = builder::BuildOptions {
            weld: true,
            canonical: true,
            ..Default::default()
        };
        let mesh = builder::build_with_options(&geometry, options)
            .ok_or_else(|| "the OBJ holds no usable triangles".to_string())?;
        let (material_table, collision_masks) = obj::material_tables(&geometry);
        self.shape = BphshShape {
            shape: mesh,
            materials: material_table,
            collision_masks,
        };
        Ok(geometry.triangles.len())
    }

    /// A binary glTF preview: one flat-shaded mesh per material group,
    /// tinted by material so the viewer tells stone from air walls.
    pub fn glb(&self, name: &str) -> Vec<u8> {
        let geometry = self.geometry();
        let keys = obj::group_names(&geometry.materials);
        glb_from_geometry(name, &geometry, &keys)
    }

    fn send_data(path: &Path, compression: ZstdDictionary, status: String) -> SendData {
        let mut data = SendData::default();
        data.path = Pathlib::new(path);
        data.file_label = format!("{} [BPHSH]", data.path.name);
        data.file_metadata = "[BPHSH] [3D]".into();
        if compression != ZstdDictionary::None {
            data.file_metadata += &format!(" [{compression:?}]");
        }
        data.file_type = TotkFileType::Bphsh;
        data.status_text = status;
        data.tab = "3D".into();
        data.read_only = false;
        data
    }

    fn opened(file: Self, path: &Path, compression: ZstdDictionary) -> OpenedFile<'static> {
        let mut opened = OpenedFile::default();
        opened.file_type = TotkFileType::Bphsh;
        opened.path = Pathlib::new(path);
        opened.compression = (compression != ZstdDictionary::None).then_some(compression);
        opened.visual_data = Some(file.glb(&opened.path.name));
        opened.bphsh = Some(file);
        opened
    }

    pub fn open(path: &Path, zstd: Arc<TotkZstd>) -> Option<(OpenedFile<'static>, SendData)> {
        let bytes = fs::read(path).ok()?;
        Self::open_binary(&bytes, path, zstd)
    }

    pub fn open_binary(
        bytes: &[u8],
        path: &Path,
        zstd: Arc<TotkZstd>,
    ) -> Option<(OpenedFile<'static>, SendData)> {
        let (source, compression) = zstd.try_decompress_all_ordered_safe(bytes, path);
        if !reader::is_phive(&source) || !crate::Settings::Magic::is_bphsh(&source) {
            return None;
        }
        let file = match Self::from_binary(&source) {
            Ok(file) => file,
            Err(error) => {
                eprintln!("[bphsh] {}: {error}", path.display());
                return None;
            }
        };
        let status = format!("Opened BPHSH {}", path.display());
        let data = Self::send_data(path, compression, status);
        Some((Self::opened(file, path, compression), data))
    }
}

/// Preview tint of a material group: fixed colors for the common TOTK
/// materials, a stable hashed hue for the rest, shifted a little by the
/// group ordinal so `Stone00` and `Stone01` stay distinguishable.
fn material_color(material: &obj::Material, ordinal: usize) -> [f32; 4] {
    let name = materials::material_name(material.material_id).unwrap_or("");
    let (rgb, alpha): ([u8; 3], f32) = match name {
        "Undefined" => ([0x9a, 0x9a, 0x9a], 1.0),
        "Soil" | "Bog" => ([0x8b, 0x6b, 0x4a], 1.0),
        "Grass" => ([0x5a, 0xa0, 0x4a], 1.0),
        "Sand" => ([0xd8, 0xc4, 0x8a], 1.0),
        "HeavySand" => ([0xc9, 0xb0, 0x6e], 1.0),
        "Snow" | "HeavySnow" => ([0xee, 0xf3, 0xf8], 1.0),
        "Stone" | "StoneSlip" | "StoneNoSlip" => ([0x8d, 0x93, 0x9c], 1.0),
        "Metal" | "MetalSlip" | "MetalNoSlip" | "WireNet" => ([0x7f, 0x8f, 0xa6], 1.0),
        "Wood" => ([0xa8, 0x74, 0x3f], 1.0),
        "Ice" | "IceWater" => ([0xa9, 0xd8, 0xef], 1.0),
        "Cloth" => ([0xc2, 0x6f, 0x9d], 1.0),
        "Glass" => ([0x8f, 0xd0, 0xe6], 0.6),
        "Water" | "HotWater" | "WaterSlip" | "ContaminatedWater" => ([0x3d, 0x7f, 0xd6], 0.7),
        "Lava" => ([0xe2, 0x51, 0x2b], 1.0),
        "Tar" | "Grudge" | "GrudgeSlow" => ([0x4a, 0x3d, 0x33], 1.0),
        "AirWall" | "Barrier" | "Gas" => ([0x58, 0xd0, 0xd8], 0.45),
        "Dragon" => ([0xd9, 0xb2, 0x3c], 1.0),
        "Rope" => ([0xc4, 0xa7, 0x6b], 1.0),
        "Bone" => ([0xe6, 0xdc, 0xc3], 1.0),
        "Rail" | "Cart" | "Conveyer" | "SlipBoard" | "LaunchPad" => ([0x6f, 0x7c, 0x8a], 1.0),
        "Meat" | "Vegetable" => ([0xc9, 0x7b, 0x5a], 1.0),
        "Character" | "Ragdoll" => ([0xe0, 0xa8, 0x8c], 1.0),
        _ => {
            // Golden-ratio hue spread keeps unlisted ids apart.
            let hue = ((material.material_id.unsigned_abs() as f32) * 0.618_034) % 1.0;
            let (r, g, b) = hsl_to_rgb(hue, 0.55, 0.55);
            ([r, g, b], 1.0)
        }
    };
    // ±6% lightness per ordinal, cycling every four groups.
    let shift = 1.0 + 0.06 * ((ordinal % 4) as f32 - 1.5);
    let channel = |value: u8| (value as f32 / 255.0 * shift).clamp(0.0, 1.0);
    [channel(rgb[0]), channel(rgb[1]), channel(rgb[2]), alpha]
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to_byte = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    (to_byte(r), to_byte(g), to_byte(b))
}

/// Serializes `geometry` as a self-contained GLB (glTF 2.0) with one node,
/// mesh and material per group. Vertices stay indexed (a 216k-triangle
/// dungeon corridor is 4 MB this way instead of 18 MB unrolled, and the
/// payload crosses the IPC bridge as base64), and normals are left out so
/// the viewer derives them; 16-bit indices are used whenever a group fits.
fn glb_from_geometry(name: &str, geometry: &obj::Geometry, keys: &[String]) -> Vec<u8> {
    use serde_json::json;
    use std::collections::HashMap;
    let mut bin: Vec<u8> = Vec::new();
    let mut buffer_views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    let mut gltf_materials = Vec::new();

    let mut push_view = |bin: &mut Vec<u8>, bytes: &[u8], target: u32| -> usize {
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let offset = bin.len();
        bin.extend_from_slice(bytes);
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": offset,
            "byteLength": bytes.len(),
            "target": target,
        }));
        buffer_views.len() - 1
    };

    let material_count = geometry.materials.len().max(1);
    let mut ordinals: BTreeMap<i32, usize> = BTreeMap::new();
    for m in 0..material_count {
        let material = geometry.materials.get(m).copied().unwrap_or_default();
        let key = keys
            .get(m)
            .cloned()
            .unwrap_or_else(|| format!("Material{m:02}"));
        let ordinal = ordinals.entry(material.material_id).or_default();
        let color = material_color(&material, *ordinal);
        *ordinal += 1;
        let mut gltf_material = json!({
            "name": key,
            "doubleSided": true,
            "pbrMetallicRoughness": {
                "baseColorFactor": color,
                "metallicFactor": 0.0,
                "roughnessFactor": 0.9,
            },
        });
        if color[3] < 1.0 {
            gltf_material["alphaMode"] = json!("BLEND");
        }
        gltf_materials.push(gltf_material);

        // Remap the group's vertices to a compact range of their own.
        let mut remap: HashMap<u32, u32> = HashMap::new();
        let mut positions: Vec<f32> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for t in geometry
            .triangles
            .iter()
            .filter(|t| t.material as usize == m)
        {
            for source in [t.a, t.b, t.c] {
                let index = *remap.entry(source).or_insert_with(|| {
                    let p = geometry
                        .vertices
                        .get(source as usize)
                        .copied()
                        .unwrap_or([0.0; 3]);
                    positions.extend_from_slice(&p);
                    for axis in 0..3 {
                        min[axis] = min[axis].min(p[axis]);
                        max[axis] = max[axis].max(p[axis]);
                    }
                    (positions.len() / 3 - 1) as u32
                });
                indices.push(index);
            }
        }
        if indices.is_empty() {
            continue;
        }
        let vertex_count = positions.len() / 3;
        let position_bytes: Vec<u8> = positions.iter().flat_map(|v| v.to_le_bytes()).collect();
        let (index_bytes, index_type): (Vec<u8>, u32) = if vertex_count <= u16::MAX as usize {
            (
                indices
                    .iter()
                    .flat_map(|v| (*v as u16).to_le_bytes())
                    .collect(),
                5123,
            )
        } else {
            (indices.iter().flat_map(|v| v.to_le_bytes()).collect(), 5125)
        };
        let position_view = push_view(&mut bin, &position_bytes, 34962);
        let index_view = push_view(&mut bin, &index_bytes, 34963);
        let position_accessor = accessors.len();
        accessors.push(json!({
            "bufferView": position_view,
            "componentType": 5126,
            "count": vertex_count,
            "type": "VEC3",
            "min": min,
            "max": max,
        }));
        let index_accessor = accessors.len();
        accessors.push(json!({
            "bufferView": index_view,
            "componentType": index_type,
            "count": indices.len(),
            "type": "SCALAR",
        }));
        let mesh_index = meshes.len();
        meshes.push(json!({
            "name": key,
            "primitives": [{
                "attributes": { "POSITION": position_accessor },
                "indices": index_accessor,
                "material": m,
                "mode": 4,
            }],
        }));
        nodes.push(json!({ "name": key, "mesh": mesh_index }));
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let scene_nodes: Vec<usize> = (0..nodes.len()).collect();
    let document = json!({
        "asset": { "version": "2.0", "generator": "TotkBits bphsh preview" },
        "scene": 0,
        "scenes": [{ "name": name, "nodes": scene_nodes }],
        "nodes": nodes,
        "meshes": meshes,
        "materials": gltf_materials,
        "accessors": accessors,
        "bufferViews": buffer_views,
        "buffers": [{ "byteLength": bin.len() }],
    });
    let mut json_bytes = serde_json::to_vec(&document).unwrap_or_default();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    out.extend_from_slice(&bin);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::physics::bphsh::obj::{Geometry, Material, Triangle};

    fn cube() -> Geometry {
        let mut geometry = Geometry::default();
        for z in [0.0f32, 1.0] {
            for y in [0.0f32, 1.0] {
                for x in [0.0f32, 1.0] {
                    geometry.vertices.push([x, y, z]);
                }
            }
        }
        let quads = [
            [0, 1, 3, 2],
            [4, 6, 7, 5],
            [0, 4, 5, 1],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 5, 7, 3],
        ];
        for (i, q) in quads.iter().enumerate() {
            let material = (i % 2) as u32;
            geometry.triangles.push(Triangle {
                a: q[0],
                b: q[1],
                c: q[2],
                material,
            });
            geometry.triangles.push(Triangle {
                a: q[0],
                b: q[2],
                c: q[3],
                material,
            });
        }
        geometry.materials = vec![
            Material {
                material_id: 7,
                flags: 0x8000,
                collision_mask: u64::MAX,
            },
            Material {
                material_id: 35,
                flags: 0,
                collision_mask: !0b1000,
            },
        ];
        geometry
    }

    fn file_from(geometry: &Geometry) -> BphshFile {
        let shape = builder::build(geometry).expect("cube builds");
        let (materials, collision_masks) = obj::material_tables(geometry);
        let bytes = writer::write(&BphshShape {
            shape,
            materials,
            collision_masks,
        })
        .unwrap();
        BphshFile::from_binary(&bytes).unwrap()
    }

    #[test]
    fn info_rows_name_materials_and_count_triangles() {
        let file = file_from(&cube());
        let info = file.info();
        assert_eq!(info.materials.len(), 2);
        assert_eq!(info.materials[0].key, "Stone00");
        assert_eq!(info.materials[0].mat_name, "Stone");
        assert_eq!(info.materials[0].mat_flags, vec!["NoStick".to_string()]);
        assert_eq!(info.materials[0].triangles, 6);
        assert_eq!(info.materials[1].key, "AirWall00");
        assert_eq!(
            info.materials[1].col_disable_flags,
            vec!["Player".to_string()]
        );
        assert_eq!(info.triangles, 12);
    }

    #[test]
    fn apply_materials_rewrites_tables_and_survives_resave() {
        let mut file = file_from(&cube());
        let mut rows = file.info().materials;
        rows[1].mat_name = "Wood".into();
        rows[1].mat_flags = vec!["NoClimb".into(), "0x40".into()];
        rows[1].col_disable_flags = vec!["NoHit".into()];
        rows.swap(0, 1);
        file.apply_materials(&rows).unwrap();
        let reread = BphshFile::from_binary(&file.raw_binary().unwrap()).unwrap();
        let info = reread.info();
        assert_eq!(info.materials[0].mat_name, "Stone");
        assert_eq!(info.materials[1].mat_name, "Wood");
        assert_eq!(info.materials[1].material_id, 16);
        assert_eq!(reread.shape.materials[1].flags, 0x41);
        assert!(info.materials[1]
            .col_disable_flags
            .contains(&"NoHit".to_string()));
        let missing = vec![rows[0].clone()];
        assert!(file.apply_materials(&missing).is_err());
    }

    #[test]
    fn glb_preview_has_one_mesh_per_used_material() {
        let file = file_from(&cube());
        let glb = file.glb("cube");
        assert_eq!(&glb[..4], b"glTF");
        let json_length = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        let document: serde_json::Value =
            serde_json::from_slice(&glb[20..20 + json_length]).unwrap();
        assert_eq!(document["meshes"].as_array().unwrap().len(), 2);
        assert_eq!(document["materials"][1]["alphaMode"], "BLEND");
        assert_eq!(document["accessors"][0]["count"], 7);
        assert_eq!(document["accessors"][1]["componentType"], 5123);
        assert_eq!(
            u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize,
            glb.len()
        );
    }

    #[test]
    fn obj_round_trip_keeps_materials_without_side_file() {
        let file = file_from(&cube());
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/_CLAUDE")
            .join(format!("bphsh_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let (obj_path, json_path) = file.export_obj(&dir.join("cube")).unwrap();
        assert!(obj_path.ends_with("cube.obj") && json_path.is_file());
        fs::remove_file(&json_path).unwrap();
        let mut replaced = file.clone();
        let triangles = replaced.replace_from_obj(&obj_path).unwrap();
        assert_eq!(triangles, 12);
        let info = replaced.info();
        assert_eq!(info.materials.len(), 2);
        assert_eq!(info.materials[1].mat_name, "AirWall");
        assert_eq!(
            info.materials[1].col_disable_flags,
            vec!["Player".to_string()]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// The real disk opener on a vanilla shape from the decompressed corpus
    /// (`tmp/_bphsh`, see `parser::physics::bphsh::tests`): magic detection,
    /// the 3D SendData, a non-empty GLB preview, and a byte-exact resave.
    #[test]
    fn opens_corpus_file_and_resaves_byte_identical() {
        let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/_bphsh");
        let Some(path) = fs::read_dir(&corpus).ok().and_then(|entries| {
            let mut paths: Vec<PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "bphsh"))
                .collect();
            paths.sort();
            paths.into_iter().next()
        }) else {
            eprintln!("bphsh corpus missing at {}", corpus.display());
            return;
        };
        let bytes = fs::read(&path).unwrap();
        assert!(crate::Settings::Magic::is_bphsh(&bytes));
        assert_eq!(
            crate::Settings::Magic::from_binary(&bytes),
            TotkFileType::Bphsh
        );
        let zstd = Arc::new(TotkZstd::dictionaryless(
            Arc::new(crate::TotkConfig::TotkConfig::default()),
            crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
        ));
        let (opened, data) = BphshFile::open(&path, zstd).expect("corpus file opens");
        assert_eq!(data.tab, "3D");
        assert_eq!(data.file_type, TotkFileType::Bphsh);
        assert!(!data.read_only);
        assert!(opened
            .visual_data
            .as_ref()
            .is_some_and(|glb| glb.starts_with(b"glTF")));
        let file = opened.bphsh.as_ref().unwrap();
        assert!(!file.info().materials.is_empty());
        assert_eq!(file.raw_binary().unwrap(), bytes, "{}", path.display());
    }

    /// `BPHSH_PREVIEW_FILE=<file.bphsh> cargo test --release report_preview_size -- --ignored --nocapture`:
    /// prints the preview payload size and build time of one shape.
    #[test]
    #[ignore]
    fn report_preview_size() {
        let Ok(path) = std::env::var("BPHSH_PREVIEW_FILE") else {
            return;
        };
        let bytes = fs::read(&path).unwrap();
        let started = std::time::Instant::now();
        let file = BphshFile::from_binary(&bytes).unwrap();
        let parsed = started.elapsed();
        let glb = file.glb("shape");
        let built = started.elapsed();
        if let Ok(out) = std::env::var("BPHSH_PREVIEW_OUT") {
            fs::write(out, &glb).unwrap();
        }
        let info = file.info();
        println!(
            "{path}: {} bytes in, {} triangles, GLB {} bytes ({} base64), parse {parsed:?}, parse+glb {built:?}, +info {:?}",
            bytes.len(),
            info.triangles,
            glb.len(),
            glb.len() / 3 * 4,
            started.elapsed()
        );
    }
}
