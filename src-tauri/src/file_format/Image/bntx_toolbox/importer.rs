//! Port of the Toolbox texture replacement path used by its command line
//! mode: `TextureImporterSettings.FromBitMap` + `SwizzleSurfaceMipMaps`
//! (`TegraX1Swizzle` block-linear tiling through the same `tegra_swizzle`
//! crate the Toolbox DLL is built from). ASTC blocks come from ARM's astcenc
//! exactly like the C# side; BC formats are encoded natively and are the one
//! part that is not byte-identical to Toolbox's DirectXTex output.

use super::model::{
    err, Result, Texture, CHANNEL_ALPHA, CHANNEL_BLUE, CHANNEL_GREEN, CHANNEL_ONE, CHANNEL_RED,
    TILE_MODE_LINEAR_ALIGNED,
};
use crate::file_format::Image::switch_texture;
use image::RgbaImage;
use image_dds::ImageFormat;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Command;
use tegra_swizzle::surface::BlockDim;
use tegra_swizzle::BlockHeight;

/// `Dim.Dim2D`, the importer default.
const DIM_2D: u8 = 2;
/// `TextureImporterSettings.TextureLayout2` default.
const DEFAULT_TEXTURE_LAYOUT2: u32 = 0x010007;
/// `TextureImporterSettings.Alignment` default.
const DEFAULT_ALIGNMENT: u32 = 512;

const ASTC_ENCODER_NAMES: [&str; 4] = [
    "astcenc-avx2.exe",
    "astcenc-sse4.1.exe",
    "astcenc-sse2.exe",
    "astcenc.exe",
];

/// Locates ARM's astcenc: an explicit path, the `ASTCENC` environment
/// variable, the executable directory (plus `bin/` and `bin/cpp/`), then PATH.
pub fn find_astc_encoder(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return path.is_file().then(|| path.to_path_buf());
    }
    if let Ok(value) = std::env::var("ASTCENC") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe_dir) = crate::utils::running_exe_dir() {
        dirs.push(exe_dir.clone());
        dirs.push(exe_dir.join("bin"));
        dirs.push(exe_dir.join("bin/cpp"));
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin/cpp"));
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    for dir in dirs {
        for name in ASTC_ENCODER_NAMES {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// `STGenericTexture.GenerateTotalMipCount`.
pub fn total_mip_count(width: u32, height: u32) -> u32 {
    let mut count = 1;
    let (mut width, mut height) = (width, height);
    while width > 1 || height > 1 {
        count += 1;
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    count
}

fn div_round_up(n: u32, d: u32) -> u32 {
    (n + d - 1) / d
}

/// `TegraX1Swizzle.round_up`, with C# `uint` wrap-around (`round_up(0, y)` is 0).
fn round_up(x: u32, y: u32) -> u32 {
    (x.wrapping_sub(1) | y.wrapping_sub(1)).wrapping_add(1)
}

fn pow2_round_up(x: u32) -> u32 {
    let mut x = x.wrapping_sub(1);
    x |= x >> 1;
    x |= x >> 2;
    x |= x >> 4;
    x |= x >> 8;
    x |= x >> 16;
    x.wrapping_add(1)
}

/// `(block width, block height, bytes per block)` for a BNTX surface format.
fn block_layout(format: u32) -> Result<(u32, u32, u32)> {
    if let Some((width, height)) = switch_texture::astc_block_from_bntx(format) {
        return Ok((width as u32, height as u32, 16));
    }
    let image_format = switch_texture::format_from_bntx(format)?;
    Ok(switch_texture::block_layout(image_format)?)
}

/// `STGenericTexture.SetChannelsByFormat`.
fn channels_by_format(format: u32) -> [u8; 4] {
    match switch_texture::format_from_bntx(format).ok() {
        Some(ImageFormat::BC5RgUnorm | ImageFormat::BC5RgSnorm) => {
            [CHANNEL_RED, CHANNEL_GREEN, CHANNEL_ONE, CHANNEL_ONE]
        }
        Some(ImageFormat::BC4RUnorm | ImageFormat::BC4RSnorm) => {
            [CHANNEL_RED, CHANNEL_RED, CHANNEL_RED, CHANNEL_RED]
        }
        _ => [CHANNEL_RED, CHANNEL_GREEN, CHANNEL_BLUE, CHANNEL_ALPHA],
    }
}

/// Encodes one mip level to ASTC blocks with astcenc, exactly like the C#
/// CLI helper: `astcenc -cs|-cl in.png out.astc WxH -thorough -silent`.
fn encode_astc_level(
    image: &RgbaImage,
    block_width: u32,
    block_height: u32,
    srgb: bool,
    encoder: &Path,
    temp_dir: &Path,
    level: u32,
) -> Result<Vec<u8>> {
    let png = temp_dir.join(format!("mip{level}.png"));
    let astc = temp_dir.join(format!("mip{level}.astc"));
    image
        .save_with_format(&png, image::ImageFormat::Png)
        .map_err(|error| super::model::BntxToolboxError(error.to_string()))?;
    let output = Command::new(encoder)
        .arg(if srgb { "-cs" } else { "-cl" })
        .arg(&png)
        .arg(&astc)
        .arg(format!("{block_width}x{block_height}"))
        .arg("-thorough")
        .arg("-silent")
        .output()
        .map_err(|error| {
            super::model::BntxToolboxError(format!("cannot run {}: {error}", encoder.display()))
        })?;
    if !output.status.success() || !astc.is_file() {
        return err(format!(
            "astcenc failed ({}) for mip {level}: {} {}",
            output.status,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let file = std::fs::read(&astc)?;
    if file.len() < 16 || file[..4] != [0x13, 0xAB, 0xA1, 0x5C] {
        return err(format!(
            "astcenc produced an unexpected file for mip {level}"
        ));
    }
    if file[4] as u32 != block_width || file[5] as u32 != block_height {
        return err(format!(
            "astcenc produced {}x{} blocks instead of {block_width}x{block_height}",
            file[4], file[5]
        ));
    }
    Ok(file[16..].to_vec())
}

/// Encodes a mip chain to linear (untiled) block data, mip levels packed
/// back to back the way `TextureHelper.GetCurrentMipSize` expects.
fn encode_linear_mips(
    image: &RgbaImage,
    format: u32,
    mip_count: u32,
    encoder: Option<&Path>,
) -> Result<Vec<u8>> {
    let is_astc = switch_texture::astc_block_from_bntx(format);
    let temp_dir = std::env::temp_dir().join(format!(
        "totkbits_astc_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    if is_astc.is_some() {
        std::fs::create_dir_all(&temp_dir)?;
    }
    let result = (|| {
        let mut data = Vec::new();
        for level in 0..mip_count {
            let width = (image.width() >> level).max(1);
            let height = (image.height() >> level).max(1);
            let resized;
            let mip: &RgbaImage = if level == 0 {
                image
            } else {
                resized = image::imageops::resize(
                    image,
                    width,
                    height,
                    image::imageops::FilterType::CatmullRom,
                );
                &resized
            };
            let blocks = match is_astc {
                Some((block_width, block_height)) => {
                    let encoder = encoder.ok_or_else(|| {
                        super::model::BntxToolboxError(
                            "no astcenc executable found; pass --astcenc, set ASTCENC or put astcenc-avx2.exe next to the executable".into(),
                        )
                    })?;
                    encode_astc_level(
                        mip,
                        block_width as u32,
                        block_height as u32,
                        format & 0xff == 6,
                        encoder,
                        &temp_dir,
                        level,
                    )?
                }
                None => {
                    let image_format = switch_texture::format_from_bntx(format)?;
                    switch_texture::encode(mip, image_format, 0, true)?
                }
            };
            data.extend_from_slice(&blocks);
        }
        Ok(data)
    })();
    let _ = std::fs::remove_dir_all(&temp_dir);
    result
}

/// `TegraX1Swizzle.swizzle` for one mip level.
fn swizzle_level(
    width: u32,
    height: u32,
    depth: u32,
    block_width: u32,
    block_height: u32,
    block_depth: u32,
    bpp: u32,
    tile_mode: u16,
    block_height_log2: u32,
    data: &[u8],
) -> Result<Vec<u8>> {
    let width_blocks = div_round_up(width, block_width);
    let height_blocks = div_round_up(height, block_height);
    let depth_blocks = div_round_up(depth, block_depth);
    if tile_mode == TILE_MODE_LINEAR_ALIGNED {
        // SwizzlePitchLinear with roundPitch == 1
        let pitch = round_up(width_blocks * bpp, 32);
        let surface_size = pitch * height_blocks;
        let mut result = vec![0u8; surface_size as usize];
        for z in 0..depth_blocks {
            let _ = z;
            for y in 0..height_blocks {
                for x in 0..width_blocks {
                    let pos = (y * pitch + x * bpp) as usize;
                    let pos_ = ((y * width_blocks + x) * bpp) as usize;
                    if pos + bpp as usize <= surface_size as usize {
                        let source = data.get(pos_..pos_ + bpp as usize).ok_or_else(|| {
                            super::model::BntxToolboxError(
                                "linear surface data is too short".into(),
                            )
                        })?;
                        result[pos..pos + bpp as usize].copy_from_slice(source);
                    }
                }
            }
        }
        return Ok(result);
    }

    let block_height_mip0 = 1usize << block_height_log2.min(5);
    let tegra_block_height = BlockHeight::new(block_height_mip0)
        .ok_or_else(|| super::model::BntxToolboxError("invalid Tegra block height".into()))?;
    let mut block_dim = BlockDim::uncompressed();
    block_dim.width = NonZeroUsize::new(block_width as usize).unwrap();
    block_dim.height = NonZeroUsize::new(block_height as usize).unwrap();
    block_dim.depth = NonZeroUsize::new(block_depth as usize).unwrap();
    let surface_size = tegra_swizzle::surface::swizzled_surface_size(
        width as usize,
        height as usize,
        depth as usize,
        block_dim,
        Some(tegra_block_height),
        bpp as usize,
        1,
        1,
    );
    let swizzled = tegra_swizzle::swizzle::swizzle_block_linear(
        width_blocks as usize,
        height_blocks as usize,
        depth_blocks as usize,
        data,
        tegra_block_height,
        bpp as usize,
    )
    .map_err(|error| super::model::BntxToolboxError(error.to_string()))?;
    let mut output = vec![0u8; surface_size];
    let copied = swizzled.len().min(surface_size);
    output[..copied].copy_from_slice(&swizzled[..copied]);
    Ok(output)
}

/// Replaces `texture`'s image the way Toolbox's CLI does: the surface
/// format, flags, access flags, tile mode, surface dimension and sparse bits
/// are kept, everything else is rebuilt from the picture. Returns a warning
/// when the requested mip count had to be reduced.
pub fn replace_texture_from_image(
    texture: &mut Texture,
    image: &RgbaImage,
    requested_mip_count: u32,
    encoder: Option<&Path>,
) -> Result<Option<String>> {
    let format = texture.format;
    let (block_width, block_height, bpp) = block_layout(format)?;
    let block_depth = 1u32;
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return err("the replacement image is empty");
    }

    let mut mip_count = requested_mip_count.max(1);
    let max_mips = total_mip_count(width, height);
    let mut warning = None;
    if mip_count > max_mips {
        warning = Some(format!(
            "mip count {mip_count} -> {max_mips} (image is only {width}x{height})"
        ));
        mip_count = max_mips;
    }
    if mip_count > 1 {
        let note =
            "mip levels above 0 are resampled natively and are not byte-identical to Toolbox";
        warning = Some(match warning {
            Some(text) => format!("{text}; {note}"),
            None => note.to_string(),
        });
    }

    let linear = encode_linear_mips(image, format, mip_count, encoder)?;

    // FromBitMap
    let channels = channels_by_format(format);
    texture.width = width;
    texture.height = height;
    texture.mip_count = mip_count;
    texture.depth = 1;
    texture.dim = DIM_2D;
    texture.texture_layout = 0;
    texture.texture_layout2 = DEFAULT_TEXTURE_LAYOUT2;
    texture.swizzle = 0;
    texture.sample_count = 1;
    texture.channel_red = channels[0];
    texture.channel_green = channels[1];
    texture.channel_blue = channels[2];
    texture.channel_alpha = channels[3];
    texture.mip_offsets = vec![0; mip_count as usize];
    // Replace(): the other array slices of the original texture are kept.
    let mut slices = std::mem::take(&mut texture.texture_data);

    // SwizzleSurfaceMipMaps
    let (block_height_mip0, lines_per_block_height);
    if texture.tile_mode == TILE_MODE_LINEAR_ALIGNED {
        block_height_mip0 = 1u32;
        texture.block_height_log2 = 0;
        texture.alignment = 1;
        lines_per_block_height = 1u32;
        texture.read_texture_layout = 0;
    } else {
        block_height_mip0 =
            tegra_swizzle::block_height_mip0(div_round_up(height, block_height) as usize) as u32;
        texture.block_height_log2 = 31 - block_height_mip0.leading_zeros();
        texture.alignment = DEFAULT_ALIGNMENT as i32;
        texture.read_texture_layout = 1;
        lines_per_block_height = block_height_mip0 * 8;
    }

    let mut block_height_shift = 0u32;
    let mut surface_size = 0u32;
    let mut mipmaps: Vec<Vec<u8>> = Vec::with_capacity(mip_count as usize);
    let mut linear_offset = 0usize;
    for level in 0..mip_count {
        let width_ = (width >> level).max(1);
        let height_ = (height >> level).max(1);
        let depth_ = 1u32;
        let width__ = div_round_up(width_, block_width);
        let height__ = div_round_up(height_, block_height);
        let size = (width__ * height__ * bpp) as usize;
        let data_ = linear
            .get(linear_offset..linear_offset + size)
            .ok_or_else(|| {
                super::model::BntxToolboxError("encoded mip data is too short".into())
            })?;
        linear_offset += size;

        let aligned_len = if texture.alignment == 1 {
            0
        } else {
            round_up(surface_size, texture.alignment as u32).wrapping_sub(surface_size)
        };
        surface_size = surface_size.wrapping_add(aligned_len);
        texture.mip_offsets[level as usize] = surface_size as i64;
        if texture.tile_mode == TILE_MODE_LINEAR_ALIGNED {
            let pitch = round_up(width__ * bpp, 32);
            surface_size = surface_size.wrapping_add(pitch.wrapping_mul(height__));
        } else {
            if pow2_round_up(height__) < lines_per_block_height {
                block_height_shift += 1;
            }
            let pitch = round_up(width__ * bpp, 64);
            let rows = round_up(
                height__,
                (block_height_mip0 >> block_height_shift).max(1) * 8,
            );
            surface_size = surface_size.wrapping_add(pitch.wrapping_mul(rows));
        }

        let swizzled = swizzle_level(
            width_,
            height_,
            depth_,
            block_width,
            block_height,
            block_depth,
            bpp,
            texture.tile_mode,
            texture.block_height_log2.saturating_sub(block_height_shift),
            data_,
        )?;
        let mut mip = vec![0u8; aligned_len as usize];
        mip.extend_from_slice(&swizzled);
        mipmaps.push(mip);
    }
    texture.image_size = surface_size;

    // settings.Alignment (512) != 1, so the slice holds the combined mips.
    let combined: Vec<u8> = mipmaps.concat();
    if slices.is_empty() {
        slices.push(combined);
    } else {
        slices[0] = combined;
    }
    texture.texture_data = slices;
    Ok(warning)
}
