//! Switch Toolbox compatible Zstandard frames.
//!
//! Toolbox's `Zstb` compression format goes through ZstdNet, which ships
//! libzstd 1.3.3 and calls `ZSTD_compressCCtx` with the requested level and
//! default frame parameters. Different libzstd releases produce different
//! (but compatible) bytes for the same input, so byte-exact output needs the
//! same library: `bin/cpp/libzstd_toolbox.dll` is that 1.3.3 build.
#[cfg(windows)]
use std::ffi::{c_void, CStr};
use std::io;

/// Level ZstdNet uses inside Toolbox's `Zstb.SCompress`.
pub const TOOLBOX_ZSTD_LEVEL: i32 = 19;

/// The DLL ships as a bundle resource next to the executable; the source
/// tree location keeps `cargo test` and `cargo run` working.
#[cfg(windows)]
fn dll_path() -> Option<std::path::PathBuf> {
    let relative = std::path::Path::new("bin/cpp/libzstd_toolbox.dll");
    let mut candidates = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)];
    if let Ok(exe_dir) = crate::utils::running_exe_dir() {
        candidates.push(exe_dir.join(relative));
        candidates.push(exe_dir.join("cpp/libzstd_toolbox.dll"));
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// Compresses `source` into a regular Zstandard frame exactly like Toolbox's
/// ZstdNet backend (libzstd 1.3.3, `ZSTD_compressCCtx`, no dictionary).
#[cfg(windows)]
pub fn compress_like_toolbox(source: &[u8], level: i32) -> io::Result<Vec<u8>> {
    type Create = unsafe extern "C" fn() -> *mut c_void;
    type Free = unsafe extern "C" fn(*mut c_void) -> usize;
    type Bound = unsafe extern "C" fn(usize) -> usize;
    type CompressCCtx =
        unsafe extern "C" fn(*mut c_void, *mut c_void, usize, *const c_void, usize, i32) -> usize;
    type IsError = unsafe extern "C" fn(usize) -> u32;
    type ErrorName = unsafe extern "C" fn(usize) -> *const std::ffi::c_char;

    let path = dll_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Toolbox Zstandard 1.3.3 backend not found (expected bin/cpp/libzstd_toolbox.dll)",
        )
    })?;
    let library = unsafe { libloading::Library::new(&path) }.map_err(|error| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("unable to load {}: {error}", path.display()),
        )
    })?;
    unsafe {
        let create: libloading::Symbol<Create> = library
            .get(b"ZSTD_createCCtx\0")
            .map_err(io::Error::other)?;
        let free: libloading::Symbol<Free> =
            library.get(b"ZSTD_freeCCtx\0").map_err(io::Error::other)?;
        let bound: libloading::Symbol<Bound> = library
            .get(b"ZSTD_compressBound\0")
            .map_err(io::Error::other)?;
        let compress: libloading::Symbol<CompressCCtx> = library
            .get(b"ZSTD_compressCCtx\0")
            .map_err(io::Error::other)?;
        let is_error: libloading::Symbol<IsError> =
            library.get(b"ZSTD_isError\0").map_err(io::Error::other)?;
        let error_name: libloading::Symbol<ErrorName> = library
            .get(b"ZSTD_getErrorName\0")
            .map_err(io::Error::other)?;

        let context = create();
        if context.is_null() {
            return Err(io::Error::other("Toolbox ZSTD context allocation failed"));
        }
        struct Guard(*mut c_void, Free);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe {
                    (self.1)(self.0);
                }
            }
        }
        let _guard = Guard(context, *free);

        let mut output = vec![0u8; bound(source.len())];
        let written = compress(
            context,
            output.as_mut_ptr().cast(),
            output.len(),
            source.as_ptr().cast(),
            source.len(),
            level,
        );
        if is_error(written) != 0 {
            return Err(io::Error::other(
                CStr::from_ptr(error_name(written))
                    .to_string_lossy()
                    .into_owned(),
            ));
        }
        output.truncate(written);
        Ok(output)
    }
}

#[cfg(not(windows))]
pub fn compress_like_toolbox(_source: &[u8], _level: i32) -> io::Result<Vec<u8>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Toolbox Zstandard 1.3.3 backend is only bundled on Windows",
    ))
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn frames_decompress_with_the_regular_backend() {
        let input: Vec<u8> = (0..20000u32).map(|i| (i % 251) as u8).collect();
        let compressed = super::compress_like_toolbox(&input, super::TOOLBOX_ZSTD_LEVEL).unwrap();
        assert_eq!(&compressed[..4], &[0x28, 0xb5, 0x2f, 0xfd]);
        let restored = zstd::bulk::decompress(&compressed, input.len()).unwrap();
        assert_eq!(restored, input);
    }
}
