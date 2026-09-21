//! Convex collision of a custom Zonai device from an OBJ mesh.
//!
//! Ultrahand bonds and the Autobuild fans attach to convex shapes, so a
//! device with a custom model gets its collision as a Phive `Polytope` list
//! rather than a mesh: the OBJ (in the model's own space) is split into
//! convex pieces with CoACD, every piece is thinned to the vertex budget
//! vanilla polytopes stay within, and each piece's `AutoCalc` block (volume,
//! centroid, inertia, bounds) is computed the way the vanilla shape files
//! record it. The pieces replace the template's ShapeParam wholesale; the
//! rigid body that uses that shape gets its centre of mass moved to the new
//! centroid. Both entries are then renamed after the device (the generic
//! rename pass does the rest of the chain: ControllerSetParam, PhysicsParam,
//! the ActorParam's `PhysicsRef`).

use super::zonai::{component_reference, parent_reference, reference_to_internal};
use crate::{
    file_format::BinTextFile::BymlFile,
    tools::{
        coacd::{self, CoacdOptions, ConvexPiece},
        convex_hull::{compare_points, thin_vertices, ConvexHull, MassProperties, Point},
    },
};
use roead::byml::{Byml, Map};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

/// Upper bound on the convex pieces (vanilla devices use up to about 14; a
/// ship-sized hull needs more to keep its deck walkable).
pub const MAX_PIECES: i32 = 40;
/// Vertices per piece (vanilla polytopes stay around 40).
pub const MAX_VERTICES: usize = 40;
/// Plain climbable stone; the template's presets often carry device-specific
/// variants (`Material_Stone_ZonauLight`) that do not suit a custom hull.
pub const DEFAULT_MATERIAL: &str = "Material_Stone";
/// CoACD concavity threshold: fine enough to keep decks and bulwarks apart.
const THRESHOLD: f64 = 0.04;
/// Manifold preprocessing resolution (custom OBJ exports are rarely watertight).
const PREPROCESS_RESOLUTION: i32 = 60;

/// One Polytope entry.
#[derive(Clone, Debug, PartialEq)]
pub struct PolytopePiece {
    pub vertices: Vec<Point>,
    pub mass: MassProperties,
    pub min: Point,
    pub max: Point,
}

/// The decomposed collision, ready to be written as a ShapeParam.
#[derive(Clone, Debug, PartialEq)]
pub struct ConvexCollision {
    pub source: PathBuf,
    pub material: String,
    pub pieces: Vec<PolytopePiece>,
    /// Combined properties, with the inertia moved to the common centroid.
    pub mass: MassProperties,
    pub min: Point,
    pub max: Point,
}

/// What the collision step wrote, for the generation report.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollisionReport {
    pub source: PathBuf,
    /// Final pack path of the Polytope ShapeParam.
    pub shape_param: String,
    /// Final pack path of the rigid body whose centre of mass was moved.
    pub rigid_body: String,
    pub material: String,
    pub pieces: usize,
    pub max_vertices: usize,
    pub volume: f32,
}

/// The entries `apply_to_pack` edited (template paths) and the explicit
/// rename the rigid body needs.
pub(super) struct CollisionEntries {
    pub shape: String,
    pub body: String,
    /// `(template path, new path)` when the body entry is not named after
    /// the template actor (a shared `<X>_Body_0` gets `<actor>_Body_0`).
    pub body_rename: Option<(String, String)>,
}

impl ConvexCollision {
    /// Decomposes the OBJ at `path` with CoACD.
    pub fn from_obj(path: &Path, material: Option<&str>) -> io::Result<Self> {
        let text = fs::read_to_string(path).map_err(|error| {
            io::Error::new(error.kind(), format!("{}: {error}", path.display()))
        })?;
        let (vertices, triangles) = parse_obj(&text)?;
        if triangles.is_empty() {
            return Err(invalid(format!("{}: the OBJ has no faces", path.display())));
        }
        let options = CoacdOptions {
            threshold: THRESHOLD,
            max_convex_hulls: MAX_PIECES,
            preprocess: true,
            preprocess_resolution: PREPROCESS_RESOLUTION,
            ..CoacdOptions::default()
        };
        eprintln!(
            "[items creator] CoACD: decomposing {} ({} vertices, {} triangles, up to {MAX_PIECES} pieces)",
            path.display(),
            vertices.len(),
            triangles.len()
        );
        let started = std::time::Instant::now();
        let pieces = coacd::decompose(&vertices, &triangles, &options)?;
        eprintln!(
            "[items creator] CoACD: {} pieces in {:.1?}",
            pieces.len(),
            started.elapsed()
        );
        Self::from_pieces(pieces, material, path)
    }

    /// Thins every piece to [`MAX_VERTICES`] and computes the mass properties.
    pub fn from_pieces(
        pieces: Vec<ConvexPiece>,
        material: Option<&str>,
        source: &Path,
    ) -> io::Result<Self> {
        let material = material
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(DEFAULT_MATERIAL)
            .to_owned();
        let mut shaped = Vec::with_capacity(pieces.len());
        for (index, piece) in pieces.iter().enumerate() {
            let vertices = thin_vertices(&piece.vertices, MAX_VERTICES)
                .ok_or_else(|| invalid(format!("convex piece {index} spans no volume")))?;
            let hull = ConvexHull::build(&vertices)
                .ok_or_else(|| invalid(format!("convex piece {index} spans no volume")))?;
            let mass = hull.mass_properties();
            let (min, max) = bounds(&vertices);
            shaped.push(PolytopePiece {
                vertices,
                mass,
                min,
                max,
            });
        }
        let total_volume: f64 = shaped.iter().map(|piece| piece.mass.volume).sum();
        if !(total_volume > 0.0) {
            return Err(invalid("the convex pieces span no volume"));
        }
        let mut center = [0.0; 3];
        for piece in &shaped {
            for axis in 0..3 {
                center[axis] += piece.mass.volume * piece.mass.center[axis];
            }
        }
        let center = center.map(|value| value / total_volume);
        // Parallel-axis shift of every piece's unit-mass diagonal.
        let mut inertia = [0.0; 3];
        for piece in &shaped {
            let fraction = piece.mass.volume / total_volume;
            let d = [
                piece.mass.center[0] - center[0],
                piece.mass.center[1] - center[1],
                piece.mass.center[2] - center[2],
            ];
            let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            for axis in 0..3 {
                inertia[axis] += fraction * (piece.mass.inertia[axis] + d2 - d[axis] * d[axis]);
            }
        }
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for piece in &shaped {
            for axis in 0..3 {
                min[axis] = min[axis].min(piece.min[axis]);
                max[axis] = max[axis].max(piece.max[axis]);
            }
        }
        Ok(Self {
            source: source.to_path_buf(),
            material,
            pieces: shaped,
            mass: MassProperties {
                volume: total_volume,
                center,
                inertia,
            },
            min,
            max,
        })
    }

    pub fn max_vertices(&self) -> usize {
        self.pieces
            .iter()
            .map(|piece| piece.vertices.len())
            .max()
            .unwrap_or(0)
    }

    /// The whole `phive__ShapeParam` document: the combined `AutoCalc` and
    /// one `Polytope` entry per piece.
    pub fn shape_param(&self) -> Byml {
        let mut root = Map::default();
        root.insert("AutoCalc".into(), auto_calc(&self.mass, self.min, self.max));
        let polytopes = self
            .pieces
            .iter()
            .map(|piece| {
                let mut entry = Map::default();
                entry.insert(
                    "AutoCalc".into(),
                    auto_calc(&piece.mass, piece.min, piece.max),
                );
                entry.insert("MassDistributionFactor".into(), Byml::Float(1.0));
                entry.insert(
                    "MaterialPresets".into(),
                    Byml::Array(vec![Byml::String(self.material.as_str().into())]),
                );
                entry.insert("Name".into(), Byml::String("".into()));
                entry.insert("OffsetRotation".into(), vec3([0.0, -0.0, 0.0]));
                entry.insert("OffsetTranslation".into(), vec3([0.0, 0.0, 0.0]));
                entry.insert(
                    "Vertices".into(),
                    Byml::Array(piece.vertices.iter().map(|&vertex| vec3(vertex)).collect()),
                );
                Byml::Map(entry)
            })
            .collect();
        root.insert("Polytope".into(), Byml::Array(polytopes));
        Byml::Map(root)
    }
}

/// Replaces the template's shape with the collision and moves the rigid
/// body's centre of mass, in the pack's parsed entries (pre-rename paths).
pub(super) fn apply_to_pack(
    files: &mut BTreeMap<String, BymlFile<'_>>,
    originals: &BTreeMap<String, Byml>,
    template_actor: &str,
    collision: &ConvexCollision,
) -> io::Result<CollisionEntries> {
    let actor_file = format!("Actor/{template_actor}.engine__actor__ActorParam.bgyml");
    let actor = files
        .get(&actor_file)
        .ok_or_else(|| invalid(format!("template pack has no {actor_file}")))?;
    let physics = component_reference(&actor.pio, originals, "PhysicsRef", 0)?
        .map(|reference| reference_to_internal(&reference))
        .filter(|path| files.contains_key(path))
        .ok_or_else(|| invalid("the template keeps its PhysicsParam outside its pack"))?;
    let controller = inherited_string(files, &physics, "ControllerSetPath", 0)
        .map(|reference| reference_to_internal(&reference))
        .filter(|path| files.contains_key(path))
        .ok_or_else(|| invalid("the template keeps its ControllerSetParam outside its pack"))?;
    let controller_map = files[&controller]
        .pio
        .as_map()
        .map_err(|_| invalid(format!("{controller} is not a map")))?;
    let bodies = named_paths(controller_map, "RigidBodyEntityNamePathAry");
    let (_, body) = bodies
        .first()
        .cloned()
        .ok_or_else(|| invalid(format!("{controller} names no rigid body")))?;
    let body = reference_to_internal(&body);
    if !files.contains_key(&body) {
        return Err(invalid(format!(
            "the template keeps its rigid body {body} outside its pack"
        )));
    }
    let shape_name = files[&body]
        .pio
        .as_map()
        .ok()
        .and_then(|map| map.get("ShapeName"))
        .and_then(|value| value.as_string().ok())
        .map(|name| name.to_string());
    let shapes = named_paths(controller_map, "ShapeNamePathAry");
    let (_, shape) = shapes
        .iter()
        .find(|(name, _)| Some(name) == shape_name.as_ref())
        .or_else(|| shapes.first())
        .cloned()
        .ok_or_else(|| invalid(format!("{controller} names no shape")))?;
    let shape = reference_to_internal(&shape);
    if !files.contains_key(&shape) {
        return Err(invalid(format!(
            "the template keeps its shape {shape} outside its pack"
        )));
    }

    files
        .get_mut(&shape)
        .ok_or_else(|| invalid("entry vanished"))?
        .pio = collision.shape_param();
    let body_document = files
        .get_mut(&body)
        .ok_or_else(|| invalid("entry vanished"))?;
    body_document
        .pio
        .as_mut_map()
        .map_err(|_| invalid(format!("{body} is not a map")))?
        .insert("ComPos".into(), vec3(collision.mass.center));

    Ok(CollisionEntries {
        shape,
        body_rename: body_private_name(&body, template_actor),
        body,
    })
}

/// `Dir/<X>_Body_0.<suffix>` -> `Dir/<actor>_Body_0.<suffix>` for a rigid
/// body not named after the template (the generic rename covers those).
fn body_private_name(path: &str, template_actor: &str) -> Option<(String, String)> {
    let (dir, file) = path.rsplit_once('/')?;
    let (stem, suffix) = file.split_once('.')?;
    if stem.contains(template_actor) {
        return None;
    }
    let body_at = stem.rfind("_Body")?;
    Some((
        path.to_owned(),
        format!("{dir}/{{actor}}{}.{suffix}", &stem[body_at..]),
    ))
}

/// The `body_rename` placeholder resolved for the new actor.
pub(super) fn resolve_body_rename(template: &str, new_actor: &str) -> String {
    template.replace("{actor}", new_actor)
}

/// `[{FilePath, Name}]` entries of a ControllerSetParam list as `(Name, FilePath)`.
fn named_paths(map: &Map, key: &str) -> Vec<(String, String)> {
    map.get(key)
        .and_then(|value| value.as_array().ok())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let entry = entry.as_map().ok()?;
                    let path = entry.get("FilePath")?.as_string().ok()?.to_string();
                    let name = entry
                        .get("Name")
                        .and_then(|value| value.as_string().ok())
                        .map(|name| name.to_string())
                        .unwrap_or_default();
                    Some((name, path))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A string field of an entry, following in-pack `$parent` links.
fn inherited_string(
    files: &BTreeMap<String, BymlFile<'_>>,
    path: &str,
    key: &str,
    depth: usize,
) -> Option<String> {
    let document = &files.get(path)?.pio;
    let map = document.as_map().ok()?;
    if let Some(value) = map.get(key).and_then(|value| value.as_string().ok()) {
        return Some(value.to_string());
    }
    if depth > 8 {
        return None;
    }
    let parent = reference_to_internal(&parent_reference(document)?);
    inherited_string(files, &parent, key, depth + 1)
}

/// Positions and fan-triangulated faces of an OBJ (materials and normals ignored).
pub fn parse_obj(text: &str) -> io::Result<(Vec<Point>, Vec<[u32; 3]>)> {
    let mut vertices: Vec<Point> = Vec::new();
    let mut triangles = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let mut tokens = line.split_whitespace();
        match tokens.next() {
            Some("v") => {
                let mut point = [0.0; 3];
                for value in &mut point {
                    *value = tokens
                        .next()
                        .and_then(|token| token.parse::<f64>().ok())
                        .ok_or_else(|| invalid(format!("OBJ line {}: bad vertex", number + 1)))?;
                }
                vertices.push(point);
            }
            Some("f") => {
                let mut indices = Vec::new();
                for token in tokens {
                    let index: i64 = token
                        .split('/')
                        .next()
                        .and_then(|value| value.parse().ok())
                        .ok_or_else(|| invalid(format!("OBJ line {}: bad face", number + 1)))?;
                    let resolved = if index < 0 {
                        vertices.len() as i64 + index
                    } else {
                        index - 1
                    };
                    if resolved < 0 || resolved as usize >= vertices.len() {
                        return Err(invalid(format!(
                            "OBJ line {}: vertex {index} does not exist",
                            number + 1
                        )));
                    }
                    indices.push(resolved as u32);
                }
                for k in 1..indices.len().saturating_sub(1) {
                    triangles.push([indices[0], indices[k], indices[k + 1]]);
                }
            }
            _ => {}
        }
    }
    Ok((vertices, triangles))
}

fn bounds(points: &[Point]) -> (Point, Point) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for point in points {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    (min, max)
}

/// Six significant digits, the precision the vanilla shape files carry.
pub fn round6(value: f64) -> f32 {
    if value == 0.0 || !value.is_finite() {
        return value as f32;
    }
    let exponent = value.abs().log10().floor() as i32;
    let scale = if exponent < -4 {
        1e6
    } else {
        10f64.powi(5 - exponent)
    };
    ((value * scale).round() / scale) as f32
}

fn vec3(point: Point) -> Byml {
    let mut map = Map::default();
    map.insert("X".into(), Byml::Float(round6(point[0])));
    map.insert("Y".into(), Byml::Float(round6(point[1])));
    map.insert("Z".into(), Byml::Float(round6(point[2])));
    Byml::Map(map)
}

fn auto_calc(mass: &MassProperties, min: Point, max: Point) -> Byml {
    let mut map = Map::default();
    map.insert("Axis".into(), vec3([0.0, 0.0, 0.0]));
    map.insert("Center".into(), vec3(mass.center));
    map.insert("Max".into(), vec3(max));
    map.insert("Min".into(), vec3(min));
    map.insert("Tensor".into(), vec3(mass.inertia));
    map.insert("Volume".into(), Byml::Float(round6(mass.volume)));
    Byml::Map(map)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_to_six_significant_digits() {
        assert_eq!(round6(0.4018063), 0.401806);
        assert_eq!(round6(412.9581), 412.958);
        assert_eq!(round6(-14.92499), -14.925);
        assert_eq!(round6(0.0000012345), 1.0e-6);
        assert_eq!(round6(0.0), 0.0);
    }

    #[test]
    fn body_names_follow_the_actor() {
        assert_eq!(
            body_private_name(
                "Phive/RigidBodyEntityParam/Test_AsbObj_SlipBoard_Body_0.phive__RigidBodyEntityParam.bgyml",
                "SpObj_SlipBoard_A_01"
            )
            .map(|(_, to)| resolve_body_rename(&to, "SpObj_SlipBoard_A_90")),
            Some("Phive/RigidBodyEntityParam/SpObj_SlipBoard_A_90_Body_0.phive__RigidBodyEntityParam.bgyml".to_owned())
        );
        assert!(body_private_name(
            "Phive/RigidBodyEntityParam/SpObj_Cannon_A_01_Body_0.phive__RigidBodyEntityParam.bgyml",
            "SpObj_Cannon_A_01"
        )
        .is_none());
    }

    #[test]
    fn cube_obj_becomes_one_polytope() {
        let obj = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\n\
                   f 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n";
        let (vertices, triangles) = parse_obj(obj).expect("obj");
        assert_eq!(vertices.len(), 8);
        assert_eq!(triangles.len(), 12);
        let piece = ConvexPiece {
            vertices,
            triangles,
        };
        let collision =
            ConvexCollision::from_pieces(vec![piece], None, Path::new("cube.obj")).expect("pieces");
        assert_eq!(collision.pieces.len(), 1);
        assert_eq!(collision.material, DEFAULT_MATERIAL);
        assert!((collision.mass.volume - 1.0).abs() < 1e-9);
        let shape = collision.shape_param();
        let root = shape.as_map().expect("map");
        let polytopes = root["Polytope"].as_array().expect("array");
        assert_eq!(polytopes.len(), 1);
        let entry = polytopes[0].as_map().expect("map");
        assert_eq!(entry["Vertices"].as_array().expect("array").len(), 8);
        assert_eq!(
            root["AutoCalc"].as_map().expect("map")["Volume"],
            Byml::Float(1.0)
        );
    }

    /// `cargo test --release coacd_ship -- --ignored --nocapture` with
    /// `COLLISION_OBJ=<hull.obj>`: runs the real decomposition.
    #[test]
    #[ignore]
    fn coacd_ship() {
        let Ok(path) = std::env::var("COLLISION_OBJ") else {
            return;
        };
        let collision = ConvexCollision::from_obj(Path::new(&path), None).expect("collision");
        eprintln!(
            "{} pieces, volume {:.3}, centre {:?}, max vertices {}",
            collision.pieces.len(),
            collision.mass.volume,
            collision.mass.center,
            collision.max_vertices()
        );
        assert!(collision.max_vertices() <= MAX_VERTICES);
    }
}
