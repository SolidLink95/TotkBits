use super::{read_string, u32_at, u64_at, BfresSection, Endian};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BfresTextureSlot {
    pub index: usize,
    pub name: String,
    pub sampler: String,
    pub texture_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BfresMaterial {
    pub name: String,
    pub offset: u64,
    pub texture_slots: Vec<BfresTextureSlot>,
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
            let (names_pointer_offset, count_offset) = match version_major {
                0..=8 => (56, 168),
                9 => (48, 179),
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
            BfresMaterial {
                name: section
                    .name
                    .clone()
                    .unwrap_or_else(|| "Unnamed material".into()),
                offset: section.offset,
                texture_slots,
            }
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
