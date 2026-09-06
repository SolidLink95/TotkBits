use crate::parser::physics::{
    bphhb::{BphhbBone, BphhbDocument},
    physics_graph::PhysicsSkeleton,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BphhbBoneMapping {
    pub source_to_target: BTreeMap<usize, usize>,
    pub unresolved_source: Vec<usize>,
}

impl BphhbBoneMapping {
    pub fn is_complete(&self) -> bool {
        self.unresolved_source.is_empty()
    }
}

pub fn map_skeleton_bones(
    source: &PhysicsSkeleton,
    target: &PhysicsSkeleton,
    helper: &BphhbDocument,
) -> BphhbBoneMapping {
    map_bones(source, target, &helper.bones)
}

fn map_bones(
    source: &PhysicsSkeleton,
    target: &PhysicsSkeleton,
    helper_bones: &[BphhbBone],
) -> BphhbBoneMapping {
    let helper_by_name: BTreeMap<_, _> = helper_bones
        .iter()
        .enumerate()
        .map(|(index, bone)| (normalize(&bone.name), index))
        .collect();
    let mut used = BTreeSet::new();
    let mut source_to_target = BTreeMap::new();
    let mut unresolved_source = Vec::new();

    for (source_index, source_bone) in source.bones.iter().enumerate() {
        let Some(source_name) = source_bone.name.as_deref() else {
            unresolved_source.push(source_index);
            continue;
        };
        let source_names = ancestry_names(source_name, helper_bones, &helper_by_name);
        let mut candidates: Vec<_> = target
            .bones
            .iter()
            .enumerate()
            .filter(|(target_index, _)| !used.contains(target_index))
            .filter_map(|(target_index, target_bone)| {
                let target_name = target_bone.name.as_deref()?;
                let target_names = ancestry_names(target_name, helper_bones, &helper_by_name);
                let exact = normalize(source_name) == normalize(target_name);
                let related = source_names.iter().any(|name| target_names.contains(name));
                related.then_some((!exact, target_index))
            })
            .collect();
        candidates.sort_unstable();
        if let Some((_, target_index)) = candidates.first().copied() {
            used.insert(target_index);
            source_to_target.insert(source_index, target_index);
        } else {
            unresolved_source.push(source_index);
        }
    }
    BphhbBoneMapping {
        source_to_target,
        unresolved_source,
    }
}

fn ancestry_names(
    name: &str,
    bones: &[BphhbBone],
    by_name: &BTreeMap<String, usize>,
) -> BTreeSet<String> {
    let mut names = BTreeSet::from([normalize(name)]);
    let mut current = by_name.get(&normalize(name)).copied();
    let mut seen = BTreeSet::new();
    while let Some(index) = current {
        if !seen.insert(index) {
            break;
        }
        let bone = &bones[index];
        let parent_name = bone.parent_name.as_deref().map(normalize);
        if let Some(parent) = &parent_name {
            names.insert(parent.clone());
        }
        current = bone.parent_index.or_else(|| {
            parent_name
                .as_ref()
                .and_then(|name| by_name.get(name).copied())
        });
    }
    names
}

fn normalize(name: &str) -> String {
    name.strip_prefix("Link:")
        .unwrap_or(name)
        .trim()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::physics::{
        bphhb::BphhbTransform, hkcl_to_bphcl::tests::graph, physics_graph::PhysicsFormat,
    };

    #[test]
    fn maps_helper_names_through_their_parent_chain() {
        let mut source = graph(PhysicsFormat::Bphcl).skeletons.remove(0);
        source.bones[0].name = Some("Link:HelperHair".into());
        let target = graph(PhysicsFormat::Hkcl).skeletons.remove(0);
        let helper = BphhbBone {
            name: "HelperHair".into(),
            parent_name: Some("Root".into()),
            parent_index: None,
            transform: BphhbTransform::default(),
            object_path: vec![1],
            metadata: BTreeMap::new(),
        };
        let mapping = map_bones(&source, &target, &[helper]);
        assert!(mapping.is_complete());
        assert_eq!(mapping.source_to_target.get(&0), Some(&0));
    }
}
