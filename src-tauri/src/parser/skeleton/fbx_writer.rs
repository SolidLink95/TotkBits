//! Bones-only FBX 7.4 writer. The records mirror what 3ds Max emits for a
//! skeleton (`Model` version 232 with the `InheritType` / `ScalingMax` /
//! `DefaultAttributeIndex` / `Lcl *` properties, one `NodeAttribute` per
//! model) so the file opens in Max, Blender and the Toolbox-compatible
//! BFRES skeleton import alike. Object ids are deterministic, which keeps
//! two writes of the same skeleton byte-identical apart from the timestamp
//! the header carries.

use super::{invalid, Skeleton};
use crate::parser::fbx::binary::{
    p_color_rgb, p_double, p_enum, p_int, p_lcl, p_object, p_string, p_vector, properties70,
    write_document, Attr, Node, CREATION_TIME, FILE_ID,
};
use crate::parser::fbx::export::{global_settings, header_extension, node_template, CREATOR};
use std::io;

const DOCUMENT_ID: i64 = 999_999;
const FIRST_OBJECT_ID: i64 = 1_000_000;

pub fn write(skeleton: &Skeleton, document_name: &str) -> io::Result<Vec<u8>> {
    skeleton.validate()?;
    if skeleton.bones.is_empty() {
        return Err(invalid("cannot write an FBX without bones"));
    }
    let model_id = |index: usize| FIRST_OBJECT_ID + 2 * index as i64;
    let attribute_id = |index: usize| FIRST_OBJECT_ID + 2 * index as i64 + 1;

    let mut objects = Node::new("Objects");
    let mut connections = Node::new("Connections");
    for (index, bone) in skeleton.bones.iter().enumerate() {
        let kind = if bone.skinned { "LimbNode" } else { "Null" };
        objects.push(node_attribute(
            attribute_id(index),
            bone.skinned,
            size_of(skeleton, index),
        ));
        objects.push(
            Node::object("Model", model_id(index), &bone.name, "Model", kind)
                .child(Node::leaf("Version", 232i32))
                .child(properties70([
                    p_enum("InheritType", 1),
                    p_vector("ScalingMax", [0.0; 3]),
                    p_int("DefaultAttributeIndex", 0),
                    p_lcl("Lcl Translation", bone.translation.map(f64::from)),
                    p_lcl("Lcl Rotation", bone.rotation.map(f64::from)),
                    p_lcl("Lcl Scaling", bone.scale.map(f64::from)),
                ]))
                .child(Node::leaf("Shading", true))
                .child(Node::leaf("Culling", "CullingOff")),
        );
    }
    for (index, bone) in skeleton.bones.iter().enumerate() {
        connections.push(connection(attribute_id(index), model_id(index)));
        connections.push(connection(
            model_id(index),
            bone.parent.map(model_id).unwrap_or(0),
        ));
    }

    let bone_count = skeleton.bones.len();
    let definitions = Node::new("Definitions")
        .child(Node::leaf("Version", 100i32))
        .child(Node::leaf("Count", (1 + 2 * bone_count) as i32))
        .child(Node::leaf("ObjectType", "GlobalSettings").child(Node::leaf("Count", 1i32)))
        .child(
            Node::leaf("ObjectType", "NodeAttribute")
                .child(Node::leaf("Count", bone_count as i32))
                .child(
                    Node::leaf("PropertyTemplate", "FbxNull").child(properties70(null_template())),
                ),
        )
        .child(
            Node::leaf("ObjectType", "Model")
                .child(Node::leaf("Count", bone_count as i32))
                .child(
                    Node::leaf("PropertyTemplate", "FbxNode").child(properties70(node_template())),
                ),
        );

    let document_url = format!("{document_name}.fbx");
    write_document(&[
        header_extension(&document_url),
        Node::leaf("FileId", Attr::Raw(FILE_ID.to_vec())),
        Node::leaf("CreationTime", CREATION_TIME),
        Node::leaf("Creator", CREATOR),
        global_settings(),
        Node::new("Documents")
            .child(Node::leaf("Count", 1i32))
            .child(
                Node::object("Document", DOCUMENT_ID, "Scene", "", "Scene")
                    .child(properties70([
                        p_object("SourceObject"),
                        p_string("ActiveAnimStackName", ""),
                    ]))
                    .child(Node::leaf("RootNode", 0i64)),
            ),
        Node::new("References"),
        definitions,
        objects,
        connections,
        Node::new("Takes").child(Node::leaf("Current", "")),
    ])
}

/// Bone display length: the distance to the first child, as a reasonable
/// stand-in for the value Max derives from its own bone geometry.
fn size_of(skeleton: &Skeleton, index: usize) -> f64 {
    skeleton
        .children(index)
        .first()
        .and_then(|child| skeleton.bones.get(*child))
        .map(|child| {
            let t = child.translation;
            f64::from(t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt()
        })
        .filter(|length| *length > 0.0)
        .unwrap_or(1.0)
}

fn node_attribute(id: i64, skinned: bool, size: f64) -> Node {
    if skinned {
        Node::object("NodeAttribute", id, "", "NodeAttribute", "LimbNode")
            .child(properties70([p_double("Size", size)]))
            .child(Node::leaf("TypeFlags", "Skeleton"))
    } else {
        Node::object("NodeAttribute", id, "", "NodeAttribute", "Null")
            .child(properties70(null_template()))
            .child(Node::leaf("TypeFlags", "Null"))
    }
}

fn null_template() -> Vec<Node> {
    vec![
        p_color_rgb("Color", [0.8; 3]),
        p_double("Size", 100.0),
        p_enum("Look", 1),
    ]
}

fn connection(child: i64, parent: i64) -> Node {
    Node::new("C").attr("OO").attr(child).attr(parent)
}

#[cfg(test)]
mod tests {
    use super::super::{fbx, Bone};
    use super::*;
    use std::path::Path;

    fn sample() -> Skeleton {
        let mut bones = vec![
            Bone::new("Armature", None),
            Bone::new("Root", Some(0)),
            Bone::new("Skl_Root", Some(1)),
            Bone::new("Spine_1", Some(2)),
            Bone::new("Clavicle_L", Some(3)),
            Bone::new("Arm_1_L", Some(4)),
        ];
        bones[2].translation = [0.0, 0.9942646, 0.0];
        bones[3].rotation = [90.0, 0.0, 90.0];
        bones[3].scale = [1.0000002, 1.0000002, 1.0000002];
        bones[4].translation = [0.2396068, -1.64349e-5, 0.0329087];
        bones[4].rotation = [0.0, -90.0, 0.0];
        bones[5].translation = [0.15, 0.0, 0.0107387];
        bones[5].skinned = true;
        bones[4].skinned = true;
        bones[3].skinned = true;
        Skeleton { bones }
    }

    #[test]
    fn round_trips_through_the_reader() {
        let skeleton = sample();
        let bytes = write(&skeleton, "sample").unwrap();
        assert!(bytes.starts_with(b"Kaydara FBX Binary"));
        let reread = fbx::parse(&bytes).unwrap();
        assert_eq!(reread, skeleton);
    }

    #[test]
    fn round_trips_through_the_toolbox_importer() {
        let skeleton = sample();
        let bytes = write(&skeleton, "sample").unwrap();
        let imported =
            crate::parser::fbx::toolbox_skeleton::import_skeleton_like_toolbox(&bytes).unwrap();
        let names: Vec<&str> = imported.bones.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            names,
            ["Root", "Skl_Root", "Spine_1", "Clavicle_L", "Arm_1_L"]
        );
        for (imported_bone, bone) in imported.bones.iter().zip(&skeleton.bones[1..]) {
            for axis in 0..3 {
                assert!(
                    (imported_bone.position[axis] - bone.translation[axis]).abs() < 1e-6,
                    "{}: {:?} vs {:?}",
                    bone.name,
                    imported_bone.position,
                    bone.translation
                );
            }
        }
        assert_eq!(imported.bones[0].parent_index, -1);
        assert_eq!(imported.bones[4].parent_index, 3);
        assert!(imported.mesh_bone_usage.is_empty());
    }

    #[test]
    fn rewrites_the_max_merged_skeleton_losslessly() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../res/1/merged005.fbx");
        let Ok(data) = std::fs::read(path) else {
            return;
        };
        let first = fbx::parse(&data).unwrap();
        let bytes = write(&first, "merged005").unwrap();
        let second = fbx::parse(&bytes).unwrap();
        assert_eq!(second, first);
        let output =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_CLAUDE/merged005.bones.fbx");
        if let Some(parent) = output.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(output, bytes);
    }

    #[test]
    fn refuses_an_empty_skeleton() {
        assert!(write(&Skeleton::default(), "empty").is_err());
    }
}
