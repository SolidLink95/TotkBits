use crate::{DocumentState::DocumentState, Settings::NO_WINDOW_FLAG};
use serde::Serialize;
use std::{
    env, fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRenderFile {
    pub source: String,
    pub output: String,
    pub model_kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchG1mInspection {
    pub status: String,
    pub model: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub async fn inspect_batch_g1m(path: String) -> BatchG1mInspection {
    tauri::async_runtime::spawn_blocking(move || inspect_batch_g1m_blocking(path))
        .await
        .unwrap_or_else(|error| batch_g1m_error(error.to_string()))
}

fn inspect_batch_g1m_blocking(path: String) -> BatchG1mInspection {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
    if let Err(error) = fs::create_dir_all(&temp_root) {
        return batch_g1m_error(format!("failed to create worker temp directory: {error}"));
    }
    let output = temp_root.join(format!("totkbits-batch-g1m-{}-{nonce}.json", process::id()));
    let executable = match env::current_exe() {
        Ok(value) => value,
        Err(error) => return batch_g1m_error(error.to_string()),
    };
    let mut child = match Command::new(executable)
        .args(["--cli", "batch_g1m_worker", "g1m"])
        .arg(&path)
        .arg(&output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(NO_WINDOW_FLAG)
        .spawn()
    {
        Ok(value) => value,
        Err(error) => return batch_g1m_error(error.to_string()),
    };
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < Duration::from_secs(60) => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => break None,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(&output);
                return batch_g1m_error(error.to_string());
            }
        }
    };
    let Some(status) = status else {
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_file(&output);
        return BatchG1mInspection {
            status: "timeout".into(),
            model: None,
            error: Some("G1M parsing exceeded 60 seconds".into()),
        };
    };
    if !status.success() {
        let _ = fs::remove_file(&output);
        return batch_g1m_error(format!("G1M worker exited with {status}"));
    }
    let result = fs::read(&output)
        .map_err(|error| error.to_string())
        .and_then(|data| serde_json::from_slice(&data).map_err(|error| error.to_string()));
    let _ = fs::remove_file(&output);
    match result {
        Ok(model) => BatchG1mInspection {
            status: "ok".into(),
            model: Some(model),
            error: None,
        },
        Err(error) => batch_g1m_error(error),
    }
}

fn batch_g1m_error(error: String) -> BatchG1mInspection {
    BatchG1mInspection {
        status: "error".into(),
        model: None,
        error: Some(error),
    }
}

/// Finds renderable standalone model files without attempting to parse every
/// unrelated file in a potentially large tree. The output tree mirrors the
/// input tree and replaces each model extension with `.png`.
pub fn list_batch_render_files(
    app_handle: tauri::AppHandle,
    documentId: String,
    source_root: String,
    output_root: String,
    existing_png: String,
    model_kind: String,
) -> Result<Vec<BatchRenderFile>, String> {
    let source_root = fs::canonicalize(&source_root).map_err(|error| error.to_string())?;
    if !source_root.is_dir() {
        return Err("batch render source is not a directory".into());
    }
    let documents = app_handle.state::<DocumentState>();
    documents.with(&documentId, |app| {
        list_batch_render_files_with_zstd(
            source_root,
            PathBuf::from(output_root),
            &existing_png,
            &model_kind,
            &app.zstd,
        )
    })
}

fn list_batch_render_files_with_zstd(
    source_root: PathBuf,
    output_root: PathBuf,
    existing_png: &str,
    model_kind: &str,
    zstd: &crate::Zstd::TotkZstd<'_>,
) -> Result<Vec<BatchRenderFile>, String> {
    const MAX_SIZE: u64 = 500 * 1024 * 1024;
    let mut models = Vec::new();
    for entry in walkdir::WalkDir::new(&source_root) {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        // The size cap avoids content-probing arbitrarily large unrelated
        // files. GLB files are exempt: the extension is explicit intent, and
        // GLBs with embedded textures (e.g. downloaded Mii models) routinely
        // exceed the cap.
        // let is_glb_name = path
        //     .extension()
        //     .is_some_and(|extension| extension.eq_ignore_ascii_case("glb"));
        if let Ok(meta) = fs::metadata(path) {
            if meta.len() > MAX_SIZE  {
                continue;
            }
        }
        let relative = path
            .strip_prefix(&source_root)
            .map_err(|error| error.to_string())?;
        let mut output = output_root.join(relative);
        output.set_extension("png");
        if existing_png == "skip" && output.is_file() {
            continue;
        }
        if let Some(detected_kind) = crate::Settings::Pathlib::get_3d_model_type(path, zstd) {
            if model_kind != "all" && model_kind != detected_kind {
                continue;
            }
            models.push(BatchRenderFile {
                source: path.to_string_lossy().into_owned(),
                output: output.to_string_lossy().into_owned(),
                model_kind: detected_kind.into(),
            });
        }
    }
    models.sort_by(|left, right| left.source.to_lowercase().cmp(&right.source.to_lowercase()));
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::list_batch_render_files_with_zstd;
    use std::{path::Path, sync::Arc};

    #[test]
    fn discovers_glb_files_beyond_the_generic_size_cap() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../tmp/batch-render-glb-{}", std::process::id()));
        let output = root.join("renders");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // Detection only reads the magic; a full GLB body is not required.
        let mut oversized_glb = b"glTF".to_vec();
        oversized_glb.resize(2 * 1024 * 1024, 0);
        std::fs::write(root.join("model.glb"), &oversized_glb).unwrap();
        std::fs::write(root.join("small.glb"), b"glTF\x02\x00\x00\x00").unwrap();
        // Oversized non-GLB files must still be skipped by the size cap.
        std::fs::write(root.join("big.bin"), vec![0_u8; 2 * 1024 * 1024]).unwrap();

        let config = Arc::new(crate::TotkConfig::TotkConfig::default());
        let zstd = crate::Zstd::TotkZstd::dictionaryless(config, 19);
        let files =
            list_batch_render_files_with_zstd(root.clone(), output, "overwrite", "all", &zstd)
                .unwrap();
        assert_eq!(files.len(), 2);
        for file in &files {
            assert_eq!(file.model_kind, "glb");
            assert_eq!(
                Path::new(&file.output)
                    .extension()
                    .and_then(|value| value.to_str()),
                Some("png")
            );
        }
        let files = list_batch_render_files_with_zstd(
            root.clone(),
            root.join("renders"),
            "overwrite",
            "g1m",
            &zstd,
        )
        .unwrap();
        assert!(files.is_empty(), "kind filter must exclude GLB files");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn supplied_batch_render_corpus_is_discovered_and_renderable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_aocss/test");
        let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_aocss/renders");
        if !root.is_dir() {
            return;
        }
        let config = Arc::new(crate::TotkConfig::TotkConfig::safe_new(false).unwrap());
        let zstd = crate::Zstd::TotkZstd::new(config.clone(), 19)
            .unwrap_or_else(|_| crate::Zstd::TotkZstd::dictionaryless(config, 19));
        let files =
            list_batch_render_files_with_zstd(root, output, "overwrite", "all", &zstd).unwrap();
        assert_eq!(files.len(), 5);
        for file in files {
            let source = Path::new(&file.source);
            let model = crate::parser::AOC::g1m::G1mFile::from_path(source)
                .unwrap_or_else(|error| panic!("{}: {error}", source.display()));
            assert!(!model.render.meshes.is_empty(), "{}", source.display());
            assert_eq!(
                Path::new(&file.output)
                    .extension()
                    .and_then(|value| value.to_str()),
                Some("png")
            );
            assert_eq!(Path::new(&file.output).file_stem(), source.file_stem());
        }
    }
}
