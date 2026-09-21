//! Corpus tests against the vanilla `Phive/Shape/Dcc` files, decompressed
//! into `tmp/_bphsh` (`Totkbits.exe --cli decompress_dir -i <romfs>/Phive/Shape/Dcc -o tmp/_bphsh`).
//! They skip when the corpus is missing.
//!
//! What they establish:
//! - every vanilla file resaves byte for byte (reader + writer);
//! - the builder is a fixed point of its own output (canonical order);
//! - how closely the builder reproduces vanilla files from their own
//!   geometry. Exact parity there is bounded by information the file no
//!   longer holds: the section quantizer scale is derived from the source
//!   vertices *before* 16-bit quantization (off-grid by up to half a step,
//!   see `corpus_layout_report`), and the quad pairing / BVH splits break
//!   ties by source vertex index, which the file does not record.

use super::{builder, obj, reader, shape::BphshShape, writer};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn corpus() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/_bphsh");
    let Ok(entries) = fs::read_dir(&root) else {
        eprintln!("bphsh corpus missing at {}", root.display());
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "bphsh"))
        .collect();
    paths.sort();
    paths
}

fn file_name(path: &PathBuf) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

fn limit() -> usize {
    std::env::var("BPHSH_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX)
}

fn parsed_corpus(limit: usize) -> Vec<(PathBuf, Vec<u8>, reader::ParsedBphsh)> {
    corpus()
        .into_iter()
        .take(limit)
        .filter_map(|path| {
            let bytes = fs::read(&path).unwrap();
            if bytes.len() < 0x30 {
                return None;
            }
            let parsed = reader::parse(&bytes).ok()?;
            Some((path, bytes, parsed))
        })
        .collect()
}

fn build_bytes(geometry: &obj::Geometry, options: builder::BuildOptions) -> Option<Vec<u8>> {
    let shape = builder::build_with_options(geometry, options)?;
    let (materials, collision_masks) = obj::material_tables(geometry);
    writer::write(&BphshShape {
        shape,
        materials,
        collision_masks,
    })
    .ok()
}

#[test]
fn corpus_resaves_byte_identical() {
    let paths = corpus();
    let mut checked = 0usize;
    let mut parse_errors = Vec::new();
    let mut mismatches = Vec::new();
    for path in &paths {
        let bytes = fs::read(path).unwrap();
        if bytes.len() < 0x30 {
            continue;
        }
        let parsed = match reader::parse(&bytes) {
            Ok(p) => p,
            Err(e) => {
                parse_errors.push(format!("{}: {e}", file_name(path)));
                continue;
            }
        };
        assert_eq!(
            parsed.type_section,
            writer::TOTK_TYPE_SECTION,
            "{} carries a different TYPE section",
            file_name(path)
        );
        assert_eq!(&parsed.sdk_version[..], reader::TOTK_SDK_VERSION);
        let rebuilt = writer::write(&parsed.shape).unwrap();
        checked += 1;
        if rebuilt != bytes {
            let first = bytes
                .iter()
                .zip(rebuilt.iter())
                .position(|(a, b)| a != b)
                .unwrap_or(bytes.len().min(rebuilt.len()));
            mismatches.push(format!(
                "{}: {} vs {} bytes, first diff at {first:#x}",
                file_name(path),
                bytes.len(),
                rebuilt.len()
            ));
        }
    }
    eprintln!(
        "bphsh resave: {checked} checked, {} parse errors, {} mismatching",
        parse_errors.len(),
        mismatches.len()
    );
    for line in parse_errors.iter().chain(mismatches.iter()).take(10) {
        eprintln!("  {line}");
    }
    assert!(
        parse_errors.is_empty() && mismatches.is_empty(),
        "{} parse errors, {} files do not resave byte-identical",
        parse_errors.len(),
        mismatches.len()
    );
}

/// Building the geometry decoded from a built shape reproduces that shape
/// (from the second build on: the first build still sees the vanilla
/// file's off-grid quantizer domains).
#[test]
fn corpus_rebuild_is_a_fixed_point() {
    let options = builder::BuildOptions {
        canonical: true,
        ..Default::default()
    };
    let mut stable = 0usize;
    let mut unstable = Vec::new();
    let mut skipped = 0usize;
    for (path, _, parsed) in parsed_corpus(limit().min(800)) {
        let geometry = obj::indexed_geometry(&parsed.shape);
        // A few vanilla files hold junk sections (section offsets of
        // 0x7FFFFFFF, corners millions of units out): not meaningful input.
        if geometry
            .vertices
            .iter()
            .any(|v| v.iter().any(|c| !c.is_finite() || c.abs() > 1e5))
        {
            skipped += 1;
            continue;
        }
        let Some(first) = build_bytes(&geometry, options) else {
            continue;
        };
        let second_geometry = obj::indexed_geometry(&reader::parse(&first).unwrap().shape);
        let second = build_bytes(&second_geometry, options).unwrap();
        let third_geometry = obj::indexed_geometry(&reader::parse(&second).unwrap().shape);
        let third = build_bytes(&third_geometry, options).unwrap();
        if second == third {
            stable += 1;
        } else {
            unstable.push(file_name(&path));
        }
    }
    eprintln!(
        "bphsh fixed point: {stable} stable, {} unstable, {skipped} junk inputs skipped",
        unstable.len()
    );
    for line in unstable.iter().take(10) {
        eprintln!("  {line}");
    }
    assert!(
        unstable.is_empty(),
        "{} shapes are not fixed points",
        unstable.len()
    );
}

/// Order-insensitive comparison of two shapes: sections, primitive sets by
/// corner position, quantizer metadata, node counts.
fn semantically_equal(
    orig: &super::shape::MeshShape,
    new: &super::shape::MeshShape,
) -> Result<(), String> {
    if orig.sections.len() != new.sections.len() {
        return Err("section count".into());
    }
    if orig.top_level_tree.len() != new.top_level_tree.len() {
        return Err("tree node count".into());
    }
    let describe = |sec: &super::shape::GeometrySection| {
        let key = |id: u8| {
            let v = sec.vertices[id as usize];
            [
                (v[0] as i32).wrapping_add(sec.section_offset[0] as i32),
                (v[1] as i32).wrapping_add(sec.section_offset[1] as i32),
                (v[2] as i32).wrapping_add(sec.section_offset[2] as i32),
            ]
        };
        let mut prims: Vec<(Vec<[i32; 3]>, bool)> = sec
            .primitives
            .iter()
            .map(|prim| {
                let ids = prim.ids();
                let mut corners: Vec<[i32; 3]> = ids
                    .iter()
                    .take(if prim.is_triangle() { 3 } else { 4 })
                    .map(|id| key(*id))
                    .collect();
                corners.sort();
                (corners, prim.is_triangle())
            })
            .collect();
        prims.sort();
        let mut verts: Vec<[i32; 3]> = (0..sec.vertices.len() as u8).map(key).collect();
        verts.sort();
        (prims, verts)
    };
    for (i, (a, b)) in orig.sections.iter().zip(new.sections.iter()).enumerate() {
        let (pa, va) = describe(a);
        let (pb, vb) = describe(b);
        if pa != pb {
            return Err(format!("section {i}: primitive sets"));
        }
        if va != vb {
            return Err(format!("section {i}: vertex sets"));
        }
        if a.bvh.len() != b.bvh.len() {
            return Err(format!("section {i}: bvh node count"));
        }
        if a.section_offset != b.section_offset || a.vertex_buffer_size != b.vertex_buffer_size {
            return Err(format!("section {i}: section offset / pad"));
        }
        if a.bit_scale8_inv != b.bit_scale8_inv || a.bit_offset != b.bit_offset {
            return Err(format!("section {i}: quantizer scale"));
        }
    }
    Ok(())
}

/// Rebuilds every vanilla shape from its own geometry and reports how the
/// results compare: byte-identical, same sections and primitives, or
/// different. Informational: see the module comment for why exact parity
/// is bounded.
#[test]
fn corpus_rebuild_report() {
    let mut identical = 0usize;
    let mut structure = 0usize;
    let mut semantic = 0usize;
    let mut classes: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = 0usize;
    for (_, bytes, parsed) in parsed_corpus(limit()) {
        let geometry = obj::indexed_geometry(&parsed.shape);
        let Some(built) = builder::build(&geometry) else {
            continue;
        };
        total += 1;
        let (materials, collision_masks) = obj::material_tables(&geometry);
        let rebuilt = writer::write(&BphshShape {
            shape: built.clone(),
            materials,
            collision_masks,
        })
        .unwrap();
        let orig = &parsed.shape.shape;
        if rebuilt == bytes {
            identical += 1;
        }
        let same_structure = orig.sections.len() == built.sections.len()
            && orig.top_level_tree.len() == built.top_level_tree.len()
            && orig
                .sections
                .iter()
                .zip(built.sections.iter())
                .all(|(a, b)| a.primitives.len() == b.primitives.len());
        if same_structure {
            structure += 1;
        }
        match semantically_equal(orig, &built) {
            Ok(()) => semantic += 1,
            Err(reason) => {
                let class = reason.split(": ").last().unwrap().to_string();
                *classes.entry(class).or_default() += 1;
            }
        }
    }
    eprintln!(
        "bphsh rebuild of {total} vanilla shapes: {identical} byte-identical, {structure} same section structure, {semantic} same primitive sets and metadata"
    );
    for (class, count) in &classes {
        eprintln!("  {count:6} differ in {class}");
    }
}

/// Layout facts the builder relies on, checked on every vanilla file.
#[test]
fn corpus_layout_report() {
    let mut off_grid_sections = 0usize;
    let mut sections = 0usize;
    for (_, _, parsed) in parsed_corpus(limit()) {
        let shape = &parsed.shape.shape;
        assert_eq!(shape.shape_type, super::shape::MeshShape::TYPE_MESH);
        assert_eq!(
            shape.dispatch_type,
            super::shape::MeshShape::DISPATCH_COMPOSITE
        );
        assert_eq!(shape.flags, super::shape::MeshShape::FLAGS_DEFAULT);
        assert_eq!(shape.shape_tag_codec_info, 0xFFFF_FFFF);
        assert!(shape.top_level_tree_is_compact);
        assert!(shape.primitive_mapping.is_none());
        let last = shape.sections.len() - 1;
        for (i, sec) in shape.sections.iter().enumerate() {
            sections += 1;
            let pad = sec.vertex_buffer_size - sec.vertices.len() * 6;
            assert!(
                pad == 0 || (pad == 2 && i == last),
                "vertex pad {pad} in section {i}"
            );
            assert_eq!(
                sec.interior_primitive_bits.len(),
                (sec.primitives.len() * 2).div_ceil(8)
            );
            let mut aabb = super::shape::Aabb::empty();
            for v in &sec.vertices {
                aabb.include_point(shape.unpack_vertex(sec, *v));
            }
            let off_grid = (0..3).any(|axis| {
                let extent = aabb.max[axis] - aabb.min[axis];
                let span = 255.0 * sec.bit_scale8_inv[axis];
                let implied = span / (1.0 + 2.0 / 255.0) - extent;
                (implied - 0.002).abs() > 1e-4 + extent * 2e-6
            });
            if off_grid {
                off_grid_sections += 1;
            }
        }
    }
    eprintln!(
        "bphsh layout: {sections} sections, {off_grid_sections} with a quantizer domain from off-grid source vertices"
    );
}

fn describe(shape: &super::shape::MeshShape) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    writeln!(
        s,
        "keyBits={} tags={:?}",
        shape.num_shape_key_bits,
        shape
            .shape_tag_table
            .iter()
            .map(|t| (t.mesh_primitive_key, t.shape_tag))
            .collect::<Vec<_>>()
    )
    .unwrap();
    for (i, n) in shape.top_level_tree.iter().enumerate() {
        writeln!(
            s,
            "node{i} leaf={} data={:?} lane0 x[{:.4},{:.4}] y[{:.4},{:.4}] z[{:.4},{:.4}]",
            n.is_leaf,
            n.data.iter().map(|d| format!("{d:#x}")).collect::<Vec<_>>(),
            n.lx[0],
            n.hx[0],
            n.ly[0],
            n.hy[0],
            n.lz[0],
            n.hz[0]
        )
        .unwrap();
    }
    for (i, sec) in shape.sections.iter().enumerate() {
        writeln!(
            s,
            "section{i}: prims={} verts={} bvh={} bits={:?} vbsize={} offset={:?} scale8inv={:?} bitOffset={:?}",
            sec.primitives.len(),
            sec.vertices.len(),
            sec.bvh.len(),
            sec.interior_primitive_bits,
            sec.vertex_buffer_size,
            sec.section_offset.map(|v| v as i32),
            sec.bit_scale8_inv,
            sec.bit_offset
        )
        .unwrap();
        for (p, prim) in sec.primitives.iter().enumerate() {
            writeln!(s, "   prim{p}: {:?}", prim.ids()).unwrap();
        }
        for (v, vert) in sec.vertices.iter().enumerate() {
            let w = shape.unpack_vertex(sec, *vert);
            writeln!(
                s,
                "   v{v}: {vert:?} = [{:.4}, {:.4}, {:.4}]",
                w[0], w[1], w[2]
            )
            .unwrap();
        }
        for (b, node) in sec.bvh.iter().enumerate() {
            writeln!(
                s,
                "   bvh{b}: leaf={} data={:?} lx={:?} hx={:?} ly={:?} hy={:?} lz={:?} hz={:?}",
                node.is_leaf(),
                node.data,
                node.lx,
                node.hx,
                node.ly,
                node.hy,
                node.lz,
                node.hz
            )
            .unwrap();
        }
    }
    s
}

/// `BPHSH_FILE=<name> cargo test dump_one_file -- --nocapture` prints a
/// vanilla shape next to its rebuild.
#[test]
fn dump_one_file() {
    let Ok(name) = std::env::var("BPHSH_FILE") else {
        return;
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tmp/_bphsh")
        .join(&name);
    let bytes = fs::read(&path).unwrap();
    let parsed = reader::parse(&bytes).unwrap();
    let geometry = obj::indexed_geometry(&parsed.shape);
    let built = builder::build(&geometry).unwrap();
    eprintln!("===== ORIGINAL {name}\n{}", describe(&parsed.shape.shape));
    eprintln!("===== REBUILT\n{}", describe(&built));
}

#[test]
fn obj_round_trip_preserves_geometry_and_materials() {
    let geometry = obj::Geometry {
        vertices: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
        ],
        triangles: vec![
            obj::Triangle {
                a: 0,
                b: 1,
                c: 2,
                material: 0,
            },
            obj::Triangle {
                a: 0,
                b: 2,
                c: 3,
                material: 0,
            },
            obj::Triangle {
                a: 0,
                b: 4,
                c: 1,
                material: 1,
            },
        ],
        materials: vec![
            obj::Material {
                material_id: super::materials::material_id("Stone").unwrap(),
                flags: super::materials::flags_from_names(["NoClimb"]).unwrap(),
                collision_mask: u64::MAX,
            },
            obj::Material {
                material_id: super::materials::material_id("Wood").unwrap(),
                flags: 0,
                collision_mask: super::materials::mask_from_disabled_names(["Player"]).unwrap(),
            },
        ],
    };
    let (text, json) = obj::to_obj(&geometry).unwrap();
    assert!(text.contains("usemtl Stone00"));
    assert!(text.contains("usemtl Wood00"));
    assert!(json.contains("\"NoClimb\""));
    assert!(json.contains("\"Player\""));
    let back = obj::from_obj(&text, Some(&json)).unwrap();
    assert_eq!(back.vertices, geometry.vertices);
    assert_eq!(back.triangles, geometry.triangles);
    assert_eq!(back.materials, geometry.materials);

    let built = builder::build_with_options(
        &back,
        builder::BuildOptions {
            weld: true,
            canonical: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(built.sections.len(), 1);
    assert_eq!(
        built.sections[0].primitives.len(),
        2,
        "one quad and one triangle"
    );
    let (materials, collision_masks) = obj::material_tables(&back);
    let bytes = writer::write(&BphshShape {
        shape: built,
        materials,
        collision_masks,
    })
    .unwrap();
    let reparsed = reader::parse(&bytes).unwrap();
    assert_eq!(reparsed.shape.materials.len(), 2);
    assert_eq!(
        reparsed.shape.materials[0].flags,
        super::materials::flags_from_names(["NoClimb"]).unwrap()
    );
    assert_eq!(writer::write(&reparsed.shape).unwrap(), bytes);
}

/// `BPHSH_FIXED_POINT_FILE=<name>`: explains why a shape is not a fixed point.
#[test]
fn explain_fixed_point_failure() {
    let Ok(name) = std::env::var("BPHSH_FIXED_POINT_FILE") else {
        return;
    };
    let options = builder::BuildOptions {
        canonical: true,
        ..Default::default()
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tmp/_bphsh")
        .join(&name);
    let parsed = reader::parse(&fs::read(&path).unwrap()).unwrap();
    let g1 = obj::indexed_geometry(&parsed.shape);
    let b1 = build_bytes(&g1, options).unwrap();
    let s1 = reader::parse(&b1).unwrap();
    let g2 = obj::indexed_geometry(&s1.shape);
    let b2 = build_bytes(&g2, options).unwrap();
    let s2 = reader::parse(&b2).unwrap();
    let g3 = obj::indexed_geometry(&s2.shape);
    let b3 = build_bytes(&g3, options).unwrap();
    let s3 = reader::parse(&b3).unwrap();
    eprintln!(
        "geometry: g2 {} verts {} tris, g3 {} verts {} tris; shapes: s2 {} sections, s3 {} sections",
        g2.vertices.len(),
        g2.triangles.len(),
        g3.vertices.len(),
        g3.triangles.len(),
        s2.shape.shape.sections.len(),
        s3.shape.shape.sections.len()
    );
    let c2 = builder::canonicalize(&g2);
    let c3 = builder::canonicalize(&g3);
    eprintln!(
        "canonical geometry equal: vertices {} triangles {}",
        c2.vertices == c3.vertices,
        c2.triangles == c3.triangles
    );
    if c2.vertices != c3.vertices {
        let extra2: Vec<_> = c2
            .vertices
            .iter()
            .filter(|v| !c3.vertices.contains(v))
            .take(5)
            .collect();
        let extra3: Vec<_> = c3
            .vertices
            .iter()
            .filter(|v| !c2.vertices.contains(v))
            .take(5)
            .collect();
        eprintln!("only in g2: {extra2:?}\nonly in g3: {extra3:?}");
    }
    if c2.triangles != c3.triangles {
        let pos = |g: &obj::Geometry, t: &obj::Triangle| {
            [
                g.vertices[t.a as usize],
                g.vertices[t.b as usize],
                g.vertices[t.c as usize],
            ]
        };
        let t2: Vec<_> = c2.triangles.iter().map(|t| pos(&c2, t)).collect();
        let t3: Vec<_> = c3.triangles.iter().map(|t| pos(&c3, t)).collect();
        let only2: Vec<_> = t2.iter().filter(|t| !t3.contains(t)).take(4).collect();
        let only3: Vec<_> = t3.iter().filter(|t| !t2.contains(t)).take(4).collect();
        eprintln!("triangles only in g2: {only2:?}\ntriangles only in g3: {only3:?}");
    }
    for (i, (a, b)) in s2
        .shape
        .shape
        .sections
        .iter()
        .zip(s3.shape.shape.sections.iter())
        .enumerate()
    {
        if a != b {
            eprintln!(
                "first differing section {i}: prims {} vs {}, verts {} vs {}, dup positions in s2 section: {}",
                a.primitives.len(),
                b.primitives.len(),
                a.vertices.len(),
                b.vertices.len(),
                {
                    let mut v = a.vertices.clone();
                    v.sort();
                    v.dedup();
                    a.vertices.len() - v.len()
                }
            );
            break;
        }
    }
}
