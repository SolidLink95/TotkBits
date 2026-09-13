//! Skeletons independent of their container: the bone hierarchy of a COLLADA
//! (`.dae`) visual scene or an FBX model tree, the union of several of them,
//! and the bones-only FBX that carries the result into the Toolbox-compatible
//! BFRES skeleton import.
//!
//! A [`Skeleton`] lists its bones in document order (depth first, children in
//! source order, every parent before its children). Transforms are the local
//! FBX triple: translation, Euler XYZ rotation in degrees (`Lcl Rotation`
//! with the default rotation order) and scale.

pub mod compare;
pub mod dae;
pub mod fbx;
pub mod fbx_writer;
pub mod merge;

use std::io;
use std::path::Path;

pub use compare::{compare, SkeletonDiff};
pub use merge::{merge, MergeReport};

#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    pub name: String,
    /// Index into the same list; `None` for a scene root such as `Armature`.
    pub parent: Option<usize>,
    pub translation: [f32; 3],
    /// Euler XYZ in degrees, applied X first (FBX `Lcl Rotation`).
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    /// Referenced by at least one skin; written as an FBX `LimbNode`.
    /// Unreferenced joints and plain nodes become `Null` models, which is
    /// what 3ds Max produces for the same scene.
    pub skinned: bool,
}

impl Bone {
    pub fn new(name: impl Into<String>, parent: Option<usize>) -> Self {
        Self {
            name: name.into(),
            parent,
            translation: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            skinned: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
}

impl Skeleton {
    /// Reads a skeleton from a `.dae` or `.fbx` file, chosen by content.
    pub fn from_path(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let data = std::fs::read(path)?;
        Self::from_bytes(&data)
            .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))
    }

    /// Reads a skeleton from COLLADA XML or binary FBX bytes.
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.starts_with(b"Kaydara FBX Binary") {
            return fbx::parse(data);
        }
        let head = &data[..data.len().min(512)];
        if head
            .windows(8)
            .any(|window| window == b"<COLLADA" || window == b"<?xml ")
        {
            return dae::parse(data);
        }
        Err(invalid("not a COLLADA (.dae) or binary FBX file"))
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|bone| bone.name == name)
    }

    pub fn children(&self, index: usize) -> Vec<usize> {
        self.bones
            .iter()
            .enumerate()
            .filter(|(_, bone)| bone.parent == Some(index))
            .map(|(child, _)| child)
            .collect()
    }

    pub fn roots(&self) -> Vec<usize> {
        self.bones
            .iter()
            .enumerate()
            .filter(|(_, bone)| bone.parent.is_none())
            .map(|(index, _)| index)
            .collect()
    }

    /// Every parent index must point at an earlier bone and names must be
    /// unique: both are what the merge and the FBX writer rely on.
    pub fn validate(&self) -> io::Result<()> {
        let mut seen = std::collections::HashSet::new();
        for (index, bone) in self.bones.iter().enumerate() {
            if bone.name.is_empty() {
                return Err(invalid(format!("bone {index} has no name")));
            }
            if !seen.insert(bone.name.as_str()) {
                return Err(invalid(format!("duplicate bone name {}", bone.name)));
            }
            if let Some(parent) = bone.parent {
                if parent >= index {
                    return Err(invalid(format!(
                        "bone {} references parent {parent} that does not precede it",
                        bone.name
                    )));
                }
            }
        }
        Ok(())
    }

    /// Path of names from the root down to `index`, joined with `/`.
    pub fn path(&self, index: usize) -> String {
        let mut parts = Vec::new();
        let mut current = Some(index);
        while let Some(bone) = current.and_then(|i| self.bones.get(i)) {
            parts.push(bone.name.as_str());
            current = bone.parent;
        }
        parts.reverse();
        parts.join("/")
    }
}

/// A 4x4 matrix with the column-vector convention (`m[row][column]`, the
/// translation in the last column), as COLLADA `<matrix>` lists it in row
/// major text order.
pub type Matrix4 = [[f32; 4]; 4];

pub const IDENTITY: Matrix4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Splits an affine matrix into translation, Euler XYZ degrees and scale.
/// The rotation convention is FBX's default `eEulerXYZ`: X is applied first,
/// so the rotation block is `Rz * Ry * Rx`. Computed in f64 so the f32
/// inputs do not lose precision on the way through the trigonometry.
pub fn decompose_matrix(m: &Matrix4) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let translation = [m[0][3], m[1][3], m[2][3]];
    let column = |c: usize| [m[0][c] as f64, m[1][c] as f64, m[2][c] as f64];
    let length = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let (cx, cy, cz) = (column(0), column(1), column(2));
    let mut scale = [length(cx), length(cy), length(cz)];
    // A negative determinant means a mirrored basis; fold the sign into X.
    let det = cx[0] * (cy[1] * cz[2] - cy[2] * cz[1]) - cy[0] * (cx[1] * cz[2] - cx[2] * cz[1])
        + cz[0] * (cx[1] * cy[2] - cx[2] * cy[1]);
    if det < 0.0 {
        scale[0] = -scale[0];
    }
    let safe = |s: f64| if s.abs() < f64::EPSILON { 1.0 } else { s };
    let r = [
        [
            cx[0] / safe(scale[0]),
            cy[0] / safe(scale[1]),
            cz[0] / safe(scale[2]),
        ],
        [
            cx[1] / safe(scale[0]),
            cy[1] / safe(scale[1]),
            cz[1] / safe(scale[2]),
        ],
        [
            cx[2] / safe(scale[0]),
            cy[2] / safe(scale[1]),
            cz[2] / safe(scale[2]),
        ],
    ];
    let sy = (-r[2][0]).clamp(-1.0, 1.0);
    let ry = sy.asin();
    let (rx, rz) = if (sy.abs() - 1.0).abs() > 1e-9 {
        (r[2][1].atan2(r[2][2]), r[1][0].atan2(r[0][0]))
    } else {
        // Gimbal lock: put the whole remaining rotation on X.
        ((-r[1][2]).atan2(r[1][1]), 0.0)
    };
    let rotation = [
        rx.to_degrees() as f32,
        ry.to_degrees() as f32,
        rz.to_degrees() as f32,
    ];
    (
        translation,
        rotation,
        [scale[0] as f32, scale[1] as f32, scale[2] as f32],
    )
}

/// Rebuilds the affine matrix from a decomposed triple (`T * Rz*Ry*Rx * S`).
pub fn compose_matrix(translation: [f32; 3], rotation: [f32; 3], scale: [f32; 3]) -> Matrix4 {
    let (sx, cx) = (rotation[0] as f64).to_radians().sin_cos();
    let (sy, cy) = (rotation[1] as f64).to_radians().sin_cos();
    let (sz, cz) = (rotation[2] as f64).to_radians().sin_cos();
    // Rz * Ry * Rx
    let r = [
        [cz * cy, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx],
        [sz * cy, sz * sy * sx + cz * cx, sz * sy * cx - cz * sx],
        [-sy, cy * sx, cy * cx],
    ];
    let mut m = IDENTITY;
    for row in 0..3 {
        for col in 0..3 {
            m[row][col] = (r[row][col] * scale[col] as f64) as f32;
        }
        m[row][3] = translation[row];
    }
    m
}

pub(crate) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decomposition_round_trips_a_general_transform() {
        let t = [0.0487392, -0.0223346, -0.0214719];
        let r = [-40.0, -12.0, -60.0];
        let s = [1.0, 1.0, 1.0];
        let m = compose_matrix(t, r, s);
        let (t2, r2, s2) = decompose_matrix(&m);
        for axis in 0..3 {
            assert!((t[axis] - t2[axis]).abs() < 1e-6, "{t2:?}");
            assert!((r[axis] - r2[axis]).abs() < 1e-3, "{r2:?}");
            assert!((s[axis] - s2[axis]).abs() < 1e-6, "{s2:?}");
        }
    }

    #[test]
    fn decomposition_matches_the_collada_hair_bone() {
        // Hair_A_2 from Armor_005_Head.dae; 3ds Max reports (-40, -12, -60).
        let m: Matrix4 = [
            [0.4890738, 0.730235934, 0.4770351, 0.0487392],
            [-0.847100735, 0.2672843, 0.459325135, -0.0223346],
            [0.207911685, -0.6287406, 0.749305069, -0.0214719],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let (_, r, _) = decompose_matrix(&m);
        assert!((r[0] + 40.0).abs() < 1e-3, "{r:?}");
        assert!((r[1] + 12.0).abs() < 1e-3, "{r:?}");
        assert!((r[2] + 60.0).abs() < 1e-3, "{r:?}");
    }

    #[test]
    fn validate_rejects_forward_parents_and_duplicates() {
        let mut skeleton = Skeleton {
            bones: vec![Bone::new("a", Some(1)), Bone::new("b", None)],
        };
        assert!(skeleton.validate().is_err());
        skeleton.bones[0].parent = None;
        skeleton.bones[1].name = "a".into();
        assert!(skeleton.validate().is_err());
    }
}
