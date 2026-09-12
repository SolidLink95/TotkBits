//! Port of `BntxFileLoader` + the `IResData.Load` implementations of
//! `BntxFile`, `Texture` and `ResDict`. Only what the saver needs is kept,
//! but every string is loaded through the same path so `StringTable.Strings`
//! (referenced strings keyed by offset) matches Syroot's.

use super::model::{err, BntxFile, OriginalImageData, Result, Texture};
use std::collections::BTreeMap;

struct Reader<'a> {
    data: &'a [u8],
    strings: BTreeMap<u64, String>,
}

impl<'a> Reader<'a> {
    fn bytes(&self, at: usize, len: usize) -> Result<&'a [u8]> {
        self.data
            .get(at..at.checked_add(len).unwrap_or(usize::MAX))
            .ok_or_else(|| {
                super::model::BntxToolboxError(format!("read past end of file at 0x{at:x}"))
            })
    }

    fn u8(&self, at: usize) -> Result<u8> {
        Ok(self.bytes(at, 1)?[0])
    }

    fn u16(&self, at: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(at, 2)?.try_into().unwrap()))
    }

    fn u32(&self, at: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(at, 4)?.try_into().unwrap()))
    }

    fn i32(&self, at: usize) -> Result<i32> {
        Ok(i32::from_le_bytes(self.bytes(at, 4)?.try_into().unwrap()))
    }

    fn i64(&self, at: usize) -> Result<i64> {
        Ok(i64::from_le_bytes(self.bytes(at, 8)?.try_into().unwrap()))
    }

    fn offset(&self, at: usize) -> Result<usize> {
        let value = self.i64(at)?;
        usize::try_from(value)
            .map_err(|_| super::model::BntxToolboxError(format!("negative offset at 0x{at:x}")))
    }

    fn signature(&self, at: usize, expected: &[u8; 4]) -> Result<()> {
        if self.bytes(at, 4)? != expected {
            return err(format!(
                "invalid signature at 0x{at:x}, expected {}",
                String::from_utf8_lossy(expected)
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
            return err("negative dictionary count");
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

pub fn load(data: &[u8]) -> Result<BntxFile> {
    let mut reader = Reader {
        data,
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
    let target: [u8; 4] = reader.bytes(0x20, 4)?.try_into().unwrap();
    let texture_count = reader.i32(0x24)?;
    if texture_count < 0 {
        return err("negative texture count");
    }
    let texture_array_offset = reader.offset(0x28)?;

    if rlt_offset > data.len() {
        return err("relocation table offset past end of file");
    }
    let original_rlt_chunk = data[rlt_offset..].to_vec();

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
        .map(|texture| OriginalImageData {
            name: texture.name.clone(),
            mip_count: texture.mip_count,
            array_count: texture.array_length,
            image_size: texture.image_size,
            user_data_count: 0,
        })
        .collect();

    Ok(BntxFile {
        original_file_name: name.clone(),
        name,
        version,
        byte_order,
        alignment,
        target_address_size,
        flag,
        target,
        textures,
        string_table: reader.strings,
        original_rlt_chunk,
        original_file_data,
    })
}

fn load_texture(reader: &mut Reader<'_>, at: usize) -> Result<Texture> {
    reader.signature(at, b"BRTI")?;
    let p = at + 16; // after the header block (u32 next offset + u64 size)
    let flags = reader.u8(p)?;
    let dim = reader.u8(p + 1)?;
    let tile_mode = reader.u16(p + 2)?;
    let swizzle = reader.u16(p + 4)? as u32;
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
    let name = reader.load_string(p + 0x50)?;
    // p + 0x58: parent offset, p + 0x60: mip offsets, p + 0x68: user data,
    // p + 0x70 / 0x78 / 0x80: texture / view / descriptor, p + 0x88: user data dict
    let mip_offsets_ptr = reader.offset(p + 0x60)?;
    let user_data_count = reader.load_dict(p + 0x88)?;
    if user_data_count != 0 {
        return err(format!(
            "texture {name} carries user data, which the Toolbox-compatible saver does not support"
        ));
    }
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
                    .map_err(|_| super::model::BntxToolboxError("negative mip offset".into()))?;
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

    Ok(Texture {
        name,
        flags,
        dim,
        tile_mode,
        swizzle,
        mip_count,
        sample_count,
        format,
        access_flags,
        width,
        height,
        depth,
        array_length,
        texture_layout,
        texture_layout2,
        image_size,
        alignment,
        channel_red: (channels & 0xff) as u8,
        channel_green: ((channels >> 8) & 0xff) as u8,
        channel_blue: ((channels >> 16) & 0xff) as u8,
        channel_alpha: ((channels >> 24) & 0xff) as u8,
        surface_dim,
        mip_offsets,
        texture_data,
        read_texture_layout: (flags & 1) as i32,
        sparse_binding: (flags >> 1) as i32,
        sparse_residency: (flags >> 2) as i32,
        block_height_log2: texture_layout & 7,
    })
}
