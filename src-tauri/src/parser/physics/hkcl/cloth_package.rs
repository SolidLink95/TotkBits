//! Complete reader for hk2014 cloth packfiles (BOTW `.hkcl`): every object
//! the TOTK converter needs, not just the neutral physics graph.
//!
//! Layouts are the 32-bit hk2014 packfile classes (8-byte referenced-object
//! header, 12-byte `hkArray`, 4-byte pointers); the file may be big-endian
//! (Wii U) or little-endian, the packfile reader handles both. Offsets were
//! measured on the BOTW armor corpus (`tmp/hkcl`).

use super::{
    physics::{GraphReader, ObjectKey},
    HkclDocument, Item,
};
use std::{
    collections::HashMap,
    io::{self, ErrorKind},
};

/// Element layout of a deformer blend-entry block, taken from the TOTK type
/// table so BOTW blocks (identical layouts) can be read and copied verbatim.
#[derive(Clone, Copy, Debug)]
pub struct BlockLayout {
    pub element_size: u32,
    pub vertex_count: u32,
    pub bones_per_vertex: u32,
    pub has_weights: bool,
}

/// Blend-block layouts per class, indexed by blend count (1..=8 for object
/// space, 1..=4 for bone space).
#[derive(Clone, Debug)]
pub struct BlendLayouts {
    pub object_space: HashMap<u32, BlockLayout>,
    pub bone_space: HashMap<u32, BlockLayout>,
}

#[derive(Clone, Debug, Default)]
pub struct ClothPackage {
    pub big_endian: bool,
    pub collidables: Vec<CollidableData>,
    pub skeletons: Vec<SkeletonData>,
    pub cloths: Vec<ClothData>,
}

#[derive(Clone, Debug)]
pub struct SkeletonData {
    pub key: ObjectKey,
    pub name: String,
    pub parent_indices: Vec<i16>,
    pub bones: Vec<(String, bool)>,
    /// translation, rotation, scale per bone
    pub reference_pose: Vec<[[f32; 4]; 3]>,
}

#[derive(Clone, Debug)]
pub enum ShapeData {
    Capsule {
        start: [f32; 4],
        end: [f32; 4],
        dir: [f32; 4],
        radius: f32,
        cap_len_sqrd_inv: f32,
    },
    Sphere([f32; 4]),
    Plane([f32; 4]),
}

#[derive(Clone, Debug)]
pub struct CollidableData {
    pub key: ObjectKey,
    pub name: String,
    pub transform: [f32; 16],
    pub linear_velocity: [f32; 4],
    pub angular_velocity: [f32; 4],
    pub pinch_detection_enabled: bool,
    pub pinch_detection_priority: i8,
    pub pinch_detection_radius: f32,
    pub shape: Option<ShapeData>,
}

#[derive(Clone, Debug)]
pub struct ClothData {
    pub key: ObjectKey,
    pub name: String,
    pub target_platform: u32,
    pub sim_cloths: Vec<SimClothData>,
    pub buffers: Vec<BufferDefinition>,
    pub transform_sets: Vec<TransformSetDefinition>,
    pub operators: Vec<Operator>,
    pub states: Vec<ClothState>,
}

#[derive(Clone, Debug)]
pub struct ParticleData {
    pub mass: f32,
    pub inv_mass: f32,
    pub radius: f32,
    pub friction: f32,
}

#[derive(Clone, Debug, Default)]
pub struct LandscapeCollisionData {
    pub landscape_radius: f32,
    pub enable_stuck_particle_detection: bool,
    pub stuck_particles_stretch_factor_sq: f32,
    pub pinch_detection_enabled: bool,
    pub pinch_detection_priority: i8,
    pub pinch_detection_radius: f32,
    pub collision_tolerance: f32,
}

#[derive(Clone, Debug, Default)]
pub struct TransferMotionData {
    pub transfer_translation_motion: bool,
    pub min_translation_speed: f32,
    pub max_translation_speed: f32,
    pub min_translation_blend: f32,
    pub max_translation_blend: f32,
    pub transfer_rotation_motion: bool,
    pub min_rotation_speed: f32,
    pub max_rotation_speed: f32,
    pub min_rotation_blend: f32,
    pub max_rotation_blend: f32,
}

#[derive(Clone, Debug)]
pub struct SimClothData {
    pub name: String,
    pub gravity: [f32; 4],
    pub global_damping_per_second: f32,
    pub collision_tolerance: f32,
    pub particles: Vec<ParticleData>,
    pub fixed_particles: Vec<u16>,
    pub triangle_indices: Vec<u16>,
    pub triangle_flips: Vec<u8>,
    pub total_mass: f32,
    pub collidable_transform_set_index: i32,
    pub collidable_transform_indices: Vec<u32>,
    pub collidable_offsets: Vec<[f32; 16]>,
    /// Indices into `ClothPackage::collidables`.
    pub per_instance_collidables: Vec<usize>,
    pub static_constraint_sets: Vec<ConstraintSet>,
    pub anti_pinch_constraint_sets: Vec<ConstraintSet>,
    pub poses: Vec<(String, Vec<[f32; 4]>)>,
    pub static_collision_masks: Vec<u32>,
    pub per_particle_pinch_detection_enabled_flags: Vec<u8>,
    pub collidable_pinching_datas: Vec<(bool, i8, f32)>,
    pub min_pinched_particle_index: u16,
    pub max_pinched_particle_index: u16,
    pub max_collision_pairs: u32,
    pub max_particle_radius: f32,
    pub landscape: LandscapeCollisionData,
    pub num_landscape_collidable_particles: u32,
    pub do_normals: bool,
    pub transfer_motion_enabled: bool,
    pub landscape_collision_enabled: bool,
    pub pinch_detection_enabled: bool,
    pub transfer_motion: TransferMotionData,
}

#[derive(Clone, Debug)]
pub struct ConstraintSet {
    pub class_name: String,
    pub name: String,
    pub constraint_id: u32,
    pub kind: ConstraintKind,
}

#[derive(Clone, Debug)]
pub enum ConstraintKind {
    /// particleA, particleB, restLength, stiffness
    StandardLinks(Vec<(u16, u16, f32, f32)>),
    StretchLinks(Vec<(u16, u16, f32, f32)>),
    /// particleA, particleB, bendMinLength, stretchMaxLength, bendStiffness, stretchStiffness
    BendLinks(Vec<(u16, u16, f32, f32, f32, f32)>),
    BendStiffness {
        /// weightA..D, bendStiffness, restCurvature, particleA..D
        links: Vec<([f32; 4], f32, f32, [u16; 4])>,
        max_rest_pose_height_sq: f32,
    },
    LocalRange {
        /// particleIndex, referenceVertex, shapeRadius, maxNormalDistance, minNormalDistance
        constraints: Vec<(u16, u16, f32, f32, f32)>,
        reference_mesh_buffer_idx: u32,
        stiffness: f32,
        shape_type: u32,
        apply_normal_component: bool,
    },
    Transition {
        /// particleIndex, referenceVertex, toAnimDelay, toSimDelay, toSimMaxDistance
        per_particle: Vec<(u16, u16, f32, f32, f32)>,
        to_anim_period: f32,
        to_anim_plus_delay_period: f32,
        to_sim_period: f32,
        to_sim_plus_delay_period: f32,
        reference_mesh_buffer_idx: u32,
    },
    /// particleA, particleB, restLength, compressionLength, stiffness
    CompressibleLinks(Vec<(u16, u16, f32, f32, f32)>),
    BonePlanes {
        /// planeEquationBone, particleIndex, transformIndex, stiffness
        planes: Vec<([f32; 4], u16, u16, f32)>,
        transform_set_index: u32,
    },
    Volume {
        /// frameVector, particleIndex, weight
        frame_datas: Vec<([f32; 4], u16, f32)>,
        /// frameVector, particleIndex, stiffness
        apply_datas: Vec<([f32; 4], u16, f32)>,
    },
    Unsupported,
}

#[derive(Clone, Debug)]
pub struct BufferDefinition {
    pub is_scratch: bool,
    pub name: String,
    pub buffer_type: i32,
    pub sub_type: i32,
    pub num_vertices: u32,
    pub num_triangles: u32,
    pub layout: [u8; 26],
    pub triangle_indices: Vec<u16>,
    pub store_normals: bool,
    pub store_tangents_and_bi_tangents: bool,
}

#[derive(Clone, Debug)]
pub struct TransformSetDefinition {
    pub name: String,
    pub set_type: i32,
    pub num_transforms: u32,
}

/// One deformer block: vertex indices, per-vertex bone indices and (for
/// blends of two or more bones) per-vertex weights, already byte-swapped.
#[derive(Clone, Debug)]
pub struct BlendBlock {
    pub vertex_indices: Vec<u16>,
    pub bone_indices: Vec<u16>,
    pub bone_weights: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct Deformer {
    /// Indexed by blend count.
    pub blocks: HashMap<u32, Vec<BlendBlock>>,
    pub control_bytes: Vec<u8>,
    pub start_vertex_index: u16,
    pub end_vertex_index: u16,
    pub partial_write: bool,
}

#[derive(Clone, Debug)]
pub struct SkinOperator {
    pub name: String,
    pub operator_id: u32,
    pub bone_space: bool,
    pub bone_from_skin_mesh_transforms: Vec<[f32; 16]>,
    pub transform_subset: Vec<u16>,
    pub output_buffer_index: u32,
    pub transform_set_index: u32,
    pub deformer: Deformer,
    /// Bone-space packed positions (`hkVector4` per vertex, blocks of 16).
    pub local_ps: Vec<[[f32; 4]; 16]>,
    pub local_unpacked_ps: Vec<[[f32; 4]; 16]>,
}

#[derive(Clone, Debug)]
pub enum Operator {
    Skin(SkinOperator),
    MoveParticles {
        name: String,
        operator_id: u32,
        vertex_particle_pairs: Vec<(u16, u16)>,
        sim_cloth_index: u32,
        ref_buffer_idx: u32,
    },
    Simulate {
        name: String,
        operator_id: u32,
        sim_cloth_index: u32,
        sub_steps: u32,
        number_of_solve_iterations: u32,
        constraint_execution: Vec<i32>,
        adapt_constraint_stiffness: bool,
    },
    MeshBone {
        name: String,
        operator_id: u32,
        input_buffer_idx: u32,
        output_transform_set_idx: u32,
        triangle_bone_pairs: Vec<(u16, u16)>,
        local_bone_transforms: Vec<[f32; 16]>,
    },
    CopyVertices {
        name: String,
        operator_id: u32,
        input_buffer_idx: u32,
        output_buffer_idx: u32,
        number_of_vertices: u32,
        start_vertex_in: u32,
        start_vertex_out: u32,
        copy_normals: bool,
    },
    GatherAllVertices {
        name: String,
        operator_id: u32,
        vertex_input_from_vertex_output: Vec<i16>,
        input_buffer_idx: u32,
        output_buffer_idx: u32,
        gather_normals: bool,
        partial_gather: bool,
    },
    Unsupported {
        class_name: String,
        name: String,
    },
}

impl Operator {
    pub fn name(&self) -> &str {
        match self {
            Operator::Skin(op) => &op.name,
            Operator::MoveParticles { name, .. }
            | Operator::Simulate { name, .. }
            | Operator::MeshBone { name, .. }
            | Operator::CopyVertices { name, .. }
            | Operator::GatherAllVertices { name, .. }
            | Operator::Unsupported { name, .. } => name,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BufferAccess {
    pub buffer_index: u32,
    pub per_component_flags: [u8; 4],
    pub triangles_read: bool,
    pub shadow_buffer_index: u32,
}

/// read / readBeforeWrite / written bit fields (words, numBits).
#[derive(Clone, Debug)]
pub struct TransformTracker {
    pub read: (Vec<u32>, i32),
    pub read_before_write: (Vec<u32>, i32),
    pub written: (Vec<u32>, i32),
}

#[derive(Clone, Debug)]
pub struct TransformSetAccess {
    pub transform_set_index: u32,
    pub per_component_flags: [u8; 2],
    pub trackers: Vec<TransformTracker>,
}

#[derive(Clone, Debug)]
pub struct ClothState {
    pub name: String,
    pub operators: Vec<u32>,
    pub used_buffers: Vec<BufferAccess>,
    pub used_transform_sets: Vec<TransformSetAccess>,
    pub used_sim_cloths: Vec<u32>,
}

const RO: usize = 8; // hkReferencedObject on 32-bit
const ARR: usize = 12; // hkArray on 32-bit

struct PackageReader<'a> {
    reader: GraphReader<'a>,
    classes: HashMap<ObjectKey, String>,
    layouts: &'a BlendLayouts,
}

impl<'a> PackageReader<'a> {
    fn class(&self, key: ObjectKey) -> &str {
        self.classes.get(&key).map(String::as_str).unwrap_or("")
    }

    fn u8(&self, key: ObjectKey, field: usize) -> io::Result<u8> {
        Ok(self.reader.bytes(key, field, 1)?[0])
    }

    fn bool(&self, key: ObjectKey, field: usize) -> io::Result<bool> {
        Ok(self.u8(key, field)? != 0)
    }

    fn name(&self, key: ObjectKey, field: usize) -> io::Result<String> {
        Ok(self.reader.string(key, field)?.unwrap_or_default())
    }

    fn storage(&self, key: ObjectKey, field: usize) -> io::Result<Option<(ObjectKey, usize)>> {
        let (storage, count) = self.reader.array(key, field)?;
        Ok(storage
            .filter(|_| count > 0)
            .map(|storage| (storage, count)))
    }

    fn u16_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<u16>> {
        self.reader.u16_array(key, field)
    }

    fn i16_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<i16>> {
        let Some((storage, count)) = self.storage(key, field)? else {
            return Ok(Vec::new());
        };
        (0..count)
            .map(|k| self.reader.i16(storage, k * 2))
            .collect()
    }

    fn u32_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<u32>> {
        let Some((storage, count)) = self.storage(key, field)? else {
            return Ok(Vec::new());
        };
        (0..count)
            .map(|k| self.reader.u32(storage, k * 4))
            .collect()
    }

    fn i32_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<i32>> {
        Ok(self
            .u32_array(key, field)?
            .into_iter()
            .map(|v| v as i32)
            .collect())
    }

    fn u8_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<u8>> {
        let Some((storage, count)) = self.storage(key, field)? else {
            return Ok(Vec::new());
        };
        Ok(self.reader.bytes(storage, 0, count)?.to_vec())
    }

    fn vec4(&self, key: ObjectKey, field: usize) -> io::Result<[f32; 4]> {
        Ok(self.reader.vector4(key, field)?.0)
    }

    fn vec4_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<[f32; 4]>> {
        Ok(self
            .reader
            .vector4_array(key, field)?
            .into_iter()
            .map(|v| v.0)
            .collect())
    }

    fn matrix_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<[f32; 16]>> {
        let Some((storage, count)) = self.storage(key, field)? else {
            return Ok(Vec::new());
        };
        (0..count)
            .map(|k| Ok(self.reader.matrix4(storage, k * 64)?.0))
            .collect()
    }

    fn u16_pairs(&self, key: ObjectKey, field: usize) -> io::Result<Vec<(u16, u16)>> {
        let Some((storage, count)) = self.storage(key, field)? else {
            return Ok(Vec::new());
        };
        (0..count)
            .map(|k| {
                Ok((
                    self.reader.u16(storage, k * 4)?,
                    self.reader.u16(storage, k * 4 + 2)?,
                ))
            })
            .collect()
    }

    fn pointers(&self, key: ObjectKey, field: usize) -> io::Result<Vec<ObjectKey>> {
        self.reader.pointer_array(key, field)
    }

    // ---- objects ----

    fn skeleton(&self, key: ObjectKey) -> io::Result<SkeletonData> {
        let parent_indices = self.i16_array(key, RO + 4)?;
        let mut bones = Vec::new();
        if let Some((storage, count)) = self.storage(key, RO + 4 + ARR)? {
            for k in 0..count {
                let bone = ObjectKey {
                    section_index: storage.section_index,
                    offset: storage.offset + (k * 8) as u32,
                };
                bones.push((self.name(bone, 0)?, self.bool(bone, 4)?));
            }
        }
        let mut reference_pose = Vec::new();
        if let Some((storage, count)) = self.storage(key, RO + 4 + ARR * 2)? {
            for k in 0..count {
                let base = k * 48;
                reference_pose.push([
                    self.vec4(storage, base)?,
                    self.vec4(storage, base + 16)?,
                    self.vec4(storage, base + 32)?,
                ]);
            }
        }
        Ok(SkeletonData {
            key,
            name: self.name(key, RO)?,
            parent_indices,
            bones,
            reference_pose,
        })
    }

    fn shape(&self, key: ObjectKey) -> io::Result<Option<ShapeData>> {
        Ok(match self.class(key) {
            "hclCapsuleShape" => Some(ShapeData::Capsule {
                start: self.vec4(key, 16)?,
                end: self.vec4(key, 32)?,
                dir: self.vec4(key, 48)?,
                radius: self.reader.f32(key, 64)?,
                cap_len_sqrd_inv: self.reader.f32(key, 68)?,
            }),
            "hclSphereShape" => Some(ShapeData::Sphere(self.vec4(key, 16)?)),
            "hclPlaneShape" => Some(ShapeData::Plane(self.vec4(key, 16)?)),
            _ => None,
        })
    }

    fn collidable(&self, key: ObjectKey) -> io::Result<CollidableData> {
        let shape = match self.reader.pointer(key, 120) {
            Some(shape) => self.shape(shape)?,
            None => None,
        };
        Ok(CollidableData {
            key,
            name: self.name(key, RO)?,
            transform: self.reader.matrix4(key, 16)?.0,
            linear_velocity: self.vec4(key, 80)?,
            angular_velocity: self.vec4(key, 96)?,
            pinch_detection_enabled: self.bool(key, 112)?,
            pinch_detection_priority: self.u8(key, 113)? as i8,
            pinch_detection_radius: self.reader.f32(key, 116)?,
            shape,
        })
    }

    fn constraint_set(&self, key: ObjectKey) -> io::Result<ConstraintSet> {
        let class_name = self.class(key).to_owned();
        let name = self.name(key, RO)?;
        let constraint_id = self.reader.u32(key, 12)?;
        let links_field = 16;
        let kind = match class_name.as_str() {
            "hclStandardLinkConstraintSet" | "hclStretchLinkConstraintSet" => {
                let mut links = Vec::new();
                if let Some((storage, count)) = self.storage(key, links_field)? {
                    for k in 0..count {
                        let b = k * 12;
                        links.push((
                            self.reader.u16(storage, b)?,
                            self.reader.u16(storage, b + 2)?,
                            self.reader.f32(storage, b + 4)?,
                            self.reader.f32(storage, b + 8)?,
                        ));
                    }
                }
                if class_name == "hclStandardLinkConstraintSet" {
                    ConstraintKind::StandardLinks(links)
                } else {
                    ConstraintKind::StretchLinks(links)
                }
            }
            "hclCompressibleLinkConstraintSet" => {
                let mut links = Vec::new();
                if let Some((storage, count)) = self.storage(key, links_field)? {
                    for k in 0..count {
                        let b = k * 16;
                        links.push((
                            self.reader.u16(storage, b)?,
                            self.reader.u16(storage, b + 2)?,
                            self.reader.f32(storage, b + 4)?,
                            self.reader.f32(storage, b + 8)?,
                            self.reader.f32(storage, b + 12)?,
                        ));
                    }
                }
                ConstraintKind::CompressibleLinks(links)
            }
            "hclVolumeConstraint" => {
                let mut lists = [Vec::new(), Vec::new()];
                for (list, field) in lists.iter_mut().zip([16, 28]) {
                    if let Some((storage, count)) = self.storage(key, field)? {
                        for k in 0..count {
                            let b = k * 32;
                            list.push((
                                self.reader.vector4(storage, b)?.0,
                                self.reader.u16(storage, b + 16)?,
                                self.reader.f32(storage, b + 20)?,
                            ));
                        }
                    }
                }
                let [frame_datas, apply_datas] = lists;
                ConstraintKind::Volume {
                    frame_datas,
                    apply_datas,
                }
            }
            "hclBonePlanesConstraintSet" => {
                let mut planes = Vec::new();
                if let Some((storage, count)) = self.storage(key, links_field)? {
                    for k in 0..count {
                        let b = k * 32;
                        planes.push((
                            self.reader.vector4(storage, b)?.0,
                            self.reader.u16(storage, b + 16)?,
                            self.reader.u16(storage, b + 18)?,
                            self.reader.f32(storage, b + 20)?,
                        ));
                    }
                }
                ConstraintKind::BonePlanes {
                    planes,
                    transform_set_index: self.reader.u32(key, 28)?,
                }
            }
            "hclBendLinkConstraintSet" => {
                let mut links = Vec::new();
                if let Some((storage, count)) = self.storage(key, links_field)? {
                    for k in 0..count {
                        let b = k * 20;
                        links.push((
                            self.reader.u16(storage, b)?,
                            self.reader.u16(storage, b + 2)?,
                            self.reader.f32(storage, b + 4)?,
                            self.reader.f32(storage, b + 8)?,
                            self.reader.f32(storage, b + 12)?,
                            self.reader.f32(storage, b + 16)?,
                        ));
                    }
                }
                ConstraintKind::BendLinks(links)
            }
            "hclBendStiffnessConstraintSet" => {
                let mut links = Vec::new();
                if let Some((storage, count)) = self.storage(key, links_field)? {
                    for k in 0..count {
                        let b = k * 32;
                        links.push((
                            [
                                self.reader.f32(storage, b)?,
                                self.reader.f32(storage, b + 4)?,
                                self.reader.f32(storage, b + 8)?,
                                self.reader.f32(storage, b + 12)?,
                            ],
                            self.reader.f32(storage, b + 16)?,
                            self.reader.f32(storage, b + 20)?,
                            [
                                self.reader.u16(storage, b + 24)?,
                                self.reader.u16(storage, b + 26)?,
                                self.reader.u16(storage, b + 28)?,
                                self.reader.u16(storage, b + 30)?,
                            ],
                        ));
                    }
                }
                ConstraintKind::BendStiffness {
                    links,
                    max_rest_pose_height_sq: self.reader.f32(key, 28)?,
                }
            }
            "hclLocalRangeConstraintSet" => {
                let mut constraints = Vec::new();
                if let Some((storage, count)) = self.storage(key, links_field)? {
                    for k in 0..count {
                        let b = k * 16;
                        constraints.push((
                            self.reader.u16(storage, b)?,
                            self.reader.u16(storage, b + 2)?,
                            self.reader.f32(storage, b + 4)?,
                            self.reader.f32(storage, b + 8)?,
                            self.reader.f32(storage, b + 12)?,
                        ));
                    }
                }
                ConstraintKind::LocalRange {
                    constraints,
                    reference_mesh_buffer_idx: self.reader.u32(key, 28)?,
                    stiffness: self.reader.f32(key, 32)?,
                    shape_type: self.reader.u32(key, 36)?,
                    apply_normal_component: self.bool(key, 40)?,
                }
            }
            "hclTransitionConstraintSet" => {
                let mut per_particle = Vec::new();
                if let Some((storage, count)) = self.storage(key, links_field)? {
                    for k in 0..count {
                        let b = k * 16;
                        per_particle.push((
                            self.reader.u16(storage, b)?,
                            self.reader.u16(storage, b + 2)?,
                            self.reader.f32(storage, b + 4)?,
                            self.reader.f32(storage, b + 8)?,
                            self.reader.f32(storage, b + 12)?,
                        ));
                    }
                }
                ConstraintKind::Transition {
                    per_particle,
                    to_anim_period: self.reader.f32(key, 28)?,
                    to_anim_plus_delay_period: self.reader.f32(key, 32)?,
                    to_sim_period: self.reader.f32(key, 36)?,
                    to_sim_plus_delay_period: self.reader.f32(key, 40)?,
                    reference_mesh_buffer_idx: self.reader.u32(key, 44)?,
                }
            }
            _ => ConstraintKind::Unsupported,
        };
        Ok(ConstraintSet {
            class_name,
            name,
            constraint_id,
            kind,
        })
    }

    fn sim_cloth(
        &self,
        key: ObjectKey,
        collidable_index: &HashMap<ObjectKey, usize>,
    ) -> io::Result<SimClothData> {
        let mut particles = Vec::new();
        if let Some((storage, count)) = self.storage(key, 52)? {
            for k in 0..count {
                let b = k * 16;
                particles.push(ParticleData {
                    mass: self.reader.f32(storage, b)?,
                    inv_mass: self.reader.f32(storage, b + 4)?,
                    radius: self.reader.f32(storage, b + 8)?,
                    friction: self.reader.f32(storage, b + 12)?,
                });
            }
        }
        let per_instance_collidables = self
            .pointers(key, 132)?
            .into_iter()
            .filter_map(|collidable| collidable_index.get(&collidable).copied())
            .collect();
        let static_constraint_sets = self
            .pointers(key, 144)?
            .into_iter()
            .map(|set| self.constraint_set(set))
            .collect::<io::Result<Vec<_>>>()?;
        let anti_pinch_constraint_sets = self
            .pointers(key, 156)?
            .into_iter()
            .map(|set| self.constraint_set(set))
            .collect::<io::Result<Vec<_>>>()?;
        let poses = self
            .pointers(key, 168)?
            .into_iter()
            .map(|pose| Ok((self.name(pose, RO)?, self.vec4_array(pose, RO + 4)?)))
            .collect::<io::Result<Vec<_>>>()?;
        let mut collidable_pinching_datas = Vec::new();
        if let Some((storage, count)) = self.storage(key, 216)? {
            for k in 0..count {
                let b = k * 8;
                collidable_pinching_datas.push((
                    self.bool(storage, b)?,
                    self.u8(storage, b + 1)? as i8,
                    self.reader.f32(storage, b + 4)?,
                ));
            }
        }
        Ok(SimClothData {
            name: self.name(key, 48)?,
            gravity: self.vec4(key, 16)?,
            global_damping_per_second: self.reader.f32(key, 32)?,
            collision_tolerance: self.reader.f32(key, 36)?,
            particles,
            fixed_particles: self.u16_array(key, 64)?,
            triangle_indices: self.u16_array(key, 76)?,
            triangle_flips: self.u8_array(key, 88)?,
            total_mass: self.reader.f32(key, 100)?,
            collidable_transform_set_index: self.reader.u32(key, 104)? as i32,
            collidable_transform_indices: self.u32_array(key, 108)?,
            collidable_offsets: self.matrix_array(key, 120)?,
            per_instance_collidables,
            static_constraint_sets,
            anti_pinch_constraint_sets,
            poses,
            static_collision_masks: self.u32_array(key, 192)?,
            per_particle_pinch_detection_enabled_flags: self.u8_array(key, 204)?,
            collidable_pinching_datas,
            min_pinched_particle_index: self.reader.u16(key, 228)?,
            max_pinched_particle_index: self.reader.u16(key, 230)?,
            max_collision_pairs: self.reader.u32(key, 232)?,
            max_particle_radius: self.reader.f32(key, 236)?,
            landscape: LandscapeCollisionData {
                landscape_radius: self.reader.f32(key, 240)?,
                enable_stuck_particle_detection: self.bool(key, 244)?,
                stuck_particles_stretch_factor_sq: self.reader.f32(key, 248)?,
                pinch_detection_enabled: self.bool(key, 252)?,
                pinch_detection_priority: self.u8(key, 253)? as i8,
                pinch_detection_radius: self.reader.f32(key, 256)?,
                collision_tolerance: self.reader.f32(key, 260)?,
            },
            num_landscape_collidable_particles: self.reader.u32(key, 264)?,
            do_normals: self.bool(key, 268)?,
            transfer_motion_enabled: self.bool(key, 269)?,
            landscape_collision_enabled: self.bool(key, 270)?,
            pinch_detection_enabled: self.bool(key, 271)?,
            transfer_motion: TransferMotionData {
                transfer_translation_motion: self.bool(key, 276)?,
                min_translation_speed: self.reader.f32(key, 280)?,
                max_translation_speed: self.reader.f32(key, 284)?,
                min_translation_blend: self.reader.f32(key, 288)?,
                max_translation_blend: self.reader.f32(key, 292)?,
                transfer_rotation_motion: self.bool(key, 296)?,
                min_rotation_speed: self.reader.f32(key, 300)?,
                max_rotation_speed: self.reader.f32(key, 304)?,
                min_rotation_blend: self.reader.f32(key, 308)?,
                max_rotation_blend: self.reader.f32(key, 312)?,
            },
        })
    }

    fn buffer(&self, key: ObjectKey) -> io::Result<BufferDefinition> {
        let is_scratch = self.class(key) == "hclScratchBufferDefinition";
        let mut layout = [0u8; 26];
        layout.copy_from_slice(self.reader.bytes(key, 28, 26)?);
        Ok(BufferDefinition {
            is_scratch,
            name: self.name(key, RO)?,
            buffer_type: self.reader.u32(key, 12)? as i32,
            sub_type: self.reader.u32(key, 16)? as i32,
            num_vertices: self.reader.u32(key, 20)?,
            num_triangles: self.reader.u32(key, 24)?,
            layout,
            triangle_indices: if is_scratch {
                self.u16_array(key, 56)?
            } else {
                Vec::new()
            },
            store_normals: if is_scratch {
                self.bool(key, 68)?
            } else {
                false
            },
            store_tangents_and_bi_tangents: if is_scratch {
                self.bool(key, 69)?
            } else {
                false
            },
        })
    }

    fn blend_blocks(
        &self,
        key: ObjectKey,
        field: usize,
        layout: BlockLayout,
    ) -> io::Result<Vec<BlendBlock>> {
        let Some((storage, count)) = self.storage(key, field)? else {
            return Ok(Vec::new());
        };
        let mut blocks = Vec::with_capacity(count);
        let vertices = layout.vertex_count as usize;
        let bones = (layout.vertex_count * layout.bones_per_vertex) as usize;
        for k in 0..count {
            let base = k * layout.element_size as usize;
            let vertex_indices = (0..vertices)
                .map(|v| self.reader.u16(storage, base + v * 2))
                .collect::<io::Result<Vec<_>>>()?;
            let bone_indices = (0..bones)
                .map(|b| self.reader.u16(storage, base + vertices * 2 + b * 2))
                .collect::<io::Result<Vec<_>>>()?;
            let bone_weights = if layout.has_weights {
                self.reader
                    .bytes(storage, base + vertices * 2 + bones * 2, bones)?
                    .to_vec()
            } else {
                Vec::new()
            };
            blocks.push(BlendBlock {
                vertex_indices,
                bone_indices,
                bone_weights,
            });
        }
        Ok(blocks)
    }

    fn deformer(
        &self,
        key: ObjectKey,
        base: usize,
        layouts: &HashMap<u32, BlockLayout>,
        kinds: &[u32],
    ) -> io::Result<Deformer> {
        let mut blocks = HashMap::new();
        for (slot, blend) in kinds.iter().enumerate() {
            let field = base + slot * ARR;
            let Some(layout) = layouts.get(blend) else {
                // The TOTK type table has no body for this block kind: it must
                // not be used by the source either.
                if self.storage(key, field)?.is_some() {
                    return Err(invalid(&format!(
                        "{blend}-blend deformer blocks are used but the TOTK type table has no layout for them"
                    )));
                }
                continue;
            };
            let list = self.blend_blocks(key, field, *layout)?;
            if !list.is_empty() {
                blocks.insert(*blend, list);
            }
        }
        let control = base + kinds.len() * ARR;
        Ok(Deformer {
            blocks,
            control_bytes: self.u8_array(key, control)?,
            start_vertex_index: self.reader.u16(key, control + ARR)?,
            end_vertex_index: self.reader.u16(key, control + ARR + 2)?,
            partial_write: self.bool(key, control + ARR + 4)?,
        })
    }

    fn vec4_blocks(&self, key: ObjectKey, field: usize) -> io::Result<Vec<[[f32; 4]; 16]>> {
        let Some((storage, count)) = self.storage(key, field)? else {
            return Ok(Vec::new());
        };
        (0..count)
            .map(|k| {
                let mut block = [[0.0f32; 4]; 16];
                for (v, slot) in block.iter_mut().enumerate() {
                    *slot = self.vec4(storage, k * 256 + v * 16)?;
                }
                Ok(block)
            })
            .collect()
    }

    fn operator(&self, key: ObjectKey) -> io::Result<Operator> {
        let class_name = self.class(key).to_owned();
        let name = self.name(key, RO)?;
        let operator_id = self.reader.u32(key, 12)?;
        Ok(match class_name.as_str() {
            "hclObjectSpaceSkinPOperator" | "hclObjectSpaceSkinOperator" => {
                Operator::Skin(SkinOperator {
                    name,
                    operator_id,
                    bone_space: false,
                    bone_from_skin_mesh_transforms: self.matrix_array(key, 16)?,
                    transform_subset: self.u16_array(key, 28)?,
                    output_buffer_index: self.reader.u32(key, 40)?,
                    transform_set_index: self.reader.u32(key, 44)?,
                    deformer: self.deformer(
                        key,
                        48,
                        &self.layouts.object_space,
                        &[8, 7, 6, 5, 4, 3, 2, 1],
                    )?,
                    local_ps: Vec::new(),
                    local_unpacked_ps: self.vec4_blocks(key, 176)?,
                })
            }
            "hclBoneSpaceSkinPOperator" | "hclBoneSpaceSkinOperator" => {
                Operator::Skin(SkinOperator {
                    name,
                    operator_id,
                    bone_space: true,
                    bone_from_skin_mesh_transforms: Vec::new(),
                    transform_subset: self.u16_array(key, 16)?,
                    output_buffer_index: self.reader.u32(key, 28)?,
                    transform_set_index: self.reader.u32(key, 32)?,
                    deformer: self.deformer(key, 36, &self.layouts.bone_space, &[4, 3, 2, 1])?,
                    local_ps: self.vec4_blocks(key, 104)?,
                    local_unpacked_ps: self.vec4_blocks(key, 116)?,
                })
            }
            "hclMoveParticlesOperator" => Operator::MoveParticles {
                name,
                operator_id,
                vertex_particle_pairs: self.u16_pairs(key, 16)?,
                sim_cloth_index: self.reader.u32(key, 28)?,
                ref_buffer_idx: self.reader.u32(key, 32)?,
            },
            "hclSimulateOperator" => Operator::Simulate {
                name,
                operator_id,
                sim_cloth_index: self.reader.u32(key, 16)?,
                sub_steps: self.reader.u32(key, 20)?,
                number_of_solve_iterations: self.reader.u32(key, 24)?,
                constraint_execution: self.i32_array(key, 28)?,
                adapt_constraint_stiffness: self.bool(key, 40)?,
            },
            "hclSimpleMeshBoneDeformOperator" => Operator::MeshBone {
                name,
                operator_id,
                input_buffer_idx: self.reader.u32(key, 16)?,
                output_transform_set_idx: self.reader.u32(key, 20)?,
                triangle_bone_pairs: self.u16_pairs(key, 24)?,
                local_bone_transforms: self.matrix_array(key, 36)?,
            },
            "hclCopyVerticesOperator" => Operator::CopyVertices {
                name,
                operator_id,
                input_buffer_idx: self.reader.u32(key, 16)?,
                output_buffer_idx: self.reader.u32(key, 20)?,
                number_of_vertices: self.reader.u32(key, 24)?,
                start_vertex_in: self.reader.u32(key, 28)?,
                start_vertex_out: self.reader.u32(key, 32)?,
                copy_normals: self.bool(key, 36)?,
            },
            "hclGatherAllVerticesOperator" => Operator::GatherAllVertices {
                name,
                operator_id,
                vertex_input_from_vertex_output: self.i16_array(key, 16)?,
                input_buffer_idx: self.reader.u32(key, 28)?,
                output_buffer_idx: self.reader.u32(key, 32)?,
                gather_normals: self.bool(key, 36)?,
                partial_gather: self.bool(key, 37)?,
            },
            _ => Operator::Unsupported { class_name, name },
        })
    }

    fn bit_field(&self, key: ObjectKey, field: usize) -> io::Result<(Vec<u32>, i32)> {
        Ok((
            self.u32_array(key, field)?,
            self.reader.u32(key, field + ARR)? as i32,
        ))
    }

    fn state(&self, key: ObjectKey) -> io::Result<ClothState> {
        let mut used_buffers = Vec::new();
        if let Some((storage, count)) = self.storage(key, 24)? {
            for k in 0..count {
                let b = k * 16;
                let flags = self.reader.bytes(storage, b + 4, 4)?;
                used_buffers.push(BufferAccess {
                    buffer_index: self.reader.u32(storage, b)?,
                    per_component_flags: [flags[0], flags[1], flags[2], flags[3]],
                    triangles_read: self.bool(storage, b + 8)?,
                    shadow_buffer_index: self.reader.u32(storage, b + 12)?,
                });
            }
        }
        let mut used_transform_sets = Vec::new();
        if let Some((storage, count)) = self.storage(key, 36)? {
            for k in 0..count {
                let b = k * 32;
                let flags = self.reader.bytes(storage, b + 4, 2)?;
                let entry = ObjectKey {
                    section_index: storage.section_index,
                    offset: storage.offset + b as u32,
                };
                let mut trackers = Vec::new();
                if let Some((tracker_storage, tracker_count)) = self.storage(entry, 8)? {
                    for t in 0..tracker_count {
                        let tracker = ObjectKey {
                            section_index: tracker_storage.section_index,
                            offset: tracker_storage.offset + (t * 48) as u32,
                        };
                        trackers.push(TransformTracker {
                            read: self.bit_field(tracker, 0)?,
                            read_before_write: self.bit_field(tracker, 16)?,
                            written: self.bit_field(tracker, 32)?,
                        });
                    }
                }
                used_transform_sets.push(TransformSetAccess {
                    transform_set_index: self.reader.u32(storage, b)?,
                    per_component_flags: [flags[0], flags[1]],
                    trackers,
                });
            }
        }
        Ok(ClothState {
            name: self.name(key, RO)?,
            operators: self.u32_array(key, 12)?,
            used_buffers,
            used_transform_sets,
            used_sim_cloths: self.u32_array(key, 48)?,
        })
    }

    fn cloth(
        &self,
        key: ObjectKey,
        collidable_index: &HashMap<ObjectKey, usize>,
    ) -> io::Result<ClothData> {
        Ok(ClothData {
            key,
            name: self.name(key, RO)?,
            target_platform: self.reader.u32(key, 84)?,
            sim_cloths: self
                .pointers(key, 12)?
                .into_iter()
                .map(|sim| self.sim_cloth(sim, collidable_index))
                .collect::<io::Result<_>>()?,
            buffers: self
                .pointers(key, 24)?
                .into_iter()
                .map(|buffer| self.buffer(buffer))
                .collect::<io::Result<_>>()?,
            transform_sets: self
                .pointers(key, 36)?
                .into_iter()
                .map(|set| {
                    Ok(TransformSetDefinition {
                        name: self.name(set, RO)?,
                        set_type: self.reader.u32(set, 12)? as i32,
                        num_transforms: self.reader.u32(set, 16)?,
                    })
                })
                .collect::<io::Result<_>>()?,
            operators: self
                .pointers(key, 48)?
                .into_iter()
                .map(|op| self.operator(op))
                .collect::<io::Result<_>>()?,
            states: self
                .pointers(key, 60)?
                .into_iter()
                .map(|state| self.state(state))
                .collect::<io::Result<_>>()?,
        })
    }
}

impl HkclDocument {
    /// Reads the whole cloth package. `layouts` supplies the deformer block
    /// element layouts (from the TOTK type table).
    pub fn cloth_package(&self, layouts: &BlendLayouts) -> io::Result<ClothPackage> {
        if self.header.layout.pointer_size != 4 {
            return Err(invalid(
                "only 32-bit hk2014 cloth packfiles (BOTW Wii U / Switch) are supported",
            ));
        }
        let reader = GraphReader::new(
            &self.raw,
            &self.header,
            &self.sections,
            &self.local_fixups,
            &self.global_fixups,
        );
        let classes: HashMap<ObjectKey, String> = self
            .items
            .iter()
            .map(|item: &Item| {
                (
                    ObjectKey {
                        section_index: item.data_section_index,
                        offset: item.data_offset,
                    },
                    self.type_names
                        .get(item.type_index as usize)
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .collect();
        let package_reader = PackageReader {
            reader,
            classes,
            layouts,
        };
        let container = package_reader
            .classes
            .iter()
            .find(|(_, class)| class.as_str() == "hclClothContainer")
            .map(|(key, _)| *key)
            .ok_or_else(|| invalid("HKCL has no hclClothContainer"))?;
        let collidable_keys = package_reader.pointers(container, RO)?;
        let collidable_index: HashMap<ObjectKey, usize> = collidable_keys
            .iter()
            .enumerate()
            .map(|(index, key)| (*key, index))
            .collect();
        let collidables = collidable_keys
            .iter()
            .map(|key| package_reader.collidable(*key))
            .collect::<io::Result<Vec<_>>>()?;
        let cloths = package_reader
            .pointers(container, RO + ARR)?
            .into_iter()
            .map(|key| package_reader.cloth(key, &collidable_index))
            .collect::<io::Result<Vec<_>>>()?;
        let mut skeleton_keys: Vec<ObjectKey> = package_reader
            .classes
            .iter()
            .filter(|(_, class)| class.as_str() == "hkaSkeleton")
            .map(|(key, _)| *key)
            .collect();
        skeleton_keys.sort_by_key(|key| (key.section_index, key.offset));
        let skeletons = skeleton_keys
            .into_iter()
            .map(|key| package_reader.skeleton(key))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(ClothPackage {
            big_endian: matches!(
                self.header.layout.endian,
                crate::parser::binary::Endian::Big
            ),
            collidables,
            skeletons,
            cloths,
        })
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}
