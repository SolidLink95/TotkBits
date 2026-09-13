//! FBX skeleton reader: the `Model` hierarchy of a binary FBX 7.4/7.5 file
//! reduced to bones (`LimbNode`, `Null` and untyped nodes) with their local
//! `Lcl Translation` / `Lcl Rotation` / `Lcl Scaling` triples.
//!
//! Pivots, `PreRotation`/`PostRotation` and geometric transforms are ignored:
//! the skeleton keeps exactly the animatable local triple, which is what both
//! the bones-only writer and the BFRES import consume. Mesh, camera and
//! light models are skipped, but their children still attach to the nearest
//! kept ancestor so a skeleton parented under a mesh stays connected.

use super::{invalid, Bone, Skeleton};
use crate::parser::binary::BinaryReader;
use fbxcel::tree::v7400::NodeHandle;
use fbxcel_dom::{any::AnyDocument, v7400::object::TypedObjectHandle};
use std::{
    collections::{HashMap, HashSet},
    io,
};

struct FbxModel<'a> {
    name: String,
    subclass: String,
    node: NodeHandle<'a>,
}

pub fn parse(data: &[u8]) -> io::Result<Skeleton> {
    let version = BinaryReader::new(data)
        .read_u32_at(23)
        .map_err(|_| invalid("truncated FBX header"))?;
    let document =
        AnyDocument::from_seekable_reader(std::io::Cursor::new(data)).map_err(|error| {
            invalid(format!(
                "cannot read FBX version {version}: {error} (binary FBX 7.4 or newer is required)"
            ))
        })?;
    let AnyDocument::V7400(_, document) = document else {
        return Err(invalid(format!(
            "unsupported FBX document version {version}: only binary FBX 7.4 or newer can be read"
        )));
    };

    let mut models: HashMap<i64, FbxModel<'_>> = HashMap::new();
    let mut model_order = Vec::new();
    let mut parent_of: HashMap<i64, i64> = HashMap::new();
    for object in document.objects() {
        let TypedObjectHandle::Model(model) = object.get_typed() else {
            continue;
        };
        let id = object.object_id().raw();
        models.insert(
            id,
            FbxModel {
                name: model.name().unwrap_or("").to_string(),
                subclass: object.subclass().to_string(),
                node: object.node(),
            },
        );
        model_order.push(id);
        if let Some(parent) = model.parent_model() {
            parent_of.insert(id, parent.object_id().raw());
        }
    }

    // Children in the order of the Connections section, the same sequencing
    // the Toolbox importer (and Assimp) uses.
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut seen_child = HashSet::new();
    if let Some(connections) = document
        .tree()
        .root()
        .children_by_name("Connections")
        .next()
    {
        for connection in connections.children_by_name("C") {
            let attributes = connection.attributes();
            if attributes.first().and_then(|value| value.get_string()) != Some("OO") {
                continue;
            }
            let (Some(child), Some(parent)) = (
                attributes.get(1).and_then(|value| value.get_i64()),
                attributes.get(2).and_then(|value| value.get_i64()),
            ) else {
                continue;
            };
            if !models.contains_key(&child) || (parent != 0 && !models.contains_key(&parent)) {
                continue;
            }
            if seen_child.insert(child) {
                children.entry(parent).or_default().push(child);
            }
        }
    }
    for id in &model_order {
        if !seen_child.contains(id) {
            let parent = parent_of.get(id).copied().unwrap_or(0);
            children.entry(parent).or_default().push(*id);
        }
    }

    let mut skeleton = Skeleton::default();
    let mut visited = HashSet::new();
    for root in children.get(&0).cloned().unwrap_or_default() {
        visit(root, None, &children, &models, &mut skeleton, &mut visited)?;
    }
    if skeleton.bones.is_empty() {
        return Err(invalid("the FBX contains no skeleton nodes"));
    }
    skeleton.validate()?;
    Ok(skeleton)
}

fn visit(
    id: i64,
    parent: Option<usize>,
    children: &HashMap<i64, Vec<i64>>,
    models: &HashMap<i64, FbxModel<'_>>,
    skeleton: &mut Skeleton,
    visited: &mut HashSet<i64>,
) -> io::Result<()> {
    if !visited.insert(id) {
        return Err(invalid("FBX model hierarchy contains a cycle"));
    }
    let model = models
        .get(&id)
        .ok_or_else(|| invalid("FBX connection references a missing model"))?;
    let kept = matches!(model.subclass.as_str(), "LimbNode" | "Null" | "Root" | "");
    let next_parent = if kept {
        let (translation, rotation, scale) = local_transform(&model.node);
        skeleton.bones.push(Bone {
            name: model.name.clone(),
            parent,
            translation,
            rotation,
            scale,
            skinned: model.subclass == "LimbNode",
        });
        Some(skeleton.bones.len() - 1)
    } else {
        parent
    };
    for child in children.get(&id).cloned().unwrap_or_default() {
        visit(child, next_parent, children, models, skeleton, visited)?;
    }
    Ok(())
}

/// `Lcl Translation`, `Lcl Rotation` and `Lcl Scaling` of a model node, with
/// the FBX defaults when a property is absent.
fn local_transform(node: &NodeHandle<'_>) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let mut translation = [0.0; 3];
    let mut rotation = [0.0; 3];
    let mut scale = [1.0; 3];
    let Some(block) = node.children_by_name("Properties70").next() else {
        return (translation, rotation, scale);
    };
    for property in block.children_by_name("P") {
        let attributes = property.attributes();
        let Some(name) = attributes.first().and_then(|value| value.get_string()) else {
            continue;
        };
        let number = |index: usize| -> Option<f32> {
            attributes.get(index).and_then(|value| {
                value
                    .get_f64()
                    .or_else(|| value.get_f32().map(f64::from))
                    .or_else(|| value.get_i32().map(f64::from))
                    .or_else(|| value.get_i64().map(|v| v as f64))
                    .map(|v| v as f32)
            })
        };
        let (Some(x), Some(y), Some(z)) = (number(4), number(5), number(6)) else {
            continue;
        };
        match name {
            "Lcl Translation" => translation = [x, y, z],
            "Lcl Rotation" => rotation = [x, y, z],
            "Lcl Scaling" => scale = [x, y, z],
            _ => {}
        }
    }
    (translation, rotation, scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str) -> Option<Vec<u8>> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../res/1")
                .join(name),
        )
        .ok()
    }

    #[test]
    fn reads_the_max_merged_skeleton() {
        let Some(data) = fixture("merged005.fbx") else {
            return;
        };
        let skeleton = parse(&data).unwrap();
        assert_eq!(skeleton.bones.len(), 107);
        let names: Vec<&str> = skeleton.bones.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(&names[..4], ["Armature", "Root", "Skl_Root", "Spine_1"]);
        let nulls: Vec<&str> = skeleton
            .bones
            .iter()
            .filter(|b| !b.skinned)
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(
            nulls,
            [
                "Armature",
                "Root",
                "Skl_Root",
                "Wrist_R",
                "Face_Root",
                "Leg_1_L",
                "Leg_1_R"
            ]
        );
        let arm = &skeleton.bones[skeleton.index_of("Arm_1_L").unwrap()];
        assert_eq!(skeleton.bones[arm.parent.unwrap()].name, "Clavicle_L");
        assert!(
            (arm.translation[0] - 0.15).abs() < 1e-6,
            "{:?}",
            arm.translation
        );
        assert!(arm.translation[1].abs() < 1e-6);
        assert!((arm.translation[2] - 0.0107388).abs() < 1e-6);
        assert!(
            arm.rotation.iter().all(|v| v.abs() < 1e-6),
            "{:?}",
            arm.rotation
        );
        assert_eq!(arm.scale, [1.0; 3]);
    }

    #[test]
    fn rejects_the_2013_converter_files_with_a_version_message() {
        let Some(data) = fixture("Armor_005_Head.fbx") else {
            return;
        };
        match parse(&data) {
            Ok(skeleton) => assert!(skeleton.index_of("Hair_A_1").is_some()),
            Err(error) => assert!(error.to_string().contains("7300"), "{error}"),
        }
    }
}
