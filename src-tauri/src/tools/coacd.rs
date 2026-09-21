//! Approximate convex decomposition through CoACD's C API.
//!
//! CoACD (Wei et al., MIT) splits a triangle mesh into convex pieces that a
//! Phive Polytope list can hold. Its prebuilt `lib_coacd.dll` (taken out of
//! the CoACD release wheel by `repo_init.py`) ships as `bin/dlls/lib_coacd.dll`
//! and is loaded on demand, like the other native sidecars; without it the
//! decomposition reports `NotFound` and nothing else is affected.

use std::{
    ffi::CString,
    io,
    os::raw::{c_char, c_double, c_int, c_uint},
    path::{Path, PathBuf},
};

use super::convex_hull::Point;

/// The CoACD parameters the items creator exposes (the rest keep CoACD's
/// defaults).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoacdOptions {
    /// Concavity threshold (0.01 to 1): lower splits finer.
    pub threshold: f64,
    /// Upper bound on the piece count (-1 for none).
    pub max_convex_hulls: i32,
    /// Manifold preprocessing: needed for meshes that are not watertight.
    pub preprocess: bool,
    /// Voxel resolution of that preprocessing.
    pub preprocess_resolution: i32,
    /// Sample resolution of the Hausdorff distance.
    pub sample_resolution: i32,
    /// Merge pieces after the tree search.
    pub merge: bool,
    pub seed: u32,
}

impl Default for CoacdOptions {
    fn default() -> Self {
        Self {
            threshold: 0.05,
            max_convex_hulls: -1,
            preprocess: true,
            preprocess_resolution: 50,
            sample_resolution: 2000,
            merge: true,
            seed: 0,
        }
    }
}

/// One convex piece: the hull's vertices and outward triangles.
#[derive(Clone, Debug, PartialEq)]
pub struct ConvexPiece {
    pub vertices: Vec<Point>,
    pub triangles: Vec<[u32; 3]>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CoacdMesh {
    vertices_ptr: *mut c_double,
    vertices_count: u64,
    triangles_ptr: *mut c_int,
    triangles_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CoacdMeshArray {
    meshes_ptr: *mut CoacdMesh,
    meshes_count: u64,
}

const PREPROCESS_ON: c_int = 1;
const PREPROCESS_OFF: c_int = 2;
const APX_CONVEX_HULL: c_int = 0;

type RunFn = unsafe extern "C" fn(
    input: *const CoacdMesh,
    threshold: c_double,
    max_convex_hull: c_int,
    preprocess_mode: c_int,
    prep_resolution: c_int,
    sample_resolution: c_int,
    mcts_nodes: c_int,
    mcts_iteration: c_int,
    mcts_max_depth: c_int,
    pca: bool,
    merge: bool,
    decimate: bool,
    max_ch_vertex: c_int,
    extrude: bool,
    extrude_margin: c_double,
    apx_mode: c_int,
    seed: c_uint,
    real_metric: bool,
) -> CoacdMeshArray;
type FreeFn = unsafe extern "C" fn(arr: CoacdMeshArray);
type LogFn = unsafe extern "C" fn(level: *const c_char);

/// `bin/dlls/lib_coacd.dll` next to the executable (or in the source tree
/// for `cargo test` / `cargo run`).
pub fn dll_path() -> Option<PathBuf> {
    let relative = Path::new("bin/dlls/lib_coacd.dll");
    let mut candidates = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)];
    if let Ok(exe_dir) = crate::utils::running_exe_dir() {
        candidates.push(exe_dir.join(relative));
        candidates.push(exe_dir.join("dlls/lib_coacd.dll"));
    }
    candidates.into_iter().find(|path| path.is_file())
}

pub fn is_available() -> bool {
    dll_path().is_some()
}

/// Decomposes the triangle mesh into convex pieces. Blocks for the whole
/// run (a few thousand triangles take minutes at fine thresholds).
pub fn decompose(
    vertices: &[Point],
    triangles: &[[u32; 3]],
    options: &CoacdOptions,
) -> io::Result<Vec<ConvexPiece>> {
    if vertices.is_empty() || triangles.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the collision mesh has no triangles",
        ));
    }
    for triangle in triangles {
        if triangle
            .iter()
            .any(|&index| index as usize >= vertices.len())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the collision mesh indexes a vertex it does not have",
            ));
        }
    }
    let path = dll_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "CoACD not found (expected bin/dlls/lib_coacd.dll; run repo_init.py)",
        )
    })?;
    let library = unsafe { libloading::Library::new(&path) }.map_err(|error| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("unable to load CoACD {}: {error}", path.display()),
        )
    })?;
    let mut flat_vertices: Vec<c_double> = vertices.iter().flatten().copied().collect();
    let mut flat_triangles: Vec<c_int> = triangles
        .iter()
        .flatten()
        .map(|&index| index as c_int)
        .collect();
    let input = CoacdMesh {
        vertices_ptr: flat_vertices.as_mut_ptr(),
        vertices_count: vertices.len() as u64,
        triangles_ptr: flat_triangles.as_mut_ptr(),
        triangles_count: triangles.len() as u64,
    };
    unsafe {
        let run: libloading::Symbol<RunFn> =
            library.get(b"CoACD_run\0").map_err(io::Error::other)?;
        let free: libloading::Symbol<FreeFn> = library
            .get(b"CoACD_freeMeshArray\0")
            .map_err(io::Error::other)?;
        if let Ok(set_level) = library.get::<LogFn>(b"CoACD_setLogLevel\0") {
            let level = CString::new("warn").expect("static level");
            set_level(level.as_ptr());
        }
        let array = run(
            &input,
            options.threshold,
            options.max_convex_hulls,
            if options.preprocess {
                PREPROCESS_ON
            } else {
                PREPROCESS_OFF
            },
            options.preprocess_resolution,
            options.sample_resolution,
            20,
            150,
            3,
            false,
            options.merge,
            false,
            256,
            false,
            0.01,
            APX_CONVEX_HULL,
            options.seed,
            false,
        );
        let mut pieces = Vec::with_capacity(array.meshes_count as usize);
        if !array.meshes_ptr.is_null() {
            for mesh in std::slice::from_raw_parts(array.meshes_ptr, array.meshes_count as usize) {
                let points =
                    std::slice::from_raw_parts(mesh.vertices_ptr, mesh.vertices_count as usize * 3);
                let indices = std::slice::from_raw_parts(
                    mesh.triangles_ptr,
                    mesh.triangles_count as usize * 3,
                );
                pieces.push(ConvexPiece {
                    vertices: points.chunks(3).map(|p| [p[0], p[1], p[2]]).collect(),
                    triangles: indices
                        .chunks(3)
                        .map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
                        .collect(),
                });
            }
        }
        free(array);
        if pieces.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CoACD produced no convex pieces",
            ));
        }
        Ok(pieces)
    }
}
