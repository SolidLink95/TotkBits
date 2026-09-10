use super::{read_string, u16_at, u32_at, u64_at, BfresSection, Endian};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BfresTextureSlot {
    pub index: usize,
    pub name: String,
    pub sampler: String,
    pub texture_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BfresKeyValue {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BfresShaderParam {
    pub name: String,
    pub kind: String,
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BfresMaterial {
    pub name: String,
    pub offset: u64,
    pub texture_slots: Vec<BfresTextureSlot>,
    pub shader_archive: String,
    pub shading_model: String,
    /// Shader option name -> value (`elsa_enable_insert_color`, ...).
    pub shader_options: Vec<BfresKeyValue>,
    /// Shader sampler key -> material sampler name (`_s0` -> `_tcl0`).
    pub sampler_assign: Vec<BfresKeyValue>,
    /// Render info name -> values rendered as text.
    pub render_info: Vec<BfresKeyValue>,
    /// Shader parameter values in file order (`elsa_insert_color0`, ...).
    pub shader_params: Vec<BfresShaderParam>,
}

pub fn parse_materials(
    data: &[u8],
    sections: &[BfresSection],
    endian: Endian,
    version_major: u8,
) -> Vec<BfresMaterial> {
    sections
        .iter()
        .filter(|section| &section.signature == b"FMAT")
        .map(|section| {
            let offset = section.offset as usize;
            // Texture-name array pointer and texture count byte per FMAT
            // layout. Version 0.9 (Nintendo Switch Sports) uses the 168-byte
            // material without the version 8 header block: names at 0x30,
            // sampler count at 0x9C and texture count at 0x9D.
            let (names_pointer_offset, count_offset) = match version_major {
                0..=8 => (56, 168),
                9 => (48, 157),
                _ => (32, 163),
            };
            let names = u64_at(data, offset + names_pointer_offset, endian).unwrap_or(0) as usize;
            let count = data.get(offset + count_offset).copied().unwrap_or(0) as usize;
            let samplers = find_sampler_keys(data, offset, count, endian);
            let texture_slots = (0..count)
                .filter_map(|index| {
                    let name = read_string(data, u64_at(data, names + index * 8, endian).ok()?)?;
                    let sampler = samplers.get(index).cloned().unwrap_or_default();
                    let sampler_type = classify_sampler(&sampler);
                    Some(BfresTextureSlot {
                        index,
                        texture_type: if sampler_type == "Texture" {
                            classify_texture_name(&name)
                        } else {
                            sampler_type
                        }
                        .into(),
                        sampler,
                        name,
                    })
                })
                .collect();
            let shading = if version_major == 9 {
                parse_shading_v9(data, offset, endian)
            } else {
                ShadingInfo::default()
            };
            BfresMaterial {
                name: section
                    .name
                    .clone()
                    .unwrap_or_else(|| "Unnamed material".into()),
                offset: section.offset,
                texture_slots,
                shader_archive: shading.shader_archive,
                shading_model: shading.shading_model,
                shader_options: shading.shader_options,
                sampler_assign: shading.sampler_assign,
                render_info: shading.render_info,
                shader_params: shading.shader_params,
            }
        })
        .collect()
}

#[derive(Default)]
struct ShadingInfo {
    shader_archive: String,
    shading_model: String,
    shader_options: Vec<BfresKeyValue>,
    sampler_assign: Vec<BfresKeyValue>,
    render_info: Vec<BfresKeyValue>,
    shader_params: Vec<BfresShaderParam>,
}

/// Shader assignment, render info and shader parameters of a BFRES 0.9
/// material (Nintendo Switch Sports). Offsets are relative to the FMAT:
/// render info array at 0x10 (24-byte records), ShaderAssign at 0x20,
/// shader parameter array at 0x50 (32-byte records), parameter data at 0x60,
/// render info count at 0x9A and shader parameter count at 0x9E.
fn parse_shading_v9(data: &[u8], material: usize, endian: Endian) -> ShadingInfo {
    let pointer = |offset: usize| u64_at(data, material + offset, endian).unwrap_or(0) as usize;
    let mut info = ShadingInfo::default();
    let assign = pointer(0x20);
    if assign != 0 {
        let assign_pointer =
            |offset: usize| u64_at(data, assign + offset, endian).unwrap_or(0) as usize;
        info.shader_archive = read_string(data, assign_pointer(0) as u64).unwrap_or_default();
        info.shading_model = read_string(data, assign_pointer(8) as u64).unwrap_or_default();
        info.sampler_assign =
            string_dictionary(data, assign_pointer(0x20), assign_pointer(0x28), endian);
        info.shader_options =
            string_dictionary(data, assign_pointer(0x30), assign_pointer(0x38), endian);
    }
    let render_info_count = u16_at(data, material + 0x9A, endian).unwrap_or(0) as usize;
    let render_info = pointer(0x10);
    for index in 0..render_info_count.min(256) {
        let record = render_info + index * 24;
        let Some(name) = u64_at(data, record, endian)
            .ok()
            .and_then(|pointer| read_string(data, pointer))
        else {
            break;
        };
        let values = u64_at(data, record + 8, endian).unwrap_or(0) as usize;
        let count = u16_at(data, record + 0x10, endian).unwrap_or(0) as usize;
        let kind = data.get(record + 0x12).copied().unwrap_or(2);
        let rendered: Vec<String> = (0..count.min(64))
            .filter_map(|value_index| match kind {
                0 => u32_at(data, values + value_index * 4, endian)
                    .ok()
                    .map(|value| (value as i32).to_string()),
                1 => u32_at(data, values + value_index * 4, endian)
                    .ok()
                    .map(|value| format!("{}", f32::from_bits(value))),
                _ => u64_at(data, values + value_index * 8, endian)
                    .ok()
                    .and_then(|pointer| read_string(data, pointer)),
            })
            .collect();
        info.render_info.push(BfresKeyValue {
            name,
            value: rendered.join(", "),
        });
    }
    let param_count = u16_at(data, material + 0x9E, endian).unwrap_or(0) as usize;
    let params = pointer(0x50);
    let param_data = pointer(0x60);
    for index in 0..param_count.min(512) {
        let record = params + index * 32;
        let Some(name) = u64_at(data, record + 8, endian)
            .ok()
            .and_then(|pointer| read_string(data, pointer))
        else {
            break;
        };
        let kind = data.get(record + 0x10).copied().unwrap_or(0);
        let size = data.get(record + 0x11).copied().unwrap_or(0) as usize;
        let data_offset = u16_at(data, record + 0x12, endian).unwrap_or(0) as usize;
        let base = param_data + data_offset;
        let values: Vec<f32> = (0..size / 4)
            .filter_map(|word| u32_at(data, base + word * 4, endian).ok())
            .enumerate()
            .map(|(word, raw)| match kind {
                0..=11 => raw as i32 as f32,
                28 | 29 if word == 0 => raw as f32,
                _ => f32::from_bits(raw),
            })
            .collect();
        info.shader_params.push(BfresShaderParam {
            name,
            kind: shader_param_kind(kind).into(),
            values,
        });
    }
    info
}

fn shader_param_kind(kind: u8) -> &'static str {
    match kind {
        0 => "bool",
        1 => "bool2",
        2 => "bool3",
        3 => "bool4",
        4 => "int",
        5 => "int2",
        6 => "int3",
        7 => "int4",
        8 => "uint",
        9 => "uint2",
        10 => "uint3",
        11 => "uint4",
        12 => "float",
        13 => "float2",
        14 => "float3",
        15 => "float4",
        17 => "float2x2",
        18 => "float2x3",
        19 => "float2x4",
        20 => "float3x2",
        21 => "float3x3",
        22 => "float3x4",
        23 => "float4x2",
        24 => "float4x3",
        25 => "float4x4",
        26 => "srt2d",
        27 => "srt3d",
        28 => "texsrt",
        29 => "texsrtex",
        _ => "unknown",
    }
}

/// Reads a ResDict whose entries pair with an array of string pointers
/// (shader option and sampler assignment tables).
fn string_dictionary(
    data: &[u8],
    values: usize,
    dictionary: usize,
    endian: Endian,
) -> Vec<BfresKeyValue> {
    if values == 0 || dictionary == 0 {
        return Vec::new();
    }
    let count = u32_at(data, dictionary + 4, endian).unwrap_or(0) as usize;
    parse_res_dict_keys(data, dictionary, count.min(1024), endian)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, name)| BfresKeyValue {
            value: u64_at(data, values + index * 8, endian)
                .ok()
                .and_then(|pointer| read_string(data, pointer))
                .unwrap_or_default(),
            name,
        })
        .collect()
}

fn find_sampler_keys(data: &[u8], material: usize, count: usize, endian: Endian) -> Vec<String> {
    if count == 0 {
        return Vec::new();
    }
    (0x20..0xa0)
        .step_by(8)
        .filter_map(|relative| u64_at(data, material + relative, endian).ok())
        .filter_map(|pointer| parse_res_dict_keys(data, pointer as usize, count, endian))
        .find(|keys| {
            keys.iter()
                .all(|key| key.starts_with('_') || key.to_ascii_lowercase().contains("sampler"))
        })
        .unwrap_or_default()
}

fn parse_res_dict_keys(
    data: &[u8],
    offset: usize,
    expected: usize,
    endian: Endian,
) -> Option<Vec<String>> {
    if u32_at(data, offset + 4, endian).ok()? as usize != expected {
        return None;
    }
    (0..expected)
        .map(|index| {
            let node = offset.checked_add(0x18 + index.checked_mul(0x10)?)?;
            read_string(data, u64_at(data, node + 8, endian).ok()?)
        })
        .collect()
}

fn classify_sampler(sampler: &str) -> &'static str {
    match sampler.to_ascii_lowercase().as_str() {
        "_a0" | "_albedo0" | "albedo0" => "Base color",
        "_n0" | "_normal0" | "normal0" => "Normal",
        "_e0" | "_emission0" | "_emissive0" | "emissive0" | "emissive1" => "Emission",
        "_s0" | "_specular0" | "specular0" => "Specular",
        "_r0" | "_roughness0" | "_smoothness0" | "roughness0" | "smoothness0" => "Roughness",
        "_m0" | "_metallic0" | "_metalness0" | "metalness0" => "Metalness",
        "_ao" | "_ao0" | "ao" | "ao0" | "_ambientocclusion0" | "ambientocclusion0" => {
            "Ambient occlusion"
        }
        "_b0" | "_b1" => "Bake",
        // Nintendo Switch Sports "insert color" mask: RGB channels select
        // which runtime tint (skin, hair, outfit) fills the near-white albedo.
        "_tcl0" | "_tcl" => "Tint mask",
        _ => "Texture",
    }
}

fn classify_texture_name(name: &str) -> &'static str {
    let name = name.to_ascii_lowercase();
    if name.contains("_alb") || name.contains("albedo") {
        "Base color"
    } else if name.contains("_nrm") || name.contains("normal") {
        "Normal"
    } else if name.contains("_emm") || name.contains("emission") || name.contains("emissive") {
        "Emission"
    } else if name.contains("_rgh") || name.contains("roughness") {
        "Roughness"
    } else if name.contains("_mtl") || name.contains("metalness") || name.contains("metallic") {
        "Metalness"
    } else if name.contains("_spc") || name.contains("specular") {
        "Specular"
    } else if name.contains("_tcl") {
        "Tint mask"
    } else if name.contains("_msk") || name.contains("_alp") || name.contains("mask") {
        "Mask"
    } else {
        "Texture"
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_sampler, classify_texture_name};
    use crate::file_format::Model3D::bfres::BfresFile;

    #[test]
    fn ambient_occlusion_samplers_are_not_diffuse() {
        for sampler in ["_ao", "_ao0", "ao", "ao0", "_ambientocclusion0"] {
            assert_eq!(classify_sampler(sampler), "Ambient occlusion");
        }
    }

    #[test]
    fn classifies_tomodachi_dummy_texture_names_without_samplers() {
        assert_eq!(classify_texture_name("Dummy_Alb"), "Base color");
        assert_eq!(classify_texture_name("Dummy_Nrm"), "Normal");
        assert_eq!(classify_texture_name("Dummy_Rgh"), "Roughness");
        assert_eq!(classify_texture_name("Dummy_Msk"), "Mask");
        assert_eq!(classify_texture_name("Dummy_Emm"), "Emission");
    }

    #[test]
    fn resolves_texture_semantics_from_sampler_dictionary() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/bfres/Animal_Bull.Bull.bfres");
        let file = BfresFile::from_bytes(&std::fs::read(path).unwrap()).unwrap();
        let slots: Vec<_> = file
            .materials
            .iter()
            .flat_map(|material| &material.texture_slots)
            .collect();
        assert!(slots
            .iter()
            .any(|slot| slot.sampler == "_a0" && slot.texture_type == "Base color"));
        assert!(slots
            .iter()
            .any(|slot| slot.sampler == "_n0" && slot.texture_type == "Normal"));
        assert!(slots
            .iter()
            .any(|slot| slot.sampler == "_s0" && slot.texture_type == "Specular"));
    }
}
