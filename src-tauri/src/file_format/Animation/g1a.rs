use crate::parser::binary::{BinaryReader, Endian};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::LazyLock,
};

const G1A_HEADER_SIZE: usize = 0x34;
const OLD_CHANNEL_LAYOUT_MAX_VERSION: u32 = 0x3030_3430;
const MAX_CHANNEL_KEYS: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct G1aHeader {
    pub version: String,
    pub chunk_size: u32,
    pub animation_type: u16,
    pub duration: f32,
    pub data_section_offset: u32,
    pub bone_info_count: u16,
    pub bone_max_id: u16,
    pub endian: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct G1aFile {
    pub header: G1aHeader,
    pub bones: Vec<G1aBoneAnimation>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct G1aBoneAnimation {
    pub bone_id: u32,
    pub opcode: u32,
    pub scale: Vec<G1aVectorKeyframe>,
    pub rotation: Vec<G1aQuaternionKeyframe>,
    pub translation: Vec<G1aVectorKeyframe>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct G1aVectorKeyframe {
    pub time: f32,
    pub value: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct G1aQuaternionKeyframe {
    pub time: f32,
    pub value: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundG1aAnimation {
    pub duration: f32,
    pub tracks: Vec<BoundG1aBoneTrack>,
    pub unmapped_bone_ids: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundG1aBoneTrack {
    pub global_bone_id: u32,
    pub bone_index: usize,
    pub bone_name: String,
    pub scale: Vec<G1aVectorKeyframe>,
    pub rotation: Vec<G1aQuaternionKeyframe>,
    pub translation: Vec<G1aVectorKeyframe>,
}

#[derive(Debug, Clone)]
struct SplineChannel {
    values: Vec<[f32; 4]>,
    times: Vec<f32>,
}

impl G1aFile {
    pub fn from_path(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::parse(&std::fs::read(path)?)
    }

    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if matches!(data.get(..4), Some(b"_A2G") | Some(b"G2A_")) {
            return parse_g2a(data);
        }
        let endian = match data.get(..4) {
            Some(b"G1A_") => Endian::Big,
            Some(b"_A1G") => Endian::Little,
            _ => return Err(invalid("not a G1A animation")),
        };
        if data.len() < G1A_HEADER_SIZE {
            return Err(invalid("truncated G1A header"));
        }
        let mut reader = BinaryReader::with_endian(data, endian);
        reader.skip(4)?;
        let version_raw = reader.read_u32()?;
        let chunk_size = reader.read_u32()?;
        if chunk_size != 0 && chunk_size as usize > data.len() {
            return Err(invalid("G1A chunk size exceeds input"));
        }
        let animation_type = reader.read_u16()?;
        reader.skip(2)?;
        let duration = finite(reader.read_f32()?, "animation duration")?;
        let data_section_offset = reader
            .read_u32()?
            .checked_mul(0x10)
            .ok_or_else(|| invalid("G1A data section offset overflow"))?;
        reader.seek(0x30)?;
        let bone_info_count = reader.read_u16()?;
        let bone_max_id = reader.read_u16()?;
        let bone_table_end = G1A_HEADER_SIZE
            .checked_add(bone_info_count as usize * 8)
            .ok_or_else(|| invalid("G1A bone table size overflow"))?;
        if bone_table_end > data.len() {
            return Err(invalid("G1A bone table exceeds input"));
        }

        let mut bones = Vec::with_capacity(bone_info_count as usize);
        for index in 0..bone_info_count as usize {
            reader.seek(G1A_HEADER_SIZE + index * 8)?;
            let bone_id = reader.read_u32()?;
            let spline_units = reader.read_u32()? as usize;
            let spline_offset = 0x30usize
                .checked_add(
                    spline_units
                        .checked_mul(0x10)
                        .ok_or_else(|| invalid("G1A spline offset overflow"))?,
                )
                .ok_or_else(|| invalid("G1A spline offset overflow"))?;
            if let Some(bone) = parse_bone(data, endian, version_raw, bone_id, spline_offset)? {
                bones.push(bone);
            }
        }

        let version = String::from_utf8(version_raw.to_be_bytes().to_vec())
            .unwrap_or_else(|_| format!("{version_raw:08x}"));
        Ok(Self {
            header: G1aHeader {
                version,
                chunk_size,
                animation_type,
                duration,
                data_section_offset,
                bone_info_count,
                bone_max_id,
                endian: match endian {
                    Endian::Little => "little",
                    Endian::Big => "big",
                }
                .into(),
            },
            bones,
        })
    }

    pub fn bind_to_g1m(&self, model: &crate::parser::AOC::g1m::G1mFile) -> BoundG1aAnimation {
        let bone_names: Vec<_> = model
            .render
            .bones
            .iter()
            .map(|bone| bone.name.clone())
            .collect();
        self.bind_to_skeleton(&model.global_to_local_bones, &bone_names)
    }

    pub fn bind_to_skeleton(
        &self,
        global_to_local_bones: &[u16],
        bone_names: &[String],
    ) -> BoundG1aAnimation {
        let mut tracks = Vec::with_capacity(self.bones.len());
        let mut unmapped_bone_ids = Vec::new();
        for bone in &self.bones {
            let Some(&local_index) = usize::try_from(bone.bone_id)
                .ok()
                .and_then(|global_index| global_to_local_bones.get(global_index))
            else {
                unmapped_bone_ids.push(bone.bone_id);
                continue;
            };
            let bone_index = local_index as usize;
            if local_index == u16::MAX || bone_index >= bone_names.len() {
                unmapped_bone_ids.push(bone.bone_id);
                continue;
            }
            tracks.push(BoundG1aBoneTrack {
                global_bone_id: bone.bone_id,
                bone_index,
                bone_name: bone_names[bone_index].clone(),
                scale: bone.scale.clone(),
                rotation: bone.rotation.clone(),
                translation: bone.translation.clone(),
            });
        }
        BoundG1aAnimation {
            duration: self.header.duration,
            tracks,
            unmapped_bone_ids,
        }
    }
}

fn parse_g2a(data: &[u8]) -> io::Result<G1aFile> {
    let endian = match data.get(..4) {
        Some(b"_A2G") => Endian::Little,
        Some(b"G2A_") => Endian::Big,
        _ => return Err(invalid("not a G1A/G2A animation")),
    };
    if data.len() < 0x20 {
        return Err(invalid("truncated G2A header"));
    }
    let reader = BinaryReader::with_endian(data, endian);
    let version_bytes = reader.read_array_at::<4>(4)?;
    let version_raw = match endian {
        Endian::Little => u32::from_le_bytes(version_bytes),
        Endian::Big => u32::from_be_bytes(version_bytes),
    };
    let chunk_size = reader.read_u32_at(8)?;
    if chunk_size != 0 && chunk_size as usize > data.len() {
        return Err(invalid("G2A chunk size exceeds input"));
    }
    let framerate = finite(f32::from_bits(reader.read_u32_at(0x0c)?), "framerate")?;
    if framerate <= 0.0 {
        return Err(invalid("invalid G2A framerate"));
    }
    let packed = reader.read_u32_at(0x10)?;
    let (animation_length, bone_info_size) = match endian {
        Endian::Big => (packed >> 18, (packed & 0x3fff) << 2),
        Endian::Little => (packed & 0x3fff, (packed >> 18) & 0x3ffc),
    };
    let timing_size = reader.read_u32_at(0x14)? as usize;
    let entry_count = reader.read_u32_at(0x18)?;
    let is_g2a5 = version_raw == 0x3030_3530;
    let is_g2a4 = version_raw == 0x3030_3430;
    let table_start: usize = if is_g2a5 || is_g2a4 { 0x20 } else { 0x1c };
    let bone_count = bone_info_size as usize / 4;
    let table_end = table_start
        .checked_add(bone_info_size as usize)
        .ok_or_else(|| invalid("G2A bone table overflow"))?;
    let data_start = table_end
        .checked_add(timing_size)
        .ok_or_else(|| invalid("G2A data offset overflow"))?;
    if table_end > data.len() || data_start > data.len() {
        return Err(invalid("G2A sections exceed input"));
    }

    let mut bones = Vec::new();
    let mut last_id = 0u32;
    let mut global_offset = 0u32;
    for index in 0..bone_count {
        let packed_info = reader.read_u32_at(table_start + index * 4)?;
        let (spline_count, local_id, timing_offset) = match endian {
            Endian::Big => (
                packed_info >> 28,
                (packed_info >> 16) & 0xfff,
                (packed_info & 0xffff) << 2,
            ),
            Endian::Little if is_g2a5 => (
                packed_info & 0xf,
                (packed_info >> 4) & 0xff,
                packed_info >> 12,
            ),
            Endian::Little => (
                packed_info & 0xf,
                (packed_info >> 4) & 0x3ff,
                packed_info >> 14,
            ),
        };
        if local_id < last_id {
            global_offset = global_offset.saturating_add(1);
        }
        last_id = local_id;
        let bone_id = local_id + global_offset * if is_g2a5 { 256 } else { 1024 };
        let mut offset = table_end
            .checked_add(timing_offset as usize)
            .ok_or_else(|| invalid("G2A timing offset overflow"))?
            & !3;
        let mut bone = G1aBoneAnimation {
            bone_id,
            opcode: 0,
            scale: Vec::new(),
            rotation: Vec::new(),
            translation: Vec::new(),
        };
        for _ in 0..spline_count {
            if offset.checked_add(8).is_none_or(|end| end > data.len()) {
                return Err(invalid("G2A spline descriptor exceeds input"));
            }
            let opcode = reader.read_u16_at(offset)?;
            let key_count = reader.read_u16_at(offset + 2)? as usize;
            let first_data_index = reader.read_u32_at(offset + 4)? as usize;
            if key_count == 0 || key_count > MAX_CHANNEL_KEYS {
                return Err(invalid("invalid G2A key count"));
            }
            offset += 8;
            let timing_end = offset
                .checked_add(key_count * 2)
                .ok_or_else(|| invalid("G2A timing overflow"))?;
            if timing_end > data.len() {
                return Err(invalid("G2A timings exceed input"));
            }
            let mut timings = Vec::with_capacity(key_count + 1);
            for key in 0..key_count {
                timings.push(reader.read_u16_at(offset + key * 2)? as u32);
            }
            offset = (timing_end + 3) & !3;
            let coefficient_start = data_start
                .checked_add(
                    first_data_index
                        .checked_mul(32)
                        .ok_or_else(|| invalid("G2A coefficient offset overflow"))?,
                )
                .ok_or_else(|| invalid("G2A coefficient offset overflow"))?;
            let coefficient_end = coefficient_start
                .checked_add(
                    key_count
                        .checked_mul(32)
                        .ok_or_else(|| invalid("G2A coefficient size overflow"))?,
                )
                .ok_or_else(|| invalid("G2A coefficient size overflow"))?;
            if coefficient_end > data.len() {
                return Err(invalid("G2A coefficients exceed input"));
            }
            if timings.last().copied() != Some(animation_length) {
                timings.push(animation_length);
            }
            let segment_count = if key_count == 1 {
                1
            } else {
                timings.len().saturating_sub(1)
            };
            for key in 0..segment_count {
                let rows = [
                    reader.read_u64_at(coefficient_start + key * 32)?,
                    reader.read_u64_at(coefficient_start + key * 32 + 8)?,
                    reader.read_u64_at(coefficient_start + key * 32 + 16)?,
                    reader.read_u64_at(coefficient_start + key * 32 + 24)?,
                ];
                let start_frame = timings[key];
                let end_frame = timings.get(key + 1).copied().unwrap_or(start_frame + 1);
                let samples = if key_count == 1 {
                    1
                } else {
                    end_frame.saturating_sub(start_frame).max(1)
                };
                for sample in 0..samples {
                    let vector = decode_g2a_vector(rows, sample as f32, samples as f32);
                    let time = (start_frame + sample) as f32 / framerate;
                    match opcode {
                        0 => bone.rotation.push(G1aQuaternionKeyframe {
                            time,
                            value: rotation_vector_to_quaternion(vector),
                        }),
                        1 => bone.translation.push(G1aVectorKeyframe {
                            time,
                            value: vector,
                        }),
                        2 => bone.scale.push(G1aVectorKeyframe {
                            time,
                            value: vector,
                        }),
                        _ => {}
                    }
                }
            }
            bone.opcode |= 1u32.checked_shl(opcode as u32).unwrap_or(0);
        }
        bones.push(bone);
    }
    let version =
        String::from_utf8(version_bytes.to_vec()).unwrap_or_else(|_| format!("{version_raw:08x}"));
    Ok(G1aFile {
        header: G1aHeader {
            version,
            chunk_size,
            animation_type: 2,
            duration: animation_length as f32 / framerate,
            data_section_offset: data_start as u32,
            bone_info_count: u16::try_from(bone_count).unwrap_or(u16::MAX),
            bone_max_id: u16::try_from(bones.iter().map(|bone| bone.bone_id).max().unwrap_or(0))
                .unwrap_or(u16::MAX),
            endian: match endian {
                Endian::Little => "little",
                Endian::Big => "big",
            }
            .into(),
        },
        bones,
    })
}

fn decode_g2a_vector(rows: [u64; 4], current: f32, total: f32) -> [f32; 3] {
    let ratio = current / total;
    let powers = [1.0, ratio, ratio * ratio, ratio * ratio * ratio];
    let mut result = [0.0; 3];
    for (row_index, row) in rows.into_iter().enumerate() {
        let exponent_bits = (((row >> 0x25) & 0x0780_0000) as u32).wrapping_add(0x3200_0000);
        let factor = f32::from_bits(exponent_bits) * powers[row_index];
        let components = [
            ((row >> 28) & 0xffff_f000) as u32 as i32,
            ((row >> 8) & 0xffff_f000) as u32 as i32,
            (row << 12) as u32 as i32,
        ];
        for axis in 0..3 {
            result[axis] += components[axis] as f32 * factor;
        }
    }
    result
}

fn rotation_vector_to_quaternion(vector: [f32; 3]) -> [f32; 4] {
    let angle = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    let scale = if angle > 0.000011920929 {
        (angle * 0.5).sin() / angle
    } else {
        0.5
    };
    // Project-G1M transposes the resulting quaternion.
    [
        -vector[0] * scale,
        -vector[1] * scale,
        -vector[2] * scale,
        (angle * 0.5).cos(),
    ]
}

fn parse_bone(
    data: &[u8],
    endian: Endian,
    version: u32,
    bone_id: u32,
    spline_offset: usize,
) -> io::Result<Option<G1aBoneAnimation>> {
    let mut reader = BinaryReader::with_endian(data, endian);
    reader.seek(spline_offset)?;
    let opcode = reader.read_u32()?;
    let (component_count, scale_index, rotation_index, translation_index) = match opcode {
        0x1 => (2, None, None, None),
        0x2 => (4, None, Some(0), None),
        0x4 => (7, None, Some(0), Some(4)),
        0x6 => (10, Some(0), Some(3), Some(7)),
        0x8 => (7, Some(0), Some(3), None),
        // Project-G1M ignores bone records with unknown channel layouts.
        _ => return Ok(None),
    };
    let descriptor_end = spline_offset
        .checked_add(4 + component_count * 8)
        .ok_or_else(|| invalid("G1A channel descriptor overflow"))?;
    if descriptor_end > data.len() {
        return Err(invalid("G1A channel descriptors exceed input"));
    }
    let mut channels = Vec::with_capacity(component_count);
    for index in 0..component_count {
        reader.seek(spline_offset + 4 + index * 8)?;
        let key_count = reader.read_u32()? as usize;
        if key_count == 0 || key_count > MAX_CHANNEL_KEYS {
            return Err(invalid(&format!(
                "invalid G1A channel key count {key_count}"
            )));
        }
        let data_units = reader.read_u32()? as usize;
        let channel_offset = spline_offset
            .checked_add(
                data_units
                    .checked_mul(0x10)
                    .ok_or_else(|| invalid("G1A channel offset overflow"))?,
            )
            .ok_or_else(|| invalid("G1A channel offset overflow"))?;
        channels.push(parse_channel(
            data,
            endian,
            version,
            channel_offset,
            key_count,
        )?);
    }

    let scale = scale_index
        .map(|index| vector_track(&channels, index))
        .transpose()?
        .unwrap_or_default();
    let rotation = rotation_index
        .map(|index| quaternion_track(&channels, index))
        .transpose()?
        .unwrap_or_default();
    let translation = translation_index
        .map(|index| vector_track(&channels, index))
        .transpose()?
        .unwrap_or_default();
    Ok(Some(G1aBoneAnimation {
        bone_id,
        opcode,
        scale,
        rotation,
        translation,
    }))
}

fn parse_channel(
    data: &[u8],
    endian: Endian,
    version: u32,
    offset: usize,
    key_count: usize,
) -> io::Result<SplineChannel> {
    let byte_count = key_count
        .checked_mul(20)
        .ok_or_else(|| invalid("G1A channel size overflow"))?;
    let end = offset
        .checked_add(byte_count)
        .ok_or_else(|| invalid("G1A channel size overflow"))?;
    if end > data.len() {
        return Err(invalid("G1A channel data exceeds input"));
    }
    let mut reader = BinaryReader::with_endian(data, endian);
    reader.seek(offset)?;
    let values_first = version > OLD_CHANNEL_LAYOUT_MAX_VERSION;
    let mut values = Vec::with_capacity(key_count);
    let mut times = Vec::with_capacity(key_count);
    if values_first {
        read_values(&mut reader, key_count, &mut values)?;
        read_times(&mut reader, key_count, &mut times)?;
    } else {
        read_times(&mut reader, key_count, &mut times)?;
        read_values(&mut reader, key_count, &mut values)?;
    }
    if times.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(invalid("G1A channel times are not sorted"));
    }
    Ok(SplineChannel { values, times })
}

fn read_values(
    reader: &mut BinaryReader<'_>,
    count: usize,
    output: &mut Vec<[f32; 4]>,
) -> io::Result<()> {
    for _ in 0..count {
        output.push([
            finite(reader.read_f32()?, "spline coefficient")?,
            finite(reader.read_f32()?, "spline coefficient")?,
            finite(reader.read_f32()?, "spline coefficient")?,
            finite(reader.read_f32()?, "spline coefficient")?,
        ]);
    }
    Ok(())
}

fn read_times(
    reader: &mut BinaryReader<'_>,
    count: usize,
    output: &mut Vec<f32>,
) -> io::Result<()> {
    for _ in 0..count {
        output.push(finite(reader.read_f32()?, "keyframe time")?);
    }
    Ok(())
}

fn vector_track(channels: &[SplineChannel], start: usize) -> io::Result<Vec<G1aVectorKeyframe>> {
    let (times, values) = sample_components(channels, start, 3)?;
    Ok(times
        .into_iter()
        .zip(values.chunks_exact(3))
        .map(|(time, value)| G1aVectorKeyframe {
            time,
            value: [value[0], value[1], value[2]],
        })
        .collect())
}

fn quaternion_track(
    channels: &[SplineChannel],
    start: usize,
) -> io::Result<Vec<G1aQuaternionKeyframe>> {
    let (times, values) = sample_components(channels, start, 4)?;
    Ok(times
        .into_iter()
        .zip(values.chunks_exact(4))
        .map(|(time, value)| G1aQuaternionKeyframe {
            time,
            // Project-G1M transposes the spline quaternion before applying it.
            value: [-value[0], -value[1], -value[2], value[3]],
        })
        .collect())
}

fn sample_components(
    channels: &[SplineChannel],
    start: usize,
    component_count: usize,
) -> io::Result<(Vec<f32>, Vec<f32>)> {
    let selected = channels
        .get(start..start + component_count)
        .ok_or_else(|| invalid("G1A component range exceeds channel table"))?;
    let mut times = BTreeSet::new();
    times.insert(OrderedFloat(0.0));
    for channel in selected {
        times.extend(channel.times.iter().copied().map(OrderedFloat));
    }
    let times: Vec<f32> = times.into_iter().map(|time| time.0).collect();
    let mut values = Vec::with_capacity(times.len() * component_count);
    for &time in &times {
        for channel in selected {
            values.push(evaluate_channel(channel, time)?);
        }
    }
    Ok((times, values))
}

fn evaluate_channel(channel: &SplineChannel, time: f32) -> io::Result<f32> {
    let index = channel
        .times
        .iter()
        .position(|key_time| time < *key_time)
        .unwrap_or(channel.times.len() - 1);
    let end = channel.times[index];
    let start = if index == 0 {
        0.0
    } else {
        channel.times[index - 1]
    };
    let ratio = if end == start {
        0.0
    } else {
        (time - start) / (end - start)
    };
    let [a, b, c, d] = channel.values[index];
    finite(
        ((a * ratio + b) * ratio + c) * ratio + d,
        "sampled spline value",
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct OrderedFloat(f32);

impl Eq for OrderedFloat {}

impl PartialOrd for OrderedFloat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedFloat {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct AnimationPairing {
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    anims: Vec<String>,
    #[serde(default)]
    root_model: String,
}

static ANIMATION_PAIRINGS: LazyLock<HashMap<String, AnimationPairing>> = LazyLock::new(|| {
    serde_json::from_str(&crate::utils::LookupData::read_support_json(
        "Animations_paths_and_characters.json",
    ))
    .unwrap_or_default()
});

fn resolve_pairing<'a>(
    model_hash: &'a str,
    visiting: &mut HashSet<String>,
) -> Option<AnimationPairing> {
    let pairing = ANIMATION_PAIRINGS.get(model_hash)?;
    if !pairing.anims.is_empty() {
        return Some(pairing.clone());
    }
    let parent = pairing.parent.as_deref()?;
    if !visiting.insert(model_hash.to_owned()) {
        return None;
    }
    if visiting.contains(parent) {
        return None;
    }
    resolve_pairing(parent, visiting)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelAnimations {
    pub model_hash: String,
    pub root_model: String,
    pub animations: Vec<String>,
}

pub fn animations_for_model(model_hash: &str) -> Option<ModelAnimations> {
    let model_hash = model_hash.to_ascii_lowercase();
    let entry = ANIMATION_PAIRINGS.get(&model_hash)?;
    let pairing = resolve_pairing(&model_hash, &mut HashSet::new())?;
    let mut seen = BTreeMap::new();
    for path in &pairing.anims {
        seen.entry(path.to_ascii_lowercase())
            .or_insert_with(|| path.clone());
    }
    Some(ModelAnimations {
        model_hash,
        root_model: entry.root_model.clone(),
        animations: seen.into_values().collect(),
    })
}

pub fn existing_animation_paths(model_hash: &str, aoc_root: &Path) -> Vec<PathBuf> {
    animations_for_model(model_hash)
        .into_iter()
        .flat_map(|pairing| pairing.animations)
        .map(|relative| aoc_root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR)))
        .filter(|path| path.is_file())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AvailableG1aAnimation {
    pub name: String,
    pub path: String,
}

pub fn available_animations(model_hash: &str, aoc_root: &Path) -> Vec<AvailableG1aAnimation> {
    existing_animation_paths(model_hash, aoc_root)
        .into_iter()
        .map(|path| AvailableG1aAnimation {
            name: path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("G1A")
                .to_owned(),
            path: path.to_string_lossy().into_owned(),
        })
        .collect()
}

fn finite(value: f32, label: &str) -> io::Result<f32> {
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| invalid(&format!("non-finite G1A {label}")))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::binary::BinaryWriter;

    fn fixture(endian: Endian, version: u32) -> Vec<u8> {
        let mut writer = BinaryWriter::with_endian(endian);
        writer.write_bytes(match endian {
            Endian::Big => b"G1A_",
            Endian::Little => b"_A1G",
        });
        writer.write_u32(version);
        writer.write_u32(0);
        writer.write_u16(1);
        writer.write_u16(0);
        writer.write_f32(1.0);
        writer.write_u32(0);
        writer.seek(0x30);
        writer.write_u16(1);
        writer.write_u16(42);
        writer.write_u32(42);
        writer.write_u32(1);
        writer.seek(0x40);
        writer.write_u32(0x4);
        for index in 0..7 {
            writer.write_u32(1);
            writer.write_u32(4 + index * 2);
        }
        for index in 0..7 {
            writer.seek(0x80 + index as usize * 0x20);
            if version <= OLD_CHANNEL_LAYOUT_MAX_VERSION {
                writer.write_f32(1.0);
            }
            writer.write_f32(0.0);
            writer.write_f32(0.0);
            writer.write_f32(0.0);
            writer.write_f32(if index == 3 { 1.0 } else { index as f32 });
            if version > OLD_CHANNEL_LAYOUT_MAX_VERSION {
                writer.write_f32(1.0);
            }
        }
        let mut data = writer.into_inner();
        let size = data.len() as u32;
        let size_bytes = match endian {
            Endian::Big => size.to_be_bytes(),
            Endian::Little => size.to_le_bytes(),
        };
        data[8..12].copy_from_slice(&size_bytes);
        data
    }

    #[test]
    fn parses_project_g1m_spline_layout() {
        let animation = G1aFile::parse(&fixture(Endian::Big, 0x3030_3530)).unwrap();
        assert_eq!(animation.header.version, "0050");
        assert_eq!(animation.header.duration, 1.0);
        assert_eq!(animation.bones.len(), 1);
        let bone = &animation.bones[0];
        assert_eq!(bone.bone_id, 42);
        assert_eq!(bone.rotation.last().unwrap().value, [0.0, -1.0, -2.0, 1.0]);
        assert_eq!(bone.translation.last().unwrap().value, [4.0, 5.0, 6.0]);
    }

    #[test]
    fn parses_old_little_endian_timing_first_layout() {
        let animation = G1aFile::parse(&fixture(Endian::Little, 0x3030_3430)).unwrap();
        assert_eq!(animation.header.version, "0040");
        assert_eq!(animation.header.endian, "little");
        assert_eq!(
            animation.bones[0].translation.last().unwrap().value,
            [4.0, 5.0, 6.0]
        );
    }

    #[test]
    fn model_pairing_is_cached_and_deduplicated() {
        let (&ref hash, _) = ANIMATION_PAIRINGS
            .iter()
            .next()
            .expect("animation map is empty");
        let first = animations_for_model(hash).unwrap();
        let second = animations_for_model(hash).unwrap();
        assert_eq!(first, second);
        let unique: BTreeSet<_> = first
            .animations
            .iter()
            .map(|path| path.to_ascii_lowercase())
            .collect();
        assert_eq!(unique.len(), first.animations.len());
    }

    #[test]
    fn model_pairing_can_fall_back_to_parent_anims() {
        let child = ANIMATION_PAIRINGS
            .iter()
            .find(|(_, pairing)| pairing.parent.is_some());
        let Some((child_hash, child_pairing)) = child else {
            return;
        };
        let parent_hash = child_pairing.parent.as_ref().unwrap();
        let child = animations_for_model(child_hash).unwrap();
        let parent = animations_for_model(parent_hash).unwrap();
        assert_eq!(child.animations, parent.animations);
        assert_eq!(child.model_hash, child_hash.to_string());
        assert_eq!(child.root_model, child_pairing.root_model);
    }

    #[test]
    fn binds_global_animation_ids_to_local_model_bones() {
        let animation = G1aFile::parse(&fixture(Endian::Big, 0x3030_3530)).unwrap();
        let mut mapping = vec![u16::MAX; 43];
        mapping[42] = 1;
        let names = vec!["Root".to_owned(), "Tail".to_owned()];
        let bound = animation.bind_to_skeleton(&mapping, &names);
        assert!(bound.unmapped_bone_ids.is_empty());
        assert_eq!(bound.tracks.len(), 1);
        assert_eq!(bound.tracks[0].global_bone_id, 42);
        assert_eq!(bound.tracks[0].bone_index, 1);
        assert_eq!(bound.tracks[0].bone_name, "Tail");
        assert_eq!(
            bound.tracks[0].translation.last().unwrap().value,
            [4.0, 5.0, 6.0]
        );
    }

    #[test]
    fn parses_local_g1a_corpus_when_present() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/g1a");
        if !corpus.is_dir() {
            return;
        }
        let files: Vec<_> = std::fs::read_dir(&corpus)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "g1a"))
            .collect();
        assert!(!files.is_empty(), "G1A corpus is empty");
        for path in files {
            let animation = G1aFile::from_path(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(
                !animation.bones.is_empty(),
                "{} has no bones",
                path.display()
            );
        }
    }
}
