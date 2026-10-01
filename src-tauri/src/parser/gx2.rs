//! Wii U GX2 surface layout: the R600 address library that sizes, tiles
//! ("swizzles") and untiles texture surfaces.
//!
//! A port of the C# address library in Switch Toolbox's `GX2.cs`, itself a
//! port of AboodXD's GTX Extractor `addrlib`. Arithmetic is kept in the
//! reference's unsigned 32-bit wrap-around form on purpose: the surface
//! built by [`create_surface`] has to come out byte-identical to Toolbox's
//! `GX2.CreateGx2Texture`.
use std::io;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[rustfmt::skip]
const FORMAT_HW_INFO: [u8; 256] = [
    0x00, 0x00, 0x00, 0x01, 0x08, 0x03, 0x00, 0x01, 0x08, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0x01, 0x10, 0x07, 0x00, 0x00, 0x10, 0x03, 0x00, 0x01, 0x10, 0x03, 0x00, 0x01,
    0x10, 0x0B, 0x00, 0x01, 0x10, 0x01, 0x00, 0x01, 0x10, 0x03, 0x00, 0x01, 0x10, 0x03, 0x00, 0x01,
    0x10, 0x03, 0x00, 0x01, 0x20, 0x03, 0x00, 0x00, 0x20, 0x07, 0x00, 0x00, 0x20, 0x03, 0x00, 0x00,
    0x20, 0x03, 0x00, 0x01, 0x20, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x03, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x20, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0x01, 0x20, 0x0B, 0x00, 0x01, 0x20, 0x0B, 0x00, 0x01, 0x20, 0x0B, 0x00, 0x01,
    0x40, 0x05, 0x00, 0x00, 0x40, 0x03, 0x00, 0x00, 0x40, 0x03, 0x00, 0x00, 0x40, 0x03, 0x00, 0x00,
    0x40, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x80, 0x03, 0x00, 0x00, 0x80, 0x03, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x10, 0x01, 0x00, 0x00,
    0x10, 0x01, 0x00, 0x00, 0x20, 0x01, 0x00, 0x00, 0x20, 0x01, 0x00, 0x00, 0x20, 0x01, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x60, 0x01, 0x00, 0x00,
    0x60, 0x01, 0x00, 0x00, 0x40, 0x01, 0x00, 0x01, 0x80, 0x01, 0x00, 0x01, 0x80, 0x01, 0x00, 0x01,
    0x40, 0x01, 0x00, 0x01, 0x80, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[rustfmt::skip]
const FORMAT_EX_INFO: [u8; 256] = [
    0x00, 0x01, 0x01, 0x03, 0x08, 0x01, 0x01, 0x03, 0x08, 0x01, 0x01, 0x03, 0x08, 0x01, 0x01, 0x03,
    0x00, 0x01, 0x01, 0x03, 0x10, 0x01, 0x01, 0x03, 0x10, 0x01, 0x01, 0x03, 0x10, 0x01, 0x01, 0x03,
    0x10, 0x01, 0x01, 0x03, 0x10, 0x01, 0x01, 0x03, 0x10, 0x01, 0x01, 0x03, 0x10, 0x01, 0x01, 0x03,
    0x10, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03,
    0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03,
    0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03,
    0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03,
    0x40, 0x01, 0x01, 0x03, 0x40, 0x01, 0x01, 0x03, 0x40, 0x01, 0x01, 0x03, 0x40, 0x01, 0x01, 0x03,
    0x40, 0x01, 0x01, 0x03, 0x00, 0x01, 0x01, 0x03, 0x80, 0x01, 0x01, 0x03, 0x80, 0x01, 0x01, 0x03,
    0x00, 0x01, 0x01, 0x03, 0x01, 0x08, 0x01, 0x05, 0x01, 0x08, 0x01, 0x06, 0x10, 0x01, 0x01, 0x07,
    0x10, 0x01, 0x01, 0x08, 0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03, 0x20, 0x01, 0x01, 0x03,
    0x18, 0x03, 0x01, 0x04, 0x30, 0x03, 0x01, 0x04, 0x30, 0x03, 0x01, 0x04, 0x60, 0x03, 0x01, 0x04,
    0x60, 0x03, 0x01, 0x04, 0x40, 0x04, 0x04, 0x09, 0x80, 0x04, 0x04, 0x0A, 0x80, 0x04, 0x04, 0x0B,
    0x40, 0x04, 0x04, 0x0C, 0x40, 0x04, 0x04, 0x0D, 0x40, 0x04, 0x04, 0x0D, 0x40, 0x04, 0x04, 0x0D,
    0x00, 0x01, 0x01, 0x03, 0x00, 0x01, 0x01, 0x03, 0x00, 0x01, 0x01, 0x03, 0x00, 0x01, 0x01, 0x03,
    0x00, 0x01, 0x01, 0x03, 0x00, 0x01, 0x01, 0x03, 0x40, 0x01, 0x01, 0x03, 0x00, 0x01, 0x01, 0x03,
];

/// `GX2.surfaceGetBitsPerPixel`: bits per element (per 4x4 block for BCn).
pub fn bits_per_pixel(format: u32) -> u32 {
    u32::from(FORMAT_HW_INFO[((format & 0x3f) * 4) as usize])
}

/// `GX2.IsFormatBCN`.
pub fn is_bcn(format: u32) -> bool {
    matches!(
        format,
        0x31 | 0x431 | 0x32 | 0x432 | 0x33 | 0x433 | 0x34 | 0x234 | 0x35 | 0x235
    )
}

/// The `GX2SurfaceFormat` enumerator name Toolbox prints.
pub fn format_name(format: u32) -> String {
    let name = match format {
        0x001 => "TC_R8_UNORM",
        0x101 => "TC_R8_UINT",
        0x201 => "TC_R8_SNORM",
        0x301 => "TC_R8_SINT",
        0x002 => "T_R4_G4_UNORM",
        0x005 => "TCD_R16_UNORM",
        0x007 => "TC_R8_G8_UNORM",
        0x207 => "TC_R8_G8_SNORM",
        0x008 => "TCS_R5_G6_B5_UNORM",
        0x00a => "TC_R5_G5_B5_A1_UNORM",
        0x00b => "TC_R4_G4_B4_A4_UNORM",
        0x00c => "TC_A1_B5_G5_R5_UNORM",
        0x80e => "TCD_R32_FLOAT",
        0x816 => "TC_R11_G11_B10_FLOAT",
        0x019 => "TCS_R10_G10_B10_A2_UNORM",
        0x01a => "TCS_R8_G8_B8_A8_UNORM",
        0x41a => "TCS_R8_G8_B8_A8_SRGB",
        0x820 => "TC_R16_G16_B16_A16_FLOAT",
        0x823 => "TC_R32_G32_B32_A32_FLOAT",
        0x031 => "T_BC1_UNORM",
        0x431 => "T_BC1_SRGB",
        0x032 => "T_BC2_UNORM",
        0x432 => "T_BC2_SRGB",
        0x033 => "T_BC3_UNORM",
        0x433 => "T_BC3_SRGB",
        0x034 => "T_BC4_UNORM",
        0x234 => "T_BC4_SNORM",
        0x035 => "T_BC5_UNORM",
        0x235 => "T_BC5_SNORM",
        other => return format!("0x{other:X}"),
    };
    name.to_string()
}

/// `GX2.surfaceOut`: the layout of one mip level of a surface.
#[derive(Debug, Clone, Copy, Default)]
pub struct SurfaceInfo {
    pub pitch: u32,
    pub height: u32,
    pub depth: u32,
    pub surf_size: u32,
    pub tile_mode: u32,
    pub base_align: u32,
    pub pitch_align: u32,
    pub height_align: u32,
    pub depth_align: u32,
    pub bpp: u32,
    pub pixel_pitch: u32,
    pub pixel_height: u32,
    pub pixel_bits: u32,
    pub slice_size: u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct SurfaceIn {
    tile_mode: u32,
    format: u32,
    bpp: u32,
    num_samples: u32,
    width: u32,
    height: u32,
    num_slices: u32,
    slice: u32,
    mip_level: u32,
    flags: u32,
}

fn next_pow2(dim: u32) -> u32 {
    let mut new_dim = 1u32;
    if dim < 0x7fff_ffff {
        while new_dim < dim {
            new_dim *= 2;
        }
    } else {
        new_dim = 0x8000_0000;
    }
    new_dim
}

fn pow_two_align(x: u32, align: u32) -> u32 {
    !(align.wrapping_sub(1)) & x.wrapping_add(align).wrapping_sub(1)
}

fn surface_thickness(tile_mode: u32) -> u32 {
    match tile_mode {
        3 | 7 | 11 | 13 | 15 => 4,
        16 | 17 => 8,
        _ => 1,
    }
}

fn is_thick_macro_tiled(tile_mode: u32) -> bool {
    matches!(tile_mode, 7 | 11 | 13 | 15)
}

fn is_bank_swapped(tile_mode: u32) -> bool {
    matches!(tile_mode, 8 | 9 | 10 | 11 | 14 | 15)
}

fn macro_tile_aspect_ratio(tile_mode: u32) -> u32 {
    match tile_mode {
        5 | 9 => 2,
        6 | 10 => 4,
        _ => 1,
    }
}

fn surface_rotation(tile_mode: u32) -> u32 {
    match tile_mode {
        4..=11 => 2,
        12..=15 => 1,
        _ => 0,
    }
}

fn to_non_bank_swapped(tile_mode: u32) -> u32 {
    match tile_mode {
        8 => 4,
        9 => 5,
        10 => 6,
        11 => 7,
        14 => 12,
        15 => 13,
        other => other,
    }
}

fn bank_swapped_width(tile_mode: u32, bpp: u32, num_samples: u32, pitch: u32) -> u32 {
    if !is_bank_swapped(tile_mode) {
        return 0;
    }
    let mut num_samples = num_samples;
    let bytes_per_sample = 8 * bpp;
    let slices_per_tile = if bytes_per_sample != 0 {
        let samples_per_tile = 2048 / bytes_per_sample;
        (num_samples / samples_per_tile).max(1)
    } else {
        1
    };
    if is_thick_macro_tiled(tile_mode) {
        num_samples = 4;
    }
    let bytes_per_tile_slice = num_samples * bytes_per_sample / slices_per_tile;
    let factor = macro_tile_aspect_ratio(tile_mode);
    let swap_tiles = (128 / bpp).max(1);
    let swap_width = swap_tiles * 32;
    let height_bytes = num_samples * factor * bpp * 2 / slices_per_tile;
    let swap_max = 0x4000 / height_bytes;
    let swap_min = 256 / bytes_per_tile_slice;
    let mut bank_swap_width = swap_max.min(swap_min.max(swap_width));
    while bank_swap_width >= 2 * pitch {
        bank_swap_width >>= 1;
    }
    bank_swap_width
}

fn pixel_index_within_micro_tile(
    x: u32,
    y: u32,
    z: u32,
    bpp: u32,
    tile_mode: u32,
    is_depth: bool,
) -> u32 {
    let thickness = surface_thickness(tile_mode);
    let (b0, b1, b2, b3, b4, b5);
    if is_depth {
        b0 = x & 1;
        b1 = y & 1;
        b2 = (x & 2) >> 1;
        b3 = (y & 2) >> 1;
        b4 = (x & 4) >> 2;
        b5 = (y & 4) >> 2;
    } else {
        match bpp {
            8 => {
                b0 = x & 1;
                b1 = (x & 2) >> 1;
                b2 = (x & 4) >> 2;
                b3 = (y & 2) >> 1;
                b4 = y & 1;
                b5 = (y & 4) >> 2;
            }
            0x10 => {
                b0 = x & 1;
                b1 = (x & 2) >> 1;
                b2 = (x & 4) >> 2;
                b3 = y & 1;
                b4 = (y & 2) >> 1;
                b5 = (y & 4) >> 2;
            }
            0x40 => {
                b0 = x & 1;
                b1 = y & 1;
                b2 = (x & 2) >> 1;
                b3 = (x & 4) >> 2;
                b4 = (y & 2) >> 1;
                b5 = (y & 4) >> 2;
            }
            0x80 => {
                b0 = y & 1;
                b1 = x & 1;
                b2 = (x & 2) >> 1;
                b3 = (x & 4) >> 2;
                b4 = (y & 2) >> 1;
                b5 = (y & 4) >> 2;
            }
            // 0x20, 0x60 and everything else
            _ => {
                b0 = x & 1;
                b1 = (x & 2) >> 1;
                b2 = y & 1;
                b3 = (x & 4) >> 2;
                b4 = (y & 2) >> 1;
                b5 = (y & 4) >> 2;
            }
        }
    }
    let (mut b6, mut b7, mut b8) = (0, 0, 0);
    if thickness > 1 {
        b6 = z & 1;
        b7 = (z & 2) >> 1;
    }
    if thickness == 8 {
        b8 = (z & 4) >> 2;
    }
    (b8 << 8) | (b7 << 7) | (b6 << 6) | 32 * b5 | 16 * b4 | 8 * b3 | 4 * b2 | b0 | 2 * b1
}

fn addr_from_coord_linear(
    x: u32,
    y: u32,
    slice: u32,
    sample: u32,
    bytes_per_pixel: u32,
    pitch: u32,
    height: u32,
    num_slices: u32,
) -> u64 {
    let slice_offset = pitch
        .wrapping_mul(height)
        .wrapping_mul(slice.wrapping_add(sample.wrapping_mul(num_slices)));
    u64::from(
        y.wrapping_mul(pitch)
            .wrapping_add(x)
            .wrapping_add(slice_offset)
            .wrapping_mul(bytes_per_pixel),
    )
}

fn addr_from_coord_micro_tiled(
    x: u32,
    y: u32,
    slice: u32,
    bpp: u32,
    pitch: u32,
    height: u32,
    tile_mode: u32,
    is_depth: bool,
) -> u64 {
    let micro_tile_thickness: u64 = if tile_mode == 3 { 4 } else { 1 };
    let micro_tile_bytes = ((64 * micro_tile_thickness * u64::from(bpp) + 7) as u32) / 8;
    let micro_tiles_per_row = pitch >> 3;
    let micro_tile_index_x = x >> 3;
    let micro_tile_index_y = y >> 3;
    let micro_tile_index_z = slice / micro_tile_thickness as u32;

    let micro_tile_offset = u64::from(micro_tile_bytes.wrapping_mul(
        micro_tile_index_x.wrapping_add(micro_tile_index_y.wrapping_mul(micro_tiles_per_row)),
    ));
    let slice_bytes =
        (u64::from(pitch.wrapping_mul(height)) * micro_tile_thickness * u64::from(bpp) + 7) / 8;
    let slice_offset = u64::from(micro_tile_index_z) * slice_bytes;

    let pixel_index = pixel_index_within_micro_tile(x, y, slice, bpp, tile_mode, is_depth);
    let pixel_offset = u64::from(bpp.wrapping_mul(pixel_index) >> 3);
    pixel_offset + micro_tile_offset + slice_offset
}

const BANK_SWAP_ORDER: [u32; 10] = [0, 1, 3, 2, 6, 7, 5, 4, 0, 0];

#[allow(clippy::too_many_arguments)]
fn addr_from_coord_macro_tiled(
    x: u32,
    y: u32,
    slice: u32,
    sample: u32,
    bpp: u32,
    pitch: u32,
    height: u32,
    num_samples: u32,
    tile_mode: u32,
    is_depth: bool,
    pipe_swizzle: u32,
    bank_swizzle: u32,
) -> u64 {
    let mut num_samples = num_samples;
    let micro_tile_thickness = surface_thickness(tile_mode);
    let micro_tile_bits = num_samples
        .wrapping_mul(bpp)
        .wrapping_mul(micro_tile_thickness * 64);
    let micro_tile_bytes = micro_tile_bits.wrapping_add(7) / 8;

    let pixel_index = pixel_index_within_micro_tile(x, y, slice, bpp, tile_mode, is_depth);
    let bytes_per_sample = micro_tile_bytes / num_samples;
    let (sample_offset, pixel_offset) = if is_depth {
        (
            bpp.wrapping_mul(sample),
            num_samples.wrapping_mul(bpp).wrapping_mul(pixel_index),
        )
    } else {
        (
            sample.wrapping_mul(micro_tile_bits / num_samples),
            bpp.wrapping_mul(pixel_index),
        )
    };
    let mut elem_offset = pixel_offset.wrapping_add(sample_offset);

    let (num_sample_splits, sample_slice);
    if num_samples <= 1 || micro_tile_bytes <= 2048 {
        num_sample_splits = 1;
        sample_slice = 0;
    } else {
        let samples_per_slice = 2048 / bytes_per_sample;
        num_sample_splits = num_samples / samples_per_slice;
        num_samples = samples_per_slice;
        let tile_slice_bits = micro_tile_bits / num_sample_splits;
        sample_slice = elem_offset / tile_slice_bits;
        elem_offset %= tile_slice_bits;
    }
    elem_offset = elem_offset.wrapping_add(7) / 8;

    let mut pipe = ((y >> 3) ^ (x >> 3)) & 1;
    let mut bank = (((y >> 5) ^ (x >> 3)) & 1) | (2 * (((y >> 4) ^ (x >> 4)) & 1));

    let swizzle = pipe_swizzle + 2 * bank_swizzle;
    let mut bank_pipe = pipe + 2 * bank;
    let rotation = surface_rotation(tile_mode);
    let mut slice_in = slice;
    if is_thick_macro_tiled(tile_mode) {
        slice_in >>= 2;
    }
    bank_pipe ^= (2 * sample_slice * 3) ^ swizzle.wrapping_add(slice_in.wrapping_mul(rotation));
    bank_pipe %= 8;
    pipe = bank_pipe % 2;
    bank = bank_pipe / 2;

    let slice_bytes = height
        .wrapping_mul(pitch)
        .wrapping_mul(micro_tile_thickness)
        .wrapping_mul(bpp)
        .wrapping_mul(num_samples)
        .wrapping_add(7)
        / 8;
    let slice_offset = slice_bytes
        .wrapping_mul(sample_slice.wrapping_add(num_sample_splits.wrapping_mul(slice)))
        / micro_tile_thickness;

    let (macro_tile_pitch, macro_tile_height) = match tile_mode {
        5 | 9 => (16, 32),
        6 | 10 => (8, 64),
        _ => (32, 16),
    };
    let macro_tiles_per_row = pitch / macro_tile_pitch;
    let macro_tile_bytes = num_samples
        .wrapping_mul(micro_tile_thickness)
        .wrapping_mul(bpp)
        .wrapping_mul(macro_tile_height)
        .wrapping_mul(macro_tile_pitch)
        .wrapping_add(7)
        / 8;
    let macro_tile_index_x = x / macro_tile_pitch;
    let macro_tile_index_y = y / macro_tile_height;
    let macro_tile_offset = u64::from(
        macro_tile_index_x
            .wrapping_add(macro_tiles_per_row.wrapping_mul(macro_tile_index_y))
            .wrapping_mul(macro_tile_bytes),
    );

    if is_bank_swapped(tile_mode) {
        let bank_swap_width = bank_swapped_width(tile_mode, bpp, 1, pitch);
        let swap_index = macro_tile_pitch * macro_tile_index_x / bank_swap_width;
        bank ^= BANK_SWAP_ORDER[(swap_index & 3) as usize];
    }

    let total_offset =
        u64::from(elem_offset) + ((macro_tile_offset + u64::from(slice_offset)) >> 3);
    u64::from(bank << 9)
        | u64::from(pipe << 8)
        | (total_offset & 255)
        | ((total_offset & !255) << 3)
}

/// `GX2.swizzleSurf`: moves one slice of one mip level between linear and
/// tiled order. `width`/`height` are in texels, `pitch`/`bits_per_pixel`/
/// `tile_mode`/`depth` come from the level's [`SurfaceInfo`].
#[allow(clippy::too_many_arguments)]
fn swizzle_surface(
    width: u32,
    height: u32,
    depth: u32,
    format: u32,
    aa: u32,
    use_: u32,
    tile_mode: u32,
    swizzle: u32,
    pitch: u32,
    bits_per_pixel: u32,
    slice: u32,
    sample: u32,
    data: &[u8],
    to_tiled: bool,
) -> io::Result<Vec<u8>> {
    let bytes_per_pixel = (bits_per_pixel / 8) as usize;
    let mut result = vec![0u8; data.len()];
    let (width, height) = if is_bcn(format) {
        ((width + 3) / 4, (height + 3) / 4)
    } else {
        (width, height)
    };
    let pipe_swizzle = (swizzle >> 8) & 1;
    let bank_swizzle = (swizzle >> 9) & 3;
    // GX2TileModeToAddrTileMode
    let tile_mode = match tile_mode {
        0 => return Err(invalid("GX2 surface has no resolved tile mode")),
        16 => 0,
        other => other,
    };
    if tile_mode > 15 {
        return Err(invalid(format!("unsupported GX2 tile mode {tile_mode}")));
    }
    let is_depth = use_ & 4 != 0;
    let num_samples = 1u32 << aa.min(3);

    for y in 0..height {
        for x in 0..width {
            let pos = match tile_mode {
                0 | 1 => addr_from_coord_linear(
                    x,
                    y,
                    slice,
                    sample,
                    bytes_per_pixel as u32,
                    pitch,
                    height,
                    depth,
                ),
                2 | 3 => addr_from_coord_micro_tiled(
                    x,
                    y,
                    slice,
                    bits_per_pixel,
                    pitch,
                    height,
                    tile_mode,
                    is_depth,
                ),
                _ => addr_from_coord_macro_tiled(
                    x,
                    y,
                    slice,
                    sample,
                    bits_per_pixel,
                    pitch,
                    height,
                    num_samples,
                    tile_mode,
                    is_depth,
                    pipe_swizzle,
                    bank_swizzle,
                ),
            };
            let linear = (y.wrapping_mul(width).wrapping_add(x) as usize) * bytes_per_pixel;
            let Ok(tiled) = usize::try_from(pos) else {
                continue;
            };
            if linear + bytes_per_pixel <= data.len() && tiled + bytes_per_pixel <= data.len() {
                if to_tiled {
                    result[tiled..tiled + bytes_per_pixel]
                        .copy_from_slice(&data[linear..linear + bytes_per_pixel]);
                } else {
                    result[linear..linear + bytes_per_pixel]
                        .copy_from_slice(&data[tiled..tiled + bytes_per_pixel]);
                }
            }
        }
    }
    Ok(result)
}

// ---- surface info -----------------------------------------------------------

fn surface_tile_slices(tile_mode: u32, bpp: u32, num_samples: u32) -> u32 {
    let mut num_samples = num_samples;
    let byte_per_sample = ((bpp << 6) + 7) >> 3;
    let mut tile_slices = 1;
    if surface_thickness(tile_mode) > 1 {
        num_samples = 4;
    }
    if byte_per_sample != 0 {
        let sample_per_tile = 2048 / byte_per_sample;
        if sample_per_tile < num_samples {
            tile_slices = (num_samples / sample_per_tile).max(1);
        }
    }
    tile_slices
}

#[allow(clippy::too_many_arguments)]
fn mip_level_tile_mode(
    base_tile_mode: u32,
    bpp: u32,
    level: u32,
    width: u32,
    height: u32,
    num_slices: u32,
    num_samples: u32,
    is_depth: u32,
    no_recursive: bool,
) -> u32 {
    let mut bpp = bpp;
    let mut width_align_factor = 1;
    let mut macro_tile_width = 32;
    let mut macro_tile_height = 16;
    // The reference computes the thick -> thin demotion for multisampled
    // surfaces and then overwrites it in the final `else`; only the 1D
    // cases survive.
    let _ = surface_tile_slices(base_tile_mode, bpp, num_samples);
    let mut exp_tile_mode = if base_tile_mode == 2 && num_samples > 1 {
        4
    } else if base_tile_mode == 3 {
        let mut mode = base_tile_mode;
        if num_samples > 1 || is_depth != 0 {
            mode = 2;
        }
        if num_samples == 2 || num_samples == 4 {
            mode = 7;
        }
        mode
    } else {
        base_tile_mode
    };
    if no_recursive || level == 0 {
        return exp_tile_mode;
    }
    if matches!(bpp, 24 | 48 | 96) {
        bpp /= 3;
    }
    let widtha = next_pow2(width);
    let heighta = next_pow2(height);
    let num_slicesa = next_pow2(num_slices);

    exp_tile_mode = to_non_bank_swapped(exp_tile_mode);
    let thickness = surface_thickness(exp_tile_mode);
    let micro_tile_bytes = (num_samples * bpp * (thickness << 6) + 7) >> 3;
    if micro_tile_bytes < 256 {
        width_align_factor = (256 / micro_tile_bytes).max(1);
    }
    if exp_tile_mode == 4 || exp_tile_mode == 12 {
        if widtha < width_align_factor * macro_tile_width || heighta < macro_tile_height {
            exp_tile_mode = 2;
        }
    } else if exp_tile_mode == 5 {
        macro_tile_width = 16;
        macro_tile_height = 32;
        if widtha < width_align_factor * macro_tile_width || heighta < macro_tile_height {
            exp_tile_mode = 2;
        }
    } else if exp_tile_mode == 6 {
        macro_tile_width = 8;
        macro_tile_height = 64;
        if widtha < width_align_factor * macro_tile_width || heighta < macro_tile_height {
            exp_tile_mode = 2;
        }
    } else if (exp_tile_mode == 7 || exp_tile_mode == 13)
        && (widtha < width_align_factor * macro_tile_width || heighta < macro_tile_height)
    {
        exp_tile_mode = 3;
    }
    if num_slicesa < 4 {
        exp_tile_mode = match exp_tile_mode {
            3 => 2,
            7 => 4,
            13 => 12,
            other => other,
        };
    }
    mip_level_tile_mode(
        exp_tile_mode,
        bpp,
        level,
        widtha,
        heighta,
        num_slicesa,
        num_samples,
        is_depth,
        true,
    )
}

fn adjust_pitch_alignment(flags: u32, pitch_align: u32) -> u32 {
    if (flags >> 13) & 1 != 0 {
        pow_two_align(pitch_align, 0x20)
    } else {
        pitch_align
    }
}

/// `(baseAlign, pitchAlign, heightAlign)`
fn alignments_linear(tile_mode: u32, bpp: u32, flags: u32) -> (u32, u32, u32) {
    let (base_align, pitch_align, height_align) = match tile_mode {
        0 => (1, if bpp != 1 { 1 } else { 8 }, 1),
        1 => (256, (2048 / bpp).max(0x40), 1),
        _ => (1, 1, 1),
    };
    (
        base_align,
        adjust_pitch_alignment(flags, pitch_align),
        height_align,
    )
}

fn alignments_micro_tiled(
    tile_mode: u32,
    bpp: u32,
    flags: u32,
    num_samples: u32,
) -> (u32, u32, u32) {
    let bpp = if matches!(bpp, 24 | 48 | 96) {
        bpp / 3
    } else {
        bpp
    };
    let thickness = surface_thickness(tile_mode);
    let pitch_align = (256 / bpp / num_samples / thickness).max(8);
    (256, adjust_pitch_alignment(flags, pitch_align), 8)
}

/// `(baseAlign, pitchAlign, heightAlign)`
fn alignments_macro_tiled(
    tile_mode: u32,
    bpp: u32,
    flags: u32,
    num_samples: u32,
) -> (u32, u32, u32) {
    let aspect_ratio = macro_tile_aspect_ratio(tile_mode);
    let thickness = surface_thickness(tile_mode);
    let bpp = match bpp {
        24 | 48 | 96 => bpp / 3,
        3 => 1,
        other => other,
    };
    let macro_tile_width = 32 / aspect_ratio;
    let macro_tile_height = aspect_ratio * 16;
    let pitch_align =
        macro_tile_width.max(macro_tile_width * (256 / bpp / (8 * thickness) / num_samples));
    let pitch_align = adjust_pitch_alignment(flags, pitch_align);
    let height_align = macro_tile_height;
    let macro_tile_bytes = num_samples * ((bpp * macro_tile_height * macro_tile_width + 7) >> 3);
    let mut base_align = if thickness == 1 {
        macro_tile_bytes.max((num_samples * height_align * bpp * pitch_align + 7) >> 3)
    } else {
        256u32.max((4 * height_align * bpp * pitch_align + 7) >> 3)
    };
    let micro_tile_bytes = (thickness * num_samples * (bpp << 6) + 7) >> 3;
    let num_slices_per_micro_tile = if micro_tile_bytes < 2048 {
        1
    } else {
        micro_tile_bytes / 2048
    };
    base_align /= num_slices_per_micro_tile;
    (base_align, pitch_align, height_align)
}

/// The working state of one `computeSurfaceInfo` call (`pIn`, `pOut` and the
/// `expPitch`/`expHeight`/`expNumSlices` statics of the reference).
struct Calc {
    input: SurfaceIn,
    out: SurfaceInfo,
    exp_pitch: u32,
    exp_height: u32,
    exp_num_slices: u32,
}

/// `{pitch, height, numSlices, surfSize, tileMode, baseAlign, pitchAlign, heightAlign, depthAlign}`
struct Layout {
    pitch: u32,
    height: u32,
    num_slices: u32,
    surf_size: u32,
    tile_mode: u32,
    base_align: u32,
    pitch_align: u32,
    height_align: u32,
    depth_align: u32,
}

impl Calc {
    fn pad_dimensions(
        &mut self,
        tile_mode: u32,
        pad_dims: u32,
        is_cube: u32,
        pitch_align: u32,
        height_align: u32,
        slice_align: u32,
    ) {
        let thickness = surface_thickness(tile_mode);
        let pad_dims = if pad_dims == 0 { 3 } else { pad_dims };
        if pitch_align & pitch_align.wrapping_sub(1) == 0 {
            self.exp_pitch = pow_two_align(self.exp_pitch, pitch_align);
        } else {
            self.exp_pitch = self.exp_pitch.wrapping_add(pitch_align - 1);
            self.exp_pitch /= pitch_align;
            self.exp_pitch = self.exp_pitch.wrapping_mul(pitch_align);
        }
        if pad_dims > 1 {
            self.exp_height = pow_two_align(self.exp_height, height_align);
        }
        if pad_dims > 2 || thickness > 1 {
            if is_cube != 0 {
                self.exp_num_slices = next_pow2(self.exp_num_slices);
            }
            if thickness > 1 {
                self.exp_num_slices = pow_two_align(self.exp_num_slices, slice_align);
            }
        }
    }

    fn surf_size(&self, slices: u32, bpp: u32, num_samples: u32) -> u32 {
        self.exp_height
            .wrapping_mul(self.exp_pitch)
            .wrapping_mul(slices)
            .wrapping_mul(bpp)
            .wrapping_mul(num_samples)
            .wrapping_add(7)
            / 8
    }

    #[allow(clippy::too_many_arguments)]
    fn info_linear(
        &mut self,
        tile_mode: u32,
        bpp: u32,
        num_samples: u32,
        pitch: u32,
        height: u32,
        num_slices: u32,
        mip_level: u32,
        pad_dims: u32,
        flags: u32,
    ) -> Layout {
        let mut pad_dims = pad_dims;
        self.exp_pitch = pitch;
        self.exp_height = height;
        self.exp_num_slices = num_slices;
        let micro_tile_thickness = surface_thickness(tile_mode);
        let (base_align, pitch_align, height_align) = alignments_linear(tile_mode, bpp, flags);

        if (flags >> 9) & 1 != 0 && mip_level == 0 {
            self.exp_pitch /= 3;
            self.exp_pitch = next_pow2(self.exp_pitch);
        }
        if mip_level != 0 {
            self.exp_pitch = next_pow2(self.exp_pitch);
            self.exp_height = next_pow2(self.exp_height);
            if (flags >> 4) & 1 != 0 {
                self.exp_num_slices = num_slices;
                pad_dims = if num_slices <= 1 { 2 } else { 0 };
            } else {
                self.exp_num_slices = next_pow2(num_slices);
            }
        }
        self.pad_dimensions(
            tile_mode,
            pad_dims,
            (flags >> 4) & 1,
            pitch_align,
            height_align,
            micro_tile_thickness,
        );
        if (flags >> 9) & 1 != 0 && mip_level == 0 {
            self.exp_pitch = self.exp_pitch.wrapping_mul(3);
        }
        let slices = self.exp_num_slices * num_samples / micro_tile_thickness;
        Layout {
            pitch: self.exp_pitch,
            height: self.exp_height,
            num_slices: self.exp_num_slices,
            surf_size: self.surf_size(slices, bpp, num_samples),
            tile_mode,
            base_align,
            pitch_align,
            height_align,
            depth_align: micro_tile_thickness,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn info_micro_tiled(
        &mut self,
        tile_mode: u32,
        bpp: u32,
        num_samples: u32,
        pitch: u32,
        height: u32,
        num_slices: u32,
        mip_level: u32,
        pad_dims: u32,
        flags: u32,
    ) -> Layout {
        let mut pad_dims = pad_dims;
        self.exp_pitch = pitch;
        self.exp_height = height;
        self.exp_num_slices = num_slices;
        let mut exp_tile_mode = tile_mode;
        let mut micro_tile_thickness = surface_thickness(tile_mode);

        if mip_level != 0 {
            self.exp_pitch = next_pow2(pitch);
            self.exp_height = next_pow2(height);
            if (flags >> 4) & 1 != 0 {
                self.exp_num_slices = num_slices;
                pad_dims = if num_slices <= 1 { 2 } else { 0 };
            } else {
                self.exp_num_slices = next_pow2(num_slices);
            }
            if exp_tile_mode == 3 && self.exp_num_slices < 4 {
                exp_tile_mode = 2;
                micro_tile_thickness = 1;
            }
        }
        let (base_align, pitch_align, height_align) =
            alignments_micro_tiled(exp_tile_mode, bpp, flags, num_samples);
        self.pad_dimensions(
            exp_tile_mode,
            pad_dims,
            (flags >> 4) & 1,
            pitch_align,
            height_align,
            micro_tile_thickness,
        );
        Layout {
            pitch: self.exp_pitch,
            height: self.exp_height,
            num_slices: self.exp_num_slices,
            surf_size: self.surf_size(self.exp_num_slices, bpp, num_samples),
            tile_mode: exp_tile_mode,
            base_align,
            pitch_align,
            height_align,
            depth_align: micro_tile_thickness,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn info_macro_tiled(
        &mut self,
        tile_mode: u32,
        base_tile_mode: u32,
        bpp: u32,
        num_samples: u32,
        pitch: u32,
        height: u32,
        num_slices: u32,
        mip_level: u32,
        pad_dims: u32,
        flags: u32,
    ) -> Layout {
        let mut pad_dims = pad_dims;
        self.exp_pitch = pitch;
        self.exp_height = height;
        self.exp_num_slices = num_slices;
        let mut exp_tile_mode = tile_mode;
        let mut micro_tile_thickness = surface_thickness(tile_mode);

        if mip_level != 0 {
            self.exp_pitch = next_pow2(pitch);
            self.exp_height = next_pow2(height);
            if (flags >> 4) & 1 != 0 {
                self.exp_num_slices = num_slices;
                pad_dims = if num_slices <= 1 { 2 } else { 0 };
            } else {
                self.exp_num_slices = next_pow2(num_slices);
            }
            if exp_tile_mode == 7 && self.exp_num_slices < 4 {
                exp_tile_mode = 4;
                micro_tile_thickness = 1;
            }
        }

        if tile_mode == base_tile_mode
            || mip_level == 0
            || !is_thick_macro_tiled(base_tile_mode)
            || is_thick_macro_tiled(tile_mode)
        {
            return self.finish_macro_tiled(
                tile_mode,
                exp_tile_mode,
                bpp,
                num_samples,
                pitch,
                pad_dims,
                flags,
                micro_tile_thickness,
            );
        }

        let (_, pitch_align, height_align) =
            alignments_macro_tiled(base_tile_mode, bpp, flags, num_samples);
        let pitch_align_factor = (32 / bpp).max(1);
        if self.exp_pitch < pitch_align * pitch_align_factor || self.exp_height < height_align {
            // The reference returns the base mode's pitch/height alignments
            // next to the micro-tiled layout here.
            let micro = self.info_micro_tiled(
                2,
                bpp,
                num_samples,
                pitch,
                height,
                num_slices,
                mip_level,
                pad_dims,
                flags,
            );
            return Layout {
                pitch_align,
                height_align,
                ..micro
            };
        }
        self.finish_macro_tiled(
            tile_mode,
            exp_tile_mode,
            bpp,
            num_samples,
            pitch,
            pad_dims,
            flags,
            micro_tile_thickness,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_macro_tiled(
        &mut self,
        tile_mode: u32,
        exp_tile_mode: u32,
        bpp: u32,
        num_samples: u32,
        pitch: u32,
        pad_dims: u32,
        flags: u32,
        micro_tile_thickness: u32,
    ) -> Layout {
        let (base_align, mut pitch_align, height_align) =
            alignments_macro_tiled(tile_mode, bpp, flags, num_samples);
        let bank_swapped = bank_swapped_width(tile_mode, bpp, num_samples, pitch);
        if bank_swapped > pitch_align {
            pitch_align = bank_swapped;
        }
        self.pad_dimensions(
            tile_mode,
            pad_dims,
            (flags >> 4) & 1,
            pitch_align,
            height_align,
            micro_tile_thickness,
        );
        Layout {
            pitch: self.exp_pitch,
            height: self.exp_height,
            num_slices: self.exp_num_slices,
            surf_size: self.surf_size(self.exp_num_slices, bpp, num_samples),
            tile_mode: exp_tile_mode,
            base_align,
            pitch_align,
            height_align,
            depth_align: micro_tile_thickness,
        }
    }

    fn compute_mip_level(&mut self) {
        let input = &mut self.input;
        let is_block = (49..=55).contains(&input.format);
        if is_block && (input.mip_level == 0 || (input.flags >> 12) & 1 != 0) {
            input.width = pow_two_align(input.width, 4);
            input.height = pow_two_align(input.height, 4);
        }
        // hwlComputeMipLevel
        let mut handled = false;
        if is_block {
            if input.mip_level != 0 {
                let mut width = input.width;
                let mut height = input.height;
                let mut slices = input.num_slices;
                if (input.flags >> 12) & 1 != 0 {
                    let widtha = width >> input.mip_level;
                    let heighta = height >> input.mip_level;
                    if (input.flags >> 4) & 1 == 0 {
                        slices >>= input.mip_level;
                    }
                    width = widtha.max(1);
                    height = heighta.max(1);
                    slices = slices.max(1);
                }
                input.width = next_pow2(width);
                input.height = next_pow2(height);
                input.num_slices = slices;
            }
            handled = true;
        }
        if !handled && input.mip_level != 0 && (input.flags >> 12) & 1 != 0 {
            let mut width = (input.width >> input.mip_level).max(1);
            let mut height = (input.height >> input.mip_level).max(1);
            let mut slices = input.num_slices.max(1);
            if (input.flags >> 4) & 1 == 0 {
                slices = (slices >> input.mip_level).max(1);
            }
            if input.format != 47 && input.format != 48 {
                width = next_pow2(width);
                height = next_pow2(height);
                slices = next_pow2(slices);
            }
            input.width = width;
            input.height = height;
            input.num_slices = slices;
        }
    }

    /// `adjustSurfaceInfo`
    fn adjust_surface_info(
        &mut self,
        elem_mode: u32,
        expand_x: u32,
        expand_y: u32,
        bpp: u32,
        width: u32,
        height: u32,
    ) {
        let bcn_format = matches!(elem_mode, 9..=13) && bpp != 0;
        if width != 0 && height != 0 && (expand_x > 1 || expand_y > 1) {
            let (widtha, heighta) = if elem_mode == 4 {
                (expand_x * width, expand_y * height)
            } else if bcn_format {
                (width / expand_x, height / expand_y)
            } else {
                (
                    (width + expand_x - 1) / expand_x,
                    (height + expand_y - 1) / expand_y,
                )
            };
            self.input.width = widtha.max(1);
            self.input.height = heighta.max(1);
        }
        if bpp != 0 {
            self.input.bpp = match elem_mode {
                4 => bpp / expand_x / expand_y,
                5 | 6 => expand_y * expand_x * bpp,
                9 | 12 => 64,
                10 | 11 | 13 => 128,
                _ => bpp,
            };
        }
    }

    /// `restoreSurfaceInfo`
    fn restore_surface_info(&mut self, elem_mode: u32, expand_x: u32, expand_y: u32) {
        if self.out.pixel_pitch != 0 && self.out.pixel_height != 0 {
            let mut width = self.out.pixel_pitch;
            let mut height = self.out.pixel_height;
            if expand_x > 1 || expand_y > 1 {
                if elem_mode == 4 {
                    width /= expand_x;
                    height /= expand_y;
                } else {
                    width = width.wrapping_mul(expand_x);
                    height = height.wrapping_mul(expand_y);
                }
            }
            self.out.pixel_pitch = width.max(1);
            self.out.pixel_height = height.max(1);
        }
    }

    /// `ComputeSurfaceInfoEx`
    fn compute_ex(&mut self) {
        let input = self.input;
        let bpp = input.bpp;
        let num_samples = input.num_samples.max(1);
        let pitch = input.width;
        let height = input.height;
        let num_slices = input.num_slices;
        let mip_level = input.mip_level;
        let flags = input.flags;
        let base_tile_mode = input.tile_mode;
        let pad_dims = if (flags >> 4) & 1 != 0 && mip_level == 0 {
            2
        } else {
            0
        };
        let tile_mode = if (flags >> 6) & 1 != 0 {
            to_non_bank_swapped(base_tile_mode)
        } else {
            mip_level_tile_mode(
                base_tile_mode,
                bpp,
                mip_level,
                pitch,
                height,
                num_slices,
                num_samples,
                (flags >> 1) & 1,
                false,
            )
        };
        let layout = match tile_mode {
            0 | 1 => self.info_linear(
                tile_mode,
                bpp,
                num_samples,
                pitch,
                height,
                num_slices,
                mip_level,
                pad_dims,
                flags,
            ),
            2 | 3 => self.info_micro_tiled(
                tile_mode,
                bpp,
                num_samples,
                pitch,
                height,
                num_slices,
                mip_level,
                pad_dims,
                flags,
            ),
            _ => self.info_macro_tiled(
                tile_mode,
                base_tile_mode,
                bpp,
                num_samples,
                pitch,
                height,
                num_slices,
                mip_level,
                pad_dims,
                flags,
            ),
        };
        self.out.pitch = layout.pitch;
        self.out.height = layout.height;
        self.out.depth = layout.num_slices;
        self.out.tile_mode = layout.tile_mode;
        self.out.surf_size = layout.surf_size;
        self.out.base_align = layout.base_align;
        self.out.pitch_align = layout.pitch_align;
        self.out.height_align = layout.height_align;
        self.out.depth_align = layout.depth_align;
    }

    /// `computeSurfaceInfo`
    fn compute(&mut self) {
        self.compute_mip_level();
        let width = self.input.width;
        let height = self.input.height;
        self.out.pixel_bits = self.input.bpp;
        let format = self.input.format as usize;
        let bpp = u32::from(FORMAT_EX_INFO[format * 4]);
        let expand_x = u32::from(FORMAT_EX_INFO[format * 4 + 1]);
        let expand_y = u32::from(FORMAT_EX_INFO[format * 4 + 2]);
        let elem_mode = u32::from(FORMAT_EX_INFO[format * 4 + 3]);
        if elem_mode == 4 && expand_x == 3 && self.input.tile_mode == 1 {
            self.input.flags |= 0x200;
        }
        self.adjust_surface_info(elem_mode, expand_x, expand_y, bpp, width, height);
        self.compute_ex();

        self.out.bpp = self.input.bpp;
        self.out.pixel_pitch = self.out.pitch;
        self.out.pixel_height = self.out.height;
        if (self.input.flags >> 9) & 1 == 0 || self.input.mip_level == 0 {
            self.restore_surface_info(elem_mode, expand_x, expand_y);
        }
        if (self.input.flags >> 5) & 1 != 0 {
            self.out.slice_size = self.out.surf_size;
        } else {
            self.out.slice_size = self.out.surf_size / self.out.depth.max(1);
            if self.input.slice == self.input.num_slices.wrapping_sub(1)
                && self.input.num_slices > 1
            {
                self.out.slice_size = self.out.slice_size.wrapping_add(
                    self.out
                        .slice_size
                        .wrapping_mul(self.out.depth.wrapping_sub(self.input.num_slices)),
                );
            }
        }
    }
}

/// `GX2.getSurfaceInfo`: the layout of mip `level` of a surface.
#[allow(clippy::too_many_arguments)]
pub fn surface_info(
    format: u32,
    width: u32,
    height: u32,
    depth: u32,
    dim: u32,
    tile_mode: u32,
    aa: u32,
    level: u32,
) -> io::Result<SurfaceInfo> {
    let hw_format = format & 0x3f;
    let hw_bpp = u32::from(FORMAT_HW_INFO[(hw_format * 4) as usize]);
    if hw_format == 0 || hw_bpp == 0 || u32::from(FORMAT_EX_INFO[(hw_format * 4) as usize]) == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("unsupported GX2 surface format 0x{format:X}"),
        ));
    }
    if width == 0 || height == 0 || width > 16384 || height > 16384 || level > 13 || aa > 3 {
        return Err(invalid("GX2 surface dimensions are out of range"));
    }
    let level_width = (width >> level).max(1);
    let level_height = (height >> level).max(1);
    let mut out = SurfaceInfo::default();

    if tile_mode == 16 {
        let num_samples = 1u32 << aa;
        let block_size = if (0x31..=0x35).contains(&hw_format) {
            4
        } else {
            1
        };
        let aligned_width = !(block_size - 1) & (level_width + block_size - 1);
        out.bpp = hw_bpp;
        out.pitch = aligned_width / block_size;
        out.pixel_bits = hw_bpp;
        out.base_align = 1;
        out.pitch_align = 1;
        out.height_align = 1;
        out.depth_align = 1;
        match dim {
            0 => {
                out.height = 1;
                out.depth = 1;
            }
            1 | 6 => {
                out.height = level_height;
                out.depth = 1;
            }
            2 => {
                out.height = level_height;
                out.depth = (depth >> level).max(1);
            }
            3 => {
                out.height = level_height;
                out.depth = depth.max(6);
            }
            4 => {
                out.height = 1;
                out.depth = depth;
            }
            5 | 7 => {
                out.height = level_height;
                out.depth = depth;
            }
            _ => {}
        }
        out.pixel_pitch = aligned_width;
        out.pixel_height = !(block_size - 1) & (out.height + block_size - 1);
        out.height = out.pixel_height / block_size;
        out.surf_size = out
            .bpp
            .wrapping_mul(num_samples)
            .wrapping_mul(out.depth)
            .wrapping_mul(out.height)
            .wrapping_mul(out.pitch)
            >> 3;
        out.slice_size = if dim == 2 {
            out.surf_size
        } else {
            out.surf_size / out.depth.max(1)
        };
    } else {
        let mut input = SurfaceIn {
            tile_mode: tile_mode & 0x0f,
            format: hw_format,
            bpp: hw_bpp,
            num_samples: 1 << aa,
            width: level_width,
            mip_level: level,
            ..SurfaceIn::default()
        };
        match dim {
            0 => {
                input.height = 1;
                input.num_slices = 1;
            }
            1 | 6 => {
                input.height = level_height;
                input.num_slices = 1;
            }
            2 => {
                input.height = level_height;
                input.num_slices = (depth >> level).max(1);
            }
            3 => {
                input.height = level_height;
                input.num_slices = depth.max(6);
                input.flags |= 0x10;
            }
            4 => {
                input.height = 1;
                input.num_slices = depth;
            }
            5 | 7 => {
                input.height = level_height;
                input.num_slices = depth;
            }
            _ => {}
        }
        if input.height == 0 || input.num_slices == 0 {
            return Err(invalid("GX2 surface has an unsupported dimension"));
        }
        if dim == 2 {
            input.flags |= 0x20;
        }
        if level == 0 {
            input.flags = (1 << 12) | (input.flags & 0xffff_efff);
        } else {
            input.flags &= 0xffff_efff;
        }
        let mut calc = Calc {
            input,
            out,
            exp_pitch: 0,
            exp_height: 0,
            exp_num_slices: 0,
        };
        calc.compute();
        out = calc.out;
    }
    if out.tile_mode == 0 {
        out.tile_mode = 16;
    }
    Ok(out)
}

/// `GX2.getDefaultGX2TileMode`
pub fn default_tile_mode(
    dim: u32,
    width: u32,
    height: u32,
    depth: u32,
    format: u32,
    aa: u32,
    use_: u32,
) -> io::Result<u32> {
    let mut tile_mode = 1;
    let is_depth_buffer = use_ & 4 != 0;
    let is_color_buffer = use_ & 2 != 0;
    if dim != 0 || aa != 0 || is_depth_buffer {
        tile_mode = if dim != 2 || is_color_buffer { 4 } else { 7 };
        let info = surface_info(format, width, height, depth, dim, tile_mode, aa, 0)?;
        if width < info.pitch_align && height < info.height_align {
            tile_mode = if tile_mode == 7 { 3 } else { 2 };
        }
    }
    Ok(tile_mode)
}

fn round_up(x: u32, y: u32) -> u32 {
    (x.wrapping_sub(1) | y.wrapping_sub(1)).wrapping_add(1)
}

fn div_round_up(n: u32, d: u32) -> u32 {
    (n + d - 1) / d
}

/// `TextureHelper.GetCurrentMipSize`: `(offset, size)` of `level` inside a
/// linear mip chain packed back to back.
pub fn linear_mip_range(
    width: u32,
    height: u32,
    block_width: u32,
    block_height: u32,
    bytes_per_block: u32,
    level: u32,
) -> (usize, usize) {
    let size_of = |level: u32| {
        div_round_up((width >> level).max(1), block_width) as usize
            * div_round_up((height >> level).max(1), block_height) as usize
            * bytes_per_block as usize
    };
    ((0..level).map(size_of).sum(), size_of(level))
}

/// What `GX2.CreateGx2Texture` produces from a linear mip chain.
#[derive(Debug, Clone)]
pub struct CreatedSurface {
    pub tile_mode: u32,
    pub swizzle: u32,
    pub alignment: u32,
    pub pitch: u32,
    pub mip_count: u32,
    /// One entry per level above 0.
    pub mip_offsets: Vec<u32>,
    pub data: Vec<u8>,
    pub mip_data: Vec<u8>,
}

/// `GX2.CreateGx2Texture`: tiles a linear mip chain (`image_data`, levels
/// packed back to back) into a GX2 surface.
#[allow(clippy::too_many_arguments)]
pub fn create_surface(
    image_data: &[u8],
    tile_mode: u32,
    aa: u32,
    width: u32,
    height: u32,
    depth: u32,
    format: u32,
    swizzle_pattern: u32,
    dim: u32,
    mip_count: u32,
) -> io::Result<CreatedSurface> {
    let mut tile_mode = tile_mode;
    let mut surf = surface_info(format, width, height, depth, dim, tile_mode, aa, 0)?;
    let image_size = surf.surf_size;
    let alignment = surf.base_align;
    let pitch = surf.pitch;
    let mut mip_size = 0u32;
    let bpp = bits_per_pixel(format) >> 3;
    if image_data.is_empty() {
        return Err(invalid("the replacement image is empty"));
    }
    let mut s = if matches!(tile_mode, 1 | 2 | 3 | 16) {
        swizzle_pattern << 8
    } else {
        0xd0000 | (swizzle_pattern << 8)
    };
    let block = if is_bcn(format) { 4 } else { 1 };
    if tile_mode == 0 {
        tile_mode = default_tile_mode(dim, width, height, 1, format, 0, 1)?;
    }
    let mut tiling_depth = surf.depth;
    if tile_mode == 3 {
        tiling_depth /= 4;
    }
    if tiling_depth != 1 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("unsupported GX2 surface depth {}", surf.depth),
        ));
    }

    let mut tiling_1d_level = 0u32;
    let mut tiling_1d_level_set = false;
    let mut mip_offsets = Vec::new();
    let mut data = Vec::new();
    let mut mip_data = Vec::new();
    let mip_count = mip_count.max(1);
    for level in 0..mip_count {
        let (offset, size) = linear_mip_range(width, height, block, block, bpp, level);
        let source = image_data
            .get(offset..offset + size)
            .ok_or_else(|| invalid("encoded mip chain is too short"))?;
        let level_width = (width >> level).max(1);
        let level_height = (height >> level).max(1);
        if level != 0 {
            surf = surface_info(format, width, height, 1, 1, tile_mode, 0, level)?;
            mip_offsets.push(if level == 1 { image_size } else { mip_size });
        }
        let mut padded = source.to_vec();
        padded.resize((surf.surf_size as usize).max(size), 0);
        let align_bytes = round_up(mip_size, surf.base_align).wrapping_sub(mip_size) as usize;
        if level != 0 {
            mip_size = mip_size
                .wrapping_add(surf.surf_size)
                .wrapping_add(align_bytes as u32);
        }
        let tiled = swizzle_surface(
            level_width,
            level_height,
            surf.depth,
            format,
            0,
            1,
            surf.tile_mode,
            s,
            surf.pitch,
            surf.bpp,
            0,
            0,
            &padded,
            true,
        )?;
        let target = if level == 0 { &mut data } else { &mut mip_data };
        target.resize(target.len() + align_bytes, 0);
        target.extend_from_slice(&tiled);

        if matches!(surf.tile_mode, 1 | 2 | 3 | 16) {
            tiling_1d_level_set = true;
        }
        if !tiling_1d_level_set {
            tiling_1d_level += 1;
        }
    }
    if tiling_1d_level_set {
        s |= tiling_1d_level << 16;
    } else {
        s |= 13 << 16;
    }
    Ok(CreatedSurface {
        tile_mode,
        swizzle: s,
        alignment,
        pitch,
        mip_count,
        mip_offsets,
        data,
        mip_data,
    })
}

/// The stored fields of a surface that [`decode_level`] needs.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceDesc<'a> {
    pub dim: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub mip_count: u32,
    pub format: u32,
    pub aa: u32,
    pub use_: u32,
    pub tile_mode: u32,
    pub swizzle: u32,
    pub mip_offsets: &'a [u32],
    pub data: &'a [u8],
    pub mip_data: &'a [u8],
}

impl SurfaceDesc<'_> {
    /// Array layers (cube faces count as layers); 3D depth is not a layer.
    pub fn array_count(&self) -> u32 {
        match self.dim {
            3 | 4 | 5 | 7 => self.depth.max(1),
            _ => 1,
        }
    }

    /// Levels that are actually present: BOTW keeps the mip chain of
    /// `.Tex1` textures in a separate `.Tex2` file.
    pub fn stored_mip_count(&self) -> u32 {
        if self.mip_data.is_empty() {
            1
        } else {
            self.mip_count.clamp(1, 14)
        }
    }
}

/// `GX2.Decode`: untiles one array slice of one mip level into linear
/// (row-major) texels or 4x4 blocks.
pub fn decode_level(
    surface: &SurfaceDesc<'_>,
    array_index: u32,
    level: u32,
) -> io::Result<Vec<u8>> {
    if level >= surface.stored_mip_count() || array_index >= surface.array_count() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "GX2 subimage index is out of range",
        ));
    }
    let tile_mode = if surface.tile_mode == 0 {
        default_tile_mode(
            surface.dim,
            surface.width,
            surface.height,
            surface.depth,
            surface.format,
            surface.aa,
            surface.use_,
        )?
    } else {
        surface.tile_mode
    };
    let image = surface_info(
        surface.format,
        surface.width,
        surface.height,
        surface.depth,
        surface.dim,
        tile_mode,
        surface.aa,
        0,
    )?;
    let info = if level == 0 {
        image
    } else {
        surface_info(
            surface.format,
            surface.width,
            surface.height,
            surface.depth,
            surface.dim,
            tile_mode,
            surface.aa,
            level,
        )?
    };
    let source = if level == 0 {
        surface.data
    } else {
        let mut offset = surface
            .mip_offsets
            .get(level as usize - 1)
            .copied()
            .unwrap_or(0) as usize;
        if level == 1 {
            // The first offset is relative to the start of the image data.
            offset = offset.saturating_sub(image.surf_size as usize);
        }
        surface
            .mip_data
            .get(offset..)
            .ok_or_else(|| invalid("GX2 mip offset lies outside the mip data"))?
    };
    let level_width = (surface.width >> level).max(1);
    let level_height = (surface.height >> level).max(1);
    let block = if is_bcn(surface.format) { 4 } else { 1 };
    let size = div_round_up(level_width, block) as usize
        * div_round_up(level_height, block) as usize
        * (info.bpp as usize).div_ceil(8);
    // The address computation is bounded by the tiled size of the level.
    let needed = (info.surf_size as usize).max(size);
    let mut tiled = source[..source.len().min(needed)].to_vec();
    tiled.resize(needed, 0);
    let mut linear = swizzle_surface(
        level_width,
        level_height,
        info.depth,
        surface.format,
        0,
        surface.use_,
        info.tile_mode,
        surface.swizzle,
        info.pitch,
        info.bpp,
        array_index,
        0,
        &tiled,
        false,
    )?;
    linear.truncate(size);
    Ok(linear)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba8_2d_tiled_layout_matches_the_botw_icon() {
        // Armor_878_Head.sbitemico: 111x111 TCS_R8_G8_B8_A8_SRGB, 2D_TILED_THIN1
        let info = surface_info(0x41a, 111, 111, 1, 1, 4, 0, 0).unwrap();
        assert_eq!(info.pitch, 128);
        assert_eq!(info.height, 112);
        assert_eq!(info.surf_size, 0xe000);
        assert_eq!(info.base_align, 0x800);
        assert_eq!(info.tile_mode, 4);
    }

    #[test]
    fn tiling_round_trips() {
        for &(format, width, height, mips) in &[
            (0x1au32, 64u32, 40u32, 3u32),
            (0x31, 256, 256, 9),
            (0x33, 300, 130, 9),
            (0x01, 37, 5, 1),
        ] {
            let block = if is_bcn(format) { 4 } else { 1 };
            let bpp = bits_per_pixel(format) >> 3;
            let (offset, size) = linear_mip_range(width, height, block, block, bpp, mips - 1);
            let linear: Vec<u8> = (0..offset + size).map(|i| (i * 31 + i / 7) as u8).collect();
            let surface =
                create_surface(&linear, 4, 0, width, height, 1, format, 0, 1, mips).unwrap();
            let desc = SurfaceDesc {
                dim: 1,
                width,
                height,
                depth: 1,
                mip_count: mips,
                format,
                aa: 0,
                use_: 1,
                tile_mode: surface.tile_mode,
                swizzle: surface.swizzle,
                mip_offsets: &surface.mip_offsets,
                data: &surface.data,
                mip_data: &surface.mip_data,
            };
            for level in 0..mips {
                let (offset, size) = linear_mip_range(width, height, block, block, bpp, level);
                assert_eq!(
                    decode_level(&desc, 0, level).unwrap(),
                    &linear[offset..offset + size],
                    "format {format:#x} level {level}"
                );
            }
        }
    }
}
