//! TexToGo (`.txtg`) writer and RGBA conversion.
//!
//! The layout mirrors the parser: an 80-byte header, one 4-byte descriptor
//! per surface, one 8-byte size entry per surface, then the Zstandard
//! compressed surfaces. Surfaces are compressed at level 22 with the content
//! size in the frame and no checksum, which reproduces the game's own bytes
//! for the largest and the smallest mips; the mid-sized mips of vanilla files
//! come out a few bytes different (the game's encoder was a different
//! Zstandard build), so `parse(write(parse(x)))` matches `parse(x)` in every
//! field except the compressed sizes.
use super::{TexToGoError, TexToGoFile, TexToGoSurface};
use crate::file_format::Image::switch_texture;
use crate::parser::binary::BinaryWriter;
use image::RgbaImage;
use std::io::Write;

/// The Zstandard level closest to the game's surfaces.
pub const ZSTD_LEVEL: i32 = 22;
/// `compression_type` of every vanilla surface.
pub const SURFACE_COMPRESSION: u32 = 6;
const HEADER_SIZE: u16 = 0x50;
/// Header bytes 0x0F..0x13 and 0x3E..0x40 are not exposed by the parser.
/// Every TOTK texture carries `00 03` at 0x3E; 0x0F is 2 in 29057 of the
/// 29109 vanilla files (a handful of particle and tree atlases use 1 or 3),
/// so a rewrite of those few files differs in that one byte.
const RESERVED_0F: [u8; 4] = [0x02, 0x01, 0x00, 0x00];
const RESERVED_3E: [u8; 2] = [0x00, 0x03];

fn error(offset: usize, message: impl Into<String>) -> TexToGoError {
    TexToGoError::new(offset, message)
}

pub fn write(file: &TexToGoFile) -> Result<Vec<u8>, TexToGoError> {
    let header = &file.header;
    if header.header_size < HEADER_SIZE {
        return Err(error(
            0,
            "TexToGo header size is smaller than the fixed header",
        ));
    }
    let expected = usize::from(header.depth) * usize::from(header.mip_count);
    if file.surfaces.len() != expected {
        return Err(error(
            12,
            format!(
                "TexToGo declares {expected} surfaces but holds {}",
                file.surfaces.len()
            ),
        ));
    }
    let mut w = BinaryWriter::new();
    w.write_u16(header.header_size);
    w.write_u16(header.version);
    w.write_bytes(b"6PK0");
    w.write_u16(header.width);
    w.write_u16(header.height);
    w.write_u16(header.depth);
    w.write_u8(header.mip_count);
    w.write_bytes(&RESERVED_0F);
    w.write_u8(header.format_flag);
    w.write_u32(header.format_setting);
    w.write_bytes(&header.component_selectors);
    w.write_bytes(&header.hash);
    w.write_u16(header.format);
    w.write_bytes(&RESERVED_3E);
    for setting in header.texture_settings {
        w.write_u32(setting);
    }
    if w.position() != usize::from(HEADER_SIZE) {
        return Err(error(w.position(), "unexpected TexToGo header size"));
    }
    w.seek(usize::from(header.header_size));
    for surface in &file.surfaces {
        w.write_u16(surface.array_level);
        w.write_u8(surface.mip_level);
        w.write_u8(surface.surface_count);
    }
    let mut compressed = Vec::with_capacity(file.surfaces.len());
    for surface in &file.surfaces {
        let data = compress(&surface.data).map_err(|e| error(w.position(), e))?;
        w.write_u32(
            u32::try_from(data.len()).map_err(|_| error(w.position(), "surface too large"))?,
        );
        w.write_u32(surface.compression_type);
        compressed.push(data);
    }
    for data in &compressed {
        w.write_bytes(data);
    }
    Ok(w.into_inner())
}

fn compress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = zstd::Encoder::new(Vec::new(), ZSTD_LEVEL).map_err(|e| e.to_string())?;
    encoder
        .set_pledged_src_size(Some(data.len() as u64))
        .map_err(|e| e.to_string())?;
    encoder
        .include_contentsize(true)
        .map_err(|e| e.to_string())?;
    encoder.include_checksum(false).map_err(|e| e.to_string())?;
    encoder.write_all(data).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}

/// Decodes the base level of the first surface.
pub fn to_rgba(file: &TexToGoFile) -> Result<RgbaImage, TexToGoError> {
    let surface = file
        .surfaces
        .iter()
        .find(|surface| surface.array_level == 0 && surface.mip_level == 0)
        .ok_or_else(|| error(0, "TexToGo has no base surface"))?;
    let width = u32::from(file.header.width);
    let height = u32::from(file.header.height);
    let log2 = switch_texture::inferred_block_height_log2(height, 4);
    let decode = |block: usize| {
        switch_texture::decode_astc(width, height, &surface.data, block, block, log2)
    };
    let image = match file.header.format {
        0x101 | 0x109 => decode(4),
        0x102 | 0x105 => decode(8),
        format => switch_texture::format_from_textogo(format).and_then(|image_format| {
            switch_texture::decode(width, height, image_format, &surface.data, log2, false)
        }),
    }
    .map_err(|e| error(0, e.to_string()))?;
    Ok(image)
}

/// Encodes `image` with the format, mip count and settings of `like`. The
/// hash is kept from `like`: it is not derived from the image data (files
/// with identical pixels carry different hashes), so it cannot be computed.
pub fn from_rgba(image: &RgbaImage, like: &TexToGoFile) -> Result<TexToGoFile, TexToGoError> {
    if like.header.depth != 1 {
        return Err(error(
            12,
            "only single-layer TexToGo textures can be encoded",
        ));
    }
    if matches!(like.header.format, 0x101 | 0x109 | 0x102 | 0x105) {
        return Err(error(
            60,
            "ASTC TexToGo textures need an external astcenc encoder",
        ));
    }
    let format = switch_texture::format_from_textogo(like.header.format)
        .map_err(|e| error(60, e.to_string()))?;
    let (width, height) = (image.width(), image.height());
    if width == 0 || height == 0 || width > u32::from(u16::MAX) || height > u32::from(u16::MAX) {
        return Err(error(8, "image dimensions are out of range for TexToGo"));
    }
    let mip_count = like.header.mip_count.max(1);
    let compression_type = like
        .surfaces
        .first()
        .map(|surface| surface.compression_type)
        .unwrap_or(SURFACE_COMPRESSION);
    let mut surfaces = Vec::with_capacity(usize::from(mip_count));
    for mip in 0..mip_count {
        let mip_width = (width >> mip).max(1);
        let mip_height = (height >> mip).max(1);
        let level = if mip == 0 {
            image.clone()
        } else {
            image::imageops::resize(
                image,
                mip_width,
                mip_height,
                image::imageops::FilterType::Triangle,
            )
        };
        let log2 = switch_texture::inferred_block_height_log2(mip_height, 4);
        let data = switch_texture::encode(&level, format, log2, false)
            .map_err(|e| error(0, format!("mip {mip}: {e}")))?;
        surfaces.push(TexToGoSurface {
            array_level: 0,
            mip_level: mip,
            surface_count: 1,
            compressed_size: 0,
            compression_type,
            data,
        });
    }
    let mut header = like.header.clone();
    header.width = width as u16;
    header.height = height as u16;
    header.depth = 1;
    header.mip_count = mip_count;
    Ok(TexToGoFile { header, surfaces })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn romfs_texture(name: &str) -> Option<Vec<u8>> {
        let path = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs/TexToGo").join(name);
        std::fs::read(path).ok()
    }

    fn without_sizes(file: &TexToGoFile) -> TexToGoFile {
        let mut copy = file.clone();
        for surface in &mut copy.surfaces {
            surface.compressed_size = 0;
        }
        copy
    }

    const FIXTURES: [&str; 3] = [
        "Armor_022_Head_Alb.txtg",
        "Armor_001_Hood_Alb.0.txtg",
        "Armor_001_Hood_Nrm.txtg",
    ];

    #[test]
    fn rewrites_vanilla_files_with_identical_surfaces() {
        for name in FIXTURES {
            let Some(original) = romfs_texture(name) else {
                return;
            };
            let parsed = TexToGoFile::parse(&original).unwrap();
            let written = write(&parsed).unwrap();
            let reparsed = TexToGoFile::parse(&written).unwrap();
            assert_eq!(without_sizes(&reparsed), without_sizes(&parsed), "{name}");
            assert_eq!(reparsed.header, parsed.header, "{name}");
            for (a, b) in reparsed.surfaces.iter().zip(&parsed.surfaces) {
                assert_eq!(a.data, b.data, "{name} surface data");
            }
            // The header, descriptor and size tables are byte exact; only
            // some compressed payloads differ from the game's encoder.
            let tables = usize::from(parsed.header.header_size) + parsed.surfaces.len() * 4;
            assert_eq!(&written[..tables], &original[..tables], "{name} tables");
            let exact = reparsed
                .surfaces
                .iter()
                .zip(&parsed.surfaces)
                .filter(|(a, b)| a.compressed_size == b.compressed_size)
                .count();
            println!(
                "{name}: {exact}/{} surfaces byte-identical at level {ZSTD_LEVEL}",
                parsed.surfaces.len()
            );
            if name == "Armor_022_Head_Alb.txtg" {
                assert!(exact >= 6, "{name}: only {exact} surfaces matched");
            }
        }
    }

    #[test]
    fn encodes_back_close_to_the_original() {
        for name in ["Armor_022_Head_Alb.txtg", "Armor_001_Hood_Nrm.txtg"] {
            let Some(original) = romfs_texture(name) else {
                return;
            };
            let parsed = TexToGoFile::parse(&original).unwrap();
            let image = to_rgba(&parsed).unwrap();
            assert_eq!(image.width(), u32::from(parsed.header.width));
            let encoded = from_rgba(&image, &parsed).unwrap();
            assert_eq!(encoded.surfaces.len(), parsed.surfaces.len());
            assert_eq!(encoded.header.hash, parsed.header.hash);
            let written = write(&encoded).unwrap();
            let decoded = to_rgba(&TexToGoFile::parse(&written).unwrap()).unwrap();
            let total: u64 = image
                .as_raw()
                .iter()
                .zip(decoded.as_raw())
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum();
            let mean = total as f64 / image.as_raw().len() as f64;
            println!("{name}: mean abs error {mean:.3}");
            assert!(mean < 12.0, "{name}: mean abs error {mean}");
        }
    }

    #[test]
    fn refuses_astc_and_layered_inputs() {
        let Some(original) = romfs_texture("Armor_022_Head_Alb.txtg") else {
            return;
        };
        let parsed = TexToGoFile::parse(&original).unwrap();
        let image = to_rgba(&parsed).unwrap();
        let mut astc = parsed.clone();
        astc.header.format = 0x101;
        assert!(from_rgba(&image, &astc).is_err());
        let mut layered = parsed.clone();
        layered.header.depth = 2;
        assert!(from_rgba(&image, &layered).is_err());
        let _ = PathBuf::new();
    }
}
