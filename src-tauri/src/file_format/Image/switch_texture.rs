use crate::parser::binary::{BinaryWriter, Endian as BinaryEndian};
use image::{ImageBuffer, RgbaImage};
use image_dds::{ImageFormat, Mipmaps, Quality, Surface, SurfaceRgba8};
use std::io;
use std::num::NonZeroUsize;
use tegra_swizzle::{
    surface::{deswizzle_surface, swizzle_surface, swizzled_surface_size, BlockDim},
    BlockHeight,
};

pub fn decode_astc(
    width: u32,
    height: u32,
    swizzled: &[u8],
    block_width: usize,
    block_height: usize,
    block_height_log2: u8,
) -> io::Result<RgbaImage> {
    let mut block_dim = BlockDim::uncompressed();
    block_dim.width = NonZeroUsize::new(block_width)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "ASTC block width is zero"))?;
    block_dim.height = NonZeroUsize::new(block_height)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "ASTC block height is zero"))?;
    let tegra_block_height = BlockHeight::new(1usize << block_height_log2)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid Tegra block height"))?;
    let expected = swizzled_surface_size(
        width as usize,
        height as usize,
        1,
        block_dim,
        Some(tegra_block_height),
        16,
        1,
        1,
    );
    let mut padded = Vec::new();
    let input = if swizzled.len() < expected {
        padded.extend_from_slice(swizzled);
        padded.resize(expected, 0);
        padded.as_slice()
    } else {
        swizzled
    };
    let linear = deswizzle_surface(
        width as usize,
        height as usize,
        1,
        input,
        block_dim,
        Some(tegra_block_height),
        16,
        1,
        1,
    )
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    let mut pixels = vec![0u32; width as usize * height as usize];
    // texture2ddecoder panics on some malformed block payloads instead of
    // returning an error; a bad game texture must not take down the app.
    crate::Settings::catch_panic(|| {
        texture2ddecoder::decode_astc(
            &linear,
            width as usize,
            height as usize,
            block_width,
            block_height,
            &mut pixels,
        )
        .map_err(str::to_owned)
    })
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("ASTC decode: {error}")))?;
    let rgba = pixels
        .into_iter()
        .flat_map(|pixel| {
            let [blue, green, red, alpha] = pixel.to_le_bytes();
            [red, green, blue, alpha]
        })
        .collect();
    ImageBuffer::from_raw(width, height, rgba).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "decoded ASTC dimensions do not match",
        )
    })
}

pub fn decode(
    width: u32,
    height: u32,
    format: ImageFormat,
    swizzled: &[u8],
    block_height_log2: u8,
    linear: bool,
) -> io::Result<RgbaImage> {
    let (block_width, block_height, bytes_per_block) = format_layout(format)?;
    let blocks_wide = width.div_ceil(block_width);
    let blocks_high = height.div_ceil(block_height);
    let linear_size = (blocks_wide * blocks_high * bytes_per_block) as usize;
    let data = if linear {
        swizzled.get(..linear_size).unwrap_or(swizzled).to_vec()
    } else {
        let block_dim = if block_width == 4 && block_height == 4 {
            BlockDim::block_4x4()
        } else {
            BlockDim::uncompressed()
        };
        let block_height = BlockHeight::new(1usize << block_height_log2).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "invalid Tegra block height")
        })?;
        let expected = swizzled_surface_size(
            width as usize,
            height as usize,
            1,
            block_dim,
            Some(block_height),
            bytes_per_block as usize,
            1,
            1,
        );
        let padded;
        let swizzled = if swizzled.len() < expected {
            padded = {
                let mut value = Vec::with_capacity(expected);
                value.extend_from_slice(swizzled);
                value.resize(expected, 0);
                value
            };
            padded.as_slice()
        } else {
            swizzled
        };
        deswizzle_surface(
            width as usize,
            height as usize,
            1,
            swizzled,
            block_dim,
            Some(block_height),
            bytes_per_block as usize,
            1,
            1,
        )
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?
    };
    let decoded = Surface {
        width,
        height,
        depth: 1,
        layers: 1,
        mipmaps: 1,
        image_format: format,
        data,
    }
    .decode_rgba8()
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    ImageBuffer::from_raw(width, height, decoded.data).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "decoded texture dimensions do not match",
        )
    })
}

pub fn encode(
    image: &RgbaImage,
    format: ImageFormat,
    block_height_log2: u8,
    linear: bool,
) -> io::Result<Vec<u8>> {
    let width = image.width();
    let height = image.height();
    let (block_width, block_height, bytes_per_block) = format_layout(format)?;
    let mut encoded = SurfaceRgba8::from_image(image)
        .encode(format, Quality::Normal, Mipmaps::Disabled)
        .map_err(|error| io::Error::other(error.to_string()))?;
    if matches!(
        format,
        ImageFormat::BC1RgbaUnorm | ImageFormat::BC1RgbaUnormSrgb
    ) {
        encode_bc1_alpha_blocks(image, &mut encoded.data)?;
    }
    if linear {
        return Ok(encoded.data);
    }
    let block_dim = if block_width == 4 && block_height == 4 {
        BlockDim::block_4x4()
    } else {
        BlockDim::uncompressed()
    };
    let block_height = BlockHeight::new(1usize << block_height_log2)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid Tegra block height"))?;
    swizzle_surface(
        width as usize,
        height as usize,
        1,
        &encoded.data,
        block_dim,
        Some(block_height),
        bytes_per_block as usize,
        1,
        1,
    )
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
}

fn encode_bc1_alpha_blocks(image: &RgbaImage, blocks: &mut Vec<u8>) -> io::Result<()> {
    let blocks_wide = image.width().div_ceil(4) as usize;
    let blocks_high = image.height().div_ceil(4) as usize;
    if blocks.len() < blocks_wide.saturating_mul(blocks_high).saturating_mul(8) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "BC1 encoder returned a truncated surface",
        ));
    }
    let mut writer = BinaryWriter::from_vec(std::mem::take(blocks), BinaryEndian::Little);
    for block_y in 0..blocks_high {
        for block_x in 0..blocks_wide {
            let mut pixels = [[0u8; 4]; 16];
            let mut has_transparency = false;
            for y in 0..4 {
                for x in 0..4 {
                    let source_x = (block_x * 4 + x).min(image.width() as usize - 1) as u32;
                    let source_y = (block_y * 4 + y).min(image.height() as usize - 1) as u32;
                    pixels[y * 4 + x] = image.get_pixel(source_x, source_y).0;
                    has_transparency |= pixels[y * 4 + x][3] < 128;
                }
            }
            if !has_transparency {
                continue;
            }
            let opaque: Vec<_> = pixels.iter().filter(|pixel| pixel[3] >= 128).collect();
            let (endpoint0, endpoint1) = farthest_bc1_endpoints(&opaque);
            let mut color0 = rgb_to_565(endpoint0);
            let mut color1 = rgb_to_565(endpoint1);
            if color0 > color1 {
                std::mem::swap(&mut color0, &mut color1);
            }
            let palette0 = rgb_from_565(color0);
            let palette1 = rgb_from_565(color1);
            let palette2 = [
                ((u16::from(palette0[0]) + u16::from(palette1[0])) / 2) as u8,
                ((u16::from(palette0[1]) + u16::from(palette1[1])) / 2) as u8,
                ((u16::from(palette0[2]) + u16::from(palette1[2])) / 2) as u8,
            ];
            let palette = [palette0, palette1, palette2];
            let mut indices = 0u32;
            for (index, pixel) in pixels.iter().enumerate() {
                let selected = if pixel[3] < 128 {
                    3
                } else {
                    palette
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, color)| rgb_distance(pixel, color))
                        .map(|(index, _)| index as u32)
                        .unwrap_or(0)
                };
                indices |= selected << (index * 2);
            }
            writer.seek((block_y * blocks_wide + block_x) * 8);
            writer.write_u16(color0);
            writer.write_u16(color1);
            writer.write_u32(indices);
        }
    }
    *blocks = writer.into_inner();
    Ok(())
}

fn farthest_bc1_endpoints(pixels: &[&[u8; 4]]) -> ([u8; 3], [u8; 3]) {
    if pixels.is_empty() {
        return ([0; 3], [0; 3]);
    }
    let mut best = (pixels[0], pixels[0]);
    let mut best_distance = 0u32;
    for left in pixels {
        for right in pixels {
            let distance = rgb_distance(left, &[right[0], right[1], right[2]]);
            if distance > best_distance {
                best = (left, right);
                best_distance = distance;
            }
        }
    }
    (
        [best.0[0], best.0[1], best.0[2]],
        [best.1[0], best.1[1], best.1[2]],
    )
}

fn rgb_distance(pixel: &[u8; 4], color: &[u8; 3]) -> u32 {
    (0..3)
        .map(|channel| {
            let difference = i32::from(pixel[channel]) - i32::from(color[channel]);
            (difference * difference) as u32
        })
        .sum()
}

fn rgb_to_565(color: [u8; 3]) -> u16 {
    (u16::from(color[0] >> 3) << 11) | (u16::from(color[1] >> 2) << 5) | u16::from(color[2] >> 3)
}

fn rgb_from_565(color: u16) -> [u8; 3] {
    let red = ((color >> 11) & 0x1f) as u8;
    let green = ((color >> 5) & 0x3f) as u8;
    let blue = (color & 0x1f) as u8;
    [
        (red << 3) | (red >> 2),
        (green << 2) | (green >> 4),
        (blue << 3) | (blue >> 2),
    ]
}

/// BNTX PNG pixels are already in display-space bytes. Block compression must not
/// apply a second sRGB transfer; the BNTX format field retains the sRGB semantic.
pub fn encoding_format(format: ImageFormat) -> ImageFormat {
    use ImageFormat::*;
    match format {
        BC1RgbaUnormSrgb => BC1RgbaUnorm,
        BC2RgbaUnormSrgb => BC2RgbaUnorm,
        BC3RgbaUnormSrgb => BC3RgbaUnorm,
        BC7RgbaUnormSrgb => BC7RgbaUnorm,
        Rgba8UnormSrgb => Rgba8Unorm,
        Bgra8UnormSrgb => Bgra8Unorm,
        other => other,
    }
}

pub fn format_from_bntx(value: u32) -> io::Result<ImageFormat> {
    let srgb = value & 0xff == 6;
    let snorm = value & 0xff == 2;
    match value >> 8 {
        0x02 => Ok(ImageFormat::R8Unorm),
        0x09 => Ok(ImageFormat::Rg8Unorm),
        0x0b => Ok(if srgb {
            ImageFormat::Rgba8UnormSrgb
        } else {
            ImageFormat::Rgba8Unorm
        }),
        0x0c => Ok(if srgb {
            ImageFormat::Bgra8UnormSrgb
        } else {
            ImageFormat::Bgra8Unorm
        }),
        0x1a => Ok(if srgb {
            ImageFormat::BC1RgbaUnormSrgb
        } else {
            ImageFormat::BC1RgbaUnorm
        }),
        0x1b => Ok(if srgb {
            ImageFormat::BC2RgbaUnormSrgb
        } else {
            ImageFormat::BC2RgbaUnorm
        }),
        0x1c => Ok(if srgb {
            ImageFormat::BC3RgbaUnormSrgb
        } else {
            ImageFormat::BC3RgbaUnorm
        }),
        0x1d => Ok(if snorm {
            ImageFormat::BC4RSnorm
        } else {
            ImageFormat::BC4RUnorm
        }),
        0x1e => Ok(if snorm {
            ImageFormat::BC5RgSnorm
        } else {
            ImageFormat::BC5RgUnorm
        }),
        0x20 => Ok(if srgb {
            ImageFormat::BC7RgbaUnormSrgb
        } else {
            ImageFormat::BC7RgbaUnorm
        }),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("unsupported BNTX format 0x{value:X}"),
        )),
    }
}

pub fn format_for_bntx_name(name: &str) -> io::Result<(ImageFormat, u32)> {
    let format = crate::file_format::Image::dds::image_format(name)?;
    let value = match name {
        "R8_UNORM" => 0x0201,
        "R8_G8_UNORM" => 0x0901,
        "R8_G8_B8_A8_UNORM" => 0x0b01,
        "R8_G8_B8_A8_SRGB" => 0x0b06,
        "B8_G8_R8_A8_UNORM" => 0x0c01,
        "B8_G8_R8_A8_SRGB" => 0x0c06,
        "BC1_UNORM" => 0x1a01,
        "BC1_SRGB" => 0x1a06,
        "BC2_UNORM" => 0x1b01,
        "BC2_SRGB" => 0x1b06,
        "BC3_UNORM" => 0x1c01,
        "BC3_SRGB" => 0x1c06,
        "BC4_UNORM" => 0x1d01,
        "BC4_SNORM" => 0x1d02,
        "BC5_UNORM" => 0x1e01,
        "BC5_SNORM" => 0x1e02,
        "BC6_UFLOAT" => 0x1f05,
        "BC6_FLOAT" => 0x1f04,
        "BC7_UNORM" => 0x2001,
        "BC7_SRGB" => 0x2006,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("{name} cannot be stored in a Switch BNTX texture"),
            ))
        }
    };
    Ok((format, value))
}

pub fn astc_block_from_bntx(value: u32) -> Option<(usize, usize)> {
    match value >> 8 {
        0x2d => Some((4, 4)),
        0x2e => Some((5, 4)),
        0x2f => Some((5, 5)),
        0x30 => Some((6, 5)),
        0x31 => Some((6, 6)),
        0x32 => Some((8, 5)),
        0x33 => Some((8, 6)),
        0x34 => Some((8, 8)),
        0x35 => Some((10, 5)),
        0x36 => Some((10, 6)),
        0x37 => Some((10, 8)),
        0x38 => Some((10, 10)),
        0x39 => Some((12, 10)),
        0x3a => Some((12, 12)),
        _ => None,
    }
}

pub fn format_from_textogo(value: u16) -> io::Result<ImageFormat> {
    match value {
        0x202 | 0x302 => Ok(ImageFormat::BC1RgbaUnorm),
        0x203 => Ok(ImageFormat::BC1RgbaUnormSrgb),
        0x505 => Ok(ImageFormat::BC3RgbaUnormSrgb),
        0x602 | 0x606 | 0x607 => Ok(ImageFormat::BC4RUnorm),
        0x702 | 0x703 | 0x707 => Ok(ImageFormat::BC5RgUnorm),
        0x901 => Ok(ImageFormat::BC7RgbaUnorm),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("unsupported TexToGo format 0x{value:X}"),
        )),
    }
}

pub fn inferred_block_height_log2(height: u32, block_height: u32) -> u8 {
    height
        .div_ceil(block_height)
        .div_ceil(8)
        .next_power_of_two()
        .min(16)
        .trailing_zeros() as u8
}

pub fn apply_component_selectors(image: &mut RgbaImage, selectors: [u8; 4]) {
    for pixel in image.pixels_mut() {
        let source = pixel.0;
        for (target, selector) in selectors.into_iter().enumerate() {
            pixel.0[target] = match selector {
                0 => 0,
                1 => 255,
                2..=5 => source[(selector - 2) as usize],
                _ => source[target],
            };
        }
    }
}

pub fn invert_component_selectors(image: &mut RgbaImage, selectors: [u8; 4]) {
    for pixel in image.pixels_mut() {
        let displayed = pixel.0;
        let mut stored = [0, 0, 0, 255];
        for (display_channel, selector) in selectors.into_iter().enumerate() {
            if let 2..=5 = selector {
                stored[(selector - 2) as usize] = displayed[display_channel];
            }
        }
        pixel.0 = stored;
    }
}

#[cfg(test)]
mod component_selector_tests {
    use super::*;

    #[test]
    fn applies_nintendo_channel_selector_values() {
        let mut image = RgbaImage::from_pixel(1, 1, image::Rgba([10, 20, 30, 40]));
        apply_component_selectors(&mut image, [2, 3, 4, 5]);
        assert_eq!(image.get_pixel(0, 0).0, [10, 20, 30, 40]);
        apply_component_selectors(&mut image, [0, 1, 2, 5]);
        assert_eq!(image.get_pixel(0, 0).0, [0, 255, 10, 40]);
    }

    #[test]
    fn inverts_permuted_component_selectors() {
        let mut image = RgbaImage::from_pixel(1, 1, image::Rgba([10, 20, 30, 40]));
        invert_component_selectors(&mut image, [5, 4, 3, 2]);
        apply_component_selectors(&mut image, [5, 4, 3, 2]);
        assert_eq!(image.get_pixel(0, 0).0, [10, 20, 30, 40]);
    }
}

fn format_layout(format: ImageFormat) -> io::Result<(u32, u32, u32)> {
    use ImageFormat::*;
    match format {
        BC1RgbaUnorm | BC1RgbaUnormSrgb | BC4RUnorm | BC4RSnorm => Ok((4, 4, 8)),
        BC2RgbaUnorm | BC2RgbaUnormSrgb | BC3RgbaUnorm | BC3RgbaUnormSrgb | BC5RgUnorm
        | BC5RgSnorm | BC6hRgbUfloat | BC6hRgbSfloat | BC7RgbaUnorm | BC7RgbaUnormSrgb => {
            Ok((4, 4, 16))
        }
        R8Unorm | R8Snorm => Ok((1, 1, 1)),
        Rg8Unorm | Rg8Snorm => Ok((1, 1, 2)),
        Rgba8Unorm | Rgba8UnormSrgb | Rgba8Snorm | Bgra8Unorm | Bgra8UnormSrgb => Ok((1, 1, 4)),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("unsupported Switch image format {format:?}"),
        )),
    }
}
