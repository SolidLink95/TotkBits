//! Builds a [`MeshShape`] from a triangle soup: a port of Haok's
//! `hknpMeshShape` builder as reimplemented by PhieConverter
//! (`MeshShapeBuilder.cpp`), with the rules TOTK's SDK 2022 build adds on
//! top (see the corpus tests): interior bits per triangle, the 2-byte pad on
//! the last vertex buffer, and the section split of small shapes.
//!
//! The pipeline, in order:
//! 1. weld duplicate vertices (sweep on the x ordinate, radix sorted);
//! 2. pair triangles into quads by edge quality (quad-dominant geometry);
//! 3. build a 4-wide BVH over primitive centres (SAH splits);
//! 4. cut the BVH into sections of at most 256 vertices / 256 primitives
//!    and 8-bit quantized AABB trees;
//! 5. quantize vertices to 16 bits per section, refit, mark interior
//!    triangles, sort primitives by shape tag;
//! 6. assemble the shape tag table and the top-level SIMD tree.
//!
//! Floating point evaluation order follows the C++ line by line: the output
//! is compared byte for byte against the vanilla files.

use super::obj::{Geometry, Triangle};
use super::shape::*;

/// Maximum quantization error of a vertex, in world units (Havok default).
pub const DEFAULT_MAX_VERTEX_ERROR: f32 = 1e-3;

// ---------------------------------------------------------------------------
// Quantization helpers
// ---------------------------------------------------------------------------

fn calc_bit_scale(max_vertex_error: f32) -> f32 {
    let step = max_vertex_error * 2.0;
    let log2_step = step.log2().floor();
    (-log2_step).exp2()
}

#[derive(Clone, Copy)]
struct VertexConversion {
    bit_scale16: [f32; 4],
    bit_scale16_inv: [f32; 4],
}

impl VertexConversion {
    fn new(max_vertex_error: f32) -> Self {
        let scale = calc_bit_scale(max_vertex_error);
        Self {
            bit_scale16: [scale, scale, scale, 0.0],
            bit_scale16_inv: [1.0 / scale, 1.0 / scale, 1.0 / scale, 0.0],
        }
    }
    fn max_section_extent(max_vertex_error: f32) -> f32 {
        let inv = 1.0 / calc_bit_scale(max_vertex_error);
        (65536.0 - 2.0) * inv
    }
    /// The largest extent a 16-bit section of this conversion can hold.
    fn max_extent(&self) -> f32 {
        (65536.0 - 2.0) * self.bit_scale16_inv[0]
    }
    fn section_offset(&self, domain: &Aabb) -> [u32; 3] {
        let mut out = [0u32; 3];
        for i in 0..3 {
            let scaled = domain.min[i] * self.bit_scale16[i];
            out[i] = scaled.round() as i32 as u32;
        }
        out
    }
    fn pack(&self, v: [f32; 3], offset: [u32; 3]) -> [u16; 3] {
        let mut out = [0u16; 3];
        for i in 0..3 {
            let scaled = v[i] * self.bit_scale16[i];
            let q = (scaled.round() as i32).wrapping_sub(offset[i] as i32);
            out[i] = q.clamp(0, 65535) as u16;
        }
        out
    }
    fn unpack(&self, packed: [u16; 3], offset: [u32; 3]) -> [f32; 3] {
        let mut out = [0f32; 3];
        for i in 0..3 {
            let value = (packed[i] as i32).wrapping_add(offset[i] as i32);
            out[i] = value as f32 * self.bit_scale16_inv[i];
        }
        out
    }
}

/// Quantizes AABBs of a section domain to 8 bits per bound.
struct Aabb8Quantizer {
    bit_scale_inv: [f32; 3],
    bit_offset: [i32; 3],
    bit_scale: [f32; 3],
}

impl Aabb8Quantizer {
    fn new(domain: &Aabb) -> Self {
        let mut expanded = *domain;
        for i in 0..3 {
            if expanded.min[i] == expanded.max[i] {
                expanded.min[i] -= f32::EPSILON;
                expanded.max[i] += f32::EPSILON;
            }
        }
        for i in 0..3 {
            let extent = expanded.max[i] - expanded.min[i];
            let mut expand = extent / 255.0;
            let min_abs = expanded.min[i].abs() / 255.0;
            expand = expand.max(min_abs);
            expanded.max[i] += expand;
            expanded.min[i] -= expand;
        }
        let mut q = Self {
            bit_scale_inv: [0.0; 3],
            bit_offset: [0; 3],
            bit_scale: [0.0; 3],
        };
        for i in 0..3 {
            let span = expanded.max[i] - expanded.min[i];
            q.bit_scale[i] = 255.0 / span;
            let range_min = (expanded.min[i] * q.bit_scale[i]).round();
            q.bit_offset[i] = range_min as i32;
            q.bit_scale_inv[i] = 1.0 / q.bit_scale[i];
        }
        q
    }

    fn quantize_min(&self, value: f32, axis: usize) -> i32 {
        let scaled = self.bit_scale[axis] * value;
        let rounded = if scaled < 0.0 {
            -(-scaled).ceil()
        } else {
            scaled
        };
        let compressed = rounded as i32;
        let mut result = compressed - self.bit_offset[axis];
        let restored = compressed as f32 * self.bit_scale_inv[axis];
        if restored > value {
            result -= 1;
        }
        result
    }

    fn quantize_max(&self, value: f32, axis: usize) -> i32 {
        let scaled = self.bit_scale[axis] * value;
        let rounded = if scaled > 0.0 { scaled.ceil() } else { scaled };
        let compressed = rounded as i32;
        let mut result = compressed - self.bit_offset[axis];
        let restored = compressed as f32 * self.bit_scale_inv[axis];
        if restored < value {
            result += 1;
        }
        result
    }

    fn convert(&self, aabb: &Aabb) -> [u8; 6] {
        let clamp = |v: i32| v.clamp(0, 255) as u8;
        [
            clamp(self.quantize_min(aabb.min[0], 0)),
            clamp(self.quantize_max(aabb.max[0], 0)),
            clamp(self.quantize_min(aabb.min[1], 1)),
            clamp(self.quantize_max(aabb.max[1], 1)),
            clamp(self.quantize_min(aabb.min[2], 2)),
            clamp(self.quantize_max(aabb.max[2], 2)),
        ]
    }
}

/// Makes `node` a leaf or an internal node by arranging its lanes so that
/// `data[2] > data[3]` holds exactly for leaves.
fn configure_leaf_or_internal(node: &mut Aabb8TreeNode, target_leaf: bool) {
    if node.is_leaf() == target_leaf {
        return;
    }
    if node.lane_valid(2) && node.lane_valid(3) {
        node.swap_lanes(2, 3);
    } else if node.lane_valid(2) {
        if target_leaf {
            if node.data[2] == 0 {
                node.swap_lanes(1, 2);
            }
            node.data[3] = node.data[2] - 1;
        } else {
            node.data[3] = node.data[2];
        }
    } else if target_leaf {
        node.data[2] = 1;
    }
}

// ---------------------------------------------------------------------------
// Havok sorts (their exact tie behaviour matters)
// ---------------------------------------------------------------------------

/// Havok's `hkAlgorithm::quickSort` (Hoare partition, middle pivot).
fn quick_sort<T: Copy, F: Fn(&T, &T) -> bool>(arr: &mut [T], less: &F) {
    if arr.len() > 1 {
        quick_sort_recursive(arr, 0, arr.len() as isize - 1, less);
    }
}

fn quick_sort_recursive<T: Copy, F: Fn(&T, &T) -> bool>(
    arr: &mut [T],
    mut d: isize,
    h: isize,
    less: &F,
) {
    loop {
        let mut i = h;
        let mut j = d;
        let pivot = arr[((d + h) >> 1) as usize];
        loop {
            while less(&arr[j as usize], &pivot) {
                j += 1;
            }
            while less(&pivot, &arr[i as usize]) {
                i -= 1;
            }
            if i >= j {
                if i != j {
                    arr.swap(i as usize, j as usize);
                }
                i -= 1;
                j += 1;
            }
            if j > i {
                break;
            }
        }
        if d < i {
            quick_sort_recursive(arr, d, i, less);
        }
        if j < h {
            d = j;
            continue;
        }
        break;
    }
}

#[derive(Clone, Copy, Default)]
struct RadixEntry {
    key: u32,
    index: u32,
}

/// Havok's explicit-stack quicksort used below the radix threshold.
fn sort_quick_stack(data: &mut [RadixEntry]) {
    let n = data.len();
    if n <= 1 {
        return;
    }
    let mut lb_stack = [0isize; 64];
    let mut ub_stack = [0isize; 64];
    lb_stack[0] = 0;
    ub_stack[0] = n as isize - 1;
    let mut stack_pos = 0isize;
    while stack_pos >= 0 {
        let mut lb = lb_stack[stack_pos as usize];
        let mut ub = ub_stack[stack_pos as usize];
        stack_pos -= 1;
        loop {
            let mut j = lb;
            let mut i = ub;
            let pivot = data[(lb + ((ub - lb) >> 1)) as usize];
            loop {
                while data[j as usize].key < pivot.key {
                    j += 1;
                }
                while pivot.key < data[i as usize].key {
                    i -= 1;
                }
                if i >= j {
                    if i != j {
                        data.swap(i as usize, j as usize);
                    }
                    i -= 1;
                    j += 1;
                }
                if j > i {
                    break;
                }
            }
            if lb < i {
                if j < ub {
                    stack_pos += 1;
                    if (ub - j) > (i - lb) {
                        lb_stack[stack_pos as usize] = j;
                        ub_stack[stack_pos as usize] = ub;
                        ub = i;
                    } else {
                        lb_stack[stack_pos as usize] = lb;
                        ub_stack[stack_pos as usize] = i;
                        lb = j;
                    }
                    continue;
                } else {
                    ub = i;
                    continue;
                }
            } else if j < ub {
                lb = j;
                continue;
            }
            break;
        }
    }
}

fn entry_byte(e: &RadixEntry, offset: usize) -> usize {
    // Little-endian bytes of the key (offsets 0..4 select the key bytes).
    ((e.key >> (8 * offset)) & 0xFF) as usize
}

/// Havok's LSB-to-MSB radix sort; `output_in_buffer` selects the final home.
fn sort_lsb_to_msb(
    data: &mut [RadixEntry],
    buffer: &mut [RadixEntry],
    key_size: usize,
    lsb_offset: usize,
    output_buffer_index: usize,
) {
    let n = data.len();
    let mut hist = [0u32; 256];
    let mut scatter = [0u32; 256];
    // `source`/`dest` alternate between data and buffer; track with a flag.
    let mut source_is_data = true;
    let mut off0 = lsb_offset;
    for e in data.iter() {
        hist[entry_byte(e, off0)] += 1;
    }
    for _ in 0..key_size.saturating_sub(1) {
        let off1 = off0 + 1;
        if hist[0] != n as u32 && hist[255] != n as u32 {
            let mut sum = 0u32;
            for b in 0..256 {
                scatter[b] = sum;
                sum += hist[b];
            }
            hist = [0; 256];
            let (src, dst): (&[RadixEntry], &mut [RadixEntry]) = if source_is_data {
                (&*data, &mut *buffer)
            } else {
                (&*buffer, &mut *data)
            };
            for e in src.iter() {
                let t0 = entry_byte(e, off0);
                let t1 = entry_byte(e, off1);
                dst[scatter[t0] as usize] = *e;
                scatter[t0] += 1;
                hist[t1] += 1;
            }
            source_is_data = !source_is_data;
        } else {
            hist = [0; 256];
            let src: &[RadixEntry] = if source_is_data { &*data } else { &*buffer };
            for e in src.iter() {
                hist[entry_byte(e, off1)] += 1;
            }
        }
        off0 = off1;
    }
    if hist[0] != n as u32 && hist[255] != n as u32 {
        let mut sum = 0u32;
        for b in 0..256 {
            scatter[b] = sum;
            sum += hist[b];
        }
        let (src, dst): (&[RadixEntry], &mut [RadixEntry]) = if source_is_data {
            (&*data, &mut *buffer)
        } else {
            (&*buffer, &mut *data)
        };
        for e in src.iter() {
            let t0 = entry_byte(e, off0);
            dst[scatter[t0] as usize] = *e;
            scatter[t0] += 1;
        }
        source_is_data = !source_is_data;
    }
    // `dest` is where the next pass would write: the opposite of source.
    let dest_is_data = !source_is_data;
    if dest_is_data ^ (output_buffer_index == 1) {
        // Copy source into dest.
        if source_is_data {
            buffer[..n].copy_from_slice(&data[..n]);
        } else {
            data[..n].copy_from_slice(&buffer[..n]);
        }
    }
}

/// Havok's `hkRadixSort::sort32` dispatcher.
fn sort_radix(data: &mut [RadixEntry], buffer: &mut [RadixEntry]) {
    const QUICK_SORT_THRESHOLD: usize = 32;
    const SMALL_ARRAY_THRESHOLD: usize = 8192;
    const OPTIMAL_LSB2HSB_SIZE: usize = 1024;
    const SORT_KEY_SIZE: usize = 4;
    let n = data.len();
    if n <= QUICK_SORT_THRESHOLD {
        sort_quick_stack(data);
        return;
    }
    if n <= SMALL_ARRAY_THRESHOLD {
        sort_lsb_to_msb(data, buffer, SORT_KEY_SIZE, 0, 0);
        return;
    }
    let mut value_or = 0u32;
    let mut value_and = !0u32;
    for e in data.iter() {
        value_or |= e.key;
        value_and &= e.key;
    }
    let leading_zeros = value_or.leading_zeros() as usize;
    let leading_ones = (!value_and).leading_zeros() as usize;
    let num_leading_zeros = leading_zeros.max(leading_ones);
    let shift = 24usize.saturating_sub(num_leading_zeros);
    let shift = if num_leading_zeros > 24 { 0 } else { shift };
    let mut hist = [0u32; 257];
    for e in data.iter() {
        hist[((e.key >> shift) & 0xFF) as usize] += 1;
    }
    let mut hsum = [0u32; 257];
    let mut running = 0u32;
    for i in 0..256 {
        hsum[i] = running;
        running += hist[i];
    }
    hsum[256] = running;
    let mut scatter = hsum;
    for e in data.iter() {
        let k = ((e.key >> shift) & 0xFF) as usize;
        buffer[scatter[k] as usize] = *e;
        scatter[k] += 1;
    }
    let split_threshold = 1usize << (num_leading_zeros & 7);
    let mut work: Vec<(usize, usize)> = Vec::new();
    let mut stack: Vec<(usize, usize)> = vec![(0, 256)];
    while let Some((mut s, mut e)) = stack.pop() {
        loop {
            let num = (hsum[e] - hsum[s]) as usize;
            if num == 0 {
                break;
            }
            let span = e - s;
            if (num <= OPTIMAL_LSB2HSB_SIZE && (span > 4 || span <= split_threshold)) || span == 1 {
                work.push((s, e));
                break;
            }
            let mid = (s + e) >> 1;
            let num_a = (hsum[mid] - hsum[s]) as usize;
            let num_b = num - num_a;
            if num_a > num_b {
                stack.push((s, mid));
                s = mid;
            } else {
                stack.push((mid, e));
                e = mid;
            }
        }
    }
    for (ws, we) in work {
        let start = hsum[ws] as usize;
        let num = (hsum[we] - hsum[ws]) as usize;
        if num > QUICK_SORT_THRESHOLD {
            let mut key_size = SORT_KEY_SIZE - (num_leading_zeros >> 3);
            let nlz_bits = num_leading_zeros & 7;
            let span = we - ws;
            if span <= (1 << nlz_bits) {
                key_size -= 1;
            }
            // source = buffer[start..], dest = data[start..], output index 1
            sort_lsb_to_msb(
                &mut buffer[start..start + num],
                &mut data[start..start + num],
                key_size,
                0,
                1,
            );
        } else {
            data[start..start + num].copy_from_slice(&buffer[start..start + num]);
            if num >= 2 {
                sort_quick_stack(&mut data[start..start + num]);
            }
        }
    }
}

fn float_to_ordered_uint(f: f32) -> u32 {
    let ui = f.to_bits();
    ((((ui as i32) >> 31) as u32) | 0x8000_0000) ^ ui
}

// ---------------------------------------------------------------------------
// Welding
// ---------------------------------------------------------------------------

struct WeldedGeometry {
    vertices: Vec<[f32; 3]>,
    triangles: Vec<Triangle>,
}

fn weld_duplicate_vertices(geometry: &Geometry) -> WeldedGeometry {
    let num_verts = geometry.vertices.len();
    let mut triangles = geometry.triangles.clone();
    if num_verts == 0 {
        return WeldedGeometry {
            vertices: Vec::new(),
            triangles,
        };
    }
    // Sweep AABBs: min/max of the ordered x ordinate, key = vertex index.
    let mut min_x = vec![0u32; num_verts];
    let mut max_x = vec![0u32; num_verts];
    for (i, v) in geometry.vertices.iter().enumerate() {
        min_x[i] = float_to_ordered_uint(v[0]) >> 1;
        max_x[i] = (float_to_ordered_uint(v[0]) >> 1) + 1;
    }
    let mut sort_arr: Vec<RadixEntry> = (0..num_verts)
        .map(|i| RadixEntry {
            key: min_x[i],
            index: i as u32,
        })
        .collect();
    let mut sort_buf = vec![RadixEntry::default(); num_verts];
    sort_radix(&mut sort_arr, &mut sort_buf);

    // Sorted AABB list: (min, max, key) where key is the vertex index or
    // u32::MAX once welded away.
    let mut sorted_min: Vec<u32> = sort_arr.iter().map(|e| min_x[e.index as usize]).collect();
    let mut sorted_max: Vec<u32> = sort_arr.iter().map(|e| max_x[e.index as usize]).collect();
    let mut keys: Vec<u32> = sort_arr.iter().map(|e| e.index).collect();
    let _ = (&mut sorted_min, &mut sorted_max);

    let mut remap = vec![-1i32; num_verts];
    let mut unique: Vec<[f32; 3]> = Vec::with_capacity(num_verts);
    for current in 0..num_verts {
        let current_key = keys[current];
        if current_key == u32::MAX {
            continue;
        }
        let current_pos = geometry.vertices[current_key as usize];
        remap[current_key as usize] = unique.len() as i32;
        unique.push(current_pos);
        let mut potential = current + 1;
        while potential < num_verts && sorted_min[potential] <= sorted_max[current] {
            let potential_key = keys[potential];
            if potential_key != u32::MAX {
                let p = geometry.vertices[potential_key as usize];
                let dx = current_pos[0] - p[0];
                let dy = current_pos[1] - p[1];
                let dz = current_pos[2] - p[2];
                if dx * dx + dy * dy + dz * dz <= 0.0 {
                    remap[potential_key as usize] = remap[current_key as usize];
                    keys[potential] = u32::MAX;
                }
            }
            potential += 1;
        }
    }
    for t in triangles.iter_mut() {
        t.a = remap[t.a as usize] as u32;
        t.b = remap[t.b as usize] as u32;
        t.c = remap[t.c as usize] as u32;
    }
    triangles.retain(|t| t.a != t.b && t.a != t.c && t.b != t.c);
    WeldedGeometry {
        vertices: unique,
        triangles,
    }
}

// ---------------------------------------------------------------------------
// Quad-dominant geometry
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct MeshPrimitive {
    verts: [u32; 4],
    shape_tag: u16,
    is_quad: bool,
    /// Interior flag of the first triangle `(a, b, c)` and of the second
    /// `(a, c, d)` (equal to the first for a lone triangle).
    interior: [bool; 2],
}

#[derive(Clone, Copy, Default)]
struct ConnEdge {
    triangle: i32,
    start: i32,
}

struct Connectivity {
    /// Per triangle: the neighbour across edge slot `i` (edge from corner
    /// `i` to corner `(i + 1) % 3`), when any.
    links: Vec<[Option<ConnEdge>; 3]>,
}

fn tri_vert(t: &Triangle, v: usize) -> u32 {
    match v {
        0 => t.a,
        1 => t.b,
        _ => t.c,
    }
}

fn create_quad_dominant_geometry(geometry: &WeldedGeometry, max_extent: f32) -> Vec<MeshPrimitive> {
    let tris = &geometry.triangles;
    let vt = &geometry.vertices;
    let num_tris = tris.len();
    let num_verts = vt.len();
    let vertex = |t: usize, v: usize| tri_vert(&tris[t], v) as usize;

    // Vertex cardinality and first edge.
    let mut cardinality = vec![0usize; num_verts];
    for t in tris {
        if t.a != t.b && t.b != t.c && t.c != t.a {
            cardinality[t.a as usize] += 1;
            cardinality[t.b as usize] += 1;
            cardinality[t.c as usize] += 1;
        }
    }
    let mut first_edge = vec![0usize; num_verts];
    let mut num_edges = 0usize;
    for v in 0..num_verts {
        first_edge[v] = if cardinality[v] != 0 { num_edges } else { 0 };
        num_edges += cardinality[v];
    }
    let mut edges = vec![ConnEdge::default(); num_edges];
    let mut conn = Connectivity {
        links: vec![[None; 3]; num_tris],
    };
    let mut counters = vec![0usize; num_verts];
    for t in 0..num_tris {
        let tri = &tris[t];
        if !(tri.a != tri.b && tri.b != tri.c && tri.c != tri.a) {
            continue;
        }
        let verts = [tri.a as usize, tri.b as usize, tri.c as usize];
        let mut i = 2usize;
        for j in 0..3 {
            let vi = verts[i];
            let vj = verts[j];
            let idx = first_edge[vi] + counters[vi];
            counters[vi] += 1;
            edges[idx] = ConnEdge {
                triangle: t as i32,
                start: i as i32,
            };
            let count = counters[vj];
            for k in 0..count {
                let other = edges[first_edge[vj] + k];
                let other_end = vertex(other.triangle as usize, ((other.start + 1) % 3) as usize);
                if other_end == vi {
                    conn.links[t][i] = Some(other);
                    conn.links[other.triangle as usize][other.start as usize] = Some(edges[idx]);
                    break;
                }
            }
            i = j;
        }
    }

    // Reorder vertex rings (naked edge first, then fan order); only the
    // resulting edge order matters for what follows.
    for v in 0..num_verts {
        let card = cardinality[v];
        if card == 0 {
            continue;
        }
        let first = first_edge[v];
        let mut naked_idx: isize = -1;
        let mut num_naked = 0usize;
        for k in 0..card {
            let e = edges[first + k];
            if conn.links[e.triangle as usize][e.start as usize].is_none() {
                naked_idx = k as isize;
                num_naked += 1;
            }
        }
        let mut manifold = num_naked < 2;
        let mut border = num_naked == 1 && manifold;
        if naked_idx > 0 {
            edges.swap(first, first + naked_idx as usize);
        }
        if manifold {
            for i in 0..card.saturating_sub(1) {
                let cur = edges[first + i];
                let prev_slot = ((cur.start + 2) % 3) as usize;
                if let Some(link) = conn.links[cur.triangle as usize][prev_slot] {
                    let next_tri = link.triangle;
                    if edges[first + i + 1].triangle != next_tri {
                        let mut found = false;
                        for j in (i + 2)..card {
                            if edges[first + j].triangle == next_tri {
                                edges.swap(first + i + 1, first + j);
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            manifold = false;
                            border = false;
                            break;
                        }
                    }
                }
            }
            if manifold {
                let last = edges[first + card - 1];
                let last_prev_slot = ((last.start + 2) % 3) as usize;
                let link = conn.links[last.triangle as usize][last_prev_slot];
                if border {
                    if link.is_some() {
                        manifold = false;
                    }
                } else if link.is_none_or(|l| l.triangle != edges[first].triangle) {
                    manifold = false;
                }
            }
        }
        let _ = (manifold, border);
    }

    // Edge qualities over every edge, in vertex order.
    #[derive(Clone, Copy)]
    struct EdgeQuality {
        tri: i32,
        slot: i32,
        quality: f32,
        valid: bool,
    }
    let mut qualities: Vec<EdgeQuality> = Vec::with_capacity(num_edges);
    for e in 0..num_edges {
        let mut q = EdgeQuality {
            tri: -1,
            slot: -1,
            quality: f32::MAX,
            valid: false,
        };
        let edge = edges[e];
        let t = edge.triangle as usize;
        let slot = edge.start as usize;
        if let Some(link) = conn.links[t][slot] {
            let vis = [
                vertex(t, slot),
                vertex(t, (slot + 1) % 3),
                vertex(t, (slot + 2) % 3),
                vertex(link.triangle as usize, ((link.start + 2) % 3) as usize),
            ];
            let mut aabb = Aabb::empty();
            for i in vis {
                aabb.include_point(vt[i]);
            }
            let aabb_sa = aabb.surface_area();
            if aabb_sa > f32::EPSILON {
                let twice_sa = |i0: usize, i1: usize, i2: usize| -> f32 {
                    let d0x = vt[i1][0] - vt[i0][0];
                    let d0y = vt[i1][1] - vt[i0][1];
                    let d0z = vt[i1][2] - vt[i0][2];
                    let d1x = vt[i2][0] - vt[i0][0];
                    let d1y = vt[i2][1] - vt[i0][1];
                    let d1z = vt[i2][2] - vt[i0][2];
                    let cx = d0y * d1z - d0z * d1y;
                    let cy = d0z * d1x - d0x * d1z;
                    let cz = d0x * d1y - d0y * d1x;
                    (cx * cx + cy * cy + cz * cz).sqrt()
                };
                let quad_sa = twice_sa(vis[0], vis[1], vis[2]) + twice_sa(vis[0], vis[1], vis[3]);
                let quality = (aabb_sa - quad_sa) / aabb_sa;
                if vis[0] < vis[1] {
                    q.tri = t as i32;
                    q.slot = slot as i32;
                } else {
                    q.tri = link.triangle;
                    q.slot = link.start;
                }
                q.quality = quality;
                q.valid = true;
            }
        }
        qualities.push(q);
    }
    quick_sort(&mut qualities, &|a: &EdgeQuality, b: &EdgeQuality| {
        a.quality < b.quality
    });

    // Triangle planes.
    let planes: Vec<[f32; 4]> = (0..num_tris)
        .map(|t| {
            let v0 = vt[vertex(t, 0)];
            let v1 = vt[vertex(t, 1)];
            let v2 = vt[vertex(t, 2)];
            let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
            let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
            let mut nx = e1[1] * e2[2] - e1[2] * e2[1];
            let mut ny = e1[2] * e2[0] - e1[0] * e2[2];
            let mut nz = e1[0] * e2[1] - e1[1] * e2[0];
            let len_sq = nx * nx + ny * ny + nz * nz;
            let inv = if len_sq > 0.0 {
                1.0 / len_sq.sqrt()
            } else {
                0.0
            };
            nx *= inv;
            ny *= inv;
            nz *= inv;
            [nx, ny, nz, -(nx * v0[0] + ny * v0[1] + nz * v0[2])]
        })
        .collect();
    const EDGE_CLASSIFY_TOL: f32 = 0.01;
    let edge_concave_or_flat = |tri: usize, slot: usize| -> bool {
        let Some(link) = conn.links[tri][slot] else {
            return false;
        };
        let apex = vertex(link.triangle as usize, ((link.start + 2) % 3) as usize);
        let a = vt[apex];
        let p = planes[tri];
        let xy = p[0] * a[0] + p[1] * a[1];
        let zw = p[2] * a[2] + p[3];
        let d = xy + zw;
        d >= -EDGE_CLASSIFY_TOL
    };
    let triangle_concave_or_flat =
        |tri: usize| -> bool { (0..3).all(|e| edge_concave_or_flat(tri, e)) };

    let mut tri_interior = vec![false; num_tris];
    let mut tri_used = vec![false; num_tris];
    let mut primitives: Vec<MeshPrimitive> = Vec::new();

    // Greedy quad pairing by ascending quality.
    for eq in &qualities {
        if !eq.valid {
            continue;
        }
        let t = eq.tri as usize;
        let slot = eq.slot as usize;
        let Some(link) = conn.links[t][slot] else {
            continue;
        };
        let ot = link.triangle as usize;
        if tri_used[t] || tri_used[ot] {
            continue;
        }
        if tris[t].material != tris[ot].material {
            continue;
        }
        let vis = [
            vertex(t, (slot + 1) % 3),
            vertex(t, (slot + 2) % 3),
            vertex(t, slot),
            vertex(ot, ((link.start + 2) % 3) as usize),
        ];
        let mut aabb = Aabb::empty();
        for i in vis {
            aabb.include_point(vt[i]);
        }
        let ext = aabb.extents();
        if ext[0] > max_extent || ext[1] > max_extent || ext[2] > max_extent {
            continue;
        }
        let interior_t = triangle_concave_or_flat(t);
        let interior_ot = triangle_concave_or_flat(ot);
        tri_interior[t] = interior_t;
        tri_interior[ot] = interior_ot;
        primitives.push(MeshPrimitive {
            verts: [vis[0] as u32, vis[1] as u32, vis[2] as u32, vis[3] as u32],
            shape_tag: tris[t].material as u16,
            is_quad: true,
            interior: [interior_t, interior_ot],
        });
        tri_used[t] = true;
        tri_used[ot] = true;
    }
    let num_quads = primitives.len();

    // Standalone triangles.
    let mut standalone = Vec::new();
    for t in 0..num_tris {
        if tri_used[t] {
            continue;
        }
        standalone.push(t);
        let interior = triangle_concave_or_flat(t);
        tri_interior[t] = interior;
        let mut best_edge = 0usize;
        if vertex(t, 0) < vertex(t, 1) {
            best_edge = 1;
            if vertex(t, 1) < vertex(t, 2) {
                best_edge = 2;
            }
        }
        primitives.push(MeshPrimitive {
            verts: [
                vertex(t, best_edge) as u32,
                vertex(t, (best_edge + 1) % 3) as u32,
                vertex(t, (best_edge + 2) % 3) as u32,
                vertex(t, (best_edge + 2) % 3) as u32,
            ],
            shape_tag: tris[t].material as u16,
            is_quad: false,
            interior: [interior, interior],
        });
    }
    // Cascade: a standalone triangle whose neighbours are all convex-edged
    // and not interior becomes interior.
    for (si, &t) in standalone.iter().enumerate() {
        if tri_interior[t] {
            continue;
        }
        let mut can_disable = true;
        for e in 0..3 {
            let Some(link) = conn.links[t][e] else {
                can_disable = false;
                break;
            };
            if edge_concave_or_flat(link.triangle as usize, link.start as usize) {
                can_disable = false;
                break;
            }
            if tri_interior[link.triangle as usize] {
                can_disable = false;
                break;
            }
        }
        if can_disable {
            tri_interior[t] = true;
            primitives[num_quads + si].interior = [true, true];
        }
    }
    primitives
}

// ---------------------------------------------------------------------------
// BVH over primitives
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct BvhPoint {
    pos: [f32; 3],
    index: u32,
}

struct BvhBuilder {
    nodes: Vec<SimdTreeNode>,
    aabbs: Vec<Aabb>,
}

fn leaf_node() -> SimdTreeNode {
    let mut n = SimdTreeNode::cleared();
    n.is_leaf = true;
    n.is_active = true;
    n.data = [0xFFFF_FFFF; 4];
    n
}

fn internal_node() -> SimdTreeNode {
    let mut n = SimdTreeNode::cleared();
    n.is_active = true;
    n
}

impl BvhBuilder {
    fn build(points: &mut [BvhPoint], aabbs: &[Aabb]) -> Self {
        let mut b = Self {
            nodes: vec![SimdTreeNode::cleared(), SimdTreeNode::cleared()],
            aabbs: aabbs.to_vec(),
        };
        if points.is_empty() {
            return b;
        }
        b.build_hierarchy(points, 0, points.len(), 1);
        b.refit();
        b.sort_by_aabb_size();
        b
    }

    fn build_hierarchy(
        &mut self,
        points: &mut [BvhPoint],
        start: usize,
        count: usize,
        node: usize,
    ) {
        let mut stack = vec![(start, count, node)];
        while let Some((start, count, node)) = stack.pop() {
            if count <= 32 {
                self.process_small_range(points, start, count, node);
                continue;
            }
            let mut splits = [0usize; 5];
            splits[0] = start;
            splits[4] = start + count;
            splits[2] = self.split_sah3(points, splits[0], splits[4], 2);
            splits[1] = self.split_sah3(points, splits[0], splits[2], 1);
            splits[3] = self.split_sah3(points, splits[2], splits[4], 1);
            let mut subs: Vec<(usize, usize)> = Vec::new();
            for i in 0..4 {
                let (s, e) = (splits[i], splits[i + 1]);
                if e > s {
                    subs.push((s, e - s));
                }
            }
            self.nodes[node] = internal_node();
            let first_child = self.nodes.len();
            for _ in 0..subs.len() {
                self.nodes.push(SimdTreeNode::cleared());
            }
            for (i, &(s, c)) in subs.iter().enumerate() {
                let child = first_child + i;
                self.nodes[node].data[i] = child as u32;
                if c <= 4 {
                    self.nodes[child] = leaf_node();
                    for j in 0..c {
                        self.nodes[child].data[j] = points[s + j].index;
                    }
                }
            }
            for (i, &(s, c)) in subs.iter().enumerate() {
                if c > 4 {
                    stack.push((s, c, first_child + i));
                }
            }
        }
    }

    fn insertion_sort_axis(arr: &mut [BvhPoint], axis: usize) {
        for i in 0..arr.len() {
            let key = arr[i];
            let mut j = i;
            while j > 0 && key.pos[axis] < arr[j - 1].pos[axis] {
                arr[j] = arr[j - 1];
                j -= 1;
            }
            arr[j] = key;
        }
    }

    fn sah_reorder_small_range(&self, points: &mut [BvhPoint], start: usize, count: usize) {
        if count <= 4 {
            return;
        }
        let mut pow2 = 4usize;
        while pow2 < count {
            pow2 *= 2;
        }
        let pivot = pow2 / 2;
        let mut best_score = f32::MAX;
        let mut best_axis = 0usize;
        let mut axis_sets: [Vec<BvhPoint>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for axis in 0..3 {
            axis_sets[axis] = points[start..start + count].to_vec();
            Self::insertion_sort_axis(&mut axis_sets[axis], axis);
            let mut left = Aabb::empty();
            let mut right = Aabb::empty();
            for i in 0..count {
                let a = &self.aabbs[axis_sets[axis][i].index as usize];
                if i < pivot {
                    left.include_aabb(a);
                } else {
                    right.include_aabb(a);
                }
            }
            let score = left.surface_area() + right.surface_area();
            if score < best_score {
                best_score = score;
                best_axis = axis;
            }
        }
        points[start..start + count].copy_from_slice(&axis_sets[best_axis]);
        self.sah_reorder_small_range(points, start, pivot);
        self.sah_reorder_small_range(points, start + pivot, count - pivot);
    }

    fn process_small_range(
        &mut self,
        points: &mut [BvhPoint],
        start: usize,
        count: usize,
        node: usize,
    ) {
        self.sah_reorder_small_range(points, start, count);
        let mut offset = start;
        let mut remaining = count;
        let mut current_root = node;
        let mut has_left_overs = true;
        while has_left_overs {
            let mut sub: Vec<(usize, usize)> = Vec::new();
            while remaining > 4 && sub.len() < 3 {
                sub.push((offset, 4));
                offset += 4;
                remaining -= 4;
            }
            if remaining > 0 {
                sub.push((offset, remaining));
            }
            self.nodes[current_root] = internal_node();
            has_left_overs = false;
            let first_child = self.nodes.len();
            for _ in 0..sub.len() {
                self.nodes.push(SimdTreeNode::cleared());
            }
            for (i, &(s, c)) in sub.iter().enumerate() {
                let child = first_child + i;
                self.nodes[current_root].data[i] = child as u32;
                if c <= 4 {
                    self.nodes[child] = leaf_node();
                    for j in 0..c {
                        self.nodes[child].data[j] = points[s + j].index;
                    }
                } else {
                    has_left_overs = true;
                    current_root = child;
                    offset = s;
                    remaining = c;
                }
            }
        }
    }

    fn split_sah3(
        &self,
        points: &mut [BvhPoint],
        start: usize,
        end: usize,
        min_count: usize,
    ) -> usize {
        let count = end - start;
        if count <= 1 {
            return end;
        }
        let mut best_cost = f32::MAX;
        let mut best_axis = 0usize;
        let mut best_split = start + count / 2;
        let mut axis_sets: [Vec<BvhPoint>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for axis in 0..3 {
            axis_sets[axis] = points[start..end].to_vec();
            quick_sort(&mut axis_sets[axis], &|a: &BvhPoint, b: &BvhPoint| {
                a.pos[axis] < b.pos[axis]
            });
            let mut scores = vec![0f32; count];
            let mut running = Aabb::empty();
            for i in 0..count {
                running.include_aabb(&self.aabbs[axis_sets[axis][i].index as usize]);
                scores[i] = (i + 1) as f32 * running.surface_area();
            }
            running = Aabb::empty();
            let mut i = count - 1;
            let mut j = 1usize;
            while i > 0 {
                running.include_aabb(&self.aabbs[axis_sets[axis][i].index as usize]);
                let sum = scores[i - 1] + j as f32 * running.surface_area();
                if sum < best_cost {
                    best_cost = sum;
                    best_axis = axis;
                    best_split = start + i;
                }
                i -= 1;
                j += 1;
            }
        }
        points[start..end].copy_from_slice(&axis_sets[best_axis]);
        let left = best_split - start;
        let right = end - best_split;
        if left < min_count || right < min_count {
            best_split = start + count / 2;
        }
        best_split
    }

    fn refit(&mut self) {
        for i in (0..self.nodes.len()).rev() {
            let node = self.nodes[i];
            let mut updated = node;
            if node.is_leaf {
                for c in 0..4 {
                    if node.data[c] == 0xFFFF_FFFF {
                        updated.lx[c] = 0.0;
                        updated.hx[c] = 0.0;
                        updated.ly[c] = 0.0;
                        updated.hy[c] = 0.0;
                        updated.lz[c] = 0.0;
                        updated.hz[c] = 0.0;
                        continue;
                    }
                    let a = self.aabbs[node.data[c] as usize];
                    updated.set_lane_aabb(c, &a);
                }
            } else {
                for c in 0..4 {
                    if node.data[c] == 0 {
                        updated.lx[c] = 0.0;
                        updated.hx[c] = 0.0;
                        updated.ly[c] = 0.0;
                        updated.hy[c] = 0.0;
                        updated.lz[c] = 0.0;
                        updated.hz[c] = 0.0;
                        continue;
                    }
                    let child = self.nodes[node.data[c] as usize].compound_aabb();
                    updated.set_lane_aabb(c, &child);
                }
            }
            self.nodes[i] = updated;
        }
    }

    fn sort_by_aabb_size(&mut self) {
        for node in self.nodes.iter_mut() {
            if !node.is_active {
                continue;
            }
            sort_node_lanes_by_volume(node);
        }
    }
}

/// Sorts the valid lanes of `node` by descending volume (insertion sort,
/// stable), clearing the rest to zero.
fn sort_node_lanes_by_volume(node: &mut SimdTreeNode) {
    let saved = *node;
    let mut entries: Vec<(f32, usize)> = Vec::new();
    for i in 0..4 {
        if saved.lane_valid(i) {
            let ex = saved.hx[i] - saved.lx[i];
            let ey = saved.hy[i] - saved.ly[i];
            let ez = saved.hz[i] - saved.lz[i];
            entries.push((ex * ey * ez, i));
        }
    }
    for i in 1..entries.len() {
        let key = entries[i];
        let mut j = i;
        while j > 0 && key.0 > entries[j - 1].0 {
            entries[j] = entries[j - 1];
            j -= 1;
        }
        entries[j] = key;
    }
    for (i, &(_, orig)) in entries.iter().enumerate() {
        node.data[i] = saved.data[orig];
        node.lx[i] = saved.lx[orig];
        node.hx[i] = saved.hx[orig];
        node.ly[i] = saved.ly[orig];
        node.hy[i] = saved.hy[orig];
        node.lz[i] = saved.lz[orig];
        node.hz[i] = saved.hz[orig];
    }
    for i in entries.len()..4 {
        node.data[i] = if node.is_leaf { 0xFFFF_FFFF } else { 0 };
        node.lx[i] = 0.0;
        node.hx[i] = 0.0;
        node.ly[i] = 0.0;
        node.hy[i] = 0.0;
        node.lz[i] = 0.0;
        node.hz[i] = 0.0;
    }
}

fn count_primitives_in_subtree(nodes: &[SimdTreeNode], idx: usize) -> usize {
    let node = &nodes[idx];
    if node.is_leaf {
        return (0..4).filter(|&i| node.data[i] != 0xFFFF_FFFF).count();
    }
    (0..4)
        .filter(|&i| node.data[i] != 0)
        .map(|i| count_primitives_in_subtree(nodes, node.data[i] as usize))
        .sum()
}

fn spatial_hash(v: [f32; 3]) -> u64 {
    let (p1, p2, p3) = (73856093u64, 19349663u64, 83492791u64);
    (v[0].to_bits() as u64).wrapping_mul(p1)
        ^ (v[1].to_bits() as u64).wrapping_mul(p2)
        ^ (v[2].to_bits() as u64).wrapping_mul(p3)
}

fn count_unique_vertices_in_subtree(
    nodes: &[SimdTreeNode],
    idx: usize,
    verts: &[[f32; 3]],
    prims: &[MeshPrimitive],
) -> usize {
    #[derive(Clone, Copy)]
    struct VertexHash {
        pos: [f32; 3],
        hash: u64,
    }
    let mut all: Vec<VertexHash> = Vec::new();
    let mut stack = vec![idx];
    while let Some(i) = stack.pop() {
        let n = &nodes[i];
        if n.is_leaf {
            for lane in 0..4 {
                if n.data[lane] == 0xFFFF_FFFF {
                    continue;
                }
                let mp = &prims[n.data[lane] as usize];
                let count = if mp.is_quad { 4 } else { 3 };
                for j in 0..count {
                    let pos = verts[mp.verts[j] as usize];
                    all.push(VertexHash {
                        pos,
                        hash: spatial_hash(pos),
                    });
                }
            }
        } else {
            for lane in 0..4 {
                if n.data[lane] != 0 {
                    stack.push(n.data[lane] as usize);
                }
            }
        }
    }
    quick_sort(&mut all, &|a: &VertexHash, b: &VertexHash| a.hash < b.hash);
    let mut unique = 0usize;
    for i in 0..all.len() {
        if i > 0 && all[i].hash == all[i - 1].hash && all[i].pos == all[i - 1].pos {
            continue;
        }
        unique += 1;
    }
    unique
}

fn can_subtree_fit_into_section(
    nodes: &[SimdTreeNode],
    idx: usize,
    max_extent: f32,
    verts: &[[f32; 3]],
    prims: &[MeshPrimitive],
) -> bool {
    let aabb = nodes[idx].compound_aabb();
    let ext = aabb.extents();
    if ext.iter().any(|e| *e > max_extent) {
        return false;
    }
    let prim_count = count_primitives_in_subtree(nodes, idx);
    if (1.1f32 * prim_count as f32) as i32 > GeometrySection::MAX_VERTICES as i32 {
        return false;
    }
    count_unique_vertices_in_subtree(nodes, idx, verts, prims) <= GeometrySection::MAX_VERTICES
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

#[derive(Default)]
struct TempSection {
    domain: Aabb,
    original_domain: Aabb,
    refit_domain: Aabb,
    bvh: Vec<Aabb8TreeNode>,
    primitives: Vec<Primitive>,
    /// World-space vertices before quantization.
    vertices: Vec<[f32; 3]>,
    quantized: Vec<[u16; 3]>,
    shape_tags: Vec<u16>,
    /// Interior flags per primitive: first and second triangle.
    interior: Vec<[bool; 2]>,
    interior_bits: Vec<u8>,
    section_offset: [u32; 3],
    bit_scale8_inv: [f32; 3],
    bit_offset: [i16; 3],
    /// Built from a leaf group of at most four primitives (no welding).
    from_leaf_group: bool,
}

impl Default for Aabb {
    fn default() -> Self {
        Aabb::empty()
    }
}

fn find_vertex(verts: &[[f32; 3]], v: [f32; 3]) -> Option<usize> {
    verts.iter().position(|w| *w == v)
}

fn build_section_geometry(
    nodes: &[SimdTreeNode],
    root: usize,
    verts: &[[f32; 3]],
    prims: &[MeshPrimitive],
    max_vertex_error: f32,
) -> TempSection {
    let mut section = TempSection::default();
    section.original_domain = nodes[root].compound_aabb();
    section.domain = section.original_domain;
    section.domain.expand_by(max_vertex_error);
    let converter = Aabb8Quantizer::new(&section.domain);
    for i in 0..3 {
        section.bit_offset[i] = converter.bit_offset[i] as i16;
        section.bit_scale8_inv[i] = converter.bit_scale_inv[i];
    }
    let mut stack: Vec<(usize, isize)> = vec![(root, -1)];
    let mut node_is_leaf: Vec<bool> = Vec::new();
    while let Some((bvh_idx, parent)) = stack.pop() {
        let node = nodes[bvh_idx];
        let current = section.bvh.len();
        let mut aabb8 = Aabb8TreeNode::default();
        node_is_leaf.push(node.is_leaf);
        for i in 0..4 {
            if !node.lane_valid(i) {
                aabb8.clear_lane(i);
                continue;
            }
            let mut child = node.lane_aabb(i);
            for a in 0..3 {
                child.min[a] = child.min[a].max(section.domain.min[a]);
                child.max[a] = child.max[a].min(section.domain.max[a]);
            }
            aabb8.set_lane(i, converter.convert(&child));
        }
        section.bvh.push(aabb8);
        if parent >= 0 {
            let p = &mut section.bvh[parent as usize];
            for i in 0..4 {
                if p.data[i] == 0 {
                    p.data[i] = current as u8;
                    break;
                }
            }
        }
        if node.is_leaf {
            for i in 0..4 {
                if node.data[i] == 0xFFFF_FFFF {
                    continue;
                }
                let mp = &prims[node.data[i] as usize];
                section.shape_tags.push(mp.shape_tag);
                let count = if mp.is_quad { 4 } else { 3 };
                let mut ids = [0u8; 4];
                for v in 0..count {
                    let pos = verts[mp.verts[v] as usize];
                    let idx = match find_vertex(&section.vertices, pos) {
                        Some(idx) => idx,
                        None => {
                            section.vertices.push(pos);
                            section.vertices.len() - 1
                        }
                    };
                    ids[v] = idx as u8;
                }
                if !mp.is_quad {
                    ids[3] = ids[2];
                }
                section.bvh[current].data[i] = section.primitives.len() as u8;
                section.primitives.push(Primitive {
                    a: ids[0],
                    b: ids[1],
                    c: ids[2],
                    d: ids[3],
                });
                section.interior.push(mp.interior);
            }
        } else {
            for i in (0..4).rev() {
                if node.data[i] != 0 {
                    stack.push((node.data[i] as usize, current as isize));
                }
            }
        }
    }
    for i in 0..section.bvh.len() {
        configure_leaf_or_internal(&mut section.bvh[i], node_is_leaf[i]);
    }
    section
}

fn build_section_from_primitives(
    prim_indices: &[u32],
    verts: &[[f32; 3]],
    prims: &[MeshPrimitive],
    max_vertex_error: f32,
) -> TempSection {
    let mut section = TempSection {
        from_leaf_group: true,
        ..Default::default()
    };
    let mut aabb = Aabb::empty();
    let mut prim_aabbs = Vec::new();
    for &pi in prim_indices {
        let mp = &prims[pi as usize];
        let count = if mp.is_quad { 4 } else { 3 };
        let mut prim_aabb = Aabb::empty();
        let start = section.vertices.len();
        for v in 0..count {
            let pos = verts[mp.verts[v] as usize];
            prim_aabb.include_point(pos);
            section.vertices.push(pos);
        }
        let mut p = Primitive {
            a: start as u8,
            b: (start + 1) as u8,
            c: (start + 2) as u8,
            d: (start + 3) as u8,
        };
        if !mp.is_quad {
            p.d = p.c;
        }
        section.primitives.push(p);
        section.shape_tags.push(mp.shape_tag);
        section.interior.push(mp.interior);
        aabb.include_aabb(&prim_aabb);
        prim_aabbs.push(prim_aabb);
    }
    section.original_domain = aabb;
    section.domain = aabb;
    section.domain.expand_by(max_vertex_error);
    let converter = Aabb8Quantizer::new(&section.domain);
    for i in 0..3 {
        section.bit_offset[i] = converter.bit_offset[i] as i16;
        section.bit_scale8_inv[i] = converter.bit_scale_inv[i];
    }
    let mut leaf = Aabb8TreeNode::default();
    for i in 0..4 {
        leaf.clear_lane(i);
    }
    for (i, prim_aabb) in prim_aabbs.iter().enumerate() {
        leaf.set_lane(i, converter.convert(prim_aabb));
        leaf.data[i] = i as u8;
    }
    configure_leaf_or_internal(&mut leaf, true);
    section.bvh.push(leaf);
    section
}

/// Encodes a section's vertices. A section whose domain exceeds the 16-bit
/// range (a single primitive wider than `max_extent`, which the grouping
/// leaves alone in its section) is stored the way Havok does: the offset
/// marker `0x7FFFFFFF` and raw `f32` positions, two 16-bit triples each.
fn quantize_section(conv: &VertexConversion, section: &mut TempSection) {
    let ext = section.original_domain.extents();
    if ext.iter().any(|e| *e > conv.max_extent()) {
        section.section_offset = FLOAT_VERTEX_SECTION_OFFSET;
        section.quantized = section
            .vertices
            .iter()
            .flat_map(|v| {
                let [x, y, z] = v.map(f32::to_bits);
                let lo = |b: u32| (b & 0xFFFF) as u16;
                let hi = |b: u32| (b >> 16) as u16;
                [[lo(x), hi(x), lo(y)], [hi(y), lo(z), hi(z)]]
            })
            .collect();
        return;
    }
    section.section_offset = conv.section_offset(&section.original_domain);
    section.quantized = section
        .vertices
        .iter()
        .map(|v| conv.pack(*v, section.section_offset))
        .collect();
}

/// World position of vertex `id` as the file will hold it: the original
/// float in a float section, the unpacked 16-bit value otherwise.
fn section_vertex(conv: &VertexConversion, section: &TempSection, id: usize) -> [f32; 3] {
    if section.section_offset == FLOAT_VERTEX_SECTION_OFFSET {
        section.vertices.get(id).copied().unwrap_or_default()
    } else {
        conv.unpack(section.quantized[id], section.section_offset)
    }
}

fn refit_section_bvh(conv: &VertexConversion, section: &mut TempSection) {
    let converter = Aabb8Quantizer::new(&section.domain);
    for idx in (0..section.bvh.len()).rev() {
        let node = section.bvh[idx];
        let mut updated = node;
        if node.is_leaf() {
            for c in 0..4 {
                if !node.lane_valid(c) {
                    continue;
                }
                let prim = section.primitives[node.data[c] as usize];
                let ids = prim.ids();
                let count = if prim.is_triangle() { 3 } else { 4 };
                let mut aabb = Aabb::empty();
                for v in 0..count {
                    let p = section_vertex(conv, section, ids[v] as usize);
                    aabb.include_point(p);
                }
                updated.set_lane(c, converter.convert(&aabb));
            }
        } else {
            for c in 0..4 {
                if !node.lane_valid(c) {
                    continue;
                }
                let child = section.bvh[node.data[c] as usize].compound();
                updated.set_lane(c, child);
            }
        }
        section.bvh[idx] = updated;
    }
}

fn triangles_equivalent(a: [u8; 3], b: [u8; 3]) -> bool {
    (0..3).any(|o| a[0] == b[o % 3] && a[1] == b[(o + 1) % 3] && a[2] == b[(o + 2) % 3])
}

/// Rotates a quad so that `b > d` encodes "flat convex" the way Havok does.
/// Returns true when the rotation swapped which triangle comes first.
fn set_flat_convex_quad(p: &mut Primitive, flat_convex: bool) -> bool {
    let current = p.b > p.d;
    if current == flat_convex {
        return false;
    }
    let first = [p.a, p.c, p.d];
    for offset in 0..3 {
        let result = [
            first[offset % 3],
            first[(offset + 1) % 3],
            first[(offset + 2) % 3],
            p.b,
        ];
        if (result[1] > result[3]) != flat_convex {
            continue;
        }
        if !triangles_equivalent([result[0], result[2], result[3]], [p.a, p.b, p.c]) {
            continue;
        }
        p.a = result[0];
        p.b = result[1];
        p.c = result[2];
        p.d = result[3];
        return true;
    }
    false
}

/// Havok's flat-convex quad test on dequantized corners: coplanar within
/// tolerance, non-degenerate second half, and convex at the diagonal.
fn check_flat_convex_quad(a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]) -> bool {
    let sub = |p: [f32; 3], q: [f32; 3]| [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
    let cross = |u: [f32; 3], v: [f32; 3]| {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let dot = |u: [f32; 3], v: [f32; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let v_ab = sub(b, a);
    let v_ac = sub(c, a);
    let n = cross(v_ab, v_ac);
    let n_len_sq = dot(n, n);
    let p30 = sub(a, d);
    let out_of_plane = dot(p30, n).abs();
    let tol = 1e-3f32;
    if out_of_plane * out_of_plane > tol * tol * n_len_sq {
        return false;
    }
    let v_ad = sub(d, a);
    let n2 = cross(v_ac, v_ad);
    let n2_len_sq = dot(n2, n2);
    if n2_len_sq < n_len_sq * 0.01 {
        return false;
    }
    let v_cd = sub(d, c);
    let v_cb = sub(b, c);
    let cx0 = cross(v_ab, v_ad);
    let cx1 = cross(v_cd, v_cb);
    dot(cx0, cx1) >= 0.0
}

fn mark_flat_convex_quads(conv: &VertexConversion, section: &mut TempSection) {
    let flats: Vec<Option<bool>> = section
        .primitives
        .iter()
        .map(|p| {
            if p.is_triangle() {
                return None;
            }
            let corner = |id: u8| section_vertex(conv, section, id as usize);
            Some(check_flat_convex_quad(
                corner(p.a),
                corner(p.b),
                corner(p.c),
                corner(p.d),
            ))
        })
        .collect();
    for (i, flat) in flats.into_iter().enumerate() {
        let Some(flat) = flat else { continue };
        if set_flat_convex_quad(&mut section.primitives[i], flat) {
            section.interior[i].swap(0, 1);
        }
    }
}

fn flag_interior_triangles(section: &mut TempSection) {
    let count = section.primitives.len();
    section.interior_bits = vec![0u8; (count * 2).div_ceil(8)];
    for (p, prim) in section.primitives.iter().enumerate() {
        let flags = section.interior[p];
        let first = 2 * p;
        if flags[0] {
            section.interior_bits[first / 8] |= 1 << (first % 8);
        }
        if !prim.is_triangle() && flags[1] {
            let second = first + 1;
            section.interior_bits[second / 8] |= 1 << (second % 8);
        }
    }
}

fn reorder_section_by_shape_tag(section: &mut TempSection) {
    let n = section.primitives.len();
    if n <= 1 {
        return;
    }
    let mut perm: Vec<u32> = (0..n as u32).collect();
    let tags = section.shape_tags.clone();
    quick_sort(&mut perm, &|a: &u32, b: &u32| {
        tags[*a as usize] < tags[*b as usize]
    });
    if perm.iter().enumerate().all(|(i, p)| *p as usize == i) {
        return;
    }
    let old_prims = section.primitives.clone();
    let old_tags = section.shape_tags.clone();
    let old_interior = section.interior.clone();
    for i in 0..n {
        let from = perm[i] as usize;
        section.primitives[i] = old_prims[from];
        section.shape_tags[i] = old_tags[from];
        section.interior[i] = old_interior[from];
    }
    let mut inverse = vec![0u8; n];
    for i in 0..n {
        inverse[perm[i] as usize] = i as u8;
    }
    for node in section.bvh.iter_mut() {
        if !node.is_leaf() {
            continue;
        }
        for c in 0..4 {
            if !node.lane_valid(c) {
                continue;
            }
            node.data[c] = inverse[node.data[c] as usize];
        }
        configure_leaf_or_internal(node, true);
    }
    flag_interior_triangles(section);
}

fn build_shape_tag_table(sections: &[TempSection], shift: u32) -> Vec<ShapeTagEntry> {
    let mut out = vec![ShapeTagEntry {
        mesh_primitive_key: 0,
        shape_tag: sections
            .first()
            .and_then(|s| s.shape_tags.first().copied())
            .unwrap_or(0xFFFF),
    }];
    for (s, section) in sections.iter().enumerate() {
        for (p, tag) in section.shape_tags.iter().enumerate() {
            if out.last().unwrap().shape_tag != *tag {
                out.push(ShapeTagEntry {
                    mesh_primitive_key: pack_mesh_primitive_key(s as u32, p as u32, 0, shift),
                    shape_tag: *tag,
                });
            }
        }
    }
    out.push(ShapeTagEntry {
        mesh_primitive_key: pack_mesh_primitive_key(sections.len() as u32, 0xFF, 0, shift),
        shape_tag: 0xFFFF,
    });
    out
}

// ---------------------------------------------------------------------------
// Top-level assembly
// ---------------------------------------------------------------------------

/// Build settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildOptions {
    /// Maximum quantization error of a vertex, in world units.
    pub max_vertex_error: f32,
    /// Merge vertices with identical positions before building. TOTK's
    /// builder keeps the source indexing as is; welding is for unindexed
    /// inputs (three vertices per triangle).
    pub weld: bool,
    /// Sort vertices and triangles into a canonical order first, so the
    /// result depends on the geometry alone and not on the order the
    /// exporter wrote it in (ties in the quad pairing and BVH splits are
    /// broken by index). Rebuilding a shape from its own geometry then
    /// reproduces it byte for byte.
    pub canonical: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            max_vertex_error: DEFAULT_MAX_VERTEX_ERROR,
            weld: false,
            canonical: false,
        }
    }
}

/// Reorders `geometry` canonically: vertices ascending by (x, y, z) bit
/// patterns, every triangle rotated to start at its smallest index, and
/// triangles ascending by their indices (material last).
pub fn canonicalize(geometry: &Geometry) -> Geometry {
    let mut order: Vec<usize> = (0..geometry.vertices.len()).collect();
    let key = |v: &[f32; 3]| {
        [
            float_to_ordered_uint(v[0]),
            float_to_ordered_uint(v[1]),
            float_to_ordered_uint(v[2]),
        ]
    };
    order.sort_by_key(|&i| key(&geometry.vertices[i]));
    let mut remap = vec![0u32; geometry.vertices.len()];
    for (new, &old) in order.iter().enumerate() {
        remap[old] = new as u32;
    }
    let vertices: Vec<[f32; 3]> = order.iter().map(|&i| geometry.vertices[i]).collect();
    let mut triangles: Vec<Triangle> = geometry
        .triangles
        .iter()
        .map(|t| {
            let ids = [
                remap[t.a as usize],
                remap[t.b as usize],
                remap[t.c as usize],
            ];
            let start = (0..3).min_by_key(|&i| ids[i]).unwrap();
            Triangle {
                a: ids[start],
                b: ids[(start + 1) % 3],
                c: ids[(start + 2) % 3],
                material: t.material,
            }
        })
        .collect();
    triangles.sort_by_key(|t| (t.a, t.b, t.c, t.material));
    Geometry {
        vertices,
        triangles,
        materials: geometry.materials.clone(),
    }
}

/// Builds a mesh shape from `geometry` with the default options.
pub fn build(geometry: &Geometry) -> Option<MeshShape> {
    build_with_options(geometry, BuildOptions::default())
}

pub fn build_with_options(geometry: &Geometry, options: BuildOptions) -> Option<MeshShape> {
    if geometry.triangles.is_empty() {
        return None;
    }
    let max_vertex_error = options.max_vertex_error;
    let conv = VertexConversion::new(max_vertex_error);
    let max_extent = VertexConversion::max_section_extent(max_vertex_error);

    let canonical;
    let geometry = if options.canonical {
        canonical = canonicalize(geometry);
        &canonical
    } else {
        geometry
    };
    let welded = if options.weld {
        weld_duplicate_vertices(geometry)
    } else {
        let mut triangles = geometry.triangles.clone();
        triangles.retain(|t| t.a != t.b && t.a != t.c && t.b != t.c);
        WeldedGeometry {
            vertices: geometry.vertices.clone(),
            triangles,
        }
    };
    let mut prims = create_quad_dominant_geometry(&welded, max_extent);
    // Triangles first, then quads (stable).
    let (tris, quads): (Vec<_>, Vec<_>) = prims.iter().copied().partition(|p| !p.is_quad);
    prims = tris.into_iter().chain(quads).collect();
    let verts = &welded.vertices;

    let mut prim_aabbs = Vec::with_capacity(prims.len());
    let mut points = Vec::with_capacity(prims.len());
    for (i, mp) in prims.iter().enumerate() {
        let mut aabb = Aabb::empty();
        let count = if mp.is_quad { 4 } else { 3 };
        for v in 0..count {
            aabb.include_point(verts[mp.verts[v] as usize]);
        }
        points.push(BvhPoint {
            pos: aabb.center(),
            index: i as u32,
        });
        prim_aabbs.push(aabb);
    }
    let bvh = BvhBuilder::build(&mut points, &prim_aabbs);

    // Cut the BVH into sections while recording the section tree. TOTK
    // first tries the whole tree as one section (the port only ever tests
    // the children of a visited node).
    let mut sections: Vec<TempSection> = Vec::new();
    let root_fits = !bvh.nodes[1].is_leaf
        && can_subtree_fit_into_section(&bvh.nodes, 1, max_extent, verts, &prims);
    let mut bvh_stack: Vec<usize> = if root_fits { Vec::new() } else { vec![1] };
    let mut tree: Vec<SimdTreeNode> = vec![SimdTreeNode::cleared()];
    let mut tree_stack: Vec<(usize, usize)> = vec![(0, 0)];
    if root_fits {
        let section = build_section_geometry(&bvh.nodes, 1, verts, &prims, max_vertex_error);
        let mut leaf = leaf_node();
        leaf.set_lane_aabb(0, &section.original_domain);
        leaf.data[0] = 0;
        sections.push(section);
        tree.push(leaf);
    }
    while let Some(root) = bvh_stack.pop() {
        let root_node = bvh.nodes[root];
        let mut convert = [false; 4];
        let mut valid_children = 0usize;
        for i in (0..4).rev() {
            if !root_node.lane_valid(i) {
                continue;
            }
            valid_children += 1;
            convert[i] = if root_node.is_leaf {
                true
            } else {
                can_subtree_fit_into_section(
                    &bvh.nodes,
                    root_node.data[i] as usize,
                    max_extent,
                    verts,
                    &prims,
                )
            };
            if !convert[i] {
                bvh_stack.push(root_node.data[i] as usize);
            }
        }
        let mut num_in_section = 0usize;
        let mut section_indices: Vec<usize> = Vec::new();
        if !root_node.is_leaf {
            for i in 0..valid_children {
                if !convert[i] {
                    continue;
                }
                let section = build_section_geometry(
                    &bvh.nodes,
                    root_node.data[i] as usize,
                    verts,
                    &prims,
                    max_vertex_error,
                );
                sections.push(section);
                section_indices.push(sections.len() - 1);
                num_in_section += 1;
            }
        } else {
            let prim_indices: Vec<u32> = (0..4)
                .filter(|&i| root_node.data[i] != 0xFFFF_FFFF)
                .map(|i| root_node.data[i])
                .collect();
            let leaf_aabbs: Vec<Aabb> = prim_indices
                .iter()
                .map(|&pi| {
                    let mp = &prims[pi as usize];
                    let count = if mp.is_quad { 4 } else { 3 };
                    let mut a = Aabb::empty();
                    for v in 0..count {
                        a.include_point(verts[mp.verts[v] as usize]);
                    }
                    a
                })
                .collect();
            let prim_count = prim_indices.len();
            let mut used = vec![false; prim_count];
            let mut group: Vec<u32> = vec![prim_indices[0]];
            used[0] = true;
            let mut remaining = prim_count - 1;
            let mut group_aabb = leaf_aabbs[0];
            while remaining > 0 {
                let mut expanded = false;
                for i in 0..prim_count {
                    if used[i] {
                        continue;
                    }
                    let mut new_aabb = group_aabb;
                    new_aabb.include_aabb(&leaf_aabbs[i]);
                    let ext = new_aabb.extents();
                    if ext.iter().all(|e| *e <= max_extent) {
                        group.push(prim_indices[i]);
                        used[i] = true;
                        expanded = true;
                        group_aabb = new_aabb;
                        remaining -= 1;
                    }
                }
                if !expanded && remaining > 0 {
                    let section =
                        build_section_from_primitives(&group, verts, &prims, max_vertex_error);
                    sections.push(section);
                    section_indices.push(sections.len() - 1);
                    for i in 0..prim_count {
                        if !used[i] {
                            group = vec![prim_indices[i]];
                            used[i] = true;
                            remaining -= 1;
                            group_aabb = leaf_aabbs[i];
                            break;
                        }
                    }
                }
            }
            let section = build_section_from_primitives(&group, verts, &prims, max_vertex_error);
            sections.push(section);
            section_indices.push(sections.len() - 1);
            num_in_section = prim_count;
        }

        // Section tree bookkeeping.
        let all_converted = valid_children == num_in_section;
        let new_node = tree.len();
        tree.push(if all_converted {
            leaf_node()
        } else {
            internal_node()
        });
        if let Some((parent, slot)) = tree_stack.pop() {
            if parent != 0 {
                tree[parent].data[slot] = new_node as u32;
            }
        }
        if num_in_section == 0 || num_in_section == valid_children {
            if !root_node.is_leaf {
                for i in 0..4 {
                    if root_node.lane_valid(i) {
                        let a = root_node.lane_aabb(i);
                        tree[new_node].set_lane_aabb(i, &a);
                    }
                }
                tree[new_node].is_leaf = all_converted;
                for i in (0..valid_children).rev() {
                    if all_converted {
                        tree[new_node].data[i] = section_indices[i] as u32;
                    } else {
                        tree_stack.push((new_node, i));
                    }
                }
            } else {
                tree[new_node] = leaf_node();
                for (i, &si) in section_indices.iter().enumerate() {
                    let domain = sections[si].original_domain;
                    tree[new_node].set_lane_aabb(i, &domain);
                    tree[new_node].data[i] = si as u32;
                }
            }
        } else {
            let leaf_idx = tree.len();
            tree.push(leaf_node());
            for (i, &si) in section_indices.iter().enumerate() {
                let domain = sections[si].original_domain;
                tree[leaf_idx].set_lane_aabb(i, &domain);
                tree[leaf_idx].data[i] = si as u32;
            }
            let mut leaf_child_idx: isize = -1;
            for i in 0..valid_children {
                if convert[i] {
                    let leaf_aabb = tree[leaf_idx].compound_aabb();
                    tree[new_node].set_lane_aabb(i, &leaf_aabb);
                    tree[new_node].data[i] = leaf_idx as u32;
                    leaf_child_idx = i as isize;
                    break;
                }
            }
            let mut slot = valid_children as isize - section_indices.len() as isize;
            for i in (0..valid_children).rev() {
                if !convert[i] {
                    if slot == leaf_child_idx {
                        slot -= 1;
                    }
                    let a = root_node.lane_aabb(i);
                    tree[new_node].set_lane_aabb(slot as usize, &a);
                    tree_stack.push((new_node, slot as usize));
                    slot -= 1;
                }
            }
        }
    }

    // The tree root is always an internal node: a lone leaf gets wrapped.
    if tree.len() == 2 && tree[1].is_leaf {
        let leaf = tree[1];
        let mut wrapper = internal_node();
        wrapper.set_lane_aabb(0, &leaf.compound_aabb());
        wrapper.data[0] = 2;
        tree[1] = wrapper;
        tree.push(leaf);
    }

    // Quantize, refit, flag, reorder.
    for section in sections.iter_mut() {
        quantize_section(&conv, section);
        mark_flat_convex_quads(&conv, section);
        refit_section_bvh(&conv, section);
        section.refit_domain = Aabb::empty();
        for prim in &section.primitives {
            let ids = prim.ids();
            let count = if prim.is_triangle() { 3 } else { 4 };
            for v in 0..count {
                let p = section_vertex(&conv, section, ids[v] as usize);
                section.refit_domain.include_point(p);
            }
        }
        flag_interior_triangles(section);
        reorder_section_by_shape_tag(section);
    }
    let max_prims = sections
        .iter()
        .map(|s| s.primitives.len())
        .max()
        .unwrap_or(0);
    let num_prim_bits = bit_width(max_prims as i64 - 1);
    let shift = num_prim_bits + 1;
    let shape_tag_table = build_shape_tag_table(&sections, shift);

    // Finalize the section tree: refit from dequantized domains, sort lanes.
    for node in tree.iter_mut() {
        node.is_active = false;
    }
    for i in (0..tree.len()).rev() {
        let node = tree[i];
        let mut updated = node;
        if node.is_leaf {
            for c in 0..4 {
                if node.data[c] == 0xFFFF_FFFF {
                    continue;
                }
                let si = node.data[c] as usize;
                if si < sections.len() {
                    updated.set_lane_aabb(c, &sections[si].refit_domain);
                }
            }
        } else {
            for c in 0..4 {
                if node.data[c] == 0 {
                    continue;
                }
                let child = node.data[c] as usize;
                if child > 0 && child < tree.len() {
                    let child_aabb = tree[child].compound_aabb();
                    updated.set_lane_aabb(c, &child_aabb);
                }
            }
        }
        tree[i] = updated;
    }
    for node in tree.iter_mut() {
        let saved = *node;
        let mut entries: Vec<(f32, usize)> = Vec::new();
        for c in 0..4 {
            if saved.lane_valid(c) {
                let ex = saved.hx[c] - saved.lx[c];
                let ey = saved.hy[c] - saved.ly[c];
                let ez = saved.hz[c] - saved.lz[c];
                entries.push((ex * ey * ez, c));
            }
        }
        for a in 1..entries.len() {
            let key = entries[a];
            let mut j = a;
            while j > 0 && key.0 > entries[j - 1].0 {
                entries[j] = entries[j - 1];
                j -= 1;
            }
            entries[j] = key;
        }
        for (a, &(_, orig)) in entries.iter().enumerate() {
            node.lx[a] = saved.lx[orig];
            node.hx[a] = saved.hx[orig];
            node.ly[a] = saved.ly[orig];
            node.hy[a] = saved.hy[orig];
            node.lz[a] = saved.lz[orig];
            node.hz[a] = saved.hz[orig];
            node.data[a] = saved.data[orig];
        }
        for a in entries.len()..4 {
            node.lx[a] = SimdTreeNode::empty_min();
            node.hx[a] = SimdTreeNode::empty_max();
            node.ly[a] = SimdTreeNode::empty_min();
            node.hy[a] = SimdTreeNode::empty_max();
            node.lz[a] = SimdTreeNode::empty_min();
            node.hz[a] = SimdTreeNode::empty_max();
            node.data[a] = if node.is_leaf { 0xFFFF_FFFF } else { 0 };
        }
    }

    let num_section_bits = bit_width(sections.len() as i64 - 1);
    let last = sections.len() - 1;
    let out_sections: Vec<GeometrySection> = sections
        .iter()
        .enumerate()
        .map(|(i, s)| GeometrySection {
            bvh: s.bvh.clone(),
            primitives: s.primitives.clone(),
            vertices: s.quantized.clone(),
            interior_primitive_bits: s.interior_bits.clone(),
            vertex_buffer_size: s.quantized.len() * 6
                + if i == last && !s.from_leaf_group {
                    2
                } else {
                    0
                },
            section_offset: s.section_offset,
            bit_scale8_inv: s.bit_scale8_inv,
            bit_offset: s.bit_offset,
        })
        .collect();
    Some(MeshShape {
        shape_type: MeshShape::TYPE_MESH,
        dispatch_type: MeshShape::DISPATCH_COMPOSITE,
        flags: MeshShape::FLAGS_DEFAULT,
        num_shape_key_bits: (num_section_bits + num_prim_bits + 1) as u8,
        convex_radius: 0.0,
        user_data: 0,
        shape_tag_codec_info: 0xFFFF_FFFF,
        bit_scale16: conv.bit_scale16,
        bit_scale16_inv: conv.bit_scale16_inv,
        shape_tag_table,
        top_level_tree: tree,
        top_level_tree_is_compact: true,
        sections: out_sections,
        primitive_mapping: None,
    })
}
