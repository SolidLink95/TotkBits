//! In-memory model of a BNTX file shaped after Syroot's `BntxFile` /
//! `Texture` classes (the KillzXGaming fork Switch Toolbox ships), including
//! the bookkeeping the fork keeps for its "write the original relocation
//! table back" optimisation.

use std::collections::BTreeMap;
use std::fmt;

/// Error raised by the Toolbox-compatible BNTX loader/saver.
#[derive(Debug)]
pub struct BntxToolboxError(pub String);

impl fmt::Display for BntxToolboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BntxToolboxError {}

impl From<std::io::Error> for BntxToolboxError {
    fn from(error: std::io::Error) -> Self {
        BntxToolboxError(error.to_string())
    }
}

impl From<BntxToolboxError> for std::io::Error {
    fn from(error: BntxToolboxError) -> Self {
        std::io::Error::new(std::io::ErrorKind::InvalidData, error.0)
    }
}

pub type Result<T> = std::result::Result<T, BntxToolboxError>;

pub(super) fn err<T>(message: impl Into<String>) -> Result<T> {
    Err(BntxToolboxError(message.into()))
}

/// `GFX.ChannelType` values.
pub const CHANNEL_ZERO: u8 = 0;
pub const CHANNEL_ONE: u8 = 1;
pub const CHANNEL_RED: u8 = 2;
pub const CHANNEL_GREEN: u8 = 3;
pub const CHANNEL_BLUE: u8 = 4;
pub const CHANNEL_ALPHA: u8 = 5;

/// `GFX.TileMode.LinearAligned`.
pub const TILE_MODE_LINEAR_ALIGNED: u16 = 1;

/// Snapshot Syroot takes of every texture on load (`OriginalImageData`); the
/// saver reuses the vanilla relocation table only while all of it still holds.
#[derive(Clone, Debug)]
pub struct OriginalImageData {
    pub name: String,
    pub mip_count: u32,
    pub array_count: u32,
    pub image_size: u32,
    pub user_data_count: u32,
}

/// One `BRTI` block. Field names follow the C# properties.
#[derive(Clone, Debug)]
pub struct Texture {
    pub name: String,
    pub flags: u8,
    pub dim: u8,
    pub tile_mode: u16,
    pub swizzle: u32,
    pub mip_count: u32,
    pub sample_count: u32,
    pub format: u32,
    pub access_flags: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_length: u32,
    pub texture_layout: u32,
    pub texture_layout2: u32,
    pub image_size: u32,
    pub alignment: i32,
    pub channel_red: u8,
    pub channel_green: u8,
    pub channel_blue: u8,
    pub channel_alpha: u8,
    pub surface_dim: u32,
    /// Mip offsets relative to the first one.
    pub mip_offsets: Vec<i64>,
    /// `TextureData[slice][0]`: the complete (all mips) swizzled data of each
    /// array slice. Syroot only ever writes index 0 of each slice back.
    pub texture_data: Vec<Vec<u8>>,
    pub read_texture_layout: i32,
    pub sparse_binding: i32,
    pub sparse_residency: i32,
    pub block_height_log2: u32,
}

impl Texture {
    /// `Texture.GetTotalSize()`.
    pub fn total_size(&self) -> usize {
        self.texture_data.iter().map(Vec::len).sum()
    }

    /// `Texture.UseSRGB`.
    pub fn use_srgb(&self) -> bool {
        self.format & 0xff == 6
    }

    pub fn is_astc(&self) -> bool {
        crate::file_format::Image::switch_texture::astc_block_from_bntx(self.format).is_some()
    }
}

/// A BNTX file as Syroot's loader leaves it in memory.
#[derive(Clone, Debug)]
pub struct BntxFile {
    pub name: String,
    pub(super) original_file_name: String,
    pub version: u32,
    pub byte_order: u16,
    pub alignment: u8,
    pub target_address_size: u8,
    pub flag: u16,
    pub target: [u8; 4],
    pub textures: Vec<Texture>,
    /// Strings referenced while loading, keyed by their file offset (the
    /// position of the length prefix), i.e. `StringTable.Strings`.
    pub(super) string_table: BTreeMap<u64, String>,
    /// Everything from the original `_RLT` block to the end of the file.
    pub(super) original_rlt_chunk: Vec<u8>,
    pub(super) original_file_data: Vec<OriginalImageData>,
}

impl BntxFile {
    /// `BntxFile.DataAlignment`.
    pub fn data_alignment(&self) -> u32 {
        1u32 << (self.alignment as u32 & 31)
    }

    pub fn version_major2(&self) -> u32 {
        (self.version >> 16) & 0xff
    }

    pub fn version_minor(&self) -> u32 {
        (self.version >> 8) & 0xff
    }

    /// `BntxFile.WriteOriginalRLT()`: true when the vanilla relocation table
    /// can be written back verbatim.
    pub(super) fn write_original_rlt(&self) -> bool {
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
            // Textures with user data always force a rebuilt table (the port
            // rejects user data on load, so this is always false here).
            if original.mip_count != texture.mip_count {
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

    /// `strings_containing`-style diagnostic: every string the file carries.
    pub fn strings(&self) -> Vec<String> {
        let mut strings = vec![self.name.clone()];
        strings.extend(self.textures.iter().map(|texture| texture.name.clone()));
        strings
    }
}
