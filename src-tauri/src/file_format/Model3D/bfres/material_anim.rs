//! BFRES material animations (`FMAA`): the texture-pattern animations that
//! armor dyeing plays (`Head_ftp`, `Upper_ftp`, `Lower_ftp`) to switch a
//! material's albedo between the sixteen slices of its `_Alb.N` array.
//!
//! The writer reproduces the layout of the game's own `Model/<project>.anim.bfres`
//! files byte for byte (verified against `Armor_001.anim.bfres`): a version
//! 10 header, the sorted `FMAA` array and its dictionary, one data block per
//! animation (bind indices, material anim data, texture bind array, runtime
//! texture pointer array, texture name array, then per material a pattern
//! info, a step-int curve, its byte frames and signed-byte keys), the string
//! pool in ResDict bit order, and a relocation table whose entries are the
//! run-length grouping the game's exporter produces.

use super::toolbox::patricia::build_nodes;
use super::BfresError;
use crate::parser::binary::{BinaryReader, BinaryWriter};
use std::collections::BTreeMap;

/// One material inside a texture-pattern animation: `textures[frame]` is the
/// texture the sampler shows at that frame.
#[derive(Clone, Debug, PartialEq)]
pub struct TexturePatternMaterial {
    pub material: String,
    /// The material's *sampler* name (`_a0` for the albedo sampler in TOTK's
    /// armor materials), not a texture slot index: the vanilla pattern info
    /// stores the sampler name and leaves `SubBindIndex` at -1.
    pub sampler: String,
    pub textures: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TexturePatternAnim {
    pub name: String,
    /// The `FrameCount` field as stored: the last frame index, so a
    /// texture-per-frame pattern over 16 textures stores 15.
    pub frame_count: u32,
    pub materials: Vec<TexturePatternMaterial>,
}

impl TexturePatternAnim {
    /// An animation that shows `textures[i]` at frame `i` for every material.
    pub fn texture_per_frame(
        name: impl Into<String>,
        materials: Vec<TexturePatternMaterial>,
    ) -> Self {
        let frames = materials
            .iter()
            .map(|material| material.textures.len())
            .max()
            .unwrap_or(1)
            .max(1);
        Self {
            name: name.into(),
            frame_count: (frames - 1) as u32,
            materials,
        }
    }
}

const VERSION: u32 = 0x000A_0000;
/// Header bytes 0x0E/0x0F of the game's animation files.
const ALIGNMENT: u8 = 3;
const TARGET_ADDRESS_SIZE: u8 = 0;
const HEADER_SIZE: usize = 0xF0;
const ANIM_SIZE: usize = 0x70;
const MATERIAL_DATA_SIZE: usize = 0x40;
const PATTERN_INFO_SIZE: usize = 0x10;
const CURVE_SIZE: usize = 0x30;
/// `FrameType::Byte | KeyType::SByte | CurveType::StepInt`.
const CURVE_FLAGS: u16 = 0x004A;
/// `AnimCurve.CalculateBakeSize` for a Switch step-int curve.
fn baked_size(frames: u32) -> u32 {
    frames + 1 + 12 + 4
}

fn error(offset: usize, message: impl Into<String>) -> BfresError {
    BfresError::new(offset, message)
}

/// ResDict key order: characters compared from the last one backwards, each
/// character's bits from the least significant up; a missing character reads
/// as zero bits. The game's string pool is emitted in this order.
fn bit_order(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let bits = a.len().max(b.len()) * 8;
    let bit = |s: &[u8], i: usize| -> u8 {
        let index = i / 8;
        if index < s.len() {
            (s[s.len() - 1 - index] >> (i % 8)) & 1
        } else {
            0
        }
    };
    for i in 0..bits {
        match bit(a, i).cmp(&bit(b, i)) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    a.len().cmp(&b.len())
}

struct Writer {
    out: BinaryWriter,
    /// `(position of the u64 pointer, string)` fixed up once the pool exists.
    string_refs: Vec<(usize, String)>,
    /// Every relocatable pointer word.
    pointers: Vec<usize>,
    strings: Vec<String>,
}

impl Writer {
    fn pos(&self) -> usize {
        self.out.position()
    }
    fn u8(&mut self, v: u8) {
        self.out.write_u8(v);
    }
    fn u16(&mut self, v: u16) {
        self.out.write_u16(v);
    }
    fn u32(&mut self, v: u32) {
        self.out.write_u32(v);
    }
    fn u64(&mut self, v: u64) {
        self.out.write_u64(v);
    }
    fn f32(&mut self, v: f32) {
        self.out.write_f32(v);
    }
    fn bytes(&mut self, v: &[u8]) {
        self.out.write_bytes(v);
    }
    fn align(&mut self, alignment: usize) -> Result<(), BfresError> {
        self.out
            .align(alignment)
            .map_err(|e| error(self.out.position(), e.to_string()))
    }
    /// A relocatable pointer written later.
    fn pointer(&mut self) -> usize {
        let at = self.pos();
        self.pointers.push(at);
        self.u64(0);
        at
    }
    /// A relocatable pointer to a string.
    fn string(&mut self, value: &str) {
        let at = self.pointer();
        self.string_refs.push((at, value.to_owned()));
        if !self.strings.iter().any(|s| s == value) {
            self.strings.push(value.to_owned());
        }
    }
    fn patch_u64(&mut self, at: usize, value: u64) {
        self.out.write_u64_at(at, value);
    }
    fn patch_u32(&mut self, at: usize, value: u32) {
        self.out.write_u32_at(at, value);
    }
    fn patch_u16(&mut self, at: usize, value: u16) {
        self.out.write_u16_at(at, value);
    }
}

struct AnimFixups {
    bind_indices: usize,
    material_data: usize,
    runtime_textures: usize,
    texture_names: usize,
    texture_binds: usize,
}

/// Writes a BFRES that contains only the given material animations.
pub fn write_material_anim_bfres(
    file_name: &str,
    anims: &[TexturePatternAnim],
) -> Result<Vec<u8>, BfresError> {
    if file_name.is_empty() {
        return Err(error(0, "the BFRES file name is empty"));
    }
    if anims.is_empty() {
        return Err(error(0, "no material animations to write"));
    }
    let mut anims: Vec<TexturePatternAnim> = anims.to_vec();
    for anim in &mut anims {
        if anim.name.is_empty() {
            return Err(error(0, "a material animation has no name"));
        }
        if anim.materials.is_empty() {
            return Err(error(0, format!("{} animates no material", anim.name)));
        }
        for material in &anim.materials {
            if material.material.is_empty() || material.sampler.is_empty() {
                return Err(error(
                    0,
                    format!("{} has an unnamed material or sampler", anim.name),
                ));
            }
            if material.textures.is_empty() || material.textures.len() > 128 {
                return Err(error(
                    0,
                    format!(
                        "{}/{} needs 1 to 128 frame textures",
                        anim.name, material.material
                    ),
                ));
            }
            if material.textures.len() as u32 > anim.frame_count + 1 {
                return Err(error(
                    0,
                    format!(
                        "{}/{} has more frame textures than frames",
                        anim.name, material.material
                    ),
                ));
            }
        }
        anim.materials.sort_by(|a, b| a.material.cmp(&b.material));
    }
    anims.sort_by(|a, b| a.name.cmp(&b.name));
    if anims.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return Err(error(0, "duplicate material animation name"));
    }

    let mut w = Writer {
        out: BinaryWriter::new(),
        string_refs: Vec::new(),
        pointers: Vec::new(),
        strings: vec![String::new()],
    };

    // ---- file header ------------------------------------------------------
    w.bytes(b"FRES    ");
    w.u32(VERSION);
    w.bytes(&[0xff, 0xfe]);
    w.u8(ALIGNMENT);
    w.u8(TARGET_ADDRESS_SIZE);
    let ofs_file_name = w.pos();
    w.u32(0);
    w.u16(0); // flags
    let ofs_first_block = w.pos();
    w.u16(0);
    let ofs_relocation_table = w.pos();
    w.u32(0);
    let ofs_file_size = w.pos();
    w.u32(0);
    w.string(file_name); // 0x20
    w.pointer(); // 0x28 model array
    w.pointer(); // 0x30 model dictionary
    w.bytes(&[0; 32]); // 0x38
                       // 0x58: skeletal, material, bone visibility, shape and scene animation
                       // arrays and dictionaries.
    let mut anim_table = [0usize; 10];
    for slot in &mut anim_table {
        *slot = w.pointer();
    }
    w.pointer(); // 0xA8 memory pool
    w.u64(0); // 0xB0 buffer info (not relocated)
    w.pointer(); // 0xB8 external file array
    w.pointer(); // 0xC0 external file dictionary
    w.u64(0); // 0xC8
    let ofs_string_pool = w.pointer(); // 0xD0
    w.u32(0); // 0xD8 string pool size
    w.u32(0); // 0xDC
    w.u16(0); // 0xE0 model count
    w.u16(0); // 0xE2 skeletal animation count
    w.u16(anims.len() as u16); // 0xE4
    w.bytes(&[0; 10]); // 0xE6..0xF0
    if w.pos() != HEADER_SIZE {
        return Err(error(w.pos(), "unexpected header size"));
    }

    // ---- FMAA array -------------------------------------------------------
    let anim_array = w.pos();
    let mut fixups = Vec::with_capacity(anims.len());
    for anim in &anims {
        let curve_count: usize = anim.materials.len();
        let start = w.pos();
        w.bytes(b"FMAA");
        w.u16(0); // flags
        w.u16(0);
        w.string(&anim.name);
        w.string("");
        w.u64(0); // bind model
        let bind_indices = w.pointer();
        let material_data = w.pointer();
        let runtime_textures = w.pointer();
        let texture_names = w.pointer();
        w.pointer(); // user data
        w.pointer(); // user data dictionary
        let texture_binds = w.pointer();
        w.u32(anim.frame_count);
        w.u32(baked_size(anim.frame_count) * curve_count as u32);
        w.u16(0); // user data count
        w.u16(anim.materials.len() as u16);
        w.u16(curve_count as u16);
        w.u16(0); // shader parameter animations
        w.u16(curve_count as u16); // texture pattern animations
        w.u16(0); // visibility animations
        w.u16(texture_list(anim).len() as u16);
        w.u16(0);
        if w.pos() - start != ANIM_SIZE {
            return Err(error(w.pos(), "unexpected FMAA size"));
        }
        fixups.push(AnimFixups {
            bind_indices,
            material_data,
            runtime_textures,
            texture_names,
            texture_binds,
        });
    }

    // ---- FMAA dictionary --------------------------------------------------
    let anim_dict = w.pos();
    let keys: Vec<String> = anims.iter().map(|anim| anim.name.clone()).collect();
    let nodes = build_nodes(&keys);
    w.u32(0);
    w.u32(anims.len() as u32);
    for node in &nodes {
        w.u32(node.reference);
        w.u16(node.left);
        w.u16(node.right);
        let key = node.key.clone().unwrap_or_default();
        w.string(&key);
    }

    // ---- per animation data -----------------------------------------------
    for (anim, fixup) in anims.iter().zip(&fixups) {
        let textures = texture_list(anim);
        w.patch_u64(fixup.bind_indices, w.pos() as u64);
        for _ in &anim.materials {
            w.u16(u16::MAX);
        }
        w.align(8)?;

        w.patch_u64(fixup.material_data, w.pos() as u64);
        let mut material_fixups = Vec::with_capacity(anim.materials.len());
        for (index, material) in anim.materials.iter().enumerate() {
            let start = w.pos();
            w.string(&material.material);
            w.u64(0); // shader parameter infos (not relocated when empty)
            let pattern_infos = w.pointer();
            let curves = w.pointer();
            w.u64(0); // constants (not relocated when empty)
            w.u16(u16::MAX); // shader parameter curve index
            w.u16(index as u16); // texture pattern curve index
            w.u16(u16::MAX); // visual constant index
            w.u16(u16::MAX); // visual curve index
            w.u16(u16::MAX); // begin visual constant index
            w.u16(0); // shader parameter animation count
            w.u16(1); // texture pattern animation count
            w.u16(0); // constant count
            w.u16(1); // curve count
            w.bytes(&[0; 6]);
            if w.pos() - start != MATERIAL_DATA_SIZE {
                return Err(error(w.pos(), "unexpected material animation data size"));
            }
            material_fixups.push((pattern_infos, curves));
        }

        w.patch_u64(fixup.texture_binds, w.pos() as u64);
        for _ in &textures {
            w.u64(u64::MAX);
        }
        w.patch_u64(fixup.runtime_textures, w.pos() as u64);
        for _ in &textures {
            w.u64(0);
        }
        w.patch_u64(fixup.texture_names, w.pos() as u64);
        for texture in &textures {
            w.string(texture);
        }

        for (material, (pattern_infos, curves)) in anim.materials.iter().zip(&material_fixups) {
            // Pattern info.
            w.patch_u64(*pattern_infos, w.pos() as u64);
            w.string(&material.sampler);
            w.u16(0); // curve index
            w.u16(u16::MAX); // begin constant
            w.u8(0xff); // sub bind index
            w.bytes(&[0; 3]);

            // Curve: one key per frame texture, values are indices into the
            // sorted texture name array, stored as signed bytes around the
            // midpoint of the used range.
            let values: Vec<i32> = material
                .textures
                .iter()
                .map(|name| {
                    textures
                        .iter()
                        .position(|candidate| candidate == name)
                        .map(|index| index as i32)
                        .ok_or_else(|| {
                            error(w.pos(), format!("texture {name} is not in the array"))
                        })
                })
                .collect::<Result<_, _>>()?;
            let min = values.iter().copied().min().unwrap_or(0);
            let max = values.iter().copied().max().unwrap_or(0);
            let offset = (min + max) / 2;
            let delta = (max - min) / 2;
            w.patch_u64(*curves, w.pos() as u64);
            let frames_pointer = w.pointer();
            let keys_pointer = w.pointer();
            w.u16(CURVE_FLAGS);
            w.u16(values.len() as u16);
            w.u32(0); // animation data offset
            w.f32(0.0); // start frame
            w.f32((values.len() - 1) as f32); // end frame
            w.f32(0.0); // scale
            w.u32(offset as u32);
            w.u32(delta as u32);
            w.u32(0);
            w.patch_u64(frames_pointer, w.pos() as u64);
            for frame in 0..values.len() {
                w.u8(frame as u8);
            }
            w.align(4)?;
            w.patch_u64(keys_pointer, w.pos() as u64);
            for value in &values {
                let key = value - offset;
                if !(i8::MIN as i32..=i8::MAX as i32).contains(&key) {
                    return Err(error(
                        w.pos(),
                        "texture index does not fit a signed byte key",
                    ));
                }
                w.u8(key as i8 as u8);
            }
            w.align(4)?;
        }
    }

    // ---- string pool ------------------------------------------------------
    w.align(8)?;
    let string_block = w.pos();
    w.bytes(b"_STR");
    w.u32(0);
    let ofs_string_block_size = w.pos();
    w.u64(0);
    let mut strings = w.strings.clone();
    strings.sort_by(|a, b| bit_order(a, b));
    strings.dedup();
    w.u32(strings.iter().filter(|s| !s.is_empty()).count() as u32);
    let pool_start = w.pos();
    let mut positions: BTreeMap<String, usize> = BTreeMap::new();
    for value in &strings {
        if value.len() > u16::MAX as usize {
            return Err(error(w.pos(), format!("string too long: {value}")));
        }
        positions.insert(value.clone(), w.pos());
        w.u16(value.len() as u16);
        w.bytes(value.as_bytes());
        w.u8(0);
        w.align(2)?;
    }
    let string_end = w.pos();
    w.align(8)?;
    let relocation_table = w.pos();
    w.patch_u64(
        ofs_string_block_size,
        (relocation_table - string_block) as u64,
    );

    // Pointer fixups now that every string has a home.
    for (at, value) in std::mem::take(&mut w.string_refs) {
        let position = positions
            .get(&value)
            .copied()
            .ok_or_else(|| error(at, format!("string {value} was not pooled")))?;
        w.patch_u64(at, position as u64);
    }
    let name_position = positions
        .get(file_name)
        .copied()
        .ok_or_else(|| error(0, "file name was not pooled"))?;
    w.patch_u32(ofs_file_name, (name_position + 2) as u32);
    w.patch_u16(ofs_first_block, string_block as u16);
    w.patch_u64(anim_table[2], anim_array as u64);
    w.patch_u64(anim_table[3], anim_dict as u64);
    w.patch_u32(ofs_string_pool, pool_start as u32);
    w.patch_u32(ofs_string_pool + 8, (string_end - string_block) as u32);

    // ---- relocation table -------------------------------------------------
    let entries = relocation_entries(&mut w.pointers);
    w.bytes(b"_RLT");
    w.u32(relocation_table as u32);
    w.u32(5);
    w.u32(0);
    // Section 0 spans the whole file up to the string pool end and owns
    // every entry; the game writes the other four sections empty at that end
    // position with their entry index past the last entry.
    let entry_count = entries.len() as u32;
    let sections: [(u32, u32, u32, u32); 5] = [
        (0, string_end as u32, 0, entry_count),
        (string_end as u32, 0, entry_count, 0),
        (string_end as u32, 0, entry_count, 0),
        (string_end as u32, 0, entry_count, 0),
        (string_end as u32, 0, entry_count, 0),
    ];
    for (position, size, index, count) in sections {
        w.u64(0);
        w.u32(position);
        w.u32(size);
        w.u32(index);
        w.u32(count);
    }
    for entry in &entries {
        w.u32(entry.position as u32);
        w.u16(entry.struct_count as u16);
        w.u8(entry.offset_count as u8);
        w.u8(entry.padding_count as u8);
    }
    w.patch_u32(ofs_relocation_table, relocation_table as u32);
    let size = w.pos();
    w.patch_u32(ofs_file_size, size as u32);
    Ok(w.out.into_inner())
}

/// The sorted, de-duplicated texture names an animation references.
fn texture_list(anim: &TexturePatternAnim) -> Vec<String> {
    let mut textures: Vec<String> = anim
        .materials
        .iter()
        .flat_map(|material| material.textures.iter().cloned())
        .collect();
    textures.sort();
    textures.dedup();
    textures
}

struct RelocationEntry {
    position: usize,
    struct_count: usize,
    offset_count: usize,
    padding_count: usize,
}

/// Groups the pointer words into the game's run-length entries: maximal runs
/// of consecutive pointers become groups, and a group repeats (with the gap
/// as padding) as long as the next unconsumed group of the same size sits at
/// the same stride.
fn relocation_entries(pointers: &mut Vec<usize>) -> Vec<RelocationEntry> {
    pointers.sort_unstable();
    pointers.dedup();
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for &position in pointers.iter() {
        match groups.last_mut() {
            Some((start, count)) if *start + *count * 8 == position => *count += 1,
            _ => groups.push((position, 1)),
        }
    }
    let mut consumed = vec![false; groups.len()];
    let mut entries = Vec::new();
    for i in 0..groups.len() {
        if consumed[i] {
            continue;
        }
        consumed[i] = true;
        let (start, count) = groups[i];
        let next = (i + 1..groups.len()).find(|&j| !consumed[j] && groups[j].1 == count);
        let Some(next) = next else {
            entries.push(RelocationEntry {
                position: start,
                struct_count: 1,
                offset_count: count,
                padding_count: 0,
            });
            continue;
        };
        let stride = groups[next].0 - start;
        let mut struct_count = 1;
        let mut expected = start + stride;
        let mut j = next;
        while j < groups.len() {
            if consumed[j] {
                j += 1;
                continue;
            }
            if groups[j].0 == expected && groups[j].1 == count {
                consumed[j] = true;
                struct_count += 1;
                expected += stride;
                j += 1;
            } else if groups[j].0 > expected {
                break;
            } else {
                j += 1;
            }
        }
        entries.push(RelocationEntry {
            position: start,
            struct_count,
            offset_count: count,
            padding_count: stride / 8 - count,
        });
    }
    entries
}

// ---- reader ----------------------------------------------------------------

/// Reads the texture-pattern animations of an `.anim.bfres`.
pub fn read_material_anim_bfres(
    data: &[u8],
) -> Result<(String, Vec<TexturePatternAnim>), BfresError> {
    let r = BinaryReader::new(data);
    let read = |at: usize, what: &str| -> Result<u64, BfresError> {
        r.read_u64_at(at)
            .map_err(|_| error(at, format!("truncated {what}")))
    };
    let read32 = |at: usize, what: &str| -> Result<u32, BfresError> {
        r.read_u32_at(at)
            .map_err(|_| error(at, format!("truncated {what}")))
    };
    let read16 = |at: usize, what: &str| -> Result<u16, BfresError> {
        r.read_u16_at(at)
            .map_err(|_| error(at, format!("truncated {what}")))
    };
    if r.read_bytes_at(0, 4)
        .map_err(|_| error(0, "truncated header"))?
        != b"FRES"
    {
        return Err(error(0, "not a BFRES file"));
    }
    if read32(8, "version")? != VERSION {
        return Err(error(
            8,
            "only BFRES version 10 material animations are supported",
        ));
    }
    let file_name = res_string(&r, read(0x20, "file name")? as usize)?;
    let anim_array = read(0x68, "material animation array")? as usize;
    let anim_count = read16(0xE4, "material animation count")? as usize;
    let mut anims = Vec::with_capacity(anim_count.min(1024));
    for index in 0..anim_count {
        let base = anim_array
            .checked_add(index * ANIM_SIZE)
            .ok_or_else(|| error(anim_array, "material animation offset overflow"))?;
        if r.read_bytes_at(base, 4)
            .map_err(|_| error(base, "truncated FMAA"))?
            != b"FMAA"
        {
            return Err(error(base, "expected an FMAA section"));
        }
        let name = res_string(&r, read(base + 0x08, "animation name")? as usize)?;
        let material_data = read(base + 0x28, "material data offset")? as usize;
        let texture_names = read(base + 0x38, "texture name array offset")? as usize;
        let frame_count = read32(base + 0x58, "frame count")?;
        let material_count = read16(base + 0x62, "material count")? as usize;
        let texture_count = read16(base + 0x6C, "texture count")? as usize;
        let mut textures = Vec::with_capacity(texture_count.min(1024));
        for t in 0..texture_count {
            textures.push(res_string(
                &r,
                read(texture_names + t * 8, "texture name pointer")? as usize,
            )?);
        }
        let mut materials = Vec::with_capacity(material_count.min(1024));
        for m in 0..material_count {
            let at = material_data
                .checked_add(m * MATERIAL_DATA_SIZE)
                .ok_or_else(|| error(material_data, "material data offset overflow"))?;
            let material = res_string(&r, read(at, "material name")? as usize)?;
            let pattern_infos = read(at + 0x10, "pattern info offset")? as usize;
            let curves = read(at + 0x18, "curve offset")? as usize;
            let pattern_count = read16(at + 0x34, "pattern animation count")? as usize;
            let curve_count = read16(at + 0x38, "curve count")? as usize;
            for p in 0..pattern_count {
                let info = pattern_infos + p * PATTERN_INFO_SIZE;
                let sampler = res_string(&r, read(info, "sampler name")? as usize)?;
                let curve_index = read16(info + 8, "curve index")? as i16;
                if curve_index < 0 || curve_index as usize >= curve_count {
                    return Err(error(
                        info + 8,
                        "pattern info without a curve is not supported",
                    ));
                }
                let curve = curves + curve_index as usize * CURVE_SIZE;
                let frame_textures = read_step_curve(&r, curve, frame_count, &textures)?;
                materials.push(TexturePatternMaterial {
                    material: material.clone(),
                    sampler,
                    textures: frame_textures,
                });
            }
        }
        anims.push(TexturePatternAnim {
            name,
            frame_count,
            materials,
        });
    }
    Ok((file_name, anims))
}

/// Expands a step curve into one texture per frame (`0..=frame_count`).
fn read_step_curve(
    r: &BinaryReader<'_>,
    at: usize,
    frame_count: u32,
    textures: &[String],
) -> Result<Vec<String>, BfresError> {
    let frames_at = r
        .read_u64_at(at)
        .map_err(|_| error(at, "truncated curve"))? as usize;
    let keys_at = r
        .read_u64_at(at + 8)
        .map_err(|_| error(at, "truncated curve"))? as usize;
    let flags = r
        .read_u16_at(at + 16)
        .map_err(|_| error(at, "truncated curve"))?;
    let key_count = r
        .read_u16_at(at + 18)
        .map_err(|_| error(at, "truncated curve"))? as usize;
    let offset = r
        .read_u32_at(at + 36)
        .map_err(|_| error(at, "truncated curve"))? as i32;
    if flags & 0x70 != 0x40 {
        return Err(error(
            at + 16,
            "only step-int texture pattern curves are supported",
        ));
    }
    let mut keyed: Vec<(u32, usize)> = Vec::with_capacity(key_count.min(1024));
    for k in 0..key_count {
        let frame = match flags & 3 {
            0 => r
                .read_f32_at(frames_at + k * 4)
                .map_err(|_| error(frames_at, "truncated frames"))? as u32,
            1 => {
                u32::from(
                    r.read_u16_at(frames_at + k * 2)
                        .map_err(|_| error(frames_at, "truncated frames"))?,
                ) >> 5
            }
            _ => u32::from(
                r.read_u8_at(frames_at + k)
                    .map_err(|_| error(frames_at, "truncated frames"))?,
            ),
        };
        let raw = match flags & 0xC {
            0 => r
                .read_u32_at(keys_at + k * 4)
                .map_err(|_| error(keys_at, "truncated keys"))? as i32,
            4 => i32::from(
                r.read_i16_at(keys_at + k * 2)
                    .map_err(|_| error(keys_at, "truncated keys"))?,
            ),
            _ => i32::from(
                r.read_i8_at(keys_at + k)
                    .map_err(|_| error(keys_at, "truncated keys"))?,
            ),
        };
        let index = usize::try_from(raw + offset)
            .ok()
            .filter(|index| *index < textures.len())
            .ok_or_else(|| error(keys_at + k, "texture pattern key is out of range"))?;
        keyed.push((frame, index));
    }
    let mut result = Vec::with_capacity(frame_count as usize + 1);
    let mut current = keyed.first().map(|(_, index)| *index).unwrap_or(0);
    for frame in 0..=frame_count {
        if let Some((_, index)) = keyed.iter().find(|(f, _)| *f == frame) {
            current = *index;
        }
        result.push(
            textures
                .get(current)
                .cloned()
                .ok_or_else(|| error(keys_at, "texture pattern key is out of range"))?,
        );
    }
    Ok(result)
}

/// A length-prefixed pool string; the pointer targets the `u16` length.
fn res_string(r: &BinaryReader<'_>, at: usize) -> Result<String, BfresError> {
    let length = r
        .read_u16_at(at)
        .map_err(|_| error(at, "truncated string"))? as usize;
    let bytes = r
        .read_bytes_at(at + 2, length)
        .map_err(|_| error(at, "truncated string"))?;
    String::from_utf8(bytes.to_vec()).map_err(|_| error(at, "string is not UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn reference() -> Option<Vec<u8>> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tmp/_CLAUDE/armor_research/Armor_001.anim.bfres");
        std::fs::read(path).ok()
    }

    fn slices(prefix: &str) -> Vec<String> {
        (0..16).map(|i| format!("{prefix}_Alb.{i}")).collect()
    }

    fn hylian_hood_anims() -> Vec<TexturePatternAnim> {
        let material = |name: &str, prefix: &str| TexturePatternMaterial {
            material: name.into(),
            sampler: "_a0".into(),
            textures: slices(prefix),
        };
        vec![
            TexturePatternAnim::texture_per_frame(
                "Head_ftp",
                vec![material("Mt_Hood_001", "Armor_001_Hood")],
            ),
            TexturePatternAnim::texture_per_frame(
                "Upper_ftp",
                vec![
                    material("Mt_Upper_001", "Armor_001_Upper"),
                    material("Mt_Upper_Belt_001", "Armor_001_Upper"),
                    material("Mt_Upper_Kusari_001", "Armor_001_Upper"),
                ],
            ),
            TexturePatternAnim::texture_per_frame(
                "Lower_ftp",
                vec![material("Mt_Lower_001", "Armor_001_Lower")],
            ),
        ]
    }

    #[test]
    fn string_order_follows_resdict_bits() {
        let mut names = vec![
            "Armor_001_Hood_Alb.1",
            "Head_ftp",
            "_a0",
            "Armor_001_Hood_Alb.0",
            "",
            "Armor_001_Hood_Alb.10",
            "Mt_Hood_001",
        ];
        names.sort_by(|a, b| bit_order(a, b));
        assert_eq!(
            names,
            vec![
                "",
                "Armor_001_Hood_Alb.0",
                "_a0",
                "Armor_001_Hood_Alb.10",
                "Head_ftp",
                "Mt_Hood_001",
                "Armor_001_Hood_Alb.1",
            ]
        );
    }

    #[test]
    fn from_scratch_matches_the_vanilla_file_byte_for_byte() {
        let Some(expected) = reference() else {
            return;
        };
        let actual = write_material_anim_bfres("Armor_001.anim", &hylian_hood_anims()).unwrap();
        if actual != expected {
            let _ = std::fs::write(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../tmp/_CLAUDE/armor_research/Armor_001.anim.ours.bfres"),
                &actual,
            );
            let first = actual
                .iter()
                .zip(&expected)
                .position(|(a, b)| a != b)
                .unwrap_or(actual.len().min(expected.len()));
            panic!(
                "output differs from the vanilla file at 0x{first:X} (sizes {} vs {})",
                actual.len(),
                expected.len()
            );
        }
    }

    #[test]
    fn reads_the_vanilla_file_and_rewrites_it_identically() {
        let Some(expected) = reference() else {
            return;
        };
        let (name, anims) = read_material_anim_bfres(&expected).unwrap();
        assert_eq!(name, "Armor_001.anim");
        assert_eq!(anims.len(), 3);
        assert_eq!(anims[0].name, "Head_ftp");
        assert_eq!(anims[0].frame_count, 15);
        assert_eq!(anims[0].materials[0].material, "Mt_Hood_001");
        assert_eq!(anims[0].materials[0].sampler, "_a0");
        assert_eq!(anims[0].materials[0].textures, slices("Armor_001_Hood"));
        assert_eq!(anims[2].name, "Upper_ftp");
        assert_eq!(anims[2].materials.len(), 3);
        assert_eq!(anims[2].materials[1].material, "Mt_Upper_Belt_001");
        assert_eq!(anims[2].materials[2].textures, slices("Armor_001_Upper"));
        let rewritten = write_material_anim_bfres(&name, &anims).unwrap();
        assert_eq!(rewritten, expected);
    }

    #[test]
    fn rejects_empty_and_inconsistent_input() {
        assert!(write_material_anim_bfres("x.anim", &[]).is_err());
        let mut anims = hylian_hood_anims();
        anims[0].materials[0].textures.clear();
        assert!(write_material_anim_bfres("x.anim", &anims).is_err());
        assert!(read_material_anim_bfres(b"not a bfres").is_err());
    }
}
