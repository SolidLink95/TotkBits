//! TexToGo picture replacement: the pipeline behind `--cli txtg_edit`,
//! usable from the app and the Items Creator without going through the
//! command line.
//!
//! A `.txtg` (optionally Zstandard compressed) is parsed as the template,
//! the picture is re-encoded in the template's own format with the same mip
//! count, settings and hash, and the result is written back the way
//! [`SurfaceStyle`] asks: [`SurfaceStyle::Padded`] reproduces the bytes of
//! the established Switch texture editors, [`SurfaceStyle::Compact`] the
//! game's own layout. ASTC formats go through the bundled astcenc library
//! (`file_format::Image::astcenc`); BC formats are encoded natively.
use crate::file_format::Image::astcenc;
use crate::parser::textogo::{writer, TexToGoFile};
use crate::Zstd::TotkZstd;
use image::RgbaImage;
use std::path::{Path, PathBuf};
use std::{fs, io};

pub use crate::parser::textogo::writer::SurfaceStyle;

/// What a replacement produced.
#[derive(Clone, Debug)]
pub struct TxtgReplaceReport {
    /// Format code kept from the template.
    pub format: u16,
    pub width: u16,
    pub height: u16,
    pub mip_count: u8,
    /// ASTC block footprint when the texture is ASTC.
    pub astc_block: Option<(u32, u32)>,
    /// The astcenc library that encoded the ASTC mips.
    pub encoder: Option<PathBuf>,
    /// Size of the written file.
    pub bytes: usize,
    /// Whether the output was Zstandard compressed (`.zs`).
    pub compressed: bool,
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Parses a `.txtg` that may be Zstandard compressed (plain frame or one
/// of the game's dictionaries).
pub fn parse_texture(raw: &[u8], zstd: &TotkZstd<'_>) -> io::Result<TexToGoFile> {
    let raw = if crate::Settings::Magic::is_zstd(raw) {
        match zstd::decode_all(raw) {
            Ok(data) => data,
            Err(_) => zstd.try_decompress(raw)?,
        }
    } else {
        raw.to_vec()
    };
    TexToGoFile::parse(&raw).map_err(|error| invalid_data(error.to_string()))
}

/// Re-encodes `image` as the same kind of texture as `template` and
/// returns the uncompressed `.txtg` bytes.
pub fn encode_replacement(
    template: &TexToGoFile,
    image: &RgbaImage,
    style: SurfaceStyle,
) -> io::Result<Vec<u8>> {
    if writer::astc_block_from_textogo(&template.header).is_some() && !astcenc::is_available() {
        return Err(astcenc::missing_error());
    }
    let encoded = writer::from_rgba_with_options(image, template, style)
        .map_err(|error| invalid_data(error.to_string()))?;
    writer::write_with_style(&encoded, style).map_err(|error| invalid_data(error.to_string()))
}

/// Replaces the picture of `input` with `picture` and writes `output`,
/// Zstandard compressed when the output name ends with `.zs`.
pub fn replace_from_paths(
    input: &Path,
    picture: &Path,
    output: &Path,
    style: SurfaceStyle,
    zstd: &TotkZstd<'_>,
) -> io::Result<TxtgReplaceReport> {
    let raw = fs::read(input)?;
    let template = parse_texture(&raw, zstd)
        .map_err(|error| invalid_data(format!("{}: {error}", input.display())))?;
    let image = image::open(picture)
        .map_err(|error| invalid_data(format!("{}: {error}", picture.display())))?
        .to_rgba8();
    let mut bytes = encode_replacement(&template, &image, style)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", picture.display())))?;
    let compressed = output
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("zs"));
    if compressed {
        bytes = zstd
            .compressor
            .compress_zs(&bytes)
            .map_err(|error| io::Error::other(format!("zstd: {error}")))?;
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(output, &bytes)?;
    let header = &template.header;
    Ok(TxtgReplaceReport {
        format: header.format,
        width: image.width().min(u32::from(u16::MAX)) as u16,
        height: image.height().min(u32::from(u16::MAX)) as u16,
        mip_count: header
            .mip_count
            .max(1)
            .min(((u32::BITS - image.width().max(image.height()).leading_zeros()) as u8).max(1)),
        astc_block: writer::astc_block_from_textogo(header),
        encoder: astcenc::library_path(),
        bytes: bytes.len(),
        compressed,
    })
}
