use crate::parser::physics::{
    bphhb::BphhbDocument,
    bphhb_mapping::map_skeleton_bones,
    hkcl_to_bphcl::{CompatibilityIssue, HkclToBphclCompatibility},
    physics_graph::{
        FormatNeutralPhysicsGraph, PhysicsCloth, PhysicsFormat, PhysicsId, PhysicsShape,
    },
};
use std::{collections::BTreeMap, io};

pub type BphclToHkclCompatibility = HkclToBphclCompatibility;

/// Converts a BPHCL cloth through a compatible HKCL template and appends the
/// resulting complete cloth package to the HKCL graph. The selected template
/// remains unchanged and supplies HKCL-only topology and layout data.
pub fn merge_bphcl_cloth_into_hkcl(
    source: &FormatNeutralPhysicsGraph,
    target: &FormatNeutralPhysicsGraph,
    source_cloth: usize,
    template_cloth: usize,
) -> io::Result<FormatNeutralPhysicsGraph> {
    let converted =
        convert_bphcl_cloth_to_hkcl_template(source, target, source_cloth, template_cloth)?;
    append_converted_hkcl(target, &converted, template_cloth)
}

fn append_converted_hkcl(
    target: &FormatNeutralPhysicsGraph,
    converted: &FormatNeutralPhysicsGraph,
    template_cloth: usize,
) -> io::Result<FormatNeutralPhysicsGraph> {
    let cloth = &converted.cloths[template_cloth];
    let skeleton_index = paired_skeleton_index(&converted, template_cloth)
        .ok_or_else(|| invalid("converted HKCL cloth has no paired skeleton"))?;
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
                .ok_or_else(|| invalid("converted HKCL cloth references a missing constraint"))?;
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
                .ok_or_else(|| invalid("converted HKCL cloth references a missing collidable"))?;
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

pub fn merge_bphcl_cloth_into_hkcl_with_bphhb(
    source: &FormatNeutralPhysicsGraph,
    target: &FormatNeutralPhysicsGraph,
    source_cloth: usize,
    template_cloth: usize,
    helper: &BphhbDocument,
) -> io::Result<FormatNeutralPhysicsGraph> {
    let converted = convert_bphcl_cloth_to_hkcl_template_with_bphhb(
        source,
        target,
        source_cloth,
        template_cloth,
        helper,
    )?;
    append_converted_hkcl(target, &converted, template_cloth)
}

pub fn analyze_bphcl_to_hkcl(
    source: &FormatNeutralPhysicsGraph,
    target: &FormatNeutralPhysicsGraph,
    source_cloth: usize,
    template_cloth: usize,
) -> BphclToHkclCompatibility {
    analyze_bphcl_to_hkcl_internal(source, target, source_cloth, template_cloth, None)
}

pub fn analyze_bphcl_to_hkcl_with_bphhb(
    source: &FormatNeutralPhysicsGraph,
    target: &FormatNeutralPhysicsGraph,
    source_cloth: usize,
    template_cloth: usize,
    helper: &BphhbDocument,
) -> BphclToHkclCompatibility {
    analyze_bphcl_to_hkcl_internal(source, target, source_cloth, template_cloth, Some(helper))
}

fn analyze_bphcl_to_hkcl_internal(
    source: &FormatNeutralPhysicsGraph,
    target: &FormatNeutralPhysicsGraph,
    source_cloth: usize,
    template_cloth: usize,
    helper: Option<&BphhbDocument>,
) -> BphclToHkclCompatibility {
    let mut issues = Vec::new();
    check(
        source.source_format == PhysicsFormat::Bphcl,
        "source-format",
        "source must be BPHCL",
        &mut issues,
    );
    check(
        target.source_format == PhysicsFormat::Hkcl,
        "target-format",
        "target template must be HKCL",
        &mut issues,
    );
    let Some(source_cloth_value) = source.cloths.get(source_cloth) else {
        issue(
            "source-cloth",
            "source cloth index is out of range",
            &mut issues,
        );
        return report(source_cloth, template_cloth, issues);
    };
    let Some(template_cloth_value) = target.cloths.get(template_cloth) else {
        issue(
            "template-cloth",
            "template cloth index is out of range",
            &mut issues,
        );
        return report(source_cloth, template_cloth, issues);
    };
    check(
        source_cloth_value.simulations.len() == 1,
        "simulation-count",
        &format!(
            "BPHCL source must have exactly one simulation (has {})",
            source_cloth_value.simulations.len()
        ),
        &mut issues,
    );
    check(
        template_cloth_value.simulations.len() == 1,
        "template-simulation-count",
        &format!(
            "HKCL template must have exactly one simulation (has {})",
            template_cloth_value.simulations.len()
        ),
        &mut issues,
    );
    if let (Some(a), Some(b)) = (
        source_cloth_value.simulations.first(),
        template_cloth_value.simulations.first(),
    ) {
        check(
            a.particles.len() == b.particles.len(),
            "particle-count",
            &format!(
                "particle count differs ({} BPHCL / {} HKCL template)",
                a.particles.len(),
                b.particles.len()
            ),
            &mut issues,
        );
        for source_id in &a.constraints {
            let Some(source_set) = source
                .constraints
                .iter()
                .find(|value| &value.id == source_id)
            else {
                issue(
                    "constraint-reference",
                    "a BPHCL constraint reference cannot be resolved",
                    &mut issues,
                );
                continue;
            };
            if source_set.elements.is_empty() {
                continue;
            }
            let matches = b
                .constraints
                .iter()
                .filter_map(|id| target.constraints.iter().find(|value| &value.id == id))
                .any(|value| {
                    value.class_name == source_set.class_name
                        && value.elements.len() == source_set.elements.len()
                });
            check(
                matches,
                "constraint-layout",
                &format!(
                    "HKCL template has no matching {} layout with {} links",
                    source_set.class_name.as_deref().unwrap_or("constraint"),
                    source_set.elements.len()
                ),
                &mut issues,
            );
        }
    }
    let source_skeleton = paired_skeleton(source, source_cloth);
    let template_skeleton = paired_skeleton(target, template_cloth);
    check(
        source_skeleton.is_some() && template_skeleton.is_some(),
        "paired-skeleton",
        "both cloths require paired skeletons",
        &mut issues,
    );
    if let (Some(a), Some(b)) = (source_skeleton, template_skeleton) {
        check(
            a.bones.len() == b.bones.len(),
            "bone-count",
            &format!(
                "bone count differs ({} BPHCL / {} HKCL template)",
                a.bones.len(),
                b.bones.len()
            ),
            &mut issues,
        );
        let target_names: BTreeMap<_, _> = b
            .bones
            .iter()
            .filter_map(|bone| bone.name.as_ref())
            .map(|name| (name.to_ascii_lowercase(), ()))
            .collect();
        let missing: Vec<_> = a
            .bones
            .iter()
            .filter_map(|bone| bone.name.as_ref())
            .map(|name| strip_prefix(name))
            .filter(|name| !target_names.contains_key(&name.to_ascii_lowercase()))
            .collect();
        let helper_mapping = helper.map(|helper| map_skeleton_bones(a, b, helper));
        check(
            missing.is_empty() || helper_mapping.as_ref().is_some_and(|map| map.is_complete()),
            "bone-names",
            &format!(
                "HKCL template is missing BPHCL bones: {}",
                missing.join(", ")
            ),
            &mut issues,
        );
    }
    let source_colliders = referenced_colliders(source, source_cloth_value);
    let template_colliders = referenced_colliders(target, template_cloth_value);
    check(
        source_colliders.len() == template_colliders.len(),
        "collider-count",
        &format!(
            "referenced collider count differs ({} BPHCL / {} HKCL template)",
            source_colliders.len(),
            template_colliders.len()
        ),
        &mut issues,
    );
    check(
        source_colliders.iter().all(|(_, value)| {
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
        "BPHCL source colliders must use capsule, sphere, or plane shapes",
        &mut issues,
    );
    report(source_cloth, template_cloth, issues)
}

pub fn convert_bphcl_cloth_to_hkcl_template(
    source: &FormatNeutralPhysicsGraph,
    target: &FormatNeutralPhysicsGraph,
    source_cloth: usize,
    template_cloth: usize,
) -> io::Result<FormatNeutralPhysicsGraph> {
    convert_bphcl_cloth_to_hkcl_template_internal(
        source,
        target,
        source_cloth,
        template_cloth,
        None,
    )
}

pub fn convert_bphcl_cloth_to_hkcl_template_with_bphhb(
    source: &FormatNeutralPhysicsGraph,
    target: &FormatNeutralPhysicsGraph,
    source_cloth: usize,
    template_cloth: usize,
    helper: &BphhbDocument,
) -> io::Result<FormatNeutralPhysicsGraph> {
    convert_bphcl_cloth_to_hkcl_template_internal(
        source,
        target,
        source_cloth,
        template_cloth,
        Some(helper),
    )
}

fn convert_bphcl_cloth_to_hkcl_template_internal(
    source: &FormatNeutralPhysicsGraph,
    target: &FormatNeutralPhysicsGraph,
    source_cloth: usize,
    template_cloth: usize,
    helper: Option<&BphhbDocument>,
) -> io::Result<FormatNeutralPhysicsGraph> {
    let compatibility =
        analyze_bphcl_to_hkcl_internal(source, target, source_cloth, template_cloth, helper);
    if !compatibility.is_compatible() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            compatibility
                .issues
                .iter()
                .map(|value| value.message.as_str())
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
    let source_sim = source_cloth_value.simulations.first().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "source cloth has no simulation")
    })?;
    let source_skeleton = paired_skeleton(source, source_cloth).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "source cloth has no paired skeleton",
        )
    })?;
    let template_skeleton_index =
        paired_skeleton_index(target, template_cloth).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "template cloth has no paired skeleton",
            )
        })?;
    let source_colliders = referenced_colliders(source, source_cloth_value);
    let template_colliders = referenced_colliders(target, &target.cloths[template_cloth]);
    let mut converted = target.clone();
    converted.cloths[template_cloth].name = source_cloth_value
        .name
        .as_deref()
        .map(strip_prefix)
        .map(str::to_owned);
    let converted_sim = &mut converted.cloths[template_cloth].simulations[0];
    converted_sim.name = source_sim.name.clone();
    converted_sim.particles.clone_from(&source_sim.particles);
    // PhysicsTool deliberately keeps HKCL topology, masks, deformers and local ranges.

    let target_bone_names: BTreeMap<_, _> = converted.skeletons[template_skeleton_index]
        .bones
        .iter()
        .enumerate()
        .filter_map(|(index, bone)| {
            bone.name
                .as_ref()
                .map(|name| (name.to_ascii_lowercase(), index))
        })
        .collect();
    let direct_source_to_target: BTreeMap<_, _> = source_skeleton
        .bones
        .iter()
        .enumerate()
        .filter_map(|(source_index, bone)| {
            bone.name.as_ref().and_then(|name| {
                target_bone_names
                    .get(&strip_prefix(name).to_ascii_lowercase())
                    .copied()
                    .map(|target_index| (source_index, target_index))
            })
        })
        .collect();
    let source_to_target = helper
        .map(|helper| {
            map_skeleton_bones(
                source_skeleton,
                &target.skeletons[template_skeleton_index],
                helper,
            )
            .source_to_target
        })
        .filter(|mapping| mapping.len() == source_skeleton.bones.len())
        .unwrap_or(direct_source_to_target);
    converted.skeletons[template_skeleton_index].name = source_skeleton
        .name
        .as_deref()
        .map(strip_prefix)
        .map(str::to_owned);
    for (source_index, target_index) in &source_to_target {
        let mut bone = source_skeleton.bones[*source_index].clone();
        bone.name = bone.name.as_deref().map(strip_prefix).map(str::to_owned);
        bone.parent_index = bone
            .parent_index
            .and_then(|parent| source_to_target.get(&parent).copied());
        converted.skeletons[template_skeleton_index].bones[*target_index] = bone;
    }
    for source_id in &source_sim.constraints {
        let Some(source_set) = source
            .constraints
            .iter()
            .find(|value| &value.id == source_id)
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("source constraint {source_id:?} is missing"),
            ));
        };
        if source_set.elements.is_empty() {
            continue;
        }
        if let Some(target_id) = target.cloths[template_cloth].simulations[0]
            .constraints
            .iter()
            .find(|id| {
                target.constraints.iter().any(|value| {
                    &value.id == *id
                        && value.class_name == source_set.class_name
                        && value.elements.len() == source_set.elements.len()
                })
            })
        {
            let index = converted
                .constraints
                .iter()
                .position(|value| &value.id == target_id)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "target constraint is missing")
                })?;
            converted.constraints[index].name = source_set.name.clone();
            converted.constraints[index]
                .elements
                .clone_from(&source_set.elements);
        }
    }
    for ((_, source_value), (_, template_value)) in source_colliders.iter().zip(&template_colliders)
    {
        let index = converted
            .collidables
            .iter()
            .position(|value| value.id == template_value.id)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "template collider is missing")
            })?;
        converted.collidables[index].name = source_value
            .name
            .as_deref()
            .map(strip_prefix)
            .map(str::to_owned);
        converted.collidables[index].translation = source_value.translation;
        converted.collidables[index].axes = source_value.axes;
        converted.collidables[index].transform =
            collider_matrix(source_value.translation, source_value.axes);
        converted.collidables[index]
            .shape
            .clone_from(&source_value.shape);
        // Pinch detection travels with the collider; the template's own
        // settings belonged to a different body.
        converted.collidables[index].pinch_detection = source_value.pinch_detection;
    }
    Ok(converted)
}

fn collider_matrix(
    translation: Option<[f32; 4]>,
    axes: Option<[[f32; 4]; 3]>,
) -> Option<[f32; 16]> {
    let (translation, axes) = (translation?, axes?);
    Some([
        axes[0][0],
        axes[0][1],
        axes[0][2],
        0.0,
        axes[1][0],
        axes[1][1],
        axes[1][2],
        0.0,
        axes[2][0],
        axes[2][1],
        axes[2][2],
        0.0,
        translation[0],
        translation[1],
        translation[2],
        1.0,
    ])
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
        .find(|value| value.cloth == cloth_value.id)
        .and_then(|value| {
            graph
                .skeletons
                .iter()
                .position(|skeleton| skeleton.id == value.skeleton)
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
        .flat_map(|value| value.collidables.iter().cloned())
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
        let prefix = format!("bphcl-to-hkcl:{serial}");
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
                return Err(invalid("merged HKCL cloth has an unresolved constraint"));
            }
            if simulation
                .collidables
                .iter()
                .any(|id| !graph.collidables.iter().any(|value| &value.id == id))
            {
                return Err(invalid("merged HKCL cloth has an unresolved collidable"));
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
            "merged HKCL graph has an unresolved skeleton binding",
        ));
    }
    Ok(())
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn report(
    source_cloth: usize,
    template_cloth: usize,
    issues: Vec<CompatibilityIssue>,
) -> BphclToHkclCompatibility {
    BphclToHkclCompatibility {
        source_cloth,
        template_cloth,
        issues,
    }
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
mod tests {
    use super::*;
    use crate::parser::physics::{
        bphhb::{BphhbBone, BphhbHeader, BphhbMetadata, BphhbTransform},
        hkcl_to_bphcl::tests::graph,
        physics_graph::PhysicsShape,
    };

    fn helper_document(name: &str, parent: &str) -> BphhbDocument {
        BphhbDocument {
            raw: Vec::new(),
            header: BphhbHeader {
                archive_version: 0,
                flags: 0,
                file_size: 0,
                parameter_io_version: 0,
                parameter_io_offset: 0,
                list_count: 0,
                object_count: 0,
                parameter_count: 0,
                data_size: 0,
                string_pool_size: 0,
                unknown: 0,
                data_type: "phhb".into(),
            },
            metadata: BphhbMetadata {
                archive_version: 0,
                parameter_io_version: 0,
                data_version: 0,
                data_type: "phhb".into(),
                list_count: 0,
                object_count: 0,
                parameter_count: 0,
                data_size: 0,
                string_pool_size: 0,
                string_pool: Vec::new(),
            },
            bones: vec![BphhbBone {
                name: name.into(),
                parent_name: Some(parent.into()),
                parent_index: None,
                transform: BphhbTransform::default(),
                object_path: vec![1],
                metadata: BTreeMap::new(),
            }],
        }
    }

    #[test]
    fn compatible_template_maps_names_particles_and_collider_geometry() {
        let source = graph(PhysicsFormat::Bphcl);
        let mut target = graph(PhysicsFormat::Hkcl);
        target.collidables[0].translation = Some([0.0, 0.0, 0.0, 1.0]);
        target.collidables[0].axes = Some([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ]);
        let mut source = source;
        source.collidables[0].translation = Some([4.0, 5.0, 6.0, 1.0]);
        source.collidables[0].axes = Some([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ]);
        assert!(analyze_bphcl_to_hkcl(&source, &target, 0, 0).is_compatible());
        let converted = convert_bphcl_cloth_to_hkcl_template(&source, &target, 0, 0).unwrap();
        assert_eq!(converted.cloths[0].name.as_deref(), Some("Template"));
        assert_eq!(
            converted.skeletons[0].bones[0].name.as_deref(),
            Some("Root")
        );
        assert_eq!(converted.collidables[0].transform.unwrap()[12], 4.0);
        assert!(matches!(
            converted.collidables[0].shape,
            Some(PhysicsShape::Sphere { radius: 3.0, .. })
        ));
    }

    #[test]
    fn preflight_rejects_multiple_simulations_and_unsupported_shapes() {
        let mut source = graph(PhysicsFormat::Bphcl);
        let extra_simulation = source.cloths[0].simulations[0].clone();
        source.cloths[0].simulations.push(extra_simulation);
        source.collidables[0].shape = Some(PhysicsShape::Unknown {
            class_name: "Unsupported".into(),
            kind: 9,
        });
        let target = graph(PhysicsFormat::Hkcl);
        let report = analyze_bphcl_to_hkcl(&source, &target, 0, 0);
        assert!(report
            .issues
            .iter()
            .any(|value| value.code == "simulation-count"));
        assert!(report
            .issues
            .iter()
            .any(|value| value.code == "collider-shape"));
        assert!(convert_bphcl_cloth_to_hkcl_template(&source, &target, 0, 0).is_err());
    }

    #[test]
    fn cross_format_merge_appends_a_remapped_complete_hkcl_package() {
        let mut source = graph(PhysicsFormat::Bphcl);
        source.collidables[0].translation = Some([4.0, 5.0, 6.0, 1.0]);
        source.collidables[0].axes = Some([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ]);
        let target = graph(PhysicsFormat::Hkcl);
        let original_triangles = target.cloths[0].simulations[0].triangle_indices.clone();

        let merged = merge_bphcl_cloth_into_hkcl(&source, &target, 0, 0).unwrap();

        assert_eq!(merged.source_format, PhysicsFormat::Hkcl);
        assert_eq!(merged.cloths.len(), 2);
        assert_eq!(merged.skeletons.len(), 2);
        assert_eq!(merged.constraints.len(), 2);
        assert_eq!(merged.collidables.len(), 2);
        assert_eq!(merged.cloths[0].name.as_deref(), Some("Source"));
        assert_eq!(merged.cloths[1].name.as_deref(), Some("Template"));
        assert_eq!(
            merged.cloths[1].simulations[0].triangle_indices,
            original_triangles
        );
        assert_eq!(merged.collidables[1].transform.unwrap()[12], 4.0);
        assert!(matches!(
            merged.collidables[1].shape,
            Some(PhysicsShape::Sphere { radius: 3.0, .. })
        ));

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
        let source = graph(PhysicsFormat::Bphcl);
        let mut target = graph(PhysicsFormat::Hkcl);
        target.cloths[0].simulations[0].particles.clear();
        assert!(merge_bphcl_cloth_into_hkcl(&source, &target, 0, 0).is_err());
    }

    #[test]
    fn bphhb_assists_helper_bone_mapping_for_conversion_and_merge() {
        let mut source = graph(PhysicsFormat::Bphcl);
        source.skeletons[0].bones[0].name = Some("Link:HelperHair".into());
        let target = graph(PhysicsFormat::Hkcl);
        let helper = helper_document("HelperHair", "Root");

        assert!(!analyze_bphcl_to_hkcl(&source, &target, 0, 0).is_compatible());
        assert!(analyze_bphcl_to_hkcl_with_bphhb(&source, &target, 0, 0, &helper).is_compatible());
        let merged =
            merge_bphcl_cloth_into_hkcl_with_bphhb(&source, &target, 0, 0, &helper).unwrap();
        assert_eq!(merged.cloths.len(), 2);
        assert_eq!(merged.skeletons.len(), 2);
        assert_eq!(
            merged.skeletons[1].bones[0].name.as_deref(),
            Some("HelperHair")
        );
    }
}
