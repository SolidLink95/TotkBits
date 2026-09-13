//! Union of several skeletons by bone name.
//!
//! The result keeps the first source's tree and hangs every later source's
//! new bones under their (name-matched) parents, after the children that are
//! already there, in the later source's own order. Serialized depth first
//! this is exactly the order 3ds Max produces when it imports one COLLADA
//! scene over another.

use super::{invalid, Bone, Skeleton};
use std::collections::HashMap;
use std::io;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeReport {
    /// Bones each source contributed that no earlier source had; the first
    /// source contributes everything.
    pub added: Vec<Vec<String>>,
}

struct MergedNode {
    bone: Bone,
    children: Vec<usize>,
}

pub fn merge(skeletons: &[Skeleton]) -> io::Result<(Skeleton, MergeReport)> {
    let Some(first) = skeletons.first() else {
        return Err(invalid("no skeletons to merge"));
    };
    for (index, skeleton) in skeletons.iter().enumerate() {
        skeleton
            .validate()
            .map_err(|error| invalid(format!("skeleton {index}: {error}")))?;
    }

    let mut nodes: Vec<MergedNode> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut report = MergeReport::default();

    // Source 0 as it is.
    let mut added = Vec::new();
    for bone in &first.bones {
        let index = nodes.len();
        let parent = match bone.parent {
            Some(parent) => Some(
                *by_name
                    .get(&first.bones[parent].name)
                    .ok_or_else(|| invalid("skeleton 0 is not in parent-first order"))?,
            ),
            None => None,
        };
        nodes.push(MergedNode {
            bone: Bone {
                parent,
                ..bone.clone()
            },
            children: Vec::new(),
        });
        match parent {
            Some(parent) => nodes[parent].children.push(index),
            None => roots.push(index),
        }
        by_name.insert(bone.name.clone(), index);
        added.push(bone.name.clone());
    }
    report.added.push(added);

    for (source_index, skeleton) in skeletons.iter().enumerate().skip(1) {
        let mut added = Vec::new();
        for bone in &skeleton.bones {
            let parent_name = bone
                .parent
                .map(|parent| skeleton.bones[parent].name.as_str());
            match by_name.get(&bone.name).copied() {
                Some(existing) => {
                    let existing_parent = nodes[existing]
                        .bone
                        .parent
                        .map(|parent| nodes[parent].bone.name.clone());
                    if existing_parent.as_deref() != parent_name {
                        return Err(invalid(format!(
                            "bone {} has parent {} in skeleton {source_index} but parent {} in an earlier skeleton",
                            bone.name,
                            parent_name.unwrap_or("<root>"),
                            existing_parent.as_deref().unwrap_or("<root>")
                        )));
                    }
                    nodes[existing].bone.skinned |= bone.skinned;
                }
                None => {
                    let parent = match parent_name {
                        Some(name) => Some(*by_name.get(name).ok_or_else(|| {
                            invalid(format!(
                                "skeleton {source_index}: parent {name} of {} is missing",
                                bone.name
                            ))
                        })?),
                        None => {
                            let existing_roots: Vec<&str> = roots
                                .iter()
                                .map(|root| nodes[*root].bone.name.as_str())
                                .collect();
                            return Err(invalid(format!(
                                "skeleton {source_index} has root {} but the merged skeleton's roots are {}",
                                bone.name,
                                existing_roots.join(", ")
                            )));
                        }
                    };
                    let index = nodes.len();
                    nodes.push(MergedNode {
                        bone: Bone {
                            parent,
                            ..bone.clone()
                        },
                        children: Vec::new(),
                    });
                    if let Some(parent) = parent {
                        nodes[parent].children.push(index);
                    }
                    by_name.insert(bone.name.clone(), index);
                    added.push(bone.name.clone());
                }
            }
        }
        report.added.push(added);
    }

    // Serialize depth first, remapping parent indices to the new order.
    let mut skeleton = Skeleton::default();
    let mut new_index: Vec<Option<usize>> = vec![None; nodes.len()];
    let mut stack: Vec<usize> = roots.iter().rev().copied().collect();
    while let Some(index) = stack.pop() {
        let node = &nodes[index];
        let parent = match node.bone.parent {
            Some(parent) => Some(new_index[parent].ok_or_else(|| {
                invalid(format!(
                    "bone {} was serialized before its parent",
                    node.bone.name
                ))
            })?),
            None => None,
        };
        new_index[index] = Some(skeleton.bones.len());
        skeleton.bones.push(Bone {
            parent,
            ..node.bone.clone()
        });
        for child in node.children.iter().rev() {
            stack.push(*child);
        }
    }
    skeleton.validate()?;
    Ok((skeleton, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> Option<Vec<u8>> {
        let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../res/1")
            .join(name);
        std::fs::read(path).ok()
    }

    fn skeleton(spec: &[(&str, Option<&str>, bool)]) -> Skeleton {
        let mut skeleton = Skeleton::default();
        for (name, parent, skinned) in spec {
            let parent = parent.map(|p| skeleton.index_of(p).unwrap());
            let mut bone = Bone::new(*name, parent);
            bone.skinned = *skinned;
            skeleton.bones.push(bone);
        }
        skeleton
    }

    fn names(skeleton: &Skeleton) -> Vec<&str> {
        skeleton.bones.iter().map(|b| b.name.as_str()).collect()
    }

    #[test]
    fn appends_new_children_after_existing_ones() {
        let a = skeleton(&[
            ("Root", None, false),
            ("A", Some("Root"), true),
            ("A1", Some("A"), true),
            ("B", Some("Root"), false),
        ]);
        let b = skeleton(&[
            ("Root", None, false),
            ("A", Some("Root"), false),
            ("A0", Some("A"), true),
            ("A1", Some("A"), false),
            ("B", Some("Root"), true),
            ("C", Some("Root"), true),
        ]);
        let (merged, report) = merge(&[a, b]).unwrap();
        assert_eq!(names(&merged), ["Root", "A", "A1", "A0", "B", "C"]);
        assert_eq!(report.added[1], ["A0", "C"]);
        assert!(merged.bones[merged.index_of("B").unwrap()].skinned);
        assert_eq!(
            merged.bones[merged.index_of("A0").unwrap()].parent,
            merged.index_of("A")
        );
    }

    #[test]
    fn rejects_parent_and_root_conflicts() {
        let a = skeleton(&[("Root", None, false), ("A", Some("Root"), true)]);
        let b = skeleton(&[
            ("Root", None, false),
            ("B", Some("Root"), true),
            ("A", Some("B"), true),
        ]);
        assert!(merge(&[a.clone(), b]).is_err());
        let c = skeleton(&[("Other", None, false), ("A", Some("Other"), true)]);
        assert!(merge(&[a, c]).is_err());
        assert!(merge(&[]).is_err());
    }

    #[test]
    fn head_and_upper_merge_in_3ds_max_order() {
        let (Some(head), Some(upper)) = (
            fixture("Armor_005_Head.dae"),
            fixture("Armor_005_RaulSkin_Upper.dae"),
        ) else {
            return;
        };
        let head = super::super::dae::parse(&head).unwrap();
        let upper = super::super::dae::parse(&upper).unwrap();
        let (merged, report) = merge(&[head, upper]).unwrap();
        let names = names(&merged);
        println!("merged order: {}", names.join(" "));
        assert_eq!(names.len(), 107);
        assert_eq!(
            &names[..12],
            [
                "Armature",
                "Root",
                "Skl_Root",
                "Spine_1",
                "Spine_2",
                "Clavicle_L",
                "Arm_1_L",
                "Arm_2_L",
                "Elbow_L",
                "Wrist_Assist_L",
                "Wrist_L",
                "Finger_A_1_L"
            ]
        );
        let position = |name: &str| names.iter().position(|n| *n == name).unwrap();
        assert_eq!(position("Arm_1_Assist_L"), position("Hand_3_Assist_L") + 1);
        assert_eq!(
            position("Clavicle_Assist_L"),
            position("Arm_1_Assist_L") + 1
        );
        assert_eq!(position("Waist"), position("Chin") + 1);
        assert_eq!(position("Face_Root"), position("Hat_4_Armor") + 1);
        let nulls: Vec<&str> = merged
            .bones
            .iter()
            .filter(|b| !b.skinned)
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(
            nulls,
            [
                "Armature",
                "Root",
                "Skl_Root",
                "Wrist_R",
                "Face_Root",
                "Leg_1_L",
                "Leg_1_R"
            ]
        );
        assert_eq!(report.added[0].len(), 50);
        assert_eq!(report.added[1].len(), 57);
    }
}
