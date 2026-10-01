//! Wii U (`FRES`, big-endian) texture archives: BOTW's `.sbitemico`,
//! `.sbmapopen`, `.sbstftex` and the like are Yaz0-compressed BFRES files that hold
//! nothing but `FTEX` GX2 textures.
//!
//! Loading, editing and saving follow Switch Toolbox step for step
//! (`Syroot.NintenTools.Bfres` for the container, `GX2.CreateGx2Texture`
//! for the surface, TrueYZ for Yaz0), so a file edited here comes out with
//! the bytes Toolbox writes for the same edit.
use crate::file_format::Image::{block_compress, gdiplus_resample, switch_texture};
use crate::parser::binary::{BinaryReader, BinaryWriter, Endian};
use crate::parser::gx2;
use image::RgbaImage;
use image_dds::ImageFormat;
use std::io;
use std::path::Path;

const FTEX_HEADER_SIZE: usize = 0xc0;
const GROUP_COUNT: usize = 12;
const TEXTURE_GROUP: usize = 1;
/// `GX2TileMode.Mode2dTiledThin1`, the importer default.
const IMPORT_TILE_MODE: u32 = 4;
/// `GX2SurfaceDim.Dim2D`
const DIM_2D: u32 = 1;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn unsupported(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message.into())
}

/// Where a texture's data block came from. Syroot keys saved blocks by
/// array identity, and .NET hands out one shared instance for every
/// zero-length read, so all untouched empty blocks collapse into the first.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Block {
    /// The offset field is 0.
    Null,
    /// Loaded with a size of 0: the shared empty array.
    SharedEmpty,
    Owned(Vec<u8>),
}

impl Block {
    fn bytes(&self) -> &[u8] {
        match self {
            Block::Owned(data) => data,
            _ => &[],
        }
    }
}

#[derive(Debug, Clone)]
pub struct FtexTexture {
    pub name: String,
    pub path: String,
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
    pub alignment: u32,
    pub pitch: u32,
    pub mip_offsets: [u32; 13],
    pub view_mip_first: u32,
    pub view_mip_count: u32,
    pub view_slice_first: u32,
    pub view_slice_count: u32,
    /// `GX2CompSel` per output channel: 0-3 = R/G/B/A, 4 = zero, 5 = one.
    pub comp_sel: [u8; 4],
    pub regs: [u32; 5],
    pub array_length: u8,
    data: Block,
    mip_data: Block,
    /// Offsets of the FTEX header and of the image and mip blocks in the
    /// file the texture came from.
    header_offset: usize,
    data_offset: usize,
    mip_data_offset: usize,
    has_user_data: bool,
    modified: bool,
}

impl FtexTexture {
    pub fn data(&self) -> &[u8] {
        self.data.bytes()
    }

    pub fn mip_data(&self) -> &[u8] {
        self.mip_data.bytes()
    }

    fn surface(&self) -> gx2::SurfaceDesc<'_> {
        gx2::SurfaceDesc {
            dim: self.dim,
            width: self.width,
            height: self.height,
            depth: self.depth,
            mip_count: self.mip_count,
            format: self.format,
            aa: self.aa,
            use_: self.use_,
            tile_mode: self.tile_mode,
            swizzle: self.swizzle,
            mip_offsets: &self.mip_offsets,
            data: self.data(),
            mip_data: self.mip_data(),
        }
    }

    pub fn array_count(&self) -> u32 {
        self.surface().array_count()
    }

    /// Mip levels stored in this file (a `.Tex1` texture keeps only level 0).
    pub fn stored_mip_count(&self) -> u32 {
        self.surface().stored_mip_count()
    }

    pub fn format_name(&self) -> String {
        gx2::format_name(self.format)
    }

    /// One line per texture, the way the BNTX CLI describes its textures.
    pub fn describe(&self) -> String {
        format!(
            "{} {}x{} {} mips={} tile={} swizzle=0x{:X} pitch={} align={} size={}+{}",
            self.name,
            self.width,
            self.height,
            self.format_name(),
            self.mip_count,
            self.tile_mode,
            self.swizzle,
            self.pitch,
            self.alignment,
            self.data().len(),
            self.mip_data().len(),
        )
    }

    /// Decodes one array layer of one mip level to RGBA, component
    /// selectors applied.
    pub fn decode(&self, array_index: u32, mip_index: u32) -> io::Result<RgbaImage> {
        let format = image_format(self.format)?;
        let linear = gx2::decode_level(&self.surface(), array_index, mip_index)?;
        let width = (self.width >> mip_index).max(1);
        let height = (self.height >> mip_index).max(1);
        let mut image = switch_texture::decode(width, height, format, &linear, 0, true)?;
        switch_texture::apply_component_selectors(&mut image, self.comp_sel.map(selector));
        Ok(image)
    }
}

/// `GX2CompSel` -> the selector values `switch_texture` understands
/// (0 = zero, 1 = one, 2-5 = R/G/B/A).
fn selector(comp_sel: u8) -> u8 {
    match comp_sel {
        0..=3 => comp_sel + 2,
        4 => 0,
        5 => 1,
        _ => u8::MAX,
    }
}

/// The decoder/encoder format of a GX2 surface format.
pub fn image_format(format: u32) -> io::Result<ImageFormat> {
    Ok(match format {
        0x001 => ImageFormat::R8Unorm,
        0x201 => ImageFormat::R8Snorm,
        0x007 => ImageFormat::Rg8Unorm,
        0x207 => ImageFormat::Rg8Snorm,
        0x01a => ImageFormat::Rgba8Unorm,
        0x41a => ImageFormat::Rgba8UnormSrgb,
        0x031 => ImageFormat::BC1RgbaUnorm,
        0x431 => ImageFormat::BC1RgbaUnormSrgb,
        0x032 => ImageFormat::BC2RgbaUnorm,
        0x432 => ImageFormat::BC2RgbaUnormSrgb,
        0x033 => ImageFormat::BC3RgbaUnorm,
        0x433 => ImageFormat::BC3RgbaUnormSrgb,
        0x034 => ImageFormat::BC4RUnorm,
        0x234 => ImageFormat::BC4RSnorm,
        0x035 => ImageFormat::BC5RgUnorm,
        0x235 => ImageFormat::BC5RgSnorm,
        other => {
            return Err(unsupported(format!(
                "unsupported GX2 texture format {}",
                gx2::format_name(other)
            )))
        }
    })
}

/// `FTEX.SetChannelsByFormat` on top of the importer's R/G/B/A default.
fn channels_by_format(format: u32) -> [u8; 4] {
    match format {
        0x035 => [0, 1, 5, 5],
        0x235 => [0, 1, 4, 5],
        0x034 | 0x234 => [0, 0, 0, 0],
        _ => [0, 1, 2, 3],
    }
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

/// `GTXImporterSettings.GenerateMips`: every level is resampled from the
/// previous one and encoded to linear blocks, levels packed back to back.
/// The second value is false when a level went through an encoder that is
/// not bit-exact with Toolbox's.
fn encode_linear_mips(
    image: &RgbaImage,
    format: u32,
    mip_count: u32,
) -> io::Result<(Vec<u8>, bool)> {
    let image_format = image_format(format)?;
    let mut exact = true;
    let mut data = Vec::new();
    let mut level_image = image.clone();
    for level in 0..mip_count {
        if level != 0 {
            let width = (image.width() >> level).max(1);
            let height = (image.height() >> level).max(1);
            level_image = gdiplus_resample::resize(&level_image, width, height)?;
            exact &= cfg!(windows);
        }
        let blocks = match block_compress::encode_linear(&level_image, image_format) {
            Ok(blocks) => blocks,
            Err(_) => {
                exact &= !gx2::is_bcn(format);
                switch_texture::encode(
                    &level_image,
                    switch_texture::encoding_format(image_format),
                    0,
                    true,
                )?
            }
        };
        data.extend_from_slice(&blocks);
    }
    Ok((data, exact))
}

/// How the file on disk wraps the BFRES.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtexCompression {
    None,
    /// Yaz0, with the header's alignment hint.
    Yaz0(u32),
}

#[derive(Debug, Clone)]
pub struct FtexFile {
    pub version: u32,
    pub alignment: u32,
    pub name: String,
    pub textures: Vec<FtexTexture>,
    pub compression: FtexCompression,
    original: Vec<u8>,
    /// The Syroot layout reproduces the loaded file bit for bit, so the file
    /// can be rebuilt (textures may change size). Otherwise edits are
    /// patched in place.
    rebuildable: bool,
}

/// True for a (raw, uncompressed) Wii U BFRES that holds textures and
/// nothing a 3D view could show.
pub fn is_texture_archive(data: &[u8]) -> bool {
    let reader = BinaryReader::with_endian(data, Endian::Big);
    if data.len() < 0x6c || &data[..4] != b"FRES" || data[8..10] != [0xfe, 0xff] {
        return false;
    }
    let count = |group: usize| reader.read_u16_at(0x50 + group * 2).unwrap_or(0);
    count(TEXTURE_GROUP) > 0 && count(0) == 0
}

/// Decompresses `data` when it is Yaz0 and reports how it was wrapped.
pub fn unwrap(data: &[u8]) -> io::Result<(Vec<u8>, FtexCompression)> {
    if crate::Settings::Magic::is_yaz0(data) {
        let alignment = data
            .get(8..12)
            .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .unwrap_or(0);
        let raw = roead::yaz0::decompress(data).map_err(|error| invalid(error.to_string()))?;
        Ok((raw, FtexCompression::Yaz0(alignment)))
    } else {
        Ok((data.to_vec(), FtexCompression::None))
    }
}

/// True when `data` (raw or Yaz0) is a Wii U texture archive.
pub fn is_texture_archive_file(data: &[u8]) -> bool {
    if is_texture_archive(data) {
        return true;
    }
    if !crate::Settings::Magic::is_yaz0(data) {
        return false;
    }
    // Only the header is needed; decompressing is cheap for these files.
    roead::yaz0::decompress(data)
        .map(|raw| is_texture_archive(&raw))
        .unwrap_or(false)
}

struct Loader<'a> {
    reader: BinaryReader<'a>,
}

impl Loader<'_> {
    fn u32(&self, at: usize) -> io::Result<u32> {
        self.reader.read_u32_at(at)
    }

    /// A self-relative offset field; `None` when it is 0.
    fn offset(&self, at: usize) -> io::Result<Option<usize>> {
        let value = self.reader.read_i32_at(at)?;
        if value == 0 {
            return Ok(None);
        }
        let target = at as i64 + i64::from(value);
        usize::try_from(target)
            .ok()
            .filter(|target| *target <= self.reader.len())
            .map(Some)
            .ok_or_else(|| invalid("BFRES offset lies outside the file"))
    }

    fn string(&self, at: usize) -> io::Result<String> {
        match self.offset(at)? {
            Some(target) => self.reader.read_c_string_at(target),
            None => Ok(String::new()),
        }
    }

    fn block(&self, at: usize, size: usize) -> io::Result<(Block, usize)> {
        let Some(target) = self.offset(at)? else {
            return Ok((Block::Null, 0));
        };
        if size == 0 {
            return Ok((Block::SharedEmpty, target));
        }
        let bytes = self
            .reader
            .read_bytes_at(target, size)
            .map_err(|_| invalid("FTEX image data lies outside the file"))?;
        Ok((Block::Owned(bytes.to_vec()), target))
    }
}

impl FtexFile {
    /// Parses a raw or Yaz0-compressed Wii U BFRES.
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        let (raw, compression) = unwrap(data)?;
        let mut file = Self::parse(raw)?;
        file.compression = compression;
        Ok(file)
    }

    pub fn open(path: &Path) -> io::Result<Self> {
        Self::from_bytes(&std::fs::read(path)?)
    }

    fn parse(raw: Vec<u8>) -> io::Result<Self> {
        if raw.len() < 0x6c || &raw[..4] != b"FRES" {
            return Err(invalid("not a BFRES file"));
        }
        if raw[8..10] != [0xfe, 0xff] {
            return Err(unsupported("not a Wii U (big-endian) BFRES file"));
        }
        let loader = Loader {
            reader: BinaryReader::with_endian(&raw, Endian::Big),
        };
        let version = loader.u32(4)?;
        if version < 0x0204_0000 {
            return Err(unsupported(format!(
                "BFRES version 0x{version:08X} is too old"
            )));
        }
        let alignment = loader.u32(0x10)?;
        let name = loader.string(0x14)?;
        let is_tex2 = name.contains(".Tex2");
        let is_tex1 = name.contains(".Tex1");

        let mut only_textures = true;
        for group in 0..GROUP_COUNT {
            if group != TEXTURE_GROUP
                && (loader.u32(0x20 + group * 4)? != 0
                    || loader.reader.read_u16_at(0x50 + group * 2)? != 0)
            {
                only_textures = false;
            }
        }

        let mut textures = Vec::new();
        if let Some(dict) = loader.offset(0x20 + TEXTURE_GROUP * 4)? {
            let count = usize::try_from(loader.reader.read_i32_at(dict + 4)?)
                .map_err(|_| invalid("invalid FTEX dictionary"))?;
            if count > 0xffff {
                return Err(invalid("invalid FTEX dictionary"));
            }
            for index in 1..=count {
                let node = dict + 8 + index * 16;
                let header = loader
                    .offset(node + 12)?
                    .ok_or_else(|| invalid("FTEX dictionary entry has no data"))?;
                textures.push(Self::parse_texture(&loader, header, is_tex1, is_tex2)?);
            }
        }
        if textures.is_empty() {
            return Err(invalid("the BFRES file holds no textures"));
        }

        let mut file = Self {
            version,
            alignment,
            name,
            textures,
            compression: FtexCompression::None,
            original: raw,
            rebuildable: false,
        };
        file.rebuildable = only_textures
            && !is_tex2
            && file.textures.iter().all(|texture| !texture.has_user_data)
            && file
                .rebuild()
                .is_ok_and(|rebuilt| file.is_same_layout(&rebuilt));
        Ok(file)
    }

    fn parse_texture(
        loader: &Loader<'_>,
        at: usize,
        is_tex1: bool,
        is_tex2: bool,
    ) -> io::Result<FtexTexture> {
        let reader = &loader.reader;
        if reader
            .read_bytes_at(at, 4)
            .map_err(|_| invalid("truncated FTEX header"))?
            != b"FTEX"
            || at + FTEX_HEADER_SIZE > reader.len()
        {
            return Err(invalid("invalid FTEX header"));
        }
        let u32_at = |offset: usize| reader.read_u32_at(at + offset);
        let image_size = u32_at(0x24)? as usize;
        let mip_size = u32_at(0x2c)? as usize;
        let mut mip_offsets = [0u32; 13];
        for (index, value) in mip_offsets.iter_mut().enumerate() {
            *value = u32_at(0x44 + index * 4)?;
        }
        let mut regs = [0u32; 5];
        for (index, value) in regs.iter_mut().enumerate() {
            *value = u32_at(0x8c + index * 4)?;
        }
        let comp_sel = reader.read_array_at::<4>(at + 0x88)?;
        // Syroot: a .Tex1 file carries only the image, a .Tex2 file only the
        // mip chain (in the first offset slot).
        let (data, data_offset, mip_data, mip_data_offset) = if is_tex2 {
            let (mip_data, offset) = loader.block(at + 0xb0, mip_size)?;
            (Block::SharedEmpty, 0, mip_data, offset)
        } else if is_tex1 {
            let (data, offset) = loader.block(at + 0xb0, image_size)?;
            (data, offset, Block::SharedEmpty, 0)
        } else {
            let (data, offset) = loader.block(at + 0xb0, image_size)?;
            let (mip_data, mip_offset) = loader.block(at + 0xb4, mip_size)?;
            (data, offset, mip_data, mip_offset)
        };
        Ok(FtexTexture {
            name: loader.string(at + 0xa8)?,
            path: loader.string(at + 0xac)?,
            dim: u32_at(0x04)?,
            width: u32_at(0x08)?,
            height: u32_at(0x0c)?,
            depth: u32_at(0x10)?,
            mip_count: u32_at(0x14)?,
            format: u32_at(0x18)?,
            aa: u32_at(0x1c)?,
            use_: u32_at(0x20)?,
            tile_mode: u32_at(0x34)?,
            swizzle: u32_at(0x38)?,
            alignment: u32_at(0x3c)?,
            pitch: u32_at(0x40)?,
            mip_offsets,
            view_mip_first: u32_at(0x78)?,
            view_mip_count: u32_at(0x7c)?,
            view_slice_first: u32_at(0x80)?,
            view_slice_count: u32_at(0x84)?,
            comp_sel,
            regs,
            array_length: reader.read_u8_at(at + 0xa4)?,
            data,
            mip_data,
            header_offset: at,
            data_offset,
            mip_data_offset,
            has_user_data: u32_at(0xb8)? != 0 || reader.read_u16_at(at + 0xbc)? != 0,
            modified: false,
        })
    }

    /// True when `rebuilt` (the Syroot layout of the unmodified textures)
    /// is the loaded file. The offset fields of empty blocks are left out
    /// of the comparison: they take no room, and a writer that does not
    /// share them the way Syroot does points them somewhere else.
    fn is_same_layout(&self, rebuilt: &[u8]) -> bool {
        if rebuilt.len() != self.original.len() {
            return false;
        }
        let mut ignored = Vec::new();
        for texture in &self.textures {
            for (block, field) in [(&texture.data, 0xb0), (&texture.mip_data, 0xb4)] {
                if *block == Block::SharedEmpty {
                    ignored.push(texture.header_offset + field);
                }
            }
        }
        rebuilt
            .iter()
            .zip(&self.original)
            .enumerate()
            .all(|(position, (left, right))| {
                left == right
                    || ignored
                        .iter()
                        .any(|field| (*field..*field + 4).contains(&position))
            })
    }

    /// Whether a replacement may change a texture's stored size.
    pub fn can_resize(&self) -> bool {
        self.rebuildable
    }

    pub fn find_texture(&self, name: &str) -> Option<usize> {
        self.textures
            .iter()
            .position(|texture| texture.name.eq_ignore_ascii_case(name))
    }

    /// `FTEX.Replace` with the import dialog accepted as shown: the surface
    /// format is kept, a single-level texture stays single-level and any
    /// other gets a full mip chain, the surface is re-tiled as 2D_TILED_THIN1.
    /// Returns a warning when the result is not guaranteed to be
    /// byte-identical to Toolbox's.
    pub fn replace_texture(
        &mut self,
        index: usize,
        image: &RgbaImage,
    ) -> io::Result<Option<String>> {
        let rebuildable = self.rebuildable;
        let texture = self.textures.get_mut(index).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "FTEX texture index is out of range",
            )
        })?;
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the replacement image is empty",
            ));
        }
        let format = texture.format;
        if !rebuildable {
            // The container cannot be laid out again, so the new image has
            // to fit the surface that is already there: same size, same
            // tiling, header untouched.
            let same_layout = (width, height) == (texture.width, texture.height)
                && texture.tile_mode == IMPORT_TILE_MODE
                && texture.dim == DIM_2D
                && texture.depth == 1
                && texture.aa == 0;
            let mip_count = if texture.mip_data().is_empty() {
                1
            } else {
                texture.mip_count
            };
            let surface = if same_layout {
                let (linear, _) = encode_linear_mips(image, format, mip_count)?;
                Some(gx2::create_surface(
                    &linear,
                    IMPORT_TILE_MODE,
                    0,
                    width,
                    height,
                    1,
                    format,
                    (texture.swizzle >> 8) & 7,
                    DIM_2D,
                    mip_count,
                )?)
            } else {
                None
            };
            let Some(surface) = surface.filter(|surface| {
                surface.data.len() == texture.data().len()
                    && surface.mip_data.len() == texture.mip_data().len()
                    && (surface.swizzle & 0xffff) == (texture.swizzle & 0xffff)
            }) else {
                return Err(unsupported(format!(
                    "this BFRES cannot be rebuilt, so {} can only be replaced in place by a {} x {} image on a 2D tiled surface",
                    texture.name, texture.width, texture.height
                )));
            };
            texture.data = Block::Owned(surface.data);
            texture.mip_data = Block::Owned(surface.mip_data);
            texture.modified = true;
            return Ok(Some(
                "the file was patched in place; it holds more than Toolbox-rebuildable textures"
                    .to_string(),
            ));
        }
        let (mip_count, swizzle_pattern) = if texture.mip_count == 1 {
            (1, 0)
        } else {
            (total_mip_count(width, height), (texture.swizzle >> 8) & 7)
        };
        let (linear, exact) = encode_linear_mips(image, format, mip_count)?;
        let surface = gx2::create_surface(
            &linear,
            IMPORT_TILE_MODE,
            0,
            width,
            height,
            1,
            format,
            swizzle_pattern,
            DIM_2D,
            mip_count,
        )?;

        // FTEX.FromGx2Surface + UpdateTex
        let mut mip_offsets = [0u32; 13];
        for (slot, offset) in mip_offsets.iter_mut().zip(&surface.mip_offsets) {
            *slot = *offset;
        }
        texture.data = Block::Owned(surface.data);
        texture.mip_data = Block::Owned(surface.mip_data);
        texture.mip_offsets = mip_offsets;
        texture.pitch = surface.pitch;
        texture.swizzle = surface.swizzle;
        texture.width = width;
        texture.height = height;
        texture.aa = 0;
        texture.alignment = surface.alignment;
        texture.array_length = 1;
        texture.comp_sel = channels_by_format(format);
        texture.depth = 1;
        texture.dim = DIM_2D;
        texture.path = String::new();
        texture.tile_mode = surface.tile_mode;
        texture.use_ = 1;
        texture.view_mip_count = surface.mip_count;
        texture.view_mip_first = 0;
        texture.view_slice_count = 1;
        texture.view_slice_first = 0;
        texture.regs = [0; 5];
        texture.mip_count = surface.mip_count;
        texture.modified = true;
        Ok((!exact).then(|| {
            format!(
                "{} is encoded with a different block compressor or mip filter than Toolbox's; the image matches, the bytes may not",
                gx2::format_name(format)
            )
        }))
    }

    pub fn replace_texture_from_file(
        &mut self,
        index: usize,
        picture: &Path,
    ) -> io::Result<Option<String>> {
        let image = image::open(picture)
            .map_err(|error| invalid(format!("{}: {error}", picture.display())))?
            .to_rgba8();
        self.replace_texture(index, &image)
    }

    /// The raw (uncompressed) BFRES.
    pub fn save_raw(&self) -> io::Result<Vec<u8>> {
        if self.rebuildable {
            self.rebuild()
        } else {
            self.patch()
        }
    }

    /// The file as it goes back to disk: Yaz0 (TrueYZ, like Toolbox) when it
    /// was loaded compressed.
    pub fn save(&self) -> io::Result<Vec<u8>> {
        let raw = self.save_raw()?;
        Ok(match self.compression {
            FtexCompression::None => raw,
            FtexCompression::Yaz0(alignment) => {
                crate::compression::trueyz::compress(&raw, alignment)
            }
        })
    }

    /// Writes edited image data over its original slot; the layout was
    /// checked by [`Self::replace_texture`] and the headers stay as they are.
    fn patch(&self) -> io::Result<Vec<u8>> {
        let mut writer = BinaryWriter::from_vec(self.original.clone(), Endian::Big);
        for texture in self.textures.iter().filter(|texture| texture.modified) {
            for (offset, bytes) in [
                (texture.data_offset, texture.data()),
                (texture.mip_data_offset, texture.mip_data()),
            ] {
                if bytes.is_empty() {
                    continue;
                }
                if offset == 0 || offset + bytes.len() > self.original.len() {
                    return Err(invalid("FTEX image data lies outside the file"));
                }
                writer.seek(offset);
                writer.write_bytes(bytes);
            }
        }
        Ok(writer.into_inner())
    }

    /// `ResFileSaver.Execute` for a file that holds only textures.
    fn rebuild(&self) -> io::Result<Vec<u8>> {
        let mut saver = Saver::default();
        let writer = &mut saver.writer;
        writer.write_bytes(b"FRES");
        writer.write_u32(self.version);
        writer.write_u16(0xfeff);
        writer.write_u16(0x0010);
        writer.write_u32(0); // file size
        writer.write_u32(self.alignment);
        saver.save_string(&self.name);
        let string_pool_field = saver.writer.position();
        saver.writer.write_zeros(8);
        let mut dict_field = 0;
        for group in 0..GROUP_COUNT {
            if group == TEXTURE_GROUP {
                dict_field = saver.writer.position();
            }
            saver.writer.write_u32(0);
        }
        for group in 0..GROUP_COUNT {
            let count = if group == TEXTURE_GROUP {
                self.textures.len() as u16
            } else {
                0
            };
            saver.writer.write_u16(count);
        }
        saver.writer.write_u32(0); // user pointer

        // The texture dictionary, then every FTEX header it references.
        saver.align(4);
        saver.satisfy(&[dict_field], saver.writer.position());
        let nodes = dict_nodes(self.textures.iter().map(|texture| texture.name.as_str()))?;
        saver.writer.write_u32((8 + nodes.len() * 16) as u32);
        saver.writer.write_i32(self.textures.len() as i32);
        let mut value_fields = Vec::with_capacity(self.textures.len());
        for (index, node) in nodes.iter().enumerate() {
            saver.writer.write_u32(node.reference);
            saver.writer.write_u16(node.left);
            saver.writer.write_u16(node.right);
            if index == 0 {
                saver.writer.write_zeros(8);
            } else {
                saver.save_string(&self.textures[index - 1].name);
                value_fields.push(saver.writer.position());
                saver.writer.write_u32(0);
            }
        }
        let is_tex1 = self.name.contains(".Tex1");
        for (texture, field) in self.textures.iter().zip(value_fields) {
            saver.align(4);
            saver.satisfy(&[field], saver.writer.position());
            saver.writer.write_bytes(b"FTEX");
            write_surface_fields(&mut saver.writer, texture);
            saver.writer.write_u32(0); // handle
            saver.writer.write_u8(texture.array_length);
            saver.writer.write_zeros(3);
            saver.save_string(&texture.name);
            saver.save_string(&texture.path);
            saver.save_block(&texture.data, self.alignment);
            if is_tex1 {
                saver.writer.write_u32(0);
            } else {
                saver.save_block(&texture.mip_data, self.alignment);
            }
            saver.writer.write_u32(0); // user data dictionary
            saver.writer.write_u16(0); // user data count
            saver.writer.write_zeros(2);
        }

        saver.write_strings(string_pool_field);
        saver.write_blocks();
        let mut data = saver.writer.into_inner();
        // An alignment seek that nothing is written after does not grow the
        // stream.
        data.truncate(saver.length);
        let size = data.len() as u32;
        data[0x0c..0x10].copy_from_slice(&size.to_be_bytes());
        Ok(data)
    }
}

/// The FTEX fields from `dim` to the texture registers (header + 4 .. + 0xa0).
fn write_surface_fields(writer: &mut BinaryWriter, texture: &FtexTexture) {
    writer.write_u32(texture.dim);
    writer.write_u32(texture.width);
    writer.write_u32(texture.height);
    writer.write_u32(texture.depth);
    writer.write_u32(texture.mip_count);
    writer.write_u32(texture.format);
    writer.write_u32(texture.aa);
    writer.write_u32(texture.use_);
    writer.write_u32(texture.data().len() as u32);
    writer.write_u32(0); // image pointer
    writer.write_u32(texture.mip_data().len() as u32);
    writer.write_u32(0); // mip pointer
    writer.write_u32(texture.tile_mode);
    writer.write_u32(texture.swizzle);
    writer.write_u32(texture.alignment);
    writer.write_u32(texture.pitch);
    for offset in texture.mip_offsets {
        writer.write_u32(offset);
    }
    writer.write_u32(texture.view_mip_first);
    writer.write_u32(texture.view_mip_count);
    writer.write_u32(texture.view_slice_first);
    writer.write_u32(texture.view_slice_count);
    writer.write_bytes(&texture.comp_sel);
    for reg in texture.regs {
        writer.write_u32(reg);
    }
}

struct DictNode {
    reference: u32,
    left: u16,
    right: u16,
}

/// `ResDict.UpdateNodes`: the Patricia trie over the keys, root included.
fn dict_nodes<'a>(keys: impl Iterator<Item = &'a str>) -> io::Result<Vec<DictNode>> {
    let keys: Vec<Vec<u16>> = std::iter::once(Vec::new())
        .chain(keys.map(|key| key.encode_utf16().collect()))
        .collect();
    let direction = |key: &[u16], reference: u32| -> u32 {
        let walk = (reference >> 3) as usize;
        key.get(walk)
            .map_or(0, |unit| (u32::from(*unit) >> (reference & 7)) & 1)
    };
    let mut nodes: Vec<DictNode> = keys
        .iter()
        .map(|_| DictNode {
            reference: u32::MAX,
            left: 0,
            right: 0,
        })
        .collect();
    for index in 1..nodes.len() {
        let key = &keys[index];
        let next = |nodes: &[DictNode], child: usize, reference: u32| -> usize {
            if direction(key, reference) == 1 {
                usize::from(nodes[child].right)
            } else {
                usize::from(nodes[child].left)
            }
        };
        let mut parent = 0usize;
        let mut child = usize::from(nodes[0].left);
        while nodes[parent].reference > nodes[child].reference {
            parent = child;
            child = next(&nodes, child, nodes[child].reference);
        }
        let mut reference = (key.len().max(keys[child].len()) * 8) as u32;
        while direction(&keys[child], reference) == direction(key, reference) {
            if reference == 0 {
                return Err(invalid(format!(
                    "duplicate texture name {}",
                    String::from_utf16_lossy(key)
                )));
            }
            reference -= 1;
        }
        nodes[index].reference = reference;

        parent = 0;
        child = usize::from(nodes[0].left);
        while nodes[parent].reference > nodes[child].reference && nodes[child].reference > reference
        {
            parent = child;
            child = next(&nodes, child, nodes[child].reference);
        }
        if direction(key, reference) == 1 {
            nodes[index].left = child as u16;
            nodes[index].right = index as u16;
        } else {
            nodes[index].left = index as u16;
            nodes[index].right = child as u16;
        }
        if direction(key, nodes[parent].reference) == 1 {
            nodes[parent].right = index as u16;
        } else {
            nodes[parent].left = index as u16;
        }
    }
    Ok(nodes)
}

struct SavedBlock {
    /// `None` is the shared empty array.
    data: Option<Vec<u8>>,
    alignment: u32,
    fields: Vec<usize>,
}

/// The parts of Syroot's `ResFileSaver` a texture archive goes through.
struct Saver {
    writer: BinaryWriter,
    /// Stream length: the furthest byte actually written.
    length: usize,
    /// String -> the offset fields that point at it, in first-use order.
    strings: Vec<(String, Vec<usize>)>,
    blocks: Vec<SavedBlock>,
}

impl Default for Saver {
    fn default() -> Self {
        Self {
            writer: BinaryWriter::with_endian(Endian::Big),
            length: 0,
            strings: Vec::new(),
            blocks: Vec::new(),
        }
    }
}

impl Saver {
    /// Seeks forward to the next multiple of `alignment`.
    fn align(&mut self, alignment: usize) {
        self.length = self.length.max(self.writer.position());
        let position = self.writer.position().next_multiple_of(alignment.max(1));
        if position > self.writer.len() {
            let end = self.writer.len();
            self.writer.seek(end);
            self.writer.write_zeros(position - end);
        }
        self.writer.seek(position);
    }

    /// Writes the self-relative offset to `target` into every field.
    fn satisfy(&mut self, fields: &[usize], target: usize) {
        for field in fields {
            self.writer
                .write_u32_at(*field, (target as i64 - *field as i64) as u32);
        }
    }

    fn save_string(&mut self, text: &str) {
        let field = self.writer.position();
        match self.strings.iter_mut().find(|(saved, _)| saved == text) {
            Some((_, fields)) => fields.push(field),
            None => self.strings.push((text.to_string(), vec![field])),
        }
        self.writer.write_u32(u32::MAX);
    }

    fn save_block(&mut self, block: &Block, alignment: u32) {
        let field = self.writer.position();
        match block {
            Block::Null => {
                self.writer.write_u32(0);
                return;
            }
            Block::SharedEmpty => match self.blocks.iter_mut().find(|saved| saved.data.is_none()) {
                Some(saved) => saved.fields.push(field),
                None => self.blocks.push(SavedBlock {
                    data: None,
                    alignment,
                    fields: vec![field],
                }),
            },
            Block::Owned(data) => self.blocks.push(SavedBlock {
                data: Some(data.clone()),
                alignment,
                fields: vec![field],
            }),
        }
        self.writer.write_u32(u32::MAX);
    }

    /// `WriteStrings`: ordinal order with the empty string last, each entry
    /// a length prefix, the text and its terminator, padded to 4 bytes.
    fn write_strings(&mut self, pool_field: usize) {
        let mut strings = std::mem::take(&mut self.strings);
        strings.sort_by(|(left, _), (right, _)| {
            left.is_empty()
                .cmp(&right.is_empty())
                .then_with(|| left.encode_utf16().cmp(right.encode_utf16()))
        });
        self.align(4);
        let pool = self.writer.position();
        for (text, fields) in &strings {
            self.writer.write_u32(text.encode_utf16().count() as u32);
            let target = self.writer.position();
            self.satisfy(fields, target);
            self.writer.write_bytes(text.as_bytes());
            self.writer.write_u8(0);
            self.align(4);
        }
        // SetLength(Position): the last alignment does grow the file.
        self.length = self.writer.position();
        let size = (self.writer.position() - pool) as u32;
        self.writer.write_u32_at(pool_field, size);
        self.satisfy(&[pool_field + 4], pool);
    }

    fn write_blocks(&mut self) {
        let blocks = std::mem::take(&mut self.blocks);
        for block in &blocks {
            if block.alignment != 0 {
                self.align(block.alignment as usize);
            }
            let target = self.writer.position();
            self.satisfy(&block.fields, target);
            if let Some(data) = &block.data {
                self.writer.write_bytes(data);
            }
            self.length = self.length.max(self.writer.position());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> Option<Vec<u8>> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/_CLAUDE/bitemico")
            .join(name);
        std::fs::read(path).ok()
    }

    fn scratch_image(name: &str) -> Option<RgbaImage> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/_CLAUDE/bitemico")
            .join(name);
        image::open(path).ok().map(|image| image.to_rgba8())
    }

    /// Files written by Toolbox resave to what Toolbox makes of them, raw
    /// and Yaz0.
    #[test]
    fn toolbox_files_resave_like_toolbox() {
        for (source, expected) in [
            ("ref_resave", "ref_resave"),
            ("base_bc1", "base_bc1"),
            ("base_bc3m", "base_bc3m"),
            ("base_bc4", "base_bc4"),
            ("base_bc5", "base_bc5"),
            ("base_rgba_m", "base_rgba_m"),
            // three textures with an empty mip block each: Syroot folds
            // the blocks into one on a resave
            ("base_two", "ref_two_resave"),
            ("ref_two_resave", "ref_two_resave"),
        ] {
            let (Some(source_bytes), Some(raw), Some(packed)) = (
                scratch(&format!("{source}.sbitemico")),
                scratch(&format!("{expected}.bfres")),
                scratch(&format!("{expected}.sbitemico")),
            ) else {
                continue;
            };
            let file = FtexFile::from_bytes(&source_bytes).unwrap();
            assert!(file.can_resize(), "{source} is not rebuildable");
            assert_eq!(file.save_raw().unwrap(), raw, "{source} raw");
            assert_eq!(file.save().unwrap(), packed, "{source} yaz0");
        }
    }

    /// Replacing one of several textures, block formats and mip chains.
    #[test]
    fn replacements_in_other_layouts_match_toolbox() {
        for (source, picture, texture, expected, exact) in [
            ("base_two", "small.png", "Zeta", "ref_two_replace", true),
            ("base_bc1", "wide.png", "", "ref_bc1_replace", true),
            ("base_bc4", "same.png", "", "ref_bc4_replace", true),
            ("base_bc5", "same.png", "", "ref_bc5_replace", true),
            ("base_rgba_m", "wide.png", "", "ref_rgba_m_replace", true),
            ("base_bc3m", "small.png", "", "ref_bc3m_replace", true),
            ("base_bc3m", "big.png", "", "ref_bc3m_big", true),
            ("base_bc2", "same.png", "", "ref_bc2_replace", true),
        ] {
            let (Some(source_bytes), Some(image), Some(expected_bytes)) = (
                scratch(&format!("{source}.sbitemico")),
                scratch_image(picture),
                scratch(&format!("{expected}.bfres")),
            ) else {
                continue;
            };
            let mut file = FtexFile::from_bytes(&source_bytes).unwrap();
            let index = file.find_texture(texture).unwrap_or(0);
            let warning = file.replace_texture(index, &image).unwrap();
            let saved = file.save_raw().unwrap();
            if exact {
                let differing = saved
                    .iter()
                    .zip(&expected_bytes)
                    .filter(|(left, right)| left != right)
                    .count();
                assert_eq!(saved.len(), expected_bytes.len(), "{expected} size");
                assert_eq!(
                    differing, 0,
                    "{expected}: {differing} bytes differ ({warning:?})"
                );
            } else {
                // BC3 has no bit-exact encoder here: same layout, same header.
                assert!(warning.is_some(), "{expected}");
                assert_eq!(saved.len(), expected_bytes.len(), "{expected} size");
                let header = file.textures[index].header_offset + FTEX_HEADER_SIZE;
                assert_eq!(
                    saved[..header],
                    expected_bytes[..header],
                    "{expected} header"
                );
            }
        }
    }

    /// PNG replacement of an RGBA8 texture matches Toolbox for every size.
    #[test]
    fn png_replacement_matches_toolbox() {
        let Some(source) = scratch("ref_resave.sbitemico") else {
            return;
        };
        for name in ["same", "big", "small", "wide"] {
            let (Some(image), Some(expected)) = (
                scratch_image(&format!("{name}.png")),
                scratch(&format!("ref_{name}.sbitemico")),
            ) else {
                continue;
            };
            let mut file = FtexFile::from_bytes(&source).unwrap();
            let warning = file.replace_texture(0, &image).unwrap();
            assert_eq!(warning, None, "{name}");
            assert_eq!(file.save().unwrap(), expected, "{name}");
            let decoded = FtexFile::from_bytes(&expected).unwrap().textures[0]
                .decode(0, 0)
                .unwrap();
            assert_eq!(decoded.dimensions(), image.dimensions(), "{name}");
        }
    }

    #[test]
    fn dictionary_of_one_key_matches_the_game_file() {
        let nodes = dict_nodes(["Armor_878_Head"].into_iter()).unwrap();
        assert_eq!((nodes[0].left, nodes[0].right), (1, 0));
        assert_eq!(nodes[1].reference, 0x6e);
        assert_eq!((nodes[1].left, nodes[1].right), (0, 1));
    }
}
