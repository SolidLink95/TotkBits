use super::{GlobalFixup, HkclHeader, HkclSection, Item, LocalFixup};
use crate::parser::binary::{BinaryReader, Endian};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    io,
    io::ErrorKind,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct ObjectKey {
    pub section_index: usize,
    pub offset: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Vector4(pub [f32; 4]);

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Matrix4(pub [f32; 16]);

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct QsTransform {
    pub translation: Vector4,
    pub rotation: Vector4,
    pub scale: Vector4,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Bone {
    pub name: Option<String>,
    pub parent_index: i16,
    pub lock_translation: bool,
    pub reference_pose: Option<QsTransform>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Skeleton {
    pub key: ObjectKey,
    pub name: Option<String>,
    pub bones: Vec<Bone>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Particle {
    pub mass: f32,
    pub inverse_mass: f32,
    pub radius: f32,
    pub friction: f32,
    pub fixed: bool,
    pub position: Option<Vector4>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConstraintElement {
    pub particles: Vec<u16>,
    pub values: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Constraint {
    pub key: ObjectKey,
    pub class_name: String,
    pub name: Option<String>,
    pub element_count: usize,
    pub elements: Vec<ConstraintElement>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SimCloth {
    pub key: ObjectKey,
    pub name: Option<String>,
    pub gravity: Vector4,
    pub total_mass: f32,
    pub collision_tolerance: f32,
    pub max_particle_radius: f32,
    pub particles: Vec<Particle>,
    pub triangle_indices: Vec<u16>,
    pub collidables: Vec<ObjectKey>,
    pub constraints: Vec<ObjectKey>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cloth {
    pub key: ObjectKey,
    pub name: Option<String>,
    pub target_platform: u32,
    pub simulations: Vec<SimCloth>,
    pub buffer_definition_count: usize,
    pub transform_set_definition_count: usize,
    pub operator_count: usize,
    pub state_count: usize,
    pub action_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Collidable {
    pub key: ObjectKey,
    pub name: Option<String>,
    pub transform: Matrix4,
    pub linear_velocity: Vector4,
    pub angular_velocity: Vector4,
    pub pinch_detection_enabled: bool,
    pub pinch_detection_priority: i8,
    pub pinch_detection_radius: f32,
    pub shape: Option<ObjectKey>,
    pub shape_class: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PhysicsGraph {
    pub skeletons: Vec<Skeleton>,
    pub cloths: Vec<Cloth>,
    pub constraints: Vec<Constraint>,
    pub collidables: Vec<Collidable>,
}

impl PhysicsGraph {
    pub(super) fn validate(&self, objects: &HashSet<ObjectKey>) -> io::Result<()> {
        let constraint_keys: HashSet<_> = self.constraints.iter().map(|value| value.key).collect();
        let collidable_keys: HashSet<_> = self.collidables.iter().map(|value| value.key).collect();

        for skeleton in &self.skeletons {
            require_object(objects, skeleton.key, "skeleton")?;
            for (index, bone) in skeleton.bones.iter().enumerate() {
                if bone.parent_index < -1
                    || bone.parent_index >= 0 && bone.parent_index as usize >= index
                {
                    return Err(invalid(&format!(
                        "skeleton bone {index} has invalid parent {}",
                        bone.parent_index
                    )));
                }
                if let Some(pose) = bone.reference_pose {
                    require_finite(&pose.translation.0, "skeleton translation")?;
                    require_finite(&pose.rotation.0, "skeleton rotation")?;
                    require_finite(&pose.scale.0, "skeleton scale")?;
                }
            }
        }

        for constraint in &self.constraints {
            require_object(objects, constraint.key, "constraint")?;
            if supports_constraint_elements(&constraint.class_name)
                && constraint.element_count != constraint.elements.len()
            {
                return Err(invalid(&format!(
                    "{} declares {} elements but parsed {}",
                    constraint.class_name,
                    constraint.element_count,
                    constraint.elements.len()
                )));
            }
            for element in &constraint.elements {
                require_finite(&element.values, "constraint values")?;
            }
        }

        for collidable in &self.collidables {
            require_object(objects, collidable.key, "collidable")?;
            require_finite(&collidable.transform.0, "collidable transform")?;
            require_finite(&collidable.linear_velocity.0, "collidable linear velocity")?;
            require_finite(
                &collidable.angular_velocity.0,
                "collidable angular velocity",
            )?;
            if let Some(shape) = collidable.shape {
                require_object(objects, shape, "collidable shape")?;
            }
        }

        for cloth in &self.cloths {
            require_object(objects, cloth.key, "cloth")?;
            let mut simulations = HashSet::new();
            for simulation in &cloth.simulations {
                require_object(objects, simulation.key, "simulation")?;
                if !simulations.insert(simulation.key) {
                    return Err(invalid("cloth references a simulation more than once"));
                }
                require_finite(&simulation.gravity.0, "simulation gravity")?;
                require_finite(
                    &[
                        simulation.total_mass,
                        simulation.collision_tolerance,
                        simulation.max_particle_radius,
                    ],
                    "simulation values",
                )?;
                for particle in &simulation.particles {
                    require_finite(
                        &[
                            particle.mass,
                            particle.inverse_mass,
                            particle.radius,
                            particle.friction,
                        ],
                        "particle values",
                    )?;
                    if let Some(position) = particle.position {
                        require_finite(&position.0, "particle position")?;
                    }
                }
                if simulation.triangle_indices.len() % 3 != 0 {
                    return Err(invalid(
                        "simulation triangle index count is not divisible by 3",
                    ));
                }
                if simulation
                    .triangle_indices
                    .iter()
                    .any(|index| *index as usize >= simulation.particles.len())
                {
                    return Err(invalid("simulation triangle references a missing particle"));
                }
                for key in &simulation.collidables {
                    if !collidable_keys.contains(key) {
                        return Err(invalid("simulation references a missing collidable"));
                    }
                }
                for key in &simulation.constraints {
                    let constraint = self
                        .constraints
                        .iter()
                        .find(|constraint| constraint.key == *key)
                        .ok_or_else(|| invalid("simulation references a missing constraint"))?;
                    if !constraint_keys.contains(key) {
                        return Err(invalid("simulation references a missing constraint"));
                    }
                    let checked_indices = constraint_particle_count(&constraint.class_name);
                    if constraint.elements.iter().any(|element| {
                        element
                            .particles
                            .iter()
                            .take(checked_indices)
                            .any(|index| *index as usize >= simulation.particles.len())
                    }) {
                        return Err(invalid("constraint references a missing particle"));
                    }
                }
            }
        }
        Ok(())
    }
}

fn require_object(objects: &HashSet<ObjectKey>, key: ObjectKey, kind: &str) -> io::Result<()> {
    if objects.contains(&key) {
        Ok(())
    } else {
        Err(invalid(&format!("{kind} is not backed by an HKCL object")))
    }
}

fn require_finite(values: &[f32], kind: &str) -> io::Result<()> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(invalid(&format!("{kind} contain a non-finite value")))
    }
}

fn supports_constraint_elements(class_name: &str) -> bool {
    matches!(
        class_name,
        "hclStandardLinkConstraintSet"
            | "hclStretchLinkConstraintSet"
            | "hclBendLinkConstraintSet"
            | "hclBendStiffnessConstraintSet"
            | "hclLocalRangeConstraintSet"
            | "hclTransitionConstraintSet"
            | "hclBonePlanesConstraintSet"
    )
}

fn constraint_particle_count(class_name: &str) -> usize {
    match class_name {
        "hclLocalRangeConstraintSet"
        | "hclTransitionConstraintSet"
        | "hclBonePlanesConstraintSet" => 1,
        _ => usize::MAX,
    }
}

pub(super) fn parse_physics_graph(
    raw: &[u8],
    header: &HkclHeader,
    sections: &[HkclSection],
    items: &[Item],
    type_names: &[String],
    local_fixups: &[LocalFixup],
    global_fixups: &[GlobalFixup],
) -> io::Result<PhysicsGraph> {
    if !sections.iter().any(|section| section.tag == "__data__") {
        return Ok(PhysicsGraph::default());
    }
    let reader = GraphReader::new(raw, header, sections, local_fixups, global_fixups);
    let objects: HashMap<_, _> = items
        .iter()
        .map(|item| {
            (
                ObjectKey {
                    section_index: item.data_section_index,
                    offset: item.data_offset,
                },
                type_names
                    .get(item.type_index as usize)
                    .cloned()
                    .unwrap_or_default(),
            )
        })
        .collect();

    let mut constraints = objects
        .iter()
        // Only hcl cloth constraint sets; ragdoll packfiles carry hkp
        // constraint classes with unrelated layouts.
        .filter(|(_, class)| class.starts_with("hcl") && class.contains("Constraint"))
        .map(|(key, class)| reader.constraint(*key, class))
        .collect::<io::Result<Vec<_>>>()?;
    let mut collidables = objects
        .iter()
        .filter(|(_, class)| class.as_str() == "hclCollidable")
        .map(|(key, _)| reader.collidable(*key, &objects))
        .collect::<io::Result<Vec<_>>>()?;
    let mut skeletons = objects
        .iter()
        .filter(|(_, class)| class.as_str() == "hkaSkeleton")
        .map(|(key, _)| reader.skeleton(*key))
        .collect::<io::Result<Vec<_>>>()?;
    let simulations: HashMap<_, _> = objects
        .iter()
        .filter(|(_, class)| class.as_str() == "hclSimClothData")
        .map(|(key, _)| reader.sim_cloth(*key).map(|simulation| (*key, simulation)))
        .collect::<io::Result<_>>()?;
    let mut cloths = objects
        .iter()
        .filter(|(_, class)| class.as_str() == "hclClothData")
        .map(|(key, _)| reader.cloth(*key, &simulations))
        .collect::<io::Result<Vec<_>>>()?;

    constraints.sort_by_key(|value| (value.key.section_index, value.key.offset));
    collidables.sort_by_key(|value| (value.key.section_index, value.key.offset));
    skeletons.sort_by_key(|value| (value.key.section_index, value.key.offset));
    cloths.sort_by_key(|value| (value.key.section_index, value.key.offset));

    Ok(PhysicsGraph {
        skeletons,
        cloths,
        constraints,
        collidables,
    })
}

struct GraphReader<'a> {
    raw: &'a [u8],
    sections: &'a [HkclSection],
    endian: Endian,
    pointer_size: usize,
    local: HashMap<(usize, u32), ObjectKey>,
    global: HashMap<(usize, u32), ObjectKey>,
}

impl<'a> GraphReader<'a> {
    fn new(
        raw: &'a [u8],
        header: &HkclHeader,
        sections: &'a [HkclSection],
        local: &[LocalFixup],
        global: &[GlobalFixup],
    ) -> Self {
        Self {
            raw,
            sections,
            endian: header.layout.endian,
            pointer_size: header.layout.pointer_size as usize,
            local: local
                .iter()
                .map(|fixup| {
                    (
                        (fixup.section_index, fixup.source_offset),
                        ObjectKey {
                            section_index: fixup.section_index,
                            offset: fixup.destination_offset,
                        },
                    )
                })
                .collect(),
            global: global
                .iter()
                .map(|fixup| {
                    (
                        (fixup.section_index, fixup.source_offset),
                        ObjectKey {
                            section_index: fixup.destination_section_index,
                            offset: fixup.destination_offset,
                        },
                    )
                })
                .collect(),
        }
    }

    fn pointer(&self, key: ObjectKey, field: usize) -> Option<ObjectKey> {
        let source = key.offset.checked_add(field as u32)?;
        self.local
            .get(&(key.section_index, source))
            .or_else(|| self.global.get(&(key.section_index, source)))
            .copied()
    }

    fn bytes(&self, key: ObjectKey, field: usize, len: usize) -> io::Result<&'a [u8]> {
        let section = self
            .sections
            .get(key.section_index)
            .ok_or_else(|| invalid("HKCL object section is out of range"))?;
        let relative = usize::try_from(key.offset)
            .ok()
            .and_then(|offset| offset.checked_add(field))
            .ok_or_else(|| invalid("HKCL object offset overflows"))?;
        let start = section
            .absolute_data_start
            .checked_add(relative)
            .ok_or_else(|| invalid("HKCL object offset overflows"))?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| invalid("HKCL object read overflows"))?;
        if end > section.local_fixups.start {
            return Err(invalid("HKCL object read exceeds section DATA"));
        }
        self.raw
            .get(start..end)
            .ok_or_else(|| invalid("HKCL object read exceeds input"))
    }

    fn u16(&self, key: ObjectKey, field: usize) -> io::Result<u16> {
        let source = self.bytes(key, field, 2)?;
        BinaryReader::with_endian(source, self.endian).read_u16_at(0)
    }

    fn i16(&self, key: ObjectKey, field: usize) -> io::Result<i16> {
        Ok(self.u16(key, field)? as i16)
    }

    fn u32(&self, key: ObjectKey, field: usize) -> io::Result<u32> {
        let source = self.bytes(key, field, 4)?;
        BinaryReader::with_endian(source, self.endian).read_u32_at(0)
    }

    fn f32(&self, key: ObjectKey, field: usize) -> io::Result<f32> {
        Ok(f32::from_bits(self.u32(key, field)?))
    }

    fn vector4(&self, key: ObjectKey, field: usize) -> io::Result<Vector4> {
        Ok(Vector4([
            self.f32(key, field)?,
            self.f32(key, field + 4)?,
            self.f32(key, field + 8)?,
            self.f32(key, field + 12)?,
        ]))
    }

    fn matrix4(&self, key: ObjectKey, field: usize) -> io::Result<Matrix4> {
        let mut values = [0.0; 16];
        for (index, value) in values.iter_mut().enumerate() {
            *value = self.f32(key, field + index * 4)?;
        }
        Ok(Matrix4(values))
    }

    fn string(&self, key: ObjectKey, field: usize) -> io::Result<Option<String>> {
        let Some(target) = self.pointer(key, field) else {
            return Ok(None);
        };
        let section = &self.sections[target.section_index];
        let start = section.absolute_data_start + target.offset as usize;
        let bytes = self
            .raw
            .get(start..section.local_fixups.start)
            .ok_or_else(|| invalid("HKCL string pointer exceeds section"))?;
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        Ok(Some(String::from_utf8_lossy(&bytes[..end]).into_owned()))
    }

    fn array(&self, key: ObjectKey, field: usize) -> io::Result<(Option<ObjectKey>, usize)> {
        let count = self.u32(key, field + self.pointer_size)? as usize;
        if count > 1_000_000 {
            return Err(invalid("HKCL array count is unreasonable"));
        }
        Ok((self.pointer(key, field), count))
    }

    fn pointer_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<ObjectKey>> {
        let (storage, count) = self.array(key, field)?;
        let Some(storage) = storage else {
            return Ok(Vec::new());
        };
        Ok((0..count)
            .filter_map(|index| self.pointer(storage, index * self.pointer_size))
            .collect())
    }

    fn skeleton(&self, key: ObjectKey) -> io::Result<Skeleton> {
        let descriptor = self.pointer_size + 8;
        let base = self.referenced_size();
        let parents_field = base + self.pointer_size;
        let bones_field = parents_field + descriptor;
        let pose_field = bones_field + descriptor;
        let (parents_storage, parent_count) = self.array(key, parents_field)?;
        let parents = if let Some(storage) = parents_storage {
            (0..parent_count)
                .map(|index| self.i16(storage, index * 2))
                .collect::<io::Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let (bone_storage, bone_count) = self.array(key, bones_field)?;
        let bone_stride = self.pointer_size * 2;
        let mut bones = Vec::with_capacity(bone_count);
        if let Some(storage) = bone_storage {
            for index in 0..bone_count {
                let bone = ObjectKey {
                    section_index: storage.section_index,
                    offset: storage.offset + (index * bone_stride) as u32,
                };
                bones.push(Bone {
                    name: self.string(bone, 0)?,
                    parent_index: parents.get(index).copied().unwrap_or(-1),
                    lock_translation: self.bytes(bone, self.pointer_size, 1)?[0] != 0,
                    reference_pose: None,
                });
            }
        }
        let (pose_storage, pose_count) = self.array(key, pose_field)?;
        if let Some(storage) = pose_storage {
            for (index, bone) in bones.iter_mut().take(pose_count).enumerate() {
                let field = index * 48;
                bone.reference_pose = Some(QsTransform {
                    translation: self.vector4(storage, field)?,
                    rotation: self.vector4(storage, field + 16)?,
                    scale: self.vector4(storage, field + 32)?,
                });
            }
        }
        Ok(Skeleton {
            key,
            name: self.string(key, base)?,
            bones,
        })
    }

    fn sim_cloth(&self, key: ObjectKey) -> io::Result<SimCloth> {
        let descriptor = self.pointer_size + 8;
        let simulation_info = align(self.pointer_size, 16);
        let gravity = self.vector4(key, simulation_info)?;
        let collision_tolerance = self.f32(key, simulation_info + 20)?;
        let name_field = simulation_info + 32;
        let particle_field = name_field + self.pointer_size;
        let fixed_field = particle_field + descriptor;
        let triangle_field = fixed_field + descriptor;
        let flips_field = triangle_field + descriptor;
        let total_mass_field = flips_field + descriptor;
        let map_field = total_mass_field + 4 + if self.pointer_size == 8 { 4 } else { 0 };
        let collidable_field = map_field + 4 + descriptor * 2;
        let constraints_field = collidable_field + descriptor;
        let anti_pinch_field = constraints_field + descriptor;
        let poses_field = anti_pinch_field + descriptor;
        let actions_field = poses_field + descriptor;
        let masks_field = actions_field + descriptor;
        let pinch_flags_field = masks_field + descriptor;
        let pinching_field = pinch_flags_field + descriptor;
        let max_radius_field = pinching_field + descriptor + 8;

        let (particle_storage, particle_count) = self.array(key, particle_field)?;
        let fixed = self.u16_array(key, fixed_field)?;
        let positions = self
            .pointer_array(key, poses_field)?
            .first()
            .copied()
            .map(|pose| self.vector4_array(pose, self.referenced_size() + self.pointer_size))
            .transpose()?
            .unwrap_or_default();
        let mut particles = Vec::with_capacity(particle_count);
        if let Some(storage) = particle_storage {
            for index in 0..particle_count {
                let field = index * 16;
                particles.push(Particle {
                    mass: self.f32(storage, field)?,
                    inverse_mass: self.f32(storage, field + 4)?,
                    radius: self.f32(storage, field + 8)?,
                    friction: self.f32(storage, field + 12)?,
                    fixed: fixed.contains(&(index as u16)),
                    position: positions.get(index).copied(),
                });
            }
        }
        let mut constraints = self.pointer_array(key, constraints_field)?;
        constraints.extend(self.pointer_array(key, anti_pinch_field)?);
        Ok(SimCloth {
            key,
            name: self.string(key, name_field)?,
            gravity,
            total_mass: self.f32(key, total_mass_field)?,
            collision_tolerance,
            max_particle_radius: self.f32(key, max_radius_field)?,
            particles,
            triangle_indices: self.u16_array(key, triangle_field)?,
            collidables: self.pointer_array(key, collidable_field)?,
            constraints,
        })
    }

    fn cloth(
        &self,
        key: ObjectKey,
        simulations: &HashMap<ObjectKey, SimCloth>,
    ) -> io::Result<Cloth> {
        let descriptor = self.pointer_size + 8;
        let base = self.referenced_size();
        let simulation_field = base + self.pointer_size;
        let simulation_keys = self.pointer_array(key, simulation_field)?;
        Ok(Cloth {
            key,
            name: self.string(key, base)?,
            target_platform: self.u32(key, simulation_field + descriptor * 6)?,
            simulations: simulation_keys
                .iter()
                .filter_map(|key| simulations.get(key).cloned())
                .collect(),
            buffer_definition_count: self.array(key, simulation_field + descriptor)?.1,
            transform_set_definition_count: self.array(key, simulation_field + descriptor * 2)?.1,
            operator_count: self.array(key, simulation_field + descriptor * 3)?.1,
            state_count: self.array(key, simulation_field + descriptor * 4)?.1,
            action_count: self.array(key, simulation_field + descriptor * 5)?.1,
        })
    }

    fn constraint(&self, key: ObjectKey, class_name: &str) -> io::Result<Constraint> {
        let name_field = self.referenced_size();
        let field = name_field + self.pointer_size * 2;
        let (storage, count) = self.array(key, field)?;
        let (stride, particles_at, particle_count, values_at, value_count) = match class_name {
            "hclStandardLinkConstraintSet" | "hclStretchLinkConstraintSet" => (12, 0, 2, 4, 2),
            "hclBendLinkConstraintSet" => (20, 0, 2, 4, 4),
            "hclBendStiffnessConstraintSet" => (32, 24, 4, 0, 6),
            "hclLocalRangeConstraintSet" | "hclTransitionConstraintSet" => (16, 0, 2, 4, 3),
            "hclBonePlanesConstraintSet" => (32, 16, 2, 0, 5),
            _ => (0, 0, 0, 0, 0),
        };
        let mut elements = Vec::new();
        if let Some(storage) = storage.filter(|_| stride != 0) {
            for index in 0..count {
                let base = index * stride;
                elements.push(ConstraintElement {
                    particles: (0..particle_count)
                        .map(|particle| self.u16(storage, base + particles_at + particle * 2))
                        .collect::<io::Result<_>>()?,
                    values: (0..value_count)
                        .map(|value| self.f32(storage, base + values_at + value * 4))
                        .collect::<io::Result<_>>()?,
                });
            }
        }
        Ok(Constraint {
            key,
            class_name: class_name.to_owned(),
            name: self.string(key, name_field)?,
            element_count: count,
            elements,
        })
    }

    fn collidable(
        &self,
        key: ObjectKey,
        objects: &HashMap<ObjectKey, String>,
    ) -> io::Result<Collidable> {
        let transform_field = align(self.pointer_size * 2, 16);
        let linear_field = transform_field + 64;
        let angular_field = linear_field + 16;
        let flags_field = angular_field + 16;
        let shape_field = flags_field + 8;
        let shape = self.pointer(key, shape_field);
        Ok(Collidable {
            key,
            name: self.string(key, self.referenced_size())?,
            transform: self.matrix4(key, transform_field)?,
            linear_velocity: self.vector4(key, linear_field)?,
            angular_velocity: self.vector4(key, angular_field)?,
            pinch_detection_enabled: self.bytes(key, flags_field, 1)?[0] != 0,
            pinch_detection_priority: self.bytes(key, flags_field + 1, 1)?[0] as i8,
            pinch_detection_radius: self.f32(key, flags_field + 4)?,
            shape,
            shape_class: shape.and_then(|key| objects.get(&key).cloned()),
        })
    }

    fn u16_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<u16>> {
        let (storage, count) = self.array(key, field)?;
        let Some(storage) = storage else {
            return Ok(Vec::new());
        };
        (0..count)
            .map(|index| self.u16(storage, index * 2))
            .collect()
    }

    fn vector4_array(&self, key: ObjectKey, field: usize) -> io::Result<Vec<Vector4>> {
        let (storage, count) = self.array(key, field)?;
        let Some(storage) = storage else {
            return Ok(Vec::new());
        };
        (0..count)
            .map(|index| self.vector4(storage, index * 16))
            .collect()
    }

    fn referenced_size(&self) -> usize {
        self.pointer_size.max(8)
    }
}

fn align(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::physics::hkcl::HkclLayoutRules;
    use std::ops::Range;

    #[test]
    fn skeleton_layout_is_endian_aware() {
        for endian in [Endian::Little, Endian::Big] {
            let mut raw = vec![0u8; 128];
            raw[64..69].copy_from_slice(b"Skel\0");
            raw[88..93].copy_from_slice(b"Bone\0");
            write_u32(&mut raw, 16, 1, endian);
            write_u32(&mut raw, 28, 1, endian);
            write_u16(&mut raw, 72, (-2i16) as u16, endian);
            raw[84] = 1;
            let section = HkclSection {
                tag: "__data__".to_owned(),
                absolute_data_start: 0,
                local_fixups: 128..128,
                global_fixups: Range::default(),
                virtual_fixups: Range::default(),
                exports: Range::default(),
                imports: Range::default(),
                end: 128,
            };
            let header = HkclHeader {
                user_tag: 0,
                file_version: 11,
                layout: HkclLayoutRules {
                    pointer_size: 4,
                    endian,
                    reuse_padding_optimization: false,
                    empty_base_class_optimization: true,
                },
                section_count: 1,
                contents_section_index: None,
                contents_section_offset: 0,
                contents_class_name_section_index: None,
                contents_class_name_section_offset: 0,
                contents_version: String::new(),
                flags: 0,
                max_predicate: 0,
                predicate_array_size: 0,
            };
            let local = [
                LocalFixup {
                    section_index: 0,
                    source_offset: 8,
                    destination_offset: 64,
                },
                LocalFixup {
                    section_index: 0,
                    source_offset: 12,
                    destination_offset: 72,
                },
                LocalFixup {
                    section_index: 0,
                    source_offset: 24,
                    destination_offset: 80,
                },
                LocalFixup {
                    section_index: 0,
                    source_offset: 80,
                    destination_offset: 88,
                },
            ];
            let sections = [section];
            let reader = GraphReader::new(&raw, &header, &sections, &local, &[]);
            let skeleton = reader
                .skeleton(ObjectKey {
                    section_index: 0,
                    offset: 0,
                })
                .unwrap();
            assert_eq!(skeleton.name.as_deref(), Some("Skel"));
            assert_eq!(skeleton.bones.len(), 1);
            assert_eq!(skeleton.bones[0].name.as_deref(), Some("Bone"));
            assert_eq!(skeleton.bones[0].parent_index, -2);
            assert!(skeleton.bones[0].lock_translation);
        }
    }

    fn write_u16(raw: &mut [u8], offset: usize, value: u16, endian: Endian) {
        crate::parser::binary::BinaryPatcher::with_endian(raw, endian)
            .write_u16_at(offset, value)
            .unwrap();
    }

    fn write_u32(raw: &mut [u8], offset: usize, value: u32, endian: Endian) {
        crate::parser::binary::BinaryPatcher::with_endian(raw, endian)
            .write_u32_at(offset, value)
            .unwrap();
    }
}
