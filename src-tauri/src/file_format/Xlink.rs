use std::{
    ffi::{c_char, c_void, CStr, CString},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use roead::Endian;
use xlink2_bindings as xlink_bindings;

use crate::{
    file_format::BinTextFile::OpenedFile,
    InternalFile::InternalFile,
    Open_and_Save::SendData,
    Settings::Magic,
    Settings::Pathlib,
    Zstd::{TotkFileType, TotkZstd, ZstdDictionary},
};

/// A lazily-created handle to the native XLink converter.
pub struct Xlink_rs<'a> {
    pub zstd: Arc<TotkZstd<'a>>,
}

/// Runtime-loaded converter for the legacy XLink representation.
#[allow(non_camel_case_types)]
#[cfg(windows)]
pub struct xlink_legacy<'a> {
    zstd: Arc<TotkZstd<'a>>,
    library: libloading::Library,
}

#[cfg(windows)]
impl<'a> xlink_legacy<'a> {
    pub fn new(zstd: Arc<TotkZstd<'a>>) -> io::Result<Self> {
        let path = Self::dll_path().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "legacy XLink converter not found (expected bin/dlls/xlink_tool.dll)",
            )
        })?;
        let library = unsafe { libloading::Library::new(&path) }.map_err(|error| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "unable to load legacy XLink converter {}: {error}",
                    path.display()
                ),
            )
        })?;
        Ok(Self { zstd, library })
    }

    fn dll_path() -> Option<PathBuf> {
        let relative = Path::new("bin/dlls/xlink_tool.dll");
        let mut candidates = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)];
        if let Ok(exe_dir) = crate::utils::running_exe_dir() {
            candidates.push(exe_dir.join(relative));
            candidates.push(exe_dir.join("dlls/xlink_tool.dll"));
        }
        candidates.into_iter().find(|path| path.is_file())
    }

    pub fn binary_to_yaml(&self, data: &[u8]) -> io::Result<String> {
        type Convert = unsafe extern "C" fn(*const u8, usize) -> *mut c_char;
        type Free = unsafe extern "C" fn(*mut c_void);

        let rawdata = xlink_data(&self.zstd, data)?;
        unsafe {
            let convert: libloading::Symbol<Convert> = self
                .library
                .get(b"xlink_binary_to_yaml\0")
                .map_err(io::Error::other)?;
            let free: libloading::Symbol<Free> = self
                .library
                .get(b"free_xlink_string\0")
                .map_err(io::Error::other)?;
            let yaml_ptr = convert(rawdata.as_ptr(), rawdata.len());
            if yaml_ptr.is_null() {
                return Err(io::Error::other(
                    "legacy XLink converter failed to convert binary to YAML",
                ));
            }
            let yaml_bytes = CStr::from_ptr(yaml_ptr).to_bytes().to_vec();
            free(yaml_ptr.cast());
            String::from_utf8(yaml_bytes).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("legacy XLink converter returned invalid UTF-8 text: {error}"),
                )
            })
        }
    }

    pub fn yaml_to_binary(&self, data: &str) -> io::Result<Vec<u8>> {
        type Convert = unsafe extern "C" fn(*const c_char, usize, *mut usize) -> *mut u8;
        type Free = unsafe extern "C" fn(*mut c_void);

        let yaml = CString::new(data).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "XLink YAML contains a NUL byte",
            )
        })?;
        unsafe {
            let convert: libloading::Symbol<Convert> = self
                .library
                .get(b"xlink_yaml_to_binary\0")
                .map_err(io::Error::other)?;
            let free: libloading::Symbol<Free> = self
                .library
                .get(b"free_xlink_binary\0")
                .map_err(io::Error::other)?;
            let mut out_size = 0;
            let binary_ptr = convert(yaml.as_ptr(), data.len(), &mut out_size);
            if binary_ptr.is_null() {
                return Err(io::Error::other(
                    "legacy XLink converter failed to convert YAML to binary",
                ));
            }
            let binary = std::slice::from_raw_parts(binary_ptr, out_size).to_vec();
            free(binary_ptr.cast());
            if !Magic::is_xlink(&binary) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "legacy XLink converter returned data without XLNK magic",
                ));
            }
            Ok(binary)
        }
    }
}

fn xlink_data(zstd: &TotkZstd<'_>, data: &[u8]) -> io::Result<Vec<u8>> {
    let rawdata = if Magic::is_xlink(data) {
        data.to_vec()
    } else {
        zstd.try_decompress(data)
            .map_err(|err| io::Error::other(err.to_string()))?
    };
    if !Magic::is_xlink(&rawdata) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a valid XLink binary (missing XLNK magic)",
        ));
    }
    Ok(rawdata)
}

fn validate_modern_profile(data: &[u8]) -> io::Result<()> {
    let version_bytes: [u8; 4] = data
        .get(8..12)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "XLink header is truncated before the profile version",
            )
        })?;
    let version = u32::from_le_bytes(version_bytes);
    if matches!(version, 0x21 | 0x22 | 0x24) {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!(
            "unsupported XLink profile version 0x{version:02x}; the modern converter supports 0x21, 0x22, and 0x24"
        ),
    ))
}

#[cfg(windows)]
impl<'a> Xlink_rs<'a> {
    pub fn new(zstd: Arc<TotkZstd<'a>>) -> io::Result<Self> {
        #[cfg(windows)]
        {
            Ok(Self { zstd })
        }
        #[cfg(not(windows))]
        {
            let _ = zstd;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Xlink bindings are only supported on Windows",
            ))
        }
    }

    pub fn binary_to_yaml(&self, data: &[u8]) -> io::Result<String> {
        if self.zstd.totk_config.xlink_format == "legacy" {
            return xlink_legacy::new(self.zstd.clone())?.binary_to_yaml(data);
        }
        let rawdata = xlink_data(&self.zstd, data)?;
        // The native converter terminates the entire process when handed an
        // unsupported profile, so validate this field before crossing FFI.
        validate_modern_profile(&rawdata)?;

        unsafe {
            let yaml_ptr = xlink_bindings::binary_to_yaml(&rawdata).cast::<i8>();
            if yaml_ptr.is_null() {
                return Err(io::Error::other(
                    "XLink converter failed to convert binary to YAML",
                ));
            }
            let yaml_bytes = CStr::from_ptr(yaml_ptr).to_bytes().to_vec();
            xlink_bindings::free_string(yaml_ptr.cast_mut());
            String::from_utf8(yaml_bytes).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("XLink converter returned invalid UTF-8 text: {error}"),
                )
            })
        }
    }

    pub fn yaml_to_binary(&self, data: &str) -> io::Result<Vec<u8>> {
        if self.zstd.totk_config.xlink_format == "legacy" {
            return xlink_legacy::new(self.zstd.clone())?.yaml_to_binary(data);
        }
        unsafe {
            let (binary_ptr, out_size) = xlink_bindings::yaml_to_binary(data.as_bytes());
            if binary_ptr.is_null() {
                return Err(io::Error::other(
                    "XLink converter failed to convert YAML to binary",
                ));
            }
            let binary = std::slice::from_raw_parts(binary_ptr as *const u8, out_size).to_vec();
            xlink_bindings::free_binary(binary_ptr);
            if !Magic::is_xlink(&binary) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "XLink converter returned binary data without XLNK magic",
                ));
            }
            Ok(binary)
        }
    }

    pub fn open_internal<P: AsRef<Path>>(
        path: P,
        data: &[u8],
        zstd: Arc<TotkZstd<'a>>,
        compression: Option<ZstdDictionary>,
    ) -> Option<(InternalFile<'static>, String)> {
        let path = path.as_ref();
        if !Magic::is_xlink(data) {
            return None;
        }
        let text = match Self::new(zstd).and_then(|xlink| xlink.binary_to_yaml(data)) {
            Ok(text) => text,
            Err(error) => {
                println!("Unable to parse XLink entry {}: {error}", path.display());
                return None;
            }
        };
        let mut internal = InternalFile::default();
        internal.endian = Some(Endian::Little);
        internal.path = Pathlib::new(path);
        internal.file_type = TotkFileType::Xlink;
        internal.compression = compression;
        Some((internal, text))
    }

    pub fn text_to_binary(
        text: &str,
        file_path: &str,
        zstd: Arc<TotkZstd<'a>>,
        _dictionary: Option<ZstdDictionary>,
    ) -> Option<Vec<u8>> {
        let data = match Self::new(zstd.clone()).and_then(|xlink| xlink.yaml_to_binary(text)) {
            Ok(data) => data,
            Err(error) => {
                println!("Unable to save XLink YAML for {file_path}: {error}");
                return None;
            }
        };
        Some(data)
    }

    pub fn open_xlink<P: AsRef<Path>>(
        path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(OpenedFile<'static>, SendData)> {
        let xlink_format = zstd.clone().totk_config.clone().xlink_format.clone();
        let path = path.as_ref();
        let pathlib = Pathlib::new(path);
        let rawdata = std::fs::read(path).ok()?;
        let rawdata = match xlink_data(&zstd, &rawdata) {
            Ok(rawdata) => rawdata,
            Err(error) => {
                println!("Is {} an XLink file? no: {error}", path.display());
                return None;
            }
        };
        print!("Is {} an XLink file? ", path.display());
        let text = match Self::new(zstd).and_then(|xlink| xlink.binary_to_yaml(&rawdata)) {
            Ok(text) => text,
            Err(error) => {
                println!("no: {error}");
                return None;
            }
        };
        println!("yes");

        let mut opened_file = OpenedFile::default();
        opened_file.path = pathlib.clone();
        opened_file.endian = Some(Endian::Little);
        opened_file.file_type = TotkFileType::Xlink;

        let mut data = SendData::default();
        data.status_text = format!("Opened {}", pathlib.full_path);
        data.path = pathlib;
        data.text = text;
        data.tab = "YAML".to_string();
        data.lang = if xlink_format == "modern" {
            "xlink".to_string()
        } else {
            "yaml".to_string()
        };
        data.get_file_label(TotkFileType::Xlink, Some(Endian::Little));
        Some((opened_file, data))
    }

    pub fn open_xlink_binary<P: AsRef<Path>>(
        binary: &[u8],
        display_path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(OpenedFile<'static>, SendData)> {
        let path = display_path.as_ref();
        let rawdata = xlink_data(&zstd, binary).ok()?;
        let text = Self::new(zstd.clone())
            .ok()?
            .binary_to_yaml(&rawdata)
            .ok()?;
        let mut opened = OpenedFile::default();
        opened.path = Pathlib::new(path);
        opened.endian = Some(Endian::Little);
        opened.file_type = TotkFileType::Xlink;
        let mut data = SendData::default();
        data.path = Pathlib::new(path);
        data.text = text;
        data.status_text = format!("Opened {}", path.display());
        data.lang = if zstd.totk_config.xlink_format == "modern" {
            "xlink".into()
        } else {
            "yaml".into()
        };
        data.get_file_label(TotkFileType::Xlink, Some(Endian::Little));
        Some((opened, data))
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_modern_profile, xlink_data, Xlink_rs};
    use crate::TotkConfig::TotkConfig;

    #[test]
    fn opens_slink2_profile_with_detected_native_profile() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/crash/slink2.Product.100.bslnk.zs");
        if !path.is_file() {
            return;
        }
        let app = crate::TotkApp::TotkBitsApp::default();
        let compressed = std::fs::read(&path).unwrap();
        let raw = xlink_data(&app.zstd, &compressed).unwrap();
        validate_modern_profile(&raw).unwrap();
        let text = Xlink_rs::new(app.zstd.clone())
            .unwrap()
            .binary_to_yaml(&compressed)
            .unwrap();
        assert!(text.starts_with("Metadata {"));
        assert!(text.contains("ModuleType = SLink"));
        let (opened, data) = Xlink_rs::open_xlink(&path, app.zstd).unwrap();
        assert_eq!(opened.file_type, crate::Zstd::TotkFileType::Xlink);
        assert_eq!(data.tab, "YAML");
    }

    #[test]
    fn opens_elink2_profile_with_detected_native_profile() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/crash/elink2.Product.100.belnk.zs");
        if !path.is_file() {
            return;
        }
        let app = crate::TotkApp::TotkBitsApp::default();
        let compressed = std::fs::read(&path).unwrap();
        let text = Xlink_rs::new(app.zstd.clone())
            .unwrap()
            .binary_to_yaml(&compressed)
            .unwrap();
        assert!(text.starts_with("Metadata {"));
        assert!(text.contains("ModuleType = ELink"));
        let (opened, data) = Xlink_rs::open_xlink(&path, app.zstd).unwrap();
        assert_eq!(opened.file_type, crate::Zstd::TotkFileType::Xlink);
        assert_eq!(data.tab, "YAML");
    }

    #[test]
    fn corrupted_xlink_magic_is_reported_without_process_abort() {
        let app = crate::TotkApp::TotkBitsApp::default();
        let mut corrupted = vec![0_u8; 12];
        corrupted[..4].copy_from_slice(b"XLNK");
        corrupted[4..8].copy_from_slice(&12_u32.to_le_bytes());
        corrupted[8..12].copy_from_slice(&0x24_u32.to_le_bytes());

        let error = Xlink_rs::new(app.zstd.clone())
            .unwrap()
            .binary_to_yaml(&corrupted)
            .unwrap_err();
        assert!(error.to_string().contains("failed to convert"));
        assert!(
            Xlink_rs::open_xlink_binary(&corrupted, "corrupted.Product.100.belnk", app.zstd)
                .is_none()
        );
    }

    use crate::Zstd::TotkZstd;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    #[cfg(windows)]
    fn elink_binary_fixture_converts_to_yaml_and_round_trips() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_ss/elink2.Product.110.belnk");
        let input = fs::read(path).expect("missing ELink binary fixture");
        let zstd = Arc::new(TotkZstd::dictionaryless(
            Arc::new(TotkConfig::default()),
            crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
        ));
        let converter = Xlink_rs::new(zstd).expect("construct XLink converter");
        let yaml = converter
            .binary_to_yaml(&input)
            .expect("convert ELink to YAML");
        assert!(!yaml.is_empty(), "XLink converter returned empty YAML");
        let rebuilt = converter
            .yaml_to_binary(&yaml)
            .expect("rebuild ELink from YAML");
        assert!(crate::Settings::Magic::is_xlink(&rebuilt));
    }

    #[test]
    #[cfg(windows)]
    fn elink_yaml_fixture_converts_to_binary_and_round_trips() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_ss/elink2.Product.110.belnk.yaml");
        let yaml = fs::read_to_string(path).expect("missing ELink YAML fixture");
        let zstd = Arc::new(TotkZstd::dictionaryless(
            Arc::new(TotkConfig::default()),
            crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
        ));
        let converter = Xlink_rs::new(zstd).expect("construct XLink converter");
        let binary = converter
            .yaml_to_binary(&yaml)
            .expect("convert YAML fixture to ELink binary");
        assert!(crate::Settings::Magic::is_xlink(&binary));
        let rebuilt_yaml = converter
            .binary_to_yaml(&binary)
            .expect("convert rebuilt ELink binary to YAML");
        assert!(
            !rebuilt_yaml.is_empty(),
            "XLink converter returned empty rebuilt YAML"
        );
    }

    #[test]
    #[cfg(windows)]
    fn compressed_elink_fixture_decompresses_and_converts() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/elink2.Product.110.belnk.zs");
        let mut config = TotkConfig::safe_new(false).unwrap_or_default();
        config.xlink_format = "legacy".into();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ZSTD dictionaries"),
        );
        let input = fs::read(fixture).expect("missing compressed ELink fixture");
        let converter = Xlink_rs::new(zstd).expect("construct XLink converter");
        let yaml = converter
            .binary_to_yaml(&input)
            .expect("decompress and convert compressed ELink fixture with legacy DLL");
        assert!(!yaml.is_empty(), "compressed XLink returned empty text");
        let rebuilt = converter
            .yaml_to_binary(&yaml)
            .expect("convert legacy XLink YAML back to binary");
        assert!(crate::Settings::Magic::is_xlink(&rebuilt));
    }
}

#[cfg(not(windows))]
impl<'a> Xlink_rs<'a> {
    pub fn new(zstd: Arc<TotkZstd<'a>>) -> io::Result<Self> {
        let _ = zstd;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Xlink bindings are only supported on Windows",
        ))
    }

    pub fn binary_to_yaml(&self, _data: &[u8]) -> io::Result<String> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Xlink bindings are only supported on Windows",
        ))
    }

    pub fn yaml_to_binary(&self, _data: &str) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Xlink bindings are only supported on Windows",
        ))
    }

    pub fn open_internal<P: AsRef<Path>>(
        _path: P,
        _data: &[u8],
        _zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(InternalFile<'static>, String)> {
        None
    }

    pub fn text_to_binary(
        _text: &str,
        _file_path: &str,
        _zstd: Arc<TotkZstd<'a>>,
        _dictionary: Option<ZstdDictionary>,
    ) -> Option<Vec<u8>> {
        None
    }

    pub fn open_xlink<P: AsRef<Path>>(
        _path: P,
        _zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(OpenedFile<'static>, SendData)> {
        None
    }
}
