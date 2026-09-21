//! 3D convex hulls of small point sets: the incremental algorithm (visible
//! faces are replaced by a fan over the horizon), the mass properties a
//! Phive `AutoCalc` block holds, and vertex thinning for TOTK's Polytope
//! shapes, which vanilla keeps at a few dozen vertices per piece.
//!
//! The inputs are the convex pieces a decomposition returns (a few hundred
//! points each), so an O(n²) hull with a plane tolerance is enough; the
//! result is checked to be a closed manifold before it is used.

use std::collections::HashSet;

pub type Point = [f64; 3];

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: Point, s: f64) -> Point {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: Point) -> f64 {
    dot(a, a).sqrt()
}

/// Lexicographic order (x, then y, then z), the order Polytope vertices are
/// written in.
pub fn compare_points(a: &Point, b: &Point) -> std::cmp::Ordering {
    a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
}

#[derive(Clone, Copy, Debug)]
struct Face {
    vertices: [usize; 3],
    normal: Point,
    offset: f64,
}

impl Face {
    fn new(points: &[Point], vertices: [usize; 3]) -> Option<Self> {
        let [a, b, c] = vertices;
        let n = cross(sub(points[b], points[a]), sub(points[c], points[a]));
        let length = norm(n);
        if !(length > 0.0) {
            return None;
        }
        let normal = scale(n, 1.0 / length);
        Some(Self {
            vertices,
            normal,
            offset: dot(normal, points[a]),
        })
    }

    fn distance(&self, p: Point) -> f64 {
        dot(self.normal, p) - self.offset
    }
}

/// A convex hull over the points it was built from: `faces` are outward
/// oriented triangles indexing `points`; points not on the hull are kept in
/// `points` but referenced by no face.
#[derive(Clone, Debug)]
pub struct ConvexHull {
    pub points: Vec<Point>,
    pub faces: Vec<[usize; 3]>,
}

/// Unit-mass properties of a convex solid, what Phive's `AutoCalc` records.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MassProperties {
    pub volume: f64,
    pub center: Point,
    /// Diagonal of the inertia tensor about the center for unit mass.
    pub inertia: Point,
}

impl ConvexHull {
    /// The hull of `points`, or `None` when they are fewer than four,
    /// degenerate (all coplanar) or the result is not a closed surface.
    pub fn build(points: &[Point]) -> Option<Self> {
        if points.len() < 4 {
            return None;
        }
        let mut min = points[0];
        let mut max = points[0];
        for p in points {
            for axis in 0..3 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        let extent = norm(sub(max, min));
        if !(extent > 0.0) {
            return None;
        }
        // A tolerance too tight lets rounding produce a non-manifold horizon;
        // widen it step by step (merging nearly coplanar points) before giving up.
        for relative in [1e-10, 1e-8, 1e-6, 1e-4] {
            if let Some(hull) = Self::build_with_tolerance(points, extent * relative) {
                return Some(hull);
            }
        }
        None
    }

    fn build_with_tolerance(points: &[Point], eps: f64) -> Option<Self> {
        let n = points.len();
        // Initial simplex: the two extremes along x, the point farthest from
        // their line, then the point farthest from that plane.
        let i0 = (0..n)
            .min_by(|&a, &b| compare_points(&points[a], &points[b]))
            .unwrap();
        let i1 = (0..n)
            .max_by(|&a, &b| {
                norm(sub(points[a], points[i0]))
                    .partial_cmp(&norm(sub(points[b], points[i0])))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();
        let dir = sub(points[i1], points[i0]);
        if !(norm(dir) > eps) {
            return None;
        }
        let line_distance = |p: Point| norm(cross(sub(p, points[i0]), dir)) / norm(dir);
        let i2 = (0..n)
            .max_by(|&a, &b| {
                line_distance(points[a])
                    .partial_cmp(&line_distance(points[b]))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();
        if !(line_distance(points[i2]) > eps) {
            return None;
        }
        let base = Face::new(points, [i0, i1, i2])?;
        let i3 = (0..n)
            .max_by(|&a, &b| {
                base.distance(points[a])
                    .abs()
                    .partial_cmp(&base.distance(points[b]).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();
        if !(base.distance(points[i3]).abs() > eps) {
            return None;
        }
        let simplex = [i0, i1, i2, i3];
        let mut faces: Vec<Face> = Vec::new();
        for (skip, triangle) in [
            (3, [i0, i1, i2]),
            (2, [i0, i1, i3]),
            (1, [i0, i2, i3]),
            (0, [i1, i2, i3]),
        ] {
            let mut face = Face::new(points, triangle)?;
            if face.distance(points[simplex[skip]]) > 0.0 {
                face = Face::new(points, [triangle[0], triangle[2], triangle[1]])?;
            }
            faces.push(face);
        }
        for p in 0..n {
            if simplex.contains(&p) {
                continue;
            }
            let point = points[p];
            let visible: Vec<usize> = (0..faces.len())
                .filter(|&f| faces[f].distance(point) > eps)
                .collect();
            if visible.is_empty() {
                continue;
            }
            let mut visible_edges: HashSet<(usize, usize)> = HashSet::new();
            for &f in &visible {
                let [a, b, c] = faces[f].vertices;
                visible_edges.insert((a, b));
                visible_edges.insert((b, c));
                visible_edges.insert((c, a));
            }
            let mut horizon: Vec<(usize, usize)> = Vec::new();
            for &f in &visible {
                let [a, b, c] = faces[f].vertices;
                for edge in [(a, b), (b, c), (c, a)] {
                    if !visible_edges.contains(&(edge.1, edge.0)) {
                        horizon.push(edge);
                    }
                }
            }
            let mut keep = 0;
            for f in 0..faces.len() {
                if !visible.contains(&f) {
                    faces.swap(keep, f);
                    keep += 1;
                }
            }
            faces.truncate(keep);
            for (u, v) in horizon {
                faces.push(Face::new(points, [u, v, p])?);
            }
        }
        let hull = Self {
            points: points.to_vec(),
            faces: faces.iter().map(|face| face.vertices).collect(),
        };
        hull.is_closed().then_some(hull)
    }

    /// Every directed edge is matched by its reverse exactly once.
    fn is_closed(&self) -> bool {
        let mut edges: HashSet<(usize, usize)> = HashSet::new();
        for &[a, b, c] in &self.faces {
            for edge in [(a, b), (b, c), (c, a)] {
                if !edges.insert(edge) {
                    return false;
                }
            }
        }
        edges.iter().all(|&(a, b)| edges.contains(&(b, a)))
    }

    /// Indices of the points on the hull, ascending.
    pub fn vertex_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = self.faces.iter().flatten().copied().collect();
        indices.sort_unstable();
        indices.dedup();
        indices
    }

    /// The hull's vertices, in lexicographic order.
    pub fn vertices(&self) -> Vec<Point> {
        let mut vertices: Vec<Point> = self
            .vertex_indices()
            .into_iter()
            .map(|index| self.points[index])
            .collect();
        vertices.sort_by(compare_points);
        vertices
    }

    pub fn volume(&self) -> f64 {
        self.mass_properties().volume
    }

    /// Volume, centroid and unit-mass inertia diagonal, summed over the
    /// tetrahedra between the vertex mean and each face.
    pub fn mass_properties(&self) -> MassProperties {
        let indices = self.vertex_indices();
        let mut inner = [0.0; 3];
        for &index in &indices {
            inner = add(inner, self.points[index]);
        }
        let inner = scale(inner, 1.0 / indices.len().max(1) as f64);
        // Canonical tetrahedron covariance (Blow & Binstock).
        let canon = [
            [1.0 / 60.0, 1.0 / 120.0, 1.0 / 120.0],
            [1.0 / 120.0, 1.0 / 60.0, 1.0 / 120.0],
            [1.0 / 120.0, 1.0 / 120.0, 1.0 / 60.0],
        ];
        let mut volume = 0.0;
        let mut center = [0.0; 3];
        let mut covariance = [[0.0; 3]; 3];
        for &[ia, ib, ic] in &self.faces {
            let rows = [
                sub(self.points[ia], inner),
                sub(self.points[ib], inner),
                sub(self.points[ic], inner),
            ];
            let det = dot(rows[0], cross(rows[1], rows[2]));
            volume += det / 6.0;
            let sum = add(add(rows[0], rows[1]), rows[2]);
            center = add(center, scale(sum, det / 24.0));
            for i in 0..3 {
                for j in 0..3 {
                    let mut value = 0.0;
                    for k in 0..3 {
                        for l in 0..3 {
                            value += rows[k][i] * canon[k][l] * rows[l][j];
                        }
                    }
                    covariance[i][j] += det * value;
                }
            }
        }
        if !(volume > 0.0) {
            return MassProperties {
                volume: 0.0,
                center: inner,
                inertia: [0.0; 3],
            };
        }
        let center = scale(center, 1.0 / volume);
        for i in 0..3 {
            for j in 0..3 {
                covariance[i][j] -= volume * center[i] * center[j];
            }
        }
        let trace = covariance[0][0] + covariance[1][1] + covariance[2][2];
        let inertia = [
            (trace - covariance[0][0]) / volume,
            (trace - covariance[1][1]) / volume,
            (trace - covariance[2][2]) / volume,
        ];
        MassProperties {
            volume,
            center: add(center, inner),
            inertia,
        }
    }

    /// The vertices sharing a face with `vertex`.
    fn link(&self, vertex: usize) -> Vec<usize> {
        let mut link: Vec<usize> = self
            .faces
            .iter()
            .filter(|face| face.contains(&vertex))
            .flatten()
            .copied()
            .filter(|&other| other != vertex)
            .collect();
        link.sort_unstable();
        link.dedup();
        link
    }
}

fn hull_volume(points: &[Point]) -> f64 {
    ConvexHull::build(points).map_or(0.0, |hull| hull.volume())
}

/// The hull's vertices thinned to at most `max_vertices`: at each step the
/// vertex whose removal loses the least volume (the cap between its faces
/// and the hull of its neighbours) is dropped. Returns the remaining
/// vertices in lexicographic order, or `None` when the points span no volume.
pub fn thin_vertices(points: &[Point], max_vertices: usize) -> Option<Vec<Point>> {
    let mut hull = ConvexHull::build(points)?;
    let mut vertices = hull.vertices();
    while vertices.len() > max_vertices.max(4) {
        hull = ConvexHull::build(&vertices)?;
        let mut best: Option<(f64, usize)> = None;
        for index in 0..vertices.len() {
            let link: Vec<Point> = hull.link(index).into_iter().map(|i| vertices[i]).collect();
            let mut with_vertex = link.clone();
            with_vertex.push(vertices[index]);
            let loss = hull_volume(&with_vertex) - hull_volume(&link);
            if best.is_none_or(|(least, _)| loss < least) {
                best = Some((loss, index));
            }
        }
        let (_, index) = best?;
        vertices.remove(index);
    }
    let hull = ConvexHull::build(&vertices)?;
    Some(hull.vertices())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(size: f64) -> Vec<Point> {
        let mut points = Vec::new();
        for x in [0.0, size] {
            for y in [0.0, size] {
                for z in [0.0, size] {
                    points.push([x, y, z]);
                }
            }
        }
        points
    }

    #[test]
    fn cube_hull_has_eight_vertices_and_unit_volume() {
        let mut points = cube(2.0);
        points.push([1.0, 1.0, 1.0]); // interior
        points.push([1.0, 1.0, 0.0]); // on a face
        let hull = ConvexHull::build(&points).expect("hull");
        assert_eq!(hull.vertex_indices().len(), 8);
        assert_eq!(hull.faces.len(), 12);
        let mass = hull.mass_properties();
        assert!((mass.volume - 8.0).abs() < 1e-9, "{mass:?}");
        for axis in 0..3 {
            assert!((mass.center[axis] - 1.0).abs() < 1e-9, "{mass:?}");
            // Unit-mass cube of side 2: (2² + 2²) / 12.
            assert!((mass.inertia[axis] - 8.0 / 12.0).abs() < 1e-9, "{mass:?}");
        }
    }

    #[test]
    fn coplanar_points_have_no_hull() {
        let points = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ];
        assert!(ConvexHull::build(&points).is_none());
    }

    #[test]
    fn thinning_drops_the_cheapest_vertices_first() {
        let mut points = cube(2.0);
        // A tiny bump on one face costs the least volume.
        points.push([1.0, 1.0, 2.05]);
        let thinned = thin_vertices(&points, 8).expect("thinned");
        assert_eq!(thinned.len(), 8);
        assert!(!thinned.contains(&[1.0, 1.0, 2.05]));
        let thinned = thin_vertices(&points, 6).expect("thinned");
        assert_eq!(thinned.len(), 6);
        assert!(hull_volume(&thinned) > 5.0);
    }

    #[test]
    fn sphere_cloud_hull_is_closed() {
        let mut points = Vec::new();
        for i in 0..200u32 {
            let t = i as f64 * 0.618033988 * std::f64::consts::TAU;
            let z = 1.0 - 2.0 * (i as f64 + 0.5) / 200.0;
            let r = (1.0 - z * z).sqrt();
            points.push([r * t.cos(), r * t.sin(), z]);
        }
        let hull = ConvexHull::build(&points).expect("hull");
        assert_eq!(hull.vertex_indices().len(), 200);
        let volume = hull.volume();
        assert!(volume > 3.9 && volume < 4.19, "{volume}");
    }
}
