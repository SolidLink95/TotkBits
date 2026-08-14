use flate2::read::ZlibDecoder;
use serde::de::DeserializeOwned;
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

fn support_paths(name: &str) -> [PathBuf; 2] {
    [
        crate::Settings::exe_relative_path(Path::new("misc").join(name)),
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("misc")
            .join(name),
    ]
}

pub(crate) fn read_support_bytes(name: &str) -> std::io::Result<Vec<u8>> {
    let mut last_error = None;
    for path in support_paths(name) {
        match std::fs::read(&path) {
            Ok(value) => return Ok(value),
            Err(error) => last_error = Some((path, error)),
        }
    }
    let (path, error) = last_error.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no support-file search paths are configured",
        )
    })?;
    Err(std::io::Error::new(
        error.kind(),
        format!(
            "failed to load misc support file {}: {error}",
            path.display()
        ),
    ))
}

pub(crate) fn read_support_text(name: &str, fallback: &str) -> String {
    match read_support_bytes(name).and_then(|bytes| {
        String::from_utf8(bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{error}");
            fallback.to_owned()
        }
    }
}

pub(crate) fn read_support_json(name: &str) -> String {
    read_support_text(name, "{}")
}

fn load_zlib_json<T: DeserializeOwned + Default>(name: &str) -> T {
    read_support_bytes(name)
        .and_then(|compressed| decode_zlib_json(&compressed))
        .map_err(|error| eprintln!("Failed to load misc lookup data {name}: {error}"))
        .unwrap_or_default()
}

fn decode_zlib_json<T: DeserializeOwned>(compressed: &[u8]) -> std::io::Result<T> {
    let mut json = String::new();
    ZlibDecoder::new(compressed).read_to_string(&mut json)?;
    serde_json::from_str(&json).map_err(std::io::Error::other)
}

static SARC_SHA256: LazyLock<Arc<HashMap<String, String>>> =
    LazyLock::new(|| Arc::new(load_zlib_json("totk_sarc_sha256.bin")));
static RSTB_PATHS: LazyLock<Arc<Vec<String>>> =
    LazyLock::new(|| Arc::new(load_zlib_json("totk_rstb_paths.bin")));
static INTERNAL_FILEPATHS: LazyLock<Arc<HashMap<String, String>>> =
    LazyLock::new(|| Arc::new(load_zlib_json("totk_internal_filepaths.bin")));
static FILENAME_TO_LOCALPATH: LazyLock<Arc<HashMap<String, String>>> =
    LazyLock::new(|| Arc::new(load_zlib_json("totk_filename_to_localpath.bin")));

pub fn sarc_sha256() -> Arc<HashMap<String, String>> {
    Arc::clone(&SARC_SHA256)
}

pub fn rstb_paths() -> Arc<Vec<String>> {
    Arc::clone(&RSTB_PATHS)
}

pub fn internal_filepaths() -> &'static HashMap<String, String> {
    INTERNAL_FILEPATHS.as_ref()
}

pub fn filename_to_localpath() -> &'static HashMap<String, String> {
    FILENAME_TO_LOCALPATH.as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_lookup_tables_are_initialized_once_and_shared() {
        let sarc_first = sarc_sha256();
        let sarc_second = sarc_sha256();
        assert!(!sarc_first.is_empty());
        assert!(Arc::ptr_eq(&sarc_first, &sarc_second));

        let rstb_first = rstb_paths();
        let rstb_second = rstb_paths();
        assert!(!rstb_first.is_empty());
        assert!(Arc::ptr_eq(&rstb_first, &rstb_second));

        assert!(!internal_filepaths().is_empty());
        assert!(std::ptr::eq(internal_filepaths(), internal_filepaths()));
        assert!(!filename_to_localpath().is_empty());
        assert!(std::ptr::eq(
            filename_to_localpath(),
            filename_to_localpath()
        ));
    }

    #[test]
    fn every_misc_json_and_bin_support_file_loads() {
        for name in [
            "AOC_names.json",
            "Animations_paths_and_characters.json",
            "bars_bwav_sha256.json",
            "bones_botw.json",
            "bphcl_nodes.json",
            "G1M_to_G1T_pairs.json",
        ] {
            let value = read_support_json(name);
            assert_ne!(value, "{}", "failed to load misc/{name}");
            assert!(
                serde_json::from_str::<serde_json::Value>(&value).is_ok(),
                "misc/{name} is invalid JSON"
            );
        }
        for name in [
            "totk_filename_to_localpath.bin",
            "totk_internal_filepaths.bin",
            "totk_rstb_paths.bin",
            "totk_sarc_sha256.bin",
        ] {
            assert!(
                !read_support_bytes(name).unwrap().is_empty(),
                "misc/{name} is empty"
            );
        }
    }

    #[test]
    fn missing_json_support_file_falls_back_to_empty_object() {
        assert_eq!(read_support_json("does-not-exist.json"), "{}");
    }
}
