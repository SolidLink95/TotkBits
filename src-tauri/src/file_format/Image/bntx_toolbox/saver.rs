//! Port of `BntxFileSaver.Execute` together with the `IResData.Save`
//! implementations of `BntxFile`, `ResDict` and `Texture`, reproducing the
//! exact byte layout Switch Toolbox writes (including its quirks: strings are
//! reserved first and rewritten at the end, the vanilla `_RLT` chunk is
//! reused while nothing structural changed, otherwise a generic one-entry-
//! per-pointer relocation table is generated).

use super::model::{BntxFile, Result, Texture};
use crate::file_format::Model3D::bfres::toolbox::patricia;
use std::collections::BTreeMap;

const SECTION_1: u32 = 1;
const SECTION_2: u32 = 2;

/// `BinaryDataWriter` over a `MemoryStream`: writing past the end grows the
/// buffer with zeros, seeking past the end does not.
struct Writer {
    buf: Vec<u8>,
    pos: usize,
}

impl Writer {
    fn new() -> Self {
        Writer {
            buf: Vec::new(),
            pos: 0,
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        let end = self.pos + bytes.len();
        if end > self.buf.len() {
            self.buf.resize(end, 0);
        }
        self.buf[self.pos..end].copy_from_slice(bytes);
        self.pos = end;
    }

    fn u8(&mut self, value: u8) {
        self.write(&[value]);
    }

    fn u16(&mut self, value: u16) {
        self.write(&value.to_le_bytes());
    }

    fn i16(&mut self, value: i16) {
        self.write(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.write(&value.to_le_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.write(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn zeros(&mut self, count: usize) {
        let end = self.pos + count;
        if end > self.buf.len() {
            self.buf.resize(end, 0);
        }
        self.buf[self.pos..end].fill(0);
        self.pos = end;
    }

    fn align(&mut self, alignment: usize) {
        self.pos = self.pos.div_ceil(alignment) * alignment;
    }

    fn at<T>(&mut self, position: usize, f: impl FnOnce(&mut Writer) -> T) -> T {
        let saved = self.pos;
        self.pos = position;
        let result = f(self);
        self.pos = saved;
        result
    }

    fn len(&self) -> usize {
        self.buf.len()
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
    /// `BntxFile.TextureArrayOffset` / `TextureDictOffset`.
    texture_array_offset: i64,
    texture_dict_offset: i64,
}

pub fn save(file: &BntxFile) -> Result<Vec<u8>> {
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
    Ok(saver.w.buf)
}

impl<'a> Saver<'a> {
    fn position(&self) -> i64 {
        self.w.pos as i64
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
        self.w.pos += 16;
        let data_alignment = self.file.data_alignment() as i32;
        let padding = round_up_i32(self.position() as i32, data_alignment) - self.position() as i32;
        if padding > 0 {
            self.w.pos = (self.position() + (padding - 16) as i64) as usize;
        }
        self.data_block_position = self.position();
        self.write_texture_block();

        let can_write = self.setup_relocation_table();
        self.write_relocation_table(can_write);

        let string_pool = self.ofs_string_pool as usize;
        {
            let saved = self.w.pos;
            self.w.pos = string_pool;
            self.write_strings();
            self.w.pos = saved;
        }

        let count = self.header_block_positions.len();
        for index in 0..count {
            let block = self.header_block_positions[index];
            self.w.pos = block as usize;
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

        self.w.pos = self.ofs_texture_data_block as usize;
        let data_block_position = self.data_block_position;
        self.w.i64(data_block_position);

        self.w.pos = self.ofs_file_size as usize;
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
        self.w.u32(file.version);
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
        self.w.write(&file.target);
        self.w.i32(file.textures.len() as i32);
        let pos = self.position();
        self.save_relocate_entry_to_section(pos, 1, 2, 1, SECTION_1);
        self.texture_array_offset = self.save_offset();
        let pos = self.position();
        self.save_relocate_entry_to_section(pos, 1, 1, 0, SECTION_2);
        // SaveTextureDataBlocks
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
        let mut push = |strings: &mut Vec<String>, value: &str| {
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

    /// `Texture.Save`.
    fn save_texture(&mut self, texture: &Texture) {
        let channels = ((texture.channel_alpha as u32) << 24)
            | ((texture.channel_blue as u32) << 16)
            | ((texture.channel_green as u32) << 8)
            | texture.channel_red as u32;
        let texture_layout = if texture.read_texture_layout != 1 {
            0
        } else if self.file.version_major2() == 4 && self.file.version_minor() >= 1 {
            texture.block_height_log2
        } else {
            ((texture.sparse_residency as u32) << 5)
                | ((texture.sparse_binding as u32) << 4)
                | texture.block_height_log2
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
        self.w.i32(texture.alignment);
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
            texture.mip_count,
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

    /// `WriteTextureBlock`: the `BRTD` block with every slice of every texture.
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
