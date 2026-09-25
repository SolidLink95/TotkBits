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
    let mut skipped = 0usize;
    let mut unstable = Vec::new();
    for (path, _, parsed) in parsed_corpus(limit().min(800)) {
        // Shapes with float sections encode a vertex shared by an oversize
        // primitive and a regular one twice (exact and snapped), so their
        // decoded geometry is not the builder's input space; the builder is
        // exercised on them by `oversize_triangles_build_float_sections`.
        if parsed
            .shape
            .shape
            .sections
            .iter()
            .any(|s| s.has_float_vertices())
        {
            skipped += 1;
            continue;
        }
        let geometry = obj::indexed_geometry(&parsed.shape);
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
        "bphsh fixed point: {stable} stable, {} unstable, {skipped} float-section inputs skipped",
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
            for v in 0..sec.vertex_count() {
                aabb.include_point(shape.vertex_position(sec, v));
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
        for v in 0..sec.vertex_count() {
            let w = shape.vertex_position(sec, v);
            let vert = sec.vertices.get(v).copied().unwrap_or_default();
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
    for (label, shape, geometry) in [("s1", &s1, &g2), ("s2", &s2, &g3), ("s3", &s3, &g3)] {
        let _ = geometry;
        for (i, sec) in shape.shape.shape.sections.iter().enumerate() {
            let positions: Vec<[f32; 3]> = (0..sec.vertex_count())
                .map(|v| shape.shape.shape.vertex_position(sec, v))
                .collect();
            let mut unique: Vec<[u32; 3]> = positions.iter().map(|p| p.map(f32::to_bits)).collect();
            unique.sort();
            unique.dedup();
            eprintln!(
                "  {label} section {i}: float={} prims={} verts={} unique={} offset={:?} first={:?}",
                sec.has_float_vertices(),
                sec.primitives.len(),
                sec.vertex_count(),
                unique.len(),
                sec.section_offset.map(|v| v as i32),
                positions.first()
            );
        }
    }
    let multiplicity = |g: &obj::Geometry| {
        let mut m: BTreeMap<[u32; 3], usize> = BTreeMap::new();
        for v in &g.vertices {
            *m.entry(v.map(f32::to_bits)).or_default() += 1;
        }
        m
    };
    let (m2, m3) = (multiplicity(&g2), multiplicity(&g3));
    for (key, count) in &m3 {
        let other = m2.get(key).copied().unwrap_or(0);
        if other != *count {
            eprintln!(
                "  multiplicity {:?}: g2 {other} vs g3 {count}",
                key.map(f32::from_bits)
            );
        }
    }
}

/// `BPHSH_FILE=<name> cargo test inspect_int_max_sections -- --ignored --nocapture`:
/// dumps the sections whose offset is `0x7FFFFFFF` (seen in large dungeon
/// shapes) next to their 8-bit BVH bounds and the top-level lanes pointing
/// at them, and counts how many corpus files hold such sections.
#[test]
#[ignore]
fn inspect_int_max_sections() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/_bphsh");
    if let Ok(name) = std::env::var("BPHSH_FILE") {
        let bytes = fs::read(root.join(&name)).unwrap();
        let parsed = reader::parse(&bytes).unwrap();
        let mesh = &parsed.shape.shape;
        eprintln!(
            "bit_scale16={:?} inv={:?}",
            mesh.bit_scale16, mesh.bit_scale16_inv
        );
        for (si, sec) in mesh.sections.iter().enumerate() {
            if sec.section_offset != [0x7FFF_FFFF; 3] {
                continue;
            }
            eprintln!("--- section {si}: bit_offset={:?} scale8inv={:?} prims={} verts={} bvh={} bits={:?}",
                sec.bit_offset, sec.bit_scale8_inv, sec.primitives.len(), sec.vertices.len(), sec.bvh.len(), sec.interior_primitive_bits);
            for (ni, node) in mesh.top_level_tree.iter().enumerate() {
                if node.is_leaf {
                    for lane in 0..4 {
                        if node.data[lane] as usize == si {
                            let a = node.lane_aabb(lane);
                            eprintln!(
                                "  top-level node {ni} lane {lane}: min={:?} max={:?}",
                                a.min, a.max
                            );
                        }
                    }
                }
            }
            for (bi, node) in sec.bvh.iter().enumerate() {
                for lane in 0..4 {
                    if !node.lane_valid(lane) {
                        continue;
                    }
                    let l = node.lane(lane);
                    let dec = |b: u8, axis: usize| {
                        (b as i32 + sec.bit_offset[axis] as i32) as f32 * sec.bit_scale8_inv[axis]
                    };
                    eprintln!("  bvh{bi} leaf={} lane{lane} data={} x[{:.3},{:.3}] y[{:.3},{:.3}] z[{:.3},{:.3}]",
                        node.is_leaf(), node.data[lane], dec(l[0],0), dec(l[1],0), dec(l[2],1), dec(l[3],1), dec(l[4],2), dec(l[5],2));
                }
            }
            for (pi, prim) in sec.primitives.iter().enumerate() {
                eprintln!("  prim{pi} ids={:?}", prim.ids());
            }
            for (vi, v) in sec.vertices.iter().enumerate() {
                let a = [
                    v[0] as f32 * mesh.bit_scale16_inv[0],
                    v[1] as f32 * mesh.bit_scale16_inv[1],
                    v[2] as f32 * mesh.bit_scale16_inv[2],
                ];
                let c = [
                    sec.bit_offset[0] as f32 * sec.bit_scale8_inv[0] + a[0],
                    sec.bit_offset[1] as f32 * sec.bit_scale8_inv[1] + a[1],
                    sec.bit_offset[2] as f32 * sec.bit_scale8_inv[2] + a[2],
                ];
                eprintln!(
                    "  v{vi} raw={:?} (a) no-offset={:.3?} (c) bitOffset8+raw={:.3?}",
                    v, a, c
                );
            }
        }
        return;
    }
    let mut files = 0;
    let mut hits = Vec::new();
    for path in corpus() {
        files += 1;
        let bytes = fs::read(&path).unwrap();
        let parsed = reader::parse(&bytes).unwrap();
        let n = parsed
            .shape
            .shape
            .sections
            .iter()
            .filter(|s| s.section_offset == [0x7FFF_FFFF; 3])
            .count();
        let any_huge = parsed.shape.shape.sections.iter().any(|s| {
            s.section_offset
                .iter()
                .any(|o| (*o as i32).unsigned_abs() > 1 << 24)
        });
        if n > 0 || any_huge {
            hits.push((
                file_name(&path),
                n,
                parsed.shape.shape.sections.len(),
                any_huge,
            ));
        }
    }
    eprintln!(
        "{} of {files} files hold INT_MAX/huge-offset sections",
        hits.len()
    );
    for h in &hits {
        eprintln!("  {:?}", h);
    }
}

/// Every vanilla section's decoded vertices must lie inside the top-level
/// tree lane that points at the section (within the quantization slack);
/// this is what catches a wrong vertex encoding such as the float sections.
#[test]
fn corpus_vertices_fit_their_top_level_lanes() {
    let mut checked_files = 0;
    let mut float_sections = 0;
    let mut failures = Vec::new();
    for path in corpus().into_iter().take(limit()) {
        let bytes = fs::read(&path).unwrap();
        let parsed = reader::parse(&bytes).unwrap();
        let mesh = &parsed.shape.shape;
        checked_files += 1;
        for node in mesh.top_level_tree.iter().filter(|n| n.is_leaf) {
            for lane in 0..4 {
                if !node.lane_valid(lane) {
                    continue;
                }
                let Some(section) = mesh.sections.get(node.data[lane] as usize) else {
                    continue;
                };
                if section.has_float_vertices() {
                    float_sections += 1;
                }
                let bounds = node.lane_aabb(lane);
                let slack = 0.5;
                for v in 0..section.vertex_count() {
                    let p = mesh.vertex_position(section, v);
                    let outside = (0..3).any(|axis| {
                        p[axis] < bounds.min[axis] - slack || p[axis] > bounds.max[axis] + slack
                    });
                    if outside {
                        failures.push(format!(
                            "{} section {} (float={}) vertex {v} {p:?} outside lane {:?}..{:?}",
                            file_name(&path),
                            node.data[lane],
                            section.has_float_vertices(),
                            bounds.min,
                            bounds.max
                        ));
                        break;
                    }
                }
            }
        }
    }
    eprintln!(
        "checked {checked_files} files, {float_sections} float sections, {} sections out of bounds",
        failures.len()
    );
    for failure in failures.iter().take(20) {
        eprintln!("  {failure}");
    }
    assert!(failures.is_empty());
}

/// A primitive wider than a 16-bit section (128 m at the default error)
/// must come out as a float section and survive the round trip exactly.
#[test]
fn oversize_triangles_build_float_sections() {
    let geometry = obj::Geometry {
        vertices: vec![
            [-150.123, 0.5, -150.75],
            [150.25, 0.5, -150.75],
            [0.0, 0.5, 150.5],
            [1.0, 2.0, 3.0],
            [2.0, 2.0, 3.0],
            [1.0, 3.0, 3.0],
        ],
        triangles: vec![
            obj::Triangle {
                a: 0,
                b: 1,
                c: 2,
                material: 0,
            },
            obj::Triangle {
                a: 3,
                b: 4,
                c: 5,
                material: 0,
            },
        ],
        materials: vec![obj::Material::default()],
    };
    let shape = builder::build(&geometry).expect("builds");
    let float_sections: Vec<_> = shape
        .sections
        .iter()
        .filter(|s| s.has_float_vertices())
        .collect();
    assert_eq!(
        float_sections.len(),
        1,
        "one float section for the big triangle"
    );
    assert_eq!(float_sections[0].vertex_count(), 3);
    let (materials, collision_masks) = obj::material_tables(&geometry);
    let bytes = writer::write(&BphshShape {
        shape,
        materials,
        collision_masks,
    })
    .unwrap();
    let reread = reader::parse(&bytes).unwrap();
    let back = obj::indexed_geometry(&reread.shape);
    let big: Vec<[f32; 3]> = back
        .vertices
        .iter()
        .copied()
        .filter(|v| v[0].abs() > 100.0 || v[2].abs() > 100.0)
        .collect();
    assert_eq!(big.len(), 3);
    for expected in &geometry.vertices[..3] {
        assert!(
            big.contains(expected),
            "{expected:?} kept exactly, got {big:?}"
        );
    }
    let rebuilt = build_bytes(
        &back,
        builder::BuildOptions {
            canonical: true,
            ..Default::default()
        },
    )
    .unwrap();
    let again = obj::indexed_geometry(&reader::parse(&rebuilt).unwrap().shape);
    assert_eq!(
        build_bytes(
            &again,
            builder::BuildOptions {
                canonical: true,
                ..Default::default()
            }
        )
        .unwrap(),
        rebuilt
    );
}
