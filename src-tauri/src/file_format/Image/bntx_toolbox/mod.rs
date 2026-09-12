//! Switch Toolbox compatible BNTX editing.
//!
//! Loads a texture container the way Toolbox's Syroot fork does, applies the
//! same edits its command line mode offers (internal name, texture renames,
//! image replacement) and serializes with a port of `BntxFileSaver`, so an
//! open-and-save or a rename produces exactly the bytes Toolbox writes. The
//! `.zs` variant goes through the bundled libzstd 1.3.3 like Toolbox's
//! ZstdNet backend, so compressed files match byte for byte as well.

mod importer;
mod loader;
mod model;
mod saver;

pub use importer::{find_astc_encoder, total_mip_count};
pub use model::{BntxFile, BntxToolboxError, Texture};

use std::path::Path;

impl BntxFile {
    /// Parses a little-endian BNTX file (already decompressed).
    pub fn load(data: &[u8]) -> model::Result<Self> {
        loader::load(data)
    }

    /// Serializes the file exactly like Switch Toolbox does (raw).
    pub fn save_like_toolbox(&self) -> model::Result<Vec<u8>> {
        saver::save(self)
    }

    /// `save_like_toolbox` followed by Toolbox's `Zstb` compression
    /// (libzstd 1.3.3, level 19, no dictionary).
    pub fn save_like_toolbox_zs(&self) -> model::Result<Vec<u8>> {
        let raw = self.save_like_toolbox()?;
        Ok(crate::compression::toolbox_zstd::compress_like_toolbox(
            &raw,
            crate::compression::toolbox_zstd::TOOLBOX_ZSTD_LEVEL,
        )?)
    }

    /// Replaces the image of texture `index` from a picture file, keeping
    /// the surface format (ASTC through astcenc, BC natively). Returns a
    /// warning text when something could not be matched exactly.
    pub fn replace_texture_from_file(
        &mut self,
        index: usize,
        picture: &Path,
        encoder: Option<&Path>,
    ) -> model::Result<Option<String>> {
        let image = image::open(picture)
            .map_err(|error| BntxToolboxError(format!("{}: {error}", picture.display())))?
            .to_rgba8();
        let texture = self
            .textures
            .get_mut(index)
            .ok_or_else(|| BntxToolboxError(format!("texture index {index} out of range")))?;
        let mip_count = texture.mip_count;
        importer::replace_texture_from_image(texture, &image, mip_count, encoder)
    }

    /// Decodes texture `index` (first array slice, mip 0) to an RGBA image.
    pub fn decode_texture(&self, index: usize) -> model::Result<image::RgbaImage> {
        let texture = self
            .textures
            .get(index)
            .ok_or_else(|| BntxToolboxError(format!("texture index {index} out of range")))?;
        let data = texture
            .texture_data
            .first()
            .ok_or_else(|| BntxToolboxError("texture has no image data".into()))?;
        let linear = texture.tile_mode == model::TILE_MODE_LINEAR_ALIGNED;
        let log2 = texture.block_height_log2 as u8;
        let image = if let Some((block_width, block_height)) =
            crate::file_format::Image::switch_texture::astc_block_from_bntx(texture.format)
        {
            if linear {
                return Err(BntxToolboxError(
                    "linear ASTC textures cannot be previewed".into(),
                ));
            }
            crate::file_format::Image::switch_texture::decode_astc(
                texture.width,
                texture.height,
                data,
                block_width,
                block_height,
                log2,
            )?
        } else {
            let format =
                crate::file_format::Image::switch_texture::format_from_bntx(texture.format)?;
            crate::file_format::Image::switch_texture::decode(
                texture.width,
                texture.height,
                format,
                data,
                log2,
                linear,
            )?
        };
        Ok(image)
    }
}

impl Texture {
    /// Human readable one-liner used by the CLI log.
    pub fn describe(&self) -> String {
        format!(
            "{} {}x{} {} mips={} array={}",
            self.name,
            self.width,
            self.height,
            format_name(self.format),
            self.mip_count,
            self.texture_data.len()
        )
    }
}

/// Toolbox style name of a BNTX surface format (`ASTC_4x4_SRGB`, `BC7_UNORM`…).
pub fn format_name(format: u32) -> String {
    let suffix = match format & 0xff {
        1 => "UNORM",
        2 => "SNORM",
        3 => "UINT",
        4 => "SINT",
        5 => "FLOAT",
        6 => "SRGB",
        _ => "UNKNOWN",
    };
    if let Some((w, h)) = crate::file_format::Image::switch_texture::astc_block_from_bntx(format) {
        return format!("ASTC_{w}x{h}_{suffix}");
    }
    let base = match format >> 8 {
        0x02 => "R8",
        0x07 => "R5G6B5",
        0x09 => "R8G8",
        0x0a => "R16",
        0x0b => "R8G8B8A8",
        0x0c => "B8G8R8A8",
        0x0e => "R10G10B10A2",
        0x0f => "R16G16",
        0x14 => "R32",
        0x16 => "R16G16B16A16",
        0x17 => "R32G32",
        0x1a => "BC1",
        0x1b => "BC2",
        0x1c => "BC3",
        0x1d => "BC4",
        0x1e => "BC5",
        0x1f => "BC6H",
        0x20 => "BC7",
        _ => return format!("0x{format:08X}"),
    };
    format!("{base}_{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus(name: &str) -> Option<Vec<u8>> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/bntx")
            .join(name);
        let data = std::fs::read(path).ok()?;
        zstd::bulk::decompress(&data, 64 << 20).ok()
    }

    fn scratch(name: &str) -> Option<Vec<u8>> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/_CLAUDE/bntx_test")
            .join(name);
        std::fs::read(path).ok()
    }

    #[test]
    fn unmodified_resave_is_byte_identical() {
        let Some(raw) = corpus("Armor_001_Head_Black.bntx.zs") else {
            return;
        };
        let file = BntxFile::load(&raw).unwrap();
        assert_eq!(file.name, "Armor_001_Head_Black");
        assert_eq!(file.textures.len(), 1);
        assert_eq!(
            file.textures[0].describe(),
            "Armor_001_Head_Black 256x256 ASTC_4x4_SRGB mips=1 array=1"
        );
        let saved = file.save_like_toolbox().unwrap();
        assert_eq!(saved, raw);
    }

    #[test]
    fn rename_matches_toolbox_reference() {
        let (Some(raw), Some(expected)) = (
            corpus("Armor_001_Head_Black.bntx.zs"),
            scratch("roundtrip.bntx.zs"),
        ) else {
            return;
        };
        let mut file = BntxFile::load(&raw).unwrap();
        file.set_internal_name("Armor_900_Head_Black");
        file.rename_texture(0, "Armor_900_Head_Black").unwrap();
        let saved = file.save_like_toolbox().unwrap();
        let expected_raw = zstd::bulk::decompress(&expected, 64 << 20).unwrap();
        assert_eq!(saved, expected_raw);
        #[cfg(windows)]
        assert_eq!(file.save_like_toolbox_zs().unwrap(), expected);
    }

    #[test]
    fn whole_corpus_resaves_byte_identical() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bntx");
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.to_string_lossy().ends_with(".bntx.zs") {
                continue;
            }
            let raw = zstd::bulk::decompress(&std::fs::read(&path).unwrap(), 64 << 20).unwrap();
            let file = BntxFile::load(&raw).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let saved = file.save_like_toolbox().unwrap();
            assert_eq!(saved, raw, "{}", path.display());
            count += 1;
        }
        assert!(count == 0 || count > 100, "resaved {count} files");
        eprintln!("resaved {count} corpus files");
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "needs astcenc and the Toolbox reference outputs in tmp/_CLAUDE/bntx_test"]
    fn png_replacement_matches_toolbox_reference() {
        let (Some(raw), Some(expected), Some(png)) = (
            corpus("Armor_001_Head_Black.bntx.zs"),
            scratch("replaced.bntx.zs"),
            Some(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../tmp/_CLAUDE/bntx_test/test_icon.png"),
            ),
        ) else {
            return;
        };
        let encoder = find_astc_encoder(None).expect("astcenc not found");
        let mut file = BntxFile::load(&raw).unwrap();
        let warning = file
            .replace_texture_from_file(0, &png, Some(&encoder))
            .unwrap();
        assert_eq!(warning, None);
        assert_eq!(file.save_like_toolbox_zs().unwrap(), expected);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "needs astcenc and the Toolbox reference outputs in tmp/_CLAUDE/bntx_test"]
    fn rename_plus_named_replacement_matches_toolbox_reference() {
        let (Some(raw), Some(expected)) =
            (corpus("Armor_001_Upper.bntx.zs"), scratch("named.bntx.zs"))
        else {
            return;
        };
        let png = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/_CLAUDE/bntx_test/test_icon.png");
        let encoder = find_astc_encoder(None).expect("astcenc not found");
        let mut file = BntxFile::load(&raw).unwrap();
        file.rename_texture(0, "Armor_777_Upper").unwrap();
        file.replace_texture_from_file(0, &png, Some(&encoder))
            .unwrap();
        assert_eq!(file.save_like_toolbox_zs().unwrap(), expected);
    }

    /// A smaller picture changes the image size, so the relocation table is
    /// rebuilt instead of copied from the vanilla file.
    #[cfg(windows)]
    #[test]
    #[ignore = "needs astcenc and the Toolbox reference outputs in tmp/_CLAUDE/bntx_test"]
    fn smaller_replacement_rebuilds_the_relocation_table_like_toolbox() {
        let (Some(raw), Some(expected)) = (
            corpus("Armor_001_Head_Black.bntx.zs"),
            scratch("small.bntx.zs"),
        ) else {
            return;
        };
        let png = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/_CLAUDE/bntx_test/small.png");
        let encoder = find_astc_encoder(None).expect("astcenc not found");
        let mut file = BntxFile::load(&raw).unwrap();
        file.replace_texture_from_file(0, &png, Some(&encoder))
            .unwrap();
        assert_eq!(file.textures[0].width, 128);
        assert_eq!(file.save_like_toolbox_zs().unwrap(), expected);
    }
}
