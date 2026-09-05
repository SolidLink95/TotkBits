//! Corpus checks for the helper-bone and support-bone sidecars. They read
//! `tmp/bphhb` (TotK) and `tmp/bphyssb` (BotW) and skip when either is absent.

use super::{
    aamp_tree::{crc32, AampTree},
    bphhb::{self, HelperBoneDocument},
    bphyssb::{self, SupportBoneDocument},
};
use std::{fs, path::PathBuf};

fn corpus(folder: &str, extension: &str) -> Vec<(String, Vec<u8>)> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../tmp/{folder}"));
    let Ok(entries) = fs::read_dir(&directory) else {
        eprintln!("skipping: {} is missing", directory.display());
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|value| value == extension))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(&path).unwrap(),
            )
        })
        .collect()
}

fn helper_corpus() -> Vec<(String, HelperBoneDocument)> {
    corpus("bphhb", "bphhb")
        .into_iter()
        .map(|(name, bytes)| {
            let document =
                HelperBoneDocument::parse(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
            (name, document)
        })
        .collect()
}

fn support_corpus() -> Vec<(String, SupportBoneDocument)> {
    corpus("bphyssb", "bphyssb")
        .into_iter()
        .map(|(name, bytes)| {
            let document = SupportBoneDocument::parse(&bytes)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            (name, document)
        })
        .collect()
}

fn header_int(tree: &AampTree, hash: u32) -> i32 {
    tree.root.objects[0].int(hash)
}

#[test]
fn helper_bone_corpus_round_trips_and_exposes_driver_groups() {
    for (name, document) in helper_corpus() {
        let graph = document
            .graph
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: helper graph did not decode"));
        assert!(!graph.bones.is_empty(), "{name}: no bones");
        let (lists, objects, parameters) = document.tree.counts();
        assert_eq!(lists, document.header.list_count, "{name}: list count");
        assert_eq!(
            objects, document.header.object_count,
            "{name}: object count"
        );
        assert_eq!(
            parameters, document.header.parameter_count,
            "{name}: parameter count"
        );

        let rewritten = document.tree.to_bytes().unwrap();
        let reparsed = HelperBoneDocument::parse(&rewritten)
            .unwrap_or_else(|error| panic!("{name}: rewritten file failed to parse: {error}"));
        assert_eq!(
            reparsed.tree, document.tree,
            "{name}: tree changed across a rewrite"
        );
        assert_eq!(reparsed.graph, document.graph);

        let groups = document.driver_groups().unwrap();
        assert!(!groups.is_empty(), "{name}: no driver groups");
        for group in &groups {
            assert!(!group.driver_indices.is_empty());
            for bone in &group.required_bone_indices {
                assert!(*bone < graph.bones.len(), "{name}: group bone out of range");
            }
            assert_eq!(
                group.required_bone_names.len(),
                group.required_bone_indices.len()
            );
        }
        let covered: usize = groups.iter().map(|group| group.driver_indices.len()).sum();
        assert_eq!(
            covered,
            graph.drivers.len(),
            "{name}: every driver belongs to one group"
        );
    }
}

#[test]
fn support_bone_corpus_round_trips_and_exposes_driver_groups() {
    for (name, document) in support_corpus() {
        let summary = document
            .validate()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(summary.bone_count > 0, "{name}: no bones");
        let graph = document
            .graph
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: support graph did not decode"));
        assert_eq!(graph.bones.len(), summary.bone_count);
        let (lists, objects, parameters) = document.tree.counts();
        assert_eq!(lists, document.header.list_count, "{name}: list count");
        assert_eq!(
            objects, document.header.object_count,
            "{name}: object count"
        );
        assert_eq!(
            parameters, document.header.parameter_count,
            "{name}: parameter count"
        );

        let rewritten = document.tree.to_bytes().unwrap();
        let reparsed = SupportBoneDocument::parse(&rewritten)
            .unwrap_or_else(|error| panic!("{name}: rewritten file failed to parse: {error}"));
        assert_eq!(
            reparsed.tree, document.tree,
            "{name}: tree changed across a rewrite"
        );

        let groups = document.driver_groups().unwrap();
        assert!(!groups.is_empty(), "{name}: no driver groups");
        let covered: usize = groups.iter().map(|group| group.driver_indices.len()).sum();
        assert_eq!(
            covered,
            graph.main_bones.len(),
            "{name}: every driver belongs to one group"
        );
        for group in &groups {
            for bone in &group.required_bone_indices {
                assert!(*bone < graph.bones.len(), "{name}: group bone out of range");
            }
        }
    }
}

#[test]
fn helper_bone_driver_groups_merge_between_corpus_files() {
    let documents = helper_corpus();
    if documents.len() < 2 {
        return;
    }
    let mut merged_count = 0;
    for window in documents.windows(2).take(8) {
        let (target_name, target) = &window[0];
        let (source_name, source) = &window[1];
        let source_groups = source.driver_groups().unwrap();
        let target_groups = target.driver_groups().unwrap();
        let merged = target
            .merge_driver_group(source, 0)
            .unwrap_or_else(|error| panic!("{source_name} into {target_name}: {error}"));
        let graph = merged.graph.as_ref().expect("merged graph decodes");
        let bone_list = crc32("bone_list");
        let bones = merged.tree.root.children[0]
            .find_child(bone_list)
            .unwrap()
            .objects
            .len();
        assert_eq!(bones, graph.bones.len());
        assert_eq!(header_int(&merged.tree, crc32("bone_num")), bones as i32);
        assert_eq!(
            header_int(&merged.tree, crc32("driver_bone_num")),
            graph.drivers.len() as i32
        );
        assert_eq!(
            header_int(&merged.tree, crc32("pose_driven_num")),
            graph.pose_driven.len() as i32
        );
        assert_eq!(graph.driven.len(), graph.pose_driven.len());
        let merged_groups = merged.driver_groups().unwrap();
        assert!(merged_groups.len() >= target_groups.len());
        assert!(
            merged.graph.as_ref().unwrap().curve_count
                >= target.graph.as_ref().unwrap().curve_count
        );
        // The imported group's driver and driven bones must now exist by name.
        for bone in &source_groups[0].required_bone_names {
            assert!(
                graph.bones.contains(bone),
                "{source_name}: bone {bone} missing after merge"
            );
        }
        merged_count += 1;
    }
    assert!(merged_count > 0);
}

#[test]
fn support_bone_driver_groups_merge_between_corpus_files() {
    let documents = support_corpus();
    if documents.len() < 2 {
        return;
    }
    for window in documents.windows(2).take(8) {
        let (target_name, target) = &window[0];
        let (source_name, source) = &window[1];
        let source_groups = source.driver_groups().unwrap();
        let merged = target
            .merge_driver_group(source, 0)
            .unwrap_or_else(|error| panic!("{source_name} into {target_name}: {error}"));
        let summary = merged.validate().unwrap();
        let graph = merged.graph.as_ref().unwrap();
        assert_eq!(
            header_int(&merged.tree, bphyssb::BONE_COUNT),
            summary.bone_count as i32
        );
        assert_eq!(
            header_int(&merged.tree, bphyssb::MAIN_BONE_COUNT),
            summary.main_bone_count as i32
        );
        assert_eq!(
            header_int(&merged.tree, bphyssb::SUPPORT_BONE_COUNT),
            summary.support_bone_count as i32
        );
        assert_eq!(
            header_int(&merged.tree, bphyssb::CURVE_COUNT),
            summary.curve_count as i32
        );
        assert!(merged.driver_groups().unwrap().len() >= target.driver_groups().unwrap().len());
        for bone in &source_groups[0].required_bone_names {
            assert!(
                graph.bones.contains(bone),
                "{source_name}: bone {bone} missing after merge"
            );
        }
    }
}

#[test]
fn mirroring_a_helper_group_reflects_its_poses() {
    for (name, document) in helper_corpus().into_iter().take(6) {
        let groups = document.driver_groups().unwrap();
        let mirrored = document
            .mirror_driver_group_across_x(0)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let before = document.graph.as_ref().unwrap();
        let after = mirrored.graph.as_ref().unwrap();
        assert_eq!(before.drivers.len(), after.drivers.len());
        for index in &groups[0].driver_indices {
            let (source, target) = (&before.drivers[*index], &after.drivers[*index]);
            assert_eq!(
                target.translation[0], -source.translation[0],
                "{name}: x flips"
            );
            assert_eq!(target.translation[1], source.translation[1]);
            // Mirroring normalises the quaternion, so allow float noise.
            assert!(
                (target.rotation[0] - source.rotation[0]).abs() < 1.0e-4,
                "{name}: rotation x"
            );
            assert!(
                (target.rotation[1] + source.rotation[1]).abs() < 1.0e-4,
                "{name}: rotation y flips"
            );
        }
        // Mirroring twice restores the original names.
        let restored = mirrored.mirror_driver_group_across_x(0).unwrap();
        assert_eq!(restored.graph.as_ref().unwrap().bones, before.bones);
    }
}

#[test]
fn support_bone_records_clone_onto_a_new_target_bone() {
    for (name, document) in support_corpus().into_iter().take(6) {
        let graph = document.graph.as_ref().unwrap();
        let Some(source_bone) = graph.support_bones.first().map(|bone| bone.bone_id) else {
            continue;
        };
        let (rebuilt, target) = document
            .create_support_from_bone(source_bone as usize, "Test_Support")
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(target, graph.bones.len());
        let rebuilt_graph = rebuilt.graph.as_ref().unwrap();
        assert_eq!(rebuilt_graph.bones[target], "Test_Support");
        let cloned = rebuilt_graph
            .support_bones
            .iter()
            .filter(|bone| bone.bone_id == target as i32)
            .count();
        let original = graph
            .support_bones
            .iter()
            .filter(|bone| bone.bone_id == source_bone)
            .count();
        assert_eq!(cloned, original, "{name}: every support record was cloned");
        assert!(rebuilt
            .create_support_from_bone(source_bone as usize, "Test_Support")
            .is_err());
    }
}

#[test]
fn helper_bone_support_cloning_copies_outputs_and_curves() {
    for (name, document) in helper_corpus().into_iter().take(6) {
        let graph = document.graph.as_ref().unwrap();
        let Some(source_bone) = graph.driven.first().map(|bone| bone.bone_id) else {
            continue;
        };
        let (rebuilt, target) = document
            .create_support_from_bone(source_bone as usize, "Test_Support")
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let rebuilt_graph = rebuilt.graph.as_ref().unwrap();
        assert_eq!(rebuilt_graph.bones[target], "Test_Support");
        assert!(rebuilt_graph.driven.len() > graph.driven.len());
        assert_eq!(rebuilt_graph.driven.len(), rebuilt_graph.pose_driven.len());
        assert!(rebuilt_graph.outputs.len() >= graph.outputs.len());
        assert_eq!(
            header_int(&rebuilt.tree, crc32("output_num")),
            rebuilt_graph.outputs.len() as i32
        );
        assert_eq!(
            header_int(&rebuilt.tree, crc32("connection_curve_num")),
            rebuilt_graph.curve_count as i32
        );
    }
}

#[test]
fn conversion_between_the_two_sidecar_formats_preserves_the_graph() {
    for (name, document) in helper_corpus() {
        let support = SupportBoneDocument::from_helper_bones(&document)
            .unwrap_or_else(|error| panic!("{name} -> bphyssb: {error}"));
        let summary = support.validate().unwrap();
        let graph = document.graph.as_ref().unwrap();
        assert_eq!(summary.main_bone_count, graph.drivers.len());
        assert_eq!(summary.support_bone_count, graph.driven.len());
        assert_eq!(summary.curve_count, graph.curve_count);
        assert_eq!(
            support.driver_groups().unwrap().len(),
            document.driver_groups().unwrap().len(),
            "{name}: driver groups survive conversion"
        );

        let back = HelperBoneDocument::from_support_bones(&support)
            .unwrap_or_else(|error| panic!("{name} -> bphhb: {error}"));
        let back_graph = back.graph.as_ref().unwrap();
        let bone = |graph: &bphhb::HelperBoneGraph, id: i32| graph.bones[id as usize].clone();
        let driver_names: Vec<String> = graph
            .drivers
            .iter()
            .map(|driver| bone(graph, driver.bone_id))
            .collect();
        let back_driver_names: Vec<String> = back_graph
            .drivers
            .iter()
            .map(|driver| bone(back_graph, driver.bone_id))
            .collect();
        assert_eq!(driver_names, back_driver_names, "{name}: driver bones");
        let driven_names: Vec<String> = graph
            .driven
            .iter()
            .map(|driven| bone(graph, driven.bone_id))
            .collect();
        let back_driven_names: Vec<String> = back_graph
            .driven
            .iter()
            .map(|driven| bone(back_graph, driven.bone_id))
            .collect();
        assert_eq!(driven_names, back_driven_names, "{name}: driven bones");
        assert_eq!(back_graph.curve_count, graph.curve_count);
    }
}

#[test]
fn botw_support_bones_convert_to_helper_bones() {
    for (name, document) in support_corpus() {
        let helper = HelperBoneDocument::from_support_bones(&document)
            .unwrap_or_else(|error| panic!("{name} -> bphhb: {error}"));
        let graph = helper.graph.as_ref().unwrap();
        let source = document.graph.as_ref().unwrap();
        assert_eq!(
            graph.drivers.len(),
            source.main_bones.len(),
            "{name}: drivers"
        );
        assert_eq!(
            graph.driven.len(),
            source.support_bones.len(),
            "{name}: driven"
        );
        assert_eq!(graph.curve_count, source.curve_count, "{name}: curves");
        for driver in &graph.drivers {
            assert!(driver.base_bone_id >= 0, "{name}: driver has a base bone");
            assert_ne!(
                driver.base_bone_id, driver.bone_id,
                "{name}: a helper never bases itself"
            );
        }
        assert_eq!(
            helper.driver_groups().unwrap().len(),
            document.driver_groups().unwrap().len(),
            "{name}: driver groups survive conversion"
        );
    }
}
