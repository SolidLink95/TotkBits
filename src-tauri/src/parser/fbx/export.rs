use crate::file_format::Model3D::bfres::{BfresBone, BfresMesh};
use crate::parser::AOC::g1m::{G1mFile, G1mMaterial, ResolvedG1tTexture};
use base64::Engine;
use image_dds::{ImageFormat, Mipmaps, Quality};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::binary::{
    p_bool, p_color, p_color_rgb, p_compound, p_datetime, p_double, p_enum, p_int, p_ktime, p_lcl,
    p_number, p_object, p_string, p_url, p_vector, p_visibility, p_visibility_inheritance,
    p_xref_url, properties70, write_document, Attr, Node, CREATION_TIME, FILE_ID,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TextureExportFormat {
    None,
    Png,
    Dds,
}

impl TextureExportFormat {
    pub fn parse(value: &str) -> io::Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "png" => Ok(Self::Png),
            "dds" => Ok(Self::Dds),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "texture format must be none, png, or dds",
            )),
        }
    }
}

struct ModelInput<'a> {
    model: &'a G1mFile,
    textures: &'a [ResolvedG1tTexture],
    prefix: String,
}

struct Ids {
    next: i64,
}

impl Ids {
    fn new() -> Self {
        Self { next: 1_000_000 }
    }

    fn take(&mut self) -> i64 {
        let value = self.next;
        self.next += 1;
        value
    }
}

struct TextureLink {
    id: i64,
    video_id: i64,
    material_id: i64,
    property: &'static str,
    name: String,
    relative_path: String,
    uv_set: String,
    has_transparency: bool,
}

struct ExportedTexture {
    name: String,
    relative_path: String,
    has_transparency: bool,
}

struct MeshLink {
    geometry_id: i64,
    model_id: i64,
    material_id: i64,
    skin_id: Option<i64>,
    clusters: Vec<(i64, usize)>,
}

const CREATOR: &str = concat!("TotkBits ", env!("CARGO_PKG_VERSION"));

pub fn export_g1m(
    models: &[(&G1mFile, &[ResolvedG1tTexture], String)],
    output: &Path,
    texture_format: TextureExportFormat,
    armature_name: &str,
) -> io::Result<()> {
    if models.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no G1M models to export",
        ));
    }
    let inputs: Vec<_> = models
        .iter()
        .map(|(model, textures, prefix)| ModelInput {
            model,
            textures,
            prefix: prefix.clone(),
        })
        .collect();
    let texture_paths = export_textures(&inputs, output, texture_format)?;
    let document = build_document(&inputs, &texture_paths, armature_name, output);
    fs::write(output, write_document(&document)?)
}

fn export_textures(
    models: &[ModelInput<'_>],
    output: &Path,
    format: TextureExportFormat,
) -> io::Result<BTreeMap<String, ExportedTexture>> {
    let mut paths = BTreeMap::new();
    if format == TextureExportFormat::None {
        return Ok(paths);
    }
    let folder = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(folder)?;
    let extension = if format == TextureExportFormat::Dds {
        "dds"
    } else {
        "png"
    };
    let mut used = BTreeSet::new();
    for input in models {
        for texture in input.textures {
            let key = texture_key(&input.prefix, &texture.name);
            if paths.contains_key(&key) {
                continue;
            }
            let base = texture_export_base(input, texture);
            let mut filename = format!("{base}.{extension}");
            let mut suffix = 2;
            while !used.insert(filename.to_ascii_lowercase()) {
                filename = format!("{base}_{suffix}.{extension}");
                suffix += 1;
            }
            let png = decode_data_url(&texture.data_url)?;
            let has_transparency = has_fully_transparent_pixel(&png)?;
            let bytes = if format == TextureExportFormat::Png {
                png
            } else {
                png_to_dds(&png)?
            };
            fs::write(folder.join(&filename), bytes)?;
            paths.insert(
                key,
                ExportedTexture {
                    name: filename
                        .strip_suffix(&format!(".{extension}"))
                        .unwrap_or(&filename)
                        .to_owned(),
                    relative_path: filename,
                    has_transparency,
                },
            );
        }
    }
    Ok(paths)
}

fn texture_export_base(input: &ModelInput<'_>, texture: &ResolvedG1tTexture) -> String {
    let texture_names: BTreeSet<_> = std::iter::once(texture.name.as_str())
        .chain(texture.aliases.iter().map(String::as_str))
        .collect();
    let kind = input
        .model
        .materials
        .iter()
        .flat_map(|material| &material.texture_slots)
        .find(|slot| texture_names.contains(slot.name.as_str()))
        .map(|slot| texture_kind_prefix(&slot.texture_type))
        .unwrap_or("tex");
    let (archive_path, archive_index) = texture
        .path
        .rsplit_once('#')
        .unwrap_or((texture.path.as_str(), texture.name.as_str()));
    let archive_hash = Path::new(archive_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("g1t");
    safe_name(&format!("{kind}_{archive_hash}_{archive_index}"))
}

fn texture_kind_prefix(kind: &str) -> &'static str {
    match kind {
        "Diffuse" => "alb",
        "Normal" => "nrm",
        "Emission" => "emm",
        "AmbientOcclusion" => "aoo",
        "Specular" => "spm",
        _ => "tex",
    }
}

fn has_fully_transparent_pixel(data: &[u8]) -> io::Result<bool> {
    let image = image::load_from_memory(data)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    Ok(image.to_rgba8().pixels().any(|pixel| pixel[3] == 0))
}

fn decode_data_url(value: &str) -> io::Result<Vec<u8>> {
    let encoded = value
        .split_once(',')
        .map(|(_, encoded)| encoded)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid texture data URL"))?;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
}

fn png_to_dds(png: &[u8]) -> io::Result<Vec<u8>> {
    let image = image::load_from_memory(png)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?
        .to_rgba8();
    let dds = image_dds::dds_from_image(
        &image,
        ImageFormat::Rgba8Unorm,
        Quality::Normal,
        Mipmaps::Disabled,
    )
    .map_err(|error| io::Error::other(error.to_string()))?;
    let mut bytes = Vec::new();
    dds.write(&mut bytes)
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(bytes)
}

// ---- Document -----------------------------------------------------------

fn build_document(
    models: &[ModelInput<'_>],
    texture_paths: &BTreeMap<String, ExportedTexture>,
    armature_name: &str,
    output: &Path,
) -> Vec<Node> {
    let mut ids = Ids::new();
    let root_id = ids.take();
    let root_attribute_id = ids.take();
    let document_id = ids.take();
    let mut bone_ids = Vec::with_capacity(models.len());
    let mut bone_attribute_ids = Vec::with_capacity(models.len());
    let mut material_ids = Vec::with_capacity(models.len());
    let mut mesh_links = Vec::new();
    let mut texture_links = Vec::new();
    for input in models {
        bone_ids.push(
            (0..input.model.render.bones.len())
                .map(|_| ids.take())
                .collect::<Vec<_>>(),
        );
        bone_attribute_ids.push(
            (0..input.model.render.bones.len())
                .map(|_| ids.take())
                .collect::<Vec<_>>(),
        );
        material_ids.push(
            (0..input.model.materials.len())
                .map(|_| [ids.take(), ids.take()])
                .collect::<Vec<_>>(),
        );
    }
    let texture_folder = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Bones, meshes and cluster bind matrices all share the G1M's native
    // Y-up space, so the node hierarchy must not introduce any extra
    // rotation: SDK based readers (Noesis, Autodesk) place bones from the
    // hierarchy, not from `TransformLink`.
    let mut objects = Node::new("Objects");
    objects.push(null_attribute(root_attribute_id, armature_name));
    objects.push(model_object(
        root_id,
        armature_name,
        "Null",
        [0.0; 3],
        [0.0; 3],
        [1.0; 3],
    ));
    for (model_index, input) in models.iter().enumerate() {
        for (index, bone) in input.model.render.bones.iter().enumerate() {
            let name = format!("{}{}", input.prefix, bone.name);
            let rotation = quaternion_euler_degrees(bone.rotation);
            objects.push(model_object(
                bone_ids[model_index][index],
                &name,
                "LimbNode",
                bone.translation,
                rotation,
                bone.scale,
            ));
            objects.push(bone_attribute(
                bone_attribute_ids[model_index][index],
                &name,
            ));
        }
        for (index, material) in input.model.materials.iter().enumerate() {
            for secondary_uv in [false, true] {
                let variant = usize::from(secondary_uv);
                objects.push(material_object(
                    material_ids[model_index][index][variant],
                    &format!(
                        "{}{}_{}",
                        input.prefix,
                        material.name,
                        if secondary_uv { "UV2" } else { "UV1" }
                    ),
                ));
                for (property, slot) in material_texture_slots(material) {
                    let Some(exported_texture) =
                        texture_paths.get(&texture_key(&input.prefix, &slot.name))
                    else {
                        continue;
                    };
                    let uv_index = texture_uv_index(property, secondary_uv);
                    texture_links.push(TextureLink {
                        id: ids.take(),
                        video_id: ids.take(),
                        material_id: material_ids[model_index][index][variant],
                        property,
                        name: exported_texture.name.clone(),
                        relative_path: exported_texture.relative_path.clone(),
                        uv_set: format!("UVChannel_{uv_index}"),
                        has_transparency: property == "DiffuseColor"
                            && exported_texture.has_transparency,
                    });
                }
            }
        }
        for mesh in &input.model.render.meshes {
            let geometry_id = ids.take();
            let model_id = ids.take();
            let used_bones = mesh_bones(mesh, input.model.render.bones.len());
            let skin_id = (!used_bones.is_empty()).then(|| ids.take());
            let clusters: Vec<(i64, usize)> = used_bones
                .into_iter()
                .map(|bone| (ids.take(), bone))
                .collect();
            let name = format!("{}{}", input.prefix, mesh.name);
            objects.push(geometry_object(
                geometry_id,
                &name,
                mesh,
                &input.model.render.bones,
            ));
            objects.push(model_object(
                model_id, &name, "Mesh", [0.0; 3], [0.0; 3], [1.0; 3],
            ));
            if let Some(skin) = skin_id {
                objects.push(
                    Node::object("Deformer", skin, &name, "Deformer", "Skin")
                        .child(Node::leaf("Version", 101i32))
                        .child(Node::leaf("Link_DeformAcuracy", 50.0f64)),
                );
                for &(cluster, bone) in &clusters {
                    objects.push(cluster_object(
                        cluster,
                        mesh,
                        bone,
                        &input.model.render.bones,
                    ));
                }
            }
            let material_id = material_ids[model_index]
                .get(mesh.material_index as usize)
                .map(|variants| variants[usize::from(mesh_has_secondary_uv(mesh))])
                .unwrap_or(root_id);
            mesh_links.push(MeshLink {
                geometry_id,
                model_id,
                material_id,
                skin_id,
                clusters,
            });
        }
    }
    for texture in &texture_links {
        let (texture_node, video_node) = texture_objects(texture, &texture_folder);
        objects.push(texture_node);
        objects.push(video_node);
    }

    let mut connections = Node::new("Connections");
    connections.push(connection(root_id, 0));
    connections.push(connection(root_attribute_id, root_id));
    for (model_index, input) in models.iter().enumerate() {
        for (index, bone) in input.model.render.bones.iter().enumerate() {
            connections.push(connection(
                bone_attribute_ids[model_index][index],
                bone_ids[model_index][index],
            ));
            let parent = if bone.parent_index >= 0 {
                bone_ids[model_index]
                    .get(bone.parent_index as usize)
                    .copied()
                    .unwrap_or(root_id)
            } else {
                root_id
            };
            connections.push(connection(bone_ids[model_index][index], parent));
        }
    }
    let mut mesh_cursor = 0;
    for (model_index, input) in models.iter().enumerate() {
        for _mesh in &input.model.render.meshes {
            let link = &mesh_links[mesh_cursor];
            connections.push(connection(link.geometry_id, link.model_id));
            connections.push(connection(link.model_id, root_id));
            connections.push(connection(link.material_id, link.model_id));
            if let Some(skin) = link.skin_id {
                connections.push(connection(skin, link.geometry_id));
                for &(cluster, bone) in &link.clusters {
                    connections.push(connection(cluster, skin));
                    connections.push(connection(bone_ids[model_index][bone], cluster));
                }
            }
            mesh_cursor += 1;
        }
    }
    for texture in &texture_links {
        connections.push(connection(texture.video_id, texture.id));
        connections.push(property_connection(
            texture.id,
            texture.material_id,
            texture.property,
        ));
        if texture.property == "DiffuseColor" && texture.has_transparency {
            connections.push(property_connection(
                texture.id,
                texture.material_id,
                "TransparentColor",
            ));
        }
    }

    let bone_count = models
        .iter()
        .map(|input| input.model.render.bones.len())
        .sum::<usize>();
    let mesh_count = mesh_links.len();
    let material_count = models
        .iter()
        .map(|input| input.model.materials.len() * 2)
        .sum::<usize>();
    let deformer_count = mesh_links
        .iter()
        .map(|link| usize::from(link.skin_id.is_some()) + link.clusters.len())
        .sum::<usize>();
    let counts = ObjectCounts {
        node_attributes: bone_count + 1,
        geometries: mesh_count,
        models: 1 + bone_count + mesh_count,
        deformers: deformer_count,
        materials: material_count,
        textures: texture_links.len(),
        videos: texture_links.len(),
    };

    let document_url = output.to_string_lossy().replace('/', "\\");
    vec![
        header_extension(&document_url),
        Node::leaf("FileId", Attr::Raw(FILE_ID.to_vec())),
        Node::leaf("CreationTime", CREATION_TIME),
        Node::leaf("Creator", CREATOR),
        global_settings(),
        Node::new("Documents")
            .child(Node::leaf("Count", 1i32))
            .child(
                Node::object("Document", document_id, "Scene", "", "Scene")
                    .child(properties70([
                        p_object("SourceObject"),
                        p_string("ActiveAnimStackName", ""),
                    ]))
                    .child(Node::leaf("RootNode", 0i64)),
            ),
        Node::new("References"),
        definitions(&counts),
        objects,
        connections,
        Node::new("Takes").child(Node::leaf("Current", "")),
    ]
}

struct ObjectCounts {
    node_attributes: usize,
    geometries: usize,
    models: usize,
    deformers: usize,
    materials: usize,
    textures: usize,
    videos: usize,
}

fn header_extension(document_url: &str) -> Node {
    let (year, month, day, hour, minute, second, millisecond) = utc_now();
    let date_time_gmt =
        format!("{month:02}/{day:02}/{year:04} {hour:02}:{minute:02}:{second:02}.{millisecond:03}");
    let application = |prefix: &str| {
        [
            p_compound(prefix),
            p_string(&format!("{prefix}|ApplicationVendor"), "TotkBits"),
            p_string(&format!("{prefix}|ApplicationName"), "TotkBits"),
            p_string(
                &format!("{prefix}|ApplicationVersion"),
                env!("CARGO_PKG_VERSION"),
            ),
            p_datetime(&format!("{prefix}|DateTime_GMT"), &date_time_gmt),
        ]
    };
    let mut scene_properties = vec![
        p_url("DocumentUrl", document_url),
        p_url("SrcDocumentUrl", document_url),
    ];
    scene_properties.extend(application("Original"));
    scene_properties.push(p_string("Original|FileName", document_url));
    scene_properties.extend(application("LastSaved"));
    scene_properties.push(p_string("Original|ApplicationNativeFile", ""));
    Node::new("FBXHeaderExtension")
        .child(Node::leaf("FBXHeaderVersion", 1003i32))
        .child(Node::leaf("FBXVersion", 7400i32))
        .child(Node::leaf("EncryptionType", 0i32))
        .child(
            Node::new("CreationTimeStamp")
                .child(Node::leaf("Version", 1000i32))
                .child(Node::leaf("Year", year))
                .child(Node::leaf("Month", month))
                .child(Node::leaf("Day", day))
                .child(Node::leaf("Hour", hour))
                .child(Node::leaf("Minute", minute))
                .child(Node::leaf("Second", second))
                .child(Node::leaf("Millisecond", millisecond)),
        )
        .child(Node::leaf("Creator", CREATOR))
        .child(
            Node::new("SceneInfo")
                .attr("GlobalInfo\0\u{1}SceneInfo")
                .attr("UserData")
                .child(Node::leaf("Type", "UserData"))
                .child(Node::leaf("Version", 100i32))
                .child(
                    Node::new("MetaData")
                        .child(Node::leaf("Version", 100i32))
                        .child(Node::leaf("Title", ""))
                        .child(Node::leaf("Subject", ""))
                        .child(Node::leaf("Author", ""))
                        .child(Node::leaf("Keywords", ""))
                        .child(Node::leaf("Revision", ""))
                        .child(Node::leaf("Comment", "")),
                )
                .child(properties70(scene_properties)),
        )
}

fn global_settings() -> Node {
    Node::new("GlobalSettings")
        .child(Node::leaf("Version", 1000i32))
        .child(properties70([
            p_int("UpAxis", 1),
            p_int("UpAxisSign", 1),
            p_int("FrontAxis", 2),
            p_int("FrontAxisSign", -1),
            p_int("CoordAxis", 0),
            p_int("CoordAxisSign", 1),
            p_int("OriginalUpAxis", -1),
            p_int("OriginalUpAxisSign", 1),
            p_double("UnitScaleFactor", 1.0),
            p_double("OriginalUnitScaleFactor", 1.0),
            p_color_rgb("AmbientColor", [0.0; 3]),
            p_string("DefaultCamera", "Producer Perspective"),
            p_enum("TimeMode", 10),
            p_ktime("TimeSpanStart", 0),
            p_ktime("TimeSpanStop", 46_186_158_000),
            p_double("CustomFrameRate", 25.0),
        ]))
}

fn definitions(counts: &ObjectCounts) -> Node {
    let total = 1
        + counts.node_attributes
        + counts.geometries
        + counts.models
        + counts.deformers
        + counts.materials
        + counts.textures
        + counts.videos;
    let object_type = |name: &str, count: usize| {
        Node::leaf("ObjectType", name).child(Node::leaf("Count", count as i32))
    };
    let template = |name: &str, properties: Vec<Node>| {
        Node::leaf("PropertyTemplate", name).child(properties70(properties))
    };
    let mut node = Node::new("Definitions")
        .child(Node::leaf("Version", 100i32))
        .child(Node::leaf("Count", total as i32))
        .child(object_type("GlobalSettings", 1))
        .child(object_type("NodeAttribute", counts.node_attributes))
        .child(
            object_type("Geometry", counts.geometries).child(template("FbxMesh", mesh_template())),
        )
        .child(object_type("Model", counts.models).child(template("FbxNode", node_template())));
    if counts.deformers > 0 {
        node.push(object_type("Deformer", counts.deformers));
    }
    node.push(
        object_type("Material", counts.materials)
            .child(template("FbxSurfacePhong", phong_template())),
    );
    if counts.textures > 0 {
        node.push(
            object_type("Texture", counts.textures)
                .child(template("FbxFileTexture", texture_template())),
        );
        node.push(
            object_type("Video", counts.videos).child(template("FbxVideo", video_template())),
        );
    }
    node
}

fn mesh_template() -> Vec<Node> {
    vec![
        p_color_rgb("Color", [0.8; 3]),
        p_vector("BBoxMin", [0.0; 3]),
        p_vector("BBoxMax", [0.0; 3]),
        p_bool("Primary Visibility", true),
        p_bool("Casts Shadows", true),
        p_bool("Receive Shadows", true),
    ]
}

fn node_template() -> Vec<Node> {
    let mut properties = vec![
        p_enum("QuaternionInterpolate", 0),
        p_vector("RotationOffset", [0.0; 3]),
        p_vector("RotationPivot", [0.0; 3]),
        p_vector("ScalingOffset", [0.0; 3]),
        p_vector("ScalingPivot", [0.0; 3]),
        p_bool("TranslationActive", false),
        p_vector("TranslationMin", [0.0; 3]),
        p_vector("TranslationMax", [0.0; 3]),
    ];
    for axis in ["X", "Y", "Z"] {
        properties.push(p_bool(&format!("TranslationMin{axis}"), false));
    }
    for axis in ["X", "Y", "Z"] {
        properties.push(p_bool(&format!("TranslationMax{axis}"), false));
    }
    properties.extend([
        p_enum("RotationOrder", 0),
        p_bool("RotationSpaceForLimitOnly", false),
        p_double("RotationStiffnessX", 0.0),
        p_double("RotationStiffnessY", 0.0),
        p_double("RotationStiffnessZ", 0.0),
        p_double("AxisLen", 10.0),
        p_vector("PreRotation", [0.0; 3]),
        p_vector("PostRotation", [0.0; 3]),
        p_bool("RotationActive", false),
        p_vector("RotationMin", [0.0; 3]),
        p_vector("RotationMax", [0.0; 3]),
    ]);
    for axis in ["X", "Y", "Z"] {
        properties.push(p_bool(&format!("RotationMin{axis}"), false));
    }
    for axis in ["X", "Y", "Z"] {
        properties.push(p_bool(&format!("RotationMax{axis}"), false));
    }
    properties.extend([
        p_enum("InheritType", 0),
        p_bool("ScalingActive", false),
        p_vector("ScalingMin", [0.0; 3]),
        p_vector("ScalingMax", [1.0; 3]),
    ]);
    for axis in ["X", "Y", "Z"] {
        properties.push(p_bool(&format!("ScalingMin{axis}"), false));
    }
    for axis in ["X", "Y", "Z"] {
        properties.push(p_bool(&format!("ScalingMax{axis}"), false));
    }
    properties.extend([
        p_vector("GeometricTranslation", [0.0; 3]),
        p_vector("GeometricRotation", [0.0; 3]),
        p_vector("GeometricScaling", [1.0; 3]),
    ]);
    for name in [
        "MinDampRange",
        "MaxDampRange",
        "MinDampStrength",
        "MaxDampStrength",
        "PreferedAngle",
    ] {
        for axis in ["X", "Y", "Z"] {
            properties.push(p_double(&format!("{name}{axis}"), 0.0));
        }
    }
    properties.extend([
        p_object("LookAtProperty"),
        p_object("UpVectorProperty"),
        p_bool("Show", true),
        p_bool("NegativePercentShapeSupport", true),
        p_int("DefaultAttributeIndex", -1),
        p_bool("Freeze", false),
        p_bool("LODBox", false),
        p_lcl("Lcl Translation", [0.0; 3]),
        p_lcl("Lcl Rotation", [0.0; 3]),
        p_lcl("Lcl Scaling", [1.0; 3]),
        p_visibility(1.0),
        p_visibility_inheritance(1),
    ]);
    properties
}

fn phong_template() -> Vec<Node> {
    vec![
        p_string("ShadingModel", "Phong"),
        p_bool("MultiLayer", false),
        p_color("EmissiveColor", [0.0; 3]),
        p_number("EmissiveFactor", 1.0),
        p_color("AmbientColor", [0.2; 3]),
        p_number("AmbientFactor", 1.0),
        p_color("DiffuseColor", [0.8; 3]),
        p_number("DiffuseFactor", 1.0),
        p_color("TransparentColor", [0.0; 3]),
        p_number("TransparencyFactor", 0.0),
        p_number("Opacity", 1.0),
        p_vector("NormalMap", [0.0; 3]),
        p_vector("Bump", [0.0; 3]),
        p_double("BumpFactor", 1.0),
        p_color_rgb("DisplacementColor", [0.0; 3]),
        p_double("DisplacementFactor", 1.0),
        p_color_rgb("VectorDisplacementColor", [0.0; 3]),
        p_double("VectorDisplacementFactor", 1.0),
        p_color("SpecularColor", [0.2; 3]),
        p_number("SpecularFactor", 1.0),
        p_number("Shininess", 20.0),
        p_number("ShininessExponent", 20.0),
        p_color("ReflectionColor", [0.0; 3]),
        p_number("ReflectionFactor", 1.0),
    ]
}

fn texture_template() -> Vec<Node> {
    vec![
        p_enum("TextureTypeUse", 0),
        p_enum("AlphaSource", 2),
        p_double("Texture alpha", 1.0),
        p_bool("PremultiplyAlpha", true),
        p_enum("CurrentTextureBlendMode", 1),
        p_enum("CurrentMappingType", 0),
        p_string("UVSet", "default"),
        p_enum("WrapModeU", 0),
        p_enum("WrapModeV", 0),
        p_bool("UVSwap", false),
        p_vector("Translation", [0.0; 3]),
        p_vector("Rotation", [0.0; 3]),
        p_vector("Scaling", [1.0; 3]),
        p_vector("TextureRotationPivot", [0.0; 3]),
        p_vector("TextureScalingPivot", [0.0; 3]),
        p_bool("UseMaterial", false),
        p_bool("UseMipMap", false),
    ]
}

fn video_template() -> Vec<Node> {
    vec![
        p_int("Width", 0),
        p_int("Height", 0),
        p_url("Path", ""),
        p_enum("AccessMode", 0),
        p_int("StartFrame", 0),
        p_int("StopFrame", 0),
        p_ktime("Offset", 0),
        p_double("PlaySpeed", 0.0),
        p_bool("FreeRunning", false),
        p_bool("Loop", false),
        p_enum("InterlaceMode", 0),
        p_bool("ImageSequence", false),
        p_int("ImageSequenceOffset", 0),
        p_double("FrameRate", 0.0),
        p_int("LastFrame", 0),
    ]
}

// ---- Objects ------------------------------------------------------------

fn null_attribute(id: i64, name: &str) -> Node {
    Node::object("NodeAttribute", id, name, "NodeAttribute", "Null")
        .child(Node::leaf("TypeFlags", "Null"))
        .child(properties70([
            p_color_rgb("Color", [0.8; 3]),
            p_double("Size", 100.0),
            p_enum("Look", 1),
        ]))
}

fn bone_attribute(id: i64, name: &str) -> Node {
    Node::object("NodeAttribute", id, name, "NodeAttribute", "LimbNode")
        .child(Node::leaf("TypeFlags", "Skeleton"))
        .child(properties70([p_double("Size", 3.3)]))
}

fn model_object(
    id: i64,
    name: &str,
    kind: &str,
    translation: [f32; 3],
    rotation: [f32; 3],
    scale: [f32; 3],
) -> Node {
    let mut properties = vec![
        p_lcl("Lcl Translation", translation.map(f64::from)),
        p_lcl("Lcl Rotation", rotation.map(f64::from)),
        p_lcl("Lcl Scaling", scale.map(f64::from)),
    ];
    properties.push(p_int("DefaultAttributeIndex", 0));
    properties.push(p_enum("InheritType", 1));
    Node::object("Model", id, name, "Model", kind)
        .child(Node::leaf("Version", 232i32))
        .child(properties70(properties))
        .child(Node::leaf("MultiLayer", 0i32))
        .child(Node::leaf("MultiTake", 0i32))
        .child(Node::leaf("Shading", true))
        .child(Node::leaf("Culling", "CullingOff"))
}

fn material_object(id: i64, name: &str) -> Node {
    Node::object("Material", id, name, "Material", "")
        .child(Node::leaf("Version", 102i32))
        .child(Node::leaf("ShadingModel", "Phong"))
        .child(Node::leaf("MultiLayer", 0i32))
        .child(properties70([
            p_color("EmissiveColor", [0.0; 3]),
            p_number("EmissiveFactor", 1.0),
            p_color("AmbientColor", [0.2; 3]),
            p_number("AmbientFactor", 1.0),
            p_color("DiffuseColor", [0.8; 3]),
            p_number("DiffuseFactor", 1.0),
            p_color("TransparentColor", [1.0; 3]),
            p_number("TransparencyFactor", 0.0),
            p_number("Opacity", 1.0),
            p_color("SpecularColor", [0.2; 3]),
            p_number("SpecularFactor", 1.0),
            p_number("Shininess", 20.0),
            p_number("ShininessExponent", 20.0),
            p_color("ReflectionColor", [0.0; 3]),
            p_number("ReflectionFactor", 1.0),
        ]))
}

fn texture_objects(texture: &TextureLink, folder: &Path) -> (Node, Node) {
    let relative = texture.relative_path.replace('/', "\\");
    let absolute = folder
        .join(&texture.relative_path)
        .to_string_lossy()
        .replace('/', "\\");
    let texture_node = Node::object("Texture", texture.id, &texture.name, "Texture", "")
        .child(Node::leaf("Type", "TextureVideoClip"))
        .child(Node::leaf("Version", 202i32))
        .child(Node::leaf(
            "TextureName",
            format!("Texture::{}", texture.name),
        ))
        .child(properties70([
            p_string("UVSet", &texture.uv_set),
            p_bool("UseMaterial", true),
        ]))
        .child(Node::leaf("Media", format!("Video::{}", texture.name)))
        .child(Node::leaf("FileName", absolute.as_str()))
        .child(Node::leaf("RelativeFilename", relative.as_str()))
        .child(Node::new("ModelUVTranslation").attr(0i32).attr(0i32))
        .child(Node::new("ModelUVScaling").attr(1i32).attr(1i32))
        .child(Node::leaf(
            "Texture_Alpha_Source",
            if texture.has_transparency {
                "Alpha_Black"
            } else {
                "None"
            },
        ))
        .child(
            Node::new("Cropping")
                .attr(0i32)
                .attr(0i32)
                .attr(0i32)
                .attr(0i32),
        );
    let video_node = Node::object("Video", texture.video_id, &texture.name, "Video", "Clip")
        .child(Node::leaf("Type", "Clip"))
        .child(properties70([p_xref_url("Path", &absolute)]))
        .child(Node::leaf("UseMipMap", 0i32))
        .child(Node::leaf("Filename", absolute.as_str()))
        .child(Node::leaf("RelativeFilename", relative.as_str()));
    (texture_node, video_node)
}

fn geometry_object(id: i64, name: &str, mesh: &BfresMesh, bones: &[BfresBone]) -> Node {
    let (positions, normals) = model_space_geometry(mesh, bones);
    let polygons: Vec<i32> = mesh
        .indices
        .chunks_exact(3)
        .flat_map(|triangle| {
            [
                triangle[0] as i32,
                triangle[1] as i32,
                -(triangle[2] as i32) - 1,
            ]
        })
        .collect();
    let mut geometry = Node::object("Geometry", id, name, "Geometry", "Mesh")
        .child(Node::new("Properties70"))
        .child(Node::leaf("GeometryVersion", 124i32))
        .child(Node::leaf("Vertices", flatten3(&positions)))
        .child(Node::leaf("PolygonVertexIndex", polygons));
    let has_normals = normals.len() == positions.len();
    if has_normals {
        geometry.push(
            Node::leaf("LayerElementNormal", 0i32)
                .child(Node::leaf("Version", 101i32))
                .child(Node::leaf("Name", "Normals"))
                .child(Node::leaf("MappingInformationType", "ByVertice"))
                .child(Node::leaf("ReferenceInformationType", "Direct"))
                .child(Node::leaf("Normals", flatten3(&normals))),
        );
    }
    let uv_maps = if mesh.uv_maps.is_empty() {
        std::slice::from_ref(&mesh.uv0)
    } else {
        &mesh.uv_maps
    };
    let valid_uvs: Vec<_> = uv_maps
        .iter()
        .filter(|uv| uv.len() == positions.len())
        .collect();
    for (index, uv) in valid_uvs.iter().enumerate() {
        let flat: Vec<f64> = uv
            .iter()
            .flat_map(|value| [f64::from(value[0]), 1.0 - f64::from(value[1])])
            .collect();
        geometry.push(
            Node::leaf("LayerElementUV", index as i32)
                .child(Node::leaf("Version", 101i32))
                .child(Node::leaf("Name", format!("UVChannel_{}", index + 1)))
                .child(Node::leaf("MappingInformationType", "ByVertice"))
                .child(Node::leaf("ReferenceInformationType", "Direct"))
                .child(Node::leaf("UV", flat)),
        );
    }
    geometry.push(
        Node::leaf("LayerElementMaterial", 0i32)
            .child(Node::leaf("Version", 101i32))
            .child(Node::leaf("Name", ""))
            .child(Node::leaf("MappingInformationType", "AllSame"))
            .child(Node::leaf("ReferenceInformationType", "IndexToDirect"))
            .child(Node::leaf("Materials", vec![0i32])),
    );
    let mut layer = Node::leaf("Layer", 0i32).child(Node::leaf("Version", 100i32));
    if has_normals {
        layer.push(layer_element("LayerElementNormal", 0));
    }
    if !valid_uvs.is_empty() {
        layer.push(layer_element("LayerElementUV", 0));
    }
    layer.push(layer_element("LayerElementMaterial", 0));
    geometry.push(layer);
    for index in 1..valid_uvs.len() {
        geometry.push(
            Node::leaf("Layer", index as i32)
                .child(Node::leaf("Version", 100i32))
                .child(layer_element("LayerElementUV", index)),
        );
    }
    geometry
}

fn layer_element(kind: &str, index: usize) -> Node {
    Node::new("LayerElement")
        .child(Node::leaf("Type", kind))
        .child(Node::leaf("TypedIndex", index as i32))
}

fn cluster_object(id: i64, mesh: &BfresMesh, bone: usize, bones: &[BfresBone]) -> Node {
    let mut indices = Vec::new();
    let mut weights = Vec::new();
    for vertex in 0..mesh.positions.len() {
        let mut weight = 0.0f32;
        if mesh.vertex_skin_count == 1 {
            let index = mesh
                .bone_indices
                .get(vertex)
                .map(|v| v[0] as usize)
                .unwrap_or(mesh.bone_index as usize);
            if index == bone {
                weight = 1.0;
            }
        } else {
            for influence in 0..4 {
                if mesh
                    .bone_indices
                    .get(vertex)
                    .is_some_and(|v| v[influence] as usize == bone)
                {
                    weight += mesh
                        .bone_weights
                        .get(vertex)
                        .map_or(if influence == 0 { 1.0 } else { 0.0 }, |v| v[influence]);
                }
            }
        }
        if weight > 0.0 {
            indices.push(vertex as i32);
            weights.push(f64::from(weight));
        }
    }
    let bone_world = bone_world_matrix(bones, bone);
    let name = bones
        .get(bone)
        .map(|value| value.name.as_str())
        .unwrap_or("bone");
    Node::object(
        "Deformer",
        id,
        &format!("Cluster_{name}"),
        "SubDeformer",
        "Cluster",
    )
    .child(Node::leaf("Version", 100i32))
    .child(Node::new("UserData").attr("").attr(""))
    .child(Node::leaf("Indexes", indices))
    .child(Node::leaf("Weights", weights))
    .child(Node::leaf(
        "Transform",
        fbx_matrix(inverse_affine_matrix(bone_world)),
    ))
    .child(Node::leaf("TransformLink", fbx_matrix(bone_world)))
    .child(Node::leaf(
        "TransformAssociateModel",
        fbx_matrix(identity_matrix()),
    ))
}

fn connection(child: i64, parent: i64) -> Node {
    Node::new("C").attr("OO").attr(child).attr(parent)
}

fn property_connection(child: i64, parent: i64, property: &str) -> Node {
    Node::new("C")
        .attr("OP")
        .attr(child)
        .attr(parent)
        .attr(property)
}

fn flatten3(values: &[[f32; 3]]) -> Vec<f64> {
    values
        .iter()
        .flat_map(|value| value.map(f64::from))
        .collect()
}

/// FBX stores transform matrices transposed relative to the column-vector
/// matrices used by the viewer and the geometry conversion helpers.
fn fbx_matrix(values: [f64; 16]) -> Vec<f64> {
    let mut fbx = vec![0.0; 16];
    for row in 0..4 {
        for column in 0..4 {
            fbx[row * 4 + column] = values[column * 4 + row];
        }
    }
    fbx
}

/// Current UTC time as (year, month, day, hour, minute, second, millisecond).
fn utc_now() -> (i32, i32, i32, i32, i32, i32, i32) {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = elapsed.as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year as i32,
        month as i32,
        day as i32,
        (day_seconds / 3600) as i32,
        (day_seconds % 3600 / 60) as i32,
        (day_seconds % 60) as i32,
        elapsed.subsec_millis() as i32,
    )
}

// ---- Model helpers ------------------------------------------------------

fn mesh_bones(mesh: &BfresMesh, bone_count: usize) -> Vec<usize> {
    if mesh.vertex_skin_count > 0 {
        // A cluster also carries a bone's bind matrix. Blender's FBX exporter
        // deliberately emits zero-weight clusters for every armature bone so
        // importers can reconstruct the complete rest hierarchy instead of
        // guessing transforms for unweighted ancestors and siblings.
        return (0..bone_count).collect();
    }
    Vec::new()
}

fn material_texture_slots(
    material: &G1mMaterial,
) -> Vec<(&'static str, &crate::parser::AOC::g1m::G1mTextureSlot)> {
    let mut result = Vec::with_capacity(2);
    if let Some(slot) = material
        .texture_slots
        .iter()
        .find(|slot| slot.texture_type == "Diffuse")
    {
        result.push(("DiffuseColor", slot));
    }
    if let Some(slot) = material
        .texture_slots
        .iter()
        .find(|slot| slot.texture_type == "Normal")
    {
        result.push(("NormalMap", slot));
    }
    result
}

fn mesh_has_secondary_uv(mesh: &BfresMesh) -> bool {
    mesh.uv_maps
        .iter()
        .filter(|uvs| uvs.len() == mesh.positions.len())
        .count()
        > 1
}

fn texture_uv_index(property: &str, secondary_uv: bool) -> usize {
    if property == "DiffuseColor" || !secondary_uv {
        1
    } else {
        2
    }
}

fn model_space_geometry(mesh: &BfresMesh, bones: &[BfresBone]) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    if mesh.vertex_skin_count != 1 || bones.is_empty() {
        return (mesh.positions.clone(), mesh.normals.clone());
    }
    let worlds: Vec<_> = (0..bones.len())
        .map(|index| bone_world_matrix(bones, index))
        .collect();
    let mut positions = mesh.positions.clone();
    let mut normals = mesh.normals.clone();
    for vertex in 0..positions.len() {
        let bone = mesh
            .bone_indices
            .get(vertex)
            .map(|value| value[0] as usize)
            .unwrap_or(mesh.bone_index as usize);
        let Some(matrix) = worlds.get(bone) else {
            continue;
        };
        positions[vertex] = transform_point(*matrix, positions[vertex]);
        if let Some(normal) = normals.get_mut(vertex) {
            *normal = transform_normal(*matrix, *normal);
        }
    }
    (positions, normals)
}

pub(crate) fn bone_world_matrix(bones: &[BfresBone], index: usize) -> [f64; 16] {
    let Some(bone) = bones.get(index) else {
        return identity_matrix();
    };
    let local = compose_matrix(bone.translation, bone.rotation, bone.scale);
    if bone.parent_index >= 0 {
        multiply_matrix(bone_world_matrix(bones, bone.parent_index as usize), local)
    } else {
        local
    }
}

fn compose_matrix(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> [f64; 16] {
    let [x, y, z, w] = normalize4(q);
    let (x, y, z, w) = (x as f64, y as f64, z as f64, w as f64);
    let (sx, sy, sz) = (s[0] as f64, s[1] as f64, s[2] as f64);
    [
        (1.0 - 2.0 * (y * y + z * z)) * sx,
        (2.0 * (x * y - z * w)) * sy,
        (2.0 * (x * z + y * w)) * sz,
        t[0] as f64,
        (2.0 * (x * y + z * w)) * sx,
        (1.0 - 2.0 * (x * x + z * z)) * sy,
        (2.0 * (y * z - x * w)) * sz,
        t[1] as f64,
        (2.0 * (x * z - y * w)) * sx,
        (2.0 * (y * z + x * w)) * sy,
        (1.0 - 2.0 * (x * x + y * y)) * sz,
        t[2] as f64,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

fn multiply_matrix(a: [f64; 16], b: [f64; 16]) -> [f64; 16] {
    let mut result = [0.0; 16];
    for row in 0..4 {
        for column in 0..4 {
            for k in 0..4 {
                result[row * 4 + column] += a[row * 4 + k] * b[k * 4 + column];
            }
        }
    }
    result
}

pub(crate) fn inverse_affine_matrix(m: [f64; 16]) -> [f64; 16] {
    let determinant = m[0] * (m[5] * m[10] - m[6] * m[9]) - m[1] * (m[4] * m[10] - m[6] * m[8])
        + m[2] * (m[4] * m[9] - m[5] * m[8]);
    if determinant.abs() <= f64::EPSILON {
        return identity_matrix();
    }
    let inverse = [
        (m[5] * m[10] - m[6] * m[9]) / determinant,
        (m[2] * m[9] - m[1] * m[10]) / determinant,
        (m[1] * m[6] - m[2] * m[5]) / determinant,
        (m[6] * m[8] - m[4] * m[10]) / determinant,
        (m[0] * m[10] - m[2] * m[8]) / determinant,
        (m[2] * m[4] - m[0] * m[6]) / determinant,
        (m[4] * m[9] - m[5] * m[8]) / determinant,
        (m[1] * m[8] - m[0] * m[9]) / determinant,
        (m[0] * m[5] - m[1] * m[4]) / determinant,
    ];
    let translation = [m[3], m[7], m[11]];
    [
        inverse[0],
        inverse[1],
        inverse[2],
        -(inverse[0] * translation[0] + inverse[1] * translation[1] + inverse[2] * translation[2]),
        inverse[3],
        inverse[4],
        inverse[5],
        -(inverse[3] * translation[0] + inverse[4] * translation[1] + inverse[5] * translation[2]),
        inverse[6],
        inverse[7],
        inverse[8],
        -(inverse[6] * translation[0] + inverse[7] * translation[1] + inverse[8] * translation[2]),
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

fn transform_point(m: [f64; 16], p: [f32; 3]) -> [f32; 3] {
    [
        (m[0] * p[0] as f64 + m[1] * p[1] as f64 + m[2] * p[2] as f64 + m[3]) as f32,
        (m[4] * p[0] as f64 + m[5] * p[1] as f64 + m[6] * p[2] as f64 + m[7]) as f32,
        (m[8] * p[0] as f64 + m[9] * p[1] as f64 + m[10] * p[2] as f64 + m[11]) as f32,
    ]
}

fn transform_normal(m: [f64; 16], normal: [f32; 3]) -> [f32; 3] {
    let determinant = m[0] * (m[5] * m[10] - m[6] * m[9]) - m[1] * (m[4] * m[10] - m[6] * m[8])
        + m[2] * (m[4] * m[9] - m[5] * m[8]);
    if determinant.abs() <= f64::EPSILON {
        return normalize3(normal);
    }
    let inverse = [
        (m[5] * m[10] - m[6] * m[9]) / determinant,
        (m[2] * m[9] - m[1] * m[10]) / determinant,
        (m[1] * m[6] - m[2] * m[5]) / determinant,
        (m[6] * m[8] - m[4] * m[10]) / determinant,
        (m[0] * m[10] - m[2] * m[8]) / determinant,
        (m[2] * m[4] - m[0] * m[6]) / determinant,
        (m[4] * m[9] - m[5] * m[8]) / determinant,
        (m[1] * m[8] - m[0] * m[9]) / determinant,
        (m[0] * m[5] - m[1] * m[4]) / determinant,
    ];
    normalize3([
        (inverse[0] * normal[0] as f64
            + inverse[3] * normal[1] as f64
            + inverse[6] * normal[2] as f64) as f32,
        (inverse[1] * normal[0] as f64
            + inverse[4] * normal[1] as f64
            + inverse[7] * normal[2] as f64) as f32,
        (inverse[2] * normal[0] as f64
            + inverse[5] * normal[1] as f64
            + inverse[8] * normal[2] as f64) as f32,
    ])
}

fn quaternion_euler_degrees(q: [f32; 4]) -> [f32; 3] {
    let [x, y, z, w] = normalize4(q);
    let r00 = 1.0 - 2.0 * (y * y + z * z);
    let r10 = 2.0 * (x * y + z * w);
    let r11 = 1.0 - 2.0 * (x * x + z * z);
    let r12 = 2.0 * (y * z - x * w);
    let r20 = 2.0 * (x * z - y * w);
    let r21 = 2.0 * (y * z + x * w);
    let r22 = 1.0 - 2.0 * (x * x + y * y);
    let pitch = (-r20).clamp(-1.0, 1.0).asin();
    let (roll, yaw) = if pitch.cos().abs() > 1.0e-5 {
        (r21.atan2(r22), r10.atan2(r00))
    } else {
        // At +/-90 degrees, roll and yaw describe the same degree of
        // freedom. Pin yaw to zero instead of evaluating atan2(0, 0), which
        // otherwise introduces a spurious 180-degree pose rotation.
        ((-r12).atan2(r11), 0.0)
    };
    [roll.to_degrees(), pitch.to_degrees(), yaw.to_degrees()]
}

fn normalize4(q: [f32; 4]) -> [f32; 4] {
    let length = q.iter().map(|value| value * value).sum::<f32>().sqrt();
    if length <= f32::EPSILON {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        q.map(|value| value / length)
    }
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let length = v.iter().map(|value| value * value).sum::<f32>().sqrt();
    if length <= f32::EPSILON {
        v
    } else {
        v.map(|value| value / length)
    }
}

fn identity_matrix() -> [f64; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn texture_key(prefix: &str, name: &str) -> String {
    format!("{prefix}\0{name}")
}

fn safe_name(value: &str) -> String {
    let result: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if result.is_empty() {
        "texture".into()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_g1m_importer_texture_prefixes() {
        assert_eq!(texture_kind_prefix("Diffuse"), "alb");
        assert_eq!(texture_kind_prefix("Normal"), "nrm");
        assert_eq!(texture_kind_prefix("Emission"), "emm");
        assert_eq!(texture_kind_prefix("AmbientOcclusion"), "aoo");
        assert_eq!(texture_kind_prefix("Specular"), "spm");
        assert_eq!(texture_kind_prefix("Unknown"), "tex");
    }

    #[test]
    fn routes_material_textures_like_the_viewer() {
        assert_eq!(texture_uv_index("DiffuseColor", false), 1);
        assert_eq!(texture_uv_index("DiffuseColor", true), 1);
        assert_eq!(texture_uv_index("NormalMap", false), 1);
        assert_eq!(texture_uv_index("NormalMap", true), 2);
    }

    #[test]
    fn transparency_requires_a_fully_transparent_pixel() {
        let png = |alpha| {
            let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                1,
                1,
                image::Rgba([255, 255, 255, alpha]),
            ));
            let mut output = Cursor::new(Vec::new());
            image
                .write_to(&mut output, image::ImageFormat::Png)
                .unwrap();
            output.into_inner()
        };
        assert!(!has_fully_transparent_pixel(&png(255)).unwrap());
        assert!(!has_fully_transparent_pixel(&png(127)).unwrap());
        assert!(!has_fully_transparent_pixel(&png(1)).unwrap());
        assert!(has_fully_transparent_pixel(&png(0)).unwrap());
    }

    #[test]
    fn utc_now_is_a_plausible_calendar_date() {
        let (year, month, day, hour, minute, second, millisecond) = utc_now();
        assert!(year >= 2024);
        assert!((1..=12).contains(&month));
        assert!((1..=31).contains(&day));
        assert!((0..24).contains(&hour));
        assert!((0..60).contains(&minute));
        assert!((0..60).contains(&second));
        assert!((0..1000).contains(&millisecond));
    }

    #[test]
    fn exports_g1m_geometry_skeleton_and_skinning() {
        let source = std::env::var_os("TOTKBITS_TEST_G1M")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/g1m/038bb045.g1m")
            });
        if !source.is_file() {
            return;
        }
        let data = fs::read(&source).unwrap();
        let model = G1mFile::parse_for_export(&data, "038bb045").unwrap();
        let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/g1m_fbx_export_test.fbx");
        export_g1m(
            &[(&model, &[], String::new())],
            &output,
            TextureExportFormat::None,
            source.file_stem().and_then(|stem| stem.to_str()).unwrap(),
        )
        .unwrap();
        let exported = fs::read(&output).unwrap();
        assert!(crate::Settings::Magic::is_fbx(&exported));
        let parsed = crate::parser::fbx::FbxFile::parse(&exported, "roundtrip").unwrap();
        assert_eq!(parsed.render.meshes.len(), model.render.meshes.len());
        let imported = crate::parser::fbx::import::import_for_g1m(&exported).unwrap();
        assert_eq!(imported.bones.len(), model.render.bones.len());
        assert_eq!(imported.meshes.len(), model.render.meshes.len());
        if std::env::var_os("TOTKBITS_KEEP_TEST_FBX").is_none() {
            fs::remove_file(output).unwrap();
        }
    }
}
