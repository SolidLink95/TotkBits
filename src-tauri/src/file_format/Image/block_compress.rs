//! BC1 / BC4 / BC5 block compressors that reproduce the DirectXTex
//! (`D3DXEncodeBC1`, `D3DXEncodeBC4U`, `D3DXEncodeBC5U`) results bit for
//! bit: the same single-precision arithmetic in the same order, the same
//! luminance-weighted endpoint search, the same partial-block replication.
//!
//! Inputs are 8-bit RGBA texels; they enter the encoder as `value * (1/255)`
//! like DirectXMath's `XMLoadUByteN4`, and the BC1 alpha cut-off is the
//! library default of 0.5.
//!
//! Verified byte-identical for BC1 on a corpus of real and synthetic
//! textures (every mip). BC4/BC5 end points match too; the index of a texel
//! sitting exactly half-way between two palette entries still comes out one
//! step different from the reference in roughly 0.1% of the blocks (the
//! reference's fast-math build rounds those ties in a way that has not been
//! pinned down).
use image::RgbaImage;
use image_dds::ImageFormat;
use std::io;

const PIXELS_PER_BLOCK: usize = 16;
const UBYTE_TO_FLOAT: f32 = 1.0 / 255.0;
/// `TEX_THRESHOLD_DEFAULT`: alpha below this becomes the BC1 punch-through index.
pub const BC1_ALPHA_THRESHOLD: f32 = 0.5;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rgba {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

impl Rgba {
    const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

const LUMINANCE: Rgba = Rgba::new(0.2125 / 0.7154, 1.0, 0.0721 / 0.7154, 1.0);
const LUMINANCE_INV: Rgba = Rgba::new(0.7154 / 0.2125, 1.0, 0.7154 / 0.0721, 1.0);

/// Encodes a whole image into linear (row-major) blocks of `format`.
/// Partial edge blocks replicate texels the way the reference does:
/// column/row 2 copies 0, column/row 3 copies 1.
pub fn encode_linear(image: &RgbaImage, format: ImageFormat) -> io::Result<Vec<u8>> {
    let (width, height) = (image.width() as usize, image.height() as usize);
    if width == 0 || height == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty image"));
    }
    let blocks_wide = width.div_ceil(4);
    let blocks_high = height.div_ceil(4);
    let block_bytes = match format {
        ImageFormat::BC1RgbaUnorm | ImageFormat::BC1RgbaUnormSrgb => 8,
        ImageFormat::BC4RUnorm => 8,
        ImageFormat::BC5RgUnorm => 16,
        other => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("no exact block compressor for {other:?}"),
            ))
        }
    };
    let mut out = Vec::with_capacity(blocks_wide * blocks_high * block_bytes);
    let mut texels = [Rgba::default(); PIXELS_PER_BLOCK];
    for block_y in 0..blocks_high {
        for block_x in 0..blocks_wide {
            let pw = (width - block_x * 4).min(4);
            let ph = (height - block_y * 4).min(4);
            for y in 0..ph {
                for x in 0..pw {
                    let p = image
                        .get_pixel((block_x * 4 + x) as u32, (block_y * 4 + y) as u32)
                        .0;
                    texels[y * 4 + x] = Rgba::new(
                        f32::from(p[0]) * UBYTE_TO_FLOAT,
                        f32::from(p[1]) * UBYTE_TO_FLOAT,
                        f32::from(p[2]) * UBYTE_TO_FLOAT,
                        f32::from(p[3]) * UBYTE_TO_FLOAT,
                    );
                }
            }
            if pw != 4 || ph != 4 {
                const SRC: [usize; 4] = [0, 0, 0, 1];
                if pw < 4 {
                    for t in 0..ph.min(4) {
                        for s in pw..4 {
                            texels[(t << 2) | s] = texels[(t << 2) | SRC[s]];
                        }
                    }
                }
                if ph < 4 {
                    for t in ph..4 {
                        for s in 0..4 {
                            texels[(t << 2) | s] = texels[(SRC[t] << 2) | s];
                        }
                    }
                }
            }
            match format {
                ImageFormat::BC1RgbaUnorm | ImageFormat::BC1RgbaUnormSrgb => {
                    out.extend_from_slice(&encode_bc1(&texels, BC1_ALPHA_THRESHOLD));
                }
                ImageFormat::BC4RUnorm => {
                    let mut u = [0f32; PIXELS_PER_BLOCK];
                    for (i, t) in texels.iter().enumerate() {
                        u[i] = t.r;
                    }
                    out.extend_from_slice(&encode_bc4(&u));
                }
                ImageFormat::BC5RgUnorm => {
                    let mut u = [0f32; PIXELS_PER_BLOCK];
                    let mut v = [0f32; PIXELS_PER_BLOCK];
                    for (i, t) in texels.iter().enumerate() {
                        u[i] = t.r;
                        v[i] = t.g;
                    }
                    out.extend_from_slice(&encode_bc4(&u));
                    out.extend_from_slice(&encode_bc4(&v));
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(out)
}

// ----------------------------------------------------------------------------- BC1

fn encode_565(color: &Rgba) -> u16 {
    let r = color.r.clamp(0.0, 1.0);
    let g = color.g.clamp(0.0, 1.0);
    let b = color.b.clamp(0.0, 1.0);
    (((r * 31.0 + 0.5) as i32) << 11 | ((g * 63.0 + 0.5) as i32) << 5 | ((b * 31.0 + 0.5) as i32))
        as u16
}

fn decode_565(w: u16) -> Rgba {
    Rgba::new(
        f32::from((w >> 11) & 31) * (1.0 / 31.0),
        f32::from((w >> 5) & 63) * (1.0 / 63.0),
        f32::from(w & 31) * (1.0 / 31.0),
        1.0,
    )
}

fn lerp(c1: &Rgba, c2: &Rgba, s: f32) -> Rgba {
    Rgba::new(
        c1.r + s * (c2.r - c1.r),
        c1.g + s * (c2.g - c1.g),
        c1.b + s * (c2.b - c1.b),
        c1.a + s * (c2.a - c1.a),
    )
}

/// `OptimizeRGB`: Newton refinement of the colour axis end points in
/// luminance-weighted space.
fn optimize_rgb(points: &[Rgba; PIXELS_PER_BLOCK], steps: usize) -> (Rgba, Rgba) {
    const EPSILON: f32 = (0.25 / 64.0) * (0.25 / 64.0);
    const C3: [f32; 3] = [2.0 / 2.0, 1.0 / 2.0, 0.0 / 2.0];
    const D3: [f32; 3] = [0.0 / 2.0, 1.0 / 2.0, 2.0 / 2.0];
    const C4: [f32; 4] = [3.0 / 3.0, 2.0 / 3.0, 1.0 / 3.0, 0.0 / 3.0];
    const D4: [f32; 4] = [0.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 3.0 / 3.0];
    let (pc, pd): (&[f32], &[f32]) = if steps == 3 { (&C3, &D3) } else { (&C4, &D4) };

    // Min / max start points (the points are luminance weighted).
    let mut x = LUMINANCE;
    let mut y = Rgba::new(0.0, 0.0, 0.0, 1.0);
    for p in points {
        if p.r < x.r {
            x.r = p.r;
        }
        if p.g < x.g {
            x.g = p.g;
        }
        if p.b < x.b {
            x.b = p.b;
        }
        if p.r > y.r {
            y.r = p.r;
        }
        if p.g > y.g {
            y.g = p.g;
        }
        if p.b > y.b {
            y.b = p.b;
        }
    }

    // Diagonal axis
    let ab = Rgba::new(y.r - x.r, y.g - x.g, y.b - x.b, 0.0);
    let f_ab = ab.r * ab.r + ab.g * ab.g + ab.b * ab.b;

    // Single colour block
    if f_ab < f32::MIN_POSITIVE {
        return (x, y);
    }

    // Try all four axis directions, to determine which diagonal best fits the data
    let f_ab_inv = 1.0 / f_ab;
    let mut dir = Rgba::new(ab.r * f_ab_inv, ab.g * f_ab_inv, ab.b * f_ab_inv, 0.0);
    let mid = Rgba::new((x.r + y.r) * 0.5, (x.g + y.g) * 0.5, (x.b + y.b) * 0.5, 0.0);
    let mut f_dir = [0f32; 4];
    for p in points {
        let pt = Rgba::new(
            (p.r - mid.r) * dir.r,
            (p.g - mid.g) * dir.g,
            (p.b - mid.b) * dir.b,
            0.0,
        );
        let mut f = pt.r + pt.g + pt.b;
        f_dir[0] += f * f;
        f = pt.r + pt.g - pt.b;
        f_dir[1] += f * f;
        f = pt.r - pt.g + pt.b;
        f_dir[2] += f * f;
        f = pt.r - pt.g - pt.b;
        f_dir[3] += f * f;
    }
    let mut f_dir_max = f_dir[0];
    let mut i_dir_max = 0usize;
    for (i, value) in f_dir.iter().enumerate().skip(1) {
        if *value > f_dir_max {
            f_dir_max = *value;
            i_dir_max = i;
        }
    }
    if i_dir_max & 2 != 0 {
        std::mem::swap(&mut x.g, &mut y.g);
    }
    if i_dir_max & 1 != 0 {
        std::mem::swap(&mut x.b, &mut y.b);
    }

    // Two colour block
    if f_ab < 1.0 / 4096.0 {
        return (x, y);
    }

    // Newton's method on the sum of squared errors
    let f_steps = (steps - 1) as f32;
    for _ in 0..8 {
        let mut p_steps = [Rgba::default(); 4];
        for (i, step) in p_steps.iter_mut().enumerate().take(steps) {
            step.r = x.r * pc[i] + y.r * pd[i];
            step.g = x.g * pc[i] + y.g * pd[i];
            step.b = x.b * pc[i] + y.b * pd[i];
        }

        dir.r = y.r - x.r;
        dir.g = y.g - x.g;
        dir.b = y.b - x.b;
        let f_len = dir.r * dir.r + dir.g * dir.g + dir.b * dir.b;
        if f_len < 1.0 / 4096.0 {
            break;
        }
        let f_scale = f_steps / f_len;
        dir.r *= f_scale;
        dir.g *= f_scale;
        dir.b *= f_scale;

        let (mut d2x, mut d2y) = (0f32, 0f32);
        let mut dx = Rgba::default();
        let mut dy = Rgba::default();
        for p in points {
            let f_dot = (p.r - x.r) * dir.r + (p.g - x.g) * dir.g + (p.b - x.b) * dir.b;
            let i_step = if f_dot <= 0.0 {
                0
            } else if f_dot >= f_steps {
                steps - 1
            } else {
                (f_dot + 0.5) as usize
            };
            let diff = Rgba::new(
                p_steps[i_step].r - p.r,
                p_steps[i_step].g - p.g,
                p_steps[i_step].b - p.b,
                0.0,
            );
            let f_c = pc[i_step] * (1.0 / 8.0);
            let f_d = pd[i_step] * (1.0 / 8.0);
            d2x += f_c * pc[i_step];
            dx.r += f_c * diff.r;
            dx.g += f_c * diff.g;
            dx.b += f_c * diff.b;
            d2y += f_d * pd[i_step];
            dy.r += f_d * diff.r;
            dy.g += f_d * diff.g;
            dy.b += f_d * diff.b;
        }

        if d2x > 0.0 {
            let f = -1.0 / d2x;
            x.r += dx.r * f;
            x.g += dx.g * f;
            x.b += dx.b * f;
        }
        if d2y > 0.0 {
            let f = -1.0 / d2y;
            y.r += dy.r * f;
            y.g += dy.g * f;
            y.b += dy.b * f;
        }
        if dx.r * dx.r < EPSILON
            && dx.g * dx.g < EPSILON
            && dx.b * dx.b < EPSILON
            && dy.r * dy.r < EPSILON
            && dy.g * dy.g < EPSILON
            && dy.b * dy.b < EPSILON
        {
            break;
        }
    }
    (x, y)
}

/// `EncodeBC1` with colour keying, no dithering, luminance weighting.
pub fn encode_bc1_block(pixels: &[[u8; 4]; PIXELS_PER_BLOCK], threshold: f32) -> [u8; 8] {
    let mut texels = [Rgba::default(); PIXELS_PER_BLOCK];
    for (t, p) in texels.iter_mut().zip(pixels) {
        *t = Rgba::new(
            f32::from(p[0]) * UBYTE_TO_FLOAT,
            f32::from(p[1]) * UBYTE_TO_FLOAT,
            f32::from(p[2]) * UBYTE_TO_FLOAT,
            f32::from(p[3]) * UBYTE_TO_FLOAT,
        );
    }
    encode_bc1(&texels, threshold)
}

fn encode_bc1(color: &[Rgba; PIXELS_PER_BLOCK], threshold: f32) -> [u8; 8] {
    // Colour key
    let keyed = color.iter().filter(|c| c.a < threshold).count();
    if keyed == PIXELS_PER_BLOCK {
        return pack_bc1(0x0000, 0xffff, 0xffff_ffff);
    }
    let steps = if keyed > 0 { 3 } else { 4 };

    // Quantize to the 5:6:5 grid, then move the points into
    // luminance-weighted space for the end point search.
    let mut quantized = [Rgba::default(); PIXELS_PER_BLOCK];
    for (q, c) in quantized.iter_mut().zip(color) {
        q.r = ((c.r * 31.0 + 0.5) as i32) as f32 * (1.0 / 31.0) * LUMINANCE.r;
        q.g = ((c.g * 63.0 + 0.5) as i32) as f32 * (1.0 / 63.0) * LUMINANCE.g;
        q.b = ((c.b * 31.0 + 0.5) as i32) as f32 * (1.0 / 31.0) * LUMINANCE.b;
        q.a = 1.0;
    }

    let (color_a, color_b) = optimize_rgb(&quantized, steps);
    let color_c = Rgba::new(
        color_a.r * LUMINANCE_INV.r,
        color_a.g * LUMINANCE_INV.g,
        color_a.b * LUMINANCE_INV.b,
        1.0,
    );
    let color_d = Rgba::new(
        color_b.r * LUMINANCE_INV.r,
        color_b.g * LUMINANCE_INV.g,
        color_b.b * LUMINANCE_INV.b,
        1.0,
    );
    let w_color_a = encode_565(&color_c);
    let w_color_b = encode_565(&color_d);

    if steps == 4 && w_color_a == w_color_b {
        return pack_bc1(w_color_a, w_color_b, 0);
    }

    let color_c = decode_565(w_color_a);
    let color_d = decode_565(w_color_b);

    let mut step = [Rgba::default(); 4];
    let (rgb0, rgb1);
    if (steps == 3) == (w_color_a <= w_color_b) {
        rgb0 = w_color_a;
        rgb1 = w_color_b;
        step[0] = color_c;
        step[1] = color_d;
    } else {
        rgb0 = w_color_b;
        rgb1 = w_color_a;
        step[0] = color_d;
        step[1] = color_c;
    }
    const STEPS3: [u32; 3] = [0, 2, 1];
    const STEPS4: [u32; 4] = [0, 2, 3, 1];
    let p_steps: &[u32] = if steps == 3 { &STEPS3 } else { &STEPS4 };
    if steps == 3 {
        step[2] = lerp(&step[0], &step[1], 0.5);
    } else {
        step[2] = lerp(&step[0], &step[1], 1.0 / 3.0);
        step[3] = lerp(&step[0], &step[1], 2.0 / 3.0);
    }
    // Weight the palette like the texels it is compared with.
    for s in step.iter_mut().take(steps) {
        s.r *= LUMINANCE.r;
        s.g *= LUMINANCE.g;
        s.b *= LUMINANCE.b;
    }

    let mut dir = Rgba::new(
        step[1].r - step[0].r,
        step[1].g - step[0].g,
        step[1].b - step[0].b,
        0.0,
    );
    let f_steps = (steps - 1) as f32;
    let f_scale = if w_color_a != w_color_b {
        f_steps / (dir.r * dir.r + dir.g * dir.g + dir.b * dir.b)
    } else {
        0.0
    };
    dir.r *= f_scale;
    dir.g *= f_scale;
    dir.b *= f_scale;

    let mut dw = 0u32;
    for c in color {
        if steps == 3 && c.a < threshold {
            dw = (3u32 << 30) | (dw >> 2);
        } else {
            let clr = Rgba::new(c.r * LUMINANCE.r, c.g * LUMINANCE.g, c.b * LUMINANCE.b, 0.0);
            let f_dot = (clr.r - step[0].r) * dir.r
                + (clr.g - step[0].g) * dir.g
                + (clr.b - step[0].b) * dir.b;
            let i_step = if f_dot <= 0.0 {
                0
            } else if f_dot >= f_steps {
                1
            } else {
                p_steps[(f_dot + 0.5) as usize]
            };
            dw = (i_step << 30) | (dw >> 2);
        }
    }
    pack_bc1(rgb0, rgb1, dw)
}

fn pack_bc1(rgb0: u16, rgb1: u16, bitmap: u32) -> [u8; 8] {
    let mut out = [0u8; 8];
    out[..2].copy_from_slice(&rgb0.to_le_bytes());
    out[2..4].copy_from_slice(&rgb1.to_le_bytes());
    out[4..].copy_from_slice(&bitmap.to_le_bytes());
    out
}

// ----------------------------------------------------------------------------- BC4 / BC5

/// `OptimizeAlpha<false>`: Newton refinement of the two end points of a
/// single-channel block, 8 or 6 interpolated codes.
fn optimize_alpha(points: &[f32; PIXELS_PER_BLOCK], steps: usize) -> (f32, f32) {
    const C6: [f32; 6] = [
        5.0 / 5.0,
        4.0 / 5.0,
        3.0 / 5.0,
        2.0 / 5.0,
        1.0 / 5.0,
        0.0 / 5.0,
    ];
    const D6: [f32; 6] = [
        0.0 / 5.0,
        1.0 / 5.0,
        2.0 / 5.0,
        3.0 / 5.0,
        4.0 / 5.0,
        5.0 / 5.0,
    ];
    const C8: [f32; 8] = [
        7.0 / 7.0,
        6.0 / 7.0,
        5.0 / 7.0,
        4.0 / 7.0,
        3.0 / 7.0,
        2.0 / 7.0,
        1.0 / 7.0,
        0.0 / 7.0,
    ];
    const D8: [f32; 8] = [
        0.0 / 7.0,
        1.0 / 7.0,
        2.0 / 7.0,
        3.0 / 7.0,
        4.0 / 7.0,
        5.0 / 7.0,
        6.0 / 7.0,
        7.0 / 7.0,
    ];
    let (pc, pd): (&[f32], &[f32]) = if steps == 6 { (&C6, &D6) } else { (&C8, &D8) };
    const MAX_VALUE: f32 = 1.0;
    const MIN_VALUE: f32 = 0.0;

    let mut fx = MAX_VALUE;
    let mut fy = MIN_VALUE;
    if steps == 8 {
        for p in points {
            if *p < fx {
                fx = *p;
            }
            if *p > fy {
                fy = *p;
            }
        }
    } else {
        for p in points {
            if *p < fx && *p > MIN_VALUE {
                fx = *p;
            }
            if *p > fy && *p < MAX_VALUE {
                fy = *p;
            }
        }
        if fx == fy {
            fy = MAX_VALUE;
        }
    }

    let f_steps = (steps - 1) as f32;
    for _ in 0..8 {
        if (fy - fx) < (1.0 / 256.0) {
            break;
        }
        let f_scale = f_steps / (fy - fx);

        let mut p_steps = [0f32; 8];
        for i in 0..steps {
            p_steps[i] = pc[i] * fx + pd[i] * fy;
        }
        if steps == 6 {
            p_steps[6] = MIN_VALUE;
            p_steps[7] = MAX_VALUE;
        }

        let (mut dx, mut dy, mut d2x, mut d2y) = (0f32, 0f32, 0f32, 0f32);
        for p in points {
            let f_dot = (*p - fx) * f_scale;
            let i_step = if f_dot <= 0.0 {
                if steps == 6 && *p <= fx * 0.5 {
                    6
                } else {
                    0
                }
            } else if f_dot >= f_steps {
                if steps == 6 && *p >= (fy + 1.0) * 0.5 {
                    7
                } else {
                    steps - 1
                }
            } else {
                (f_dot + 0.5) as usize
            };
            if i_step < steps {
                let f_diff = p_steps[i_step] - *p;
                dx += pc[i_step] * f_diff;
                d2x += pc[i_step] * pc[i_step];
                dy += pd[i_step] * f_diff;
                d2y += pd[i_step] * pd[i_step];
            }
        }

        if d2x > 0.0 {
            fx -= dx / d2x;
        }
        if d2y > 0.0 {
            fy -= dy / d2y;
        }
        if fx > fy {
            std::mem::swap(&mut fx, &mut fy);
        }
        if dx * dx < (1.0 / 64.0) && dy * dy < (1.0 / 64.0) {
            break;
        }
    }
    (
        fx.clamp(MIN_VALUE, MAX_VALUE),
        fy.clamp(MIN_VALUE, MAX_VALUE),
    )
}

/// The eight decoded values of a BC4 block as the reference computes them
/// (fast-math build: the end points and the interpolation weights are
/// reciprocal multiplications, `f0 * ((7 - i) / 7) + f1 * (i / 7)`).
fn bc4_palette(red0: u8, red1: u8) -> [f32; 8] {
    let f0 = f32::from(red0) * UBYTE_TO_FLOAT;
    let f1 = f32::from(red1) * UBYTE_TO_FLOAT;
    let mut out = [0f32; 8];
    out[0] = f0;
    out[1] = f1;
    for (index, value) in out.iter_mut().enumerate().skip(2) {
        let i = (index - 1) as f32;
        *value = if red0 > red1 {
            f0 * ((7.0 - i) / 7.0) + f1 * (i / 7.0)
        } else if index == 6 {
            0.0
        } else if index == 7 {
            1.0
        } else {
            f0 * ((5.0 - i) / 5.0) + f1 * (i / 5.0)
        };
    }
    out
}

/// `D3DXEncodeBC4U` for one channel of 16 texels in `0..=1`.
fn encode_bc4(texels: &[f32; PIXELS_PER_BLOCK]) -> [u8; 8] {
    const MIN_NORM: f32 = 0.0;
    const MAX_NORM: f32 = 1.0;
    let mut block_max = texels[0];
    let mut block_min = texels[0];
    for t in texels {
        if *t < block_min {
            block_min = *t;
        } else if *t > block_max {
            block_max = *t;
        }
    }
    let using_4 = MIN_NORM == block_min || MAX_NORM == block_max;
    let (end0, end1) = if !using_4 {
        let (start, end) = optimize_alpha(texels, 8);
        ((end * 255.0) as u8, (start * 255.0) as u8)
    } else {
        let (start, end) = optimize_alpha(texels, 6);
        ((start * 255.0) as u8, (end * 255.0) as u8)
    };

    let palette = bc4_palette(end0, end1);
    let mut indices = 0u64;
    for (i, t) in texels.iter().enumerate() {
        let mut best_index = 0usize;
        let mut best_delta = 100000f32;
        for (index, value) in palette.iter().enumerate() {
            let delta = (value - t).abs();
            if delta < best_delta {
                best_index = index;
                best_delta = delta;
            }
        }
        indices |= (best_index as u64) << (3 * i);
    }
    let mut out = [0u8; 8];
    out[0] = end0;
    out[1] = end1;
    out[2..].copy_from_slice(&indices.to_le_bytes()[..6]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_block_is_the_punch_through_constant() {
        let pixels = [[10u8, 20, 30, 0]; 16];
        assert_eq!(
            encode_bc1_block(&pixels, BC1_ALPHA_THRESHOLD),
            [0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
        );
    }

    #[test]
    fn flat_block_uses_a_single_index() {
        let pixels = [[200u8, 30, 30, 255]; 16];
        let block = encode_bc1_block(&pixels, BC1_ALPHA_THRESHOLD);
        assert_eq!(&block[4..], &[0, 0, 0, 0]);
        assert_eq!(block[..2], block[2..4]);
    }

    #[test]
    fn bc4_of_a_constant_block_decodes_back() {
        let block = encode_bc4(&[0.5f32; 16]);
        let palette = bc4_palette(block[0], block[1]);
        let index = (u64::from_le_bytes([
            block[2], block[3], block[4], block[5], block[6], block[7], 0, 0,
        ]) & 7) as usize;
        assert!((palette[index] - 0.5).abs() < 0.01);
    }
}
