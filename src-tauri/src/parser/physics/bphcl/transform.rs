//! Affine transforms of a BPHCL cloth package, driven by the editable
//! `Transform.yaml` leaf of the tree view (and by the CLI).
//!
//! Two kinds of edits share one YAML sheet:
//!
//! * `document` — one scale / rotation / location applied to the whole
//!   package, the way the model would change when a DCC tool transforms it
//!   and applies the transform: model-space points (particle rest positions,
//!   object-space skin vertices, root bones) get the full affine map, the
//!   inverse bind matrices follow the rotated skeleton, bone-space offsets
//!   (child bone translations, collidable offsets, collider shapes) get the
//!   scale expressed in their bone's frame, and every scalar length (rest
//!   lengths, radii) is multiplied by the geometric mean of the scale. With a
//!   uniform scale and no rotation this is exactly the legacy rescaler.
//! * `bones` — reference-pose edits of named skeleton bones in parent space:
//!   translation gains `location`, rotation is pre-multiplied by `rotation`,
//!   the pose scale is multiplied by `scale`.
//!
//! Every block carries `use: false` by default; only enabled blocks are
//! applied, and the sheet returned after an apply has them switched off so a
//! second save does not apply them twice.

use super::{hkcl_convert::pack_vector3, reflect::Reflect, BphclDocument};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{self, ErrorKind},
};

pub use super::rescale::{MAX_SCALE, MIN_SCALE};

/// Tree-view leaf name of the sheet.
pub const TRANSFORM_LEAF: &str = "Transform.yaml";

/// One scale / rotation / location block of the sheet.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TransformSpec {
    #[serde(rename = "use", default)]
    pub enabled: bool,
    #[serde(default = "unit_scale")]
    pub scale: [f32; 3],
    /// XYZ Euler angles in degrees, applied X first, then Y, then Z.
    #[serde(default)]
    pub rotation: [f32; 3],
    #[serde(default)]
    pub location: [f32; 3],
}

fn unit_scale() -> [f32; 3] {
    [1.0; 3]
}

impl Default for TransformSpec {
    fn default() -> Self {
        Self {
            enabled: false,
            scale: unit_scale(),
            rotation: [0.0; 3],
            location: [0.0; 3],
        }
    }
}

impl TransformSpec {
    pub fn uniform_scale(scale: f32) -> Self {
        Self {
            enabled: true,
            scale: [scale; 3],
            ..Self::default()
        }
    }

    pub fn is_identity(&self) -> bool {
        self.scale == unit_scale() && self.rotation == [0.0; 3] && self.location == [0.0; 3]
    }

    fn validate(&self, what: &str) -> io::Result<()> {
        let finite = self
            .scale
            .iter()
            .chain(&self.rotation)
            .chain(&self.location)
            .all(|v| v.is_finite());
        if !finite {
            return Err(invalid_input(format!(
                "{what}: every value must be a finite number"
            )));
        }
        if self
            .scale
            .iter()
            .any(|s| !(MIN_SCALE..=MAX_SCALE).contains(s))
        {
            return Err(invalid_input(format!(
                "{what}: scale {:?} is outside {MIN_SCALE}..={MAX_SCALE}",
                self.scale
            )));
        }
        Ok(())
    }
}

/// The parsed `Transform.yaml`.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransformSheet {
    #[serde(default)]
    pub document: Option<TransformSpec>,
    #[serde(default)]
    pub bones: IndexMap<String, TransformSpec>,
}

impl TransformSheet {
    /// Whether any block is switched on.
    pub fn has_enabled(&self) -> bool {
        self.document.as_ref().is_some_and(|spec| spec.enabled)
            || self.bones.values().any(|spec| spec.enabled)
    }

    fn enabled_bones(&self) -> impl Iterator<Item = (&String, &TransformSpec)> {
        self.bones.iter().filter(|(_, spec)| spec.enabled)
    }
}

/// Parses and validates a sheet. An empty or comment-only file is a sheet
/// with nothing enabled.
pub fn parse_transform_yaml(text: &str) -> io::Result<TransformSheet> {
    let value: serde_yaml::Value = serde_yaml::from_str(text)
        .map_err(|error| invalid_input(format!("{TRANSFORM_LEAF}: {error}")))?;
    if value.is_null() {
        return Ok(TransformSheet::default());
    }
    let sheet: TransformSheet = serde_yaml::from_value(value)
        .map_err(|error| invalid_input(format!("{TRANSFORM_LEAF}: {error}")))?;
    if let Some(spec) = &sheet.document {
        spec.validate("document")?;
    }
    for (name, spec) in &sheet.bones {
        spec.validate(&format!("bone '{name}'"))?;
    }
    Ok(sheet)
}

/// Switches every `use: true` line back to `use: false`, leaving comments,
/// indentation and every other line untouched.
pub fn reset_use_flags(text: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            let Some(rest) = trimmed.strip_prefix("use:") else {
                return line.to_owned();
            };
            let value = rest.trim_start();
            let gap = &rest[..rest.len() - value.len()];
            let word_len = value
                .find(|c: char| c.is_whitespace() || c == '#')
                .unwrap_or(value.len());
            if !value[..word_len].eq_ignore_ascii_case("true") {
                return line.to_owned();
            }
            format!("{indent}use:{gap}false{}", &value[word_len..])
        })
        .collect()
}

/// The sheet shown for a document that has none stored yet: every block off,
/// with the first root bone of the first skeleton as the per-bone example.
pub fn default_transform_yaml(document: &BphclDocument) -> String {
    let example_bone = document
        .skeletons
        .first()
        .and_then(|skeleton| {
            skeleton
                .bones
                .iter()
                .find(|bone| bone.parent_index.is_none())
                .or_else(|| skeleton.bones.first())
        })
        .map(|bone| bone.name.clone())
        .unwrap_or_else(|| "Root".to_owned());
    let bone_key = if example_bone
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        example_bone
    } else {
        format!("{example_bone:?}")
    };
    format!(
        "\
# Transform.yaml - pending transforms for this BPHCL cloth package.
#
# Save this file (Ctrl+S) to apply every block whose `use` is true, then
# save the BPHCL itself to write the result to disk. Applied blocks are
# switched back to `use: false`, so saving this file again changes nothing.
#
#   scale:    per-axis multipliers, each within {MIN_SCALE} .. {MAX_SCALE}
#   rotation: XYZ Euler angles in degrees (applied X first, then Y, then Z)
#   location: offset in model units, applied after scale and rotation
#
# `document` transforms the whole package the way the model changes when a
# DCC tool moves, rotates or scales it and applies the transform: particle
# rest positions, skin vertex positions and root bones are transformed, bone
# offsets, collider shapes and every length (rest lengths, radii) are scaled.
# A non-uniform scale is exact for positions and approximate for lengths and
# bone-space skin data (they use the geometric mean of the three factors).
#
# `bones` edits the reference pose of the named skeleton bones in parent
# space: translation += location, rotation = rotation * old rotation, the
# pose scale is multiplied by scale. Add any bone listed in the Skeletons
# folder by name; the entry below is only an example.
document:
  use: false
  scale: [1.0, 1.0, 1.0]
  rotation: [0.0, 0.0, 0.0]
  location: [0.0, 0.0, 0.0]
bones:
  {bone_key}:
    use: false
    scale: [1.0, 1.0, 1.0]
    rotation: [0.0, 0.0, 0.0]
    location: [0.0, 0.0, 0.0]
"
    )
}

/// What an apply touched, for the status bar.
#[derive(Clone, Debug, Default)]
pub struct TransformReport {
    /// `(member kind, values written)`.
    pub edits: Vec<(String, usize)>,
    pub document_applied: bool,
    /// Bone names whose reference pose was edited.
    pub bones_applied: Vec<String>,
}

impl TransformReport {
    pub fn touched(&self) -> usize {
        self.edits.iter().map(|(_, count)| count).sum()
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.document_applied {
            parts.push("document transform".to_owned());
        }
        match self.bones_applied.len() {
            0 => {}
            1 => parts.push(format!("bone {}", self.bones_applied[0])),
            n => parts.push(format!("{n} bones")),
        }
        if parts.is_empty() {
            return "nothing enabled".to_owned();
        }
        format!(
            "{} ({} values in {} member kinds)",
            parts.join(" and "),
            self.touched(),
            self.edits.len()
        )
    }
}

impl BphclDocument {
    /// Applies every enabled block of `sheet` and returns the rebuilt bytes.
    /// Enabled bones that no skeleton contains are an error; the document
    /// block is applied first, then the bones in sheet order.
    pub fn apply_transform_sheet(
        &self,
        sheet: &TransformSheet,
    ) -> io::Result<(Vec<u8>, TransformReport)> {
        if let Some(spec) = &sheet.document {
            spec.validate("document")?;
        }
        let mut known: Vec<&str> = Vec::new();
        for bone in self.skeletons.iter().flat_map(|skeleton| &skeleton.bones) {
            if !known.contains(&bone.name.as_str()) {
                known.push(&bone.name);
            }
        }
        for (name, spec) in sheet.enabled_bones() {
            spec.validate(&format!("bone '{name}'"))?;
            if !known.contains(&name.as_str()) {
                let mut listed = known.clone();
                let more = if listed.len() > 24 {
                    format!(", ... ({} bones)", listed.len())
                } else {
                    String::new()
                };
                listed.truncate(24);
                return Err(invalid_input(format!(
                    "bone '{name}' is not in any skeleton of this BPHCL; known bones: {}{more}",
                    listed.join(", ")
                )));
            }
        }
        let reflect = Reflect::new(self)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "BPHCL has no DATA section"))?;
        let mut transformer = Transformer {
            reflect: &reflect,
            raw: self.raw.clone(),
            report: TransformReport::default(),
        };
        let document_spec = sheet
            .document
            .as_ref()
            .filter(|spec| spec.enabled && !spec.is_identity());
        if let Some(spec) = document_spec {
            transformer.apply_document(&Affine::from_spec(spec))?;
            transformer.report.document_applied = true;
        } else if sheet.document.as_ref().is_some_and(|spec| spec.enabled) {
            transformer.report.document_applied = true;
        }
        for (name, spec) in sheet.enabled_bones() {
            transformer.apply_bone(name, spec)?;
            transformer.report.bones_applied.push(name.clone());
        }
        let raw = transformer.raw;
        let rebuilt = BphclDocument::parse(&raw)?;
        rebuilt.validate_item_graph()?;
        Ok((raw, transformer.report))
    }
}

// ---------------------------------------------------------------------------
// Small linear algebra: quaternions are `[x, y, z, w]`, matrices row-major.

type Vec3 = [f32; 3];
type Mat3 = [[f32; 3]; 3];
type Quat = [f32; 4];

const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn diagonal(s: Vec3) -> Mat3 {
    [[s[0], 0.0, 0.0], [0.0, s[1], 0.0], [0.0, 0.0, s[2]]]
}

fn mat_mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    out
}

fn transpose(m: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = m[c][r];
        }
    }
    out
}

fn apply(m: &Mat3, v: Vec3) -> Vec3 {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn inverse(m: &Mat3) -> Option<Mat3> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if !det.is_finite() || det.abs() < 1.0e-12 {
        return None;
    }
    let d = 1.0 / det;
    Some([
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * d,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * d,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * d,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * d,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * d,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * d,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * d,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * d,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * d,
        ],
    ])
}

/// `a ⊗ b`: rotate by `b` first, then by `a`.
fn quat_mul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

fn quat_normalize(q: Quat) -> Quat {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if !len.is_finite() || len < 1.0e-8 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    [q[0] / len, q[1] / len, q[2] / len, q[3] / len]
}

fn quat_about(axis: usize, degrees: f32) -> Quat {
    let half = degrees.to_radians() * 0.5;
    let mut q = [0.0, 0.0, 0.0, half.cos()];
    q[axis] = half.sin();
    q
}

/// XYZ Euler in degrees: X applied first, then Y, then Z (`qz ⊗ qy ⊗ qx`).
fn quat_from_euler_xyz(degrees: Vec3) -> Quat {
    quat_normalize(quat_mul(
        quat_about(2, degrees[2]),
        quat_mul(quat_about(1, degrees[1]), quat_about(0, degrees[0])),
    ))
}

fn mat_from_quat(q: Quat) -> Mat3 {
    let [x, y, z, w] = quat_normalize(q);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

/// `p' = T + R · S · p` and its derived pieces.
struct Affine {
    rotation: Mat3,
    quat: Quat,
    scale: Vec3,
    translation: Vec3,
    /// `Some(s)` when all three factors are equal.
    uniform: Option<f32>,
    /// Geometric mean of the scale, used for lengths and unknown frames.
    mean_scale: f32,
    /// False when the Euler angles are all zero (the rotation is skipped).
    rotates: bool,
}

impl Affine {
    fn from_spec(spec: &TransformSpec) -> Self {
        let rotates = spec.rotation != [0.0; 3];
        let quat = if rotates {
            quat_from_euler_xyz(spec.rotation)
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };
        let uniform = (spec.scale[0] == spec.scale[1] && spec.scale[1] == spec.scale[2])
            .then_some(spec.scale[0]);
        Self {
            rotation: if rotates {
                mat_from_quat(quat)
            } else {
                IDENTITY
            },
            quat,
            scale: spec.scale,
            translation: spec.location,
            uniform,
            mean_scale: (spec.scale[0] * spec.scale[1] * spec.scale[2]).cbrt(),
            rotates,
        }
    }

    fn scaled(&self, v: Vec3) -> Vec3 {
        [
            v[0] * self.scale[0],
            v[1] * self.scale[1],
            v[2] * self.scale[2],
        ]
    }

    /// Model-space point.
    fn point(&self, p: Vec3) -> Vec3 {
        let v = self.rotate(self.scaled(p));
        [
            v[0] + self.translation[0],
            v[1] + self.translation[1],
            v[2] + self.translation[2],
        ]
    }

    fn rotate(&self, v: Vec3) -> Vec3 {
        if self.rotates {
            apply(&self.rotation, v)
        } else {
            v
        }
    }

    /// Scale as seen from a frame whose world rotation is `frame`
    /// (`Rᵀ S R`); the mean scale when the frame is unknown.
    fn frame_scale(&self, frame: Option<&Mat3>) -> Mat3 {
        match (self.uniform, frame) {
            (Some(s), _) => diagonal([s; 3]),
            (None, Some(r)) => mat_mul(&transpose(r), &mat_mul(&diagonal(self.scale), r)),
            (None, None) => diagonal([self.mean_scale; 3]),
        }
    }
}

/// Bind-pose world rotation of every bone of one skeleton.
struct SkeletonFrames {
    world: Vec<Mat3>,
}

impl SkeletonFrames {
    fn frame(&self, bone: usize) -> Option<&Mat3> {
        self.world.get(bone)
    }
}

struct Transformer<'a> {
    reflect: &'a Reflect<'a>,
    raw: Vec<u8>,
    report: TransformReport,
}

impl<'a> Transformer<'a> {
    fn f32_at(&self, offset: u32) -> f32 {
        let at = self.reflect.base + offset as usize;
        f32::from_le_bytes(self.raw[at..at + 4].try_into().unwrap_or([0; 4]))
    }

    fn vec3_at(&self, offset: u32) -> Vec3 {
        [
            self.f32_at(offset),
            self.f32_at(offset + 4),
            self.f32_at(offset + 8),
        ]
    }

    fn quat_at(&self, offset: u32) -> Quat {
        [
            self.f32_at(offset),
            self.f32_at(offset + 4),
            self.f32_at(offset + 8),
            self.f32_at(offset + 12),
        ]
    }

    /// The 3×3 held as three `hkVector4` columns at `offset` (hkRotation,
    /// the rotation part of hkTransform and hkMatrix4).
    fn mat_at(&self, offset: u32) -> Mat3 {
        let mut m = [[0.0; 3]; 3];
        for c in 0..3 {
            for (r, row) in m.iter_mut().enumerate() {
                row[c] = self.f32_at(offset + c as u32 * 16 + r as u32 * 4);
            }
        }
        m
    }

    fn note(&mut self, what: &str, count: usize) {
        match self.report.edits.iter_mut().find(|(name, _)| name == what) {
            Some((_, n)) => *n += count,
            None => self.report.edits.push((what.to_owned(), count)),
        }
    }

    fn set_f32(&mut self, offset: u32, value: f32, what: &str) -> io::Result<()> {
        let at = self.reflect.base + offset as usize;
        let slot = self.raw.get_mut(at..at + 4).ok_or_else(|| {
            io::Error::new(ErrorKind::InvalidData, "transform write exceeds DATA")
        })?;
        slot.copy_from_slice(&value.to_le_bytes());
        self.note(what, 1);
        Ok(())
    }

    fn set_vec3(&mut self, offset: u32, value: Vec3, what: &str) -> io::Result<()> {
        for (i, component) in value.iter().enumerate() {
            self.set_f32(offset + i as u32 * 4, *component, what)?;
        }
        Ok(())
    }

    fn set_quat(&mut self, offset: u32, value: Quat, what: &str) -> io::Result<()> {
        for (i, component) in value.iter().enumerate() {
            self.set_f32(offset + i as u32 * 4, *component, what)?;
        }
        Ok(())
    }

    fn set_mat(&mut self, offset: u32, m: &Mat3, what: &str) -> io::Result<()> {
        for c in 0..3 {
            for r in 0..3 {
                self.set_f32(offset + c as u32 * 16 + r as u32 * 4, m[r][c], what)?;
            }
        }
        Ok(())
    }

    /// Scalar length: zero and non-finite values are left alone.
    fn mul(&mut self, offset: u32, factor: f32, what: &str) -> io::Result<()> {
        let value = self.f32_at(offset);
        if value != 0.0 && value.is_finite() {
            self.set_f32(offset, value * factor, what)?;
        }
        Ok(())
    }

    /// Model-space point at `offset`.
    fn map_point(&mut self, offset: u32, affine: &Affine, what: &str) -> io::Result<()> {
        let p = self.vec3_at(offset);
        self.set_vec3(offset, affine.point(p), what)
    }

    /// Vector in a bone or collider frame: `K · v`.
    fn map_frame_vector(&mut self, offset: u32, k: &Mat3, what: &str) -> io::Result<()> {
        let v = self.vec3_at(offset);
        self.set_vec3(offset, apply(k, v), what)
    }

    fn member(&self, type_index: u32, name: &str) -> io::Result<u32> {
        self.reflect.member_offset(type_index, name).ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "type {} has no member {name}",
                    self.reflect.type_name(type_index)
                ),
            )
        })
    }

    fn array(&self, field: u32) -> Option<(u32, u32, u32)> {
        self.reflect.array(field)
    }

    // ---- skeletons ----

    /// Bone names and bind-pose world rotations of the `hkaSkeleton` item.
    fn skeleton_frames(&self, item_index: usize) -> io::Result<SkeletonFrames> {
        let item = self.reflect.document.items[item_index].clone();
        let (t, base) = (item.type_index, item.data_offset);
        let parents = self
            .reflect
            .u16_array(base + self.member(t, "parentIndices")?);
        let locals: Vec<Quat> = match self.array(base + self.member(t, "referencePose")?) {
            Some((at, count, size)) => (0..count)
                .map(|k| self.quat_at(at + k * size + 16))
                .collect(),
            None => Vec::new(),
        };
        let count = locals.len();
        fn world_of(
            bone: usize,
            locals: &[Quat],
            parents: &[u16],
            memo: &mut [Option<Quat>],
            depth: usize,
        ) -> Quat {
            if let Some(q) = memo[bone] {
                return q;
            }
            let parent = parents.get(bone).copied().unwrap_or(u16::MAX) as usize;
            // A parent chain longer than the bone count is a cycle: stop.
            let q = if parent == u16::MAX as usize || parent >= locals.len() || depth > locals.len()
            {
                locals[bone]
            } else {
                quat_normalize(quat_mul(
                    world_of(parent, locals, parents, memo, depth + 1),
                    locals[bone],
                ))
            };
            memo[bone] = Some(q);
            q
        }
        let mut memo: Vec<Option<Quat>> = vec![None; count];
        let world: Vec<Mat3> = (0..count)
            .map(|k| mat_from_quat(world_of(k, &locals, &parents, &mut memo, 0)))
            .collect();
        Ok(SkeletonFrames { world })
    }

    /// Skeleton frames keyed by skeleton item index, plus the skeleton each
    /// sim cloth item is paired with.
    fn frame_tables(&self) -> io::Result<(HashMap<usize, SkeletonFrames>, HashMap<usize, usize>)> {
        let document = self.reflect.document;
        let mut frames = HashMap::new();
        for skeleton in &document.skeletons {
            frames.insert(
                skeleton.item_index,
                self.skeleton_frames(skeleton.item_index)?,
            );
        }
        let mut sim_to_skeleton = HashMap::new();
        for cloth in &document.cloth {
            if let Some(skeleton) = document.paired_skeleton(cloth.index) {
                for sim in &cloth.simulations {
                    sim_to_skeleton.insert(sim.item_index, skeleton.item_index);
                }
            }
        }
        Ok((frames, sim_to_skeleton))
    }

    // ---- the document transform ----

    fn apply_document(&mut self, affine: &Affine) -> io::Result<()> {
        let (frames, sim_to_skeleton) = self.frame_tables()?;
        let single_skeleton = (frames.len() == 1).then(|| *frames.keys().next().unwrap_or(&0));
        let mean = affine.mean_scale;
        let items: Vec<_> = self.reflect.document.items.clone();
        for (item_index, item) in items.iter().enumerate() {
            let type_name = self.reflect.type_name(item.type_index);
            let t = item.type_index;
            let base = item.data_offset;
            let element_size = self.reflect.size_of(t);
            match type_name.as_str() {
                "hkaSkeleton" => {
                    let Some(skeleton) = frames.get(&item_index) else {
                        continue;
                    };
                    let parents = self
                        .reflect
                        .u16_array(base + self.member(t, "parentIndices")?);
                    if let Some((at, count, size)) =
                        self.array(base + self.member(t, "referencePose")?)
                    {
                        for k in 0..count {
                            let pose = at + k * size;
                            let parent = parents.get(k as usize).copied().unwrap_or(u16::MAX);
                            if parent == u16::MAX || parent as u32 >= count {
                                self.map_point(pose, affine, "skeleton reference pose")?;
                                if affine.rotates {
                                    let q = self.quat_at(pose + 16);
                                    self.set_quat(
                                        pose + 16,
                                        quat_normalize(quat_mul(affine.quat, q)),
                                        "skeleton reference pose",
                                    )?;
                                }
                            } else {
                                let k_parent = affine.frame_scale(skeleton.frame(parent as usize));
                                self.map_frame_vector(pose, &k_parent, "skeleton reference pose")?;
                            }
                        }
                    }
                }
                "hclSimClothPose" => {
                    if let Some((offset, count, size)) =
                        self.array(base + self.member(t, "positions")?)
                    {
                        for k in 0..count {
                            self.map_point(offset + k * size, affine, "particle rest positions")?;
                        }
                    }
                }
                "hclSimClothData" => {
                    let particles_field = base + self.member(t, "particleDatas")?;
                    if let Some(storage) = self.reflect.referenced(particles_field) {
                        let storage = self.reflect.document.items[storage].clone();
                        let size = self.reflect.size_of(storage.type_index);
                        let radius = self.member(storage.type_index, "radius")?;
                        for k in 0..storage.count {
                            self.mul(
                                storage.data_offset + k * size + radius,
                                mean,
                                "particle radius",
                            )?;
                        }
                    }
                    self.mul(
                        base + self.member(t, "maxParticleRadius")?,
                        mean,
                        "max particle radius",
                    )?;
                    let map = self
                        .reflect
                        .member(t, "collidableTransformMap")
                        .ok_or_else(|| {
                            io::Error::new(ErrorKind::InvalidData, "no collidableTransformMap")
                        })?;
                    let map_base = base + map.offset;
                    let bone_indices = self
                        .reflect
                        .u32_array(map_base + self.member(map.type_index, "transformIndices")?);
                    let skeleton = sim_to_skeleton
                        .get(&item_index)
                        .or(single_skeleton.as_ref())
                        .and_then(|index| frames.get(index));
                    if let Some((offset, count, size)) =
                        self.array(map_base + self.member(map.type_index, "offsets")?)
                    {
                        for k in 0..count {
                            let frame = skeleton.and_then(|frames| {
                                bone_indices
                                    .get(k as usize)
                                    .and_then(|bone| frames.frame(*bone as usize))
                            });
                            let k_bone = affine.frame_scale(frame);
                            self.map_frame_vector(
                                offset + k * size + 48,
                                &k_bone,
                                "collidable transform map offsets",
                            )?;
                        }
                    }
                    let landscape = self
                        .reflect
                        .member(t, "landscapeCollisionData")
                        .ok_or_else(|| {
                            io::Error::new(ErrorKind::InvalidData, "no landscapeCollisionData")
                        })?;
                    self.mul(
                        base + landscape.offset
                            + self.member(landscape.type_index, "landscapeRadius")?,
                        mean,
                        "landscape radius",
                    )?;
                }
                "hclCollidable" => {
                    let transform = base + self.member(t, "transform")?;
                    let rotation = self.mat_at(transform);
                    self.map_point(transform + 48, affine, "collidable translation")?;
                    if affine.rotates {
                        self.set_mat(
                            transform,
                            &mat_mul(&affine.rotation, &rotation),
                            "collidable rotation",
                        )?;
                    }
                    let k_local = affine.frame_scale(Some(&rotation));
                    if let Some(shape_index) =
                        self.reflect.referenced(base + self.member(t, "shape")?)
                    {
                        let shape = self.reflect.document.items[shape_index].clone();
                        let st = shape.type_index;
                        let sb = shape.data_offset;
                        match self.reflect.type_name(st).as_str() {
                            "hclCapsuleShape" => {
                                let start = sb + self.member(st, "start")?;
                                let end = sb + self.member(st, "end")?;
                                self.map_frame_vector(start, &k_local, "capsule end points")?;
                                self.map_frame_vector(end, &k_local, "capsule end points")?;
                                self.mul(sb + self.member(st, "radius")?, mean, "capsule radius")?;
                                let length_squared: f32 = (0..3)
                                    .map(|i| {
                                        (self.f32_at(end + i * 4) - self.f32_at(start + i * 4))
                                            .powi(2)
                                    })
                                    .sum();
                                self.set_f32(
                                    sb + self.member(st, "capLenSqrdInv")?,
                                    1.0 / length_squared.max(1.0e-4),
                                    "capsule inverse squared length",
                                )?;
                            }
                            "hclSphereShape" => {
                                let sphere = sb + self.member(st, "sphere")?;
                                self.map_frame_vector(
                                    sphere,
                                    &k_local,
                                    "sphere centre and radius",
                                )?;
                                self.mul(sphere + 12, mean, "sphere centre and radius")?;
                            }
                            "hclPlaneShape" => {
                                // (n, -d) in collider space: a scaled space
                                // maps the plane through the inverse transpose.
                                let equation = sb + self.member(st, "planeEquation")?;
                                let normal = self.vec3_at(equation);
                                let distance = self.f32_at(equation + 12);
                                let mapped = inverse(&k_local)
                                    .map(|inv| apply(&transpose(&inv), normal))
                                    .unwrap_or(normal);
                                let length = (mapped[0] * mapped[0]
                                    + mapped[1] * mapped[1]
                                    + mapped[2] * mapped[2])
                                    .sqrt();
                                if length > 1.0e-8 && length.is_finite() {
                                    self.set_vec3(
                                        equation,
                                        [
                                            mapped[0] / length,
                                            mapped[1] / length,
                                            mapped[2] / length,
                                        ],
                                        "plane distance",
                                    )?;
                                    self.set_f32(
                                        equation + 12,
                                        distance / length,
                                        "plane distance",
                                    )?;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "hclStandardLinkConstraintSet::Link" | "hclStretchLinkConstraintSet::Link" => {
                    let rest = self.member(t, "restLength")?;
                    for k in 0..item.count {
                        self.mul(base + k * element_size + rest, mean, "link rest lengths")?;
                    }
                }
                "hclBendLinkConstraintSet::Link" => {
                    let (a, b) = (
                        self.member(t, "bendMinLength")?,
                        self.member(t, "stretchMaxLength")?,
                    );
                    for k in 0..item.count {
                        self.mul(base + k * element_size + a, mean, "bend link lengths")?;
                        self.mul(base + k * element_size + b, mean, "bend link lengths")?;
                    }
                }
                "hclBendStiffnessConstraintSet::Link" => {
                    let curvature = self.member(t, "restCurvature")?;
                    for k in 0..item.count {
                        self.mul(
                            base + k * element_size + curvature,
                            1.0 / mean,
                            "rest curvature",
                        )?;
                    }
                }
                "hclBendStiffnessConstraintSet" => {
                    self.mul(
                        base + self.member(t, "maxRestPoseHeightSq")?,
                        mean * mean,
                        "max rest pose height",
                    )?;
                }
                "hclLocalRangeConstraintSet::LocalConstraint"
                | "hclLocalRangeConstraintSet::LocalStiffnessConstraint" => {
                    let radius = self.member(t, "shapeRadius")?;
                    let (max, min) = (
                        self.member(t, "maxNormalDistance")?,
                        self.member(t, "minNormalDistance")?,
                    );
                    for k in 0..item.count {
                        let e = base + k * element_size;
                        self.mul(e + radius, mean, "local range radius")?;
                        for offset in [max, min] {
                            if self.f32_at(e + offset).abs() < 1.0e30 {
                                self.mul(e + offset, mean, "local range normal distance")?;
                            }
                        }
                    }
                }
                "hclObjectSpaceSkinPOperator" => {
                    // Inverse bind matrices B = W⁻¹ follow the moved bones:
                    // W' = [R_M·R | M(t)] so B' = [A·R_Mᵀ | −A·R_Mᵀ·M(t)]
                    // with A the old rotation part and t = −A⁻¹·b.
                    if let Some((offset, count, size)) =
                        self.array(base + self.member(t, "boneFromSkinMeshTransforms")?)
                    {
                        for k in 0..count {
                            let at = offset + k * size;
                            let a = self.mat_at(at);
                            let b = self.vec3_at(at + 48);
                            let bone_translation = match inverse(&a) {
                                Some(inv) => apply(&inv, [-b[0], -b[1], -b[2]]),
                                None => apply(&transpose(&a), [-b[0], -b[1], -b[2]]),
                            };
                            let moved = affine.point(bone_translation);
                            let a_new = if affine.rotates {
                                mat_mul(&a, &transpose(&affine.rotation))
                            } else {
                                a
                            };
                            let b_new = apply(&a_new, [-moved[0], -moved[1], -moved[2]]);
                            if affine.rotates {
                                self.set_mat(at, &a_new, "skin bone transforms")?;
                            }
                            self.set_vec3(at + 48, b_new, "skin bone transforms")?;
                        }
                    }
                    if let Some((offset, count, size)) =
                        self.array(base + self.member(t, "localPs")?)
                    {
                        // hkPackedVector3 blocks: 16 × (i16 x, y, z, u16 scale)
                        for k in 0..count {
                            for v in 0..16u32 {
                                let at = self.reflect.base + (offset + k * size + v * 8) as usize;
                                let Some(slot) = self.raw.get(at..at + 8) else {
                                    continue;
                                };
                                let packed: [u8; 8] = slot.try_into().unwrap_or([0; 8]);
                                let p = unpack_vector3(&packed);
                                let moved = affine.point(p);
                                self.raw[at..at + 8].copy_from_slice(&pack_vector3(&[
                                    moved[0], moved[1], moved[2], 0.0,
                                ]));
                            }
                            self.note("packed skin positions", 16);
                        }
                    }
                    if let Some((offset, count, size)) =
                        self.array(base + self.member(t, "localUnpackedPs")?)
                    {
                        for k in 0..count {
                            for v in 0..(size / 16) {
                                self.map_point(
                                    offset + k * size + v * 16,
                                    affine,
                                    "unpacked skin positions",
                                )?;
                            }
                        }
                    }
                }
                "hclBoneSpaceSkinPOperator" => {
                    // Bone-space vertices: the owning bone of each entry is
                    // not tracked here, so the mean scale stands in.
                    let k_mean = diagonal([mean; 3]);
                    for member in ["localPs", "localUnpackedPs"] {
                        if let Some((offset, count, size)) =
                            self.array(base + self.member(t, member)?)
                        {
                            for k in 0..count {
                                for v in 0..(size / 16) {
                                    self.map_frame_vector(
                                        offset + k * size + v * 16,
                                        &k_mean,
                                        "bone-space skin positions",
                                    )?;
                                }
                            }
                        }
                    }
                }
                "hclSimpleMeshBoneDeformOperator" => {
                    // Triangle-frame offsets of the driven bones.
                    let k_mean = diagonal([mean; 3]);
                    if let Some((offset, count, size)) =
                        self.array(base + self.member(t, "localBoneTransforms")?)
                    {
                        for k in 0..count {
                            self.map_frame_vector(
                                offset + k * size + 48,
                                &k_mean,
                                "mesh-bone local transforms",
                            )?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    // ---- per-bone reference pose edits ----

    fn apply_bone(&mut self, name: &str, spec: &TransformSpec) -> io::Result<()> {
        let rotation = quat_from_euler_xyz(spec.rotation);
        let rotates = spec.rotation != [0.0; 3];
        let items: Vec<_> = self.reflect.document.items.clone();
        let mut hits = 0usize;
        for item in &items {
            if self.reflect.type_name(item.type_index) != "hkaSkeleton" {
                continue;
            }
            let (t, base) = (item.type_index, item.data_offset);
            let Some((bones_at, count, bone_size)) = self.array(base + self.member(t, "bones")?)
            else {
                continue;
            };
            let Some((pose_at, pose_count, pose_size)) =
                self.array(base + self.member(t, "referencePose")?)
            else {
                continue;
            };
            for k in 0..count.min(pose_count) {
                if self.reflect.string_at(bones_at + k * bone_size).as_deref() != Some(name) {
                    continue;
                }
                hits += 1;
                let pose = pose_at + k * pose_size;
                let what = "bone reference pose";
                if spec.location != [0.0; 3] {
                    let t = self.vec3_at(pose);
                    self.set_vec3(
                        pose,
                        [
                            t[0] + spec.location[0],
                            t[1] + spec.location[1],
                            t[2] + spec.location[2],
                        ],
                        what,
                    )?;
                }
                if rotates {
                    let q = self.quat_at(pose + 16);
                    self.set_quat(pose + 16, quat_normalize(quat_mul(rotation, q)), what)?;
                }
                if spec.scale != unit_scale() {
                    let s = self.vec3_at(pose + 32);
                    self.set_vec3(
                        pose + 32,
                        [
                            s[0] * spec.scale[0],
                            s[1] * spec.scale[1],
                            s[2] * spec.scale[2],
                        ],
                        what,
                    )?;
                }
            }
        }
        if hits == 0 {
            return Err(invalid_input(format!(
                "bone '{name}' is not in any skeleton of this BPHCL"
            )));
        }
        Ok(())
    }
}

/// Inverse of [`pack_vector3`]: three int16 components times the
/// power-of-two scale whose upper float half is the fourth word.
pub(super) fn unpack_vector3(bytes: &[u8; 8]) -> Vec3 {
    let word = u16::from_le_bytes([bytes[6], bytes[7]]);
    let scale = f32::from_bits((word as u32) << 16) * 65536.0;
    let component = |i: usize| i16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]) as f32 * scale;
    [component(0), component(1), component(2)]
}

fn invalid_input(message: String) -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus_document() -> Option<BphclDocument> {
        let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/_bphcl");
        let path = std::fs::read_dir(&directory).ok().and_then(|entries| {
            let mut paths: Vec<_> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|e| e == "bphcl"))
                .collect();
            paths.sort();
            paths.into_iter().next()
        })?;
        BphclDocument::parse(&std::fs::read(path).ok()?).ok()
    }

    fn sheet(document: Option<TransformSpec>, bones: &[(&str, TransformSpec)]) -> TransformSheet {
        TransformSheet {
            document,
            bones: bones
                .iter()
                .map(|(name, spec)| ((*name).to_owned(), spec.clone()))
                .collect(),
        }
    }

    fn close(a: [f32; 3], b: [f32; 3], tolerance: f32) -> bool {
        a.iter().zip(&b).all(|(x, y)| (x - y).abs() <= tolerance)
    }

    #[test]
    fn packed_vectors_round_trip() {
        for vector in [
            [0.0, 0.0, 0.0, 0.0],
            [1.5, -2.25, 0.125, 0.0],
            [123.4, -0.001, 9.0, 0.0],
        ] {
            let packed = pack_vector3(&vector);
            let unpacked = unpack_vector3(&packed);
            let magnitude = vector[..3].iter().fold(0.0f32, |acc, v| acc.max(v.abs()));
            assert!(
                close(
                    unpacked,
                    [vector[0], vector[1], vector[2]],
                    magnitude / 16000.0 + 1e-6
                ),
                "{vector:?} -> {unpacked:?}"
            );
        }
    }

    #[test]
    fn euler_rotation_follows_xyz_order() {
        let m = mat_from_quat(quat_from_euler_xyz([0.0, 0.0, 90.0]));
        assert!(close(apply(&m, [1.0, 0.0, 0.0]), [0.0, 1.0, 0.0], 1e-5));
        let m = mat_from_quat(quat_from_euler_xyz([90.0, 0.0, 90.0]));
        // X first: (0,1,0) -> (0,0,1); then Z leaves the Z axis alone.
        assert!(close(apply(&m, [0.0, 1.0, 0.0]), [0.0, 0.0, 1.0], 1e-5));
    }

    #[test]
    fn reset_use_flags_only_touches_enabled_lines() {
        let text = "document:\n  use: true # keep me\n  scale: [1, 1, 1]\nbones:\n  Root:\n    use: TRUE\n  Other:\n    use: false\n# use: true in a comment\n";
        let reset = reset_use_flags(text);
        assert_eq!(
            reset,
            "document:\n  use: false # keep me\n  scale: [1, 1, 1]\nbones:\n  Root:\n    use: false\n  Other:\n    use: false\n# use: true in a comment\n"
        );
        assert_eq!(reset_use_flags("use: true"), "use: false");
    }

    #[test]
    fn sheet_parsing_validates_and_tolerates_empty_files() {
        assert!(!parse_transform_yaml("").unwrap().has_enabled());
        assert!(!parse_transform_yaml("# only a comment\n")
            .unwrap()
            .has_enabled());
        let sheet = parse_transform_yaml(
            "document:\n  use: true\n  scale: [2, 2, 2]\nbones:\n  Root:\n    location: [0, 1, 0]\n",
        )
        .unwrap();
        assert!(sheet.has_enabled());
        assert_eq!(sheet.document.as_ref().unwrap().scale, [2.0; 3]);
        assert_eq!(sheet.bones["Root"].location, [0.0, 1.0, 0.0]);
        assert!(!sheet.bones["Root"].enabled);
        assert!(parse_transform_yaml("document:\n  use: true\n  scale: [9, 1, 1]\n").is_err());
        assert!(parse_transform_yaml("document:\n  scal: [1, 1, 1]\n").is_err());
        assert!(parse_transform_yaml("bones:\n  Root:\n    rotation: [.nan, 0, 0]\n").is_err());
    }

    #[test]
    fn default_sheet_parses_with_nothing_enabled() {
        let Some(document) = corpus_document() else {
            return;
        };
        let text = default_transform_yaml(&document);
        let sheet = parse_transform_yaml(&text).unwrap();
        assert!(!sheet.has_enabled());
        assert_eq!(sheet.bones.len(), 1);
        let (bytes, report) = document.apply_transform_sheet(&sheet).unwrap();
        assert_eq!(bytes, document.raw);
        assert_eq!(report.touched(), 0);
    }

    #[test]
    fn rotation_and_location_move_particles_and_root_bones() {
        let Some(document) = corpus_document() else {
            return;
        };
        let spec = TransformSpec {
            enabled: true,
            rotation: [0.0, 0.0, 90.0],
            location: [1.0, 2.0, 3.0],
            ..TransformSpec::default()
        };
        let (bytes, report) = document
            .apply_transform_sheet(&sheet(Some(spec), &[]))
            .unwrap();
        assert!(report.document_applied);
        let moved = BphclDocument::parse(&bytes).unwrap();
        let before = &document.cloth[0].simulations[0].particles;
        let after = &moved.cloth[0].simulations[0].particles;
        assert_eq!(before.len(), after.len());
        for (a, b) in before.iter().zip(after) {
            let expected = [-a.position.y + 1.0, a.position.x + 2.0, a.position.z + 3.0];
            assert!(
                close([b.position.x, b.position.y, b.position.z], expected, 1e-3),
                "{:?} -> {:?}, expected {expected:?}",
                a.position,
                b.position
            );
            assert_eq!(a.radius, b.radius);
        }
        for (before, after) in document.skeletons.iter().zip(&moved.skeletons) {
            for (a, b) in before.bones.iter().zip(&after.bones) {
                if a.parent_index.is_none() {
                    let expected = [
                        -a.translation.y + 1.0,
                        a.translation.x + 2.0,
                        a.translation.z + 3.0,
                    ];
                    assert!(close(
                        [b.translation.x, b.translation.y, b.translation.z],
                        expected,
                        1e-3
                    ));
                } else {
                    assert!(close(
                        [a.translation.x, a.translation.y, a.translation.z],
                        [b.translation.x, b.translation.y, b.translation.z],
                        1e-4
                    ));
                    assert_eq!(a.rotation.x, b.rotation.x);
                }
            }
        }
    }

    #[test]
    fn uniform_scale_is_reversible() {
        let Some(document) = corpus_document() else {
            return;
        };
        let (bytes, _) = document
            .apply_transform_sheet(&sheet(Some(TransformSpec::uniform_scale(2.0)), &[]))
            .unwrap();
        let doubled = BphclDocument::parse(&bytes).unwrap();
        let (bytes, _) = doubled
            .apply_transform_sheet(&sheet(Some(TransformSpec::uniform_scale(0.5)), &[]))
            .unwrap();
        let restored = BphclDocument::parse(&bytes).unwrap();
        for (a, b) in document.collidables.iter().zip(&restored.collidables) {
            assert!(close(
                [a.translation.x, a.translation.y, a.translation.z],
                [b.translation.x, b.translation.y, b.translation.z],
                1e-4
            ));
        }
        let before = &document.cloth[0].simulations[0].particles;
        let after = &restored.cloth[0].simulations[0].particles;
        for (a, b) in before.iter().zip(after) {
            assert!(close(
                [a.position.x, a.position.y, a.position.z],
                [b.position.x, b.position.y, b.position.z],
                1e-4
            ));
            assert!((a.radius - b.radius).abs() < 1e-5);
        }
    }

    #[test]
    fn bone_blocks_edit_only_the_named_pose() {
        let Some(document) = corpus_document() else {
            return;
        };
        let Some(bone) = document
            .skeletons
            .first()
            .and_then(|skeleton| skeleton.bones.first())
        else {
            return;
        };
        let name = bone.name.clone();
        let spec = TransformSpec {
            enabled: true,
            location: [0.5, 0.0, -0.25],
            rotation: [0.0, 90.0, 0.0],
            scale: [2.0, 1.0, 1.0],
        };
        let (bytes, report) = document
            .apply_transform_sheet(&sheet(None, &[(name.as_str(), spec)]))
            .unwrap();
        assert_eq!(report.bones_applied, vec![name.clone()]);
        assert!(!report.document_applied);
        let edited = BphclDocument::parse(&bytes).unwrap();
        let (before, after) = (
            &document.skeletons[0].bones[0],
            &edited.skeletons[0].bones[0],
        );
        assert!(close(
            [
                after.translation.x,
                after.translation.y,
                after.translation.z
            ],
            [
                before.translation.x + 0.5,
                before.translation.y,
                before.translation.z - 0.25
            ],
            1e-5
        ));
        assert_ne!(before.rotation.y, after.rotation.y);
        for (a, b) in document.skeletons[0]
            .bones
            .iter()
            .zip(&edited.skeletons[0].bones)
            .skip(1)
            .filter(|(a, _)| a.name != name)
        {
            assert_eq!(a.translation.x, b.translation.x);
            assert_eq!(a.rotation.w, b.rotation.w);
        }
        assert_eq!(
            document.cloth[0].simulations[0].particles[0].position.x,
            edited.cloth[0].simulations[0].particles[0].position.x
        );
        let missing = sheet(
            None,
            &[(
                "NoSuchBone",
                TransformSpec {
                    enabled: true,
                    ..TransformSpec::default()
                },
            )],
        );
        assert!(document.apply_transform_sheet(&missing).is_err());
    }
}
