//! COLLADA (`.dae`) skeleton reader: the `<visual_scene>` node tree without
//! its mesh nodes, plus which joints the `<skin>` controllers reference.

use super::{decompose_matrix, invalid, Bone, Matrix4, Skeleton, IDENTITY};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::HashSet;
use std::io;

/// One `<node>` of the visual scene, kept in document order.
#[derive(Debug, Default)]
struct DaeNode {
    name: String,
    id: String,
    sid: String,
    joint: bool,
    /// Carries geometry, a controller, a camera or a light: not a bone.
    has_instance: bool,
    transform: Matrix4,
    children: Vec<DaeNode>,
}

/// Which element text is being collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextKind {
    Matrix,
    Translate,
    Rotate,
    Scale,
    JointArray,
}

pub fn parse(data: &[u8]) -> io::Result<Skeleton> {
    let data = data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(data);
    let mut reader = Reader::from_reader(data);
    let mut buffer = Vec::new();

    // Element name stack, so context (visual scene, skin) is known.
    let mut path: Vec<String> = Vec::new();
    // Nodes under construction: the last entry is the innermost open <node>.
    let mut open_nodes: Vec<DaeNode> = Vec::new();
    let mut roots: Vec<DaeNode> = Vec::new();
    let mut text: Option<(TextKind, String)> = None;
    let mut joint_ids: HashSet<String> = HashSet::new();
    let mut joint_sids: HashSet<String> = HashSet::new();

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|error| invalid(format!("COLLADA XML error: {error}")))?;
        match event {
            Event::Start(start) => {
                let name = local_name(&start);
                let kind = start_element(&name, &path, &start, &mut open_nodes)?;
                if let Some(kind) = kind {
                    text = Some((kind, String::new()));
                }
                path.push(name);
            }
            Event::Empty(start) => {
                let name = local_name(&start);
                let kind = start_element(&name, &path, &start, &mut open_nodes)?;
                // An empty element has no text; finish it right away.
                path.push(name.clone());
                if let Some(kind) = kind {
                    text = Some((kind, String::new()));
                }
                end_element(
                    &name,
                    &path,
                    &mut open_nodes,
                    &mut roots,
                    &mut text,
                    &mut joint_ids,
                    &mut joint_sids,
                )?;
                path.pop();
            }
            Event::Text(bytes) => {
                if let Some((_, buffer)) = text.as_mut() {
                    let value = bytes
                        .unescape()
                        .map_err(|error| invalid(format!("COLLADA text error: {error}")))?;
                    buffer.push_str(&value);
                }
            }
            Event::CData(bytes) => {
                if let Some((_, buffer)) = text.as_mut() {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                }
            }
            Event::End(end) => {
                let name = String::from_utf8_lossy(end.local_name().as_ref()).into_owned();
                end_element(
                    &name,
                    &path,
                    &mut open_nodes,
                    &mut roots,
                    &mut text,
                    &mut joint_ids,
                    &mut joint_sids,
                )?;
                path.pop();
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    if !open_nodes.is_empty() {
        return Err(invalid("COLLADA visual scene has an unterminated <node>"));
    }
    if roots.is_empty() {
        return Err(invalid("COLLADA file has no <visual_scene> nodes"));
    }

    let mut skeleton = Skeleton::default();
    for root in &roots {
        emit(root, None, &joint_ids, &joint_sids, &mut skeleton);
    }
    if skeleton.bones.is_empty() {
        return Err(invalid("COLLADA visual scene contains no skeleton nodes"));
    }
    skeleton.validate()?;
    Ok(skeleton)
}

fn local_name(start: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(start.local_name().as_ref()).into_owned()
}

fn in_visual_scene(path: &[String]) -> bool {
    path.iter().any(|name| name == "visual_scene")
}

fn in_skin(path: &[String]) -> bool {
    path.iter().any(|name| name == "skin")
}

fn attribute(start: &BytesStart<'_>, key: &str) -> io::Result<Option<String>> {
    for attribute in start.attributes() {
        let attribute =
            attribute.map_err(|error| invalid(format!("COLLADA attribute error: {error}")))?;
        if attribute.key.local_name().as_ref() == key.as_bytes() {
            let value = attribute
                .unescape_value()
                .map_err(|error| invalid(format!("COLLADA attribute error: {error}")))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

/// Handles the opening of an element; returns the kind of text to collect.
fn start_element(
    name: &str,
    path: &[String],
    start: &BytesStart<'_>,
    open_nodes: &mut Vec<DaeNode>,
) -> io::Result<Option<TextKind>> {
    if in_visual_scene(path) {
        match name {
            "node" => {
                let id = attribute(start, "id")?.unwrap_or_default();
                let sid = attribute(start, "sid")?.unwrap_or_default();
                let name = attribute(start, "name")?
                    .filter(|value| !value.is_empty())
                    .or_else(|| (!sid.is_empty()).then(|| sid.clone()))
                    .unwrap_or_else(|| id.clone());
                let joint = attribute(start, "type")?.as_deref() == Some("JOINT");
                open_nodes.push(DaeNode {
                    name,
                    id,
                    sid,
                    joint,
                    has_instance: false,
                    transform: IDENTITY,
                    children: Vec::new(),
                });
                return Ok(None);
            }
            "instance_geometry" | "instance_controller" | "instance_camera" | "instance_light" => {
                if let Some(node) = open_nodes.last_mut() {
                    node.has_instance = true;
                }
                return Ok(None);
            }
            "matrix" if !open_nodes.is_empty() => return Ok(Some(TextKind::Matrix)),
            "translate" if !open_nodes.is_empty() => return Ok(Some(TextKind::Translate)),
            "rotate" if !open_nodes.is_empty() => return Ok(Some(TextKind::Rotate)),
            "scale" if !open_nodes.is_empty() => return Ok(Some(TextKind::Scale)),
            _ => return Ok(None),
        }
    }
    if in_skin(path) && (name == "Name_array" || name == "IDREF_array") {
        return Ok(Some(TextKind::JointArray));
    }
    Ok(None)
}

fn end_element(
    name: &str,
    path: &[String],
    open_nodes: &mut Vec<DaeNode>,
    roots: &mut Vec<DaeNode>,
    text: &mut Option<(TextKind, String)>,
    joint_ids: &mut HashSet<String>,
    joint_sids: &mut HashSet<String>,
) -> io::Result<()> {
    if name == "node" && in_visual_scene(path) {
        let Some(node) = open_nodes.pop() else {
            return Err(invalid(
                "COLLADA visual scene closes a <node> it never opened",
            ));
        };
        match open_nodes.last_mut() {
            Some(parent) => parent.children.push(node),
            None => roots.push(node),
        }
        return Ok(());
    }
    let Some((kind, buffer)) = text.take() else {
        return Ok(());
    };
    match kind {
        TextKind::JointArray => {
            for joint in buffer.split_whitespace() {
                if name == "IDREF_array" {
                    joint_ids.insert(joint.to_owned());
                } else {
                    joint_sids.insert(joint.to_owned());
                }
            }
        }
        TextKind::Matrix | TextKind::Translate | TextKind::Rotate | TextKind::Scale => {
            let values = parse_floats(&buffer)?;
            let operation = match kind {
                TextKind::Matrix => matrix_from_values(&values)?,
                TextKind::Translate => translation(&values)?,
                TextKind::Rotate => axis_angle(&values)?,
                TextKind::Scale => scaling(&values)?,
                TextKind::JointArray => IDENTITY,
            };
            if let Some(node) = open_nodes.last_mut() {
                // COLLADA applies a node's transform elements in document
                // order, each one multiplying on the right.
                node.transform = multiply(&node.transform, &operation);
            }
        }
    }
    Ok(())
}

fn parse_floats(text: &str) -> io::Result<Vec<f32>> {
    text.split_whitespace()
        .map(|value| {
            value.parse::<f32>().map_err(|_| {
                invalid(format!(
                    "COLLADA transform has a non-numeric value {value:?}"
                ))
            })
        })
        .collect()
}

fn matrix_from_values(values: &[f32]) -> io::Result<Matrix4> {
    if values.len() != 16 {
        return Err(invalid(format!(
            "COLLADA <matrix> has {} values instead of 16",
            values.len()
        )));
    }
    let mut m = IDENTITY;
    for (index, value) in values.iter().enumerate() {
        m[index / 4][index % 4] = *value;
    }
    Ok(m)
}

fn translation(values: &[f32]) -> io::Result<Matrix4> {
    let [x, y, z] = three(values, "translate")?;
    let mut m = IDENTITY;
    m[0][3] = x;
    m[1][3] = y;
    m[2][3] = z;
    Ok(m)
}

fn scaling(values: &[f32]) -> io::Result<Matrix4> {
    let [x, y, z] = three(values, "scale")?;
    let mut m = IDENTITY;
    m[0][0] = x;
    m[1][1] = y;
    m[2][2] = z;
    Ok(m)
}

fn three(values: &[f32], element: &str) -> io::Result<[f32; 3]> {
    match values {
        [x, y, z] => Ok([*x, *y, *z]),
        _ => Err(invalid(format!(
            "COLLADA <{element}> has {} values instead of 3",
            values.len()
        ))),
    }
}

/// `<rotate x y z angle>`: a rotation of `angle` degrees about the axis.
fn axis_angle(values: &[f32]) -> io::Result<Matrix4> {
    let [x, y, z, angle] = match values {
        [x, y, z, angle] => [*x as f64, *y as f64, *z as f64, *angle as f64],
        _ => {
            return Err(invalid(format!(
                "COLLADA <rotate> has {} values instead of 4",
                values.len()
            )))
        }
    };
    let length = (x * x + y * y + z * z).sqrt();
    if length < f64::EPSILON {
        return Ok(IDENTITY);
    }
    let (x, y, z) = (x / length, y / length, z / length);
    let (s, c) = angle.to_radians().sin_cos();
    let t = 1.0 - c;
    let r = [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
    ];
    let mut m = IDENTITY;
    for row in 0..3 {
        for col in 0..3 {
            m[row][col] = r[row][col] as f32;
        }
    }
    Ok(m)
}

fn multiply(a: &Matrix4, b: &Matrix4) -> Matrix4 {
    let mut out = [[0.0f32; 4]; 4];
    for row in 0..4 {
        for col in 0..4 {
            let mut sum = 0.0f64;
            for k in 0..4 {
                sum += a[row][k] as f64 * b[k][col] as f64;
            }
            out[row][col] = sum as f32;
        }
    }
    out
}

/// Appends `node` (when it is a bone) and its descendants in document order.
fn emit(
    node: &DaeNode,
    parent: Option<usize>,
    joint_ids: &HashSet<String>,
    joint_sids: &HashSet<String>,
    skeleton: &mut Skeleton,
) {
    let kept = node.joint || !node.has_instance;
    let mut next_parent = parent;
    if kept {
        let (translation, rotation, scale) = decompose_matrix(&node.transform);
        let skinned = joint_ids.contains(&node.id)
            || (!node.sid.is_empty() && joint_sids.contains(&node.sid))
            || (node.sid.is_empty() && joint_sids.contains(&node.name));
        skeleton.bones.push(Bone {
            name: node.name.clone(),
            parent,
            translation,
            rotation,
            scale,
            skinned,
        });
        next_parent = Some(skeleton.bones.len() - 1);
    }
    for child in &node.children {
        emit(child, next_parent, joint_ids, joint_sids, skeleton);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> Option<Vec<u8>> {
        let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../res/1")
            .join(name);
        std::fs::read(path).ok()
    }

    fn close(a: [f32; 3], b: [f32; 3], tolerance: f32) -> bool {
        (0..3).all(|axis| (a[axis] - b[axis]).abs() <= tolerance)
    }

    #[test]
    fn parses_inline_scene_with_translate_rotate_scale() {
        let xml = r##"<?xml version="1.0"?>
<COLLADA xmlns="http://www.collada.org/2005/11/COLLADASchema" version="1.4.1">
  <library_controllers>
    <controller id="c"><skin source="#m"><source id="j"><Name_array id="ja" count="1">Bone</Name_array></source></skin></controller>
  </library_controllers>
  <library_visual_scenes><visual_scene id="S" name="S">
    <node id="Armature" name="Armature" type="NODE">
      <node id="Armature_Bone" name="Bone" sid="Bone" type="JOINT">
        <translate>1 2 3</translate>
        <rotate sid="rz">0 0 1 90</rotate>
        <scale>2 2 2</scale>
      </node>
      <node id="mesh" name="mesh" type="NODE"><instance_geometry url="#m"/></node>
    </node>
  </visual_scene></library_visual_scenes>
</COLLADA>"##;
        let skeleton = parse(xml.as_bytes()).unwrap();
        assert_eq!(skeleton.bones.len(), 2);
        assert_eq!(skeleton.bones[0].name, "Armature");
        assert!(!skeleton.bones[0].skinned);
        let bone = &skeleton.bones[1];
        assert_eq!(bone.parent, Some(0));
        assert!(bone.skinned);
        assert!(close(bone.translation, [1.0, 2.0, 3.0], 1e-6));
        assert!(
            close(bone.rotation, [0.0, 0.0, 90.0], 1e-3),
            "{:?}",
            bone.rotation
        );
        assert!(close(bone.scale, [2.0, 2.0, 2.0], 1e-5), "{:?}", bone.scale);
    }

    #[test]
    fn rejects_non_collada_and_malformed_input() {
        assert!(parse(b"<html/>").is_err());
        assert!(parse(b"<COLLADA><library_visual_scenes><visual_scene><node><matrix>1 2</matrix></node></visual_scene></library_visual_scenes></COLLADA>").is_err());
    }

    #[test]
    fn head_fixture_has_expected_bones() {
        let Some(data) = fixture("Armor_005_Head.dae") else {
            return;
        };
        let skeleton = parse(&data).unwrap();
        assert_eq!(skeleton.bones.len(), 50);
        assert_eq!(skeleton.bones.iter().filter(|b| b.skinned).count(), 39);
        let names: Vec<&str> = skeleton
            .bones
            .iter()
            .take(4)
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(names, ["Armature", "Root", "Skl_Root", "Spine_1"]);
        let arm = skeleton.index_of("Arm_1_L").unwrap();
        let bone = &skeleton.bones[arm];
        assert_eq!(skeleton.bones[bone.parent.unwrap()].name, "Clavicle_L");
        assert!(
            close(bone.translation, [0.15, 0.0, 0.0107387], 1e-6),
            "{:?}",
            bone.translation
        );
        let hair = &skeleton.bones[skeleton.index_of("Hair_A_2").unwrap()];
        assert!(
            close(hair.rotation, [-40.0, -12.0, -60.0], 1e-3),
            "{:?}",
            hair.rotation
        );
        assert!(skeleton.index_of("Hair_1_005__Mt_Hair_005").is_none());
    }

    #[test]
    fn upper_fixture_has_expected_bones() {
        let Some(data) = fixture("Armor_005_RaulSkin_Upper.dae") else {
            return;
        };
        let skeleton = parse(&data).unwrap();
        assert_eq!(skeleton.bones.len(), 70);
        assert_eq!(skeleton.bones.iter().filter(|b| b.skinned).count(), 63);
        assert!(!skeleton.bones[skeleton.index_of("Wrist_R").unwrap()].skinned);
        assert!(skeleton.bones[skeleton.index_of("Chin").unwrap()].skinned);
        assert!(!skeleton.bones[skeleton.index_of("Face_Root").unwrap()].skinned);
    }
}
