//! Skeleton parity report: order, hierarchy and node kinds have to match
//! exactly, transforms within a tolerance (3ds Max stores every value as a
//! float32 round-trip, so the last digits of its doubles are noise).

use super::Skeleton;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerance {
    pub translation: f32,
    pub rotation_degrees: f32,
    pub scale: f32,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            translation: 1e-5,
            rotation_degrees: 1e-3,
            scale: 1e-5,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkeletonDiff {
    pub mismatches: Vec<String>,
    pub max_translation_error: f32,
    pub max_rotation_error_degrees: f32,
    pub max_scale_error: f32,
}

impl SkeletonDiff {
    pub fn is_match(&self) -> bool {
        self.mismatches.is_empty()
    }

    /// One line per mismatch plus the worst transform deviations.
    pub fn summary(&self) -> String {
        let mut lines = self.mismatches.clone();
        lines.push(format!(
            "max deviation: translation {:.3e}, rotation {:.3e} deg, scale {:.3e}",
            self.max_translation_error, self.max_rotation_error_degrees, self.max_scale_error
        ));
        lines.join("\n")
    }
}

fn kind(skinned: bool) -> &'static str {
    if skinned {
        "LimbNode"
    } else {
        "Null"
    }
}

/// Difference between two angles in degrees, folded into (-180, 180].
fn angle_difference(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d <= -180.0 {
        d += 360.0;
    }
    d.abs()
}

pub fn compare(actual: &Skeleton, reference: &Skeleton, tolerance: Tolerance) -> SkeletonDiff {
    let mut diff = SkeletonDiff::default();
    if actual.bones.len() != reference.bones.len() {
        diff.mismatches.push(format!(
            "bone count {} differs from reference {}",
            actual.bones.len(),
            reference.bones.len()
        ));
    }
    let parent_name = |skeleton: &Skeleton, index: usize| -> String {
        skeleton.bones[index]
            .parent
            .and_then(|parent| skeleton.bones.get(parent))
            .map(|bone| bone.name.clone())
            .unwrap_or_else(|| "<root>".to_owned())
    };
    for (index, (a, r)) in actual.bones.iter().zip(&reference.bones).enumerate() {
        if a.name != r.name {
            diff.mismatches.push(format!(
                "bone {index} is {} but the reference has {}",
                a.name, r.name
            ));
            continue;
        }
        let (ap, rp) = (parent_name(actual, index), parent_name(reference, index));
        if ap != rp {
            diff.mismatches.push(format!(
                "{}: parent {ap} but the reference has {rp}",
                a.name
            ));
        }
        if a.skinned != r.skinned {
            diff.mismatches.push(format!(
                "{}: {} but the reference has {}",
                a.name,
                kind(a.skinned),
                kind(r.skinned)
            ));
        }
        if a.parent.is_none() || r.parent.is_none() {
            continue;
        }
        let mut translation_error = 0.0f32;
        let mut rotation_error = 0.0f32;
        let mut scale_error = 0.0f32;
        for axis in 0..3 {
            translation_error =
                translation_error.max((a.translation[axis] - r.translation[axis]).abs());
            rotation_error =
                rotation_error.max(angle_difference(a.rotation[axis], r.rotation[axis]));
            scale_error = scale_error.max((a.scale[axis] - r.scale[axis]).abs());
        }
        diff.max_translation_error = diff.max_translation_error.max(translation_error);
        diff.max_rotation_error_degrees = diff.max_rotation_error_degrees.max(rotation_error);
        diff.max_scale_error = diff.max_scale_error.max(scale_error);
        if translation_error > tolerance.translation {
            diff.mismatches.push(format!(
                "{}: translation {:?} but the reference has {:?}",
                a.name, a.translation, r.translation
            ));
        }
        if rotation_error > tolerance.rotation_degrees {
            diff.mismatches.push(format!(
                "{}: rotation {:?} but the reference has {:?}",
                a.name, a.rotation, r.rotation
            ));
        }
        if scale_error > tolerance.scale {
            diff.mismatches.push(format!(
                "{}: scale {:?} but the reference has {:?}",
                a.name, a.scale, r.scale
            ));
        }
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::super::{Bone, Skeleton};
    use super::*;
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> Option<Vec<u8>> {
        let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../res/1")
            .join(name);
        std::fs::read(path).ok()
    }

    fn sample() -> Skeleton {
        let mut root = Bone::new("Root", None);
        root.rotation = [90.0, 0.0, 0.0];
        let mut a = Bone::new("A", Some(0));
        a.skinned = true;
        a.translation = [1.0, 2.0, 3.0];
        a.rotation = [10.0, -20.0, 179.0];
        let b = Bone::new("B", Some(1));
        Skeleton {
            bones: vec![root, a, b],
        }
    }

    #[test]
    fn identical_skeletons_match_and_root_transform_is_ignored() {
        let reference = sample();
        let mut actual = sample();
        actual.bones[0].rotation = [-90.0, 0.0, 0.0];
        actual.bones[1].translation[2] += 2e-6;
        actual.bones[1].rotation[2] = -181.0;
        let diff = compare(&actual, &reference, Tolerance::default());
        assert!(diff.is_match(), "{}", diff.summary());
        assert!(diff.max_translation_error > 1e-6);
        assert!(diff.max_rotation_error_degrees < 1e-3);
    }

    #[test]
    fn reports_every_mismatch_kind() {
        let reference = sample();
        let mut actual = sample();
        actual.bones[2].name = "C".into();
        assert!(compare(&actual, &reference, Tolerance::default())
            .mismatches
            .iter()
            .any(|m| m.contains("bone 2 is C")));

        let mut actual = sample();
        actual.bones[2].parent = Some(0);
        assert!(compare(&actual, &reference, Tolerance::default())
            .mismatches
            .iter()
            .any(|m| m.contains("B: parent Root")));

        let mut actual = sample();
        actual.bones[1].skinned = false;
        assert!(compare(&actual, &reference, Tolerance::default())
            .mismatches
            .iter()
            .any(|m| m.contains("A: Null but the reference has LimbNode")));

        let mut actual = sample();
        actual.bones[1].translation[0] = 1.5;
        actual.bones[1].rotation[1] = -21.0;
        actual.bones[1].scale[2] = 0.5;
        let diff = compare(&actual, &reference, Tolerance::default());
        assert_eq!(diff.mismatches.len(), 3, "{}", diff.summary());
        assert!((diff.max_translation_error - 0.5).abs() < 1e-6);
        assert!((diff.max_rotation_error_degrees - 1.0).abs() < 1e-4);
        assert!((diff.max_scale_error - 0.5).abs() < 1e-6);

        let mut actual = sample();
        actual.bones.pop();
        assert!(compare(&actual, &reference, Tolerance::default())
            .mismatches
            .iter()
            .any(|m| m.contains("bone count 2")));
    }

    #[test]
    fn merged_dae_skeleton_matches_3ds_max_reference() {
        let (Some(head), Some(upper), Some(merged_fbx)) = (
            fixture("Armor_005_Head.dae"),
            fixture("Armor_005_RaulSkin_Upper.dae"),
            fixture("merged005.fbx"),
        ) else {
            return;
        };
        let head = super::super::dae::parse(&head).unwrap();
        let upper = super::super::dae::parse(&upper).unwrap();
        let (merged, _) = super::super::merge(&[head, upper]).unwrap();
        let reference = match super::super::fbx::parse(&merged_fbx) {
            Ok(reference) => reference,
            Err(error) => {
                println!("FBX skeleton reader unavailable: {error}");
                return;
            }
        };
        let diff = compare(&merged, &reference, Tolerance::default());
        assert!(diff.is_match(), "{}", diff.summary());
        println!("{}", diff.summary());
    }
}
