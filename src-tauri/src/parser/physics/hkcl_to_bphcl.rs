use crate::parser::physics::physics_graph::{
    FormatNeutralPhysicsGraph, PhysicsCloth, PhysicsFormat, PhysicsId, PhysicsShape,
};
use serde::Serialize;
use std::{collections::BTreeMap, io};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CompatibilityIssue {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HkclToBphclCompatibility {
    pub source_cloth: usize,
    pub template_cloth: usize,
    pub issues: Vec<CompatibilityIssue>,
}

impl HkclToBphclCompatibility {
    pub fn is_compatible(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Converts an HKCL cloth through a compatible BPHCL template and appends the
/// resulting complete cloth package to the BPHCL graph. The selected template
/// remains unchanged and supplies fields that have no HKCL representation.
pub fn merge_hkcl_cloth_into_bphcl(
    source: &FormatNeutralPhysicsGraph,
    target: &FormatNeutralPhysicsGraph,
    source_cloth: usize,
    template_cloth: usize,
) -> io::Result<FormatNeutralPhysicsGraph> {
    let converted =
        convert_hkcl_cloth_to_bphcl_template(source, target, source_cloth, template_cloth)?;
    let cloth = &converted.cloths[template_cloth];
    let skeleton_index = paired_skeleton_index(&converted, template_cloth)
        .ok_or_else(|| invalid("converted BPHCL cloth has no paired skeleton"))?;
    let skeleton = &converted.skeletons[skeleton_index];
    let mut merged = target.clone();
    let prefix = unique_merge_prefix(&merged);
    let mut remap = BTreeMap::<PhysicsId, PhysicsId>::new();

    let skeleton_id = merged_id(&prefix, &skeleton.id);
    remap.insert(skeleton.id.clone(), skeleton_id.clone());
    let mut copied_skeleton = skeleton.clone();
    copied_skeleton.id = skeleton_id.clone();
    merged.skeletons.push(copied_skeleton);

    for simulation in &cloth.simulations {
        remap.insert(simulation.id.clone(), merged_id(&prefix, &simulation.id));
        for constraint_id in &simulation.constraints {
            if remap.contains_key(constraint_id) {
                continue;
            }
            let constraint = converted
                .constraints
                .iter()
                .find(|value| &value.id == constraint_id)
                .ok_or_else(|| invalid("converted BPHCL cloth references a missing constraint"))?;
            let id = merged_id(&prefix, constraint_id);
            remap.insert(constraint_id.clone(), id.clone());
            let mut copied = constraint.clone();
            copied.id = id;
            merged.constraints.push(copied);
        }
        for collidable_id in &simulation.collidables {
            if remap.contains_key(collidable_id) {
                continue;
            }
            let collidable = converted
                .collidables
                .iter()
                .find(|value| &value.id == collidable_id)
                .ok_or_else(|| invalid("converted BPHCL cloth references a missing collidable"))?;
            let id = merged_id(&prefix, collidable_id);
            remap.insert(collidable_id.clone(), id.clone());
            let mut copied = collidable.clone();
            copied.id = id;
            merged.collidables.push(copied);
        }
    }

    let cloth_id = merged_id(&prefix, &cloth.id);
    let mut copied_cloth = cloth.clone();
    copied_cloth.id = cloth_id.clone();
    for simulation in &mut copied_cloth.simulations {
        simulation.id = remap[&simulation.id].clone();
        simulation.constraints = simulation
            .constraints
            .iter()
            .map(|id| remap[id].clone())
            .collect();
        simulation.collidables = simulation
            .collidables
            .iter()
            .map(|id| remap[id].clone())
            .collect();
    }
    merged.cloths.push(copied_cloth);
    merged
        .skeleton_bindings
        .push(crate::parser::physics::physics_graph::SkeletonBinding {
            cloth: cloth_id,
            skeleton: skeleton_id,
        });
    validate_merged_references(&merged)?;
    Ok(merged)
}

/// Applies the conservative template-conversion policy: BPHCL-only shape and
/// container data stays in place while shared HKCL values replace the template.
pub fn convert_hkcl_cloth_to_bphcl_template(
    source: &FormatNeutralPhysicsGraph,
    target: &FormatNeutralPhysicsGraph,
    source_cloth: usize,
    template_cloth: usize,
) -> io::Result<FormatNeutralPhysicsGraph> {
    let report = analyze_hkcl_to_bphcl(source, target, source_cloth, template_cloth);
    if !report.is_compatible() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            report
                .issues
                .iter()
                .map(|issue| issue.message.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    let source_cloth_value = source.cloths.get(source_cloth).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "source cloth index is out of range",
        )
    })?;
    let source_skeleton = paired_skeleton(source, source_cloth).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "source cloth has no paired skeleton",
        )
    })?;
    let target_skeleton_index = paired_skeleton_index(target, template_cloth).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "template cloth has no paired skeleton",
        )
    })?;
    let source_colliders = referenced_colliders(source, source_cloth_value);
    let target_colliders = referenced_colliders(target, &target.cloths[template_cloth]);
    let mut converted = target.clone();

    converted.cloths[template_cloth].name = source_cloth_value.name.clone();
    for (target_sim, source_sim) in converted.cloths[template_cloth]
        .simulations
        .iter_mut()
        .zip(&source_cloth_value.simulations)
    {
        target_sim.name = source_sim.name.clone();
        target_sim.particles.clone_from(&source_sim.particles);
        // BPHCL topology, buffers, and operator layout remain owned by the
        // template. HKCL triangle indices are not a standalone BPHCL array.
    }
    for (target_sim, source_sim) in target.cloths[template_cloth]
        .simulations
        .iter()
        .zip(&source_cloth_value.simulations)
    {
        for (target_id, source_id) in target_sim.constraints.iter().zip(&source_sim.constraints) {
            let target_index = converted
                .constraints
                .iter()
                .position(|value| &value.id == target_id)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "target constraint is missing")
                })?;
            let source_value = source
                .constraints
                .iter()
                .find(|value| &value.id == source_id)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "source constraint is missing")
                })?;
            converted.constraints[target_index].name = source_value.name.clone();
            converted.constraints[target_index].elements = source_value.elements.clone();
        }
    }
    converted.skeletons[target_skeleton_index].name = source_skeleton.name.clone();
    let target_bones = converted.skeletons[target_skeleton_index].bones.clone();
    let source_by_name: BTreeMap<_, _> = source_skeleton
        .bones
        .iter()
        .enumerate()
        .filter_map(|(index, bone)| {
            bone.name
                .as_ref()
                .map(|name| (strip_prefix(name).to_ascii_lowercase(), (index, bone)))
        })
        .collect();
    let target_index_by_source: BTreeMap<_, _> = target_bones
        .iter()
        .enumerate()
        .filter_map(|(target_index, target_bone)| {
            let source_index = target_bone
                .name
                .as_ref()
                .and_then(|name| source_by_name.get(&strip_prefix(name).to_ascii_lowercase()))
                .map(|(source_index, _)| *source_index)?;
            Some((source_index, target_index))
        })
        .collect();
    converted.skeletons[target_skeleton_index].bones = target_bones
        .into_iter()
        .map(|target_bone| {
            let Some((_, source_bone)) = target_bone
                .name
                .as_ref()
                .and_then(|name| source_by_name.get(&strip_prefix(name).to_ascii_lowercase()))
            else {
                return target_bone;
            };
            let mut converted_bone = (*source_bone).clone();
            // BPHCL namespaces and array order belong to the template.
            converted_bone.name = target_bone.name;
            converted_bone.parent_index = source_bone
                .parent_index
                .and_then(|index| target_index_by_source.get(&index).copied());
            converted_bone
        })
        .collect();
    for ((_, source_value), (_, target_value)) in
        source_colliders.iter().zip(target_colliders.iter())
    {
        let index = converted
            .collidables
            .iter()
            .position(|value| value.id == target_value.id)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "target collider is missing")
            })?;
        converted.collidables[index].name = source_value.name.clone();
        converted.collidables[index].transform = source_value.transform;
        converted.collidables[index].linear_velocity = source_value.linear_velocity;
        converted.collidables[index].angular_velocity = source_value.angular_velocity;
        converted.collidables[index].pinch_detection = source_value.pinch_detection;
    }
    Ok(converted)
}

pub fn analyze_hkcl_to_bphcl(
    source: &FormatNeutralPhysicsGraph,
    target: &FormatNeutralPhysicsGraph,
    source_cloth: usize,
    template_cloth: usize,
) -> HkclToBphclCompatibility {
    let mut issues = Vec::new();
    check(
        source.source_format == PhysicsFormat::Hkcl,
        "source-format",
        "source must be HKCL",
        &mut issues,
    );
    check(
        target.source_format == PhysicsFormat::Bphcl,
        "target-format",
        "target template must be BPHCL",
        &mut issues,
    );
    let Some(source_cloth_value) = source.cloths.get(source_cloth) else {
        issue(
            "source-cloth",
            "source cloth index is out of range",
            &mut issues,
        );
        return HkclToBphclCompatibility {
            source_cloth,
            template_cloth,
            issues,
        };
    };
    let Some(template_cloth_value) = target.cloths.get(template_cloth) else {
        issue(
            "template-cloth",
            "template cloth index is out of range",
            &mut issues,
        );
        return HkclToBphclCompatibility {
            source_cloth,
            template_cloth,
            issues,
        };
    };
    check(
        source_cloth_value.simulations.len() == template_cloth_value.simulations.len(),
        "simulation-count",
        &format!(
            "simulation count differs ({} HKCL / {} BPHCL template)",
            source_cloth_value.simulations.len(),
            template_cloth_value.simulations.len()
        ),
        &mut issues,
    );
    for (index, (source_sim, template_sim)) in source_cloth_value
        .simulations
        .iter()
        .zip(&template_cloth_value.simulations)
        .enumerate()
    {
        check(
            source_sim.particles.len() == template_sim.particles.len(),
            "particle-count",
            &format!(
                "simulation {index} particle count differs ({} HKCL / {} BPHCL template)",
                source_sim.particles.len(),
                template_sim.particles.len()
            ),
            &mut issues,
        );
        check(
            source_sim
                .particles
                .iter()
                .all(|particle| particle.position.is_some()),
            "particle-position",
            &format!("simulation {index} has particles without positions"),
            &mut issues,
        );
        check(
            source_sim.constraints.len() == template_sim.constraints.len(),
            "constraint-count",
            &format!("simulation {index} constraint-set count differs"),
            &mut issues,
        );
        for (source_id, target_id) in source_sim.constraints.iter().zip(&template_sim.constraints) {
            let a = source
                .constraints
                .iter()
                .find(|value| &value.id == source_id);
            let b = target
                .constraints
                .iter()
                .find(|value| &value.id == target_id);
            check(
                a.is_some() && b.is_some(),
                "constraint-reference",
                "a constraint reference cannot be resolved",
                &mut issues,
            );
            if let (Some(a), Some(b)) = (a, b) {
                check(
                    a.class_name == b.class_name,
                    "constraint-class",
                    "constraint classes do not match the template",
                    &mut issues,
                );
                check(
                    a.elements.len() == b.elements.len(),
                    "constraint-layout",
                    "constraint element counts do not match the template",
                    &mut issues,
                );
            }
        }
    }
    let source_skeleton = paired_skeleton(source, source_cloth);
    let target_skeleton = paired_skeleton(target, template_cloth);
    check(
        source_skeleton.is_some() && target_skeleton.is_some(),
        "paired-skeleton",
        "both cloths require paired skeletons",
        &mut issues,
    );
    if let (Some(a), Some(b)) = (source_skeleton, target_skeleton) {
        check(
            a.bones.len() == b.bones.len(),
            "bone-count",
            &format!(
                "bone count differs ({} HKCL / {} BPHCL template)",
                a.bones.len(),
                b.bones.len()
            ),
            &mut issues,
        );
        let target_names: BTreeMap<_, _> = b
            .bones
            .iter()
            .filter_map(|bone| bone.name.as_ref())
            .map(|name| (strip_prefix(name).to_ascii_lowercase(), ()))
            .collect();
        let missing: Vec<_> = a
            .bones
            .iter()
            .filter_map(|bone| bone.name.as_ref())
            .filter(|name| !target_names.contains_key(&strip_prefix(name).to_ascii_lowercase()))
            .cloned()
            .collect();
        check(
            missing.is_empty(),
            "bone-names",
            &format!(
                "BPHCL template is missing HKCL bones: {}",
                missing.join(", ")
            ),
            &mut issues,
        );
    }
    let source_colliders = referenced_colliders(source, source_cloth_value);
    let target_colliders = referenced_colliders(target, template_cloth_value);
    check(
        source_colliders.len() == target_colliders.len(),
        "collider-count",
        &format!(
            "referenced collider count differs ({} HKCL / {} BPHCL template)",
            source_colliders.len(),
            target_colliders.len()
        ),
        &mut issues,
    );
    check(
        target_colliders.iter().all(|(_, value)| {
            matches!(
                value.shape,
                Some(
                    PhysicsShape::Capsule { .. }
                        | PhysicsShape::Sphere { .. }
                        | PhysicsShape::Plane { .. }
                )
            )
        }),
        "collider-shape",
        "template colliders must use capsule, sphere, or plane shapes",
        &mut issues,
    );
    HkclToBphclCompatibility {
        source_cloth,
        template_cloth,
        issues,
    }
}

fn paired_skeleton(
    graph: &FormatNeutralPhysicsGraph,
    cloth: usize,
) -> Option<&crate::parser::physics::physics_graph::PhysicsSkeleton> {
    paired_skeleton_index(graph, cloth).and_then(|index| graph.skeletons.get(index))
}
fn paired_skeleton_index(graph: &FormatNeutralPhysicsGraph, cloth: usize) -> Option<usize> {
    let cloth_value = graph.cloths.get(cloth)?;
    graph
        .skeleton_bindings
        .iter()
        .find(|binding| binding.cloth == cloth_value.id)
        .and_then(|binding| {
            graph
                .skeletons
                .iter()
                .position(|value| value.id == binding.skeleton)
        })
        .or_else(|| (cloth < graph.skeletons.len()).then_some(cloth))
}
fn referenced_colliders<'a>(
    graph: &'a FormatNeutralPhysicsGraph,
    cloth: &PhysicsCloth,
) -> Vec<(
    PhysicsId,
    &'a crate::parser::physics::physics_graph::PhysicsCollidable,
)> {
    let mut ids: Vec<_> = cloth
        .simulations
        .iter()
        .flat_map(|simulation| simulation.collidables.iter().cloned())
        .collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .filter_map(|id| {
            graph
                .collidables
                .iter()
                .find(|value| value.id == id)
                .map(|value| (id, value))
        })
        .collect()
}
fn strip_prefix(name: &str) -> &str {
    name.strip_prefix("Link:").unwrap_or(name)
}

fn unique_merge_prefix(graph: &FormatNeutralPhysicsGraph) -> String {
    let mut serial = graph.cloths.len()
        + graph.skeletons.len()
        + graph.constraints.len()
        + graph.collidables.len();
    loop {
        let prefix = format!("hkcl-to-bphcl:{serial}");
        let occupied = graph
            .cloths
            .iter()
            .map(|value| &value.id)
            .chain(graph.skeletons.iter().map(|value| &value.id))
            .chain(graph.constraints.iter().map(|value| &value.id))
            .chain(graph.collidables.iter().map(|value| &value.id))
            .any(|id| id.0.starts_with(&prefix));
        if !occupied {
            return prefix;
        }
        serial += 1;
    }
}

fn merged_id(prefix: &str, source: &PhysicsId) -> PhysicsId {
    PhysicsId(format!("{prefix}:{}", source.0))
}

fn validate_merged_references(graph: &FormatNeutralPhysicsGraph) -> io::Result<()> {
    for cloth in &graph.cloths {
        for simulation in &cloth.simulations {
            if simulation
                .constraints
                .iter()
                .any(|id| !graph.constraints.iter().any(|value| &value.id == id))
            {
                return Err(invalid("merged BPHCL cloth has an unresolved constraint"));
            }
            if simulation
                .collidables
                .iter()
                .any(|id| !graph.collidables.iter().any(|value| &value.id == id))
            {
                return Err(invalid("merged BPHCL cloth has an unresolved collidable"));
            }
        }
    }
    if graph.skeleton_bindings.iter().any(|binding| {
        !graph.cloths.iter().any(|value| value.id == binding.cloth)
            || !graph
                .skeletons
                .iter()
                .any(|value| value.id == binding.skeleton)
    }) {
        return Err(invalid(
            "merged BPHCL graph has an unresolved skeleton binding",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn check(value: bool, code: &'static str, message: &str, issues: &mut Vec<CompatibilityIssue>) {
    if !value {
        issue(code, message, issues);
    }
}
fn issue(code: &'static str, message: &str, issues: &mut Vec<CompatibilityIssue>) {
    issues.push(CompatibilityIssue {
        code,
        message: message.to_owned(),
    });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::parser::physics::physics_graph::*;

    pub(crate) fn graph(format: PhysicsFormat) -> FormatNeutralPhysicsGraph {
        let prefix = if format == PhysicsFormat::Hkcl {
            "h"
        } else {
            "b"
        };
        let cloth_id = PhysicsId(format!("{prefix}:cloth"));
        let skeleton_id = PhysicsId(format!("{prefix}:skeleton"));
        let constraint_id = PhysicsId(format!("{prefix}:constraint"));
        let collider_id = PhysicsId(format!("{prefix}:collider"));
        let source = if format == PhysicsFormat::Hkcl {
            SourceRef::Hkcl {
                section: 0,
                offset: 1,
            }
        } else {
            SourceRef::Bphcl { item: 1 }
        };
        FormatNeutralPhysicsGraph {
            source_format: format,
            skeletons: vec![PhysicsSkeleton {
                id: skeleton_id.clone(),
                source: source.clone(),
                name: Some("Skeleton".into()),
                bones: vec![PhysicsBone {
                    name: Some(
                        if format == PhysicsFormat::Bphcl {
                            "Link:Root"
                        } else {
                            "Root"
                        }
                        .into(),
                    ),
                    parent_index: None,
                    lock_translation: false,
                    transform: None,
                }],
            }],
            cloths: vec![PhysicsCloth {
                id: cloth_id.clone(),
                source: source.clone(),
                name: Some(
                    if format == PhysicsFormat::Hkcl {
                        "Source"
                    } else {
                        "Template"
                    }
                    .into(),
                ),
                target_platform: None,
                simulations: vec![PhysicsSimulation {
                    id: PhysicsId(format!("{prefix}:simulation")),
                    source: source.clone(),
                    name: None,
                    gravity: None,
                    total_mass: None,
                    collision_tolerance: None,
                    max_particle_radius: None,
                    particles: vec![PhysicsParticle {
                        position: Some([1.0, 2.0, 3.0, 1.0]),
                        fixed: false,
                        mass: 2.0,
                        inverse_mass: 0.5,
                        radius: 0.2,
                        friction: 0.4,
                    }],
                    triangle_indices: Vec::new(),
                    constraints: vec![constraint_id.clone()],
                    collidables: vec![collider_id.clone()],
                }],
            }],
            constraints: vec![PhysicsConstraint {
                id: constraint_id,
                source: source.clone(),
                class_name: Some("hclStandardLinkConstraintSet".into()),
                name: None,
                elements: vec![PhysicsConstraintElement {
                    particles: vec![0, 0],
                    values: vec![1.0],
                }],
            }],
            collidables: vec![PhysicsCollidable {
                id: collider_id,
                source,
                name: Some("Collider".into()),
                transform: (format == PhysicsFormat::Hkcl).then_some([2.0; 16]),
                translation: None,
                axes: None,
                linear_velocity: None,
                angular_velocity: None,
                enabled: true,
                pinch_detection: None,
                shape: (format == PhysicsFormat::Bphcl).then_some(PhysicsShape::Sphere {
                    center: [0.0; 4],
                    radius: 3.0,
                }),
            }],
            skeleton_bindings: vec![SkeletonBinding {
                cloth: cloth_id,
                skeleton: skeleton_id,
            }],
        }
    }

    #[test]
    fn compatible_template_preserves_bphcl_shape_and_applies_hkcl_values() {
        let source = graph(PhysicsFormat::Hkcl);
        let target = graph(PhysicsFormat::Bphcl);
        assert!(analyze_hkcl_to_bphcl(&source, &target, 0, 0).is_compatible());
        let converted = convert_hkcl_cloth_to_bphcl_template(&source, &target, 0, 0).unwrap();
        assert_eq!(converted.cloths[0].name.as_deref(), Some("Source"));
        assert_eq!(converted.cloths[0].simulations[0].particles[0].mass, 2.0);
        assert_eq!(converted.collidables[0].transform, Some([2.0; 16]));
        assert!(matches!(
            converted.collidables[0].shape,
            Some(PhysicsShape::Sphere { radius: 3.0, .. })
        ));
    }

    #[test]
    fn preflight_rejects_particle_layout_mismatch() {
        let source = graph(PhysicsFormat::Hkcl);
        let mut target = graph(PhysicsFormat::Bphcl);
        target.cloths[0].simulations[0].particles.clear();
        let report = analyze_hkcl_to_bphcl(&source, &target, 0, 0);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "particle-count"));
        assert!(convert_hkcl_cloth_to_bphcl_template(&source, &target, 0, 0).is_err());
    }

    #[test]
    fn cross_format_merge_appends_a_remapped_complete_bphcl_package() {
        let source = graph(PhysicsFormat::Hkcl);
        let target = graph(PhysicsFormat::Bphcl);
        let original_shape = target.collidables[0].shape.clone();

        let merged = merge_hkcl_cloth_into_bphcl(&source, &target, 0, 0).unwrap();

        assert_eq!(merged.source_format, PhysicsFormat::Bphcl);
        assert_eq!(merged.cloths.len(), 2);
        assert_eq!(merged.skeletons.len(), 2);
        assert_eq!(merged.constraints.len(), 2);
        assert_eq!(merged.collidables.len(), 2);
        assert_eq!(merged.cloths[0].name.as_deref(), Some("Template"));
        assert_eq!(merged.cloths[1].name.as_deref(), Some("Source"));
        assert_eq!(merged.collidables[1].shape, original_shape);
        assert_eq!(merged.collidables[1].transform, Some([2.0; 16]));

        let imported_simulation = &merged.cloths[1].simulations[0];
        assert_ne!(imported_simulation.id, target.cloths[0].simulations[0].id);
        assert!(merged
            .constraints
            .iter()
            .any(|value| value.id == imported_simulation.constraints[0]));
        assert!(merged
            .collidables
            .iter()
            .any(|value| value.id == imported_simulation.collidables[0]));
        assert!(merged
            .skeleton_bindings
            .iter()
            .any(|binding| binding.cloth == merged.cloths[1].id
                && binding.skeleton == merged.skeletons[1].id));
    }

    #[test]
    fn cross_format_merge_rejects_an_incompatible_template() {
        let source = graph(PhysicsFormat::Hkcl);
        let mut target = graph(PhysicsFormat::Bphcl);
        target.cloths[0].simulations[0].particles.clear();
        assert!(merge_hkcl_cloth_into_bphcl(&source, &target, 0, 0).is_err());
    }
}
