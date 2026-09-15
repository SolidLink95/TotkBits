use super::BphclDocument;
use crate::parser::physics::{
    hkcl_to_bphcl::analyze_hkcl_to_bphcl,
    physics_graph::{FormatNeutralPhysicsGraph, PhysicsFormat},
};
use roead::{
    sarc::{Sarc, SarcWriter},
    Endian,
};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn corpus() -> Vec<(String, BphclDocument)> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/_bphcl");
    let mut paths: Vec<_> = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
        .map(|entry| entry.expect("failed to read corpus entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "bphcl")
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "BPHCL corpus is empty");
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .expect("corpus file has no name")
                .to_string_lossy()
                .into_owned();
            let bytes = fs::read(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            let document = BphclDocument::parse(&bytes)
                .unwrap_or_else(|error| panic!("failed to parse {name}: {error}"));
            document
                .validate_item_graph()
                .unwrap_or_else(|error| panic!("invalid ITEM/PTCH graph in {name}: {error}"));
            (name, document)
        })
        .collect()
}

fn validate_merged(label: &str, bytes: &[u8]) -> BphclDocument {
    let document = BphclDocument::parse(bytes)
        .unwrap_or_else(|error| panic!("failed to reparse {label}: {error}"));
    document
        .validate_item_graph()
        .unwrap_or_else(|error| panic!("invalid merged ITEM/PTCH graph in {label}: {error}"));
    document
}

#[test]
fn corpus_projects_to_format_neutral_graphs() {
    for (name, document) in corpus() {
        let graph = document.neutral_physics_graph();
        assert_eq!(graph.cloths.len(), document.cloth.len(), "{name}");
        assert_eq!(graph.skeletons.len(), document.skeletons.len(), "{name}");
        assert_eq!(
            graph.collidables.len(),
            document.collidables.len(),
            "{name}"
        );
        assert!(graph
            .cloths
            .iter()
            .all(|cloth| cloth.simulations.iter().all(|simulation| simulation
                .particles
                .iter()
                .all(|particle| particle.position.is_some()))));
    }
}

#[test]
fn corpus_projects_native_constraint_links() {
    let mut populated = 0;
    for (_, document) in corpus() {
        populated += document
            .neutral_physics_graph()
            .constraints
            .iter()
            .filter(|constraint| !constraint.elements.is_empty())
            .count();
    }
    assert!(
        populated > 0,
        "BPHCL TYPE-driven constraint parser found no native links"
    );
}

#[test]
fn hkcl_template_import_builds_a_native_savable_bphcl() {
    let (source_name, document, mut source, template_index) = corpus()
        .into_iter()
        .find_map(|(name, document)| {
            let target = document.neutral_physics_graph();
            (0..target.cloths.len()).find_map(|template_index| {
                let mut source = mock_hkcl_source(&target, template_index);
                let has_constraint_values = source.cloths[template_index]
                    .simulations
                    .iter()
                    .flat_map(|simulation| &simulation.constraints)
                    .filter_map(|id| {
                        source
                            .constraints
                            .iter()
                            .find(|constraint| &constraint.id == id)
                    })
                    .any(|constraint| {
                        constraint
                            .elements
                            .iter()
                            .any(|element| !element.values.is_empty())
                    });
                if !has_constraint_values {
                    return None;
                }
                if source.cloths[template_index]
                    .simulations
                    .iter()
                    .all(|simulation| simulation.collidables.is_empty())
                {
                    return None;
                }
                let cloth = &mut source.cloths[template_index];
                cloth.name = Some(format!(
                    "{}_HKCL",
                    cloth.name.as_deref().unwrap_or("Imported")
                ));
                analyze_hkcl_to_bphcl(&source, &target, template_index, template_index)
                    .is_compatible()
                    .then_some((name.clone(), document.clone(), source, template_index))
            })
        })
        .expect("BPHCL corpus has no HKCL-compatible template");
    let imported_name = source.cloths[template_index].name.clone().unwrap();
    let particle = source.cloths[template_index]
        .simulations
        .first_mut()
        .and_then(|simulation| simulation.particles.first_mut())
        .expect("compatible template has no particles");
    particle.position = Some([9.0, 8.0, 7.0, 1.0]);
    particle.fixed = !particle.fixed;
    particle.mass = 2.0;
    particle.inverse_mass = 0.5;
    particle.radius = 0.25;
    particle.friction = 0.75;
    let expected_fixed = particle.fixed;
    let constraint_id = source.cloths[template_index]
        .simulations
        .iter()
        .flat_map(|simulation| &simulation.constraints)
        .find(|id| {
            source
                .constraints
                .iter()
                .find(|constraint| &constraint.id == *id)
                .is_some_and(|constraint| {
                    constraint
                        .elements
                        .iter()
                        .any(|element| !element.values.is_empty())
                })
        })
        .cloned()
        .unwrap();
    let source_constraint = source
        .constraints
        .iter_mut()
        .find(|constraint| constraint.id == constraint_id)
        .unwrap();
    source_constraint.elements[0].values[0] += 1.0;
    let expected_constraint_value = source_constraint.elements[0].values[0];
    let expected_collidable_name = source.cloths[template_index]
        .simulations
        .iter()
        .flat_map(|simulation| &simulation.collidables)
        .find_map(|id| {
            source
                .collidables
                .iter()
                .find(|collidable| &collidable.id == id)
                .and_then(|collidable| collidable.name.clone())
        })
        .unwrap();

    let bytes = document
        .import_hkcl_cloth(&source, template_index, template_index)
        .unwrap_or_else(|error| panic!("failed HKCL import through {source_name}: {error}"));
    let rebuilt = validate_merged("HKCL to BPHCL native import", &bytes);
    let imported = rebuilt
        .cloth
        .iter()
        .find(|cloth| cloth.name == imported_name)
        .expect("native BPHCL lacks imported HKCL cloth");
    let imported_particle = &imported.simulations[0].particles[0];

    assert_eq!(rebuilt.cloth.len(), document.cloth.len() + 1);
    assert_eq!(
        [
            imported_particle.position.x,
            imported_particle.position.y,
            imported_particle.position.z,
            imported_particle.position.w,
        ],
        [9.0, 8.0, 7.0, 1.0]
    );
    assert_eq!(imported_particle.fixed, expected_fixed);
    assert_eq!(imported_particle.mass, 2.0);
    assert_eq!(imported_particle.inverse_mass, 0.5);
    assert_eq!(imported_particle.radius, 0.25);
    assert_eq!(imported_particle.friction, 0.75);
    let graph = rebuilt.neutral_physics_graph();
    let imported_graph_cloth = graph
        .cloths
        .iter()
        .find(|cloth| cloth.name.as_deref() == Some(imported_name.as_str()))
        .unwrap();
    let imported_constraint = imported_graph_cloth
        .simulations
        .iter()
        .flat_map(|simulation| &simulation.constraints)
        .find_map(|id| {
            graph
                .constraints
                .iter()
                .find(|constraint| &constraint.id == id)
                .filter(|constraint| {
                    constraint
                        .elements
                        .iter()
                        .any(|element| !element.values.is_empty())
                })
        })
        .unwrap();
    assert_eq!(
        imported_constraint.elements[0].values[0],
        expected_constraint_value
    );
    assert!(
        rebuilt
            .collidables
            .iter()
            .any(|collidable| collidable.name == expected_collidable_name),
        "native BPHCL lacks imported HKCL collidable"
    );
}

fn mock_hkcl_source(
    target: &FormatNeutralPhysicsGraph,
    cloth_index: usize,
) -> FormatNeutralPhysicsGraph {
    let mut source = target.clone();
    source.source_format = PhysicsFormat::Hkcl;
    let referenced: std::collections::BTreeSet<_> = source.cloths[cloth_index]
        .simulations
        .iter()
        .flat_map(|simulation| simulation.collidables.iter().cloned())
        .collect();
    for skeleton in &mut source.skeletons {
        for bone in &mut skeleton.bones {
            if let Some(name) = &mut bone.name {
                *name = name.strip_prefix("Link:").unwrap_or(name).to_owned();
            }
        }
    }
    for collidable in &mut source.collidables {
        if !referenced.contains(&collidable.id) {
            continue;
        }
        let axes = collidable.axes.unwrap();
        let translation = collidable.translation.unwrap();
        collidable.transform = Some([
            axes[0][0],
            axes[0][1],
            axes[0][2],
            axes[0][3],
            axes[1][0],
            axes[1][1],
            axes[1][2],
            axes[1][3],
            axes[2][0],
            axes[2][1],
            axes[2][2],
            axes[2][3],
            translation[0],
            translation[1],
            translation[2],
            translation[3],
        ]);
        collidable.name = Some(format!(
            "{}_HKCL",
            collidable.name.as_deref().unwrap_or("Collidable")
        ));
    }
    source
}

#[test]
fn cloth_and_collidable_removals_reparse_across_corpus_samples() {
    let documents = corpus();
    let (cloth_name, cloth_document) = documents
        .iter()
        .find(|(_, document)| document.cloth.len() > 1 && !document.collidables.is_empty())
        .expect("corpus has no BPHCL with removable cloth");
    let cloth_bytes = cloth_document
        .remove_cloth(1)
        .unwrap_or_else(|error| panic!("failed to remove cloth from {cloth_name}: {error}"));
    let cloth_result = validate_merged("cloth removal", &cloth_bytes);
    assert_eq!(cloth_result.cloth.len(), cloth_document.cloth.len() - 1);

    let (collidable_name, collidable_document) = documents
        .iter()
        .find(|(_, document)| document.collidables.len() > 1 && !document.cloth.is_empty())
        .expect("corpus has no BPHCL with removable collidable");
    let collidable_bytes = collidable_document
        .remove_collidable(1)
        .unwrap_or_else(|error| {
            panic!("failed to remove collidable from {collidable_name}: {error}")
        });
    let collidable_result = validate_merged("collidable removal", &collidable_bytes);
    assert_eq!(
        collidable_result.collidables.len(),
        collidable_document.collidables.len() - 1
    );
}

#[test]
#[ignore = "full tmp/_bphcl corpus matrix takes about 90 seconds"]
fn corpus_merges_reparse_with_valid_pointers_and_survive_sarc_roundtrips() {
    let documents = corpus();
    let mut cloth_successes = 0;
    let mut collidable_successes = 0;
    let mut rejections = BTreeMap::<String, usize>::new();
    let mut archive_sample = None;

    for source_index in 0..documents.len() {
        let target_index = (source_index + 1) % documents.len();
        let (source_name, source) = &documents[source_index];
        let (target_name, target) = &documents[target_index];
        if !source.cloth.is_empty() {
            let label = format!("cloth {source_name} -> {target_name}");
            match target.merge_complete_cloth(source, 0) {
                Ok(bytes) => {
                    validate_merged(&label, &bytes);
                    cloth_successes += 1;
                    archive_sample.get_or_insert(bytes);
                }
                Err(error) => {
                    *rejections
                        .entry(rejection_category(&error.to_string()))
                        .or_default() += 1
                }
            }
        }
        if !source.collidables.is_empty() {
            let label = format!("collidable {source_name} -> {target_name}");
            match target.merge_collidable(source, 0) {
                Ok(bytes) => {
                    validate_merged(&label, &bytes);
                    collidable_successes += 1;
                    archive_sample.get_or_insert(bytes);
                }
                Err(error) => {
                    *rejections
                        .entry(rejection_category(&error.to_string()))
                        .or_default() += 1
                }
            }
        }
    }

    assert!(
        cloth_successes > 0,
        "no complete-cloth corpus merge succeeded"
    );
    assert!(
        collidable_successes > 0,
        "no standalone-collidable corpus merge succeeded"
    );
    let merged = archive_sample.expect("no merged archive sample was produced");

    let mut root = SarcWriter::new(Endian::Little);
    root.add_file("Physics/merged.bphcl", merged.clone());
    let reopened_root = Sarc::new(root.to_binary()).expect("failed to reopen root SARC");
    validate_merged(
        "root SARC roundtrip",
        reopened_root
            .get_data("Physics/merged.bphcl")
            .expect("merged BPHCL missing after root SARC reopen"),
    );

    let mut inner = SarcWriter::new(Endian::Little);
    inner.add_file("Physics/merged.bphcl", merged);
    let mut outer = SarcWriter::new(Endian::Little);
    outer.add_file("Nested/physics.pack", inner.to_binary());
    let reopened_outer = Sarc::new(outer.to_binary()).expect("failed to reopen outer SARC");
    let inner_bytes = reopened_outer
        .get_data("Nested/physics.pack")
        .expect("nested SARC missing after outer reopen");
    let reopened_inner = Sarc::new(inner_bytes.to_vec()).expect("failed to reopen nested SARC");
    validate_merged(
        "nested SARC roundtrip",
        reopened_inner
            .get_data("Physics/merged.bphcl")
            .expect("merged BPHCL missing after nested SARC reopen"),
    );

    let rejected: usize = rejections.values().sum();
    eprintln!("validated {} corpus files; {cloth_successes} cloth merges and {collidable_successes} collidable merges succeeded; {rejected} incompatible pairs were rejected: {rejections:?}", documents.len());
}

fn rejection_category(message: &str) -> String {
    if message.contains("already has a different collidable named") {
        "different collidable with the same name".into()
    } else if message.contains("missing TYPE dependency") {
        "missing TYPE dependency".into()
    } else if message.contains("conflicting TYPE definition") {
        "conflicting TYPE definition".into()
    } else if message.contains("no paired skeleton") {
        "cloth has no paired skeleton".into()
    } else {
        message.into()
    }
}

/// Removing a cloth must take the colliders only it used along with it, so
/// the rebuilt collider array never holds an entry no simulation references.
#[test]
fn cloth_removal_prunes_orphaned_colliders_across_corpus() {
    let mut pruned_total = 0;
    let mut samples = 0;
    for (name, document) in corpus()
        .iter()
        .filter(|(_, document)| document.cloth.len() > 1 && !document.collidables.is_empty())
    {
        let bytes = document
            .remove_cloth(1)
            .unwrap_or_else(|error| panic!("failed to remove cloth from {name}: {error}"));
        let result = validate_merged("cloth removal", &bytes);
        let referenced = result.referenced_collidable_items();
        for collider in &result.collidables {
            assert!(
                referenced.contains(&collider.item_index),
                "{name}: collider '{}' survived without any simulation referencing it",
                collider.name
            );
        }
        pruned_total += document.collidables.len() - result.collidables.len();
        samples += 1;
    }
    eprintln!("cloth removal pruned {pruned_total} colliders across {samples} corpus samples");
}

/// Converts every BOTW cloth of `tmp/hkcl` with the bundled TOTK type table;
/// the result must reparse and validate, and for the actors Nintendo ported
/// (a same-named file in `tmp/_bphcl`) the cloth and operator layout of every
/// cloth that kept its name must match the official port.
#[test]
fn hkcl_corpus_converts_like_nintendo_ports() {
    use crate::parser::physics::hkcl::HkclDocument;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let type_section = fs::read(root.join("misc/bphcl_types.bin")).expect("misc/bphcl_types.bin");
    let types = super::type_donor(&type_section).expect("type donor");
    let link_bones = fs::read_to_string(root.join("misc/link_bones.txt"))
        .expect("misc/link_bones.txt")
        .lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect();
    let options = super::ConvertOptions {
        link_bones,
        ..Default::default()
    };
    let directory = root.join("../tmp/hkcl");
    let mut paths: Vec<_> = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
        .map(|entry| entry.expect("corpus entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "hkcl")
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "HKCL corpus is empty");
    let mut converted = 0;
    let mut compared = 0;
    let mut failures = Vec::new();
    for path in paths {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let hkcl = match HkclDocument::parse(&fs::read(&path).unwrap()) {
            Ok(document) => document,
            Err(error) => {
                failures.push(format!("{stem}: hkcl parse: {error}"));
                continue;
            }
        };
        let (bytes, report) =
            match super::convert_hkcl_to_bphcl(&hkcl, &types, &type_section, &options) {
                Ok(result) => result,
                Err(error) => {
                    failures.push(format!("{stem}: convert: {error}"));
                    continue;
                }
            };
        let ours = BphclDocument::parse(&bytes).expect("converted BPHCL reparses");
        if let Err(error) = ours.validate() {
            failures.push(format!("{stem}: validate: {error}"));
            continue;
        }
        assert_eq!(ours.cloth.len(), report.cloths.len(), "{stem}: cloth count");
        assert_eq!(
            ours.collidables.len(),
            report.collidables.len(),
            "{stem}: collidable count"
        );
        converted += 1;
        let vanilla_path = root.join("../tmp/_bphcl").join(format!("{stem}.bphcl"));
        let Ok(vanilla_bytes) = fs::read(&vanilla_path) else {
            continue;
        };
        let vanilla = BphclDocument::parse(&vanilla_bytes).expect("vanilla BPHCL parses");
        for cloth in &ours.cloth {
            let Some(reference) = vanilla.cloth.iter().find(|c| c.name == cloth.name) else {
                continue;
            };
            let classes = |document: &BphclDocument, cloth: &super::Cloth| -> Vec<String> {
                let reflect = super::Reflect::new(document).expect("DATA section");
                let cloth_type = reflect
                    .find_type("hclClothData")
                    .expect("hclClothData type");
                let field = reflect
                    .member_offset(cloth_type, "operators")
                    .expect("operators");
                let base = reflect
                    .item(cloth.item_index)
                    .expect("cloth item")
                    .data_offset;
                reflect
                    .pointer_array(base + field)
                    .into_iter()
                    .map(|index| {
                        reflect.type_name(reflect.item(index).expect("operator item").type_index)
                    })
                    // Nintendo re-exported a few cloths with the gather operator
                    // in place of the equivalent copy operator; both are vertex
                    // transfers between buffers.
                    .map(|name| {
                        name.replace("hclGatherAllVerticesOperator", "hclCopyVerticesOperator")
                    })
                    .collect()
            };
            if classes(&ours, cloth) != classes(&vanilla, reference) {
                failures.push(format!(
                    "{stem}/{}: operator classes {:?} vs Nintendo {:?}",
                    cloth.name,
                    classes(&ours, cloth),
                    classes(&vanilla, reference)
                ));
            }
            if cloth.simulations.len() != reference.simulations.len() {
                failures.push(format!("{stem}/{}: sim cloth count differs", cloth.name));
            }
            compared += 1;
        }
    }
    eprintln!("converted {converted} HKCL files, compared {compared} cloths with Nintendo ports");
    assert!(converted > 0, "no HKCL file converted");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
