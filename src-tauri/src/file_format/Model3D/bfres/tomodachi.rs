use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use crate::Settings::Pathlib;

#[derive(Debug, Deserialize)]
struct ModelTextures {
    #[serde(default)]
    textures: HashMap<String, Vec<String>>,
}

static MODELS: LazyLock<HashMap<String, ModelTextures>> = LazyLock::new(|| {
    serde_json::from_str(&crate::utils::LookupData::read_support_json(
        "tomodachi_bfres.json",
    ))
    .unwrap_or_default()
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextureSet {
    pub name: String,
    pub variants: Vec<u8>,
}

pub struct ResolvedTexture {
    pub name: String,
    pub path: PathBuf,
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

fn model_name(source: &Path) -> Option<String> {
    let path = Pathlib::new(source);
    (!path.stem.is_empty()).then_some(path.stem)
}

fn texture_roots(source: &Path, fallback: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(root) = source
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("Tex"))
    {
        roots.push(root);
    }
    if let Some(root) = fallback
        .filter(|root| !root.as_os_str().is_empty())
        .map(|root| root.join("Tex"))
    {
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

fn texture_path(roots: &[PathBuf], stem: &str) -> Option<PathBuf> {
    roots.iter().find_map(|root| {
        [format!("{stem}.bntx.zs"), format!("{stem}.bntx")]
            .into_iter()
            .map(|name| root.join(name))
            .find(|path| path.is_file())
    })
}

fn variant_stem(stem: &str, variant: u8) -> String {
    stem.strip_suffix(".00")
        .map(|base| format!("{base}.{variant:02}"))
        .unwrap_or_else(|| stem.to_owned())
}

fn texture_variants(roots: &[PathBuf], textures: &[String]) -> Vec<u8> {
    let mut variants = Vec::new();
    for variant in 0..=64 {
        if textures.iter().any(|stem| {
            stem.to_ascii_lowercase().contains("alb")
                && texture_path(roots, &variant_stem(stem, variant)).is_some()
        }) {
            variants.push(variant);
        }
    }
    variants
}

fn model_sets(
    source: &Path,
    fallback: Option<&Path>,
) -> Option<(&'static ModelTextures, Vec<PathBuf>)> {
    let model_name = model_name(source)?;
    let model = MODELS.get(&model_name)?;
    Some((model, texture_roots(source, fallback)))
}

pub fn available_texture_sets(source: &Path, fallback: Option<&Path>) -> Vec<TextureSet> {
    let Some((model, roots)) = model_sets(source, fallback) else {
        return Vec::new();
    };
    let mut sets: Vec<_> = model
        .textures
        .iter()
        .filter_map(|(name, textures)| {
            let variants = texture_variants(&roots, textures);
            (!variants.is_empty()).then(|| TextureSet {
                name: name.clone(),
                variants,
            })
        })
        .collect();
    sets.sort_by(|left, right| left.name.cmp(&right.name));
    sets
}

pub fn resolve_texture_set(
    source: &Path,
    set_name: &str,
    variant: u8,
    fallback: Option<&Path>,
    zstd: Option<&crate::Zstd::TotkZstd<'_>>,
) -> Vec<ResolvedTexture> {
    let Some((model, roots)) = model_sets(source, fallback) else {
        return Vec::new();
    };
    let Some(textures) = model.textures.get(set_name) else {
        return Vec::new();
    };
    if !texture_variants(&roots, textures).contains(&variant) {
        return Vec::new();
    }
    textures
        .iter()
        .filter_map(|stem| {
            let selected_stem = if stem.to_ascii_lowercase().contains("alb") {
                variant_stem(stem, variant)
            } else {
                stem.clone()
            };
            let path = texture_path(&roots, &selected_stem)?;
            let rendered =
                crate::file_format::Image::ImageDocument::render_path_selection_with_zstd(
                    &path, 0, 0, 0, zstd,
                )
                .ok()?;
            let name = rendered
                .entries
                .get(rendered.selected_index)
                .map(|entry| entry.name.clone())
                .unwrap_or_else(|| stem.clone());
            Some(ResolvedTexture {
                name,
                path,
                data_url: rendered.data_url,
                width: rendered.width,
                height: rendered.height,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_format::Model3D::bfres::BfresFile;

    #[test]
    fn configured_texture_root_is_a_fallback_after_the_adjacent_root() {
        let roots = texture_roots(
            Path::new("C:/game/Model/ClothBottomsBloomers.bfres.zs"),
            Some(Path::new("D:/Tomodachi")),
        );
        assert_eq!(
            roots,
            [
                PathBuf::from("C:/game/Tex"),
                PathBuf::from("D:/Tomodachi/Tex")
            ]
        );
    }

    #[test]
    fn tomodachi_bfres_corpus_parses_and_has_lookup_entries() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/tomo");
        if !root.is_dir() {
            return;
        }
        let mut tested = 0;
        for path in std::fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("Cloth") && name.contains(".bfres"))
            })
        {
            let name = model_name(&path).unwrap();
            assert!(
                MODELS.contains_key(&name),
                "missing lookup entry for {name}"
            );
            BfresFile::from_path(&path)
                .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
            tested += 1;
        }
        assert!(tested > 0, "Tomodachi BFRES corpus is empty");
    }

    #[test]
    fn train_emission_slot_is_classified_without_sampler_metadata() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/tomo/ClothAllTrainLong.bfres.zs");
        if !path.is_file() {
            return;
        }
        let bfres = BfresFile::from_path(path).unwrap();
        assert!(bfres.materials.iter().any(|material| material
            .texture_slots
            .iter()
            .any(|slot| slot.name == "Dummy_Emm" && slot.texture_type == "Emission")));
    }
}
