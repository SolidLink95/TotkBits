use std::ffi::c_int;
#[cfg(windows)]
use std::ffi::{c_void, CStr};
use std::io;

const MCPK_MAGIC: &[u8; 4] = b"MCPK";
const MCPK_VERSION: [u8; 4] = [1, 1, 0, 0];
const MCPK_ALIGNMENT: usize = 0x1000;
const MCPK_ZSTD_LEVEL: i32 = 20;
use meshcodec_bindings::MeshCodecBindings;

pub struct MeshCodec;

impl MeshCodec {
    pub fn new() -> io::Result<Self> {
        Self::ensure_platform()
    }

    #[cfg(windows)]
    fn ensure_platform() -> io::Result<Self> {
        Ok(Self)
    }

    #[cfg(not(windows))]
    fn ensure_platform() -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "MeshCodec bindings are only supported on Windows",
        ))
    }

    pub fn decompress(data: &[u8]) -> io::Result<Vec<u8>> {
        if !crate::Settings::Magic::is_mcpk(data) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "input does not have MCPK magic",
            ));
        }
        if data.len() > c_int::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "MCPK input is too large for the MeshCodec ABI",
            ));
        }

        Self::ensure_platform()?;
        Self::decompress_loaded(data).or_else(|_| Self::decompress_pseudo(data))
    }

    /// Reproduces Switch Toolbox's "fake" MeshCodec compression: an MCPK
    /// header followed by a dictionaryless, magicless ZSTD frame.
    pub fn compress(data: &[u8]) -> io::Result<Vec<u8>> {
        // BFRES external strings, buffers, and GPU data can live beyond the
        // header's logical file_size. Compress the complete supplied allocation;
        // truncating to file_size makes otherwise valid models lose their meshes.
        let source = data;

        let aligned_size = source
            .len()
            .checked_add(MCPK_ALIGNMENT - 1)
            .map(|size| size & !(MCPK_ALIGNMENT - 1))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "MCPK input is too large")
            })?;
        let aligned_size = u32::try_from(aligned_size).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "MCPK decompressed size exceeds the 32-bit header",
            )
        })?;
        let flags = ((aligned_size >> 12) << 5) + 12;

        let compressed = BfresZstd155::compress(source)?;

        let mut output = Vec::with_capacity(12 + compressed.len());
        output.extend_from_slice(MCPK_MAGIC);
        output.extend_from_slice(&MCPK_VERSION);
        output.extend_from_slice(&flags.to_le_bytes());
        output.extend_from_slice(&compressed);
        Ok(output)
    }

    fn decompress_loaded(data: &[u8]) -> io::Result<Vec<u8>> {
        MeshCodecBindings::decompress(data)
    }

    fn decompress_pseudo(data: &[u8]) -> io::Result<Vec<u8>> {
        let flags = data
            .get(8..12)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "truncated MCPK header"))?;
        let size = ((flags >> 5) << (flags & 0xf)) as usize;
        let mut decompressor = zstd::bulk::Decompressor::new()?;
        decompressor.set_parameter(zstd::zstd_safe::DParameter::Format(
            zstd::zstd_safe::FrameFormat::Magicless,
        ))?;
        let mut output = decompressor.decompress(&data[12..], size)?;
        if output.len() > size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "pseudo-MCPK payload exceeds its advertised size",
            ));
        }
        output.resize(size, 0);
        Ok(output)
    }
}

/// Isolated Toolbox-compatible Zstandard 1.5.5 backend. Only MCPK/BFRES
/// serialization calls this type; all other formats keep the application's
/// normal `zstd` dependency and settings.
struct BfresZstd155;

impl BfresZstd155 {
    /// The DLL ships as a bundle resource next to the executable; the source
    /// tree location keeps `cargo test` and `cargo run` working.
    #[cfg(windows)]
    fn dll_path() -> Option<std::path::PathBuf> {
        let relative = std::path::Path::new("bin/cpp/toolbox_zstd155.dll");
        let mut candidates = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)];
        if let Ok(exe_dir) = crate::utils::running_exe_dir() {
            candidates.push(exe_dir.join(relative));
            candidates.push(exe_dir.join("cpp/toolbox_zstd155.dll"));
        }
        candidates.into_iter().find(|path| path.is_file())
    }

    #[cfg(windows)]
    fn compress(source: &[u8]) -> io::Result<Vec<u8>> {
        type Bound = unsafe extern "C" fn(usize) -> usize;
        type Compress = unsafe extern "C" fn(*const u8, usize, *mut u8, usize) -> isize;

        let path = Self::dll_path().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "BFRES Zstandard 1.5.5 backend not found (expected bin/cpp/toolbox_zstd155.dll)",
            )
        })?;
        let library = unsafe { libloading::Library::new(&path) }.map_err(|error| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("unable to load {}: {error}", path.display()),
            )
        })?;
        unsafe {
            let bound: libloading::Symbol<Bound> = library
                .get(b"toolbox_zstd155_bound\0")
                .map_err(io::Error::other)?;
            let compress: libloading::Symbol<Compress> = library
                .get(b"toolbox_zstd155_compress\0")
                .map_err(io::Error::other)?;
            let mut output = vec![0; bound(source.len())];
            let size = compress(
                source.as_ptr(),
                source.len(),
                output.as_mut_ptr(),
                output.len(),
            );
            if size < 0 {
                return Err(io::Error::other("BFRES Zstandard 1.5.5 compression failed"));
            }
            output.truncate(size as usize);
            Ok(output)
        }
    }

    #[cfg(not(windows))]
    fn compress(_source: &[u8]) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "BFRES Zstandard 1.5.5 backend is only bundled on Windows",
        ))
    }
}

#[cfg(windows)]
fn compress_toolbox_zstd(source: &[u8]) -> io::Result<Vec<u8>> {
    type Create = unsafe extern "C" fn() -> *mut c_void;
    type Free = unsafe extern "C" fn(*mut c_void) -> usize;
    type Bound = unsafe extern "C" fn(usize) -> usize;
    type SetParameter = unsafe extern "C" fn(*mut c_void, i32, i32) -> usize;
    #[repr(C)]
    struct InBuffer {
        src: *const c_void,
        size: usize,
        pos: usize,
    }
    #[repr(C)]
    struct OutBuffer {
        dst: *mut c_void,
        size: usize,
        pos: usize,
    }
    type Compress = unsafe extern "C" fn(*mut c_void, *mut OutBuffer, *mut InBuffer, i32) -> usize;
    type IsError = unsafe extern "C" fn(usize) -> u32;
    type ErrorName = unsafe extern "C" fn(usize) -> *const std::ffi::c_char;

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let bundled = manifest.join("bin/cpp/libzstd_toolbox.dll");
    let library = unsafe { libloading::Library::new(&bundled) }
        .map_err(|error| io::Error::new(io::ErrorKind::NotFound, error))?;
    unsafe {
        let create: libloading::Symbol<Create> = library
            .get(b"ZSTD_createCCtx\0")
            .map_err(io::Error::other)?;
        let free: libloading::Symbol<Free> =
            library.get(b"ZSTD_freeCCtx\0").map_err(io::Error::other)?;
        let bound: libloading::Symbol<Bound> = library
            .get(b"ZSTD_compressBound\0")
            .map_err(io::Error::other)?;
        let set: libloading::Symbol<SetParameter> = library
            .get(b"ZSTD_CCtx_setParameter\0")
            .map_err(io::Error::other)?;
        let compress: libloading::Symbol<Compress> = library
            .get(b"ZSTD_compress_generic\0")
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
        for (parameter, value) in [
            (100, MCPK_ZSTD_LEVEL),
            (200, 0),
            (201, 0),
            (202, 0),
            (10, 1),
        ] {
            let result = set(context, parameter, value);
            if is_error(result) != 0 {
                return Err(io::Error::other(
                    CStr::from_ptr(error_name(result))
                        .to_string_lossy()
                        .into_owned(),
                ));
            }
        }
        let mut output = vec![0u8; bound(source.len())];
        let mut input = InBuffer {
            src: source.as_ptr().cast(),
            size: source.len(),
            pos: 0,
        };
        let mut target = OutBuffer {
            dst: output.as_mut_ptr().cast(),
            size: output.len(),
            pos: 0,
        };
        let remaining = compress(context, &mut target, &mut input, 2);
        if is_error(remaining) != 0 {
            return Err(io::Error::other(
                CStr::from_ptr(error_name(remaining))
                    .to_string_lossy()
                    .into_owned(),
            ));
        }
        if remaining != 0 || input.pos != input.size {
            return Err(io::Error::other("Toolbox ZSTD did not finish the frame"));
        }
        output.truncate(target.pos);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::MeshCodec;

    #[test]
    fn magic_gate_rejects_non_mcpk_before_loading() {
        let error = MeshCodec::decompress(b"Yaz0not-mesh-codec").unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("MCPK magic"));
    }

    #[cfg(windows)]
    #[test]
    fn decompresses_local_mcpk_corpus() {
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/mcpk");
        let mut count = 0;
        for entry in std::fs::read_dir(corpus).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|value| value.to_str()) != Some("mc") {
                continue;
            }
            let input = std::fs::read(&path).unwrap();
            let flags = u32::from_le_bytes(input[8..12].try_into().unwrap());
            let expected_len = (flags >> 5) << (flags & 0xf);
            let output = MeshCodec::decompress(&input)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(output.len(), expected_len as usize, "{}", path.display());
            count += 1;
        }
        assert!(count > 0, "MCPK corpus is empty");
    }

    #[cfg(windows)]
    #[test]
    fn toolbox_compatible_compression_roundtrips_supplied_bfres() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/mcpk/Armor_001.Armor_001_Head_A.bfres");
        let input = std::fs::read(path).unwrap();
        let compressed = MeshCodec::compress(&input).unwrap();

        assert!(crate::Settings::Magic::is_mcpk(&compressed));
        assert_eq!(&compressed[4..8], &[1, 1, 0, 0]);
        let flags = u32::from_le_bytes(compressed[8..12].try_into().unwrap());
        let expected_size = (input.len() + 0xfff) & !0xfff;
        assert_eq!(((flags >> 5) << (flags & 0xf)) as usize, expected_size);

        let decompressed = MeshCodec::decompress(&compressed).unwrap();
        assert_eq!(&decompressed[..input.len()], input);
        assert!(decompressed[input.len()..].iter().all(|byte| *byte == 0));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "diagnostic until the bundled Toolbox libzstd revision is matched"]
    fn recompresses_working_toolbox_weapon_byte_exactly() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../tmp/BotW Weapon Restoration/romfs/_model/toolbox/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc",
        );
        if !path.is_file() {
            return;
        }
        let expected = std::fs::read(path).unwrap();
        let mut raw = MeshCodec::decompress(&expected).unwrap();
        let file_size = u32::from_le_bytes(raw[0x1c..0x20].try_into().unwrap()) as usize;
        raw.truncate(file_size);
        let actual = MeshCodec::compress(&raw).unwrap();
        assert_eq!(actual, expected);
    }
}
