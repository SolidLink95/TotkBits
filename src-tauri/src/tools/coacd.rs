//! Approximate convex decomposition through CoACD's fault-tolerant C API.
//!
//! CoACD (Wei et al., MIT) splits a triangle mesh into convex pieces that a
//! Phive Polytope list can hold. `bin/dlls/lib_coacd.dll` is the release
//! build of the CoACD fork (https://github.com/SolidLink95/CoACD, downloaded
//! by `repo_init.py`), whose `CoACD_runSafe` validates its input and turns
//! every C++ exception,
//! allocation failure and (on MSVC) hardware fault into a status code and a
//! message, so a failing decomposition is an `io::Error` instead of a dead
//! process. Without the DLL the decomposition reports `NotFound` and nothing
//! else is affected.
//!
//! Two more layers keep the app alive whatever the library does:
//!
//! * The app never calls the DLL on its own threads. [`decompose`] runs
//!   `Totkbits.exe --cli coacd_decompose` as a child process, so a fault the
//!   library cannot intercept (a TBB worker thread, heap corruption, an
//!   exhausted stack, an endless loop the user kills) only takes that child
//!   with it and comes back as an error message.
//! * Wherever the DLL is loaded, it is loaded once and never unloaded:
//!   unloading it while its worker threads still exist crashed.
//!
//! The mesh is validated on the Rust side too (finite coordinates, in-range
//! indices, a positive extent), so the obvious garbage never reaches C++.

use std::{
    ffi::CStr,
    fs, io,
    os::raw::{c_char, c_double, c_int, c_uint, c_void},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, Ordering},
        OnceLock,
    },
};

use serde::{Deserialize, Serialize};

use super::convex_hull::Point;

/// Set in the worker process (and honoured anywhere) to run the DLL on the
/// calling thread instead of in a child process.
pub const IN_PROCESS_ENV: &str = "TOTKBITS_COACD_IN_PROCESS";

/// The CoACD parameters the items creator exposes (the rest keep CoACD's
/// defaults).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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
    /// Wall-clock budget for one decomposition; 0 means none. The library
    /// checks it between its search and merge steps, so no mesh can keep
    /// it running forever.
    pub time_limit_seconds: f64,
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
            time_limit_seconds: 0.0,
        }
    }
}

/// One convex piece: the hull's vertices and outward triangles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConvexPiece {
    pub vertices: Vec<Point>,
    pub triangles: Vec<[u32; 3]>,
}

/// What the parent hands the worker process (`--cli coacd_decompose -i`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecomposeRequest {
    pub vertices: Vec<Point>,
    pub triangles: Vec<[u32; 3]>,
    #[serde(default)]
    pub options: CoacdOptions,
}

/// What the worker process writes back (`-o`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecomposeResponse {
    pub pieces: Vec<ConvexPiece>,
}

// ---------------------------------------------------------------------------
// C ABI (public/coacd.h of the fork)
// ---------------------------------------------------------------------------

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

#[repr(C)]
#[derive(Clone, Copy)]
struct CoacdParams {
    threshold: c_double,
    max_convex_hull: c_int,
    preprocess_mode: c_int,
    prep_resolution: c_int,
    sample_resolution: c_int,
    mcts_nodes: c_int,
    mcts_iteration: c_int,
    mcts_max_depth: c_int,
    pca: c_int,
    merge: c_int,
    decimate: c_int,
    max_ch_vertex: c_int,
    extrude: c_int,
    extrude_margin: c_double,
    apx_mode: c_int,
    seed: c_uint,
    real_metric: c_int,
    time_limit_seconds: c_double,
}

const STATUS_OK: c_int = 0;
const STATUS_INVALID_ARGUMENT: c_int = 1;
const STATUS_BAD_ALLOC: c_int = 2;
const STATUS_HARDWARE_FAULT: c_int = 4;
const STATUS_TIMEOUT: c_int = 7;
const PREPROCESS_ON: c_int = 1;
const PREPROCESS_OFF: c_int = 2;

type RunSafeFn = unsafe extern "C" fn(
    input: *const CoacdMesh,
    params: *const CoacdParams,
    output: *mut CoacdMeshArray,
    message: *mut c_char,
    message_capacity: u64,
) -> c_int;
type FreeFn = unsafe extern "C" fn(arr: CoacdMeshArray);
type DefaultParamsFn = unsafe extern "C" fn(params: *mut CoacdParams);
type StatusNameFn = unsafe extern "C" fn(status: c_int) -> *const c_char;
type VersionFn = unsafe extern "C" fn() -> *const c_char;
type LogLevelFn = unsafe extern "C" fn(level: *const c_char);
type LogCallback = unsafe extern "C" fn(level: c_int, message: *const c_char, user: *mut c_void);
type SetLogCallbackFn = unsafe extern "C" fn(callback: Option<LogCallback>, user: *mut c_void);

/// The loaded DLL and its entry points. Lives for the rest of the process.
struct Api {
    _library: libloading::Library,
    run_safe: RunSafeFn,
    free: FreeFn,
    default_params: DefaultParamsFn,
    status_name: StatusNameFn,
    version: VersionFn,
}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

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

/// The library's own version string (`"1.0.14-safe"` for the fork build).
pub fn version() -> io::Result<String> {
    let api = api()?;
    Ok(unsafe { CStr::from_ptr((api.version)()) }
        .to_string_lossy()
        .into_owned())
}

/// Relays CoACD's log to stderr. Runs on the library's threads, so it must
/// never panic or throw: a panic across the C boundary would abort.
unsafe extern "C" fn log_sink(level: c_int, message: *const c_char, _user: *mut c_void) {
    let _ = std::panic::catch_unwind(|| {
        if message.is_null() {
            return;
        }
        let text = CStr::from_ptr(message).to_string_lossy();
        let level = match level {
            0 | 1 => "debug",
            2 => "info",
            3 => "warn",
            4 => "error",
            _ => "critical",
        };
        use io::Write;
        let _ = writeln!(io::stderr(), "[CoACD {level}] {text}");
    });
}

fn api() -> io::Result<&'static Api> {
    API.get_or_init(load)
        .as_ref()
        .map_err(|error| io::Error::new(io::ErrorKind::NotFound, error.clone()))
}

fn load() -> Result<Api, String> {
    let path = dll_path().ok_or_else(|| {
        "CoACD not found (expected bin/dlls/lib_coacd.dll; run repo_init.py)".to_owned()
    })?;
    let library = unsafe { libloading::Library::new(&path) }
        .map_err(|error| format!("unable to load CoACD {}: {error}", path.display()))?;
    let symbol = |name: &[u8]| -> Result<*const c_void, String> {
        unsafe { library.get::<*const c_void>(name) }
            .map(|symbol| *symbol)
            .map_err(|_| {
                format!(
                    "{} is an upstream CoACD build without {}; run repo_init.py to download the fork release",
                    path.display(),
                    String::from_utf8_lossy(&name[..name.len() - 1])
                )
            })
    };
    // SAFETY: the symbol types mirror public/coacd.h of the fork, and the
    // library is never unloaded, so the pointers stay valid.
    let api = unsafe {
        let run_safe: RunSafeFn = std::mem::transmute(symbol(b"CoACD_runSafe\0")?);
        let free: FreeFn = std::mem::transmute(symbol(b"CoACD_freeMeshArray\0")?);
        let default_params: DefaultParamsFn =
            std::mem::transmute(symbol(b"CoACD_defaultParams\0")?);
        let status_name: StatusNameFn = std::mem::transmute(symbol(b"CoACD_statusName\0")?);
        let version: VersionFn = std::mem::transmute(symbol(b"CoACD_version\0")?);
        let set_log_callback: SetLogCallbackFn =
            std::mem::transmute(symbol(b"CoACD_setLogCallback\0")?);
        let set_log_level: LogLevelFn = std::mem::transmute(symbol(b"CoACD_setLogLevel\0")?);
        set_log_callback(Some(log_sink), std::ptr::null_mut());
        set_log_level(b"warn\0".as_ptr() as *const c_char);
        Api {
            _library: library,
            run_safe,
            free,
            default_params,
            status_name,
            version,
        }
    };
    Ok(api)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The checks `CoACD_runSafe` repeats, done here first so garbage never
/// crosses the FFI boundary or spawns a worker process.
pub fn validate(vertices: &[Point], triangles: &[[u32; 3]]) -> io::Result<()> {
    if vertices.is_empty() {
        return Err(invalid("the collision mesh has no vertices"));
    }
    if triangles.is_empty() {
        return Err(invalid("the collision mesh has no triangles"));
    }
    if vertices.len() > c_int::MAX as usize / 3 || triangles.len() > c_int::MAX as usize / 3 {
        return Err(invalid(format!(
            "the collision mesh is too large ({} vertices, {} triangles)",
            vertices.len(),
            triangles.len()
        )));
    }
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for (index, vertex) in vertices.iter().enumerate() {
        for axis in 0..3 {
            let value = vertex[axis];
            if !value.is_finite() {
                return Err(invalid(format!(
                    "collision mesh vertex {index} has a non-finite coordinate ({value})"
                )));
            }
            min[axis] = min[axis].min(value);
            max[axis] = max[axis].max(value);
        }
    }
    let extent = (0..3).map(|axis| max[axis] - min[axis]).fold(0.0, f64::max);
    if !extent.is_finite() {
        return Err(invalid("the collision mesh's bounding box overflows"));
    }
    if extent <= 0.0 {
        return Err(invalid("every vertex of the collision mesh coincides"));
    }
    let mut degenerate = 0usize;
    for (index, triangle) in triangles.iter().enumerate() {
        for &corner in triangle {
            if corner as usize >= vertices.len() {
                return Err(invalid(format!(
                    "collision mesh triangle {index} references vertex {corner}, the mesh has {}",
                    vertices.len()
                )));
            }
        }
        if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[0] == triangle[2] {
            degenerate += 1;
        }
    }
    if degenerate == triangles.len() {
        return Err(invalid(
            "every triangle of the collision mesh is degenerate",
        ));
    }
    Ok(())
}

fn validate_options(options: &CoacdOptions) -> io::Result<()> {
    if !(options.threshold.is_finite() && (0.001..=1.0).contains(&options.threshold)) {
        return Err(invalid(format!(
            "CoACD threshold {} is outside 0.001-1",
            options.threshold
        )));
    }
    if options.max_convex_hulls != -1 && options.max_convex_hulls < 1 {
        return Err(invalid("CoACD max_convex_hulls must be -1 or at least 1"));
    }
    if !(5..=1000).contains(&options.preprocess_resolution) {
        return Err(invalid(format!(
            "CoACD preprocess_resolution {} is outside 5-1000",
            options.preprocess_resolution
        )));
    }
    if options.sample_resolution < 1 {
        return Err(invalid("CoACD sample_resolution must be at least 1"));
    }
    if !(options.time_limit_seconds.is_finite() && options.time_limit_seconds >= 0.0) {
        return Err(invalid(
            "CoACD time_limit_seconds must be 0 or a positive number",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Decomposition
// ---------------------------------------------------------------------------

/// Decomposes the triangle mesh into convex pieces. Blocks for the whole
/// run (a few thousand triangles take minutes at fine thresholds).
///
/// Runs in a worker process when called from the app binary; in-process
/// inside that worker, under `cargo test`, and when [`IN_PROCESS_ENV`] is
/// set.
pub fn decompose(
    vertices: &[Point],
    triangles: &[[u32; 3]],
    options: &CoacdOptions,
) -> io::Result<Vec<ConvexPiece>> {
    validate(vertices, triangles)?;
    validate_options(options)?;
    if !is_available() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "CoACD not found (expected bin/dlls/lib_coacd.dll; run repo_init.py)",
        ));
    }
    match worker_executable() {
        Some(exe) => decompose_isolated(&exe, vertices, triangles, options),
        None => decompose_in_process(vertices, triangles, options),
    }
}

/// The app binary when the decomposition should run in a child process.
fn worker_executable() -> Option<PathBuf> {
    if cfg!(test) || std::env::var_os(IN_PROCESS_ENV).is_some() {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let stem = exe.file_stem()?.to_str()?;
    stem.eq_ignore_ascii_case("totkbits").then_some(exe)
}

/// Calls the DLL on the current thread.
pub fn decompose_in_process(
    vertices: &[Point],
    triangles: &[[u32; 3]],
    options: &CoacdOptions,
) -> io::Result<Vec<ConvexPiece>> {
    validate(vertices, triangles)?;
    validate_options(options)?;
    let api = api()?;
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
    // SAFETY: the buffers outlive the call, the structs mirror the C header,
    // and the output array is released with the library's own free.
    unsafe {
        let mut params = std::mem::zeroed::<CoacdParams>();
        (api.default_params)(&mut params);
        params.threshold = options.threshold;
        params.max_convex_hull = options.max_convex_hulls;
        params.preprocess_mode = if options.preprocess {
            PREPROCESS_ON
        } else {
            PREPROCESS_OFF
        };
        params.prep_resolution = options.preprocess_resolution;
        params.sample_resolution = options.sample_resolution;
        params.merge = c_int::from(options.merge);
        params.seed = options.seed;
        params.time_limit_seconds = options.time_limit_seconds;

        let mut array = CoacdMeshArray {
            meshes_ptr: std::ptr::null_mut(),
            meshes_count: 0,
        };
        let mut message = vec![0u8; 1024];
        let status = (api.run_safe)(
            &input,
            &params,
            &mut array,
            message.as_mut_ptr() as *mut c_char,
            message.len() as u64,
        );
        if status != STATUS_OK {
            (api.free)(array);
            let name = CStr::from_ptr((api.status_name)(status)).to_string_lossy();
            let text = CStr::from_ptr(message.as_ptr() as *const c_char).to_string_lossy();
            let kind = match status {
                STATUS_INVALID_ARGUMENT => io::ErrorKind::InvalidData,
                STATUS_BAD_ALLOC => io::ErrorKind::OutOfMemory,
                STATUS_TIMEOUT => io::ErrorKind::TimedOut,
                _ => io::ErrorKind::Other,
            };
            let detail = if status == STATUS_HARDWARE_FAULT {
                format!("CoACD {name}: {text} (the decomposition was aborted; the mesh may be unsuitable)")
            } else {
                format!("CoACD {name}: {text}")
            };
            return Err(io::Error::new(kind, detail));
        }
        let mut pieces = Vec::with_capacity(array.meshes_count as usize);
        let mut skipped = 0usize;
        if !array.meshes_ptr.is_null() {
            for mesh in std::slice::from_raw_parts(array.meshes_ptr, array.meshes_count as usize) {
                if mesh.vertices_ptr.is_null()
                    || mesh.triangles_ptr.is_null()
                    || mesh.vertices_count < 4
                    || mesh.triangles_count < 4
                {
                    skipped += 1;
                    continue;
                }
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
        (api.free)(array);
        if skipped > 0 {
            eprintln!("[CoACD] skipped {skipped} degenerate piece(s) with fewer than 4 vertices");
        }
        if pieces.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CoACD produced no usable convex pieces",
            ));
        }
        Ok(pieces)
    }
}

static WORKER_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Runs the decomposition in `Totkbits.exe --cli coacd_decompose`, so
/// nothing the library does can take the app down.
fn decompose_isolated(
    exe: &Path,
    vertices: &[Point],
    triangles: &[[u32; 3]],
    options: &CoacdOptions,
) -> io::Result<Vec<ConvexPiece>> {
    let scratch = std::env::temp_dir().join(format!(
        "totkbits_coacd_{}_{}",
        std::process::id(),
        WORKER_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&scratch)?;
    let result = run_worker(exe, &scratch, vertices, triangles, options);
    let _ = fs::remove_dir_all(&scratch);
    result
}

fn run_worker(
    exe: &Path,
    scratch: &Path,
    vertices: &[Point],
    triangles: &[[u32; 3]],
    options: &CoacdOptions,
) -> io::Result<Vec<ConvexPiece>> {
    let request_path = scratch.join("request.json");
    let response_path = scratch.join("pieces.json");
    let request = DecomposeRequest {
        vertices: vertices.to_vec(),
        triangles: triangles.to_vec(),
        options: *options,
    };
    fs::write(&request_path, serde_json::to_vec(&request)?)?;

    let output = crate::utils::hidden_command(exe)
        .args(["--cli", "coacd_decompose", "-i"])
        .arg(&request_path)
        .arg("-o")
        .arg(&response_path)
        .env(IN_PROCESS_ENV, "1")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "unable to start the CoACD worker {}: {error}",
                    exe.display()
                ),
            )
        })?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stderr.lines().filter(|line| !line.trim().is_empty()) {
        eprintln!("[CoACD worker] {line}");
    }
    let last_line = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .trim_start_matches("error: ")
        .to_owned();

    if output.status.success() {
        let bytes = fs::read(&response_path).map_err(|error| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("the CoACD worker exited without writing its result: {error}"),
            )
        })?;
        let response: DecomposeResponse = serde_json::from_slice(&bytes)?;
        if response.pieces.is_empty() {
            return Err(invalid("CoACD produced no usable convex pieces"));
        }
        return Ok(response.pieces);
    }
    let code = output.status.code();
    let detail = match code {
        // The CLI's own failure: the message is the last stderr line.
        Some(1) if !last_line.is_empty() => last_line,
        Some(code) => format!(
            "the CoACD worker process crashed (exit code 0x{:08X}){}",
            code as u32,
            if last_line.is_empty() {
                String::new()
            } else {
                format!(": {last_line}")
            }
        ),
        None => "the CoACD worker process was terminated".to_owned(),
    };
    Err(io::Error::new(io::ErrorKind::Other, detail))
}

// ---------------------------------------------------------------------------
// OBJ output for the CLI
// ---------------------------------------------------------------------------

/// Writes the pieces as one OBJ with a group per piece.
pub fn pieces_to_obj(pieces: &[ConvexPiece]) -> String {
    let mut text = String::new();
    let mut offset = 1usize;
    for (index, piece) in pieces.iter().enumerate() {
        text.push_str(&format!("g piece_{index}\n"));
        for v in &piece.vertices {
            text.push_str(&format!("v {} {} {}\n", v[0], v[1], v[2]));
        }
        for t in &piece.triangles {
            text.push_str(&format!(
                "f {} {} {}\n",
                t[0] as usize + offset,
                t[1] as usize + offset,
                t[2] as usize + offset
            ));
        }
        offset += piece.vertices.len();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(scale: f64) -> (Vec<Point>, Vec<[u32; 3]>) {
        let s = scale;
        let vertices = vec![
            [-s, -s, -s],
            [s, -s, -s],
            [s, s, -s],
            [-s, s, -s],
            [-s, -s, s],
            [s, -s, s],
            [s, s, s],
            [-s, s, s],
        ];
        let triangles = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [2, 3, 7],
            [2, 7, 6],
            [1, 2, 6],
            [1, 6, 5],
            [0, 4, 7],
            [0, 7, 3],
        ];
        (vertices, triangles)
    }

    fn options(preprocess: bool) -> CoacdOptions {
        CoacdOptions {
            threshold: 0.04,
            max_convex_hulls: 40,
            preprocess,
            preprocess_resolution: 60,
            // Non-manifold inputs can take minutes; the point of the tests is
            // survival, not the result.
            time_limit_seconds: 20.0,
            ..CoacdOptions::default()
        }
    }

    /// A cube with one face missing takes about 90 s to decompose; a one
    /// second budget must stop it with a timeout, not a hang.
    #[test]
    fn time_limit_stops_a_long_decomposition() {
        if !is_available() {
            eprintln!("lib_coacd.dll missing; skipping");
            return;
        }
        let (vertices, triangles) = cube(1.0);
        let options = CoacdOptions {
            time_limit_seconds: 1.0,
            ..options(true)
        };
        let started = std::time::Instant::now();
        let error = decompose(&vertices, &triangles[..10], &options).expect_err("timeout");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut, "{error}");
        assert!(started.elapsed().as_secs() < 30, "{:?}", started.elapsed());
    }

    #[test]
    fn garbage_is_rejected_before_the_ffi_boundary() {
        let (vertices, triangles) = cube(1.0);
        assert!(validate(&[], &triangles).is_err());
        assert!(validate(&vertices, &[]).is_err());
        assert!(validate(&vertices, &[[0, 1, 99]]).is_err());
        assert!(validate(&[[0.0; 3]; 8], &triangles).is_err());
        let mut nan = vertices.clone();
        nan[0][0] = f64::NAN;
        assert!(validate(&nan, &triangles).is_err());
        let mut inf = vertices.clone();
        inf[3][1] = f64::INFINITY;
        assert!(validate(&inf, &triangles).is_err());
        assert!(validate(&vertices, &[[1, 1, 1]; 3]).is_err());
        assert!(validate(&vertices, &triangles).is_ok());
        assert!(validate_options(&CoacdOptions {
            threshold: 0.0,
            ..CoacdOptions::default()
        })
        .is_err());
        assert!(validate_options(&CoacdOptions {
            preprocess_resolution: 0,
            ..CoacdOptions::default()
        })
        .is_err());
    }

    #[test]
    fn cube_is_one_piece() {
        if !is_available() {
            eprintln!("lib_coacd.dll missing; skipping");
            return;
        }
        let (vertices, triangles) = cube(1.0);
        let pieces = decompose(&vertices, &triangles, &options(false)).expect("cube");
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].vertices.len(), 8);
        // Manifold preprocessing remeshes the cube (56 vertices), one piece still.
        let pieces = decompose(&vertices, &triangles, &options(true)).expect("cube");
        assert_eq!(pieces.len(), 1);
        assert!(pieces[0].vertices.len() >= 8);
        assert!(version().unwrap().contains("safe"));
    }

    /// A stand-in worker (a batch file) that fails the way a crashed or
    /// erroring `Totkbits.exe --cli coacd_decompose` would.
    fn fake_worker(name: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("totkbits_coacd_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.bat"));
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn worker_crash_becomes_an_error() {
        let (vertices, triangles) = cube(1.0);
        let exe = fake_worker(
            "crash",
            "@echo off\r\necho faulting >&2\r\nexit /b 3221225477\r\n",
        );
        let error = decompose_isolated(&exe, &vertices, &triangles, &CoacdOptions::default())
            .expect_err("a crashed worker is an error");
        let text = error.to_string();
        assert!(text.contains("crashed"), "{text}");
        assert!(text.contains("0xC0000005"), "{text}");
        assert!(text.contains("faulting"), "{text}");
    }

    #[test]
    fn worker_error_message_is_relayed() {
        let (vertices, triangles) = cube(1.0);
        let exe = fake_worker(
            "fail",
            "@echo off\r\necho CoACD invalid argument: the mesh is bad >&2\r\nexit /b 1\r\n",
        );
        let error = decompose_isolated(&exe, &vertices, &triangles, &CoacdOptions::default())
            .expect_err("a failed worker is an error");
        assert_eq!(error.to_string(), "CoACD invalid argument: the mesh is bad");
    }

    #[test]
    fn worker_without_output_is_an_error() {
        let (vertices, triangles) = cube(1.0);
        let exe = fake_worker("silent", "@echo off\r\nexit /b 0\r\n");
        let error = decompose_isolated(&exe, &vertices, &triangles, &CoacdOptions::default())
            .expect_err("a worker that wrote nothing is an error");
        assert!(error.to_string().contains("without writing"), "{error}");
    }

    /// Meshes that crashed or hung the upstream build: every one must come
    /// back as `Ok` or `Err`, and the process must survive all of them.
    #[test]
    fn hostile_meshes_do_not_crash() {
        if !is_available() {
            eprintln!("lib_coacd.dll missing; skipping");
            return;
        }
        let (cv, ct) = cube(1.0);
        let triangle: (Vec<Point>, Vec<[u32; 3]>) = (
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        );
        let missing_face = (cv.clone(), ct[..10].to_vec());
        let mut two_cubes = cube(1.0);
        two_cubes
            .0
            .extend(cv.iter().map(|p| [p[0] + 0.7, p[1] + 0.7, p[2] + 0.7]));
        two_cubes
            .1
            .extend(ct.iter().map(|t| [t[0] + 8, t[1] + 8, t[2] + 8]));
        let mut duplicate_faces = cube(1.0);
        duplicate_faces.1.extend(ct.iter().copied());
        let cases: Vec<(&str, (Vec<Point>, Vec<[u32; 3]>), bool)> = vec![
            ("open triangle, preprocess off", triangle.clone(), false),
            ("open triangle", triangle, true),
            ("missing face, preprocess off", missing_face.clone(), false),
            ("missing face", missing_face, true),
            (
                "two intersecting cubes, preprocess off",
                two_cubes.clone(),
                false,
            ),
            ("two intersecting cubes", two_cubes, true),
            ("duplicate faces, preprocess off", duplicate_faces, false),
            ("huge scale", cube(1e200), true),
            ("tiny scale", cube(1e-200), true),
        ];
        for (name, (vertices, triangles), preprocess) in cases {
            let outcome = decompose(&vertices, &triangles, &options(preprocess));
            match &outcome {
                Ok(pieces) => eprintln!("{name}: {} piece(s)", pieces.len()),
                Err(error) => eprintln!("{name}: error: {error}"),
            }
        }
        // And the library still works afterwards.
        let pieces = decompose(&cv, &ct, &options(true)).expect("cube after hostile input");
        assert_eq!(pieces.len(), 1);
    }
}
