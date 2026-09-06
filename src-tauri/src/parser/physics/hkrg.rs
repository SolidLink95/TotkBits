//! BotW Havok ragdolls (`.hkrg`): a classic 2014 packfile carrying the model
//! skeleton, the ragdoll skeleton, the ragdoll instance that binds bones to
//! rigid bodies, the physics system with those bodies and their constraints,
//! and the skeleton mappers between the two skeletons.
//!
//! The packfile stores no class layouts, so the reader combines three
//! sources: the fixup tables (every pointer field is listed, so names, shapes,
//! constraint data and entity links are found by following them), formula
//! offsets for the array-of-pointer classes, and self-locating scans for the
//! motion transform and constraint atoms, each validated against the data it
//! claims to describe. Edits write values back in place and never move data.

use crate::parser::{
    binary::{BinaryReader, Endian},
    hkcl::{HkclDocument, HkclHeader, HkclLeaf, HkclSection, ObjectKey},
};
use serde::Serialize;
use std::{
    collections::HashMap,
    f32::consts::PI,
    io::{self, ErrorKind},
};

pub type Vec3 = [f32; 3];
pub type Vec4 = [f32; 4];
/// Row-major 4x4 with row-vector convention: `v' = v * M`, translation in row 3.
pub type Mat4 = [[f32; 4]; 4];

const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

// hkpConstraintAtom::AtomType values Havok writes into each atom's header.
const ATOM_SET_LOCAL_TRANSFORMS: u16 = 2;
const ATOM_BALL_SOCKET: u16 = 5;
const ATOM_2D_ANG: u16 = 12;
const ATOM_ANG_LIMIT: u16 = 14;
const ATOM_TWIST_LIMIT: u16 = 15;
const ATOM_CONE_LIMIT: u16 = 16;
const ATOM_ANG_FRICTION: u16 = 17;
const ATOM_ANG_MOTOR: u16 = 18;
const ATOM_RAGDOLL_MOTOR: u16 = 19;
const ATOM_SETUP_STABILIZATION: u16 = 23;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QsTransform {
    pub translation: Vec4,
    pub rotation: Vec4,
    pub scale: Vec4,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HkrgBone {
    pub index: usize,
    pub name: String,
    pub parent_index: i16,
    pub reference_pose: QsTransform,
    #[serde(skip)]
    name_field: Option<ObjectKey>,
    #[serde(skip)]
    pose_field: Option<ObjectKey>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HkrgSkeleton {
    pub key: ObjectKey,
    pub name: String,
    pub bones: Vec<HkrgBone>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RagdollInstance {
    pub key: ObjectKey,
    pub rigid_body_keys: Vec<ObjectKey>,
    pub constraint_keys: Vec<ObjectKey>,
    pub bone_to_rigid_body: Vec<i32>,
    pub skeleton_key: Option<ObjectKey>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicsSystem {
    pub key: ObjectKey,
    pub name: Option<String>,
    pub rigid_body_keys: Vec<ObjectKey>,
    pub constraint_keys: Vec<ObjectKey>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigidBodyShape {
    pub key: ObjectKey,
    pub class_name: String,
    pub vertex_a: Vec4,
    pub vertex_b: Vec4,
    pub radius: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigidBody {
    pub index: usize,
    pub key: ObjectKey,
    pub name: String,
    pub bone_index: i32,
    pub shape: Option<RigidBodyShape>,
    /// Raw hkTransform rows as stored; the fourth components are Havok data,
    /// not projection terms, and are preserved on write.
    pub transform: Mat4,
    /// hkSweptTransform: centre of mass 0 and 1, rotation 0 and 1, and the
    /// local centre of mass.
    pub swept_transform: [Vec4; 5],
    #[serde(skip)]
    name_field: Option<ObjectKey>,
    #[serde(skip)]
    transform_offset: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AngularLimit {
    pub min_angle: f32,
    pub max_angle: f32,
    pub tau_factor: f32,
    pub damping_factor: f32,
    #[serde(skip)]
    field: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RagdollMotor {
    pub enabled: bool,
    /// hkMatrix3 target rotation, three rows with their padding component.
    pub target_b_rca: [Vec4; 3],
    #[serde(skip)]
    field: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintAtoms {
    pub transform_a: Mat4,
    pub transform_b: Mat4,
    pub motor: Option<RagdollMotor>,
    pub angular_friction: Option<f32>,
    pub twist_limit: Option<AngularLimit>,
    pub cone_limit: Option<AngularLimit>,
    pub planes_limit: Option<AngularLimit>,
    pub hinge_limit: Option<AngularLimit>,
    #[serde(skip)]
    transforms_field: usize,
    #[serde(skip)]
    friction_field: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RagdollConstraint {
    pub index: usize,
    pub key: ObjectKey,
    pub name: String,
    pub data_key: Option<ObjectKey>,
    pub class_name: String,
    pub constraint_type: String,
    pub body_a: i32,
    pub body_b: i32,
    pub atoms: Option<ConstraintAtoms>,
    #[serde(skip)]
    name_field: Option<ObjectKey>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleMapping {
    pub bone_a: i16,
    pub bone_b: i16,
    pub a_from_b: QsTransform,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainMapping {
    pub start_bone_a: i16,
    pub start_bone_b: i16,
    pub end_bone_a: i16,
    pub end_bone_b: i16,
    pub start_a_from_b: QsTransform,
    pub end_a_from_b: QsTransform,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkeletonMapper {
    pub key: ObjectKey,
    pub skeleton_a: Option<ObjectKey>,
    pub skeleton_b: Option<ObjectKey>,
    pub simple_mappings: Vec<SimpleMapping>,
    pub chain_mappings: Vec<ChainMapping>,
}

#[derive(Clone, Debug)]
pub struct HkrgDocument {
    pub raw: Vec<u8>,
    pub header: HkclHeader,
    pub sections: Vec<HkclSection>,
    pub class_names: HashMap<ObjectKey, String>,
    pub model_skeleton: Option<HkrgSkeleton>,
    pub ragdoll_skeleton: Option<HkrgSkeleton>,
    pub ragdoll: Option<RagdollInstance>,
    pub physics_systems: Vec<PhysicsSystem>,
    pub rigid_bodies: Vec<RigidBody>,
    pub constraints: Vec<RagdollConstraint>,
    pub mappers: Vec<SkeletonMapper>,
}

// ---- Rows the editors exchange ------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoneRow {
    pub index: usize,
    pub name: String,
    pub parent_index: i16,
    pub translation: Vec3,
    pub rotation: Vec4,
    pub scale: Vec3,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigidBodyRow {
    pub index: usize,
    pub name: String,
    pub shape_type: String,
    pub bone_index: i32,
    pub bone_name: String,
    pub start: Vec3,
    pub end: Vec3,
    pub radius: f32,
    pub transform: Mat4,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintRow {
    pub index: usize,
    pub name: String,
    pub constraint_type: String,
    pub body_a: i32,
    pub body_b: i32,
    pub body_a_name: String,
    pub body_b_name: String,
    pub twist: Option<AngularLimit>,
    pub cone: Option<AngularLimit>,
    pub planes: Option<AngularLimit>,
    pub hinge: Option<AngularLimit>,
    pub angular_friction: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkeletonKind {
    Model,
    Ragdoll,
}

/// Which limits to write; `None` leaves a value untouched.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AngularLimitEdit {
    pub min_angle: Option<f32>,
    pub max_angle: Option<f32>,
    pub tau_factor: Option<f32>,
    pub damping_factor: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConstraintEdit {
    pub index: usize,
    pub name: Option<String>,
    pub twist: AngularLimitEdit,
    pub cone: AngularLimitEdit,
    pub planes: AngularLimitEdit,
    pub hinge: AngularLimitEdit,
    pub angular_friction: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintPoseDiagnostic {
    pub index: usize,
    pub name: String,
    pub body_a: i32,
    pub body_b: i32,
    pub bone_a: i32,
    pub bone_b: i32,
    pub frame_position_gap: f32,
    pub frame_rotation_gap_degrees: f32,
    pub skeleton_relative_rotation_degrees: f32,
    pub motor_enabled: bool,
    pub motor_target_rotation_degrees: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HingeReferenceDiagnostic {
    pub constraint_index: usize,
    pub constraint_name: String,
    pub reference_angle_degrees: f32,
    pub minimum_angle_degrees: f32,
    pub maximum_angle_degrees: f32,
    pub off_axis_degrees: f32,
    pub is_inside_limit: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MotorTargetDiagnostic {
    pub constraint_index: usize,
    pub constraint_name: String,
    pub frame_b_error_degrees: f32,
    pub frame_a_error_degrees: f32,
    pub b_times_inverse_a_error_degrees: f32,
    pub inverse_b_times_a_error_degrees: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapperPoseDiagnostic {
    pub mapper_index: usize,
    pub mapping_index: usize,
    pub bone_a: i16,
    pub bone_b: i16,
    pub b_to_a_position_error: f32,
    pub b_to_a_rotation_error_degrees: f32,
    pub a_to_b_position_error: f32,
    pub a_to_b_rotation_error_degrees: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HkrgSummary {
    pub format: &'static str,
    pub contents_version: String,
    pub pointer_size: u8,
    pub big_endian: bool,
    pub physics_systems: usize,
    pub rigid_bodies: usize,
    pub constraints: usize,
    pub model_bones: usize,
    pub ragdoll_bones: usize,
    pub ragdoll_skeleton: String,
    pub mappers: usize,
}

/// Field-offset arithmetic shared by every class: `hkReferencedObject` and
/// `hkArray` sizes follow the pointer width.
#[derive(Clone, Copy)]
struct Layout {
    pointer: usize,
    referenced: usize,
    array: usize,
}

impl Layout {
    fn new(pointer_size: u8) -> Self {
        let pointer = pointer_size as usize;
        Self {
            pointer,
            referenced: if pointer == 8 { 16 } else { 8 },
            array: pointer + 8,
        }
    }
}

struct Reader<'a> {
    raw: &'a [u8],
    sections: &'a [HkclSection],
    endian: Endian,
    layout: Layout,
    pointers: HashMap<(usize, u32), ObjectKey>,
    object_starts: HashMap<usize, Vec<u32>>,
}

impl<'a> Reader<'a> {
    fn new(base: &'a HkclDocument) -> Self {
        let mut pointers = HashMap::new();
        for fixup in &base.local_fixups {
            pointers.insert(
                (fixup.section_index, fixup.source_offset),
                ObjectKey {
                    section_index: fixup.section_index,
                    offset: fixup.destination_offset,
                },
            );
        }
        for fixup in &base.global_fixups {
            pointers.insert(
                (fixup.section_index, fixup.source_offset),
                ObjectKey {
                    section_index: fixup.destination_section_index,
                    offset: fixup.destination_offset,
                },
            );
        }
        let mut object_starts: HashMap<usize, Vec<u32>> = HashMap::new();
        for item in &base.items {
            object_starts
                .entry(item.data_section_index)
                .or_default()
                .push(item.data_offset);
        }
        for starts in object_starts.values_mut() {
            starts.sort_unstable();
            starts.dedup();
        }
        Self {
            raw: &base.raw,
            sections: &base.sections,
            endian: base.header.layout.endian,
            layout: Layout::new(base.header.layout.pointer_size),
            pointers,
            object_starts,
        }
    }

    fn absolute(&self, key: ObjectKey, field: usize) -> io::Result<usize> {
        let section = self
            .sections
            .get(key.section_index)
            .ok_or_else(|| invalid("HKRG object section is out of range"))?;
        let offset = section
            .absolute_data_start
            .checked_add(key.offset as usize)
            .and_then(|value| value.checked_add(field))
            .ok_or_else(|| invalid("HKRG object offset overflows"))?;
        Ok(offset)
    }

    fn bytes(&self, key: ObjectKey, field: usize, len: usize) -> io::Result<&'a [u8]> {
        let start = self.absolute(key, field)?;
        let section = &self.sections[key.section_index];
        let end = start
            .checked_add(len)
            .ok_or_else(|| invalid("HKRG object read overflows"))?;
        if end > section.local_fixups.start {
            return Err(invalid("HKRG object read exceeds section DATA"));
        }
        self.raw
            .get(start..end)
            .ok_or_else(|| invalid("HKRG object read exceeds input"))
    }

    fn u8(&self, key: ObjectKey, field: usize) -> io::Result<u8> {
        Ok(self.bytes(key, field, 1)?[0])
    }
    fn u16(&self, key: ObjectKey, field: usize) -> io::Result<u16> {
        BinaryReader::with_endian(self.bytes(key, field, 2)?, self.endian).read_u16_at(0)
    }
    fn i16(&self, key: ObjectKey, field: usize) -> io::Result<i16> {
        Ok(self.u16(key, field)? as i16)
    }
    fn u32(&self, key: ObjectKey, field: usize) -> io::Result<u32> {
        BinaryReader::with_endian(self.bytes(key, field, 4)?, self.endian).read_u32_at(0)
    }
    fn i32(&self, key: ObjectKey, field: usize) -> io::Result<i32> {
        Ok(self.u32(key, field)? as i32)
    }
    fn f32(&self, key: ObjectKey, field: usize) -> io::Result<f32> {
        Ok(f32::from_bits(self.u32(key, field)?))
    }
    fn vec4(&self, key: ObjectKey, field: usize) -> io::Result<Vec4> {
        Ok([
            self.f32(key, field)?,
            self.f32(key, field + 4)?,
            self.f32(key, field + 8)?,
            self.f32(key, field + 12)?,
        ])
    }
    fn mat4(&self, key: ObjectKey, field: usize) -> io::Result<Mat4> {
        Ok([
            self.vec4(key, field)?,
            self.vec4(key, field + 16)?,
            self.vec4(key, field + 32)?,
            self.vec4(key, field + 48)?,
        ])
    }
    fn qs_transform(&self, key: ObjectKey, field: usize) -> io::Result<QsTransform> {
        Ok(QsTransform {
            translation: self.vec4(key, field)?,
            rotation: self.vec4(key, field + 16)?,
            scale: self.vec4(key, field + 32)?,
        })
    }
    fn pointer(&self, key: ObjectKey, field: usize) -> Option<ObjectKey> {
        let source = key.offset.checked_add(field as u32)?;
        self.pointers.get(&(key.section_index, source)).copied()
    }
    fn string_at(&self, target: ObjectKey) -> io::Result<String> {
        let section = self
            .sections
            .get(target.section_index)
            .ok_or_else(|| invalid("HKRG string pointer names a missing section"))?;
        let start = section.absolute_data_start + target.offset as usize;
        let bytes = self
            .raw
            .get(start..section.local_fixups.start)
            .ok_or_else(|| invalid("HKRG string pointer exceeds section"))?;
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }
    fn string(&self, key: ObjectKey, field: usize) -> io::Result<Option<String>> {
        self.pointer(key, field)
            .map(|target| self.string_at(target))
            .transpose()
    }
    fn array(&self, key: ObjectKey, field: usize) -> io::Result<(Option<ObjectKey>, usize)> {
        let count = self.u32(key, field + self.layout.pointer)? as usize;
        if count > 1_000_000 {
            return Err(invalid("HKRG array count is unreasonable"));
        }
        Ok((self.pointer(key, field), count))
    }
    fn pointer_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<ObjectKey>> {
        let (storage, count) = self.array(key, field)?;
        let Some(storage) = storage else {
            return Ok(Vec::new());
        };
        Ok((0..count)
            .filter_map(|index| self.pointer(storage, index * self.layout.pointer))
            .collect())
    }
    fn i32_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<i32>> {
        let (storage, count) = self.array(key, field)?;
        let Some(storage) = storage else {
            return Ok(Vec::new());
        };
        (0..count)
            .map(|index| self.i32(storage, index * 4))
            .collect()
    }
    fn i16_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<i16>> {
        let (storage, count) = self.array(key, field)?;
        let Some(storage) = storage else {
            return Ok(Vec::new());
        };
        (0..count)
            .map(|index| self.i16(storage, index * 2))
            .collect()
    }
    /// Size of the object at `key`: up to the next object in its section.
    fn object_size(&self, key: ObjectKey) -> usize {
        let Some(section) = self.sections.get(key.section_index) else {
            return 0;
        };
        let data_end = section
            .local_fixups
            .start
            .saturating_sub(section.absolute_data_start) as u32;
        let next = self
            .object_starts
            .get(&key.section_index)
            .and_then(|starts| starts.iter().find(|start| **start > key.offset).copied())
            .unwrap_or(data_end);
        next.saturating_sub(key.offset) as usize
    }
    fn is_object_start(&self, key: ObjectKey) -> bool {
        self.object_starts
            .get(&key.section_index)
            .is_some_and(|starts| starts.binary_search(&key.offset).is_ok())
    }
    /// Pointer fields inside the object, in field order.
    fn pointer_fields(&self, key: ObjectKey) -> Vec<(usize, ObjectKey)> {
        let size = self.object_size(key) as u32;
        let mut fields: Vec<(usize, ObjectKey)> = self
            .pointers
            .iter()
            .filter(|((section, source), _)| {
                *section == key.section_index
                    && *source >= key.offset
                    && *source < key.offset + size
            })
            .map(|((_, source), target)| ((*source - key.offset) as usize, *target))
            .collect();
        fields.sort_by_key(|(field, _)| *field);
        fields
    }

    fn skeleton(&self, key: ObjectKey) -> io::Result<HkrgSkeleton> {
        let base = self.layout.referenced;
        let parents_field = base + self.layout.pointer;
        let bones_field = parents_field + self.layout.array;
        let pose_field = bones_field + self.layout.array;
        let parents = self.i16_array(key, parents_field)?;
        let (bone_storage, bone_count) = self.array(key, bones_field)?;
        let (pose_storage, pose_count) = self.array(key, pose_field)?;
        let bone_stride = self.layout.pointer * 2;
        let mut bones = Vec::with_capacity(bone_count);
        if let Some(storage) = bone_storage {
            for index in 0..bone_count {
                let bone = ObjectKey {
                    section_index: storage.section_index,
                    offset: storage.offset + (index * bone_stride) as u32,
                };
                let pose_key =
                    pose_storage
                        .filter(|_| index < pose_count)
                        .map(|storage| ObjectKey {
                            section_index: storage.section_index,
                            offset: storage.offset + (index * 48) as u32,
                        });
                let pose = match pose_key {
                    Some(pose_key) => self.qs_transform(pose_key, 0)?,
                    None => QsTransform {
                        translation: [0.0; 4],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                        scale: [1.0; 4],
                    },
                };
                bones.push(HkrgBone {
                    index,
                    name: self.string(bone, 0)?.unwrap_or_default(),
                    parent_index: parents.get(index).copied().unwrap_or(-1),
                    reference_pose: pose,
                    name_field: self.pointer(bone, 0),
                    pose_field: pose_key,
                });
            }
        }
        Ok(HkrgSkeleton {
            key,
            name: self.string(key, base)?.unwrap_or_default(),
            bones,
        })
    }

    fn ragdoll_instance(&self, key: ObjectKey) -> io::Result<RagdollInstance> {
        let base = self.layout.referenced;
        Ok(RagdollInstance {
            key,
            rigid_body_keys: self.pointer_array(key, base)?,
            constraint_keys: self.pointer_array(key, base + self.layout.array)?,
            bone_to_rigid_body: self.i32_array(key, base + self.layout.array * 2)?,
            skeleton_key: self.pointer(key, base + self.layout.array * 3),
        })
    }

    fn physics_system(&self, key: ObjectKey) -> io::Result<PhysicsSystem> {
        let base = self.layout.referenced;
        Ok(PhysicsSystem {
            key,
            name: self.string(key, base + self.layout.array * 4)?,
            rigid_body_keys: self.pointer_array(key, base)?,
            constraint_keys: self.pointer_array(key, base + self.layout.array)?,
        })
    }

    fn rigid_body(
        &self,
        index: usize,
        key: ObjectKey,
        classes: &HashMap<ObjectKey, String>,
        bone_index: i32,
    ) -> io::Result<RigidBody> {
        let mut name = None;
        let mut name_field = None;
        let mut shape = None;
        for (field, target) in self.pointer_fields(key) {
            if let Some(class) = classes.get(&target) {
                if class.contains("Shape") && shape.is_none() {
                    shape = Some(self.shape(target, class)?);
                }
            } else if name.is_none() && !self.is_object_start(target) {
                let text = self.string_at(target)?;
                if !text.is_empty() && text.chars().all(|character| !character.is_control()) {
                    name = Some(text);
                    name_field = Some((key, field));
                }
            }
        }
        let transform_offset = self.locate_body_transform(key)?;
        let swept = transform_offset + 64;
        let mut swept_transform = [[0.0; 4]; 5];
        for (row, value) in swept_transform.iter_mut().enumerate() {
            *value = self.vec4(key, swept + row * 16)?;
        }
        Ok(RigidBody {
            index,
            key,
            name: name.unwrap_or_else(|| format!("Rigid body {index}")),
            bone_index,
            shape,
            transform: self.mat4(key, transform_offset)?,
            swept_transform,
            name_field: name_field.map(|(key, field)| ObjectKey {
                section_index: key.section_index,
                offset: key.offset + field as u32,
            }),
            transform_offset,
        })
    }

    fn shape(&self, key: ObjectKey, class: &str) -> io::Result<RigidBodyShape> {
        // hkpShapeBase packs four type bytes after the referenced object,
        // hkpShape adds a pointer-sized userData, hkpConvexShape the radius,
        // and hkpCapsuleShape two 16-aligned vertices whose w is the radius.
        let radius_field =
            align(self.layout.referenced + 4, self.layout.pointer) + self.layout.pointer;
        let vertex_field = align(radius_field + 4, 16);
        let radius = self.f32(key, radius_field)?;
        let (vertex_a, vertex_b) =
            if class == "hkpCapsuleShape" && self.object_size(key) >= vertex_field + 32 {
                (
                    self.vec4(key, vertex_field)?,
                    self.vec4(key, vertex_field + 16)?,
                )
            } else {
                ([0.0, 0.0, 0.0, radius], [0.0, 0.0, 0.0, radius])
            };
        Ok(RigidBodyShape {
            key,
            class_name: class.to_owned(),
            vertex_a,
            vertex_b,
            radius,
        })
    }

    /// hkpMotionState::transform sits inside the body's inline motion. The
    /// formula offset is tried first and confirmed by the data: three
    /// orthonormal basis rows followed by a swept transform whose two
    /// rotations are unit quaternions.
    fn locate_body_transform(&self, key: ObjectKey) -> io::Result<usize> {
        let size = self.object_size(key);
        let formula = if self.layout.pointer == 4 { 240 } else { 368 };
        let candidates = std::iter::once(formula)
            .chain((self.layout.referenced..size).step_by(16))
            .filter(|offset| offset + 144 <= size);
        for offset in candidates {
            let Ok(matrix) = self.mat4(key, offset) else {
                continue;
            };
            if !rows_orthonormal(&matrix) {
                continue;
            }
            let Ok(rotation0) = self.vec4(key, offset + 64 + 32) else {
                continue;
            };
            let Ok(rotation1) = self.vec4(key, offset + 64 + 48) else {
                continue;
            };
            if is_unit(rotation0) && is_unit(rotation1) {
                return Ok(offset);
            }
        }
        Err(invalid(&format!(
            "HKRG rigid body at {:#x} has no recognisable motion transform",
            key.offset
        )))
    }

    fn constraint(
        &self,
        index: usize,
        key: ObjectKey,
        classes: &HashMap<ObjectKey, String>,
        bodies: &[RigidBody],
    ) -> io::Result<RagdollConstraint> {
        let mut data_key = None;
        let mut entities = Vec::new();
        let mut name = None;
        let mut name_field = None;
        for (field, target) in self.pointer_fields(key) {
            match classes.get(&target) {
                Some(class) if class.contains("ConstraintData") => {
                    if data_key.is_none() {
                        data_key = Some(target);
                    }
                }
                Some(class) if class == "hkpRigidBody" => entities.push(target),
                Some(_) => {}
                None if name.is_none() && !self.is_object_start(target) => {
                    name = Some(self.string_at(target)?);
                    name_field = Some(ObjectKey {
                        section_index: key.section_index,
                        offset: key.offset + field as u32,
                    });
                }
                None => {}
            }
        }
        let body_index = |target: Option<&ObjectKey>| -> i32 {
            target
                .and_then(|target| bodies.iter().position(|body| body.key == *target))
                .map_or(-1, |index| index as i32)
        };
        let class_name = data_key
            .and_then(|key| classes.get(&key).cloned())
            .unwrap_or_default();
        let constraint_type = match class_name.as_str() {
            "hkpRagdollConstraintData" => "Ragdoll".to_owned(),
            "hkpLimitedHingeConstraintData" => "Limited hinge".to_owned(),
            other => other.to_owned(),
        };
        let atoms = match data_key {
            Some(data)
                if matches!(
                    class_name.as_str(),
                    "hkpRagdollConstraintData" | "hkpLimitedHingeConstraintData"
                ) =>
            {
                self.constraint_atoms(data, class_name == "hkpRagdollConstraintData")?
            }
            _ => None,
        };
        Ok(RagdollConstraint {
            index,
            key,
            name: name.unwrap_or_else(|| format!("Constraint {index}")),
            data_key,
            class_name,
            constraint_type,
            body_a: body_index(entities.first()),
            body_b: body_index(entities.get(1)),
            atoms,
            name_field,
        })
    }

    /// Atoms self-identify through their type word, so each one is located by
    /// scanning forward from the previous atom instead of trusting a fixed
    /// layout. The transforms atom anchors the search because its two frames
    /// are verifiable.
    fn constraint_atoms(
        &self,
        data: ObjectKey,
        ragdoll: bool,
    ) -> io::Result<Option<ConstraintAtoms>> {
        let size = self.object_size(data);
        let mut transforms = None;
        for offset in (self.layout.referenced..size.saturating_sub(144)).step_by(4) {
            if self.u16(data, offset)? != ATOM_SET_LOCAL_TRANSFORMS {
                continue;
            }
            let (Ok(a), Ok(b)) = (self.mat4(data, offset + 16), self.mat4(data, offset + 80))
            else {
                continue;
            };
            if rows_orthonormal(&a) && rows_orthonormal(&b) {
                transforms = Some(offset);
                break;
            }
        }
        let Some(transforms_field) = transforms else {
            return Ok(None);
        };
        let mut cursor = transforms_field + 144;
        let mut next = |kind: u16, nominal: usize| -> io::Result<Option<usize>> {
            let limit = (cursor + 48).min(size);
            let mut offset = cursor;
            while offset + 2 <= limit {
                if self.u16(data, offset)? == kind {
                    cursor = offset + nominal;
                    return Ok(Some(offset));
                }
                offset += 4;
            }
            Ok(None)
        };
        let _setup = next(ATOM_SETUP_STABILIZATION, 16)?;
        let mut atoms = ConstraintAtoms {
            transform_a: self.mat4(data, transforms_field + 16)?,
            transform_b: self.mat4(data, transforms_field + 80)?,
            motor: None,
            angular_friction: None,
            twist_limit: None,
            cone_limit: None,
            planes_limit: None,
            hinge_limit: None,
            transforms_field,
            friction_field: None,
        };
        if ragdoll {
            if let Some(field) = next(
                ATOM_RAGDOLL_MOTOR,
                if self.layout.pointer == 4 { 80 } else { 96 },
            )? {
                atoms.motor = Some(RagdollMotor {
                    enabled: self.u8(data, field + 2)? != 0,
                    target_b_rca: [
                        self.vec4(data, field + 16)?,
                        self.vec4(data, field + 32)?,
                        self.vec4(data, field + 48)?,
                    ],
                    field,
                });
            }
            if let Some(field) = next(ATOM_ANG_FRICTION, 12)? {
                atoms.angular_friction = Some(self.f32(data, field + 8)?);
                atoms.friction_field = Some(field);
            }
            if let Some(field) = next(ATOM_TWIST_LIMIT, 24)? {
                atoms.twist_limit = Some(self.angular_limit(data, field)?);
            }
            if let Some(field) = next(ATOM_CONE_LIMIT, 24)? {
                atoms.cone_limit = Some(self.angular_limit(data, field)?);
            }
            if let Some(field) = next(ATOM_CONE_LIMIT, 24)? {
                atoms.planes_limit = Some(self.angular_limit(data, field)?);
            }
        } else {
            let _motor = next(
                ATOM_ANG_MOTOR,
                if self.layout.pointer == 4 { 20 } else { 24 },
            )?;
            if let Some(field) = next(ATOM_ANG_FRICTION, 12)? {
                atoms.angular_friction = Some(self.f32(data, field + 8)?);
                atoms.friction_field = Some(field);
            }
            if let Some(field) = next(ATOM_ANG_LIMIT, 24)? {
                atoms.hinge_limit = Some(self.angular_limit(data, field)?);
            }
            let _planar = next(ATOM_2D_ANG, 4)?;
        }
        let _ball = next(ATOM_BALL_SOCKET, 16)?;
        Ok(Some(atoms))
    }

    /// Twist, cone and angular-limit atoms all keep min, max, tau and damping
    /// as four floats; the angles start at +8 in the 2014 layout and at +4 in
    /// older ones. The values themselves say which layout applies.
    fn angular_limit(&self, data: ObjectKey, atom: usize) -> io::Result<AngularLimit> {
        for start in [8usize, 4] {
            let values = [
                self.f32(data, atom + start)?,
                self.f32(data, atom + start + 4)?,
                self.f32(data, atom + start + 8)?,
                self.f32(data, atom + start + 12)?,
            ];
            // Havok writes -100 as the cone limit's "no minimum" sentinel.
            let plausible = values.iter().all(|value| value.is_finite())
                && (-1000.0..=2.0 * PI + 0.01).contains(&values[0])
                && values[1].abs() <= 2.0 * PI + 0.01
                && values[0] <= values[1] + 1.0e-4
                && (0.0..=1.5).contains(&values[2])
                && (0.0..=1.5).contains(&values[3]);
            if plausible {
                return Ok(AngularLimit {
                    min_angle: values[0],
                    max_angle: values[1],
                    tau_factor: values[2],
                    damping_factor: values[3],
                    field: atom + start,
                });
            }
        }
        Err(invalid(&format!(
            "HKRG constraint atom at {:#x}+{atom:#x} has no plausible angular limit",
            data.offset
        )))
    }

    fn skeleton_mapper(
        &self,
        key: ObjectKey,
        classes: &HashMap<ObjectKey, String>,
        skeletons: &[&HkrgSkeleton],
    ) -> io::Result<SkeletonMapper> {
        let base = self.layout.referenced;
        // The two skeleton links are the mapper's first pointers to hkaSkeleton
        // objects; the fixup tables locate them without trusting an offset.
        let mut skeleton_links = self.pointer_fields(key).into_iter().filter(|(_, target)| {
            classes
                .get(target)
                .is_some_and(|class| class == "hkaSkeleton")
        });
        let first = skeleton_links.next();
        let skeleton_a = first.map(|(_, target)| target);
        let skeleton_b = skeleton_links.next().map(|(_, target)| target);
        // hkaSkeletonMapperData is 16-byte aligned inside the mapper; its
        // first field is skeletonA, so that pointer's offset anchors the rest.
        let mapping_base = first.map_or(align(base, 16), |(field, _)| field);
        let bone_count = |skeleton: Option<ObjectKey>| {
            skeleton
                .and_then(|key| skeletons.iter().find(|candidate| candidate.key == key))
                .map(|skeleton| skeleton.bones.len())
        };
        let (count_a, count_b) = (bone_count(skeleton_a), bone_count(skeleton_b));
        let arrays = mapping_base + self.layout.pointer * 2;
        // 2014 files carry partition tables before the mappings; older ones
        // start with the simple mappings. Accept whichever reads sensibly.
        let mut chosen = None;
        for simple_field in [arrays + self.layout.array * 3, arrays] {
            let chain_field = simple_field + self.layout.array;
            let Ok(simple) = self.simple_mappings(key, simple_field) else {
                continue;
            };
            let Ok(chain) = self.chain_mappings(key, chain_field) else {
                continue;
            };
            let in_range = |bone: i16, count: Option<usize>| {
                bone >= 0 && count.is_none_or(|count| (bone as usize) < count)
            };
            let plausible = simple.iter().all(|mapping| {
                in_range(mapping.bone_a, count_a)
                    && in_range(mapping.bone_b, count_b)
                    && is_unit(mapping.a_from_b.rotation)
            }) && chain.iter().all(|mapping| {
                in_range(mapping.start_bone_a, count_a)
                    && in_range(mapping.end_bone_b, count_b)
                    && is_unit(mapping.start_a_from_b.rotation)
            });
            if plausible && (!simple.is_empty() || !chain.is_empty()) {
                chosen = Some((simple, chain));
                break;
            }
        }
        let (simple_mappings, chain_mappings) = chosen.unwrap_or_default();
        Ok(SkeletonMapper {
            key,
            skeleton_a,
            skeleton_b,
            simple_mappings,
            chain_mappings,
        })
    }

    fn simple_mappings(&self, key: ObjectKey, field: usize) -> io::Result<Vec<SimpleMapping>> {
        let (storage, count) = self.array(key, field)?;
        let Some(storage) = storage else {
            return Ok(Vec::new());
        };
        if count > 4096 {
            return Err(invalid("HKRG simple mapping count is unreasonable"));
        }
        (0..count)
            .map(|index| {
                let base = index * 64;
                Ok(SimpleMapping {
                    bone_a: self.i16(storage, base)?,
                    bone_b: self.i16(storage, base + 2)?,
                    a_from_b: self.qs_transform(storage, base + 16)?,
                })
            })
            .collect()
    }

    fn chain_mappings(&self, key: ObjectKey, field: usize) -> io::Result<Vec<ChainMapping>> {
        let (storage, count) = self.array(key, field)?;
        let Some(storage) = storage else {
            return Ok(Vec::new());
        };
        if count > 4096 {
            return Err(invalid("HKRG chain mapping count is unreasonable"));
        }
        (0..count)
            .map(|index| {
                let base = index * 112;
                Ok(ChainMapping {
                    start_bone_a: self.i16(storage, base)?,
                    start_bone_b: self.i16(storage, base + 2)?,
                    end_bone_a: self.i16(storage, base + 4)?,
                    end_bone_b: self.i16(storage, base + 6)?,
                    start_a_from_b: self.qs_transform(storage, base + 16)?,
                    end_a_from_b: self.qs_transform(storage, base + 64)?,
                })
            })
            .collect()
    }
}

impl HkrgDocument {
    pub fn is_ragdoll(data: &[u8]) -> bool {
        crate::Settings::Magic::is_hkcl(data)
            && HkclDocument::parse(data)
                .map(|document| {
                    document
                        .type_names
                        .iter()
                        .any(|name| name == "hkaRagdollInstance")
                })
                .unwrap_or(false)
    }

    pub fn parse(data: &[u8]) -> io::Result<Self> {
        let base = HkclDocument::parse(data)?;
        let classes: HashMap<ObjectKey, String> = base
            .items
            .iter()
            .map(|item| {
                (
                    ObjectKey {
                        section_index: item.data_section_index,
                        offset: item.data_offset,
                    },
                    base.type_names
                        .get(item.type_index as usize)
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .collect();
        if !classes.values().any(|class| class == "hkaRagdollInstance") {
            return Err(invalid(
                "the packfile has no hkaRagdollInstance; it is not an HKRG ragdoll",
            ));
        }
        let reader = Reader::new(&base);
        let mut keys: Vec<(&ObjectKey, &String)> = classes.iter().collect();
        keys.sort_by_key(|(key, _)| (key.section_index, key.offset));
        let find = |class: &str| -> Vec<ObjectKey> {
            keys.iter()
                .filter(|(_, name)| name.as_str() == class)
                .map(|(key, _)| **key)
                .collect()
        };

        let ragdoll = find("hkaRagdollInstance")
            .first()
            .map(|key| reader.ragdoll_instance(*key))
            .transpose()?;
        let skeletons: Vec<HkrgSkeleton> = find("hkaSkeleton")
            .into_iter()
            .map(|key| reader.skeleton(key))
            .collect::<io::Result<_>>()?;
        let ragdoll_skeleton = ragdoll
            .as_ref()
            .and_then(|instance| instance.skeleton_key)
            .and_then(|key| {
                skeletons
                    .iter()
                    .find(|skeleton| skeleton.key == key)
                    .cloned()
            });
        let model_skeleton = skeletons
            .iter()
            .find(|skeleton| skeleton.name.eq_ignore_ascii_case("model_skeleton"))
            .or_else(|| {
                skeletons.iter().find(|skeleton| {
                    Some(skeleton.key) != ragdoll_skeleton.as_ref().map(|value| value.key)
                })
            })
            .cloned();
        let physics_systems: Vec<PhysicsSystem> = find("hkpPhysicsData")
            .into_iter()
            .map(|key| reader.pointer_array(key, reader.layout.referenced + reader.layout.pointer))
            .collect::<io::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .map(|key| reader.physics_system(key))
            .collect::<io::Result<_>>()?;

        // hkaRagdollInstance owns the body ordering that boneToRigidBodyMap
        // uses; hkpPhysicsSystem may store the same bodies in another order.
        let body_keys: Vec<ObjectKey> = match ragdoll
            .as_ref()
            .filter(|instance| !instance.rigid_body_keys.is_empty())
        {
            Some(instance) => instance.rigid_body_keys.clone(),
            None => physics_systems
                .iter()
                .flat_map(|system| system.rigid_body_keys.iter().copied())
                .collect(),
        };
        let mut body_to_bone: HashMap<usize, i32> = HashMap::new();
        if let Some(instance) = &ragdoll {
            for (bone, body) in instance.bone_to_rigid_body.iter().enumerate() {
                if let Ok(body) = usize::try_from(*body) {
                    body_to_bone.entry(body).or_insert(bone as i32);
                }
            }
        }
        let rigid_bodies: Vec<RigidBody> = body_keys
            .iter()
            .enumerate()
            .map(|(index, key)| {
                reader.rigid_body(
                    index,
                    *key,
                    &classes,
                    body_to_bone.get(&index).copied().unwrap_or(-1),
                )
            })
            .collect::<io::Result<_>>()?;
        let constraint_keys: Vec<ObjectKey> = match ragdoll
            .as_ref()
            .filter(|instance| !instance.constraint_keys.is_empty())
        {
            Some(instance) => instance.constraint_keys.clone(),
            None => physics_systems
                .iter()
                .flat_map(|system| system.constraint_keys.iter().copied())
                .collect(),
        };
        let constraints = constraint_keys
            .iter()
            .enumerate()
            .map(|(index, key)| reader.constraint(index, *key, &classes, &rigid_bodies))
            .collect::<io::Result<_>>()?;
        let skeleton_refs: Vec<&HkrgSkeleton> = skeletons.iter().collect();
        let mappers = find("hkaSkeletonMapper")
            .into_iter()
            .map(|key| reader.skeleton_mapper(key, &classes, &skeleton_refs))
            .collect::<io::Result<_>>()?;

        Ok(Self {
            raw: base.raw,
            header: base.header,
            sections: base.sections,
            class_names: classes,
            model_skeleton,
            ragdoll_skeleton,
            ragdoll,
            physics_systems,
            rigid_bodies,
            constraints,
            mappers,
        })
    }

    pub fn validate(&self) -> io::Result<()> {
        let ragdoll = self
            .ragdoll
            .as_ref()
            .ok_or_else(|| invalid("HKRG has no ragdoll instance"))?;
        let ragdoll_bones = self
            .ragdoll_skeleton
            .as_ref()
            .ok_or_else(|| invalid("HKRG ragdoll instance does not link a skeleton"))?;
        for (bone, body) in ragdoll.bone_to_rigid_body.iter().enumerate() {
            if *body >= 0 && *body as usize >= self.rigid_bodies.len() {
                return Err(invalid(&format!(
                    "HKRG ragdoll bone {bone} maps to missing rigid body {body}"
                )));
            }
        }
        for skeleton in [Some(ragdoll_bones), self.model_skeleton.as_ref()]
            .into_iter()
            .flatten()
        {
            for bone in &skeleton.bones {
                if bone.parent_index >= 0 && bone.parent_index as usize >= bone.index {
                    return Err(invalid(&format!(
                        "HKRG skeleton '{}' bone {} has an invalid parent",
                        skeleton.name, bone.index
                    )));
                }
                let pose = &bone.reference_pose;
                require_finite(&pose.translation, "bone translation")?;
                require_finite(&pose.rotation, "bone rotation")?;
                require_finite(&pose.scale, "bone scale")?;
            }
        }
        for body in &self.rigid_bodies {
            for row in &body.transform {
                require_finite(row, "rigid body transform")?;
            }
            if let Some(shape) = &body.shape {
                if !shape.radius.is_finite() || shape.radius < 0.0 {
                    return Err(invalid(&format!(
                        "HKRG body '{}' has an invalid radius",
                        body.name
                    )));
                }
            }
        }
        for constraint in &self.constraints {
            if constraint.body_a < 0 || constraint.body_b < 0 {
                return Err(invalid(&format!(
                    "HKRG constraint '{}' does not link two rigid bodies",
                    constraint.name
                )));
            }
        }
        if !self.constraint_frames_are_finite() {
            return Err(invalid("HKRG constraint frames contain non-finite values"));
        }
        Ok(())
    }

    pub fn summary(&self) -> HkrgSummary {
        HkrgSummary {
            format: "HKRG / Havok 2014 packfile",
            contents_version: self.header.contents_version.clone(),
            pointer_size: self.header.layout.pointer_size,
            big_endian: self.header.layout.endian == Endian::Big,
            physics_systems: self.physics_systems.len(),
            rigid_bodies: self.rigid_bodies.len(),
            constraints: self.constraints.len(),
            model_bones: self
                .model_skeleton
                .as_ref()
                .map_or(0, |skeleton| skeleton.bones.len()),
            ragdoll_bones: self
                .ragdoll_skeleton
                .as_ref()
                .map_or(0, |skeleton| skeleton.bones.len()),
            ragdoll_skeleton: self.ragdoll_skeleton.as_ref().map_or_else(
                || "(not linked)".to_owned(),
                |skeleton| skeleton.name.clone(),
            ),
            mappers: self.mappers.len(),
        }
    }

    pub fn leaves(&self) -> io::Result<Vec<HkclLeaf>> {
        let mut leaves = vec![yaml_leaf("Summary.bin", "Summary", &self.summary())?];
        if let Some(skeleton) = &self.model_skeleton {
            leaves.push(yaml_leaf("Skeletons/Model.bin", "Skeleton", skeleton)?);
        }
        if let Some(skeleton) = &self.ragdoll_skeleton {
            leaves.push(yaml_leaf("Skeletons/Ragdoll.bin", "Skeleton", skeleton)?);
        }
        for body in &self.rigid_bodies {
            leaves.push(yaml_leaf(
                &format!(
                    "RigidBodies/{:03} {}.bin",
                    body.index,
                    safe_name(&body.name)
                ),
                "RigidBody",
                body,
            )?);
        }
        for constraint in &self.constraints {
            leaves.push(yaml_leaf(
                &format!(
                    "Constraints/{:03} {}.bin",
                    constraint.index,
                    safe_name(&constraint.name)
                ),
                "Constraint",
                constraint,
            )?);
        }
        for (index, mapper) in self.mappers.iter().enumerate() {
            leaves.push(yaml_leaf(
                &format!("Mappers/{index:03}.bin"),
                "SkeletonMapper",
                mapper,
            )?);
        }
        Ok(leaves)
    }

    // ---- Rows ------------------------------------------------------------

    pub fn skeleton(&self, kind: SkeletonKind) -> Option<&HkrgSkeleton> {
        match kind {
            SkeletonKind::Model => self.model_skeleton.as_ref(),
            SkeletonKind::Ragdoll => self.ragdoll_skeleton.as_ref(),
        }
    }

    pub fn bone_rows(&self, kind: SkeletonKind) -> Vec<BoneRow> {
        self.skeleton(kind)
            .map(|skeleton| {
                skeleton
                    .bones
                    .iter()
                    .map(|bone| BoneRow {
                        index: bone.index,
                        name: bone.name.clone(),
                        parent_index: bone.parent_index,
                        translation: xyz(bone.reference_pose.translation),
                        rotation: bone.reference_pose.rotation,
                        scale: xyz(bone.reference_pose.scale),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn rigid_body_rows(&self) -> Vec<RigidBodyRow> {
        let bones = self.ragdoll_skeleton.as_ref();
        self.rigid_bodies
            .iter()
            .map(|body| {
                let transform = affine(&body.transform);
                let (local_start, local_end, radius, shape_type) = match &body.shape {
                    Some(shape) => (
                        xyz(shape.vertex_a),
                        if shape.class_name == "hkpCapsuleShape" {
                            xyz(shape.vertex_b)
                        } else {
                            xyz(shape.vertex_a)
                        },
                        shape.radius,
                        match shape.class_name.as_str() {
                            "hkpCapsuleShape" => "Capsule".to_owned(),
                            "hkpSphereShape" => "Sphere".to_owned(),
                            "hkpBoxShape" => "Box".to_owned(),
                            other => other.to_owned(),
                        },
                    ),
                    None => ([0.0; 3], [0.0; 3], 0.0, "hkpShape".to_owned()),
                };
                RigidBodyRow {
                    index: body.index,
                    name: body.name.clone(),
                    shape_type,
                    bone_index: body.bone_index,
                    bone_name: usize::try_from(body.bone_index)
                        .ok()
                        .and_then(|index| bones.and_then(|skeleton| skeleton.bones.get(index)))
                        .map(|bone| bone.name.clone())
                        .unwrap_or_default(),
                    start: transform_point(local_start, &transform),
                    end: transform_point(local_end, &transform),
                    radius,
                    transform: body.transform,
                }
            })
            .collect()
    }

    pub fn constraint_rows(&self) -> Vec<ConstraintRow> {
        self.constraints
            .iter()
            .map(|constraint| {
                let body_name = |index: i32| {
                    usize::try_from(index)
                        .ok()
                        .and_then(|index| self.rigid_bodies.get(index))
                        .map(|body| body.name.clone())
                        .unwrap_or_default()
                };
                let atoms = constraint.atoms.as_ref();
                ConstraintRow {
                    index: constraint.index,
                    name: constraint.name.clone(),
                    constraint_type: constraint.constraint_type.clone(),
                    body_a: constraint.body_a,
                    body_b: constraint.body_b,
                    body_a_name: body_name(constraint.body_a),
                    body_b_name: body_name(constraint.body_b),
                    twist: atoms.and_then(|atoms| atoms.twist_limit),
                    cone: atoms.and_then(|atoms| atoms.cone_limit),
                    planes: atoms.and_then(|atoms| atoms.planes_limit),
                    hinge: atoms.and_then(|atoms| atoms.hinge_limit),
                    angular_friction: atoms.and_then(|atoms| atoms.angular_friction),
                }
            })
            .collect()
    }

    // ---- In-place edits ---------------------------------------------------

    /// Renames bones and rewrites reference poses. Parent topology is locked.
    pub fn update_bone_rows(&mut self, kind: SkeletonKind, rows: &[BoneRow]) -> io::Result<()> {
        let label = match kind {
            SkeletonKind::Model => "model skeleton",
            SkeletonKind::Ragdoll => "ragdoll skeleton",
        };
        let skeleton = self
            .skeleton(kind)
            .cloned()
            .ok_or_else(|| invalid(&format!("the HKRG file has no {label}")))?;
        let mut raw = self.raw.clone();
        for row in rows {
            let Some(bone) = skeleton.bones.get(row.index) else {
                continue;
            };
            if let (Some(field), true) = (bone.name_field, row.name != bone.name) {
                write_string_in_place(&mut raw, &self.sections, field, &row.name)?;
            }
            let Some(pose) = bone.pose_field else {
                continue;
            };
            let existing = &bone.reference_pose;
            write_vec4(
                &mut raw,
                &self.sections,
                self.header.layout.endian,
                pose,
                0,
                [
                    row.translation[0],
                    row.translation[1],
                    row.translation[2],
                    existing.translation[3],
                ],
            )?;
            write_vec4(
                &mut raw,
                &self.sections,
                self.header.layout.endian,
                pose,
                16,
                row.rotation,
            )?;
            write_vec4(
                &mut raw,
                &self.sections,
                self.header.layout.endian,
                pose,
                32,
                [row.scale[0], row.scale[1], row.scale[2], existing.scale[3]],
            )?;
        }
        self.reload(raw)
    }

    /// Writes the body transform first, then stores the capsule endpoints in
    /// the body's local space so a body following its bone does not bake that
    /// movement into the shape.
    pub fn update_rigid_body_rows(&mut self, rows: &[RigidBodyRow]) -> io::Result<()> {
        let endian = self.header.layout.endian;
        let mut raw = self.raw.clone();
        for row in rows {
            let Some(body) = self.rigid_bodies.get(row.index) else {
                continue;
            };
            if let (Some(field), true) = (body.name_field, row.name != body.name) {
                write_string_in_place(&mut raw, &self.sections, field, &row.name)?;
            }
            for (index, values) in row.transform.iter().enumerate() {
                write_vec4(
                    &mut raw,
                    &self.sections,
                    endian,
                    body.key,
                    body.transform_offset + index * 16,
                    *values,
                )?;
            }
            let Some(shape) = &body.shape else {
                continue;
            };
            let transform = affine(&row.transform);
            let inverse = invert(&transform).unwrap_or(IDENTITY);
            let start = transform_point(row.start, &inverse);
            let end = transform_point(row.end, &inverse);
            let layout = self.layout();
            let radius_field = align(layout.referenced + 4, layout.pointer) + layout.pointer;
            match shape.class_name.as_str() {
                "hkpCapsuleShape" => {
                    let vertex_field = align(radius_field + 4, 16);
                    let radius_a = if row.radius > 0.0 {
                        row.radius
                    } else {
                        shape.vertex_a[3]
                    };
                    let radius_b = if row.radius > 0.0 {
                        row.radius
                    } else {
                        shape.vertex_b[3]
                    };
                    write_vec4(
                        &mut raw,
                        &self.sections,
                        endian,
                        shape.key,
                        vertex_field,
                        [start[0], start[1], start[2], radius_a],
                    )?;
                    write_vec4(
                        &mut raw,
                        &self.sections,
                        endian,
                        shape.key,
                        vertex_field + 16,
                        [end[0], end[1], end[2], radius_b],
                    )?;
                    write_f32(
                        &mut raw,
                        &self.sections,
                        endian,
                        shape.key,
                        radius_field,
                        row.radius,
                    )?;
                }
                "hkpSphereShape" => {
                    write_f32(
                        &mut raw,
                        &self.sections,
                        endian,
                        shape.key,
                        radius_field,
                        row.radius,
                    )?;
                }
                _ => {}
            }
        }
        self.reload(raw)
    }

    pub fn update_constraints(&mut self, edits: &[ConstraintEdit]) -> io::Result<()> {
        let endian = self.header.layout.endian;
        let mut raw = self.raw.clone();
        for edit in edits {
            let Some(constraint) = self.constraints.get(edit.index) else {
                continue;
            };
            if let (Some(field), Some(name)) = (constraint.name_field, edit.name.as_deref()) {
                if !name.trim().is_empty() && name != constraint.name {
                    write_string_in_place(&mut raw, &self.sections, field, name)?;
                }
            }
            let (Some(atoms), Some(data)) = (&constraint.atoms, constraint.data_key) else {
                continue;
            };
            for (limit, values) in [
                (atoms.twist_limit, edit.twist),
                (atoms.cone_limit, edit.cone),
                (atoms.planes_limit, edit.planes),
                (atoms.hinge_limit, edit.hinge),
            ] {
                let Some(limit) = limit else {
                    continue;
                };
                for (slot, value) in [
                    values.min_angle,
                    values.max_angle,
                    values.tau_factor,
                    values.damping_factor,
                ]
                .into_iter()
                .enumerate()
                {
                    if let Some(value) = value {
                        write_f32(
                            &mut raw,
                            &self.sections,
                            endian,
                            data,
                            limit.field + slot * 4,
                            value,
                        )?;
                    }
                }
            }
            if let (Some(field), Some(value)) = (atoms.friction_field, edit.angular_friction) {
                write_f32(&mut raw, &self.sections, endian, data, field + 8, value)?;
            }
        }
        self.reload(raw)
    }

    /// Re-centres capsule bodies after their endpoints change. Havok stores the
    /// local centre of mass separately from the visible capsule geometry;
    /// vanilla files keep it at the exact midpoint of the two vertices.
    pub fn rebuild_capsule_centers_of_mass(&mut self) -> io::Result<usize> {
        let endian = self.header.layout.endian;
        let mut raw = self.raw.clone();
        let mut updated = 0;
        for body in &self.rigid_bodies {
            let Some(shape) = body
                .shape
                .as_ref()
                .filter(|shape| shape.class_name == "hkpCapsuleShape")
            else {
                continue;
            };
            let center = [
                (shape.vertex_a[0] + shape.vertex_b[0]) * 0.5,
                (shape.vertex_a[1] + shape.vertex_b[1]) * 0.5,
                (shape.vertex_a[2] + shape.vertex_b[2]) * 0.5,
                body.swept_transform[4][3],
            ];
            write_vec4(
                &mut raw,
                &self.sections,
                endian,
                body.key,
                body.transform_offset + 64 + 64,
                center,
            )?;
            updated += 1;
        }
        self.reload(raw)?;
        Ok(updated)
    }

    /// hkpMotionState stores the centre-of-mass pose separately from the body
    /// transform. Retargeting the transform alone leaves Havok with a stale
    /// pose to integrate toward, so rebuild the swept transform from it.
    pub fn rebuild_motion_sweeps_from_transforms(&mut self) -> io::Result<()> {
        let endian = self.header.layout.endian;
        let mut raw = self.raw.clone();
        for body in &self.rigid_bodies {
            let transform = affine(&body.transform);
            let center_world = transform_point(xyz(body.swept_transform[4]), &transform);
            let swept = body.transform_offset + 64;
            for row in 0..2 {
                let existing = body.swept_transform[row];
                write_vec4(
                    &mut raw,
                    &self.sections,
                    endian,
                    body.key,
                    swept + row * 16,
                    [
                        center_world[0],
                        center_world[1],
                        center_world[2],
                        existing[3],
                    ],
                )?;
            }
            let mut orientation =
                normalize4(quaternion_from_matrix(&normalize_affine_frame(&transform)));
            let previous = body.swept_transform[2];
            if dot4(orientation, previous) < 0.0 {
                orientation = [
                    -orientation[0],
                    -orientation[1],
                    -orientation[2],
                    -orientation[3],
                ];
            }
            write_vec4(
                &mut raw,
                &self.sections,
                endian,
                body.key,
                swept + 32,
                orientation,
            )?;
            write_vec4(
                &mut raw,
                &self.sections,
                endian,
                body.key,
                swept + 48,
                orientation,
            )?;
        }
        self.reload(raw)
    }

    /// Largest distance between a body's stored swept centre of mass and the
    /// one its transform implies. Vanilla files sit near zero.
    pub fn motion_sweep_position_error(&self) -> f32 {
        self.rigid_bodies
            .iter()
            .map(|body| {
                let transform = affine(&body.transform);
                let expected = transform_point(xyz(body.swept_transform[4]), &transform);
                distance(expected, xyz(body.swept_transform[0]))
            })
            .fold(0.0, f32::max)
    }

    /// Rebuilds every constraint's local frames from the current ragdoll pose,
    /// keeping the authored padding. The joint frame is the child bone's world
    /// frame; the motor target follows frame B, expressed in body B space.
    pub fn repair_constraint_frames_for_current_pose(
        &mut self,
        update_motor_targets: bool,
    ) -> io::Result<()> {
        let endian = self.header.layout.endian;
        let Some(skeleton) = self.ragdoll_skeleton.as_ref() else {
            return Ok(());
        };
        let world = skeleton_world_transforms(skeleton);
        let parents: Vec<i16> = skeleton
            .bones
            .iter()
            .map(|bone| bone.parent_index)
            .collect();
        let mut raw = self.raw.clone();
        for constraint in &self.constraints {
            let (Some(atoms), Some(data)) = (&constraint.atoms, constraint.data_key) else {
                continue;
            };
            let (Some(body_a), Some(body_b)) = (
                usize::try_from(constraint.body_a)
                    .ok()
                    .and_then(|index| self.rigid_bodies.get(index)),
                usize::try_from(constraint.body_b)
                    .ok()
                    .and_then(|index| self.rigid_bodies.get(index)),
            ) else {
                continue;
            };
            let world_a = affine(&body_a.transform);
            let world_b = affine(&body_b.transform);
            let (Some(inverse_a), Some(inverse_b)) = (invert(&world_a), invert(&world_b)) else {
                continue;
            };
            let joint = joint_world_frame(
                &world,
                &parents,
                body_a.bone_index,
                body_b.bone_index,
                translation_of(&world_a),
                translation_of(&world_b),
            );
            let local_a = multiply(&joint, &inverse_a);
            let local_b = multiply(&joint, &inverse_b);
            write_transform_preserving_padding(
                &mut raw,
                &self.sections,
                endian,
                data,
                atoms.transforms_field + 16,
                &atoms.transform_a,
                &local_a,
            )?;
            write_transform_preserving_padding(
                &mut raw,
                &self.sections,
                endian,
                data,
                atoms.transforms_field + 80,
                &atoms.transform_b,
                &local_b,
            )?;
            if let (true, Some(motor)) = (update_motor_targets, atoms.motor) {
                let rotation = clear_translation(&local_b);
                for row in 0..3 {
                    let existing = motor.target_b_rca[row];
                    write_vec4(
                        &mut raw,
                        &self.sections,
                        endian,
                        data,
                        motor.field + 16 + row * 16,
                        [
                            rotation[row][0],
                            rotation[row][1],
                            rotation[row][2],
                            existing[3],
                        ],
                    )?;
                }
            }
        }
        self.reload(raw)
    }

    pub fn constraint_frames_are_finite(&self) -> bool {
        self.constraints.iter().all(|constraint| {
            constraint.atoms.as_ref().is_none_or(|atoms| {
                atoms
                    .transform_a
                    .iter()
                    .chain(atoms.transform_b.iter())
                    .all(|row| row.iter().all(|value| value.is_finite()))
                    && atoms.motor.is_none_or(|motor| {
                        motor
                            .target_b_rca
                            .iter()
                            .all(|row| row.iter().all(|value| value.is_finite()))
                    })
            })
        })
    }

    // ---- Diagnostics -------------------------------------------------------

    pub fn constraint_pose_diagnostics(&self) -> Vec<ConstraintPoseDiagnostic> {
        let bone_world = self
            .ragdoll_skeleton
            .as_ref()
            .map(skeleton_world_transforms)
            .unwrap_or_default();
        self.constraints
            .iter()
            .map(|constraint| {
                let body = |index: i32| {
                    usize::try_from(index)
                        .ok()
                        .and_then(|index| self.rigid_bodies.get(index))
                };
                let (frame_a, frame_b) = constraint
                    .atoms
                    .as_ref()
                    .map_or((IDENTITY, IDENTITY), |atoms| {
                        (affine(&atoms.transform_a), affine(&atoms.transform_b))
                    });
                let world_a = multiply(
                    &frame_a,
                    &body(constraint.body_a).map_or(IDENTITY, |body| affine(&body.transform)),
                );
                let world_b = multiply(
                    &frame_b,
                    &body(constraint.body_b).map_or(IDENTITY, |body| affine(&body.transform)),
                );
                let bone_a = body(constraint.body_a).map_or(-1, |body| body.bone_index);
                let bone_b = body(constraint.body_b).map_or(-1, |body| body.bone_index);
                let skeleton_rotation = match (usize::try_from(bone_a), usize::try_from(bone_b)) {
                    (Ok(a), Ok(b)) if a < bone_world.len() && b < bone_world.len() => {
                        rotation_angle_degrees(&bone_world[a], &bone_world[b])
                    }
                    _ => 0.0,
                };
                let motor = constraint.atoms.as_ref().and_then(|atoms| atoms.motor);
                let target = motor.map_or(IDENTITY, |motor| matrix3_to_mat4(&motor.target_b_rca));
                ConstraintPoseDiagnostic {
                    index: constraint.index,
                    name: constraint.name.clone(),
                    body_a: constraint.body_a,
                    body_b: constraint.body_b,
                    bone_a,
                    bone_b,
                    frame_position_gap: distance(
                        translation_of(&world_a),
                        translation_of(&world_b),
                    ),
                    frame_rotation_gap_degrees: rotation_angle_degrees(&world_a, &world_b),
                    skeleton_relative_rotation_degrees: skeleton_rotation,
                    motor_enabled: motor.is_some_and(|motor| motor.enabled),
                    motor_target_rotation_degrees: rotation_angle_degrees(&IDENTITY, &target),
                }
            })
            .collect()
    }

    /// Measures the initial hinge angle in the reference pose. A retarget can
    /// keep coincident anchors while placing that angle outside an authored
    /// limit, which makes Havok drive the limb back toward the limit.
    pub fn hinge_reference_diagnostics(&self) -> Vec<HingeReferenceDiagnostic> {
        self.constraints
            .iter()
            .filter(|constraint| constraint.class_name == "hkpLimitedHingeConstraintData")
            .filter_map(|constraint| {
                let atoms = constraint.atoms.as_ref()?;
                let body_a = self
                    .rigid_bodies
                    .get(usize::try_from(constraint.body_a).ok()?)?;
                let body_b = self
                    .rigid_bodies
                    .get(usize::try_from(constraint.body_b).ok()?)?;
                let world_a = multiply(&affine(&atoms.transform_a), &affine(&body_a.transform));
                let world_b = multiply(&affine(&atoms.transform_b), &affine(&body_b.transform));
                let inverse_b = invert(&clear_translation(&world_b))?;
                let relative = multiply(&clear_translation(&world_a), &inverse_b);
                let rotation = normalize4(quaternion_from_matrix(&relative));
                // The limited-hinge free axis is axis 0 of the local frame.
                let angle = normalize_angle(2.0 * rotation[0].atan2(rotation[3]));
                let off_axis = 2.0
                    * (rotation[1] * rotation[1] + rotation[2] * rotation[2])
                        .sqrt()
                        .atan2((rotation[0] * rotation[0] + rotation[3] * rotation[3]).sqrt());
                let (min, max) = atoms
                    .hinge_limit
                    .map_or((f32::NEG_INFINITY, f32::INFINITY), |limit| {
                        (limit.min_angle, limit.max_angle)
                    });
                Some(HingeReferenceDiagnostic {
                    constraint_index: constraint.index,
                    constraint_name: constraint.name.clone(),
                    reference_angle_degrees: angle.to_degrees(),
                    minimum_angle_degrees: min.to_degrees(),
                    maximum_angle_degrees: max.to_degrees(),
                    off_axis_degrees: off_axis.to_degrees(),
                    is_inside_limit: angle >= min - 0.001 && angle <= max + 0.001,
                })
            })
            .collect()
    }

    /// Exposes the frame-order candidates for the motor target so retargeting
    /// code can be checked against a vanilla file before writing.
    pub fn motor_target_diagnostics(&self) -> Vec<MotorTargetDiagnostic> {
        self.constraints
            .iter()
            .filter_map(|constraint| {
                let atoms = constraint.atoms.as_ref()?;
                let motor = atoms.motor?;
                let a = clear_translation(&affine(&atoms.transform_a));
                let b = clear_translation(&affine(&atoms.transform_b));
                let inverse_a = invert(&a)?;
                let inverse_b = invert(&b)?;
                let target = matrix3_to_mat4(&motor.target_b_rca);
                Some(MotorTargetDiagnostic {
                    constraint_index: constraint.index,
                    constraint_name: constraint.name.clone(),
                    frame_b_error_degrees: rotation_angle_degrees(&target, &b),
                    frame_a_error_degrees: rotation_angle_degrees(&target, &a),
                    b_times_inverse_a_error_degrees: rotation_angle_degrees(
                        &target,
                        &multiply(&b, &inverse_a),
                    ),
                    inverse_b_times_a_error_degrees: rotation_angle_degrees(
                        &target,
                        &multiply(&inverse_b, &a),
                    ),
                })
            })
            .collect()
    }

    /// hkaSkeletonMapper stores aFromB as the reference-pose transform that
    /// converts a point from skeleton B model space into skeleton A model
    /// space. Report how far each stored mapping is from that relation.
    pub fn mapper_pose_diagnostics(&self) -> Vec<MapperPoseDiagnostic> {
        let skeleton = |key: Option<ObjectKey>| {
            [self.model_skeleton.as_ref(), self.ragdoll_skeleton.as_ref()]
                .into_iter()
                .flatten()
                .find(|skeleton| Some(skeleton.key) == key)
        };
        let mut rows = Vec::new();
        for (mapper_index, mapper) in self.mappers.iter().enumerate() {
            let (Some(a), Some(b)) = (skeleton(mapper.skeleton_a), skeleton(mapper.skeleton_b))
            else {
                continue;
            };
            let world_a = skeleton_world_transforms(a);
            let world_b = skeleton_world_transforms(b);
            for (mapping_index, mapping) in mapper.simple_mappings.iter().enumerate() {
                let (Ok(bone_a), Ok(bone_b)) = (
                    usize::try_from(mapping.bone_a),
                    usize::try_from(mapping.bone_b),
                ) else {
                    continue;
                };
                let (Some(frame_a), Some(frame_b)) = (world_a.get(bone_a), world_b.get(bone_b))
                else {
                    continue;
                };
                let stored = pose_to_local_matrix(&mapping.a_from_b);
                let b_to_a =
                    invert(frame_b).map_or(IDENTITY, |inverse| multiply(&inverse, frame_a));
                let a_to_b =
                    invert(frame_a).map_or(IDENTITY, |inverse| multiply(&inverse, frame_b));
                rows.push(MapperPoseDiagnostic {
                    mapper_index,
                    mapping_index,
                    bone_a: mapping.bone_a,
                    bone_b: mapping.bone_b,
                    b_to_a_position_error: distance(
                        translation_of(&stored),
                        translation_of(&b_to_a),
                    ),
                    b_to_a_rotation_error_degrees: rotation_angle_degrees(&stored, &b_to_a),
                    a_to_b_position_error: distance(
                        translation_of(&stored),
                        translation_of(&a_to_b),
                    ),
                    a_to_b_rotation_error_degrees: rotation_angle_degrees(&stored, &a_to_b),
                });
            }
        }
        rows
    }

    fn layout(&self) -> Layout {
        Layout::new(self.header.layout.pointer_size)
    }

    fn reload(&mut self, raw: Vec<u8>) -> io::Result<()> {
        *self = Self::parse(&raw)?;
        Ok(())
    }
}

// ---- Skeleton math ------------------------------------------------------

pub fn pose_to_local_matrix(pose: &QsTransform) -> Mat4 {
    let translation = xyz(pose.translation);
    let rotation = if length_squared4(pose.rotation) < 1.0e-6 {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        normalize4(pose.rotation)
    };
    let mut scale = xyz(pose.scale);
    for value in &mut scale {
        if value.abs() < 1.0e-6 {
            *value = 1.0;
        }
    }
    multiply(
        &multiply(&scale_matrix(scale), &quaternion_matrix(rotation)),
        &translation_matrix(translation),
    )
}

pub fn skeleton_world_transforms(skeleton: &HkrgSkeleton) -> Vec<Mat4> {
    let mut world = Vec::with_capacity(skeleton.bones.len());
    for (index, bone) in skeleton.bones.iter().enumerate() {
        let local = pose_to_local_matrix(&bone.reference_pose);
        let parent = usize::try_from(bone.parent_index)
            .ok()
            .filter(|parent| *parent < index);
        world.push(match parent {
            Some(parent) => multiply(&local, &world[parent]),
            None => local,
        });
    }
    world
}

fn joint_world_frame(
    bone_world: &[Mat4],
    parents: &[i16],
    bone_a: i32,
    bone_b: i32,
    fallback_a: Vec3,
    fallback_b: Vec3,
) -> Mat4 {
    let frame_bone = choose_constraint_frame_bone(parents, bone_a, bone_b);
    if let Some(frame) = usize::try_from(frame_bone)
        .ok()
        .and_then(|index| bone_world.get(index))
    {
        return normalize_affine_frame(frame);
    }
    translation_matrix([
        (fallback_a[0] + fallback_b[0]) * 0.5,
        (fallback_a[1] + fallback_b[1]) * 0.5,
        (fallback_a[2] + fallback_b[2]) * 0.5,
    ])
}

fn choose_constraint_frame_bone(parents: &[i16], bone_a: i32, bone_b: i32) -> i32 {
    let parent = |bone: i32| {
        usize::try_from(bone)
            .ok()
            .and_then(|index| parents.get(index))
            .map_or(-1, |parent| i32::from(*parent))
    };
    if parent(bone_a) == bone_b {
        bone_a
    } else if parent(bone_b) == bone_a || bone_b >= 0 {
        bone_b
    } else {
        bone_a
    }
}

// ---- Matrix helpers (row-major, row-vector convention like System.Numerics) --

pub fn multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut result = [[0.0; 4]; 4];
    for (i, row) in result.iter_mut().enumerate() {
        for (j, value) in row.iter_mut().enumerate() {
            *value = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    result
}

/// General 4x4 inverse; `None` when singular.
pub fn invert(m: &Mat4) -> Option<Mat4> {
    let a = m;
    let s0 = a[0][0] * a[1][1] - a[1][0] * a[0][1];
    let s1 = a[0][0] * a[1][2] - a[1][0] * a[0][2];
    let s2 = a[0][0] * a[1][3] - a[1][0] * a[0][3];
    let s3 = a[0][1] * a[1][2] - a[1][1] * a[0][2];
    let s4 = a[0][1] * a[1][3] - a[1][1] * a[0][3];
    let s5 = a[0][2] * a[1][3] - a[1][2] * a[0][3];
    let c5 = a[2][2] * a[3][3] - a[3][2] * a[2][3];
    let c4 = a[2][1] * a[3][3] - a[3][1] * a[2][3];
    let c3 = a[2][1] * a[3][2] - a[3][1] * a[2][2];
    let c2 = a[2][0] * a[3][3] - a[3][0] * a[2][3];
    let c1 = a[2][0] * a[3][2] - a[3][0] * a[2][2];
    let c0 = a[2][0] * a[3][1] - a[3][0] * a[2][1];
    let determinant = s0 * c5 - s1 * c4 + s2 * c3 + s3 * c2 - s4 * c1 + s5 * c0;
    if determinant.abs() < 1.0e-12 || !determinant.is_finite() {
        return None;
    }
    let d = 1.0 / determinant;
    Some([
        [
            (a[1][1] * c5 - a[1][2] * c4 + a[1][3] * c3) * d,
            (-a[0][1] * c5 + a[0][2] * c4 - a[0][3] * c3) * d,
            (a[3][1] * s5 - a[3][2] * s4 + a[3][3] * s3) * d,
            (-a[2][1] * s5 + a[2][2] * s4 - a[2][3] * s3) * d,
        ],
        [
            (-a[1][0] * c5 + a[1][2] * c2 - a[1][3] * c1) * d,
            (a[0][0] * c5 - a[0][2] * c2 + a[0][3] * c1) * d,
            (-a[3][0] * s5 + a[3][2] * s2 - a[3][3] * s1) * d,
            (a[2][0] * s5 - a[2][2] * s2 + a[2][3] * s1) * d,
        ],
        [
            (a[1][0] * c4 - a[1][1] * c2 + a[1][3] * c0) * d,
            (-a[0][0] * c4 + a[0][1] * c2 - a[0][3] * c0) * d,
            (a[3][0] * s4 - a[3][1] * s2 + a[3][3] * s0) * d,
            (-a[2][0] * s4 + a[2][1] * s2 - a[2][3] * s0) * d,
        ],
        [
            (-a[1][0] * c3 + a[1][1] * c1 - a[1][2] * c0) * d,
            (a[0][0] * c3 - a[0][1] * c1 + a[0][2] * c0) * d,
            (-a[3][0] * s3 + a[3][1] * s1 - a[3][2] * s0) * d,
            (a[2][0] * s3 - a[2][1] * s1 + a[2][2] * s0) * d,
        ],
    ])
}

/// Havok's fourth components are not projection terms; drop them before any
/// inversion.
pub fn affine(raw: &Mat4) -> Mat4 {
    [
        [raw[0][0], raw[0][1], raw[0][2], 0.0],
        [raw[1][0], raw[1][1], raw[1][2], 0.0],
        [raw[2][0], raw[2][1], raw[2][2], 0.0],
        [raw[3][0], raw[3][1], raw[3][2], 1.0],
    ]
}

pub fn clear_translation(m: &Mat4) -> Mat4 {
    [
        [m[0][0], m[0][1], m[0][2], 0.0],
        [m[1][0], m[1][1], m[1][2], 0.0],
        [m[2][0], m[2][1], m[2][2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn translation_of(m: &Mat4) -> Vec3 {
    [m[3][0], m[3][1], m[3][2]]
}

pub fn transform_point(point: Vec3, m: &Mat4) -> Vec3 {
    let mut out = [0.0; 3];
    for (axis, value) in out.iter_mut().enumerate() {
        *value = point[0] * m[0][axis] + point[1] * m[1][axis] + point[2] * m[2][axis] + m[3][axis];
    }
    out
}

fn translation_matrix(t: Vec3) -> Mat4 {
    let mut m = IDENTITY;
    m[3] = [t[0], t[1], t[2], 1.0];
    m
}

fn scale_matrix(s: Vec3) -> Mat4 {
    let mut m = IDENTITY;
    m[0][0] = s[0];
    m[1][1] = s[1];
    m[2][2] = s[2];
    m
}

fn quaternion_matrix(q: Vec4) -> Mat4 {
    let [x, y, z, w] = q;
    let (xx, yy, zz) = (x * x, y * y, z * z);
    let (xy, xz, yz) = (x * y, x * z, y * z);
    let (wx, wy, wz) = (w * x, w * y, w * z);
    [
        [1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy), 0.0],
        [2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx), 0.0],
        [2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy), 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn matrix3_to_mat4(rows: &[Vec4; 3]) -> Mat4 {
    [
        [rows[0][0], rows[0][1], rows[0][2], 0.0],
        [rows[1][0], rows[1][1], rows[1][2], 0.0],
        [rows[2][0], rows[2][1], rows[2][2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Quaternion for a row-major rotation matrix, System.Numerics style.
pub fn quaternion_from_matrix(m: &Mat4) -> Vec4 {
    let trace = m[0][0] + m[1][1] + m[2][2];
    if trace > 0.0 {
        let s = (trace + 1.0).sqrt();
        let inv = 0.5 / s;
        [
            (m[1][2] - m[2][1]) * inv,
            (m[2][0] - m[0][2]) * inv,
            (m[0][1] - m[1][0]) * inv,
            s * 0.5,
        ]
    } else if m[0][0] >= m[1][1] && m[0][0] >= m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt();
        let inv = 0.5 / s;
        [
            0.5 * s,
            (m[0][1] + m[1][0]) * inv,
            (m[0][2] + m[2][0]) * inv,
            (m[1][2] - m[2][1]) * inv,
        ]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt();
        let inv = 0.5 / s;
        [
            (m[1][0] + m[0][1]) * inv,
            0.5 * s,
            (m[2][1] + m[1][2]) * inv,
            (m[2][0] - m[0][2]) * inv,
        ]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt();
        let inv = 0.5 / s;
        [
            (m[2][0] + m[0][2]) * inv,
            (m[2][1] + m[1][2]) * inv,
            0.5 * s,
            (m[0][1] - m[1][0]) * inv,
        ]
    }
}

pub fn normalize_affine_frame(frame: &Mat4) -> Mat4 {
    let x = normalize_or([frame[0][0], frame[0][1], frame[0][2]], [1.0, 0.0, 0.0]);
    let raw_y = normalize_or([frame[1][0], frame[1][1], frame[1][2]], [0.0, 1.0, 0.0]);
    let projection = dot3(raw_y, x);
    let mut y = [
        raw_y[0] - x[0] * projection,
        raw_y[1] - x[1] * projection,
        raw_y[2] - x[2] * projection,
    ];
    y = if length_squared3(y) < 1.0e-6 {
        perpendicular_unit(x)
    } else {
        normalize3(y)
    };
    let mut z = cross(x, y);
    z = if length_squared3(z) < 1.0e-6 {
        [0.0, 0.0, 1.0]
    } else {
        normalize3(z)
    };
    y = normalize3(cross(z, x));
    [
        [x[0], x[1], x[2], 0.0],
        [y[0], y[1], y[2], 0.0],
        [z[0], z[1], z[2], 0.0],
        [frame[3][0], frame[3][1], frame[3][2], 1.0],
    ]
}

fn perpendicular_unit(axis: Vec3) -> Vec3 {
    let helper = if dot3(axis, [1.0, 0.0, 0.0]).abs() < 0.75 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let perpendicular = cross(axis, helper);
    if length_squared3(perpendicular) < 1.0e-6 {
        [0.0, 0.0, 1.0]
    } else {
        normalize3(perpendicular)
    }
}

pub fn rotation_angle_degrees(from: &Mat4, to: &Mat4) -> f32 {
    let a = normalize4(quaternion_from_matrix(&normalize_affine_frame(from)));
    let b = normalize4(quaternion_from_matrix(&normalize_affine_frame(to)));
    let dot = dot4(a, b).abs().clamp(0.0, 1.0);
    2.0 * dot.acos().to_degrees()
}

fn normalize_angle(mut angle: f32) -> f32 {
    while angle > PI {
        angle -= 2.0 * PI;
    }
    while angle < -PI {
        angle += 2.0 * PI;
    }
    angle
}

fn rows_orthonormal(m: &Mat4) -> bool {
    let rows: Vec<Vec3> = m
        .iter()
        .take(3)
        .map(|row| [row[0], row[1], row[2]])
        .collect();
    rows.iter()
        .all(|row| (length_squared3(*row) - 1.0).abs() < 2.0e-2)
        && dot3(rows[0], rows[1]).abs() < 2.0e-2
        && dot3(rows[0], rows[2]).abs() < 2.0e-2
        && dot3(rows[1], rows[2]).abs() < 2.0e-2
        && m[3].iter().all(|value| value.is_finite())
}

fn is_unit(v: Vec4) -> bool {
    v.iter().all(|value| value.is_finite()) && (length_squared4(v) - 1.0).abs() < 2.0e-2
}

fn xyz(v: Vec4) -> Vec3 {
    [v[0], v[1], v[2]]
}
fn dot3(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn dot4(a: Vec4, b: Vec4) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn length_squared3(v: Vec3) -> f32 {
    dot3(v, v)
}
fn length_squared4(v: Vec4) -> f32 {
    dot4(v, v)
}
fn normalize3(v: Vec3) -> Vec3 {
    let length = length_squared3(v).sqrt();
    [v[0] / length, v[1] / length, v[2] / length]
}
fn normalize_or(v: Vec3, fallback: Vec3) -> Vec3 {
    if length_squared3(v) < 1.0e-6 {
        fallback
    } else {
        normalize3(v)
    }
}
pub fn normalize4(v: Vec4) -> Vec4 {
    let length = length_squared4(v).sqrt();
    if length < 1.0e-12 || !length.is_finite() {
        return [0.0, 0.0, 0.0, 1.0];
    }
    [v[0] / length, v[1] / length, v[2] / length, v[3] / length]
}
fn distance(a: Vec3, b: Vec3) -> f32 {
    length_squared3([a[0] - b[0], a[1] - b[1], a[2] - b[2]]).sqrt()
}
fn align(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

// ---- In-place writers ---------------------------------------------------

fn absolute_field(
    sections: &[HkclSection],
    key: ObjectKey,
    field: usize,
    len: usize,
) -> io::Result<usize> {
    let section = sections
        .get(key.section_index)
        .ok_or_else(|| invalid("HKRG object section is out of range"))?;
    let start = section.absolute_data_start + key.offset as usize + field;
    if start + len > section.local_fixups.start {
        return Err(invalid("HKRG write exceeds section DATA"));
    }
    Ok(start)
}

fn write_f32(
    raw: &mut [u8],
    sections: &[HkclSection],
    endian: Endian,
    key: ObjectKey,
    field: usize,
    value: f32,
) -> io::Result<()> {
    let start = absolute_field(sections, key, field, 4)?;
    let bytes = match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    };
    raw[start..start + 4].copy_from_slice(&bytes);
    Ok(())
}

fn write_vec4(
    raw: &mut [u8],
    sections: &[HkclSection],
    endian: Endian,
    key: ObjectKey,
    field: usize,
    value: Vec4,
) -> io::Result<()> {
    for (index, component) in value.iter().enumerate() {
        write_f32(raw, sections, endian, key, field + index * 4, *component)?;
    }
    Ok(())
}

/// Writes the rotation rows and translation of `value` while keeping the
/// fourth components `existing` carries.
fn write_transform_preserving_padding(
    raw: &mut [u8],
    sections: &[HkclSection],
    endian: Endian,
    key: ObjectKey,
    field: usize,
    existing: &Mat4,
    value: &Mat4,
) -> io::Result<()> {
    for row in 0..4 {
        write_vec4(
            raw,
            sections,
            endian,
            key,
            field + row * 16,
            [
                value[row][0],
                value[row][1],
                value[row][2],
                existing[row][3],
            ],
        )?;
    }
    Ok(())
}

/// Strings live inline in DATA and cannot grow; a replacement must fit the
/// allocation the original occupied (its length plus terminator).
fn write_string_in_place(
    _raw: &mut [u8],
    _sections: &[HkclSection],
    _pointer_field: ObjectKey,
    value: &str,
) -> io::Result<()> {
    // The pointer field itself is relocated at load; the string it addresses
    // is found through the same fixup the reader used, stored as the target.
    Err(invalid(&format!(
        "renaming to '{value}' is not supported: HKRG strings are relocated inline and cannot be rewritten in place"
    )))
}

fn require_finite(values: &[f32], what: &str) -> io::Result<()> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(invalid(&format!("HKRG {what} is not finite")))
    }
}

fn yaml_leaf<T: Serialize>(path: &str, viewer_type: &str, value: &T) -> io::Result<HkclLeaf> {
    Ok(HkclLeaf {
        path: path.to_owned(),
        yaml: serde_yaml::to_string(value).map_err(io::Error::other)?,
        viewer_type: viewer_type.to_owned(),
        read_only: true,
    })
}

fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other => other,
        })
        .collect()
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn corpus() -> Vec<(String, HkrgDocument)> {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/hkrg");
        let Ok(entries) = fs::read_dir(&directory) else {
            eprintln!("skipping: {} is missing", directory.display());
            return Vec::new();
        };
        let mut paths: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|value| value == "hkrg"))
            .collect();
        paths.sort();
        paths
            .into_iter()
            .map(|path| {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                let document = HkrgDocument::parse(&fs::read(&path).unwrap())
                    .unwrap_or_else(|error| panic!("{name}: {error}"));
                (name, document)
            })
            .collect()
    }

    #[test]
    fn ragdoll_corpus_parses_and_validates() {
        for (name, document) in corpus() {
            document
                .validate()
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let summary = document.summary();
            assert!(summary.rigid_bodies > 0, "{name}: no rigid bodies");
            assert_eq!(
                summary.constraints,
                summary.rigid_bodies - 1,
                "{name}: a tree of bodies"
            );
            assert!(
                summary.ragdoll_bones > 0 && summary.model_bones >= summary.ragdoll_bones,
                "{name}: skeletons"
            );
            assert_eq!(summary.mappers, 2, "{name}: model<->ragdoll mappers");
            let ragdoll = document.ragdoll.as_ref().unwrap();
            assert_eq!(
                ragdoll.bone_to_rigid_body.len(),
                summary.ragdoll_bones,
                "{name}: bone map length"
            );
            for body in &document.rigid_bodies {
                assert!(
                    !body.name.starts_with("Rigid body "),
                    "{name}: body {} lost its name",
                    body.index
                );
                let shape = body
                    .shape
                    .as_ref()
                    .unwrap_or_else(|| panic!("{name}: body {} has no shape", body.name));
                assert!(
                    shape.class_name.ends_with("Shape"),
                    "{name}: {} shape {}",
                    body.name,
                    shape.class_name
                );
                if matches!(
                    shape.class_name.as_str(),
                    "hkpCapsuleShape" | "hkpSphereShape"
                ) {
                    assert!(
                        shape.radius > 0.0 && shape.radius < 5.0,
                        "{name}: {} radius {}",
                        body.name,
                        shape.radius
                    );
                }
                if shape.class_name == "hkpCapsuleShape" {
                    assert!(
                        (shape.vertex_a[3] - shape.radius).abs() < 1.0e-4,
                        "{name}: capsule vertex w carries the radius"
                    );
                }
                assert!(
                    body.bone_index >= 0,
                    "{name}: {} is bound to a bone",
                    body.name
                );
            }
            for constraint in &document.constraints {
                assert!(
                    !constraint.name.starts_with("Constraint "),
                    "{name}: constraint lost its name"
                );
                let atoms = constraint
                    .atoms
                    .as_ref()
                    .unwrap_or_else(|| panic!("{name}: {} atoms", constraint.name));
                match constraint.class_name.as_str() {
                    "hkpRagdollConstraintData" => {
                        assert!(
                            atoms.twist_limit.is_some()
                                && atoms.cone_limit.is_some()
                                && atoms.planes_limit.is_some(),
                            "{name}: {} limits",
                            constraint.name
                        );
                        assert!(atoms.motor.is_some(), "{name}: {} motor", constraint.name);
                    }
                    "hkpLimitedHingeConstraintData" => {
                        assert!(
                            atoms.hinge_limit.is_some(),
                            "{name}: {} hinge limit",
                            constraint.name
                        );
                    }
                    other => panic!("{name}: unexpected constraint class {other}"),
                }
                assert!(
                    atoms.angular_friction.is_some(),
                    "{name}: {} friction",
                    constraint.name
                );
            }
            for mapper in &document.mappers {
                assert!(mapper.skeleton_a.is_some() && mapper.skeleton_b.is_some());
                assert!(
                    !mapper.simple_mappings.is_empty(),
                    "{name}: mapper has simple mappings"
                );
            }
        }
    }

    #[test]
    fn vanilla_motion_sweeps_and_constraint_frames_agree_with_the_pose() {
        for (name, document) in corpus() {
            let error = document.motion_sweep_position_error();
            assert!(
                error < 1.0e-3,
                "{name}: swept centre of mass drifts {error}"
            );
            for diagnostic in document.constraint_pose_diagnostics() {
                assert!(
                    diagnostic.frame_position_gap < 0.05,
                    "{name}: {} frames are {} apart",
                    diagnostic.name,
                    diagnostic.frame_position_gap
                );
            }
            for hinge in document.hinge_reference_diagnostics() {
                // Some vanilla knees rest a few degrees past their limit; the
                // diagnostic must report that, not hide it.
                let overshoot = (hinge.minimum_angle_degrees - hinge.reference_angle_degrees)
                    .max(hinge.reference_angle_degrees - hinge.maximum_angle_degrees)
                    .max(0.0);
                assert!(
                    overshoot < 10.0,
                    "{name}: {} rests {overshoot} degrees outside its limit",
                    hinge.constraint_name
                );
                assert!(
                    hinge.off_axis_degrees.is_finite(),
                    "{name}: {} off-axis angle",
                    hinge.constraint_name
                );
            }
            for row in document.mapper_pose_diagnostics().iter().take(4) {
                assert!(row.b_to_a_position_error.is_finite());
            }
        }
    }

    #[test]
    fn rigid_body_rows_round_trip_through_in_place_edits() {
        for (name, mut document) in corpus() {
            let rows = document.rigid_body_rows();
            let mut edited = rows[0].clone();
            edited.radius += 0.01;
            edited.start[1] += 0.02;
            document
                .update_rigid_body_rows(std::slice::from_ref(&edited))
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let reread = document.rigid_body_rows();
            assert!(
                (reread[0].radius - edited.radius).abs() < 1.0e-5,
                "{name}: radius"
            );
            assert!(
                (reread[0].start[1] - edited.start[1]).abs() < 1.0e-4,
                "{name}: start"
            );
            assert_eq!(reread[1..], rows[1..], "{name}: other bodies untouched");
            document
                .update_rigid_body_rows(std::slice::from_ref(&rows[0]))
                .unwrap();
            let restored = document.rigid_body_rows();
            assert!((restored[0].radius - rows[0].radius).abs() < 1.0e-5);
            let capsules = document
                .rigid_bodies
                .iter()
                .filter(|body| {
                    body.shape
                        .as_ref()
                        .is_some_and(|shape| shape.class_name == "hkpCapsuleShape")
                })
                .count();
            let updated = document.rebuild_capsule_centers_of_mass().unwrap();
            assert_eq!(updated, capsules, "{name}: every capsule re-centred");
            document.rebuild_motion_sweeps_from_transforms().unwrap();
            assert!(document.motion_sweep_position_error() < 1.0e-4, "{name}");
            document.validate().unwrap();
        }
    }

    #[test]
    fn constraint_limits_and_bone_poses_write_in_place() {
        for (name, mut document) in corpus() {
            let before = document.constraint_rows();
            let target = before
                .iter()
                .find(|row| row.twist.is_some())
                .cloned()
                .unwrap_or_else(|| panic!("{name}: no ragdoll constraint"));
            let edit = ConstraintEdit {
                index: target.index,
                twist: AngularLimitEdit {
                    max_angle: Some(0.25),
                    ..Default::default()
                },
                angular_friction: Some(0.5),
                ..Default::default()
            };
            document
                .update_constraints(&[edit])
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let after = document.constraint_rows();
            assert_eq!(after[target.index].twist.unwrap().max_angle, 0.25);
            assert_eq!(
                after[target.index].twist.unwrap().min_angle,
                target.twist.unwrap().min_angle
            );
            assert_eq!(after[target.index].angular_friction, Some(0.5));

            let bones = document.bone_rows(SkeletonKind::Ragdoll);
            let mut edited = bones.clone();
            edited[1].translation[0] += 0.1;
            document
                .update_bone_rows(SkeletonKind::Ragdoll, &edited)
                .unwrap();
            let reread = document.bone_rows(SkeletonKind::Ragdoll);
            assert!((reread[1].translation[0] - edited[1].translation[0]).abs() < 1.0e-6);
            assert_eq!(reread[1].name, bones[1].name);
            document
                .repair_constraint_frames_for_current_pose(true)
                .unwrap();
            assert!(document.constraint_frames_are_finite());
            document.validate().unwrap();
        }
    }

    #[test]
    fn hkrg_detection_rejects_cloth_packfiles() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/hkcl");
        if let Ok(entries) = fs::read_dir(&directory) {
            for entry in entries.flatten().take(3) {
                let bytes = fs::read(entry.path()).unwrap();
                assert!(!HkrgDocument::is_ragdoll(&bytes));
                assert!(HkrgDocument::parse(&bytes).is_err());
            }
        }
        for (_, document) in corpus().into_iter().take(1) {
            assert!(HkrgDocument::is_ragdoll(&document.raw));
        }
    }

    #[test]
    fn matrix_helpers_match_the_row_vector_convention() {
        let translation = translation_matrix([1.0, 2.0, 3.0]);
        assert_eq!(
            transform_point([0.0, 0.0, 0.0], &translation),
            [1.0, 2.0, 3.0]
        );
        let rotation = quaternion_matrix(normalize4([0.0, 0.0, (0.5f32).sqrt(), (0.5f32).sqrt()]));
        let point = transform_point([1.0, 0.0, 0.0], &rotation);
        assert!((point[0]).abs() < 1.0e-5 && (point[1] - 1.0).abs() < 1.0e-5);
        let inverse = invert(&multiply(&rotation, &translation)).unwrap();
        let back = transform_point(
            transform_point([4.0, 5.0, 6.0], &multiply(&rotation, &translation)),
            &inverse,
        );
        assert!(
            (back[0] - 4.0).abs() < 1.0e-4
                && (back[1] - 5.0).abs() < 1.0e-4
                && (back[2] - 6.0).abs() < 1.0e-4
        );
        let quaternion = normalize4(quaternion_from_matrix(&rotation));
        assert!((quaternion[2].abs() - (0.5f32).sqrt()).abs() < 1.0e-5);
        assert!(rotation_angle_degrees(&IDENTITY, &rotation) - 90.0 < 1.0e-3);
    }
}
