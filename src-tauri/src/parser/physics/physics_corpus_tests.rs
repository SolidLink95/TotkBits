use super::{
    bphcl::BphclDocument,
    bphcl_to_hkcl::{analyze_bphcl_to_hkcl, convert_bphcl_cloth_to_hkcl_template},
    hkcl::HkclDocument,
    hkcl_to_bphcl::{analyze_hkcl_to_bphcl, convert_hkcl_cloth_to_bphcl_template},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn files(directory: &Path, extension: &str, limit: usize) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|value| value == extension))
        .collect();
    paths.sort();
    paths.truncate(limit);
    assert!(
        !paths.is_empty(),
        "{} corpus is empty",
        extension.to_uppercase()
    );
    paths
}

#[test]
fn cross_format_corpus_preflights_are_deterministic_and_gate_conversion() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp");
    let hkcl: Vec<_> = files(&root.join("hkcl"), "hkcl", usize::MAX)
        .into_iter()
        .map(|path| {
            HkclDocument::parse(&fs::read(path).unwrap())
                .unwrap()
                .neutral_physics_graph()
        })
        .collect();
    let bphcl: Vec<_> = files(&root.join("_bphcl"), "bphcl", 16)
        .into_iter()
        .map(|path| {
            BphclDocument::parse(&fs::read(path).unwrap())
                .unwrap()
                .neutral_physics_graph()
        })
        .collect();

    for source in &hkcl {
        if source.cloths.is_empty() {
            continue;
        }
        for target in &bphcl {
            if target.cloths.is_empty() {
                continue;
            }
            let first = analyze_hkcl_to_bphcl(source, target, 0, 0);
            assert_eq!(first, analyze_hkcl_to_bphcl(source, target, 0, 0));
            assert_eq!(
                convert_hkcl_cloth_to_bphcl_template(source, target, 0, 0).is_ok(),
                first.is_compatible()
            );
        }
    }
    for source in &bphcl {
        if source.cloths.is_empty() {
            continue;
        }
        for target in &hkcl {
            if target.cloths.is_empty() {
                continue;
            }
            let first = analyze_bphcl_to_hkcl(source, target, 0, 0);
            assert_eq!(first, analyze_bphcl_to_hkcl(source, target, 0, 0));
            assert_eq!(
                convert_bphcl_cloth_to_hkcl_template(source, target, 0, 0).is_ok(),
                first.is_compatible()
            );
        }
    }
}
