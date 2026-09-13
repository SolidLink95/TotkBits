use image_dds::{ImageFormat, Mipmaps, Quality};
use std::{fs, io, path::Path};

pub fn image_format(name: &str) -> io::Result<ImageFormat> {
    match name {
        "BC1" | "BC1_UNORM" => Ok(ImageFormat::BC1RgbaUnorm),
        "BC1_SRGB" => Ok(ImageFormat::BC1RgbaUnormSrgb),
        "BC2_UNORM" => Ok(ImageFormat::BC2RgbaUnorm),
        "BC2_SRGB" => Ok(ImageFormat::BC2RgbaUnormSrgb),
        "BC3" | "BC3_UNORM" => Ok(ImageFormat::BC3RgbaUnorm),
        "BC3_SRGB" => Ok(ImageFormat::BC3RgbaUnormSrgb),
        "BC4_UNORM" => Ok(ImageFormat::BC4RUnorm),
        "BC4_SNORM" => Ok(ImageFormat::BC4RSnorm),
        "BC5" | "BC5_UNORM" => Ok(ImageFormat::BC5RgUnorm),
        "BC5_SNORM" => Ok(ImageFormat::BC5RgSnorm),
        "BC6_UFLOAT" => Ok(ImageFormat::BC6hRgbUfloat),
        "BC6_FLOAT" => Ok(ImageFormat::BC6hRgbSfloat),
        "BC7" | "BC7_UNORM" => Ok(ImageFormat::BC7RgbaUnorm),
        "BC7_SRGB" => Ok(ImageFormat::BC7RgbaUnormSrgb),
        "RGBA8" | "R8_G8_B8_A8_UNORM" => Ok(ImageFormat::Rgba8Unorm),
        "R8_G8_B8_A8_SRGB" => Ok(ImageFormat::Rgba8UnormSrgb),
        "B8_G8_R8_A8_UNORM" => Ok(ImageFormat::Bgra8Unorm),
        "B8_G8_R8_A8_SRGB" => Ok(ImageFormat::Bgra8UnormSrgb),
        "R8_UNORM" => Ok(ImageFormat::R8Unorm),
        "R8_G8_UNORM" => Ok(ImageFormat::Rg8Unorm),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported DDS type {name}"),
        )),
    }
}

pub fn replace_from_png(
    target: impl AsRef<Path>,
    png: impl AsRef<Path>,
    format: &str,
    mip_count: u32,
) -> io::Result<()> {
    let source = image::open(png)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?
        .to_rgba8();
    let mipmaps = if mip_count <= 1 {
        Mipmaps::Disabled
    } else {
        Mipmaps::GeneratedExact(mip_count)
    };
    let dds = image_dds::dds_from_image(&source, image_format(format)?, Quality::Normal, mipmaps)
        .map_err(|error| io::Error::other(error.to_string()))?;
    let mut encoded = Vec::new();
    dds.write(&mut encoded)
        .map_err(|error| io::Error::other(error.to_string()))?;
    fs::write(target, encoded)
}

pub fn replace_surface_from_png(
    target: impl AsRef<Path>,
    png: impl AsRef<Path>,
    array_index: u32,
    mip_index: u32,
    replacement_format: Option<&str>,
) -> io::Result<()> {
    let mut dds = image_dds::ddsfile::Dds::read(&mut std::io::Cursor::new(fs::read(&target)?))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    let original_format = image_dds::dds_image_format(&dds).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported DDS format: {error:?}"),
        )
    })?;
    let mut surface = image_dds::SurfaceRgba8::decode_dds(&dds)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    if array_index >= surface.layers || mip_index >= surface.mipmaps {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "DDS subimage index is out of range",
        ));
    }
    let replacement = image::open(png)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?
        .to_rgba8();
    let expected_width = (surface.width >> mip_index).max(1);
    let expected_height = (surface.height >> mip_index).max(1);
    if replacement.dimensions() != (expected_width, expected_height) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("replacement must be {expected_width} x {expected_height} pixels"),
        ));
    }
    let mut offset = 0usize;
    for layer in 0..surface.layers {
        for mip in 0..surface.mipmaps {
            let length = ((surface.width >> mip).max(1) as usize)
                .saturating_mul((surface.height >> mip).max(1) as usize)
                .saturating_mul(4);
            if layer == array_index && mip == mip_index {
                if replacement.as_raw().len() != length {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "replacement pixel data does not match the DDS surface size",
                    ));
                }
                crate::parser::binary::BinaryPatcher::new(&mut surface.data)
                    .write_bytes_at(offset, replacement.as_raw())
                    .map_err(|_| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "DDS surface data is too small for the replacement",
                        )
                    })?;
            }
            offset = offset.saturating_add(length);
        }
    }
    let format = match replacement_format {
        Some(name) if name != "ORIGINAL" => image_format(name)?,
        _ => original_format,
    };
    dds = surface
        .encode_dds(format, Quality::Normal, Mipmaps::FromSurface)
        .map_err(|error| io::Error::other(error.to_string()))?;
    let mut encoded = Vec::new();
    dds.write(&mut encoded)
        .map_err(|error| io::Error::other(error.to_string()))?;
    fs::write(target, encoded)
}
