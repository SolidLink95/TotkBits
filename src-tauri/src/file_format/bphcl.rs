use crate::parser::physics::bphcl::BphclDocument;
use roead::aamp::{Parameter, ParameterIO, ParameterList};
use serde::{Deserialize, Serialize};
use serde_yaml::{
    value::{Tag, TaggedValue},
    Mapping, Value,
};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io, path::Path, sync::LazyLock};

/// AAMP key names shipped in `misc/botw_hashed_names.txt`, keyed by CRC32.
/// Blank lines and `#` comments are skipped.
static AAMP_TOTK_NAMES: LazyLock<HashMap<u32, String>> = LazyLock::new(|| {
    let mut names = HashMap::new();
    for line in crate::utils::LookupData::read_support_text("botw_hashed_names.txt", "").lines() {
        let name = line.trim();
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        names
            .entry(roead::aamp::hash_name(name))
            .or_insert_with(|| name.to_owned());
    }
    names
});

#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum BphclVanillaNode {
    Legacy(String),
    Hashed { name: String, sha256: String },
}

impl BphclVanillaNode {
    fn name(&self) -> &str {
        match self {
            Self::Legacy(name) | Self::Hashed { name, .. } => name,
        }
    }

    fn matches_value(&self, value: &impl Serialize) -> io::Result<bool> {
        match self {
            Self::Legacy(_) => Ok(true),
            Self::Hashed { sha256, .. } => Ok(*sha256 == canonical_node_hash(value)?),
        }
    }
}

#[derive(Deserialize, Serialize)]
struct BphclVanillaNodes {
    cloth: Vec<BphclVanillaNode>,
    collidables: Vec<BphclVanillaNode>,
    #[serde(default)]
    skeletons: Vec<BphclVanillaNode>,
}

impl BphclVanillaNodes {
    fn cloth(&self, name: &str) -> Option<&BphclVanillaNode> {
        self.cloth.iter().find(|node| node.name() == name)
    }

    fn collidable(&self, name: &str) -> Option<&BphclVanillaNode> {
        self.collidables.iter().find(|node| node.name() == name)
    }

    fn skeleton(&self, name: &str) -> Option<&BphclVanillaNode> {
        self.skeletons.iter().find(|node| node.name() == name)
    }
}

fn canonical_node_hash(value: &impl Serialize) -> io::Result<String> {
    fn remove_relocation_fields(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("index");
                map.remove("item_index");
                for child in map.values_mut() {
                    remove_relocation_fields(child);
                }
            }
            serde_json::Value::Array(values) => {
                for child in values {
                    remove_relocation_fields(child);
                }
            }
            _ => {}
        }
    }

    let mut value = serde_json::to_value(value).map_err(io::Error::other)?;
    remove_relocation_fields(&mut value);
    let bytes = serde_json::to_vec(&value).map_err(io::Error::other)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

static BPHCL_VANILLA_NODES: LazyLock<HashMap<String, BphclVanillaNodes>> = LazyLock::new(|| {
    serde_json::from_str(&crate::utils::LookupData::read_support_json(
        "bphcl_nodes.json",
    ))
    .unwrap_or_default()
});

#[derive(Clone, Debug, Serialize)]
pub struct BphclLeaf {
    pub path: String,
    pub yaml: String,
    pub viewer_type: String,
    pub read_only: bool,
}
pub struct BphclFile {
    pub source_path: Option<String>,
    pub document: BphclDocument,
}
impl BphclFile {
    pub fn open_internal(
        data: &[u8],
        path: &str,
        outer_path: Option<&str>,
    ) -> Option<(
        crate::file_format::BinTextFile::OpenedFile<'static>,
        crate::InternalFile::InternalFile<'static>,
        crate::Open_and_Save::SendData,
    )> {
        let file = Self::from_binary(data, Some(Path::new(path))).ok()?;
        let status = match outer_path {
            Some(outer) => format!("Opened {path} inside {outer}"),
            None => format!("Opened {path} from archive"),
        };
        let send_data = file.send_data(Path::new(path), status).ok()?;
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.file_type = crate::Zstd::TotkFileType::Bphcl;
        opened.path = crate::Settings::Pathlib::new(path);
        opened.bphcl = Some(file);
        let mut internal = crate::InternalFile::InternalFile::new(path.into());
        internal.file_type = crate::Zstd::TotkFileType::Bphcl;
        Some((opened, internal, send_data))
    }

    pub fn send_data(
        &self,
        path: &Path,
        status_text: String,
    ) -> io::Result<crate::Open_and_Save::SendData> {
        let root_name = path
            .file_name()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "BPHCL path has no file name")
            })?
            .to_string_lossy();
        let mut data = crate::Open_and_Save::SendData::default();
        data.path = crate::Settings::Pathlib::new(path);
        data.tab = "SARC".into();
        data.read_only = false;
        data.sarc_paths.paths = self
            .leaves()?
            .into_iter()
            .map(|leaf| format!("{root_name}/{}", leaf.path))
            .collect();
        if let Some(vanilla) = BPHCL_VANILLA_NODES.get(root_name.as_ref()) {
            let mut nodes_changed = self.document.cloth.len() != vanilla.cloth.len()
                || self.document.collidables.len() != vanilla.collidables.len()
                || self.document.skeletons.len() != vanilla.skeletons.len();
            for node in &self.document.cloth {
                let path = format!("{root_name}/Cloth/{:03} {}.bin", node.index, node.name);
                match vanilla.cloth(&node.name) {
                    None => {
                        nodes_changed = true;
                        data.sarc_paths.added_paths.push(path);
                    }
                    Some(reference) if !reference.matches_value(node)? => {
                        nodes_changed = true;
                        data.sarc_paths.modded_paths.push(path);
                    }
                    Some(_) => {}
                }
            }
            for node in &self.document.collidables {
                let path = format!(
                    "{root_name}/Collidables/{:03} {}.bin",
                    node.index, node.name
                );
                match vanilla.collidable(&node.name) {
                    None => {
                        nodes_changed = true;
                        data.sarc_paths.added_paths.push(path);
                    }
                    Some(reference) if !reference.matches_value(node)? => {
                        nodes_changed = true;
                        data.sarc_paths.modded_paths.push(path);
                    }
                    Some(_) => {}
                }
            }
            for node in &self.document.skeletons {
                let path = format!("{root_name}/Skeletons/{:03} {}.bin", node.index, node.name);
                match vanilla.skeleton(&node.name) {
                    None => {
                        nodes_changed = true;
                        data.sarc_paths.added_paths.push(path);
                    }
                    Some(reference) if !reference.matches_value(node)? => {
                        nodes_changed = true;
                        data.sarc_paths.modded_paths.push(path);
                    }
                    Some(_) => {}
                }
            }
            if nodes_changed
                && data
                    .sarc_paths
                    .paths
                    .iter()
                    .any(|entry| entry == &format!("{root_name}/Section.aamp"))
            {
                data.sarc_paths
                    .modded_paths
                    .push(format!("{root_name}/Section.aamp"));
            }
        }
        data.sarc_paths.read_only = true;
        data.get_file_label(crate::Zstd::TotkFileType::Bphcl, None);
        data.status_text = status_text;
        Ok(data)
    }

    pub fn open(
        path: &Path,
    ) -> Option<(
        crate::file_format::BinTextFile::OpenedFile<'static>,
        crate::Open_and_Save::SendData,
    )> {
        let bytes = std::fs::read(path).ok()?;
        let file = Self::from_binary(&bytes, Some(path)).ok()?;
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.file_type = crate::Zstd::TotkFileType::Bphcl;
        opened.path = crate::Settings::Pathlib::new(path);
        opened.bphcl = Some(file);
        let data = opened
            .bphcl
            .as_ref()?
            .send_data(path, "Opened BPHCL structure".into())
            .ok()?;
        Some((opened, data))
    }

    pub fn leaf(&self, path: &str) -> io::Result<BphclLeaf> {
        let path = path.split_once('/').map(|(_, leaf)| leaf).unwrap_or(path);
        self.leaves()?
            .into_iter()
            .find(|leaf| leaf.path == path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "BPHCL leaf not found"))
    }
    pub fn from_binary(data: &[u8], path: Option<&Path>) -> io::Result<Self> {
        Ok(Self {
            source_path: path.map(|p| p.to_string_lossy().into_owned()),
            document: BphclDocument::parse(data)?,
        })
    }
    pub fn raw_binary(&self) -> Vec<u8> {
        self.document.to_bytes()
    }
    pub fn replace_aamp_yaml(&mut self, yaml: &str) -> io::Result<()> {
        let aamp = aamp_from_yaml(yaml)?.to_binary();
        let mut builder = crate::parser::physics::bphcl::BphclBuilder::new(&self.document)?;
        builder.replace_aamp(aamp);
        let bytes = builder.build()?;
        let rebuilt = BphclDocument::parse(&bytes)?;
        rebuilt.validate()?;
        self.document = rebuilt;
        Ok(())
    }
    pub fn leaves(&self) -> io::Result<Vec<BphclLeaf>> {
        let mut out = vec![];
        for c in &self.document.cloth {
            out.push(BphclLeaf {
                path: format!("Cloth/{:03} {}.bin", c.index, c.name),
                yaml: serde_yaml::to_string(c).map_err(io::Error::other)?,
                viewer_type: "Cloth".into(),
                read_only: true,
            })
        }
        for c in &self.document.collidables {
            out.push(BphclLeaf {
                path: format!("Collidables/{:03} {}.bin", c.index, c.name),
                yaml: serde_yaml::to_string(c).map_err(io::Error::other)?,
                viewer_type: "Collidable".into(),
                read_only: true,
            })
        }
        for skeleton in &self.document.skeletons {
            out.push(BphclLeaf {
                path: format!("Skeletons/{:03} {}.bin", skeleton.index, skeleton.name),
                yaml: serde_yaml::to_string(skeleton).map_err(io::Error::other)?,
                viewer_type: "Skeleton".into(),
                read_only: true,
            })
        }
        if let Some(a) = &self.document.aamp {
            let pio = ParameterIO::from_binary(&a.raw).map_err(io::Error::other)?;
            out.push(BphclLeaf {
                path: "Section.aamp".into(),
                yaml: safe_aamp_yaml(&pio)?,
                viewer_type: "AAMP".into(),
                read_only: false,
            })
        }
        Ok(out)
    }
}

pub(crate) fn generate_node_catalog(input: &Path, output: &Path) -> io::Result<()> {
    use std::collections::BTreeMap;

    fn hashed(name: &str, value: impl Serialize) -> io::Result<BphclVanillaNode> {
        Ok(BphclVanillaNode::Hashed {
            name: name.to_owned(),
            sha256: canonical_node_hash(&value)?,
        })
    }

    let mut catalog = BTreeMap::new();
    let mut files: Vec<_> = std::fs::read_dir(input)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bphcl"))
        })
        .collect();
    files.sort();
    for path in files {
        let bytes = std::fs::read(&path)?;
        let document = BphclDocument::parse(&bytes)?;
        let nodes = BphclVanillaNodes {
            cloth: document
                .cloth
                .iter()
                .map(|node| hashed(&node.name, node))
                .collect::<io::Result<_>>()?,
            collidables: document
                .collidables
                .iter()
                .map(|node| hashed(&node.name, node))
                .collect::<io::Result<_>>()?,
            skeletons: document
                .skeletons
                .iter()
                .map(|node| hashed(&node.name, node))
                .collect::<io::Result<_>>()?,
        };
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid BPHCL filename"))?;
        catalog.insert(name.to_owned(), nodes);
    }
    let mut json = serde_json::to_string_pretty(&catalog).map_err(io::Error::other)?;
    json.push('\n');
    std::fs::write(output, json)
}

/// Resolves an AAMP key to its plaintext name.
///
/// `misc/botw_hashed_names.txt` is consulted first. Keys not listed there are
/// matched against numbered families using `index` (the key's position inside
/// its parent) and the parent's own name, and unknown keys fall back to their
/// numeric CRC32 so the YAML still round-trips.
fn yaml_name(hash: u32, index: usize, parent: Option<&str>) -> Value {
    if let Some(name) = AAMP_TOTK_NAMES.get(&hash) {
        return Value::String(name.clone());
    }
    match numbered_name(hash, index, parent) {
        Some(name) => Value::String(name),
        None => Value::Number(hash.into()),
    }
}

/// A numbered key family such as `ItemName%02d`: the text around one integer
/// placeholder and the zero-padded width it is printed with.
struct NumberedPattern {
    prefix: String,
    suffix: String,
    width: usize,
}

impl NumberedPattern {
    /// Parses one `misc/botw_numbered_names.txt` line. Only lines made of a
    /// single `%d`, `%2d` or `%02d` style placeholder inside an identifier are
    /// accepted; anything else in that file is noise from a strings dump.
    fn parse(line: &str) -> Option<Self> {
        let (prefix, rest) = line.split_once('%')?;
        let spec_end = rest.find('d')?;
        let (spec, suffix) = rest.split_at(spec_end);
        let suffix = &suffix[1..];
        let is_identifier =
            |text: &str| text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
        if prefix.is_empty()
            || !is_identifier(prefix)
            || !is_identifier(suffix)
            || !spec.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let width = spec.trim_start_matches('0').parse().unwrap_or(0);
        Some(Self {
            prefix: prefix.to_owned(),
            suffix: suffix.to_owned(),
            width,
        })
    }

    fn render(&self, number: usize) -> String {
        format!(
            "{}{:0width$}{}",
            self.prefix,
            number,
            self.suffix,
            width = self.width
        )
    }
}

/// Numbered families from `misc/botw_numbered_names.txt` (`AI_%d`,
/// `ItemName%02d`, `cloth_mesh_%d`, ...).
static AAMP_NUMBERED_PATTERNS: LazyLock<Vec<NumberedPattern>> = LazyLock::new(|| {
    crate::utils::LookupData::read_support_text("botw_numbered_names.txt", "")
        .lines()
        .filter_map(|line| NumberedPattern::parse(line.trim()))
        .collect()
});

/// Numbered stems whose parent name gives no hint and which the pattern file
/// does not list: `Children` holds `Child#`, `Elements` holds `Element#`,
/// actor `Tags` hold `Tag#`, AI index objects hold `Idx_N`, drop tables are
/// `NormalN` and physics sets nest `RigidBody_N`.
const NUMBERED_NAME_STEMS: &[&str] = &[
    "Child",
    "Element",
    "Tag",
    "Idx",
    "Normal",
    "RigidBody",
    "RigidBodySet",
];

/// Guesses a numbered key from its position. Candidates come from the pattern
/// file, from the parent name (`ASDefines` gives `ASDefine`, `BodyParamList`
/// gives `BodyParam`, `StringArray0` gives `String`) and from
/// [`NUMBERED_NAME_STEMS`]; each candidate is checked against the real hash,
/// so a wrong guess is never used.
fn numbered_name(hash: u32, index: usize, parent: Option<&str>) -> Option<String> {
    // Keys are normally numbered by position, but drop tables start at 01 and a
    // few files skip numbers, so a window around the position is tried.
    let low = if index < 64 { 0 } else { index - 8 };
    let high = index.saturating_add(8);
    let numbers = || low..=high;

    let mut stems: Vec<String> = Vec::new();
    if let Some(parent) = parent {
        stems.push(parent.to_owned());
        for suffix in ["List", "Idx", "Info", "es", "s"] {
            if let Some(stem) = parent.strip_suffix(suffix) {
                if !stem.is_empty() {
                    stems.push(stem.to_owned());
                }
            }
        }
        let trimmed = parent.trim_end_matches(|c: char| c.is_ascii_digit());
        if trimmed.len() < parent.len() && !trimmed.is_empty() {
            stems.push(trimmed.to_owned());
            if let Some(stem) = trimmed.strip_suffix("Array") {
                if !stem.is_empty() {
                    stems.push(stem.to_owned());
                }
            }
        }
    }
    stems.extend(NUMBERED_NAME_STEMS.iter().map(|stem| (*stem).to_owned()));
    for stem in &stems {
        for number in numbers() {
            for candidate in [
                format!("{stem}_{number}"),
                format!("{stem}{number}"),
                format!("{stem}_{number:02}"),
                format!("{stem}{number:02}"),
                format!("{stem}_{number:03}"),
                format!("{stem}{number:03}"),
            ] {
                if roead::aamp::hash_name(&candidate) == hash {
                    return Some(candidate);
                }
            }
        }
    }
    for pattern in AAMP_NUMBERED_PATTERNS.iter() {
        for number in numbers() {
            let candidate = pattern.render(number);
            if roead::aamp::hash_name(&candidate) == hash {
                return Some(candidate);
            }
        }
    }
    None
}

fn tagged(tag: &str, value: Value) -> Value {
    Value::Tagged(Box::new(TaggedValue {
        tag: Tag::new(tag),
        value,
    }))
}

fn float(value: f32) -> Value {
    Value::from(value as f64)
}

fn sequence(values: impl IntoIterator<Item = Value>) -> Value {
    Value::Sequence(values.into_iter().collect())
}

fn curves<const N: usize>(values: &[roead::types::Curve; N]) -> Value {
    tagged(
        "!curve",
        sequence(values.iter().flat_map(|curve| {
            std::iter::once(Value::from(curve.a))
                .chain(std::iter::once(Value::from(curve.b)))
                .chain(curve.floats.iter().copied().map(Value::from))
        })),
    )
}

fn parameter_yaml(parameter: &Parameter) -> Value {
    match parameter {
        Parameter::Bool(value) => Value::Bool(*value),
        Parameter::F32(value) => float(*value),
        Parameter::I32(value) => Value::Number((*value).into()),
        Parameter::Vec2(value) => tagged("!vec2", sequence([float(value.x), float(value.y)])),
        Parameter::Vec3(value) => tagged(
            "!vec3",
            sequence([float(value.x), float(value.y), float(value.z)]),
        ),
        Parameter::Vec4(value) => tagged(
            "!vec4",
            sequence([
                float(value.x),
                float(value.y),
                float(value.z),
                float(value.t),
            ]),
        ),
        Parameter::Color(value) => tagged(
            "!color",
            sequence([
                float(value.r),
                float(value.g),
                float(value.b),
                float(value.a),
            ]),
        ),
        Parameter::String32(value) => tagged("!str32", Value::String(value.to_string())),
        Parameter::String64(value) => tagged("!str64", Value::String(value.to_string())),
        Parameter::Curve1(value) => curves(value),
        Parameter::Curve2(value) => curves(value),
        Parameter::Curve3(value) => curves(value),
        Parameter::Curve4(value) => curves(value),
        Parameter::BufferInt(values) => tagged(
            "!buffer_int",
            sequence(values.iter().copied().map(Value::from)),
        ),
        Parameter::BufferF32(values) => {
            tagged("!buffer_f32", sequence(values.iter().copied().map(float)))
        }
        Parameter::String256(value) => tagged("!str256", Value::String(value.to_string())),
        Parameter::Quat(value) => tagged(
            "!quat",
            sequence([
                float(value.a),
                float(value.b),
                float(value.c),
                float(value.d),
            ]),
        ),
        Parameter::U32(value) => tagged("!u", Value::Number((*value).into())),
        Parameter::BufferU32(values) => tagged(
            "!buffer_u32",
            sequence(values.iter().copied().map(Value::from)),
        ),
        Parameter::BufferBinary(values) => tagged(
            "!buffer_binary",
            sequence(values.iter().copied().map(Value::from)),
        ),
        Parameter::StringRef(value) => Value::String(value.to_string()),
    }
}

pub(crate) fn safe_aamp_yaml(pio: &ParameterIO) -> io::Result<String> {
    fn plain(value: &Value) -> Option<&str> {
        match value {
            Value::String(name) => Some(name),
            _ => None,
        }
    }
    fn list(value: &ParameterList, own_name: Option<&str>) -> Value {
        let mut root = Mapping::new();
        let mut lists = Mapping::new();
        for (index, (name, child)) in value.lists.iter().enumerate() {
            let key = yaml_name(name.hash(), index, own_name);
            let child = list(child, plain(&key));
            lists.insert(key, child);
        }
        let mut objects = Mapping::new();
        for (index, (name, object)) in value.objects.iter().enumerate() {
            let key = yaml_name(name.hash(), index, own_name);
            let mut params = Mapping::new();
            for (parameter_index, (parameter_name, parameter)) in object.iter().enumerate() {
                params.insert(
                    yaml_name(parameter_name.hash(), parameter_index, plain(&key)),
                    parameter_yaml(parameter),
                );
            }
            objects.insert(key, tagged("!obj", params.into()));
        }
        root.insert("lists".into(), lists.into());
        root.insert("objects".into(), objects.into());
        tagged("!list", root.into())
    }
    let mut root = Mapping::new();
    root.insert("version".into(), pio.version.into());
    root.insert("type".into(), pio.data_type.to_string().into());
    root.insert(
        "param_root".into(),
        list(&pio.param_root, Some("param_root")),
    );
    serde_yaml::to_string(&tagged("!io", root.into())).map_err(io::Error::other)
}

pub(crate) fn aamp_from_yaml(yaml: &str) -> io::Result<ParameterIO> {
    ParameterIO::from_text(yaml).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::yaml_name;
    use serde_yaml::Value;

    #[test]
    fn bundled_aamp_names_are_resolved() {
        for name in [
            "cloth_mesh_list",
            "cloth_mesh_0",
            "Name",
            "BaseBone",
            "BoneCorrection",
            "BoneCorrectionAxisOrder",
            "Twist",
            "TwistSwingAxis",
            "TwistAngleCoef",
            "TwistMaxAngle",
        ] {
            assert_eq!(
                yaml_name(roead::aamp::hash_name(name), 0, None),
                Value::String(name.to_owned()),
                "missing bundled AAMP name {name}"
            );
        }
    }

    #[test]
    fn numbered_aamp_names_are_recovered_from_index_and_parent() {
        for (name, index, parent) in [
            ("cloth_mesh_7", 7, "cloth_mesh_list"),
            ("collidable_12", 12, "collidable_list"),
            ("RigidBody_3", 3, "RigidBodySet_0"),
            ("ASDefine_5", 5, "ASDefines"),
            ("BodyParam_2", 2, "BodyParamList"),
            ("Element4", 4, "Elements"),
            ("Value3", 3, "StringArray0"),
            ("ItemName01", 1, "Normal"),
            ("ItemProbability01", 2, "Normal"),
            ("Table02", 3, "Header"),
            ("Tag0", 0, "Tags"),
            ("ModelData_0", 0, "ModelData"),
            ("Unit_0", 0, "Unit"),
        ] {
            assert_eq!(
                yaml_name(roead::aamp::hash_name(name), index, Some(parent)),
                Value::String(name.to_owned()),
                "numbered AAMP name {name} under {parent}"
            );
        }
    }

    #[test]
    fn numbered_pattern_file_is_parsed_and_used() {
        let pattern = super::NumberedPattern::parse("ItemName%02d").unwrap();
        assert_eq!(pattern.render(3), "ItemName03");
        let pattern = super::NumberedPattern::parse("Bone%d_Name").unwrap();
        assert_eq!(pattern.render(12), "Bone12_Name");
        assert!(super::NumberedPattern::parse("!%At/Vh").is_none());
        assert!(super::NumberedPattern::parse("%s_%d").is_none());
        assert!(super::NumberedPattern::parse("Table (addr:0x%x, size:%d)").is_none());
        assert!(
            super::AAMP_NUMBERED_PATTERNS.len() > 300,
            "misc/botw_numbered_names.txt is missing or unreadable"
        );
        for (name, index) in [("ASName3", 3), ("CollisionInfo_11", 11), ("Check_2", 2)] {
            assert_eq!(
                yaml_name(roead::aamp::hash_name(name), index, None),
                Value::String(name.to_owned()),
                "pattern-file name {name}"
            );
        }
    }

    #[test]
    fn unknown_aamp_name_falls_back_to_numeric_hash() {
        let unknown = (0..=u32::MAX)
            .find(|hash| {
                !super::AAMP_TOTK_NAMES.contains_key(hash)
                    && super::numbered_name(*hash, 0, None).is_none()
            })
            .expect("the AAMP name table cannot contain every u32 hash");
        assert_eq!(yaml_name(unknown, 0, None), Value::Number(unknown.into()));
    }

    /// Counts mapping keys that are still numeric hashes in the rendered YAML.
    fn numeric_keys(yaml: &str) -> Vec<String> {
        yaml.lines()
            .filter_map(|line| {
                let key = line.trim_start().split(':').next()?.trim();
                (!key.is_empty() && key.bytes().all(|b| b.is_ascii_digit())).then(|| key.to_owned())
            })
            .collect()
    }

    #[test]
    fn actor_pack_aamp_files_use_plaintext_names() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/_CLAUDE/Enemy_Bokoblin_Dark.sbactorpack");
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let raw = roead::yaz0::decompress(&bytes).expect("Yaz0 actor pack");
        let sarc = roead::sarc::Sarc::new(raw).expect("SARC actor pack");
        let mut checked = 0;
        let mut total_keys = 0;
        let mut unresolved = Vec::new();
        let mut yamls = String::new();
        for file in sarc.files() {
            if !crate::Settings::Magic::is_aamp(file.data) {
                continue;
            }
            let name = file.name().unwrap_or("?");
            let pio = roead::aamp::ParameterIO::from_binary(file.data)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let yaml = super::safe_aamp_yaml(&pio).unwrap();
            total_keys += yaml.lines().filter(|line| line.contains(':')).count();
            unresolved.extend(
                numeric_keys(&yaml)
                    .into_iter()
                    .map(|hash| format!("{name}: {hash}")),
            );
            yamls.push_str(&yaml);
            checked += 1;
        }
        assert!(checked > 0, "actor pack has no AAMP files");
        // Structural names that used to render as hashes.
        for name in [
            "ControllerInfo:",
            "ModelData_0:",
            "Unit_0:",
            "RigidBodySet_0:",
            "RigidBody_0:",
            "RigidBody_1:",
            "RigidBodyParam:",
            "ContactInfo:",
            "AIProgram",
            "DemoAIActionIdx:",
        ] {
            assert!(yamls.contains(name), "actor pack YAML is missing {name}");
        }
        // A handful of ModelList fields, ragdoll blend-weight objects and
        // actor-specific AI node names have no publicly known plaintext, so a
        // residue of at most 2% hashed keys is tolerated.
        assert!(
            unresolved.len() * 50 < total_keys,
            "{} of {total_keys} AAMP keys are still hashed: {unresolved:?}",
            unresolved.len()
        );
    }

    #[test]
    fn donkey_bphcl_aamp_uses_plaintext_names() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/_bphcl/Animal_Donkey.bphcl");
        let bytes = std::fs::read(path).expect("Animal_Donkey.bphcl fixture is missing");
        let document = crate::parser::physics::bphcl::BphclDocument::parse(&bytes).unwrap();
        let aamp = document.aamp.expect("Donkey BPHCL has no AAMP section");
        let pio = roead::aamp::ParameterIO::from_binary(&aamp.raw).unwrap();
        let yaml = super::safe_aamp_yaml(&pio).unwrap();
        for name in [
            "cloth_mesh_list:",
            "cloth_mesh_0:",
            "Name:",
            "BaseBone:",
            "BoneCorrection:",
            "BoneCorrectionAxisOrder:",
            "Twist:",
            "TwistSwingAxis:",
            "TwistAngleCoef:",
            "TwistMaxAngle:",
        ] {
            assert!(yaml.contains(name), "AAMP YAML is missing {name}");
        }
        for hash in [
            "1571872146:",
            "3840643960:",
            "4262580536:",
            "1259279791:",
            "3057977986:",
            "851716768:",
            "3911543180:",
            "3958628279:",
            "521060591:",
            "1920464176:",
        ] {
            assert!(!yaml.contains(hash), "AAMP YAML still contains hash {hash}");
        }
    }
}
