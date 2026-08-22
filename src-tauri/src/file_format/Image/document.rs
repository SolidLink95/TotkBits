use base64::Engine;
use serde::Serialize;
use std::{io, path::Path};

use crate::parser::binary::{BinaryReader, BinaryWriter, Endian as BinaryEndian};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedImage {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub mip_count: u32,
    pub dds_type: Option<String>,
    pub entries: Vec<ImageEntry>,
    pub selected_index: usize,
    pub selected_array_index: u32,
    pub selected_mip_index: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageEntry {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
    pub array_count: u32,
    pub format: String,
    pub subimages: Vec<ImageSubimage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageSubimage {
    pub array_index: u32,
    pub mip_index: u32,
    pub width: u32,
    pub height: u32,
    pub name: String,
}

pub struct ImageDocument;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BntxReplacementReport {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub similarity: f64,
}

impl ImageDocument {
    /// Clone a single-texture BNTX and rename its internal texture without
    /// decoding or replacing the existing image payload.
    pub fn clone_single_bntx_with_name(
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
        new_name: &str,
        zstd: &crate::Zstd::TotkZstd<'_>,
    ) -> io::Result<BntxReplacementReport> {
        let source = source.as_ref();
        let source_bytes = std::fs::read(source)?;
        let (data, _) = decode_compressed_bntx(&source_bytes, Some(zstd))?;
        let bntx = crate::parser::bntx::BntxFile::parse(&data).map_err(invalid)?;
        if bntx.textures.len() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "weapon BNTX must contain exactly one texture, found {}",
                    bntx.textures.len()
                ),
            ));
        }
        let texture = &bntx.textures[0];
        let format = super::switch_texture::format_from_bntx(texture.format)
            .map(|value| format!("{value:?}"))
            .unwrap_or_else(|_| format!("0x{:08X}", texture.format));
        let destination = destination.as_ref();
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, destination)?;
        // Mounted ROMFS files are read-only and Windows preserves that bit on
        // copy. The mod copy must be writable before its internal name is
        // patched in place.
        let mut permissions = std::fs::metadata(destination)?.permissions();
        if permissions.readonly() {
            permissions.set_readonly(false);
            std::fs::set_permissions(destination, permissions)?;
        }
        Self::rename_bntx_texture(destination, 0, new_name, zstd)?;
        Ok(BntxReplacementReport {
            name: new_name.to_owned(),
            width: texture.width,
            height: texture.height,
            format,
            similarity: 1.0,
        })
    }

    /// Replace the sole texture in a weapon BNTX, preserving its format and layout.
    /// The PNG is resized to the original dimensions and every mip is regenerated.
    pub fn replace_single_bntx_from_png(
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
        png: impl AsRef<Path>,
        new_name: &str,
        zstd: &crate::Zstd::TotkZstd<'_>,
    ) -> io::Result<BntxReplacementReport> {
        if new_name.is_empty() || new_name.as_bytes().contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid BNTX texture name",
            ));
        }
        let source_bytes = std::fs::read(source)?;
        let (mut data, dictionary) = decode_compressed_bntx(&source_bytes, Some(zstd))?;
        let bntx = crate::parser::bntx::BntxFile::parse(&data).map_err(invalid)?;
        if bntx.textures.len() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "weapon BNTX must contain exactly one texture, found {}",
                    bntx.textures.len()
                ),
            ));
        }
        let source_texture = &bntx.textures[0];
        if source_texture.array_length.max(1) != 1 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "array BNTX replacement is not supported for weapon images",
            ));
        }
        if super::switch_texture::astc_block_from_bntx(source_texture.format).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "ASTC BNTX replacement is not supported",
            ));
        }
        data = rename_single_bntx_texture_bytes(data, source_texture, new_name)?;
        let renamed_bntx = crate::parser::bntx::BntxFile::parse(&data).map_err(invalid)?;
        let texture = renamed_bntx.textures.first().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "renamed BNTX has no texture metadata",
            )
        })?;
        if texture.name != new_name {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "renamed BNTX failed validation",
            ));
        }
        let format = super::switch_texture::format_from_bntx(texture.format)?;
        let encoding_format = super::switch_texture::encoding_format(format);
        let supplied = image::open(png)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?
            .to_rgba8();
        let resized = if supplied.dimensions() == (texture.width, texture.height) {
            supplied
        } else {
            image::imageops::resize(
                &supplied,
                texture.width,
                texture.height,
                image::imageops::FilterType::Lanczos3,
            )
        };
        let allocation_end =
            texture.data_offsets[0].saturating_add(u64::from(texture.image_size)) as usize;
        let data_len = data.len();
        let mut writer = BinaryWriter::from_vec(data, BinaryEndian::Little);
        for mip in 0..u32::from(texture.mip_count) {
            let width = (texture.width >> mip).max(1);
            let height = (texture.height >> mip).max(1);
            let mut mip_image = if mip == 0 {
                resized.clone()
            } else {
                image::imageops::resize(
                    &resized,
                    width,
                    height,
                    image::imageops::FilterType::Triangle,
                )
            };
            super::switch_texture::invert_component_selectors(
                &mut mip_image,
                texture.channel_types,
            );
            let offset = texture.data_offsets[mip as usize] as usize;
            let end = texture
                .data_offsets
                .get(mip as usize + 1)
                .copied()
                .map(|value| value as usize)
                .unwrap_or(allocation_end)
                .min(data_len);
            let encoded = super::switch_texture::encode(
                &mip_image,
                encoding_format,
                texture.block_height_log2.saturating_sub(mip as u8),
                texture.tile_mode == 1,
            )?;
            if encoded.len() > end.saturating_sub(offset) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("encoded mip {mip} does not fit the original BNTX allocation"),
                ));
            }
            writer.seek(offset);
            writer.write_bytes(&vec![0; end.saturating_sub(offset)]);
            writer.seek(offset);
            writer.write_bytes(&encoded);
        }
        let data = writer.into_inner();

        let base_offset = texture.data_offsets[0] as usize;
        let base_end = texture
            .data_offsets
            .get(1)
            .copied()
            .map(|value| value as usize)
            .unwrap_or(allocation_end)
            .min(data.len());
        let mut decoded = super::switch_texture::decode(
            texture.width,
            texture.height,
            format,
            &data[base_offset..base_end],
            texture.block_height_log2,
            texture.tile_mode == 1,
        )?;
        super::switch_texture::apply_component_selectors(&mut decoded, texture.channel_types);
        let similarity = rgba_similarity(&resized, &decoded);
        if similarity < 0.99 {
            let source_mean = rgba_channel_means(&resized);
            let decoded_mean = rgba_channel_means(&decoded);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "BNTX round-trip similarity is {:.3}%, below the required 99% (format {format:?}, selectors {:?}, means {source_mean:?} -> {decoded_mean:?})",
                    similarity * 100.0,
                    texture.channel_types
                ),
            ));
        }

        let output = match dictionary {
            Some(dictionary) => zstd.compress_with_dictionary(&data, dictionary)?,
            None => data,
        };
        if let Some(parent) = destination.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(destination, output)?;
        Ok(BntxReplacementReport {
            name: new_name.to_owned(),
            width: texture.width,
            height: texture.height,
            format: format!("{format:?}"),
            similarity,
        })
    }
    pub fn render_bntx_bytes(data: &[u8], texture_index: usize) -> io::Result<RenderedImage> {
        let bntx = crate::parser::bntx::BntxFile::parse(data).map_err(invalid)?;
        let texture = bntx.textures.get(texture_index).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "BNTX texture index is out of range",
            )
        })?;
        let offset = *texture.data_offsets.first().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "BNTX texture has no image data")
        })? as usize;
        let end = texture
            .data_offsets
            .get(1)
            .copied()
            .map(|value| value as usize)
            .unwrap_or_else(|| offset.saturating_add(texture.image_size as usize))
            .min(data.len());
        let surface_data = data.get(offset..end).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "BNTX image offset is outside the file",
            )
        })?;
        let (mut image, format_name) = if let Some((block_width, block_height)) =
            super::switch_texture::astc_block_from_bntx(texture.format)
        {
            (
                super::switch_texture::decode_astc(
                    texture.width,
                    texture.height,
                    surface_data,
                    block_width,
                    block_height,
                    texture.block_height_log2,
                )?,
                format!("ASTC_{block_width}x{block_height}"),
            )
        } else {
            let image_format = super::switch_texture::format_from_bntx(texture.format)?;
            (
                super::switch_texture::decode(
                    texture.width,
                    texture.height,
                    image_format,
                    surface_data,
                    texture.block_height_log2,
                    texture.tile_mode == 1,
                )?,
                format!("{image_format:?}"),
            )
        };
        super::switch_texture::apply_component_selectors(&mut image, texture.channel_types);
        let png = super::png::encode(&image)?;
        Ok(RenderedImage {
            data_url: format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            ),
            width: texture.width,
            height: texture.height,
            format: "BNTX".into(),
            mip_count: u32::from(texture.mip_count),
            dds_type: Some(format_name),
            entries: Vec::new(),
            selected_index: texture_index,
            selected_array_index: 0,
            selected_mip_index: 0,
        })
    }

    pub fn open(
        path: &Path,
        zstd: &crate::Zstd::TotkZstd<'_>,
    ) -> Option<(
        crate::file_format::BinTextFile::OpenedFile<'static>,
        crate::Open_and_Save::SendData,
    )> {
        // #[cfg(not(debug_assertions))]
        // {
        //     return None;
        // }
        let data = std::fs::read(path).ok()?;
        let bntx_dictionary = decode_compressed_bntx(&data, Some(zstd))
            .ok()
            .and_then(|(_, dictionary)| dictionary);
        if !Self::supports(path, &data, Some(zstd)) {
            return None;
        }
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.path = crate::Settings::Pathlib::new(path);
        opened.file_type = if crate::Settings::Magic::is_bntx(&data) || bntx_dictionary.is_some() {
            crate::Zstd::TotkFileType::Bntx
        } else {
            crate::Zstd::TotkFileType::Other
        };

        let mut send = crate::Open_and_Save::SendData::default();
        send.path = crate::Settings::Pathlib::new(path);
        send.file_label = if opened.file_type == crate::Zstd::TotkFileType::Bntx {
            format!("{} [BNTX]", send.path.name)
        } else {
            format!("{} [IMAGE]", send.path.name)
        };
        send.set_file_metadata(opened.file_type, bntx_dictionary);
        send.status_text = format!("Opened image {}", path.display());
        send.tab = "IMAGE".into();
        send.read_only = true;
        Some((opened, send))
    }

    pub fn open_binary<P: AsRef<Path>>(
        bytes: &[u8],
        path: P,
        zstd: &crate::Zstd::TotkZstd<'_>,
    ) -> Option<(
        crate::file_format::BinTextFile::OpenedFile<'static>,
        crate::Open_and_Save::SendData,
    )> {
        let path_ref = path.as_ref();
        if !Self::supports(path_ref, bytes, Some(zstd)) {
            return None;
        };
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.path = crate::Settings::Pathlib::new(path_ref);
        let mut bntx_dictionary = None;
        let mut is_bntx = crate::Settings::Magic::is_bntx(bytes);
        if !is_bntx {
            if let Ok((decoded, dictionary)) = decode_compressed_bntx(bytes, Some(zstd)) {
                is_bntx = crate::Settings::Magic::is_bntx(&decoded);
                bntx_dictionary = dictionary;
            }
        } else if let Ok((_, dictionary)) = decode_compressed_bntx(bytes, Some(zstd)) {
            bntx_dictionary = dictionary;
        }
        opened.file_type = if is_bntx {
            crate::Zstd::TotkFileType::Bntx
        } else {
            crate::Zstd::TotkFileType::Other
        };
        let mut send = crate::Open_and_Save::SendData::default();
        send.path = crate::Settings::Pathlib::new(path_ref);
        send.file_label = if opened.file_type == crate::Zstd::TotkFileType::Bntx {
            format!("{} [BNTX]", send.path.name)
        } else {
            format!("{} [IMAGE]", send.path.name)
        };
        if opened.file_type == crate::Zstd::TotkFileType::Bntx {
            send.set_file_metadata(opened.file_type, bntx_dictionary);
        } else {
            send.file_type = opened.file_type;
        }
        send.status_text = format!("Opened image {}", path_ref.display());
        send.tab = "IMAGE".into();
        send.read_only = true;
        Some((opened, send))
    }

    pub fn render_path(path: impl AsRef<Path>) -> io::Result<RenderedImage> {
        Self::render_path_selection(path, 0, 0, 0)
    }

    pub fn render_path_selection(
        path: impl AsRef<Path>,
        texture_index: usize,
        array_index: u32,
        mip_index: u32,
    ) -> io::Result<RenderedImage> {
        Self::render_path_selection_with_zstd(path, texture_index, array_index, mip_index, None)
    }

    pub fn render_path_selection_with_zstd(
        path: impl AsRef<Path>,
        texture_index: usize,
        array_index: u32,
        mip_index: u32,
        zstd: Option<&crate::Zstd::TotkZstd<'_>>,
    ) -> io::Result<RenderedImage> {
        let path = path.as_ref();
        let source = std::fs::read(path)?;
        Self::render_bytes_selection_with_zstd(
            &source,
            path,
            texture_index,
            array_index,
            mip_index,
            zstd,
        )
    }

    pub fn render_bytes_selection_with_zstd(
        source: &[u8],
        path: impl AsRef<Path>,
        texture_index: usize,
        array_index: u32,
        mip_index: u32,
        zstd: Option<&crate::Zstd::TotkZstd<'_>>,
    ) -> io::Result<RenderedImage> {
        let path = path.as_ref();
        let data = maybe_zstd(&source, zstd)?;
        let mut entries = Vec::new();
        let (rgba, format, mip_count, dds_type) = if crate::Settings::Magic::is_g1t(&data) {
            let archive = crate::parser::AOC::g1t::G1tFile::parse(&data)?;
            entries = archive
                .textures
                .iter()
                .map(|texture| ImageEntry {
                    name: format!("Texture {}", texture.index),
                    width: texture.width,
                    height: texture.height,
                    // G1T previews intentionally expose only the base mip.
                    mip_count: 1,
                    array_count: texture.array_count,
                    format: format!("G1T 0x{:02X}", texture.texture_type),
                    subimages: subimages(texture.array_count, 1, texture.width, texture.height),
                })
                .collect();
            let texture = archive.textures.get(texture_index).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "G1T texture index is out of range",
                )
            })?;
            if mip_index != 0 || array_index >= texture.array_count {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "G1T preview exposes only the highest-resolution mip",
                ));
            }
            let encoded = texture
                .data_urls
                .get(array_index as usize)
                .unwrap_or(&texture.data_url)
                .split_once(',')
                .map(|(_, value)| value)
                .ok_or_else(|| invalid("invalid G1T image URL"))?;
            let png = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(invalid)?;
            (
                super::raster::decode(&png)?,
                "G1T".into(),
                1,
                Some(format!("0x{:02X}", texture.texture_type)),
            )
        } else if crate::Settings::Magic::is_bntx(&data) {
            let bntx = crate::parser::bntx::BntxFile::parse(&data).map_err(invalid)?;
            entries = bntx
                .textures
                .iter()
                .map(|texture| ImageEntry {
                    name: texture.name.clone(),
                    width: texture.width,
                    height: texture.height,
                    mip_count: u32::from(texture.mip_count),
                    array_count: texture.array_length,
                    format: format!("0x{:X}", texture.format),
                    subimages: subimages(
                        texture.array_length.max(1),
                        u32::from(texture.mip_count),
                        texture.width,
                        texture.height,
                    ),
                })
                .collect();
            let texture = bntx.textures.get(texture_index).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "BNTX texture index is out of range",
                )
            })?;
            let array_count = texture.array_length.max(1);
            if array_index >= array_count || mip_index >= u32::from(texture.mip_count) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "BNTX subimage index is out of range",
                ));
            }
            let surface_index =
                array_index as usize * usize::from(texture.mip_count) + mip_index as usize;
            let offset = *texture.data_offsets.get(surface_index).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "BNTX texture has no image data")
            })? as usize;
            let end = texture
                .data_offsets
                .get(surface_index + 1)
                .copied()
                .map(|value| value as usize)
                .unwrap_or_else(|| offset.saturating_add(texture.image_size as usize))
                .min(data.len());
            let surface_data = data.get(offset..end).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "BNTX image offset is outside the file",
                )
            })?;
            let width = (texture.width >> mip_index).max(1);
            let height = (texture.height >> mip_index).max(1);
            let block_height_log2 = texture.block_height_log2.saturating_sub(mip_index as u8);
            let (mut image, format_name) = if let Some((block_width, block_height)) =
                super::switch_texture::astc_block_from_bntx(texture.format)
            {
                (
                    super::switch_texture::decode_astc(
                        width,
                        height,
                        surface_data,
                        block_width,
                        block_height,
                        block_height_log2,
                    )?,
                    format!("ASTC_{block_width}x{block_height}"),
                )
            } else {
                let image_format = super::switch_texture::format_from_bntx(texture.format)?;
                (
                    super::switch_texture::decode(
                        width,
                        height,
                        image_format,
                        surface_data,
                        block_height_log2,
                        texture.tile_mode == 1,
                    )?,
                    format!("{image_format:?}"),
                )
            };
            super::switch_texture::apply_component_selectors(&mut image, texture.channel_types);
            (
                image,
                "BNTX".into(),
                u32::from(texture.mip_count),
                Some(format_name),
            )
        } else if data.get(4..8) == Some(b"6PK0") {
            let txtg = crate::parser::textogo::TexToGoFile::parse(&data).map_err(invalid)?;
            let image_format = super::switch_texture::format_from_textogo(txtg.header.format);
            entries.push(ImageEntry {
                name: path
                    .file_stem()
                    .and_then(|x| x.to_str())
                    .unwrap_or("TexToGo")
                    .into(),
                width: u32::from(txtg.header.width),
                height: u32::from(txtg.header.height),
                mip_count: u32::from(txtg.header.mip_count),
                array_count: u32::from(txtg.header.depth),
                format: format!("0x{:X}", txtg.header.format),
                subimages: subimages(
                    u32::from(txtg.header.depth).max(1),
                    u32::from(txtg.header.mip_count),
                    u32::from(txtg.header.width),
                    u32::from(txtg.header.height),
                ),
            });
            let surface = txtg
                .surfaces
                .iter()
                .find(|surface| {
                    u32::from(surface.array_level) == array_index
                        && u32::from(surface.mip_level) == mip_index
                })
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "TexToGo has no base surface")
                })?;
            let selected_width = (u32::from(txtg.header.width) >> mip_index).max(1);
            let selected_height = (u32::from(txtg.header.height) >> mip_index).max(1);
            let log2 = super::switch_texture::inferred_block_height_log2(selected_height, 4);
            let (image, format_name) = match txtg.header.format {
                0x101 | 0x109 => (
                    super::switch_texture::decode_astc(
                        selected_width,
                        selected_height,
                        &surface.data,
                        4,
                        4,
                        log2,
                    )?,
                    "ASTC 4x4".to_owned(),
                ),
                0x102 | 0x105 => (
                    super::switch_texture::decode_astc(
                        selected_width,
                        selected_height,
                        &surface.data,
                        8,
                        8,
                        log2,
                    )?,
                    "ASTC 8x8".to_owned(),
                ),
                _ => {
                    let image_format = image_format?;
                    (
                        super::switch_texture::decode(
                            selected_width,
                            selected_height,
                            image_format,
                            &surface.data,
                            log2,
                            false,
                        )?,
                        format!("{image_format:?}"),
                    )
                }
            };
            (
                image,
                "TexToGo".into(),
                u32::from(txtg.header.mip_count),
                Some(format_name),
            )
        } else if crate::Settings::Magic::is_dds(&data) {
            let header = super::dds::DdsHeader::parse(&data)?;
            let dds = image_dds::ddsfile::Dds::read(&mut std::io::Cursor::new(&data))
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
            let dds_type = image_dds::dds_image_format(&dds)
                .map(|value| format!("{value:?}"))
                .unwrap_or_else(|_| "Unknown".into());
            let surface = image_dds::Surface::from_dds(&dds)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
            if array_index >= surface.layers || mip_index >= surface.mipmaps {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "DDS subimage index is out of range",
                ));
            }
            entries.push(ImageEntry {
                name: path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("DDS")
                    .into(),
                width: surface.width,
                height: surface.height,
                mip_count: surface.mipmaps,
                array_count: surface.layers,
                format: dds_type.clone(),
                subimages: subimages(
                    surface.layers,
                    surface.mipmaps,
                    surface.width,
                    surface.height,
                ),
            });
            let selected = image_dds::SurfaceRgba8::decode_layers_mipmaps_dds(
                &dds,
                array_index..array_index + 1,
                mip_index..mip_index + 1,
            )
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
            (
                selected.get_image(0, 0, 0).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "DDS subimage could not be decoded",
                    )
                })?,
                "DDS".to_string(),
                header.mipmap_count,
                Some(dds_type),
            )
        } else {
            let image = super::raster::decode(&data)?;
            let format = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("image")
                .to_ascii_uppercase();
            (image, format, 1, None)
        };
        let width = rgba.width();
        let height = rgba.height();
        let png = super::png::encode(&rgba)?;
        Ok(RenderedImage {
            data_url: format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            ),
            width,
            height,
            format,
            mip_count,
            dds_type,
            entries,
            selected_index: texture_index,
            selected_array_index: array_index,
            selected_mip_index: mip_index,
        })
    }

    pub fn replace_dds(
        target: impl AsRef<Path>,
        png: impl AsRef<Path>,
        dds_type: &str,
        mip_count: u32,
    ) -> io::Result<()> {
        super::dds::replace_from_png(target, png, dds_type, mip_count)
    }

    pub fn replace_dds_surface(
        target: impl AsRef<Path>,
        png: impl AsRef<Path>,
        array_index: u32,
        mip_index: u32,
        replacement_format: Option<&str>,
    ) -> io::Result<()> {
        super::dds::replace_surface_from_png(
            target,
            png,
            array_index,
            mip_index,
            replacement_format,
        )
    }

    pub fn replace_g1t_surface(
        target: impl AsRef<Path>,
        png: impl AsRef<Path>,
        texture_index: usize,
        array_index: u32,
        mip_index: u32,
    ) -> io::Result<()> {
        use crate::parser::AOC::g1t;
        let target = target.as_ref();
        let replacement = image::open(png)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?
            .to_rgba8();
        let mut data = std::fs::read(target)?;
        let surfaces = g1t::G1tFile::parse_surfaces(&data)?;
        let texture = surfaces.get(texture_index).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "G1T texture index is out of range",
            )
        })?;
        if mip_index != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "G1T replacement currently supports only mip 0",
            ));
        }
        if array_index >= texture.array_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "G1T array index is out of range",
            ));
        }
        let dds_format = g1t::image_dds_format_for_type(texture.texture_type)?;
        if replacement.dimensions() != (texture.width, texture.height) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "replacement must be {} x {} pixels",
                    texture.width, texture.height
                ),
            ));
        }
        let max_mips = 32 - texture.width.max(texture.height).leading_zeros() as u8;
        let mut mip_count = texture.mip_count.min(max_mips);
        let mut writable_bytes = 0usize;
        while mip_count > 0 {
            let expected = g1t::texture_chain_size(
                texture.texture_type,
                texture.width,
                texture.height,
                mip_count,
            )?;
            if expected <= texture.layer_stride && expected > 0 {
                writable_bytes = expected;
                break;
            }
            mip_count = mip_count.saturating_sub(1);
        }
        if mip_count == 0 || writable_bytes == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "G1T texture payload is too small for replacement",
            ));
        }
        let layer_start = texture.data_offset + (array_index as usize) * texture.layer_stride;
        let layer_end = layer_start
            .checked_add(texture.layer_stride)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "G1T replacement range overflow",
                )
            })?;
        if layer_end > data.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "G1T texture layer exceeds file size",
            ));
        }
        let mut cursor = layer_start;
        let writable_end = layer_start.checked_add(writable_bytes).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "G1T replacement range overflow",
            )
        })?;
        for mip in 0..mip_count {
            let mip_width = (texture.width >> mip).max(1);
            let mip_height = (texture.height >> mip).max(1);
            let expected =
                g1t::texture_mip_size(texture.texture_type, texture.width, texture.height, mip)?;
            let mip_image = if mip == 0 {
                replacement.clone()
            } else {
                image::imageops::resize(
                    &replacement,
                    mip_width,
                    mip_height,
                    image::imageops::FilterType::Triangle,
                )
            };
            let encoded =
                crate::file_format::Image::switch_texture::encode(&mip_image, dds_format, 0, true)?;
            if encoded.len() > expected {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("encoded G1T mip {mip} does not fit existing data slot"),
                ));
            }
            if cursor + expected > writable_end {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "G1T texture payload is too small for replacement",
                ));
            }
            data[cursor..cursor + expected].fill(0);
            data[cursor..cursor + encoded.len()].copy_from_slice(&encoded);
            cursor += expected;
        }
        std::fs::write(target, data)
    }

    pub fn rename_bntx_texture(
        path: impl AsRef<Path>,
        texture_index: usize,
        new_name: &str,
        zstd: &crate::Zstd::TotkZstd<'_>,
    ) -> io::Result<()> {
        if new_name.is_empty() || new_name.as_bytes().len() > u16::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid BNTX texture name",
            ));
        }
        let path = path.as_ref();
        let source = std::fs::read(path)?;
        let (mut data, dictionary) = decode_compressed_bntx(&source, Some(zstd))?;
        let bntx = crate::parser::bntx::BntxFile::parse(&data).map_err(invalid)?;
        let texture = bntx.textures.get(texture_index).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "BNTX texture index is out of range",
            )
        })?;
        if new_name.as_bytes().len() > texture.name_capacity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "name is too long for this BNTX string slot (maximum {} UTF-8 bytes)",
                    texture.name_capacity
                ),
            ));
        }
        let length = u16::try_from(new_name.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "BNTX name is too long"))?
            .to_le_bytes();
        data[texture.name_offset..texture.name_offset + 2].copy_from_slice(&length);
        let start = texture.name_offset + 2;
        data[start..start + texture.name_capacity].fill(0);
        data[start..start + new_name.as_bytes().len()].copy_from_slice(new_name.as_bytes());
        let output = match dictionary {
            Some(dictionary) => zstd.compress_with_dictionary(&data, dictionary)?,
            None => data,
        };
        std::fs::write(path, output)
    }

    pub fn replace_bntx_surface(
        path: impl AsRef<Path>,
        png: impl AsRef<Path>,
        texture_index: usize,
        array_index: u32,
        mip_index: u32,
        replacement_format: Option<&str>,
        zstd: &crate::Zstd::TotkZstd<'_>,
    ) -> io::Result<()> {
        let path = path.as_ref();
        let source = std::fs::read(path)?;
        let (mut data, dictionary) = decode_compressed_bntx(&source, Some(zstd))?;
        let bntx = crate::parser::bntx::BntxFile::parse(&data).map_err(invalid)?;
        let texture = bntx.textures.get(texture_index).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "BNTX texture index is out of range",
            )
        })?;
        let custom_format = replacement_format.filter(|value| *value != "ORIGINAL");
        if custom_format.is_some() && (texture.array_length.max(1) != 1 || mip_index != 0) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "custom BNTX format generation requires selecting mip 0 of a non-array texture",
            ));
        }
        if super::switch_texture::astc_block_from_bntx(texture.format).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "ASTC BNTX replacement is not supported",
            ));
        }
        let surface_index =
            array_index as usize * usize::from(texture.mip_count) + mip_index as usize;
        let offset = *texture.data_offsets.get(surface_index).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "BNTX subimage index is out of range",
            )
        })? as usize;
        let end = texture
            .data_offsets
            .get(surface_index + 1)
            .copied()
            .map(|value| value as usize)
            .unwrap_or_else(|| offset.saturating_add(texture.image_size as usize))
            .min(data.len());
        let width = (texture.width >> mip_index).max(1);
        let height = (texture.height >> mip_index).max(1);
        let image = image::open(png)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?
            .to_rgba8();
        if image.dimensions() != (width, height) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("replacement must be {width} x {height} pixels"),
            ));
        }
        if let Some(name) = custom_format {
            let (format, bntx_format) = super::switch_texture::format_for_bntx_name(name)?;
            let allocation_end =
                texture.data_offsets[0].saturating_add(u64::from(texture.image_size)) as usize;
            for mip in 0..u32::from(texture.mip_count) {
                let mip_width = (texture.width >> mip).max(1);
                let mip_height = (texture.height >> mip).max(1);
                let mip_image = if mip == 0 {
                    image.clone()
                } else {
                    image::imageops::resize(
                        &image,
                        mip_width,
                        mip_height,
                        image::imageops::FilterType::Triangle,
                    )
                };
                let mip_offset = texture.data_offsets[mip as usize] as usize;
                let mip_end = texture
                    .data_offsets
                    .get(mip as usize + 1)
                    .copied()
                    .map(|value| value as usize)
                    .unwrap_or(allocation_end)
                    .min(data.len());
                let encoded = super::switch_texture::encode(
                    &mip_image,
                    format,
                    texture.block_height_log2.saturating_sub(mip as u8),
                    texture.tile_mode == 1,
                )?;
                if encoded.len() > mip_end.saturating_sub(mip_offset) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{name} mip {mip} does not fit the BNTX data slot"),
                    ));
                }
                data[mip_offset..mip_end].fill(0);
                data[mip_offset..mip_offset + encoded.len()].copy_from_slice(&encoded);
            }
            data[texture.format_offset..texture.format_offset + 4]
                .copy_from_slice(&bntx_format.to_le_bytes());
            let output = match dictionary {
                Some(dictionary) => zstd.compress_with_dictionary(&data, dictionary)?,
                None => data,
            };
            return std::fs::write(path, output);
        }
        let format = super::switch_texture::format_from_bntx(texture.format)?;
        let encoded = super::switch_texture::encode(
            &image,
            format,
            texture.block_height_log2.saturating_sub(mip_index as u8),
            texture.tile_mode == 1,
        )?;
        if encoded.len() > end.saturating_sub(offset) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "encoded BNTX surface does not fit its existing data slot",
            ));
        }
        data[offset..end].fill(0);
        data[offset..offset + encoded.len()].copy_from_slice(&encoded);
        let output = match dictionary {
            Some(dictionary) => zstd.compress_with_dictionary(&data, dictionary)?,
            None => data,
        };
        std::fs::write(path, output)
    }

    pub fn export_png(source: impl AsRef<Path>, output: impl AsRef<Path>) -> io::Result<()> {
        let rendered = Self::render_path(source)?;
        Self::export_rendered_png(&rendered, output)
    }

    pub fn export_rendered_png(
        rendered: &RenderedImage,
        output: impl AsRef<Path>,
    ) -> io::Result<()> {
        let encoded = rendered
            .data_url
            .split_once(',')
            .map(|(_, value)| value)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid rendered image URL")
            })?;
        let png = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        std::fs::write(output, png)
    }

    pub(crate) fn supports(
        path: &Path,
        data: &[u8],
        zstd: Option<&crate::Zstd::TotkZstd<'_>>,
    ) -> bool {
        if crate::Settings::Magic::is_dds(data)
            || crate::Settings::Magic::is_bntx(data)
            || crate::Settings::Magic::is_g1t(data)
            || data.get(4..8) == Some(b"6PK0")
            || crate::Settings::Magic::is_png(data)
        {
            return true;
        }
        if crate::Settings::Magic::is_zstd(data) {
            if let Ok(decoded) = maybe_zstd(data, zstd) {
                return crate::Settings::Magic::is_bntx(&decoded);
            }
        }
        matches!(
            path.extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some(
                "bmp"
                    | "gif"
                    | "ico"
                    | "jpg"
                    | "jpeg"
                    | "png"
                    | "pnm"
                    | "qoi"
                    | "tga"
                    | "tif"
                    | "tiff"
                    | "webp"
                    | "dds"
                    | "bntx"
                    | "txtg"
                    | "g1t"
            )
        )
    }
}

fn subimages(array_count: u32, mip_count: u32, width: u32, height: u32) -> Vec<ImageSubimage> {
    (0..array_count)
        .flat_map(|array_index| {
            (0..mip_count).map(move |mip_index| ImageSubimage {
                array_index,
                mip_index,
                width: (width >> mip_index).max(1),
                height: (height >> mip_index).max(1),
                name: format!("Layer {array_index} / Mip {mip_index}"),
            })
        })
        .collect()
}

fn maybe_zstd(data: &[u8], zstd: Option<&crate::Zstd::TotkZstd<'_>>) -> io::Result<Vec<u8>> {
    if crate::Settings::Magic::is_zstd(data) {
        if let Some(zstd) = zstd {
            decode_compressed_bntx(data, Some(zstd)).map(|(decoded, _)| decoded)
        } else {
            crate::Zstd::TotkZstd::decompress_empty(data)
        }
    } else {
        Ok(data.to_vec())
    }
}

fn decode_compressed_bntx(
    data: &[u8],
    zstd: Option<&crate::Zstd::TotkZstd<'_>>,
) -> io::Result<(Vec<u8>, Option<crate::Zstd::ZstdDictionary>)> {
    if crate::Settings::Magic::is_bntx(data) {
        return Ok((data.to_vec(), None));
    }
    if !crate::Settings::Magic::is_zstd(data) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "data is neither BNTX nor Zstandard",
        ));
    }
    let Some(zstd) = zstd else {
        let decoded = crate::Zstd::TotkZstd::decompress_empty(data)?;
        return if crate::Settings::Magic::is_bntx(&decoded) {
            Ok((decoded, None))
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Zstandard payload is not BNTX",
            ))
        };
    };
    for dictionary in [
        crate::Zstd::ZstdDictionary::Zs,
        crate::Zstd::ZstdDictionary::Pack,
        crate::Zstd::ZstdDictionary::Empty,
        crate::Zstd::ZstdDictionary::Bcett,
    ] {
        if let Ok(decoded) = zstd.try_decompress_using(data, dictionary) {
            if crate::Settings::Magic::is_bntx(&decoded) {
                return Ok((decoded, Some(dictionary)));
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "no configured Zstandard dictionary produced a BNTX payload",
    ))
}

fn rename_single_bntx_texture_bytes(
    data: Vec<u8>,
    texture: &crate::parser::bntx::BntxTexture,
    new_name: &str,
) -> io::Result<Vec<u8>> {
    let bytes = new_name.as_bytes();
    let reader = BinaryReader::new(&data);
    let mut writer = BinaryWriter::from_vec(data.clone(), BinaryEndian::Little);
    if bytes.len() <= texture.name_capacity {
        let length = u16::try_from(bytes.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "BNTX name is too long"))?;
        writer.seek(texture.name_offset);
        writer.write_u16(length);
        let start = texture.name_offset + 2;
        writer.seek(start);
        writer.write_bytes(&vec![0; texture.name_capacity]);
        writer.seek(start);
        writer.write_bytes(bytes);
        return Ok(writer.into_inner());
    }

    let relocation = reader.read_u32_at(0x18)? as usize;
    if reader.read_bytes_at(relocation, 4)? != b"_RLT" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "BNTX relocation table is missing",
        ));
    }
    let required = 2usize
        .checked_add(bytes.len())
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "BNTX name is too long"))?;
    let zero_start = reader
        .read_bytes_at(0, relocation)?
        .iter()
        .rposition(|byte| *byte != 0)
        .map(|position| position + 1)
        .unwrap_or(0);
    if relocation.saturating_sub(zero_start) < required {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "BNTX has insufficient string-pool padding for the longer name",
        ));
    }
    let new_offset = relocation - required;
    writer.seek(new_offset);
    writer.write_u16(bytes.len() as u16);
    writer.write_bytes(bytes);
    writer.write_u8(0);

    let pointer_fields = bntx_relocation_pointer_fields(&reader, relocation)?;
    let mut updated = 0;
    for field in pointer_fields {
        let target = reader.read_u64_at(field)? as usize;
        if target == texture.name_offset {
            writer.seek(field);
            writer.write_u64(new_offset as u64);
            updated += 1;
        }
    }
    if updated == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "BNTX name has no relocation-tracked pointer",
        ));
    }
    Ok(writer.into_inner())
}

fn bntx_relocation_pointer_fields(
    reader: &BinaryReader<'_>,
    relocation: usize,
) -> io::Result<Vec<usize>> {
    let section_count = reader.read_u32_at(relocation + 8)? as usize;
    if section_count == 0 || section_count > 64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid BNTX relocation section count",
        ));
    }
    let sections = relocation + 16;
    let entries = sections
        .checked_add(section_count * 24)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "BNTX RLT overflow"))?;
    let mut total_entries = 0usize;
    for index in 0..section_count {
        let section = sections + index * 24;
        let entry_index = reader.read_u32_at(section + 16)? as usize;
        let entry_count = reader.read_u32_at(section + 20)? as usize;
        total_entries = total_entries.max(entry_index.saturating_add(entry_count));
    }
    if total_entries > reader.len().saturating_sub(entries) / 8 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "truncated BNTX relocation entries",
        ));
    }
    let mut fields = Vec::new();
    for index in 0..total_entries {
        let entry = entries + index * 8;
        let position = reader.read_u32_at(entry)? as usize;
        let structures = reader.read_u16_at(entry + 4)? as usize;
        let offsets = reader.read_u8_at(entry + 6)? as usize;
        let padding = reader.read_u8_at(entry + 7)? as usize;
        let stride = (offsets + padding) * 8;
        for structure in 0..structures {
            for pointer in 0..offsets {
                let field = position + structure * stride + pointer * 8;
                if field + 8 > relocation {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "BNTX relocated pointer is outside the data section",
                    ));
                }
                fields.push(field);
            }
        }
    }
    fields.sort_unstable();
    fields.dedup();
    Ok(fields)
}

fn rgba_similarity(left: &image::RgbaImage, right: &image::RgbaImage) -> f64 {
    if left.dimensions() != right.dimensions() || left.as_raw().is_empty() {
        return 0.0;
    }
    let error: u64 = left
        .pixels()
        .zip(right.pixels())
        .map(|(left, right)| {
            let left_alpha = u32::from(left[3]);
            let right_alpha = u32::from(right[3]);
            let rgb_error: u32 = (0..3)
                .map(|channel| {
                    let left = u32::from(left[channel]) * left_alpha / 255;
                    let right = u32::from(right[channel]) * right_alpha / 255;
                    left.abs_diff(right)
                })
                .sum();
            u64::from(rgb_error + left_alpha.abs_diff(right_alpha))
        })
        .sum();
    1.0 - error as f64 / (left.width() as f64 * left.height() as f64 * 4.0 * 255.0)
}

fn rgba_channel_means(image: &image::RgbaImage) -> [u8; 4] {
    let mut sums = [0u64; 4];
    for pixel in image.pixels() {
        for channel in 0..4 {
            sums[channel] += u64::from(pixel[channel]);
        }
    }
    let count = u64::from(image.width()) * u64::from(image.height());
    sums.map(|sum| (sum / count.max(1)) as u8)
}

fn invalid(error: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};
    use base64::Engine;
    use std::{fs, sync::Arc};

    fn similarity_coverage(original: &image::RgbaImage, candidate: &image::RgbaImage) -> f64 {
        if original.dimensions() != candidate.dimensions() {
            return 0.0;
        }
        let denominator = (u64::from(original.width()) * u64::from(original.height()) * 4) as f64;
        if denominator == 0.0 {
            return 1.0;
        }
        let total_delta: u64 = original
            .pixels()
            .zip(candidate.pixels())
            .flat_map(|(left, right)| {
                left.0
                    .iter()
                    .zip(right.0.iter())
                    .map(|(a, b)| u64::from(a.abs_diff(*b)))
            })
            .sum();
        let mean_delta = total_delta as f64 / (denominator * 255.0);
        let similarity = 1.0 - mean_delta;
        if similarity < 0.0 {
            0.0
        } else if similarity > 1.0 {
            1.0
        } else {
            similarity
        }
    }

    #[test]
    fn compressed_non_image_is_not_accepted_by_extension() {
        assert!(!ImageDocument::supports(
            Path::new("Event/Test.bfevfl.zs"),
            b"not an image",
            None,
        ));
    }

    #[test]
    fn g1t_preview_uses_only_the_highest_resolution_mip() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/g1m/00648a45.g1t");
        if !path.is_file() {
            return;
        }
        let rendered = ImageDocument::render_path(&path).expect("G1T preview did not render");
        assert_eq!(rendered.format, "G1T");
        assert_eq!(rendered.mip_count, 1);
        assert!(!rendered.entries.is_empty());
        assert!(rendered.entries.iter().all(|entry| entry.mip_count == 1));
        assert!(rendered
            .entries
            .iter()
            .flat_map(|entry| &entry.subimages)
            .all(|surface| surface.mip_index == 0));
    }

    #[test]
    #[ignore = "requires the Weapon Restoration BNTX fixture and ROMFS dictionaries"]
    fn single_bntx_png_round_trip_preserves_format_and_similarity() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/BotW Weapon Restoration/romfs/UI/Tex/Icon/Weapon_Lsword_005.bntx.zs");
        if !source.is_file() {
            return;
        }
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            crate::Zstd::TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        let rendered =
            ImageDocument::render_path_selection_with_zstd(&source, 0, 0, 0, Some(zstd.as_ref()))
                .unwrap();
        let png = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bntx_roundtrip_input.png");
        let output =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bntx_roundtrip_output.bntx.zs");
        let encoded = rendered.data_url.split_once(',').unwrap().1;
        fs::write(
            &png,
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .unwrap(),
        )
        .unwrap();
        let report = ImageDocument::replace_single_bntx_from_png(
            &source,
            &output,
            &png,
            "Weapon_Lsword_900",
            zstd.as_ref(),
        )
        .unwrap();
        assert!(report.similarity >= 0.99);
        assert_eq!(report.format, rendered.dds_type.unwrap());
        let (raw, _) =
            decode_compressed_bntx(&fs::read(&output).unwrap(), Some(zstd.as_ref())).unwrap();
        let parsed = crate::parser::bntx::BntxFile::parse(&raw).unwrap();
        assert_eq!(parsed.textures.len(), 1);
        assert_eq!(parsed.textures[0].name, "Weapon_Lsword_900");
        fs::remove_file(png).unwrap();
        fs::remove_file(output).unwrap();
    }

    #[test]
    #[ignore = "requires the supplied custom PNG, restoration BNTX, and ROMFS dictionaries"]
    fn supplied_weapon_png_round_trips_through_bntx() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let source = fixture_root
            .join("BotW Weapon Restoration/romfs/UI/Tex/Icon/Weapon_Lsword_005.bntx.zs");
        let png = fixture_root.join("BotW Weapon Restoration/Weapon_Lsword_002.png");
        if !source.is_file() || !png.is_file() {
            return;
        }
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            crate::Zstd::TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        let output = fixture_root.join("Weapon_Lsword_902.test.bntx.zs");
        let exported = fixture_root.join("Weapon_Lsword_902.test.png");
        let report = ImageDocument::replace_single_bntx_from_png(
            &source,
            &output,
            &png,
            "Weapon_Lsword_902",
            zstd.as_ref(),
        )
        .unwrap();
        println!(
            "supplied weapon PNG: format={}, size={}x{}, similarity={:.4}%",
            report.format,
            report.width,
            report.height,
            report.similarity * 100.0
        );
        assert!(
            report.similarity >= 0.99,
            "similarity was {}",
            report.similarity
        );

        let rendered =
            ImageDocument::render_path_selection_with_zstd(&output, 0, 0, 0, Some(zstd.as_ref()))
                .unwrap();
        let encoded = rendered.data_url.split_once(',').unwrap().1;
        fs::write(
            &exported,
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .unwrap(),
        )
        .unwrap();
        let input_image = image::open(&png).unwrap();
        assert_eq!(
            (input_image.width(), input_image.height()),
            (report.width, report.height)
        );
        let exported_image = image::open(&exported).unwrap();
        assert_eq!(
            (exported_image.width(), exported_image.height()),
            (report.width, report.height)
        );
        let (raw, _) =
            decode_compressed_bntx(&fs::read(&output).unwrap(), Some(zstd.as_ref())).unwrap();
        let parsed = crate::parser::bntx::BntxFile::parse(&raw).unwrap();
        assert_eq!(parsed.textures[0].name, "Weapon_Lsword_902");
        fs::remove_file(output).unwrap();
        fs::remove_file(exported).unwrap();
    }

    #[test]
    fn replace_g1t_png_round_trip_with_dataset_textures() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/impa_uchiha_all");
        let data_dir = root.join("data");
        let png_root = root.as_path();
        if !data_dir.is_dir() || !png_root.is_dir() {
            return;
        }
        let mut png_paths = Vec::new();
        if let Ok(entries) = fs::read_dir(png_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) == Some("png") {
                    png_paths.push(path);
                }
            }
        }
        if png_paths.is_empty() {
            return;
        }
        let work_dir = root.join("tmp_g1t_roundtrip");
        fs::create_dir_all(&work_dir).unwrap();
        let mut tested = 0usize;
        let mut g1t_paths = Vec::new();
        if let Ok(entries) = fs::read_dir(&data_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) == Some("g1t") {
                    g1t_paths.push(path);
                }
            }
        }
        for (index, path) in g1t_paths.into_iter().take(6).enumerate() {
            let source = fs::read(&path).unwrap_or_default();
            let surfaces = match crate::parser::AOC::g1t::G1tFile::parse_surfaces(&source) {
                Ok(value) if !value.is_empty() => value,
                _ => continue,
            };
            let Some(texture) = surfaces
                .iter()
                .find(|value| {
                    crate::parser::AOC::g1t::image_dds_format_for_type(value.texture_type).is_ok()
                })
                .cloned()
            else {
                continue;
            };
            let png_source = &png_paths[index % png_paths.len()];
            let replacement = image::open(png_source)
                .unwrap_or_else(|error| panic!("{}: {error}", png_source.display()))
                .to_rgba8();
            let replacement = if replacement.dimensions() != (texture.width, texture.height) {
                image::imageops::resize(
                    &replacement,
                    texture.width,
                    texture.height,
                    image::imageops::FilterType::Triangle,
                )
            } else {
                replacement
            };
            let replacement_path = work_dir.join("replace.png");
            std::fs::write(
                &replacement_path,
                crate::file_format::Image::png::encode(&replacement).unwrap(),
            )
            .unwrap();
            let output = work_dir.join(path.file_name().unwrap());
            std::fs::copy(&path, &output).unwrap();
            ImageDocument::replace_g1t_surface(&output, &replacement_path, texture.index, 0, 0)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let rendered =
                ImageDocument::render_path_selection_with_zstd(&output, texture.index, 0, 0, None)
                    .unwrap_or_else(|error| panic!("render {}: {error}", output.display()));
            let encoded = rendered
                .data_url
                .split_once(',')
                .map(|(_, value)| value)
                .unwrap_or_else(|| panic!("invalid data url from {}", path.display()));
            let exported = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .unwrap();
            let exported = crate::file_format::Image::raster::decode(&exported).unwrap();
            assert_eq!(
                (texture.width, texture.height),
                (exported.width(), exported.height()),
                "{} dimensions changed",
                path.display()
            );
            assert!(
                similarity_coverage(&replacement, &exported) >= 0.99,
                "{} similarity too low",
                path.display()
            );
            tested += 1;
        }
        assert!(tested > 0, "no G1T textures could be tested");
        let _ = fs::remove_file(work_dir.join("replace.png"));
    }
}
