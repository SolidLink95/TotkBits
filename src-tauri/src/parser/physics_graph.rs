use crate::parser::{bphcl, hkcl};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PhysicsFormat {
    Hkcl,
    Bphcl,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PhysicsId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "format", rename_all = "lowercase")]
pub enum SourceRef {
    Hkcl { section: usize, offset: u32 },
    Bphcl { item: usize },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Transform {
    pub translation: [f32; 4],
    pub rotation: [f32; 4],
    pub scale: [f32; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhysicsBone {
    pub name: Option<String>,
    pub parent_index: Option<usize>,
    pub lock_translation: bool,
    pub transform: Option<Transform>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhysicsSkeleton {
    pub id: PhysicsId,
    pub source: SourceRef,
    pub name: Option<String>,
    pub bones: Vec<PhysicsBone>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhysicsParticle {
    pub position: Option<[f32; 4]>,
    pub fixed: bool,
    pub mass: f32,
    pub inverse_mass: f32,
    pub radius: f32,
    pub friction: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhysicsConstraintElement {
    pub particles: Vec<u16>,
    pub values: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhysicsConstraint {
    pub id: PhysicsId,
    pub source: SourceRef,
    pub class_name: Option<String>,
    pub name: Option<String>,
    pub elements: Vec<PhysicsConstraintElement>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "shape", rename_all = "camelCase")]
pub enum PhysicsShape {
    Capsule {
        start: [f32; 4],
        end: [f32; 4],
        radius: f32,
    },
    Sphere {
        center: [f32; 4],
        radius: f32,
    },
    TaperedCapsule {
        start: [f32; 4],
        end: [f32; 4],
        start_radius: f32,
        end_radius: f32,
    },
    Plane {
        equation: [f32; 4],
    },
    Referenced {
        source: SourceRef,
        class_name: Option<String>,
    },
    Unknown {
        class_name: String,
        kind: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhysicsCollidable {
    pub id: PhysicsId,
    pub source: SourceRef,
    pub name: Option<String>,
    pub transform: Option<[f32; 16]>,
    pub translation: Option<[f32; 4]>,
    pub axes: Option<[[f32; 4]; 3]>,
    pub linear_velocity: Option<[f32; 4]>,
    pub angular_velocity: Option<[f32; 4]>,
    pub enabled: bool,
    pub pinch_detection: Option<(i8, f32)>,
    pub shape: Option<PhysicsShape>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhysicsSimulation {
    pub id: PhysicsId,
    pub source: SourceRef,
    pub name: Option<String>,
    pub gravity: Option<[f32; 4]>,
    pub total_mass: Option<f32>,
    pub collision_tolerance: Option<f32>,
    pub max_particle_radius: Option<f32>,
    pub particles: Vec<PhysicsParticle>,
    pub triangle_indices: Vec<u16>,
    pub constraints: Vec<PhysicsId>,
    pub collidables: Vec<PhysicsId>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhysicsCloth {
    pub id: PhysicsId,
    pub source: SourceRef,
    pub name: Option<String>,
    pub target_platform: Option<u32>,
    pub simulations: Vec<PhysicsSimulation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SkeletonBinding {
    pub cloth: PhysicsId,
    pub skeleton: PhysicsId,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FormatNeutralPhysicsGraph {
    pub source_format: PhysicsFormat,
    pub skeletons: Vec<PhysicsSkeleton>,
    pub cloths: Vec<PhysicsCloth>,
    pub constraints: Vec<PhysicsConstraint>,
    pub collidables: Vec<PhysicsCollidable>,
    pub skeleton_bindings: Vec<SkeletonBinding>,
}

impl From<&hkcl::PhysicsGraph> for FormatNeutralPhysicsGraph {
    fn from(graph: &hkcl::PhysicsGraph) -> Self {
        let id = |key: hkcl::ObjectKey| {
            PhysicsId(format!("hkcl:{}:{:08x}", key.section_index, key.offset))
        };
        let source = |key: hkcl::ObjectKey| SourceRef::Hkcl {
            section: key.section_index,
            offset: key.offset,
        };
        Self {
            source_format: PhysicsFormat::Hkcl,
            skeletons: graph
                .skeletons
                .iter()
                .map(|value| PhysicsSkeleton {
                    id: id(value.key),
                    source: source(value.key),
                    name: value.name.clone(),
                    bones: value
                        .bones
                        .iter()
                        .map(|bone| PhysicsBone {
                            name: bone.name.clone(),
                            parent_index: usize::try_from(bone.parent_index).ok(),
                            lock_translation: bone.lock_translation,
                            transform: bone.reference_pose.map(|pose| Transform {
                                translation: pose.translation.0,
                                rotation: pose.rotation.0,
                                scale: pose.scale.0,
                            }),
                        })
                        .collect(),
                })
                .collect(),
            cloths: graph
                .cloths
                .iter()
                .map(|cloth| PhysicsCloth {
                    id: id(cloth.key),
                    source: source(cloth.key),
                    name: cloth.name.clone(),
                    target_platform: Some(cloth.target_platform),
                    simulations: cloth
                        .simulations
                        .iter()
                        .map(|sim| PhysicsSimulation {
                            id: id(sim.key),
                            source: source(sim.key),
                            name: sim.name.clone(),
                            gravity: Some(sim.gravity.0),
                            total_mass: Some(sim.total_mass),
                            collision_tolerance: Some(sim.collision_tolerance),
                            max_particle_radius: Some(sim.max_particle_radius),
                            particles: sim
                                .particles
                                .iter()
                                .map(|p| PhysicsParticle {
                                    position: p.position.map(|v| v.0),
                                    fixed: p.fixed,
                                    mass: p.mass,
                                    inverse_mass: p.inverse_mass,
                                    radius: p.radius,
                                    friction: p.friction,
                                })
                                .collect(),
                            triangle_indices: sim.triangle_indices.clone(),
                            constraints: sim.constraints.iter().map(|key| id(*key)).collect(),
                            collidables: sim.collidables.iter().map(|key| id(*key)).collect(),
                        })
                        .collect(),
                })
                .collect(),
            constraints: graph
                .constraints
                .iter()
                .map(|value| PhysicsConstraint {
                    id: id(value.key),
                    source: source(value.key),
                    class_name: Some(value.class_name.clone()),
                    name: value.name.clone(),
                    elements: value
                        .elements
                        .iter()
                        .map(|e| PhysicsConstraintElement {
                            particles: e.particles.clone(),
                            values: e.values.clone(),
                        })
                        .collect(),
                })
                .collect(),
            collidables: graph
                .collidables
                .iter()
                .map(|value| PhysicsCollidable {
                    id: id(value.key),
                    source: source(value.key),
                    name: value.name.clone(),
                    transform: Some(value.transform.0),
                    translation: None,
                    axes: None,
                    linear_velocity: Some(value.linear_velocity.0),
                    angular_velocity: Some(value.angular_velocity.0),
                    enabled: true,
                    pinch_detection: value
                        .pinch_detection_enabled
                        .then_some((value.pinch_detection_priority, value.pinch_detection_radius)),
                    shape: value.shape.map(|key| PhysicsShape::Referenced {
                        source: source(key),
                        class_name: value.shape_class.clone(),
                    }),
                })
                .collect(),
            skeleton_bindings: Vec::new(),
        }
    }
}

impl From<&bphcl::BphclDocument> for FormatNeutralPhysicsGraph {
    fn from(document: &bphcl::BphclDocument) -> Self {
        let id = |item: usize| PhysicsId(format!("bphcl:{item}"));
        let source = |item: usize| SourceRef::Bphcl { item };
        let mut constraint_items = BTreeSet::new();
        for cloth in &document.cloth {
            for sim in &cloth.simulations {
                constraint_items.extend(&sim.constraint_item_indices);
            }
        }
        let constraints = constraint_items
            .into_iter()
            .map(|item| PhysicsConstraint {
                id: id(item),
                source: source(item),
                class_name: document
                    .items
                    .get(item)
                    .and_then(|entry| document.type_names.get(entry.type_index as usize))
                    .cloned(),
                name: None,
                elements: bphcl::read_constraint_elements(document, item).unwrap_or_default(),
            })
            .collect();
        let skeleton_ids: BTreeMap<_, _> = document
            .skeletons
            .iter()
            .map(|v| (v.item_index, id(v.item_index)))
            .collect();
        let cloth_ids: BTreeMap<_, _> = document
            .cloth
            .iter()
            .map(|v| (v.item_index, id(v.item_index)))
            .collect();
        Self {
            source_format: PhysicsFormat::Bphcl,
            skeletons: document
                .skeletons
                .iter()
                .map(|value| PhysicsSkeleton {
                    id: id(value.item_index),
                    source: source(value.item_index),
                    name: Some(value.name.clone()),
                    bones: value
                        .bones
                        .iter()
                        .map(|bone| PhysicsBone {
                            name: Some(bone.name.clone()),
                            parent_index: bone.parent_index,
                            lock_translation: bone.lock_translation,
                            transform: Some(Transform {
                                translation: vec4(bone.translation),
                                rotation: vec4(bone.rotation),
                                scale: [1.0, 1.0, 1.0, 0.0],
                            }),
                        })
                        .collect(),
                })
                .collect(),
            cloths: document
                .cloth
                .iter()
                .map(|cloth| PhysicsCloth {
                    id: id(cloth.item_index),
                    source: source(cloth.item_index),
                    name: Some(cloth.name.clone()),
                    target_platform: None,
                    simulations: cloth
                        .simulations
                        .iter()
                        .map(|sim| PhysicsSimulation {
                            id: id(sim.item_index),
                            source: source(sim.item_index),
                            name: Some(sim.name.clone()),
                            gravity: None,
                            total_mass: None,
                            collision_tolerance: None,
                            max_particle_radius: None,
                            particles: sim
                                .particles
                                .iter()
                                .map(|p| PhysicsParticle {
                                    position: Some(vec4(p.position)),
                                    fixed: p.fixed,
                                    mass: p.mass,
                                    inverse_mass: p.inverse_mass,
                                    radius: p.radius,
                                    friction: p.friction,
                                })
                                .collect(),
                            triangle_indices: Vec::new(),
                            constraints: sim
                                .constraint_item_indices
                                .iter()
                                .map(|v| id(*v))
                                .collect(),
                            collidables: sim
                                .collidable_item_indices
                                .iter()
                                .map(|v| id(*v))
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
            constraints,
            collidables: document
                .collidables
                .iter()
                .map(|value| PhysicsCollidable {
                    id: id(value.item_index),
                    source: source(value.item_index),
                    name: Some(value.name.clone()),
                    transform: None,
                    translation: Some(vec4(value.translation)),
                    axes: Some([vec4(value.axis_x), vec4(value.axis_y), vec4(value.axis_z)]),
                    linear_velocity: None,
                    angular_velocity: None,
                    enabled: value.enabled,
                    pinch_detection: value
                        .pinch_detection_enabled
                        .then_some((value.pinch_detection_priority, value.pinch_detection_radius)),
                    shape: Some(shape(&value.shape)),
                })
                .collect(),
            skeleton_bindings: document
                .cloth_skeleton_pairs
                .iter()
                .filter_map(|pair| {
                    Some(SkeletonBinding {
                        cloth: cloth_ids.get(&pair.cloth_item_index)?.clone(),
                        skeleton: skeleton_ids.get(&pair.skeleton_item_index)?.clone(),
                    })
                })
                .collect(),
        }
    }
}

fn vec4(value: bphcl::Vector4) -> [f32; 4] {
    [value.x, value.y, value.z, value.w]
}

fn shape(value: &bphcl::CollidableShape) -> PhysicsShape {
    match value {
        bphcl::CollidableShape::Capsule { start, end, radius } => PhysicsShape::Capsule {
            start: vec4(*start),
            end: vec4(*end),
            radius: *radius,
        },
        bphcl::CollidableShape::Sphere { center, radius } => PhysicsShape::Sphere {
            center: vec4(*center),
            radius: *radius,
        },
        bphcl::CollidableShape::TaperedCapsule {
            start,
            end,
            start_radius,
            end_radius,
        } => PhysicsShape::TaperedCapsule {
            start: vec4(*start),
            end: vec4(*end),
            start_radius: *start_radius,
            end_radius: *end_radius,
        },
        bphcl::CollidableShape::Plane { equation } => PhysicsShape::Plane {
            equation: vec4(*equation),
        },
        bphcl::CollidableShape::Unknown { class_name, kind } => PhysicsShape::Unknown {
            class_name: class_name.clone(),
            kind: *kind,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_hkcl_references_and_shared_particle_fields() {
        let simulation_key = hkcl::ObjectKey {
            section_index: 0,
            offset: 0x20,
        };
        let constraint_key = hkcl::ObjectKey {
            section_index: 0,
            offset: 0x40,
        };
        let graph = hkcl::PhysicsGraph {
            skeletons: Vec::new(),
            cloths: vec![hkcl::Cloth {
                key: hkcl::ObjectKey {
                    section_index: 0,
                    offset: 0x10,
                },
                name: Some("Cloth".into()),
                target_platform: 7,
                simulations: vec![hkcl::SimCloth {
                    key: simulation_key,
                    name: Some("Simulation".into()),
                    gravity: hkcl::Vector4([0.0, -9.8, 0.0, 0.0]),
                    total_mass: 1.0,
                    collision_tolerance: 0.1,
                    max_particle_radius: 0.2,
                    particles: vec![hkcl::Particle {
                        mass: 1.0,
                        inverse_mass: 1.0,
                        radius: 0.2,
                        friction: 0.5,
                        fixed: false,
                        position: Some(hkcl::Vector4([1.0, 2.0, 3.0, 1.0])),
                    }],
                    triangle_indices: Vec::new(),
                    collidables: Vec::new(),
                    constraints: vec![constraint_key],
                }],
                buffer_definition_count: 0,
                transform_set_definition_count: 0,
                operator_count: 0,
                state_count: 0,
                action_count: 0,
            }],
            constraints: vec![hkcl::Constraint {
                key: constraint_key,
                class_name: "Link".into(),
                name: None,
                element_count: 0,
                elements: Vec::new(),
            }],
            collidables: Vec::new(),
        };
        let neutral = FormatNeutralPhysicsGraph::from(&graph);
        assert_eq!(neutral.source_format, PhysicsFormat::Hkcl);
        assert_eq!(neutral.cloths[0].target_platform, Some(7));
        assert_eq!(
            neutral.cloths[0].simulations[0].constraints[0],
            neutral.constraints[0].id
        );
        assert_eq!(
            neutral.cloths[0].simulations[0].particles[0].position,
            Some([1.0, 2.0, 3.0, 1.0])
        );
    }
}
