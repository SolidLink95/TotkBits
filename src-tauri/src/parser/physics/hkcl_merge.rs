use crate::parser::physics::physics_graph::{
    FormatNeutralPhysicsGraph, PhysicsCollidable, PhysicsFormat, PhysicsId, SkeletonBinding,
};
use std::{collections::BTreeMap, io};

pub fn merge_complete_hkcl_cloth(
    target: &FormatNeutralPhysicsGraph,
    source: &FormatNeutralPhysicsGraph,
    cloth_index: usize,
) -> io::Result<FormatNeutralPhysicsGraph> {
    require_hkcl(target, source)?;
    let cloth = source
        .cloths
        .get(cloth_index)
        .ok_or_else(|| invalid("source HKCL cloth index is out of range"))?;
    let skeleton_index = paired_skeleton_index(source, cloth_index)
        .ok_or_else(|| invalid("source HKCL cloth has no paired skeleton"))?;
    let skeleton = &source.skeletons[skeleton_index];
    let mut merged = target.clone();
    let prefix = unique_prefix(&merged, "cloth");
    let mut remap = BTreeMap::<PhysicsId, PhysicsId>::new();

    let skeleton_id = mapped_id(&prefix, &skeleton.id);
    remap.insert(skeleton.id.clone(), skeleton_id.clone());
    let mut copied_skeleton = skeleton.clone();
    copied_skeleton.id = skeleton_id.clone();
    merged.skeletons.push(copied_skeleton);

    for simulation in &cloth.simulations {
        remap.insert(simulation.id.clone(), mapped_id(&prefix, &simulation.id));
        for constraint_id in &simulation.constraints {
            if remap.contains_key(constraint_id) {
                continue;
            }
            let constraint = source
                .constraints
                .iter()
                .find(|value| &value.id == constraint_id)
                .ok_or_else(|| invalid("source cloth references a missing constraint"))?;
            let new_id = mapped_id(&prefix, constraint_id);
            remap.insert(constraint_id.clone(), new_id.clone());
            let mut copied = constraint.clone();
            copied.id = new_id;
            merged.constraints.push(copied);
        }
        for collidable_id in &simulation.collidables {
            if remap.contains_key(collidable_id) {
                continue;
            }
            let collidable = source
                .collidables
                .iter()
                .find(|value| &value.id == collidable_id)
                .ok_or_else(|| invalid("source cloth references a missing collidable"))?;
            let target_id = merge_collidable_value(&mut merged, collidable, &prefix)?;
            remap.insert(collidable_id.clone(), target_id);
        }
    }

    let cloth_id = mapped_id(&prefix, &cloth.id);
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
    merged.skeleton_bindings.push(SkeletonBinding {
        cloth: cloth_id,
        skeleton: skeleton_id,
    });
    validate_references(&merged)?;
    Ok(merged)
}

pub fn merge_standalone_hkcl_collidable(
    target: &FormatNeutralPhysicsGraph,
    source: &FormatNeutralPhysicsGraph,
    collidable_index: usize,
) -> io::Result<FormatNeutralPhysicsGraph> {
    require_hkcl(target, source)?;
    let collidable = source
        .collidables
        .get(collidable_index)
        .ok_or_else(|| invalid("source HKCL collidable index is out of range"))?;
    let mut merged = target.clone();
    let prefix = unique_prefix(&merged, "collidable");
    merge_collidable_value(&mut merged, collidable, &prefix)?;
    validate_references(&merged)?;
    Ok(merged)
}

fn merge_collidable_value(
    graph: &mut FormatNeutralPhysicsGraph,
    source: &PhysicsCollidable,
    prefix: &str,
) -> io::Result<PhysicsId> {
    if let Some(existing) = graph
        .collidables
        .iter()
        .find(|value| same_name(value.name.as_deref(), source.name.as_deref()))
    {
        if equivalent_collidable(existing, source) {
            return Ok(existing.id.clone());
        }
        return Err(invalid(&format!(
            "target already has a different HKCL collidable named '{}'",
            source.name.as_deref().unwrap_or("(unnamed)")
        )));
    }
    let id = mapped_id(prefix, &source.id);
    let mut copied = source.clone();
    copied.id = id.clone();
    graph.collidables.push(copied);
    Ok(id)
}

fn equivalent_collidable(a: &PhysicsCollidable, b: &PhysicsCollidable) -> bool {
    a.name == b.name
        && a.transform == b.transform
        && a.translation == b.translation
        && a.axes == b.axes
        && a.linear_velocity == b.linear_velocity
        && a.angular_velocity == b.angular_velocity
        && a.enabled == b.enabled
        && a.pinch_detection == b.pinch_detection
        && a.shape == b.shape
}

fn paired_skeleton_index(graph: &FormatNeutralPhysicsGraph, cloth_index: usize) -> Option<usize> {
    let cloth = graph.cloths.get(cloth_index)?;
    graph
        .skeleton_bindings
        .iter()
        .find(|binding| binding.cloth == cloth.id)
        .and_then(|binding| {
            graph
                .skeletons
                .iter()
                .position(|skeleton| skeleton.id == binding.skeleton)
        })
        .or_else(|| (cloth_index < graph.skeletons.len()).then_some(cloth_index))
}

fn validate_references(graph: &FormatNeutralPhysicsGraph) -> io::Result<()> {
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

fn require_hkcl(
    target: &FormatNeutralPhysicsGraph,
    source: &FormatNeutralPhysicsGraph,
) -> io::Result<()> {
    if target.source_format != PhysicsFormat::Hkcl || source.source_format != PhysicsFormat::Hkcl {
        Err(invalid("same-format HKCL merge requires two HKCL graphs"))
    } else {
        Ok(())
    }
}

fn unique_prefix(graph: &FormatNeutralPhysicsGraph, kind: &str) -> String {
    let mut serial = graph.cloths.len()
        + graph.skeletons.len()
        + graph.constraints.len()
        + graph.collidables.len();
    loop {
        let prefix = format!("hkcl-merge:{kind}:{serial}");
        if !all_ids(graph).any(|id| id.0.starts_with(&prefix)) {
            return prefix;
        }
        serial += 1;
    }
}

fn all_ids(graph: &FormatNeutralPhysicsGraph) -> impl Iterator<Item = &PhysicsId> {
    graph
        .cloths
        .iter()
        .map(|value| &value.id)
        .chain(graph.skeletons.iter().map(|value| &value.id))
        .chain(graph.constraints.iter().map(|value| &value.id))
        .chain(graph.collidables.iter().map(|value| &value.id))
}

fn mapped_id(prefix: &str, source: &PhysicsId) -> PhysicsId {
    PhysicsId(format!("{prefix}:{}", source.0))
}

fn same_name(a: Option<&str>, b: Option<&str>) -> bool {
    a.zip(b).is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::physics::hkcl_to_bphcl::tests::graph;

    #[test]
    fn complete_cloth_merge_copies_and_remaps_the_reachable_package() {
        let target = graph(PhysicsFormat::Hkcl);
        let mut source = graph(PhysicsFormat::Hkcl);
        source.cloths[0].name = Some("Donor".into());
        source.collidables[0].name = Some("DonorCollider".into());
        let merged = merge_complete_hkcl_cloth(&target, &source, 0).unwrap();
        assert_eq!(merged.cloths.len(), 2);
        assert_eq!(merged.skeletons.len(), 2);
        assert_eq!(merged.constraints.len(), 2);
        assert_eq!(merged.collidables.len(), 2);
        let imported = &merged.cloths[1].simulations[0];
        assert!(merged
            .constraints
            .iter()
            .any(|value| value.id == imported.constraints[0]));
        assert!(merged
            .collidables
            .iter()
            .any(|value| value.id == imported.collidables[0]));
        assert_eq!(merged.skeleton_bindings.len(), 2);
    }

    #[test]
    fn standalone_collidable_reuses_equal_names_and_rejects_conflicts() {
        let target = graph(PhysicsFormat::Hkcl);
        let source = graph(PhysicsFormat::Hkcl);
        assert_eq!(
            merge_standalone_hkcl_collidable(&target, &source, 0)
                .unwrap()
                .collidables
                .len(),
            1
        );
        let mut conflict = source;
        conflict.collidables[0].enabled = false;
        assert!(merge_standalone_hkcl_collidable(&target, &conflict, 0).is_err());
    }
}
