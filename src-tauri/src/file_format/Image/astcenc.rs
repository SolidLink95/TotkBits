//! ARM's astcenc through its shared library.
//!
//! `bin/dlls/astcenc-avx2-shared.dll` and `bin/dlls/astcenc-sse4.1-shared.dll`
//! are the upstream 5.7.0 codec built with `-DASTCENC_SHAREDLIB=ON` (see
//! `repo_init.py`); the AVX2 build is used when the CPU supports it, the
//! SSE4.1 build otherwise. The library is loaded once and never unloaded.
//!
//! Every encode is configured exactly like the command line
//! `astcenc -cs|-cl in.png out.astc WxH -thorough`: the LDR sRGB or LDR linear
//! profile, the THOROUGH preset, no flags, an RGBA swizzle, 8-bit input, and
//! one worker per core compressing its share of blocks. The blocks are byte
//! identical to the payload of the `.astc` file that executable writes.
use image::RgbaImage;
use std::ffi::{c_char, c_void, CStr};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Upstream release the DLLs are built from.
pub const VERSION: &str = "5.7.0";
/// Library names, most capable first.
pub const LIBRARY_NAMES: [&str; 2] = ["astcenc-avx2-shared.dll", "astcenc-sse4.1-shared.dll"];
/// Environment variable overriding the library location.
pub const LIBRARY_ENV: &str = "ASTCENC_DLL";
/// Size of one ASTC block in bytes, whatever the footprint.
pub const BLOCK_BYTES: usize = 16;

/// `ASTCENC_PRE_THOROUGH`.
const QUALITY_THOROUGH: f32 = 98.0;
/// `ASTCENC_PRF_LDR_SRGB`.
const PROFILE_LDR_SRGB: u32 = 0;
/// `ASTCENC_PRF_LDR`.
const PROFILE_LDR: u32 = 1;
/// `ASTCENC_TYPE_U8`.
const TYPE_U8: u32 = 0;
/// `ASTCENC_SUCCESS`.
const SUCCESS: i32 = 0;
/// The command line's default pre-encode swizzle (`rgba`).
const SWIZZLE_RGBA: Swizzle = Swizzle {
    r: 0,
    g: 1,
    b: 2,
    a: 3,
};

/// `struct astcenc_config` of astcenc 5.7.0 (without diagnostics).
#[repr(C)]
#[derive(Clone, Copy)]
struct Config {
    profile: u32,
    flags: u32,
    block_x: u32,
    block_y: u32,
    block_z: u32,
    cw_r_weight: f32,
    cw_g_weight: f32,
    cw_b_weight: f32,
    cw_a_weight: f32,
    a_scale_radius: u32,
    rgbm_m_scale: f32,
    tune_partition_count_limit: u32,
    tune_2partition_index_limit: u32,
    tune_3partition_index_limit: u32,
    tune_4partition_index_limit: u32,
    tune_block_mode_limit: u32,
    tune_refinement_limit: u32,
    tune_candidate_limit: u32,
    tune_2partitioning_candidate_limit: u32,
    tune_3partitioning_candidate_limit: u32,
    tune_4partitioning_candidate_limit: u32,
    tune_db_limit: f32,
    tune_mse_overshoot: f32,
    tune_2partition_early_out_limit_factor: f32,
    tune_3partition_early_out_limit_factor: f32,
    tune_2plane_early_out_limit_correlation: f32,
    tune_search_mode0_enable: f32,
    progress_callback: Option<unsafe extern "C" fn(f32)>,
}
const _: () = assert!(std::mem::size_of::<Config>() == 120);

/// `struct astcenc_image`.
#[repr(C)]
struct Image {
    dim_x: u32,
    dim_y: u32,
    dim_z: u32,
    data_type: u32,
    data: *mut *mut c_void,
}

/// `struct astcenc_swizzle`.
#[repr(C)]
struct Swizzle {
    r: u32,
    g: u32,
    b: u32,
    a: u32,
}

type ConfigInitFn = unsafe extern "C" fn(u32, u32, u32, u32, f32, u32, *mut Config) -> i32;
type ContextAllocFn =
    unsafe extern "C" fn(*const Config, u32, *mut *mut c_void, *const c_void) -> i32;
type CompressImageFn =
    unsafe extern "C" fn(*mut c_void, *mut Image, *const Swizzle, *mut u8, usize, u32) -> i32;
type CompressResetFn = unsafe extern "C" fn(*mut c_void) -> i32;
type ContextFreeFn = unsafe extern "C" fn(*mut c_void);
type ErrorStringFn = unsafe extern "C" fn(i32) -> *const c_char;

/// The loaded library and its entry points. Lives for the rest of the process.
struct Api {
    _library: libloading::Library,
    path: PathBuf,
    config_init: ConfigInitFn,
    context_alloc: ContextAllocFn,
    compress_image: CompressImageFn,
    compress_reset: CompressResetFn,
    context_free: ContextFreeFn,
    error_string: ErrorStringFn,
}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

/// Library names the running CPU can execute, most capable first.
fn supported_names() -> Vec<&'static str> {
    let mut names = Vec::new();
    if is_x86_feature_detected!("avx2") {
        names.push(LIBRARY_NAMES[0]);
    }
    if is_x86_feature_detected!("sse4.1") {
        names.push(LIBRARY_NAMES[1]);
    }
    names
}

/// The DLL that would be loaded: `ASTCENC_DLL` when set, else the first
/// supported build in `bin/dlls` next to the executable (or in the source
/// tree for `cargo test` / `cargo run`).
pub fn dll_path() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os(LIBRARY_ENV) {
        let path = PathBuf::from(value);
        return path.is_file().then_some(path);
    }
    let mut dirs = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("bin/dlls")];
    if let Ok(exe_dir) = crate::utils::running_exe_dir() {
        dirs.push(exe_dir.join("bin/dlls"));
        dirs.push(exe_dir.join("dlls"));
    }
    let names = supported_names();
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|path| path.is_file())
}

/// Whether an encoder can be used: already loaded, or present on disk.
pub fn is_available() -> bool {
    match API.get() {
        Some(loaded) => loaded.is_ok(),
        None => dll_path().is_some(),
    }
}

/// The error every caller reports when the library is missing.
pub fn missing_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "astcenc library not found (expected bin/dlls/{} or bin/dlls/{}; run repo_init.py, or set {})",
            LIBRARY_NAMES[0], LIBRARY_NAMES[1], LIBRARY_ENV
        ),
    )
}

/// The library in use once loaded.
pub fn library_path() -> Option<PathBuf> {
    API.get()
        .and_then(|api| api.as_ref().ok())
        .map(|api| api.path.clone())
}

/// Loads the encoder from an explicit DLL (the command line's `--astcenc`).
/// Fails when a different library was already loaded by this process.
pub fn use_library(path: &Path) -> io::Result<()> {
    if !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("astcenc library not found: {}", path.display()),
        ));
    }
    let api = API
        .get_or_init(|| load(path))
        .as_ref()
        .map_err(|error| io::Error::new(io::ErrorKind::NotFound, error.clone()))?;
    if api.path != path {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "astcenc is already loaded from {}; {} cannot replace it",
                api.path.display(),
                path.display()
            ),
        ));
    }
    Ok(())
}

fn api() -> io::Result<&'static Api> {
    API.get_or_init(|| {
        let path = dll_path().ok_or_else(|| missing_error().to_string())?;
        load(&path)
    })
    .as_ref()
    .map_err(|error| io::Error::new(io::ErrorKind::NotFound, error.clone()))
}

fn load(path: &Path) -> Result<Api, String> {
    let library = unsafe { libloading::Library::new(path) }
        .map_err(|error| format!("unable to load astcenc {}: {error}", path.display()))?;
    let symbol = |name: &[u8]| -> Result<*const c_void, String> {
        unsafe { library.get::<*const c_void>(name) }
            .map(|symbol| *symbol)
            .map_err(|_| {
                format!(
                    "{} does not export {}; expected an astcenc {VERSION} shared build",
                    path.display(),
                    String::from_utf8_lossy(&name[..name.len() - 1])
                )
            })
    };
    // SAFETY: the signatures mirror astcenc.h of the version the DLLs are
    // built from, and the library is never unloaded, so the pointers stay
    // valid.
    let api = unsafe {
        Api {
            config_init: std::mem::transmute::<*const c_void, ConfigInitFn>(symbol(
                b"astcenc_config_init\0",
            )?),
            context_alloc: std::mem::transmute::<*const c_void, ContextAllocFn>(symbol(
                b"astcenc_context_alloc\0",
            )?),
            compress_image: std::mem::transmute::<*const c_void, CompressImageFn>(symbol(
                b"astcenc_compress_image\0",
            )?),
            compress_reset: std::mem::transmute::<*const c_void, CompressResetFn>(symbol(
                b"astcenc_compress_reset\0",
            )?),
            context_free: std::mem::transmute::<*const c_void, ContextFreeFn>(symbol(
                b"astcenc_context_free\0",
            )?),
            error_string: std::mem::transmute::<*const c_void, ErrorStringFn>(symbol(
                b"astcenc_get_error_string\0",
            )?),
            path: path.to_path_buf(),
            _library: library,
        }
    };
    Ok(api)
}

impl Api {
    fn describe(&self, status: i32) -> String {
        // SAFETY: the library returns a pointer to a static string, or null.
        let text = unsafe { (self.error_string)(status) };
        if text.is_null() {
            format!("astcenc error {status}")
        } else {
            unsafe { CStr::from_ptr(text) }
                .to_string_lossy()
                .into_owned()
        }
    }

    fn failure(&self, what: &str, status: i32) -> io::Error {
        io::Error::other(format!("astcenc {what} failed: {}", self.describe(status)))
    }
}

// ---------------------------------------------------------------------------
// Contexts
// ---------------------------------------------------------------------------

/// Codec contexts are expensive to allocate (partition tables per block
/// footprint) and reusable after `astcenc_compress_reset`, so idle ones are
/// kept per configuration. A context serves one image at a time; concurrent
/// encodes take separate contexts.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ContextKey {
    profile: u32,
    block_width: u32,
    block_height: u32,
    threads: u32,
}

struct Context {
    key: ContextKey,
    handle: *mut c_void,
}
// SAFETY: a context is only ever driven by the encode that took it out of the
// pool; the pool itself just stores the opaque pointer.
unsafe impl Send for Context {}

static POOL: Mutex<Vec<Context>> = Mutex::new(Vec::new());
const POOL_LIMIT: usize = 8;

fn take_context(api: &Api, key: ContextKey) -> io::Result<Context> {
    if let Ok(mut pool) = POOL.lock() {
        if let Some(index) = pool.iter().position(|context| context.key == key) {
            return Ok(pool.swap_remove(index));
        }
    }
    let mut config = std::mem::MaybeUninit::<Config>::uninit();
    // SAFETY: config_init fills the whole struct on success.
    let status = unsafe {
        (api.config_init)(
            key.profile,
            key.block_width,
            key.block_height,
            1,
            QUALITY_THOROUGH,
            0,
            config.as_mut_ptr(),
        )
    };
    if status != SUCCESS {
        return Err(api.failure("config init", status));
    }
    let config = unsafe { config.assume_init() };
    let mut handle: *mut c_void = std::ptr::null_mut();
    let status =
        unsafe { (api.context_alloc)(&config, key.threads, &mut handle, std::ptr::null()) };
    if status != SUCCESS || handle.is_null() {
        return Err(api.failure("context alloc", status));
    }
    Ok(Context { key, handle })
}

fn return_context(api: &Api, context: Context) {
    // SAFETY: the encode that used the context has finished on every thread.
    let status = unsafe { (api.compress_reset)(context.handle) };
    match POOL.lock() {
        Ok(mut pool) if status == SUCCESS && pool.len() < POOL_LIMIT => pool.push(context),
        _ => unsafe { (api.context_free)(context.handle) },
    }
}

/// One image's compression, shared by the worker threads.
struct Job {
    api: &'static Api,
    handle: *mut c_void,
    image: *mut Image,
    out: *mut u8,
    len: usize,
}
// SAFETY: astcenc_compress_image is designed to be called concurrently on
// one context by `thread_count` workers, each writing its own blocks of the
// shared output; the image is read only.
unsafe impl Sync for Job {}

impl Job {
    fn run(&self, thread_index: u32) -> i32 {
        unsafe {
            (self.api.compress_image)(
                self.handle,
                self.image,
                &SWIZZLE_RGBA,
                self.out,
                self.len,
                thread_index,
            )
        }
    }
}

fn worker_count() -> u32 {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .clamp(1, 64) as u32
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/// Compresses an 8-bit RGBA picture into linear (row-major) ASTC blocks of
/// `block_width`x`block_height` texels, as sRGB colour when `srgb` is set
/// (`-cs`) and as linear data otherwise (`-cl`). The result is the payload
/// of the `.astc` file the command line writes, without its 16-byte header.
pub fn encode(
    image: &RgbaImage,
    block_width: u32,
    block_height: u32,
    srgb: bool,
) -> io::Result<Vec<u8>> {
    let api = api()?;
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the picture to encode is empty",
        ));
    }
    let key = ContextKey {
        profile: if srgb { PROFILE_LDR_SRGB } else { PROFILE_LDR },
        block_width,
        block_height,
        threads: worker_count(),
    };
    let context = take_context(api, key)?;
    let blocks = width.div_ceil(block_width) as usize * height.div_ceil(block_height) as usize;
    let mut out = vec![0u8; blocks * BLOCK_BYTES];
    let mut plane = image.as_raw().as_ptr() as *mut c_void;
    let mut picture = Image {
        dim_x: width,
        dim_y: height,
        dim_z: 1,
        data_type: TYPE_U8,
        data: &mut plane,
    };
    let job = Job {
        api,
        handle: context.handle,
        image: &mut picture,
        out: out.as_mut_ptr(),
        len: out.len(),
    };
    let statuses: Vec<i32> = std::thread::scope(|scope| {
        let workers: Vec<_> = (1..key.threads)
            .map(|index| {
                let job = &job;
                scope.spawn(move || job.run(index))
            })
            .collect();
        let mut statuses = vec![job.run(0)];
        statuses.extend(
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap_or(-1)),
        );
        statuses
    });
    match statuses.into_iter().find(|status| *status != SUCCESS) {
        Some(status) => {
            unsafe { (api.context_free)(context.handle) };
            Err(api.failure("compression", status))
        }
        None => {
            return_context(api, context);
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_layouts_match_the_header() {
        assert_eq!(std::mem::size_of::<Config>(), 120);
        assert_eq!(std::mem::size_of::<Image>(), 24);
        assert_eq!(std::mem::size_of::<Swizzle>(), 16);
    }

    #[test]
    fn encodes_a_small_picture() {
        if !is_available() {
            return;
        }
        let image = RgbaImage::from_fn(13, 7, |x, y| {
            image::Rgba([(x * 19) as u8, (y * 37) as u8, ((x + y) * 11) as u8, 255])
        });
        let blocks = encode(&image, 4, 4, true).unwrap();
        assert_eq!(blocks.len(), 4 * 2 * BLOCK_BYTES);
        let again = encode(&image, 4, 4, true).unwrap();
        assert_eq!(blocks, again);
    }
}
