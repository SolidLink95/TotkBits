//! BNTX texture containers.
//!
//! One parser serves both the image viewer (raw offsets for in-place
//! patching, see `file_format::Image::document`) and the Switch Toolbox
//! compatible editor: the loader mirrors Syroot's `BntxFileLoader` (the
//! KillzXGaming fork Toolbox ships), the saver is a port of `BntxFileSaver`
//! that reproduces Toolbox's byte layout, and the importer follows
//! `TextureImporterSettings.FromBitMap` + `SwizzleSurfaceMipMaps` (ASTC
//! blocks come from ARM's astcenc, exactly like the C# side; BC formats are
//! encoded natively and are the one part that is not byte-identical).

use super::BntxError;
use crate::file_format::Image::switch_texture;
use crate::file_format::Model3D::bfres::toolbox::patricia;
use crate::parser::binary::{BinaryPatcher, BinaryReader, BinaryWriter};
use image::RgbaImage;
use image_dds::ImageFormat;
use serde::Serialize;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Command;
use tegra_swizzle::surface::BlockDim;
use tegra_swizzle::BlockHeight;

type Result<T> = std::result::Result<T, BntxError>;

fn err<T>(message: impl Into<String>) -> Result<T> {
    Err(BntxError::new(0, message))
}

/// `GFX.ChannelType` values.
pub const CHANNEL_ONE: u8 = 1;
pub const CHANNEL_RED: u8 = 2;
pub const CHANNEL_GREEN: u8 = 3;
pub const CHANNEL_BLUE: u8 = 4;
pub const CHANNEL_ALPHA: u8 = 5;

/// `GFX.TileMode.LinearAligned`.
pub const TILE_MODE_LINEAR_ALIGNED: u16 = 1;

/// One `BRTI` block: the viewer-facing fields (serialized to the frontend)
/// plus what Syroot's `Texture` keeps for saving (skipped in JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BntxTexture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_length: u32,
    pub mip_count: u16,
    pub format: u32,
    pub image_size: u32,
    pub alignment: u32,
    pub tile_mode: u16,
    pub swizzle: u16,
    pub block_height_log2: u8,
    /// Red, green, blue, alpha `GFX.ChannelType` sources.
    pub channel_types: [u8; 4],
    /// Absolute offset of every (slice, mip) surface in the parsed file.
    pub data_offsets: Vec<u64>,
    /// Position of the name's length prefix in the parsed file.
    pub name_offset: usize,
    pub name_capacity: usize,
    pub format_offset: usize,
    #[serde(skip)]
    pub dim: u8,
    #[serde(skip)]
    pub sample_count: u32,
    #[serde(skip)]
    pub access_flags: u32,
    #[serde(skip)]
    pub texture_layout: u32,
    #[serde(skip)]
    pub texture_layout2: u32,
    #[serde(skip)]
    pub surface_dim: u32,
    /// Mip offsets relative to the first one.
    #[serde(skip)]
    pub mip_offsets: Vec<i64>,
    /// `TextureData[slice][0]`: the complete (all mips) swizzled data of each
    /// array slice. Syroot only ever writes index 0 of each slice back.
    #[serde(skip)]
    pub texture_data: Vec<Vec<u8>>,
    #[serde(skip)]
    pub read_texture_layout: i32,
    #[serde(skip)]
    pub sparse_binding: i32,
    #[serde(skip)]
    pub sparse_residency: i32,
}

impl BntxTexture {
    /// `Texture.GetTotalSize()`.
    pub fn total_size(&self) -> usize {
        self.texture_data.iter().map(Vec::len).sum()
    }

    pub fn is_astc(&self) -> bool {
        switch_texture::astc_block_from_bntx(self.format).is_some()
    }

    /// Human readable one-liner used by the CLI log.
    pub fn describe(&self) -> String {
        format!(
            "{} {}x{} {} mips={} array={}",
            self.name,
            self.width,
            self.height,
            format_name(self.format),
            self.mip_count,
            self.array_length
        )
    }
}

/// Snapshot Syroot takes of every texture on load (`OriginalImageData`); the
/// saver reuses the vanilla relocation table only while all of it still holds.
#[derive(Clone, Debug, PartialEq, Eq)]
struct OriginalImageData {
    name: String,
    mip_count: u32,
    image_size: u32,
    user_data_count: u32,
}

/// A BNTX file as Syroot's loader leaves it in memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BntxFile {
    pub version: [u8; 4],
    pub target: String,
    pub file_size: u32,
    pub textures: Vec<BntxTexture>,
    /// Name stored in the header (`--swap_int_name`).
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    original_file_name: String,
    #[serde(skip)]
    pub byte_order: u16,
    #[serde(skip)]
    pub alignment: u8,
    #[serde(skip)]
    pub target_address_size: u8,
    #[serde(skip)]
    pub flag: u16,
    #[serde(skip)]
    target_bytes: [u8; 4],
    /// Strings referenced while loading, keyed by their file offset (the
    /// position of the length prefix), i.e. `StringTable.Strings`.
    #[serde(skip)]
    string_table: BTreeMap<u64, String>,
    /// Everything from the original `_RLT` block to the end of the file.
    #[serde(skip)]
    original_rlt_chunk: Vec<u8>,
    #[serde(skip)]
    original_file_data: Vec<OriginalImageData>,
}

impl BntxFile {
    /// Parses a little-endian BNTX file (already decompressed): every field
    /// the image viewer needs plus the Toolbox-side state the saver reuses.
    pub fn parse(data: &[u8]) -> Result<Self> {
        load(data)
    }

    /// Serializes the file exactly like Switch Toolbox does (raw).
    pub fn save_like_toolbox(&self) -> Result<Vec<u8>> {
        save(self)
    }

    /// `save_like_toolbox` followed by Toolbox's `Zstb` compression
    /// (libzstd 1.3.3, level 19, no dictionary).
    pub fn save_like_toolbox_zs(&self) -> Result<Vec<u8>> {
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
    ) -> Result<Option<String>> {
        let image = image::open(picture)
            .map_err(|error| BntxError::new(0, format!("{}: {error}", picture.display())))?
            .to_rgba8();
        self.replace_texture_from_rgba(index, &image, encoder)
    }

    /// [`Self::replace_texture_from_file`] for an already decoded picture.
    pub fn replace_texture_from_rgba(
        &mut self,
        index: usize,
        image: &RgbaImage,
        encoder: Option<&Path>,
    ) -> Result<Option<String>> {
        let texture = self
            .textures
            .get_mut(index)
            .ok_or_else(|| BntxError::new(0, format!("texture index {index} out of range")))?;
        let mip_count = u32::from(texture.mip_count);
        replace_texture_from_image(texture, image, mip_count, encoder)
    }

    /// Decodes texture `index` (first array slice, mip 0) to an RGBA image.
    pub fn decode_texture(&self, index: usize) -> Result<image::RgbaImage> {
        let texture = self
            .textures
            .get(index)
            .ok_or_else(|| BntxError::new(0, format!("texture index {index} out of range")))?;
        let data = texture
            .texture_data
            .first()
            .ok_or_else(|| BntxError::new(0, "texture has no image data"))?;
        let linear = texture.tile_mode == TILE_MODE_LINEAR_ALIGNED;
        let log2 = texture.block_height_log2;
        let image = if let Some((block_width, block_height)) =
            switch_texture::astc_block_from_bntx(texture.format)
        {
            if linear {
                return Err(BntxError::new(
                    0,
                    "linear ASTC textures cannot be previewed",
                ));
            }
            switch_texture::decode_astc(
                texture.width,
                texture.height,
                data,
                block_width,
                block_height,
                log2,
            )?
        } else {
            let format = switch_texture::format_from_bntx(texture.format)?;
            switch_texture::decode(texture.width, texture.height, format, data, log2, linear)?
        };
        Ok(image)
    }

    /// `BntxFile.DataAlignment`.
    pub fn data_alignment(&self) -> u32 {
        1u32 << (self.alignment as u32 & 31)
    }

    /// The header version as Syroot reads it (`u32` at 0x08).
    pub fn version_u32(&self) -> u32 {
        u32::from_le_bytes(self.version)
    }

    pub fn version_major2(&self) -> u32 {
        (self.version_u32() >> 16) & 0xff
    }

    pub fn version_minor(&self) -> u32 {
        (self.version_u32() >> 8) & 0xff
    }

    /// `BntxFile.WriteOriginalRLT()`: true when the vanilla relocation table
    /// can be written back verbatim.
    fn write_original_rlt(&self) -> bool {
        if self.name != self.original_file_name {
            return false;
        }
        if self.textures.len() != self.original_file_data.len() {
            return false;
        }
        for texture in &self.textures {
            let Some(original) = self
                .original_file_data
                .iter()
                .find(|entry| entry.name == texture.name)
            else {
                return false;
            };
            if original.image_size != texture.image_size {
                return false;
            }
            // Textures with user data always force a rebuilt table.
            if original.mip_count != u32::from(texture.mip_count) {
                return false;
            }
            if original.user_data_count != 0 {
                return false;
            }
        }
        true
    }

    /// Changes the name stored in the BNTX header.
    pub fn set_internal_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// Renames one texture entry (the dictionary is rebuilt on save).
    pub fn rename_texture(&mut self, index: usize, name: &str) -> Result<()> {
        if self
            .textures
            .iter()
            .enumerate()
            .any(|(i, texture)| i != index && texture.name.eq_ignore_ascii_case(name))
        {
            return err(format!("a texture named {name} already exists"));
        }
        match self.textures.get_mut(index) {
            Some(texture) => {
                texture.name = name.to_string();
                Ok(())
            }
            None => err(format!("texture index {index} out of range")),
        }
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
    if let Some((w, h)) = switch_texture::astc_block_from_bntx(format) {
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

// ---- Loader: BntxFileLoader + IResData.Load ------------------------------

/// `BntxFileLoader` state on top of [`BinaryReader`]: every string read while
/// loading is recorded by offset (`StringTable.Strings`), which is what the
/// saver rewrites at the end of the file.
struct Loader<'a> {
    reader: BinaryReader<'a>,
    strings: BTreeMap<u64, String>,
}

impl<'a> Loader<'a> {
    fn bytes(&self, at: usize, len: usize) -> Result<&'a [u8]> {
        self.reader
            .read_bytes_at(at, len)
            .map_err(|error| BntxError::new(at, error.to_string()))
    }

    fn u8(&self, at: usize) -> Result<u8> {
        self.reader
            .read_u8_at(at)
            .map_err(|error| BntxError::new(at, error.to_string()))
    }

    fn u16(&self, at: usize) -> Result<u16> {
        self.reader
            .read_u16_at(at)
            .map_err(|error| BntxError::new(at, error.to_string()))
    }

    fn u32(&self, at: usize) -> Result<u32> {
        self.reader
            .read_u32_at(at)
            .map_err(|error| BntxError::new(at, error.to_string()))
    }

    fn i32(&self, at: usize) -> Result<i32> {
        self.reader
            .read_i32_at(at)
            .map_err(|error| BntxError::new(at, error.to_string()))
    }

    fn i64(&self, at: usize) -> Result<i64> {
        self.reader
            .read_i64_at(at)
            .map_err(|error| BntxError::new(at, error.to_string()))
    }

    fn offset(&self, at: usize) -> Result<usize> {
        let value = self.i64(at)?;
        usize::try_from(value).map_err(|_| BntxError::new(at, "negative offset"))
    }

    fn signature(&self, at: usize, expected: &[u8; 4]) -> Result<()> {
        if self.bytes(at, 4)? != expected {
            return Err(BntxError::new(
                at,
                format!(
                    "invalid signature, expected {}",
                    String::from_utf8_lossy(expected)
                ),
            ));
        }
        Ok(())
    }

    /// `BntxFileLoader.LoadString`: a `u16` byte-length prefixed UTF-8 string
    /// at `offset`, recorded in the string table.
    fn load_string_at(&mut self, offset: usize) -> Result<String> {
        let length = self.u16(offset)? as usize;
        let text = String::from_utf8_lossy(self.bytes(offset + 2, length)?).into_owned();
        self.strings
            .entry(offset as u64)
            .or_insert_with(|| text.clone());
        Ok(text)
    }

    /// `LoadString()` with the offset read from the current field.
    fn load_string(&mut self, field: usize) -> Result<String> {
        let offset = self.offset(field)?;
        if offset == 0 {
            return Ok(String::new());
        }
        self.load_string_at(offset)
    }

    /// `ResDict.Load` through `LoadDict`: returns the number of entries.
    /// Keys are loaded (and thereby recorded in the string table).
    fn load_dict(&mut self, field: usize) -> Result<usize> {
        let offset = self.offset(field)?;
        if offset == 0 {
            return Ok(0);
        }
        self.signature(offset, b"_DIC")?;
        let count = self.i32(offset + 4)?;
        if count < 0 {
            return Err(BntxError::new(offset + 4, "negative dictionary count"));
        }
        let mut node = offset + 8;
        for _ in 0..=count {
            // u32 reference, u16 left, u16 right, i64 key offset
            self.load_string(node + 8)?;
            node += 16;
        }
        Ok(count as usize)
    }
}

fn load(data: &[u8]) -> Result<BntxFile> {
    let mut reader = Loader {
        reader: BinaryReader::new(data),
        strings: BTreeMap::new(),
    };
    reader.signature(0, b"BNTX")?;
    let version = reader.u32(8)?;
    let byte_order = reader.u16(0x0c)?;
    if byte_order != 0xFEFF {
        return err("big endian BNTX files are not supported");
    }
    let alignment = reader.u8(0x0e)?;
    let target_address_size = reader.u8(0x0f)?;
    let name_offset = reader.u32(0x10)? as usize;
    let flag = reader.u16(0x14)?;
    let rlt_offset = reader.u32(0x18)? as usize;
    let file_size = reader.u32(0x1c)?;
    if file_size as usize > data.len() {
        return err("declared file size exceeds input");
    }
    let target_bytes: [u8; 4] = reader
        .bytes(0x20, 4)?
        .try_into()
        .map_err(|_| BntxError::new(0x20, "target platform field is not 4 bytes"))?;
    let texture_count = reader.i32(0x24)?;
    if texture_count < 0 {
        return err("negative texture count");
    }
    let texture_array_offset = reader.offset(0x28)?;

    if rlt_offset > data.len() {
        return err("relocation table offset past end of file");
    }
    let original_rlt_chunk = data
        .get(rlt_offset..)
        .ok_or_else(|| BntxError::new(rlt_offset, "relocation table offset past end of file"))?
        .to_vec();

    let mut textures = Vec::with_capacity(texture_count as usize);
    for index in 0..texture_count as usize {
        let entry = reader.offset(texture_array_offset + index * 8)?;
        textures.push(load_texture(&mut reader, entry)?);
    }

    // BntxFile.Load reads the data block offset, then LoadDict() at 0x38.
    let dict_count = reader.load_dict(0x38)?;
    if dict_count != textures.len() {
        return err(format!(
            "texture dictionary has {dict_count} entries but the file holds {} textures",
            textures.len()
        ));
    }

    if name_offset < 2 {
        return err("invalid file name offset");
    }
    let name = reader.load_string_at(name_offset - 2)?;

    // The loader seeks past the texture offset array and expects `_STR`.
    reader.signature(texture_array_offset + texture_count as usize * 8, b"_STR")?;

    let original_file_data = textures
        .iter()
        .map(|(texture, user_data_count)| OriginalImageData {
            name: texture.name.clone(),
            mip_count: u32::from(texture.mip_count),
            image_size: texture.image_size,
            user_data_count: *user_data_count,
        })
        .collect();
    let textures = textures.into_iter().map(|(texture, _)| texture).collect();

    Ok(BntxFile {
        version: version.to_le_bytes(),
        target: String::from_utf8_lossy(&target_bytes).to_string(),
        file_size,
        textures,
        original_file_name: name.clone(),
        name,
        byte_order,
        alignment,
        target_address_size,
        flag,
        target_bytes,
        string_table: reader.strings,
        original_rlt_chunk,
        original_file_data,
    })
}

/// Loads one `BRTI` block; returns the texture and its user data count.
fn load_texture(reader: &mut Loader<'_>, at: usize) -> Result<(BntxTexture, u32)> {
    reader.signature(at, b"BRTI")?;
    let p = at + 16; // after the header block (u32 next offset + u64 size)
    let flags = reader.u8(p)?;
    let dim = reader.u8(p + 1)?;
    let tile_mode = reader.u16(p + 2)?;
    let swizzle = reader.u16(p + 4)?;
    let mip_count = reader.u16(p + 6)? as u32;
    let sample_count = reader.u32(p + 8)?;
    let format = reader.u32(p + 0x0c)?;
    let access_flags = reader.u32(p + 0x10)?;
    let width = reader.u32(p + 0x14)?;
    let height = reader.u32(p + 0x18)?;
    let depth = reader.u32(p + 0x1c)?;
    let array_length = reader.u32(p + 0x20)?;
    let texture_layout = reader.u32(p + 0x24)?;
    let texture_layout2 = reader.u32(p + 0x28)?;
    // 20 reserved bytes
    let image_size = reader.u32(p + 0x40)?;
    if image_size == 0 {
        return err("Empty image size!");
    }
    let alignment = reader.i32(p + 0x44)?;
    let channels = reader.u32(p + 0x48)?;
    let surface_dim = reader.u32(p + 0x4c)?;
    let name_offset = reader.offset(p + 0x50)?;
    let name = reader.load_string(p + 0x50)?;
    let name_capacity = if name_offset == 0 {
        0
    } else {
        usize::from(reader.u16(name_offset)?)
    };
    // p + 0x58: parent offset, p + 0x60: mip offsets, p + 0x68: user data,
    // p + 0x70 / 0x78 / 0x80: texture / view / descriptor, p + 0x88: user data dict
    let mip_offsets_ptr = reader.offset(p + 0x60)?;
    // User data is tolerated for viewing; the Toolbox-style saver refuses it.
    let user_data_count = reader.load_dict(p + 0x88)? as u32;
    if array_length == 0 {
        return err(format!("texture {name} has an array length of 0"));
    }

    let mut mip_offsets = Vec::with_capacity(mip_count as usize);
    for index in 0..mip_count as usize {
        mip_offsets.push(reader.i64(mip_offsets_ptr + index * 8)?);
    }
    if mip_offsets.is_empty() {
        return err(format!("texture {name} has no mip levels"));
    }
    // Absolute offsets of every (slice, mip) surface, as the image viewer and
    // the in-place patchers of `Image::document` address them.
    let pointer_count = mip_count as usize * array_length as usize;
    let mut data_offsets = Vec::with_capacity(pointer_count);
    for index in 0..pointer_count {
        data_offsets.push(reader.i64(mip_offsets_ptr + index * 8)? as u64);
    }

    // Texture.Load reads every mip of every slice; only `[slice][0]` (the
    // whole slice) survives to the saver, and that is what is kept here.
    let mut texture_data = Vec::with_capacity(array_length as usize);
    let mut slice_start = 0i64;
    for _ in 0..array_length {
        for (level, mip_offset) in mip_offsets.iter().enumerate() {
            let size = (mip_offsets[0] + image_size as i64 - mip_offset) / array_length as i64;
            if size <= 0 {
                return err(format!(
                    "Empty mip size! Texture {name} ImageSize {image_size} mips level {level}"
                ));
            }
            if level == 0 {
                let start = usize::try_from(slice_start + mip_offset)
                    .map_err(|_| BntxError::new(0, "negative mip offset"))?;
                let slice = reader.bytes(start, size as usize)?.to_vec();
                slice_start += slice.len() as i64;
                texture_data.push(slice);
            }
        }
    }

    let first = mip_offsets[0];
    for offset in &mut mip_offsets {
        *offset -= first;
    }

    let texture = BntxTexture {
        name,
        width,
        height,
        depth,
        array_length,
        mip_count: mip_count as u16,
        format,
        image_size,
        alignment: alignment as u32,
        tile_mode,
        swizzle,
        block_height_log2: (texture_layout & 7) as u8,
        channel_types: channels.to_le_bytes(),
        data_offsets,
        name_offset,
        name_capacity,
        format_offset: p + 0x0c,
        dim,
        sample_count,
        access_flags,
        texture_layout,
        texture_layout2,
        surface_dim,
        mip_offsets,
        texture_data,
        read_texture_layout: (flags & 1) as i32,
        sparse_binding: (flags >> 1) as i32,
        sparse_residency: (flags >> 2) as i32,
    };
    Ok((texture, user_data_count))
}

// ---- Saver: BntxFileSaver.Execute + IResData.Save -------------------------

const SECTION_1: u32 = 1;
const SECTION_2: u32 = 2;

/// `BinaryDataWriter` over a `MemoryStream`: writing past the end grows the
/// buffer with zeros, seeking past the end does not.
struct Writer {
    inner: BinaryWriter,
}

impl Writer {
    fn new() -> Self {
        Writer {
            inner: BinaryWriter::new(),
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.inner.into_inner()
    }

    fn position(&self) -> usize {
        self.inner.position()
    }

    /// Moves the cursor without growing the buffer.
    fn set_position(&mut self, position: usize) {
        self.inner.set_position(position);
    }

    fn write(&mut self, bytes: &[u8]) {
        self.inner.write_bytes(bytes);
    }

    fn u8(&mut self, value: u8) {
        self.inner.write_u8(value);
    }

    fn u16(&mut self, value: u16) {
        self.inner.write_u16(value);
    }

    fn i16(&mut self, value: i16) {
        self.inner.write_i16(value);
    }

    fn u32(&mut self, value: u32) {
        self.inner.write_u32(value);
    }

    fn i32(&mut self, value: i32) {
        self.inner.write_i32(value);
    }

    fn i64(&mut self, value: i64) {
        self.inner.write_i64(value);
    }

    fn u64(&mut self, value: u64) {
        self.inner.write_u64(value);
    }

    fn zeros(&mut self, count: usize) {
        self.inner.write_zeros(count);
    }

    fn align(&mut self, alignment: usize) {
        let position = self.inner.position().div_ceil(alignment) * alignment;
        self.inner.set_position(position);
    }

    fn at<T>(&mut self, position: usize, f: impl FnOnce(&mut Writer) -> T) -> T {
        let saved = self.inner.position();
        self.inner.set_position(position);
        let result = f(self);
        self.inner.set_position(saved);
        result
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

#[derive(Clone)]
struct RelocationEntry {
    position: u32,
    offset_count: u32,
    struct_count: u32,
    padding_count: u32,
}

struct RelocationSection {
    position: u32,
    entry_index: i32,
    size: u32,
    entries: Vec<RelocationEntry>,
}

struct StringEntry {
    offsets: Vec<u32>,
}

struct Saver<'a> {
    file: &'a BntxFile,
    w: Writer,
    ofs_file_name: u32,
    ofs_file_size: u32,
    ofs_string_pool: u32,
    ofs_relocation_table: u32,
    section1_size: u32,
    ofs_texture_data_block: i64,
    ofs_end_of_block: i64,
    data_block_position: i64,
    header_block_positions: Vec<i64>,
    /// `_savedStrings`: a SortedDictionary keyed by the string. Toolbox uses
    /// the culture-aware default comparer; ordinal order is used here, which
    /// only matters when two or more *new* strings are added at once.
    saved_strings: BTreeMap<String, StringEntry>,
    section1_entries: Vec<RelocationEntry>,
    section2_entries: Vec<RelocationEntry>,
    mip_map_offsets: Vec<i64>,
    relocated_sections: Vec<RelocationSection>,
    /// `BntxFile.BntxTextureArrayOffset` / `BntxTextureDictOffset`.
    texture_array_offset: i64,
    texture_dict_offset: i64,
}

fn save(file: &BntxFile) -> Result<Vec<u8>> {
    if let Some(original) = file
        .original_file_data
        .iter()
        .find(|entry| entry.user_data_count != 0)
    {
        return err(format!(
            "texture {} carries user data, which the Toolbox-compatible saver does not support",
            original.name
        ));
    }
    let mut saver = Saver {
        file,
        w: Writer::new(),
        ofs_file_name: 0,
        ofs_file_size: 0,
        ofs_string_pool: 0,
        ofs_relocation_table: 0,
        section1_size: 0,
        ofs_texture_data_block: 0,
        ofs_end_of_block: 0,
        data_block_position: 0,
        header_block_positions: Vec::new(),
        saved_strings: BTreeMap::new(),
        section1_entries: Vec::new(),
        section2_entries: Vec::new(),
        mip_map_offsets: Vec::new(),
        relocated_sections: Vec::new(),
        texture_array_offset: 0,
        texture_dict_offset: 0,
    };
    saver.execute()?;
    Ok(saver.w.into_inner())
}

impl<'a> Saver<'a> {
    fn position(&self) -> i64 {
        self.w.position() as i64
    }

    // ----- helpers mirroring the C# saver ---------------------------------

    fn save_relocate_entry_to_section(
        &mut self,
        pos: i64,
        offset_count: u32,
        struct_count: u32,
        padding_count: u32,
        section: u32,
    ) {
        if offset_count > 0xff {
            self.save_relocate_entry_to_section(pos, 0xff, struct_count, padding_count, section);
            self.save_relocate_entry_to_section(
                pos + 0x7f8,
                offset_count - 0xff,
                struct_count,
                padding_count,
                section,
            );
            return;
        }
        let entry = RelocationEntry {
            position: pos as u32,
            offset_count,
            struct_count,
            padding_count,
        };
        match section {
            SECTION_1 => self.section1_entries.push(entry),
            SECTION_2 => self.section2_entries.push(entry),
            _ => {}
        }
    }

    fn save_header_block(&mut self, is_binary_header: bool) {
        self.header_block_positions.push(self.position());
        if is_binary_header {
            self.w.u16(0);
        } else {
            self.write_header_block(0, 0);
        }
    }

    fn write_header_block(&mut self, size: u32, offset: i64) {
        self.w.u32(size);
        self.w.i64(offset);
    }

    fn save_offset(&mut self) -> i64 {
        let pos = self.position();
        self.w.i64(0);
        pos
    }

    /// `WriteOffset`: stores the current position at `offset`.
    fn write_offset(&mut self, offset: i64) {
        let current = self.position();
        self.w.at(offset as usize, |w| w.i64(current));
    }

    fn save_string(&mut self, text: &str) {
        let pos = self.position() as u32;
        self.saved_strings
            .entry(text.to_string())
            .or_insert_with(|| StringEntry {
                offsets: Vec::new(),
            })
            .offsets
            .push(pos);
        self.w.u32(u32::MAX);
        self.w.i32(0);
    }

    fn save_mip_map_offsets(&mut self) {
        self.mip_map_offsets.push(self.position());
        self.w.i64(i64::MAX);
    }

    fn satisfy_offsets(&mut self, offsets: &[u32], target: u32) {
        for offset in offsets {
            self.w.at(*offset as usize, |w| w.i32(target as i32));
        }
    }

    fn write_zero_terminated(&mut self, text: &str) {
        self.w.write(text.as_bytes());
        self.w.u8(0);
    }

    // ----- Execute ----------------------------------------------------------

    fn execute(&mut self) -> Result<()> {
        self.save_bntx_file();

        self.w.align(8);
        let texture_array_offset = self.texture_array_offset;
        self.write_offset(texture_array_offset);
        let mut texture_offsets = Vec::with_capacity(self.file.textures.len());
        for _ in &self.file.textures {
            texture_offsets.push(self.save_offset());
        }

        self.setup_string_pool();

        self.w.align(8);
        let dict_offset = self.texture_dict_offset;
        self.write_offset(dict_offset);
        self.save_dict();

        self.w.align(8);
        for (index, texture) in self.file.textures.iter().enumerate() {
            self.write_offset(texture_offsets[index]);
            self.save_texture(texture);
        }

        self.section1_size = self.position() as u32;
        // Seek(16, Current) then move the BRTD header right in front of the
        // aligned data block.
        let position = self.w.position() + 16;
        self.w.set_position(position);
        let data_alignment = self.file.data_alignment() as i32;
        let padding = round_up_i32(self.position() as i32, data_alignment) - self.position() as i32;
        if padding > 0 {
            let position = (self.position() + (padding - 16) as i64) as usize;
            self.w.set_position(position);
        }
        self.data_block_position = self.position();
        self.write_texture_block();

        let can_write = self.setup_relocation_table();
        self.write_relocation_table(can_write);

        let string_pool = self.ofs_string_pool as usize;
        {
            let saved = self.w.position();
            self.w.set_position(string_pool);
            self.write_strings();
            self.w.set_position(saved);
        }

        let count = self.header_block_positions.len();
        for index in 0..count {
            let block = self.header_block_positions[index];
            self.w.set_position(block as usize);
            if index == 0 {
                let value = (self.header_block_positions[1] - 4) as u16;
                self.w.u16(value);
            } else if index == count - 1 {
                self.w.i32(0);
                let value = self.ofs_end_of_block - block;
                self.w.i64(value);
            } else {
                let value = (self.header_block_positions[index + 1] - block) as u32;
                self.write_header_block(value, value as i64);
            }
        }

        self.w.set_position(self.ofs_texture_data_block as usize);
        let data_block_position = self.data_block_position;
        self.w.i64(data_block_position);

        self.w.set_position(self.ofs_file_size as usize);
        let length = self.w.len() as u32;
        self.w.u32(length);
        Ok(())
    }
}

/// `round_up(int x, int y)` with C# `int` wrap-around semantics.
fn round_up_i32(x: i32, y: i32) -> i32 {
    (x.wrapping_sub(1) | y.wrapping_sub(1)).wrapping_add(1)
}

impl<'a> Saver<'a> {
    fn save_bntx_file(&mut self) {
        let file = self.file;
        self.w.write(b"BNTX");
        self.w.i32(0);
        self.w.u32(file.version_u32());
        self.w.u16(if file.byte_order == 0 {
            0xFEFF
        } else {
            file.byte_order
        });
        self.w.u8(file.alignment);
        self.w.u8(file.target_address_size);
        // SaveFileNameString
        self.ofs_file_name = self.position() as u32;
        self.w.i32(0);
        self.w.u16(file.flag);
        self.save_header_block(true);
        // SaveRelocationTable
        self.ofs_relocation_table = self.position() as u32;
        self.w.i32(0);
        // SaveFieldFileSize
        self.ofs_file_size = self.position() as u32;
        self.w.i32(0);
        self.w.write(&file.target_bytes);
        self.w.i32(file.textures.len() as i32);
        let pos = self.position();
        self.save_relocate_entry_to_section(pos, 1, 2, 1, SECTION_1);
        self.texture_array_offset = self.save_offset();
        let pos = self.position();
        self.save_relocate_entry_to_section(pos, 1, 1, 0, SECTION_2);
        // SaveBntxTextureDataBlocks
        self.ofs_texture_data_block = self.position();
        self.w.i64(0);
        self.texture_dict_offset = self.save_offset();
        let pos = self.position();
        self.save_relocate_entry_to_section(pos, 1, 1, 0, SECTION_1);
        self.w.u64(88);
        self.w.i64(0);
        self.w.i64(0);
        self.w.zeros(0x140);
        let pos = self.position();
        self.save_relocate_entry_to_section(pos, file.textures.len() as u32, 1, 0, SECTION_1);
    }

    /// `SetupStringPool`: reserves the `_STR` block with every string the file
    /// will reference, in the order Toolbox reserves them.
    fn setup_string_pool(&mut self) {
        let file = self.file;
        // `strings`: a Dictionary (insertion ordered) of every string.
        let mut strings: Vec<String> = Vec::new();
        let push = |strings: &mut Vec<String>, value: &str| {
            if !strings.iter().any(|existing| existing == value) {
                strings.push(value.to_string());
            }
        };
        for texture in &file.textures {
            push(&mut strings, &texture.name);
        }
        push(&mut strings, &file.name);
        push(&mut strings, "");

        // `sorted`: original table order first, then the remaining new ones.
        let mut ordered: Vec<String> = Vec::new();
        for value in file.string_table.values() {
            if strings.iter().any(|s| s == value) && !ordered.iter().any(|s| s == value) {
                ordered.push(value.clone());
            }
        }
        for value in &strings {
            if !ordered.iter().any(|s| s == value) {
                ordered.push(value.clone());
            }
        }

        self.w.align(4);
        self.w.write(b"_STR");
        self.save_header_block(false);
        self.ofs_string_pool = self.position() as u32;
        self.w.i32(ordered.len() as i32 - 1);
        let mut name_position = 0u32;
        for value in &ordered {
            if *value == file.name {
                if self.saved_strings.contains_key(value) {
                    continue;
                }
                let pos = self.position() as u32;
                self.saved_strings
                    .insert(value.clone(), StringEntry { offsets: vec![pos] });
                name_position = pos;
            }
            self.w.i16(value.chars().count() as i16);
            self.write_zero_terminated(value);
            self.w.align(2);
        }
        if name_position > 0 {
            let field = self.ofs_file_name as usize;
            self.w.at(field, |w| w.u32(name_position + 2));
        }
    }

    /// `WriteStrings`: rewrites the pool from `_ofsStringPool` in the final
    /// order and satisfies every recorded string pointer.
    fn write_strings(&mut self) {
        let file = self.file;
        let mut ordered: Vec<String> = Vec::new();
        for value in file.string_table.values() {
            if self.saved_strings.contains_key(value) && !ordered.iter().any(|s| s == value) {
                ordered.push(value.clone());
            }
        }
        for value in self.saved_strings.keys() {
            if !ordered.iter().any(|s| s == value) {
                ordered.push(value.clone());
            }
        }

        self.w.i32(ordered.len() as i32 - 1);
        let mut name_position = 0u32;
        for value in &ordered {
            let pos = self.position() as u32;
            if *value == file.name {
                name_position = pos;
            }
            let offsets = self.saved_strings[value].offsets.clone();
            self.satisfy_offsets(&offsets, pos);
            self.w.i16(value.chars().count() as i16);
            let start = self.position();
            self.write_zero_terminated(value);
            let byte_length = (self.position() - start - 1) as u16;
            self.w.at((start - 2) as usize, |w| w.u16(byte_length));
            self.w.align(2);
        }
        if self.ofs_file_name > 0 && name_position > 0 {
            let field = self.ofs_file_name as usize;
            self.w.at(field, |w| w.u32(name_position + 2));
        }
    }

    /// `ResDict.Save` for the texture dictionary (rebuilt from the textures).
    fn save_dict(&mut self) {
        let keys: Vec<String> = self
            .file
            .textures
            .iter()
            .map(|texture| texture.name.clone())
            .collect();
        let nodes = patricia::build_nodes(&keys);
        self.w.write(b"_DIC");
        self.w.i32(keys.len() as i32);
        for (index, node) in nodes.iter().enumerate() {
            self.w.u32(node.reference);
            self.w.u16(node.left);
            self.w.u16(node.right);
            if index == 0 {
                let pos = self.position();
                self.save_relocate_entry_to_section(pos, 1, nodes.len() as u32, 1, SECTION_1);
                self.save_string("");
            } else {
                let key = node.key.clone().unwrap_or_default();
                self.save_string(&key);
            }
        }
    }

    /// `BntxTexture.Save`.
    fn save_texture(&mut self, texture: &BntxTexture) {
        let channels = u32::from_le_bytes(texture.channel_types);
        let texture_layout = if texture.read_texture_layout != 1 {
            0
        } else if self.file.version_major2() == 4 && self.file.version_minor() >= 1 {
            u32::from(texture.block_height_log2)
        } else {
            ((texture.sparse_residency as u32) << 5)
                | ((texture.sparse_binding as u32) << 4)
                | u32::from(texture.block_height_log2)
        };
        let flags = (((texture.sparse_residency << 2)
            | (texture.sparse_binding << 1)
            | texture.read_texture_layout) as u32) as u8;

        self.w.write(b"BRTI");
        self.save_header_block(false);
        let start = self.position();
        self.w.u8(flags);
        self.w.u8(texture.dim);
        self.w.u16(texture.tile_mode);
        self.w.u16(texture.swizzle as u16);
        self.w.u16(texture.mip_count as u16);
        self.w.u32(texture.sample_count);
        self.w.u32(texture.format);
        self.w.u32(texture.access_flags);
        self.w.u32(texture.width);
        self.w.u32(texture.height);
        self.w.u32(texture.depth);
        self.w.i32(texture.texture_data.len() as i32);
        self.w.u32(texture_layout);
        self.w.u32(texture.texture_layout2);
        self.w.zeros(20);
        self.w.i32(texture.total_size() as i32);
        self.w.i32(texture.alignment as i32);
        self.w.u32(channels);
        self.w.u32(texture.surface_dim);
        let pos = self.position();
        self.save_relocate_entry_to_section(pos, 3, 1, 0, SECTION_1);
        self.save_string(&texture.name);
        self.w.i64(32);
        let mip_offsets_field = self.save_offset();
        let pos = self.position();
        self.save_relocate_entry_to_section(pos, 1, 1, 0, SECTION_1);
        let _user_data_field = self.save_offset();
        let pos = self.position();
        self.save_relocate_entry_to_section(pos, 2, 1, 0, SECTION_1);
        self.w.i64(start + 0x90);
        self.w.i64(start + 0x190);
        self.w.i64(0);
        let pos = self.position();
        self.save_relocate_entry_to_section(pos, 1, 1, 0, SECTION_1);
        let _user_data_dict_field = self.save_offset();
        self.w.zeros(0x200);
        self.w.align(8);
        let mip_offsets_position = self.position();
        self.save_relocate_entry_to_section(
            mip_offsets_position,
            u32::from(texture.mip_count),
            1,
            0,
            SECTION_2,
        );
        for _ in &texture.mip_offsets {
            self.save_mip_map_offsets();
        }
        // No user data (rejected on load).
        self.w
            .at(mip_offsets_field as usize, |w| w.i64(mip_offsets_position));
    }

    /// `WriteBntxTextureBlock`: the `BRTD` block with every slice of every texture.
    fn write_texture_block(&mut self) {
        self.w.write(b"BRTD");
        self.save_header_block(false);
        let mut mip_index = 0;
        for texture in &self.file.textures {
            let data_start = self.position();
            for mip_offset in &texture.mip_offsets {
                let field = self.mip_map_offsets[mip_index] as usize;
                self.w.at(field, |w| w.i64(data_start + mip_offset));
                mip_index += 1;
            }
            for slice in &texture.texture_data {
                self.w.write(slice);
            }
        }
    }

    /// `SetupRelocationTable`: returns whether a new table must be written
    /// (`false` means the original chunk is copied back).
    fn setup_relocation_table(&mut self) -> bool {
        let data_alignment = self.file.data_alignment() as usize;
        self.w.align(data_alignment);
        if self.file.write_original_rlt() {
            return false;
        }
        let rlt_position = self.position();
        self.section1_entries.sort_by_key(|entry| entry.position);
        self.section2_entries.sort_by_key(|entry| entry.position);
        let section1 = RelocationSection {
            position: 0,
            entry_index: 0,
            size: self.section1_size,
            entries: self.section1_entries.clone(),
        };
        let section2 = RelocationSection {
            position: self.data_block_position as u32,
            entry_index: self.section1_entries.len() as i32,
            size: (rlt_position - self.data_block_position) as u32,
            entries: self.section2_entries.clone(),
        };
        self.relocated_sections.push(section1);
        self.relocated_sections.push(section2);
        true
    }

    fn write_relocation_table(&mut self, can_write: bool) {
        let position = self.position() as u32;
        let field = self.ofs_relocation_table as usize;
        self.w.at(field, |w| w.u32(position));
        self.ofs_end_of_block = self.position() + 4;
        if !can_write {
            let chunk = self.file.original_rlt_chunk.clone();
            self.w.write(&chunk);
            return;
        }
        self.w.write(b"_RLT");
        self.w.u32(position);
        self.w.i32(self.relocated_sections.len() as i32);
        self.w.i32(0);
        let sections = std::mem::take(&mut self.relocated_sections);
        for section in &sections {
            self.w.i64(0);
            self.w.u32(section.position);
            self.w.u32(section.size);
            self.w.i32(section.entry_index);
            self.w.i32(section.entries.len() as i32);
        }
        for section in &sections {
            for entry in &section.entries {
                self.w.u32(entry.position);
                self.w.u16(entry.struct_count as u16);
                self.w.u8(entry.offset_count as u8);
                self.w.u8(entry.padding_count as u8);
            }
        }
        self.relocated_sections = sections;
    }
}

// ---- Importer: TextureImporterSettings.FromBitMap + SwizzleSurfaceMipMaps --

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
/// variable, the bundled `bin/cpp/` folder (next to the executable, or in
/// `src-tauri` during development), the executable directory, then PATH.
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
        dirs.push(exe_dir.join("bin/cpp"));
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin/cpp"));
    if let Ok(exe_dir) = crate::utils::running_exe_dir() {
        dirs.push(exe_dir.join("bin"));
        dirs.push(exe_dir);
    }
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
fn total_mip_count(width: u32, height: u32) -> u32 {
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
pub(crate) fn encode_astc_level(
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
        .map_err(|error| BntxError::new(0, error.to_string()))?;
    let output = Command::new(encoder)
        .arg(if srgb { "-cs" } else { "-cl" })
        .arg(&png)
        .arg(&astc)
        .arg(format!("{block_width}x{block_height}"))
        .arg("-thorough")
        .arg("-silent")
        .output()
        .map_err(|error| BntxError::new(0, format!("cannot run {}: {error}", encoder.display())))?;
    if !output.status.success() || !astc.is_file() {
        return err(format!(
            "astcenc failed ({}) for mip {level}: {} {}",
            output.status,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let file = std::fs::read(&astc)?;
    if file.len() < 16 || file.get(..4) != Some(&[0x13, 0xAB, 0xA1, 0x5C][..]) {
        return err(format!(
            "astcenc produced an unexpected file for mip {level}"
        ));
    }
    let (file_block_width, file_block_height) = (
        file.get(4).copied().unwrap_or_default(),
        file.get(5).copied().unwrap_or_default(),
    );
    if u32::from(file_block_width) != block_width || u32::from(file_block_height) != block_height {
        return err(format!(
            "astcenc produced {file_block_width}x{file_block_height} blocks instead of {block_width}x{block_height}"
        ));
    }
    Ok(file.get(16..).unwrap_or_default().to_vec())
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
                        BntxError::new(
                            0,
                            "no astcenc executable found; pass --astcenc, set ASTCENC or put astcenc-avx2.exe into bin/cpp",
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
                        let source = data
                            .get(pos_..pos_ + bpp as usize)
                            .ok_or_else(|| BntxError::new(0, "linear surface data is too short"))?;
                        BinaryPatcher::new(&mut result)
                            .write_bytes_at(pos, source)
                            .map_err(|_| BntxError::new(0, "linear surface is too small"))?;
                    }
                }
            }
        }
        return Ok(result);
    }

    let block_height_mip0 = 1usize << block_height_log2.min(5);
    let tegra_block_height = BlockHeight::new(block_height_mip0)
        .ok_or_else(|| BntxError::new(0, "invalid Tegra block height"))?;
    let mut block_dim = BlockDim::uncompressed();
    let non_zero = |value: u32| {
        NonZeroUsize::new(value as usize)
            .ok_or_else(|| BntxError::new(0, "block dimension cannot be zero"))
    };
    block_dim.width = non_zero(block_width)?;
    block_dim.height = non_zero(block_height)?;
    block_dim.depth = non_zero(block_depth)?;
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
    .map_err(|error| BntxError::new(0, error.to_string()))?;
    let mut output = swizzled;
    output.resize(surface_size, 0);
    Ok(output)
}

/// Replaces `texture`'s image the way Toolbox's CLI does: the surface
/// format, flags, access flags, tile mode, surface dimension and sparse bits
/// are kept, everything else is rebuilt from the picture. Returns a warning
/// when the requested mip count had to be reduced.
fn replace_texture_from_image(
    texture: &mut BntxTexture,
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
    texture.mip_count = mip_count as u16;
    texture.depth = 1;
    texture.dim = DIM_2D;
    texture.texture_layout = 0;
    texture.texture_layout2 = DEFAULT_TEXTURE_LAYOUT2;
    texture.swizzle = 0;
    texture.sample_count = 1;
    texture.channel_types = channels;
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
        texture.block_height_log2 = (31 - block_height_mip0.leading_zeros()) as u8;
        texture.alignment = DEFAULT_ALIGNMENT;
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
            .ok_or_else(|| BntxError::new(0, "encoded mip data is too short"))?;
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
            u32::from(texture.block_height_log2).saturating_sub(block_height_shift),
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
        let file = BntxFile::parse(&raw).unwrap();
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
        let mut file = BntxFile::parse(&raw).unwrap();
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
            let file = BntxFile::parse(&raw).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
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
        let mut file = BntxFile::parse(&raw).unwrap();
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
        let mut file = BntxFile::parse(&raw).unwrap();
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
        let mut file = BntxFile::parse(&raw).unwrap();
        file.replace_texture_from_file(0, &png, Some(&encoder))
            .unwrap();
        assert_eq!(file.textures[0].width, 128);
        assert_eq!(file.save_like_toolbox_zs().unwrap(), expected);
    }
}
