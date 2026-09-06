use super::BphhbHeader;
use roead::aamp::{Parameter, ParameterIO, ParameterList, ParameterObject};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    io,
};

use crate::parser::physics::hkcl::HkclLeaf;

#[derive(Serialize)]
struct BphhbYaml<'a> {
    format: &'static str,
    metadata: &'a BphhbMetadata,
    bones: &'a [BphhbBone],
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum BphhbValue {
    Bool(bool),
    Float(f32),
    Signed(i32),
    Unsigned(u32),
    Vector2([f32; 2]),
    Vector3([f32; 3]),
    Vector4([f32; 4]),
    Quaternion([f32; 4]),
    String(String),
    SignedBuffer(Vec<i32>),
    UnsignedBuffer(Vec<u32>),
    FloatBuffer(Vec<f32>),
    Binary(Vec<u8>),
    Other(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BphhbTransform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl Default for BphhbTransform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BphhbBone {
    pub name: String,
    pub parent_name: Option<String>,
    pub parent_index: Option<usize>,
    pub transform: BphhbTransform,
    pub object_path: Vec<u32>,
    pub metadata: BTreeMap<u32, BphhbValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BphhbMetadata {
    pub archive_version: u32,
    pub parameter_io_version: u32,
    pub data_version: u32,
    pub data_type: String,
    pub list_count: u32,
    pub object_count: u32,
    pub parameter_count: u32,
    pub data_size: u32,
    pub string_pool_size: u32,
    pub string_pool: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct BphhbDocument {
    pub raw: Vec<u8>,
    pub header: BphhbHeader,
    pub metadata: BphhbMetadata,
    pub bones: Vec<BphhbBone>,
}

impl BphhbDocument {
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        let header = BphhbHeader::read(data)?;
        let archive = ParameterIO::from_binary(data).map_err(io::Error::other)?;
        let string_pool = read_string_pool(data, header.string_pool_size)?;
        let metadata = BphhbMetadata {
            archive_version: header.archive_version,
            parameter_io_version: header.parameter_io_version,
            data_version: archive.version,
            data_type: archive.data_type.to_string(),
            list_count: header.list_count,
            object_count: header.object_count,
            parameter_count: header.parameter_count,
            data_size: header.data_size,
            string_pool_size: header.string_pool_size,
            string_pool,
        };
        let mut bones = Vec::new();
        collect_bones(&archive.param_root, &mut Vec::new(), &mut bones);
        for index in 0..bones.len() {
            if bones[index].parent_index.is_none() {
                bones[index].parent_index = bones[index]
                    .parent_name
                    .as_ref()
                    .and_then(|parent| bones.iter().position(|bone| bone.name == *parent));
            }
        }
        Ok(Self {
            raw: data.to_vec(),
            header,
            metadata,
            bones,
        })
    }

    pub fn validate(&self) -> io::Result<()> {
        if self.header.file_size as usize != self.raw.len() {
            return Err(invalid(&format!(
                "BPHHB file size declares {} bytes but input has {}",
                self.header.file_size,
                self.raw.len()
            )));
        }
        if !self.header.data_type.eq_ignore_ascii_case("phhb")
            || !self.metadata.data_type.eq_ignore_ascii_case("phhb")
        {
            return Err(invalid("BPHHB data type is not phhb"));
        }
        if self.metadata.archive_version != self.header.archive_version
            || self.metadata.parameter_io_version != self.header.parameter_io_version
            || self.metadata.list_count != self.header.list_count
            || self.metadata.object_count != self.header.object_count
            || self.metadata.parameter_count != self.header.parameter_count
            || self.metadata.data_size != self.header.data_size
            || self.metadata.string_pool_size != self.header.string_pool_size
        {
            return Err(invalid("BPHHB metadata does not match its header"));
        }
        let archive = ParameterIO::from_binary(&self.raw).map_err(io::Error::other)?;
        let counts = structure_counts(&archive.param_root);
        if counts
            != (
                self.header.list_count,
                self.header.object_count,
                self.header.parameter_count,
            )
        {
            return Err(invalid("BPHHB structure counts do not match its header"));
        }

        let mut names = HashSet::new();
        let mut paths = HashSet::new();
        for (index, bone) in self.bones.iter().enumerate() {
            if bone.name.trim().is_empty() || !names.insert(bone.name.as_str()) {
                return Err(invalid("BPHHB bone names must be non-empty and unique"));
            }
            if bone.object_path.is_empty() || !paths.insert(bone.object_path.as_slice()) {
                return Err(invalid(
                    "BPHHB bone object paths must be non-empty and unique",
                ));
            }
            if bone.parent_name.is_some() && bone.parent_index.is_none() {
                return Err(invalid("BPHHB bone references a missing named parent"));
            }
            if bone
                .parent_index
                .is_some_and(|parent| parent >= self.bones.len() || parent == index)
            {
                return Err(invalid("BPHHB bone parent index is invalid"));
            }
            if !bone
                .transform
                .translation
                .iter()
                .chain(&bone.transform.rotation)
                .chain(&bone.transform.scale)
                .all(|value| value.is_finite())
            {
                return Err(invalid("BPHHB bone transform contains a non-finite value"));
            }
            if bone.metadata.values().any(value_has_non_finite_float) {
                return Err(invalid("BPHHB bone metadata contains a non-finite value"));
            }
        }
        for start in 0..self.bones.len() {
            let mut seen = HashSet::new();
            let mut current = Some(start);
            while let Some(index) = current {
                if !seen.insert(index) {
                    return Err(invalid("BPHHB bone hierarchy contains a cycle"));
                }
                current = self.bones[index].parent_index;
            }
        }
        Ok(())
    }

    pub fn to_yaml(&self) -> io::Result<String> {
        self.validate()?;
        serde_yaml::to_string(&BphhbYaml {
            format: "BPHHB",
            metadata: &self.metadata,
            bones: &self.bones,
        })
        .map_err(io::Error::other)
    }

    pub fn leaves(&self) -> io::Result<Vec<HkclLeaf>> {
        self.validate()?;
        let mut leaves = vec![yaml_leaf("Metadata.bin", "Metadata", &self.metadata)?];
        for (index, bone) in self.bones.iter().enumerate() {
            let name = bone.name.replace(['/', '\\'], "_");
            leaves.push(yaml_leaf(
                &format!("Bones/{index:03} {name}.bin"),
                "Bone",
                bone,
            )?);
        }
        Ok(leaves)
    }
}

fn yaml_leaf<T: Serialize>(path: &str, viewer_type: &str, value: &T) -> io::Result<HkclLeaf> {
    Ok(HkclLeaf {
        path: path.to_owned(),
        yaml: serde_yaml::to_string(value).map_err(io::Error::other)?,
        viewer_type: viewer_type.to_owned(),
        read_only: true,
    })
}

fn structure_counts(list: &ParameterList) -> (u32, u32, u32) {
    // AAMP's header count includes the ParameterIO root list.
    let mut lists = 1;
    let mut objects = list.objects.len() as u32;
    let mut parameters = list
        .objects
        .iter()
        .map(|(_, object)| object.len() as u32)
        .sum();
    for (_, child) in list.lists.iter() {
        let child_counts = structure_counts(child);
        lists += child_counts.0;
        objects += child_counts.1;
        parameters += child_counts.2;
    }
    (lists, objects, parameters)
}

fn value_has_non_finite_float(value: &BphhbValue) -> bool {
    match value {
        BphhbValue::Float(value) => !value.is_finite(),
        BphhbValue::Vector2(values) => values.iter().any(|value| !value.is_finite()),
        BphhbValue::Vector3(values) => values.iter().any(|value| !value.is_finite()),
        BphhbValue::Vector4(values) | BphhbValue::Quaternion(values) => {
            values.iter().any(|value| !value.is_finite())
        }
        BphhbValue::FloatBuffer(values) => values.iter().any(|value| !value.is_finite()),
        _ => false,
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn collect_bones(list: &ParameterList, path: &mut Vec<u32>, bones: &mut Vec<BphhbBone>) {
    for (name, object) in list.objects.iter() {
        path.push(name.hash());
        if let Some(bone) = parse_bone(object, path.clone()) {
            bones.push(bone);
        }
        path.pop();
    }
    for (name, child) in list.lists.iter() {
        path.push(name.hash());
        collect_bones(child, path, bones);
        path.pop();
    }
}

fn parse_bone(object: &ParameterObject, object_path: Vec<u32>) -> Option<BphhbBone> {
    let name = string_parameter(
        object,
        &["BoneName", "HelperBoneName", "SelfBoneName", "Name"],
    )?;
    let parent_name = string_parameter(
        object,
        &[
            "ParentBoneName",
            "ParentName",
            "BaseBoneName",
            "RefBoneName",
        ],
    );
    let parent_index =
        parameter(object, &["ParentIndex", "ParentBoneIndex"]).and_then(|value| match value {
            Parameter::I32(value) => usize::try_from(*value).ok(),
            Parameter::U32(value) => usize::try_from(*value).ok(),
            _ => None,
        });
    let mut transform = BphhbTransform::default();
    if let Some(value) = parameter(object, &["Translate", "Translation", "Position"]) {
        if let Parameter::Vec3(value) = value {
            transform.translation = [value.x, value.y, value.z];
        }
    }
    if let Some(value) = parameter(object, &["Rotation", "Rotate", "Quaternion"]) {
        match value {
            Parameter::Quat(value) => transform.rotation = [value.a, value.b, value.c, value.d],
            Parameter::Vec4(value) => transform.rotation = [value.x, value.y, value.z, value.t],
            _ => {}
        }
    }
    if let Some(Parameter::Vec3(value)) = parameter(object, &["Scale", "Scaling"]) {
        transform.scale = [value.x, value.y, value.z];
    }
    let metadata = object
        .iter()
        .map(|(name, value)| (name.hash(), parameter_value(value)))
        .collect();
    Some(BphhbBone {
        name,
        parent_name,
        parent_index,
        transform,
        object_path,
        metadata,
    })
}

fn parameter<'a>(object: &'a ParameterObject, names: &[&str]) -> Option<&'a Parameter> {
    names.iter().find_map(|name| object.get(*name))
}

fn string_parameter(object: &ParameterObject, names: &[&str]) -> Option<String> {
    parameter(object, names).and_then(|value| match value {
        Parameter::String32(value) => Some(value.as_str().to_owned()),
        Parameter::String64(value) => Some(value.as_str().to_owned()),
        Parameter::String256(value) => Some(value.as_str().to_owned()),
        Parameter::StringRef(value) => Some(value.to_string()),
        _ => None,
    })
}

fn parameter_value(value: &Parameter) -> BphhbValue {
    match value {
        Parameter::Bool(value) => BphhbValue::Bool(*value),
        Parameter::F32(value) => BphhbValue::Float(*value),
        Parameter::I32(value) => BphhbValue::Signed(*value),
        Parameter::U32(value) => BphhbValue::Unsigned(*value),
        Parameter::Vec2(value) => BphhbValue::Vector2([value.x, value.y]),
        Parameter::Vec3(value) => BphhbValue::Vector3([value.x, value.y, value.z]),
        Parameter::Vec4(value) => BphhbValue::Vector4([value.x, value.y, value.z, value.t]),
        Parameter::Quat(value) => BphhbValue::Quaternion([value.a, value.b, value.c, value.d]),
        Parameter::String32(value) => BphhbValue::String(value.as_str().to_owned()),
        Parameter::String64(value) => BphhbValue::String(value.as_str().to_owned()),
        Parameter::String256(value) => BphhbValue::String(value.as_str().to_owned()),
        Parameter::StringRef(value) => BphhbValue::String(value.to_string()),
        Parameter::BufferInt(value) => BphhbValue::SignedBuffer(value.clone()),
        Parameter::BufferU32(value) => BphhbValue::UnsignedBuffer(value.clone()),
        Parameter::BufferF32(value) => BphhbValue::FloatBuffer(value.clone()),
        Parameter::BufferBinary(value) => BphhbValue::Binary(value.clone()),
        other => BphhbValue::Other(format!("{other:?}")),
    }
}

fn read_string_pool(data: &[u8], size: u32) -> io::Result<Vec<String>> {
    let size = size as usize;
    let start = data.len().checked_sub(size).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "BPHHB string pool exceeds file")
    })?;
    let mut strings = Vec::new();
    for value in data[start..].split(|byte| *byte == 0) {
        if value.is_empty() {
            continue;
        }
        if let Ok(value) = std::str::from_utf8(value) {
            if !value.chars().any(char::is_control) && !strings.iter().any(|item| item == value) {
                strings.push(value.to_owned());
            }
        }
    }
    Ok(strings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roead::{
        aamp::{ParameterList, ParameterObject},
        sarc::{Sarc, SarcWriter},
        types::{Quat, Vector3f},
        Endian,
    };
    use std::{fs, path::PathBuf};

    fn fixture(use_numeric_parent: bool, nested: bool) -> Vec<u8> {
        let root = ParameterObject::new()
            .with_parameter("BoneName", Parameter::StringRef("Root".into()))
            .with_parameter(
                "Translate",
                Parameter::Vec3(Vector3f {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                }),
            );
        let mut child = ParameterObject::new()
            .with_parameter("BoneName", Parameter::StringRef("Child".into()))
            .with_parameter(
                "Rotation",
                Parameter::Quat(Quat {
                    a: 0.0,
                    b: 0.0,
                    c: 0.5,
                    d: 0.5,
                }),
            )
            .with_parameter("Enabled", Parameter::Bool(true));
        child = if use_numeric_parent {
            child.with_parameter("ParentIndex", Parameter::U32(0))
        } else {
            child.with_parameter("ParentBoneName", Parameter::StringRef("Root".into()))
        };
        let bones = ParameterList::new()
            .with_object("Bone0", root)
            .with_object("Bone1", child);
        let archive = if nested {
            ParameterIO::new().with_list(
                "HelperBoneList",
                ParameterList::new().with_list("Group0", bones),
            )
        } else {
            ParameterIO::new().with_list("HelperBoneList", bones)
        };
        let mut bytes = archive.with_data_type("phhb").with_version(7).to_binary();
        // roead records the aligned stream length but does not retain trailing
        // zero padding in a Vec-backed cursor.
        let declared_size = u32::from_le_bytes(bytes[0x0c..0x10].try_into().unwrap()) as usize;
        bytes.resize(declared_size, 0);
        bytes
    }

    fn corpus() -> Vec<(String, Vec<u8>)> {
        let mut values = vec![
            ("named-parent".into(), fixture(false, false)),
            ("numeric-parent".into(), fixture(true, false)),
            ("nested-group".into(), fixture(false, true)),
        ];
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/bphhb");
        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("bphhb"))
                {
                    values.push((
                        path.file_name().unwrap().to_string_lossy().into_owned(),
                        fs::read(&path).unwrap(),
                    ));
                }
            }
        }
        values
    }

    #[test]
    fn parses_bone_hierarchy_transforms_and_metadata() {
        let bytes = fixture(false, false);
        let document = BphhbDocument::parse(&bytes).unwrap();
        assert_eq!(document.header.data_type, "phhb");
        assert_eq!(document.metadata.data_version, 7);
        assert_eq!(document.bones.len(), 2);
        assert_eq!(document.bones[0].name, "Root");
        assert_eq!(document.bones[0].transform.translation, [1.0, 2.0, 3.0]);
        assert_eq!(document.bones[1].parent_index, Some(0));
        assert_eq!(document.bones[1].transform.rotation, [0.0, 0.0, 0.5, 0.5]);
        assert!(document.bones[1]
            .metadata
            .values()
            .any(|value| value == &BphhbValue::Bool(true)));
        let yaml = document.to_yaml().unwrap();
        assert!(yaml.contains("format: BPHHB"));
        let leaves = document.leaves().unwrap();
        assert_eq!(leaves.len(), 3);
        assert_eq!(leaves[0].path, "Metadata.bin");
        assert!(leaves.iter().all(|leaf| leaf.read_only));
        assert!(leaves.iter().any(|leaf| leaf.path.contains("Root.bin")));
    }

    #[test]
    fn parses_and_validates_bphhb_corpus() {
        for (name, bytes) in corpus() {
            let document = BphhbDocument::parse(&bytes)
                .unwrap_or_else(|error| panic!("failed to parse {name}: {error}"));
            document
                .validate()
                .unwrap_or_else(|error| panic!("failed to validate {name}: {error}"));
        }
    }

    #[test]
    fn validation_rejects_bad_size_cycles_and_non_finite_transforms() {
        let bytes = fixture(false, false);
        let mut bad_size = BphhbDocument::parse(&bytes).unwrap();
        bad_size.header.file_size += 1;
        assert!(bad_size
            .validate()
            .unwrap_err()
            .to_string()
            .contains("file size"));

        let mut cycle = BphhbDocument::parse(&bytes).unwrap();
        cycle.bones[0].parent_index = Some(1);
        assert!(cycle.validate().unwrap_err().to_string().contains("cycle"));

        let mut non_finite = BphhbDocument::parse(&bytes).unwrap();
        non_finite.bones[0].transform.translation[0] = f32::NAN;
        assert!(non_finite
            .validate()
            .unwrap_err()
            .to_string()
            .contains("non-finite"));
    }

    #[test]
    fn bphhb_corpus_preserves_bytes_and_survives_root_and_nested_sarc_reopen() {
        for (name, bytes) in corpus() {
            let document = BphhbDocument::parse(&bytes).unwrap();
            document.validate().unwrap();
            assert_eq!(document.raw, bytes, "raw roundtrip changed {name}");

            let mut inner = SarcWriter::new(Endian::Little);
            inner.add_file(&format!("Physics/{name}.bphhb"), bytes);
            let mut outer = SarcWriter::new(Endian::Little);
            outer.add_file("Nested/physics.pack", inner.to_binary());
            let outer = Sarc::new(outer.to_binary()).unwrap();
            let inner = Sarc::new(outer.get_data("Nested/physics.pack").unwrap().to_vec()).unwrap();
            BphhbDocument::parse(inner.get_data(&format!("Physics/{name}.bphhb")).unwrap())
                .unwrap()
                .validate()
                .unwrap();
        }
    }
}
