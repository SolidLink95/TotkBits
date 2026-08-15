use aes::cipher::{BlockEncrypt, KeyInit};
use base64::Engine;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::file_format::BinTextFile::OpenedFile;
use crate::Open_and_Save::SendData;
use crate::Settings::Pathlib;
use crate::Zstd::TotkFileType;
use tauri::Manager;

const RENDERER: &str = "https://mii-unsecure.ariankordi.net/miis";
const QR_KEY: [u8; 16] = [
    0x59, 0xfc, 0x81, 0x7e, 0x64, 0x46, 0xea, 0x61, 0x90, 0x34, 0x7b, 0x20, 0xe9, 0xbd, 0xce, 0x52,
];

fn show_connection_error(message: &str) {
    rfd::MessageDialog::new()
        .set_title("TotkBits - Mii renderer unavailable")
        .set_description(format!(
            "Could not connect to the Mii renderer.\n\n{message}"
        ))
        .set_level(rfd::MessageLevel::Error)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn renderer_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(90))
        .user_agent("TotkBits/1.0 Mii renderer")
        .build()
        .map_err(|error| error.to_string())
}

fn supported_binary_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "charinfo"
            | "ufsd"
            | "miigx"
            | "mii"
            | "mae"
            | "rcd"
            | "rsd"
            | "cfsd"
            | "ffsd"
            | "3dsmii"
            | "cfcd"
            | "nfcd"
            | "nfsd"
            | "mnms"
    )
}

pub fn is_mii_binary_path(path: &Path) -> bool {
    return fs::read(path)
        .map(|bytes| looks_like_mii_binary(&bytes))
        .unwrap_or(false);
    // if supported_binary_extension(path) {
    //     return true;
    // }
}

fn looks_like_mii_binary(data: &[u8]) -> bool {
    matches!(data.len(), 72 | 74 | 76 | 88 | 92 | 96) && mii_name(data).is_some()
}

fn possible_qr_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "tif" | "tiff"
    )
}

fn decode_qr_payload(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let image = image::load_from_memory(bytes)
        .map_err(|error| format!("unable to decode QR image: {error}"))?
        .to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(image);
    let grids = prepared.detect_grids();
    let mut last_error = None;
    for grid in grids {
        let mut raw = Vec::new();
        let result = grid.decode_to(&mut raw);
        if !raw.is_empty() {
            // Some older Mii QR generators encoded arbitrary bytes as Latin-1
            // text and then UTF-8. Collapse that expansion when it yields a
            // complete wrapped Mii; otherwise retain the original byte mode.
            if let Ok(content) = std::str::from_utf8(&raw) {
                let normalized: Option<Vec<u8>> = content
                    .chars()
                    .map(|character| u8::try_from(character as u32).ok())
                    .collect();
                if let Some(normalized) = normalized.filter(|value| value.len() >= 112) {
                    return Ok(normalized);
                }
            }
            return Ok(raw);
        }
        match result {
            Ok(_) => last_error = Some("QR code has an empty payload".to_string()),
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    Err(last_error.unwrap_or_else(|| "image does not contain a QR code".to_string()))
}

/// Decrypt CFLiWrappedMiiData from a 3DS/Wii U Mii QR code.
fn unwrap_qr_store_data(wrapped: &[u8]) -> Result<Vec<u8>, String> {
    if wrapped.len() < 112 {
        return Err(format!(
            "unsupported Mii QR payload size: {} bytes (expected at least 112)",
            wrapped.len()
        ));
    }
    let cipher = aes::Aes128::new_from_slice(&QR_KEY).map_err(|error| error.to_string())?;
    let mut plaintext = wrapped[8..96].to_vec();
    let mut nonce = [0_u8; 12];
    nonce[..8].copy_from_slice(&wrapped[..8]);

    // CCM with a 12-byte nonce uses a three-byte, big-endian counter. Nintendo's
    // QR implementation has a known authentication-tag erratum, so decrypt the
    // CTR stream and rely on the renderer's StoreData CRC validation.
    for (index, chunk) in plaintext.chunks_mut(16).enumerate() {
        let counter = (index + 1) as u32;
        let mut block = aes::Block::default();
        block[0] = 2; // L - 1, with L = 3.
        block[1..13].copy_from_slice(&nonce);
        block[13] = (counter >> 16) as u8;
        block[14] = (counter >> 8) as u8;
        block[15] = counter as u8;
        cipher.encrypt_block(&mut block);
        for (byte, key_byte) in chunk.iter_mut().zip(block.iter()) {
            *byte ^= key_byte;
        }
    }

    let mut store_data = Vec::with_capacity(96);
    store_data.extend_from_slice(&plaintext[..12]);
    store_data.extend_from_slice(&wrapped[..8]);
    store_data.extend_from_slice(&plaintext[12..]);
    Ok(store_data)
}

pub fn read_mii_data(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    if supported_binary_extension(path) || looks_like_mii_binary(&bytes) {
        if bytes.is_empty() {
            return Err("Mii file is empty".into());
        }
        return Ok(bytes);
    }
    if possible_qr_image(path) {
        return unwrap_qr_store_data(&decode_qr_payload(&bytes)?);
    }
    Err("unsupported Mii file type".into())
}

fn decode_utf16_name(bytes: &[u8], big_endian: bool) -> String {
    let units = bytes.chunks_exact(2).map(|pair| {
        if big_endian {
            u16::from_be_bytes([pair[0], pair[1]])
        } else {
            u16::from_le_bytes([pair[0], pair[1]])
        }
    });
    String::from_utf16_lossy(&units.take_while(|unit| *unit != 0).collect::<Vec<_>>())
        .trim()
        .to_string()
}

fn mii_name(data: &[u8]) -> Option<String> {
    let name = match data.len() {
        // nn::mii::CharInfo
        88 => decode_utf16_name(&data[16..36], false),
        // RFLCharData / RFLStoreData
        74 | 76 => decode_utf16_name(&data[2..22], true),
        // FFLiMiiDataCore / Official / StoreData
        72 | 92 | 96 => decode_utf16_name(&data[26..46], false),
        _ => return None,
    };
    (!name.is_empty()).then_some(name)
}

fn safe_mii_filename(name: &str) -> String {
    let safe = name
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0'..='\x1f' => '_',
            _ => character,
        })
        .collect::<String>();
    let safe = safe.trim().trim_end_matches(['.', ' ']);
    if safe.is_empty() {
        "Mii".into()
    } else {
        safe.into()
    }
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn read_mii_name(app_handle: tauri::AppHandle, documentId: String) -> Result<String, String> {
    let documents = app_handle.state::<crate::DocumentState::DocumentState>();
    let (enabled, file_type, path, internal_data) = documents.with(&documentId, |app| {
        (
            app.zstd.totk_config.mii_renderer,
            app.opened_file.file_type,
            app.opened_file.path.full_path.clone(),
            app.opened_file.mii_data.clone(),
        )
    });
    if !enabled {
        return Err("Failed to parse file".into());
    }
    if file_type != TotkFileType::Mii {
        return Err("active document is not a Mii".into());
    }
    let data = match internal_data {
        Some(data) => data,
        None => read_mii_data(Path::new(&path))?,
    };
    mii_name(&data).ok_or_else(|| "Mii name is unavailable in this format".into())
}

fn request_render(data: &[u8], extension: &str, width: u32) -> Result<Vec<u8>, String> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    let url = format!("{RENDERER}/image.{extension}");
    let response = renderer_client()?
        .post(url)
        .query(&[
            ("erri", "spc9r-rsm".to_string()),
            ("data", encoded),
            ("type", "face".to_string()),
            ("width", width.to_string()),
        ])
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .send()
        .map_err(|error| {
            let message = error.to_string();
            show_connection_error(&message);
            format!("Mii renderer request failed: {message}")
        })?;
    let status = response.status();
    let result = response
        .bytes()
        .map_err(|error| format!("unable to read Mii renderer response: {error}"))?
        .to_vec();
    if !status.is_success() {
        let message = String::from_utf8_lossy(&result);
        return Err(format!("Mii renderer returned HTTP {status}: {message}"));
    }
    match extension {
        "png" if !result.starts_with(b"\x89PNG\r\n\x1a\n") => {
            Err("Mii renderer did not return a PNG".into())
        }
        "glb" if result.len() < 12 || !result.starts_with(b"glTF") => {
            Err("Mii renderer did not return a GLB".into())
        }
        _ => Ok(result),
    }
}

fn unique_sibling(path: &Path, extension: &str) -> PathBuf {
    let first = path.with_extension(extension);
    if !first.exists() {
        return first;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("mii");
    for suffix in 1_u32.. {
        let candidate = parent.join(format!("{stem}_{suffix}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn error_result(path: &Path, message: String) -> (OpenedFile<'static>, SendData) {
    let mut data = SendData::default();
    data.path = Pathlib::new(path);
    data.tab = "ERROR".into();
    data.status_text = format!("Error opening Mii: {message}");
    data.text = data.status_text.clone();
    (OpenedFile::default(), data)
}

pub fn open(path: &Path) -> Option<(OpenedFile<'static>, SendData)> {
    let is_binary = supported_binary_extension(path)
        || fs::read(path)
            .map(|bytes| looks_like_mii_binary(&bytes))
            .unwrap_or(false);
    if !is_binary && !possible_qr_image(path) {
        return None;
    }
    let mii_data = match read_mii_data(path) {
        Ok(data) => data,
        Err(_) if !is_binary => return None, // A normal image, not a Mii QR.
        Err(error) => return Some(error_result(path, error)),
    };
    let png = match request_render(&mii_data, "png", 1024) {
        Ok(png) => png,
        Err(error) => return Some(error_result(path, error)),
    };
    let preview_path = unique_sibling(path, "png");
    if let Err(error) = fs::write(&preview_path, &png) {
        return Some(error_result(path, error.to_string()));
    }

    let mut opened = OpenedFile::from_path(path.to_string_lossy().into_owned(), TotkFileType::Mii);
    opened.visual_data = Some(png);
    let mut data = SendData::default();
    data.path = Pathlib::new(path);
    data.set_file_metadata(TotkFileType::Mii, None);
    data.file_label = format!("{} [Mii]", data.path.name);
    data.tab = "IMAGE".into();
    data.read_only = true;
    data.status_text = format!("Rendered HD Mii preview to {}", preview_path.display());
    Some((opened, data))
}

pub fn open_binary(path: &Path, mii_data: &[u8]) -> Option<(OpenedFile<'static>, SendData)> {
    if !looks_like_mii_binary(mii_data) {
        return None;
    }
    let png = match request_render(mii_data, "png", 1024) {
        Ok(png) => png,
        Err(error) => return Some(error_result(path, error)),
    };
    let mut opened = OpenedFile::from_path(path.to_string_lossy().into_owned(), TotkFileType::Mii);
    opened.visual_data = Some(png);
    opened.mii_data = Some(mii_data.to_vec());
    let mut data = SendData::default();
    data.path = Pathlib::new(path);
    data.set_file_metadata(TotkFileType::Mii, None);
    data.file_label = format!("{} [Mii]", data.path.name);
    data.tab = "IMAGE".into();
    data.read_only = true;
    data.status_text = format!("Rendered Mii preview from {}", path.display());
    Some((opened, data))
}

pub fn download_glb(path: &Path) -> Result<PathBuf, String> {
    let mii_data = read_mii_data(path)?;
    download_glb_data(&mii_data, path)
}

fn download_glb_data(mii_data: &[u8], output_base: &Path) -> Result<PathBuf, String> {
    let glb = request_render(&mii_data, "glb", 1024)?;
    if glb.len() < 12 || !glb.starts_with(b"glTF") {
        return Err("Mii renderer did not return a valid GLB".into());
    }
    let declared_length = u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize;
    if declared_length != glb.len() {
        return Err(format!(
            "incomplete GLB: header declares {declared_length} bytes, received {}",
            glb.len()
        ));
    }
    let output = unique_sibling(output_base, "glb");
    fs::write(&output, glb).map_err(|error| error.to_string())?;
    Ok(output)
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn download_mii_glb(
    app_handle: tauri::AppHandle,
    documentId: String,
    databasePath: Option<String>,
) -> Result<String, String> {
    let documents = app_handle.state::<crate::DocumentState::DocumentState>();
    let (enabled, file_type, path, mii_data, parent) = documents.with(&documentId, |app| {
        (
            app.zstd.totk_config.mii_renderer,
            app.opened_file.file_type,
            app.opened_file.path.full_path.clone(),
            app.opened_file.mii_data.clone(),
            app.internal_parent.clone(),
        )
    });
    if !enabled {
        return Err("Failed to parse file".into());
    }
    if file_type != TotkFileType::Mii {
        return Err("Download GLB is only available for Mii documents".into());
    }
    let output = if let Some(mii_data) = mii_data {
        let database_path = databasePath
            .or_else(|| {
                parent.map(|parent| {
                    documents.with(&parent.document_id, |app| {
                        app.opened_file.path.full_path.clone()
                    })
                })
            })
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "RFL_DB source path is unavailable".to_string())?;
        let directory = Path::new(&database_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let name = mii_name(&mii_data)
            .map(|name| safe_mii_filename(&name))
            .unwrap_or_else(|| "Mii".into());
        download_glb_data(&mii_data, &directory.join(format!("{name}.miigx")))?
    } else {
        download_glb(Path::new(&path))?
    };
    Ok(output.to_string_lossy().replace('\\', "/"))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn read_glb_preview(
    app_handle: tauri::AppHandle,
    documentId: String,
) -> Result<String, String> {
    let documents = app_handle.state::<crate::DocumentState::DocumentState>();
    let (file_type, bytes) = documents.with(&documentId, |app| {
        (
            app.opened_file.file_type,
            app.opened_file.visual_data.clone(),
        )
    });
    if file_type != TotkFileType::Glb {
        return Err("active document is not a GLB preview".into());
    }
    let bytes = bytes.ok_or_else(|| "GLB preview data is missing".to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn export_loaded_glb(
    app_handle: tauri::AppHandle,
    documentId: String,
    output: String,
) -> Result<String, String> {
    if !output.to_ascii_lowercase().ends_with(".glb") {
        return Err("GLB export requires a .glb filename".into());
    }
    let documents = app_handle.state::<crate::DocumentState::DocumentState>();
    let (file_type, bytes) = documents.with(&documentId, |app| {
        (
            app.opened_file.file_type,
            app.opened_file.visual_data.clone(),
        )
    });
    if file_type != TotkFileType::Glb {
        return Err("active document is not a GLB".into());
    }
    let bytes = bytes.ok_or_else(|| "GLB data is missing".to_string())?;
    fs::write(&output, bytes).map_err(|error| error.to_string())?;
    Ok(output.replace('\\', "/"))
}

pub fn open_glb(path: &Path) -> io::Result<(OpenedFile<'static>, SendData)> {
    let bytes = fs::read(path)?;
    if bytes.len() < 12 || !bytes.starts_with(b"glTF") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid GLB header",
        ));
    }
    let mut opened = OpenedFile::from_path(path.to_string_lossy().into_owned(), TotkFileType::Glb);
    opened.visual_data = Some(bytes);
    let mut data = SendData::default();
    data.path = Pathlib::new(path);
    data.set_file_metadata(TotkFileType::Glb, None);
    data.file_label = format!("{} [GLB]", data.path.name);
    data.tab = "3D".into();
    data.read_only = true;
    data.status_text = format!("Opened GLB preview: {}", path.display());
    Ok((opened, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_sibling_uses_requested_extension() {
        let result = unique_sibling(Path::new("Mii.charinfo"), "glb");
        assert_eq!(result, PathBuf::from("Mii.glb"));
    }

    #[test]
    fn supplied_qr_corpus_decodes_to_store_data() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/mii/data/qr_clean");
        if !corpus.is_dir() {
            return;
        }
        let mut decoded = 0;
        for entry in fs::read_dir(corpus).unwrap() {
            let path = entry.unwrap().path();
            if !path.is_file() {
                continue;
            }
            let image = fs::read(&path).unwrap();
            let payload = decode_qr_payload(&image)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let store_data = unwrap_qr_store_data(&payload)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(store_data.len(), 96, "{}", path.display());
            decoded += 1;
        }
        assert!(decoded >= 25, "expected the supplied QR corpus");
    }

    #[test]
    fn reads_names_from_supplied_switch_and_wii_miis() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/mii");
        let charinfo = fs::read(root.join("Alphie.charinfo")).unwrap();
        assert_eq!(mii_name(&charinfo).as_deref(), Some("Alphie"));
        assert!(looks_like_mii_binary(&charinfo));

        let disguised = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("mii-content-sniff-{}.asdf", std::process::id()));
        fs::create_dir_all(disguised.parent().unwrap()).unwrap();
        fs::write(&disguised, &charinfo).unwrap();
        assert!(is_mii_binary_path(&disguised));
        assert_eq!(read_mii_data(&disguised).unwrap(), charinfo);
        fs::remove_file(disguised).unwrap();

        let miigx = fs::read(root.join("data/miigx/MiiCharInfo000.miigx")).unwrap();
        assert_eq!(mii_name(&miigx).as_deref(), Some("MiiCharInf"));
        assert!(looks_like_mii_binary(&miigx));
        assert!(!looks_like_mii_binary(&[0_u8; 88]));
    }

    #[test]
    #[ignore = "contacts the public Mii renderer"]
    fn live_renderer_accepts_charinfo_and_qr_data() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/mii/data");
        let charinfo = fs::read(root.join("miis_charinfo/Alphie.charinfo")).unwrap();
        assert!(request_render(&charinfo, "png", 1024)
            .unwrap()
            .starts_with(b"\x89PNG"));
        let qr_image = fs::read(root.join("qr_clean/Alphie.jpg")).unwrap();
        let wrapped = decode_qr_payload(&qr_image).unwrap();
        let qr_data = unwrap_qr_store_data(&wrapped).unwrap();
        assert!(request_render(&qr_data, "glb", 1024)
            .unwrap()
            .starts_with(b"glTF"));
        let wii = fs::read(root.join("miigx/MiiCharInfo000.miigx")).unwrap();
        assert!(request_render(&wii, "glb", 1024)
            .unwrap()
            .starts_with(b"glTF"));
    }
}
