//! Nintendo Switch Sports "insert color" tinting.
//!
//! ElsaUber character materials ship a near-white albedo and a `*_Tcl` mask
//! whose channels select the runtime tint: R = outfit / variation colour,
//! G = hair colour, B = skin colour. The game fills the material's
//! `elsa_insert_color0..2` shader parameters from its colour tables
//! (`Parameter/PlayerEquipmentColorList/{SkinColor,HairColor}` and
//! `Parameter/NpcVariationColor`); this module reproduces that fill on the
//! decoded albedo so headless renders show real skin instead of white.

use image::imageops::FilterType;

/// `Parameter/PlayerEquipmentColorList/SkinColor` (linear RGB, file order).
pub const SKIN_COLORS: [[f32; 3]; 10] = [
    [0.9098, 0.75, 0.7804],
    [1.0, 0.7294, 0.5882],
    [1.0, 0.6196, 0.4157],
    [0.6078, 0.3294, 0.2039],
    [0.3922, 0.1867, 0.1378],
    [0.1765, 0.098, 0.0824],
    [0.8941, 0.8941, 0.8941],
    [0.6118, 0.4745, 0.6353],
    [0.3843, 0.6196, 0.2902],
    [0.4471, 0.4745, 0.7373],
];

/// `Parameter/PlayerEquipmentColorList/HairColor` (linear RGB, file order).
pub const HAIR_COLORS: [[f32; 3]; 20] = [
    [0.019, 0.0118, 0.0118],
    [0.0667, 0.0235, 0.0118],
    [0.4549, 0.4078, 0.1176],
    [0.3569, 0.3608, 0.3255],
    [0.498, 0.1686, 0.0588],
    [0.4157, 0.0941, 0.2471],
    [0.0941, 0.2471, 0.3843],
    [0.23, 0.08, 0.4],
    [0.5, 0.086, 0.086],
    [0.23, 0.35, 0.09],
    [0.513, 0.529, 0.29],
    [0.6875, 0.415, 0.525],
    [0.3333, 0.498, 0.3961],
    [0.4235, 0.3, 0.549],
    [0.2706, 0.3686, 0.4706],
    [0.5608, 0.4627, 0.3882],
    [0.2392, 0.1255, 0.0274],
    [0.0078, 0.1569, 0.0863],
    [0.125, 0.01, 0.1562],
    [0.054, 0.054, 0.2],
];

/// `Parameter/NpcVariationColor` (linear RGB, file order).
pub const VARIATION_COLORS: [[f32; 3]; 12] = [
    [0.3725, 0.3725, 0.749],
    [0.8588, 0.4275, 0.4275],
    [0.2902, 0.5804, 0.2902],
    [0.702, 0.3451, 0.1059],
    [0.7686, 0.7412, 0.3843],
    [0.3608, 0.6118, 0.7216],
    [0.3608, 0.3843, 0.7216],
    [0.4784, 0.298, 0.6],
    [0.8784, 0.4392, 0.6588],
    [0.5843, 0.5843, 0.5843],
    [0.1882, 0.1686, 0.1686],
    [0.8392, 0.8235, 0.8235],
];

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TintColors {
    pub skin: Option<[f32; 3]>,
    pub hair: Option<[f32; 3]>,
    pub variation: Option<[f32; 3]>,
}

impl TintColors {
    pub fn is_empty(&self) -> bool {
        self.skin.is_none() && self.hair.is_none() && self.variation.is_none()
    }

    /// Parses a CLI tint spec: `default` (first entry of every table), `none`,
    /// or `skin,hair,outfit` one-based table indices where `0` disables a slot
    /// (`3,0,5` = skin 3, no hair tint, outfit 5).
    pub fn parse(spec: &str) -> Result<Self, String> {
        match spec.trim().to_ascii_lowercase().as_str() {
            "none" | "off" => return Ok(Self::default()),
            "default" | "" => {
                return Ok(Self {
                    skin: Some(SKIN_COLORS[0]),
                    hair: Some(HAIR_COLORS[0]),
                    variation: Some(VARIATION_COLORS[0]),
                })
            }
            _ => {}
        }
        let mut fields = spec.split(',').map(|field| {
            field
                .trim()
                .parse::<usize>()
                .map_err(|_| format!("invalid tint index: {field}"))
        });
        let mut pick = |table: &[[f32; 3]], label: &str| -> Result<Option<[f32; 3]>, String> {
            match fields.next() {
                None | Some(Ok(0)) => Ok(None),
                Some(Ok(index)) => table.get(index - 1).copied().map(Some).ok_or_else(|| {
                    format!(
                        "{label} index {index} is out of range (1..={})",
                        table.len()
                    )
                }),
                Some(Err(error)) => Err(error),
            }
        };
        Ok(Self {
            skin: pick(&SKIN_COLORS, "skin")?,
            hair: pick(&HAIR_COLORS, "hair")?,
            variation: pick(&VARIATION_COLORS, "outfit")?,
        })
    }
}

fn srgb_to_linear(value: u8) -> f32 {
    (value as f32 / 255.0).powf(2.2)
}

fn linear_to_srgb(value: f32) -> u8 {
    (value.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8
}

/// Multiplies the albedo by the tint colours where the mask channel is set,
/// in linear light like the game's shader. Hair (G) wins over skin (B) where
/// both are painted; the outfit (R) tint is applied first.
pub fn tint_albedo_png(
    albedo_png: &[u8],
    mask_png: &[u8],
    tint: &TintColors,
) -> Result<Vec<u8>, String> {
    let mut albedo = image::load_from_memory(albedo_png)
        .map_err(|error| format!("albedo decode failed: {error}"))?
        .to_rgba8();
    let mask = image::load_from_memory(mask_png)
        .map_err(|error| format!("tint mask decode failed: {error}"))?
        .to_rgba8();
    let mask = if mask.dimensions() == albedo.dimensions() {
        mask
    } else {
        image::imageops::resize(&mask, albedo.width(), albedo.height(), FilterType::Triangle)
    };
    for (pixel, mask_pixel) in albedo.pixels_mut().zip(mask.pixels()) {
        let outfit = mask_pixel[0] as f32 / 255.0;
        let hair = mask_pixel[1] as f32 / 255.0;
        let skin = (mask_pixel[2] as f32 / 255.0) * (1.0 - hair);
        let passes = [
            (tint.variation, outfit),
            (tint.skin, skin),
            (tint.hair, hair),
        ];
        if !passes
            .iter()
            .any(|(color, weight)| color.is_some() && *weight > 0.0)
        {
            continue;
        }
        let mut linear = [
            srgb_to_linear(pixel[0]),
            srgb_to_linear(pixel[1]),
            srgb_to_linear(pixel[2]),
        ];
        for (color, weight) in passes {
            let Some(color) = color else { continue };
            if weight <= 0.0 {
                continue;
            }
            for channel in 0..3 {
                let tinted = linear[channel] * color[channel];
                linear[channel] += (tinted - linear[channel]) * weight;
            }
        }
        pixel[0] = linear_to_srgb(linear[0]);
        pixel[1] = linear_to_srgb(linear[1]);
        pixel[2] = linear_to_srgb(linear[2]);
    }
    let mut encoded = std::io::Cursor::new(Vec::new());
    albedo
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(encoded.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba(rgba));
        let mut encoded = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        encoded.into_inner()
    }

    #[test]
    fn parses_tint_specs() {
        assert!(TintColors::parse("none").unwrap().is_empty());
        let default = TintColors::parse("default").unwrap();
        assert_eq!(default.skin, Some(SKIN_COLORS[0]));
        let custom = TintColors::parse("3,0,12").unwrap();
        assert_eq!(custom.skin, Some(SKIN_COLORS[2]));
        assert_eq!(custom.hair, None);
        assert_eq!(custom.variation, Some(VARIATION_COLORS[11]));
        assert!(TintColors::parse("11,1,1").is_err());
    }

    #[test]
    fn blue_mask_channel_applies_the_skin_colour() {
        let albedo = png(2, 2, [255, 255, 255, 255]);
        let mask = png(2, 2, [0, 0, 255, 255]);
        let tint = TintColors {
            skin: Some([0.25, 0.5, 1.0]),
            hair: Some([0.0, 0.0, 0.0]),
            variation: None,
        };
        let tinted = image::load_from_memory(&tint_albedo_png(&albedo, &mask, &tint).unwrap())
            .unwrap()
            .to_rgba8();
        let pixel = tinted.get_pixel(0, 0);
        assert_eq!(pixel[0], linear_to_srgb(0.25));
        assert_eq!(pixel[1], linear_to_srgb(0.5));
        assert_eq!(pixel[2], 255);
        // Unmasked texels stay untouched.
        let untouched = tint_albedo_png(&albedo, &png(2, 2, [0, 0, 0, 255]), &tint).unwrap();
        let untouched = image::load_from_memory(&untouched).unwrap().to_rgba8();
        assert_eq!(untouched.get_pixel(0, 0)[0], 255);
    }
}
