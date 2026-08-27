//! Minimal CPU rasterizer turning a parsed model into a PNG preview without a
//! window or GPU. Framing, lighting, and background mirror the 3D viewport's
//! front camera (`Bfres3DView.jsx`) so CLI renders resemble the in-app view.
//! Geometry must already be in model space (the G1M/LM3 convention); the
//! BFRES rigid-bind bone transform is intentionally not applied.

use crate::file_format::Model3D::bfres::BfresRenderGraph;
use crate::parser::AOC::g1m::{G1mMaterial, ResolvedG1tTexture};
use base64::Engine;
use std::collections::HashMap;

pub const DEFAULT_SIZE: u32 = 1024;

const FOV_DEGREES: f32 = 42.0;
const UNTEXTURED_COLOR: [f32; 3] = [0.682, 0.722, 0.761];
const LIGHT_DIRECTION: [f32; 3] = [6.0, 10.0, 8.0];
const AMBIENT: f32 = 0.45;
const DIFFUSE: f32 = 0.55;

struct DecodedTexture {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

impl DecodedTexture {
    /// Bilinear sample with repeat wrapping. `v = 0` is the top image row,
    /// matching the viewport's `flipY = false` texture setup.
    fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        let wrap = |value: f32| value - value.floor();
        let x = wrap(u) * self.width as f32 - 0.5;
        let y = wrap(v) * self.height as f32 - 0.5;
        let x0 = x.floor() as isize;
        let y0 = y.floor() as isize;
        let fx = x - x0 as f32;
        let fy = y - y0 as f32;
        let texel = |x: isize, y: isize| {
            let x = x.rem_euclid(self.width as isize) as usize;
            let y = y.rem_euclid(self.height as isize) as usize;
            let base = (y * self.width + x) * 4;
            [
                self.rgba[base] as f32 / 255.0,
                self.rgba[base + 1] as f32 / 255.0,
                self.rgba[base + 2] as f32 / 255.0,
                self.rgba[base + 3] as f32 / 255.0,
            ]
        };
        let mut result = [0.0; 4];
        for (corner, weight) in [
            (texel(x0, y0), (1.0 - fx) * (1.0 - fy)),
            (texel(x0 + 1, y0), fx * (1.0 - fy)),
            (texel(x0, y0 + 1), (1.0 - fx) * fy),
            (texel(x0 + 1, y0 + 1), fx * fy),
        ] {
            for channel in 0..4 {
                result[channel] += corner[channel] * weight;
            }
        }
        result
    }
}

fn decode_data_url(data_url: &str) -> Option<DecodedTexture> {
    let encoded = data_url.split_once("base64,")?.1;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
    Some(DecodedTexture {
        width: image.width() as usize,
        height: image.height() as usize,
        rgba: image.into_raw(),
    })
}

/// The viewport never guesses the diffuse map from an unclassified slot; only
/// the `_a0` sampler or an explicit "Base color" slot qualifies.
fn base_color_texture<'a>(
    material: Option<&G1mMaterial>,
    textures: &'a HashMap<&str, &ResolvedG1tTexture>,
    cache: &'a mut HashMap<String, Option<DecodedTexture>>,
) -> Option<&'a DecodedTexture> {
    let slot = material?.texture_slots.iter().find(|slot| {
        slot.sampler.eq_ignore_ascii_case("_a0") || slot.texture_type == "Base color"
    })?;
    let resolved = textures.get(slot.name.as_str())?;
    cache
        .entry(slot.name.clone())
        .or_insert_with(|| decode_data_url(&resolved.data_url))
        .as_ref()
}

pub fn render_to_png(
    render: &BfresRenderGraph,
    materials: &[G1mMaterial],
    textures: &[ResolvedG1tTexture],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for mesh in &render.meshes {
        for position in &mesh.positions {
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min(position[axis]);
                maximum[axis] = maximum[axis].max(position[axis]);
            }
        }
    }
    if minimum[0] > maximum[0] {
        return Err("model has no renderable geometry".into());
    }
    let center = [
        (minimum[0] + maximum[0]) * 0.5,
        (minimum[1] + maximum[1]) * 0.5,
        (minimum[2] + maximum[2]) * 0.5,
    ];
    let dimensions = [
        maximum[0] - minimum[0],
        maximum[1] - minimum[1],
        maximum[2] - minimum[2],
    ];
    let fov = FOV_DEGREES.to_radians();
    let aspect = width as f32 / height.max(1) as f32;
    let half_tan = (fov * 0.5).tan();
    let distance = (dimensions[1] / (2.0 * half_tan))
        .max(dimensions[0] / (2.0 * half_tan * aspect))
        .max(dimensions[2])
        .max(0.01)
        * 1.15;
    let eye = [center[0], center[1], center[2] + distance];
    let near = (distance / 10_000.0).max(0.0001);
    let focal = 1.0 / half_tan;

    let light = normalize(LIGHT_DIRECTION);
    let mut texture_map: HashMap<&str, &ResolvedG1tTexture> = HashMap::new();
    for texture in textures {
        texture_map.entry(texture.name.as_str()).or_insert(texture);
        for alias in &texture.aliases {
            texture_map.entry(alias.as_str()).or_insert(texture);
        }
    }
    let mut decoded_cache: HashMap<String, Option<DecodedTexture>> = HashMap::new();

    // RGBA with a fully transparent background; covered pixels become opaque.
    let pixel_count = (width as usize) * (height as usize);
    let mut color = vec![0.0f32; pixel_count * 4];
    let mut depth = vec![f32::INFINITY; pixel_count];

    for mesh in &render.meshes {
        if mesh.positions.is_empty() || mesh.indices.len() < 3 {
            continue;
        }
        let texture = base_color_texture(
            materials.get(mesh.material_index as usize),
            &texture_map,
            &mut decoded_cache,
        );
        let uvs: &[[f32; 2]] = if mesh.uv0.len() == mesh.positions.len() {
            &mesh.uv0
        } else {
            &[]
        };
        let has_normals = mesh.normals.len() == mesh.positions.len();
        // Camera axes are world-aligned, so view space is a translation and a
        // depth flip: depth grows away from the camera along -Z.
        let view: Vec<[f32; 3]> = mesh
            .positions
            .iter()
            .map(|p| [p[0] - eye[0], p[1] - eye[1], eye[2] - p[2]])
            .collect();
        for triangle in mesh.indices.chunks_exact(3) {
            let [i0, i1, i2] = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            if i0 >= view.len() || i1 >= view.len() || i2 >= view.len() {
                continue;
            }
            let corners = [view[i0], view[i1], view[i2]];
            if corners.iter().any(|corner| corner[2] <= near) {
                continue;
            }
            let screen: Vec<[f32; 3]> = corners
                .iter()
                .map(|corner| {
                    let inverse_depth = 1.0 / corner[2];
                    [
                        (corner[0] * focal / aspect * inverse_depth * 0.5 + 0.5) * width as f32,
                        (0.5 - corner[1] * focal * inverse_depth * 0.5) * height as f32,
                        inverse_depth,
                    ]
                })
                .collect();
            let area = edge(&screen[0], &screen[1], &screen[2]);
            if area.abs() < 1e-8 {
                continue;
            }
            let face_normal = if has_normals {
                None
            } else {
                Some(triangle_normal(
                    &mesh.positions[i0],
                    &mesh.positions[i1],
                    &mesh.positions[i2],
                ))
            };
            let min_x = screen.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
            let max_x = screen
                .iter()
                .map(|p| p[0])
                .fold(f32::NEG_INFINITY, f32::max);
            let min_y = screen.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
            let max_y = screen
                .iter()
                .map(|p| p[1])
                .fold(f32::NEG_INFINITY, f32::max);
            let x_start = (min_x.floor().max(0.0)) as usize;
            let x_end = (max_x.ceil().min(width as f32 - 1.0)) as usize;
            let y_start = (min_y.floor().max(0.0)) as usize;
            let y_end = (max_y.ceil().min(height as f32 - 1.0)) as usize;
            if x_start > x_end || y_start > y_end {
                continue;
            }
            for y in y_start..=y_end {
                for x in x_start..=x_end {
                    let point = [x as f32 + 0.5, y as f32 + 0.5, 0.0];
                    let w0 = edge(&screen[1], &screen[2], &point) / area;
                    let w1 = edge(&screen[2], &screen[0], &point) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let inverse_depth = w0 * screen[0][2] + w1 * screen[1][2] + w2 * screen[2][2];
                    if inverse_depth <= 0.0 {
                        continue;
                    }
                    let pixel_depth = 1.0 / inverse_depth;
                    let pixel = y * width as usize + x;
                    if pixel_depth >= depth[pixel] {
                        continue;
                    }
                    // Perspective-correct weights for vertex attributes.
                    let p0 = w0 * screen[0][2] / inverse_depth;
                    let p1 = w1 * screen[1][2] / inverse_depth;
                    let p2 = w2 * screen[2][2] / inverse_depth;
                    let mut base = UNTEXTURED_COLOR;
                    if let Some(texture) = texture {
                        if !uvs.is_empty() {
                            let u = p0 * uvs[i0][0] + p1 * uvs[i1][0] + p2 * uvs[i2][0];
                            let v = p0 * uvs[i0][1] + p1 * uvs[i1][1] + p2 * uvs[i2][1];
                            let sampled = texture.sample(u, v);
                            // Alpha cutout so foliage/decal quads keep holes.
                            if sampled[3] < 0.5 {
                                continue;
                            }
                            base = [sampled[0], sampled[1], sampled[2]];
                        }
                    }
                    let normal = match face_normal {
                        Some(normal) => normal,
                        None => normalize([
                            p0 * mesh.normals[i0][0]
                                + p1 * mesh.normals[i1][0]
                                + p2 * mesh.normals[i2][0],
                            p0 * mesh.normals[i0][1]
                                + p1 * mesh.normals[i1][1]
                                + p2 * mesh.normals[i2][1],
                            p0 * mesh.normals[i0][2]
                                + p1 * mesh.normals[i1][2]
                                + p2 * mesh.normals[i2][2],
                        ]),
                    };
                    // Double-sided lambert: LM3 winding varies per mesh, so
                    // shade by the unsigned incidence angle.
                    let incidence =
                        (normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2]).abs();
                    let intensity = (AMBIENT + DIFFUSE * incidence).min(1.0);
                    depth[pixel] = pixel_depth;
                    for channel in 0..3 {
                        color[pixel * 4 + channel] = base[channel] * intensity;
                    }
                    color[pixel * 4 + 3] = 1.0;
                }
            }
        }
    }

    let mut pixels = Vec::with_capacity(pixel_count * 4);
    for value in color {
        pixels.push((value.clamp(0.0, 1.0) * 255.0).round() as u8);
    }
    let image = image::RgbaImage::from_raw(width, height, pixels)
        .ok_or("failed to assemble the rendered image")?;
    let mut encoded = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(encoded.into_inner())
}

fn edge(a: &[f32; 3], b: &[f32; 3], point: &[f32; 3]) -> f32 {
    (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0])
}

fn triangle_normal(a: &[f32; 3], b: &[f32; 3], c: &[f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    normalize([
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ])
}

fn normalize(vector: [f32; 3]) -> [f32; 3] {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length <= f32::EPSILON {
        [0.0, 1.0, 0.0]
    } else {
        [vector[0] / length, vector[1] / length, vector[2] / length]
    }
}
