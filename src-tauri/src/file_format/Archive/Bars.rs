use std::collections::BTreeMap;

use super::{ArchiveCodec, ArchiveResult};
use crate::parser::binary::{BinaryReader, BinaryWriter};

#[derive(Clone, Debug)]
struct BarsAsset {
    hash: u32,
    metadata_path: String,
    audio_path: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BarsFile {
    version: u16,
    assets: Vec<BarsAsset>,
    entries: BTreeMap<String, Vec<u8>>,
}

fn u16_at(data: &[u8], offset: usize) -> ArchiveResult<u16> {
    BinaryReader::new(data)
        .read_u16_at(offset)
        .map_err(|_| "truncated BARS header".into())
}

fn u32_at(data: &[u8], offset: usize) -> ArchiveResult<u32> {
    BinaryReader::new(data)
        .read_u32_at(offset)
        .map_err(|_| "truncated BARS table".into())
}

fn safe_name(value: &str, index: usize) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    if cleaned.is_empty() {
        format!("asset_{index:04}")
    } else {
        cleaned
    }
}

/// Reads the NUL-terminated string starting at `start` and ending no later
/// than `limit`; `None` when the range lies outside `data`.
fn c_string_bounded(data: &[u8], start: usize, limit: usize) -> Option<String> {
    let window = BinaryReader::new(data)
        .slice(start, limit.min(data.len()))
        .ok()?;
    let end = window.iter().position(|&v| v == 0).unwrap_or(window.len());
    Some(String::from_utf8_lossy(&window[..end]).into_owned())
}

fn amta_name(data: &[u8], index: usize) -> String {
    let fallback = || format!("asset_{index:04}");
    if !crate::Settings::Magic::is_amta(data) || data.len() < 0x1c {
        return fallback();
    }
    let reader = BinaryReader::new(data);
    if reader.read_u8_at(7).ok() == Some(5) && data.len() >= 0x28 {
        let Ok(declared) = reader.read_u32_at(8) else {
            return fallback();
        };
        let limit = (declared as usize).min(data.len());
        let Ok(string_table_offset) = reader.read_u32_at(0x24) else {
            return fallback();
        };
        let name_start = (string_table_offset as usize).checked_add(0x24);
        if let Some(name_start) = name_start.filter(|&offset| offset < limit) {
            return match c_string_bounded(data, name_start, limit) {
                Some(name) => safe_name(&name, index),
                None => fallback(),
            };
        }
    }
    let Ok(offset) = reader.read_u32_at(0x18) else {
        return fallback();
    };
    let offset = offset as usize;
    let Some(start) = offset.checked_add(8).filter(|&v| v <= data.len()) else {
        return fallback();
    };
    if reader.read_bytes_at(offset, 4).ok() != Some(b"STRG") {
        return fallback();
    }
    match c_string_bounded(data, start, data.len()) {
        Some(name) => safe_name(&name, index),
        None => fallback(),
    }
}

fn audio_extension(data: &[u8]) -> &'static str {
    match data.get(..4) {
        Some(b"FWAV") => "bfwav",
        Some(b"BWAV") => "bwav",
        Some(b"FSTP") => "bfstp",
        _ => "bin",
    }
}

impl ArchiveCodec for BarsFile {
    fn from_bytes(data: &[u8]) -> ArchiveResult<Self> {
        if !crate::Settings::Magic::is_bars(data) || data.len() < 0x10 {
            return Err("not a BARS archive".into());
        }
        let declared = u32_at(data, 4)? as usize;
        if declared != data.len() || declared < 0x10 {
            return Err(format!("invalid BARS file size {declared}"));
        }
        let version = u16_at(data, 0x0a)?;
        let count = u32_at(data, 0x0c)? as usize;
        let hashes_at = 0x10usize;
        let hash_table_size = count.checked_mul(4).ok_or("BARS count overflow")?;
        let offset_table_size = count.checked_mul(8).ok_or("BARS count overflow")?;
        let offsets_at = hashes_at
            .checked_add(hash_table_size)
            .ok_or("BARS count overflow")?;
        if offsets_at
            .checked_add(offset_table_size)
            .filter(|&v| v <= declared)
            .is_none()
        {
            return Err("truncated BARS asset table".into());
        }
        // `count` is bounded by the declared size checked above.
        let mut raw = Vec::with_capacity(count);
        let mut boundaries = vec![declared];
        for i in 0..count {
            let hash = u32_at(data, hashes_at + i * 4)?;
            let metadata = u32_at(data, offsets_at + i * 8)? as usize;
            let audio_raw = u32_at(data, offsets_at + i * 8 + 4)?;
            let audio = (audio_raw != u32::MAX).then_some(audio_raw as usize);
            if metadata >= declared || audio.is_some_and(|v| v >= declared) {
                return Err(format!("BARS asset {i} has an invalid offset"));
            }
            boundaries.push(metadata);
            if let Some(v) = audio {
                boundaries.push(v);
            }
            raw.push((hash, metadata, audio));
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        let end_for = |start: usize| {
            boundaries
                .iter()
                .copied()
                .find(|&v| v > start)
                .unwrap_or(declared)
        };
        let reader = BinaryReader::new(data);
        let mut entries = BTreeMap::new();
        let mut assets = Vec::with_capacity(count);
        for (index, (hash, metadata_at, audio_at)) in raw.into_iter().enumerate() {
            let metadata = reader
                .slice(metadata_at, end_for(metadata_at))
                .map_err(|_| format!("BARS asset {index} metadata lies outside the file"))?
                .to_vec();
            let name = amta_name(&metadata, index);
            let metadata_path = format!("Meta Data/{name}.amta");
            entries.insert(metadata_path.clone(), metadata);
            let audio_path = match audio_at {
                Some(offset) => {
                    let audio = reader
                        .slice(offset, end_for(offset))
                        .map_err(|_| format!("BARS asset {index} audio lies outside the file"))?
                        .to_vec();
                    let path = format!("Audio/{name}.{}", audio_extension(&audio));
                    entries.insert(path.clone(), audio);
                    Some(path)
                }
                None => None,
            };
            assets.push(BarsAsset {
                hash,
                metadata_path,
                audio_path,
            });
        }
        Ok(Self {
            version,
            assets,
            entries,
        })
    }

    fn to_bytes(&self) -> ArchiveResult<Vec<u8>> {
        let count = self.assets.len();
        let table_size = count.checked_mul(12).ok_or("BARS count overflow")?;
        0x10usize
            .checked_add(table_size)
            .ok_or("BARS count overflow")?;
        if count > u32::MAX as usize {
            return Err("too many BARS assets".into());
        }
        let mut output = BinaryWriter::new();
        output.write_bytes(b"BARS");
        output.write_u32(0); // file size, patched at the end
        output.write_u16(0xfeff);
        output.write_u16(self.version);
        output.write_u32(count as u32);
        for asset in &self.assets {
            output.write_u32(asset.hash);
        }
        let offset_table_at = output.position();
        output.write_zeros(count * 8);
        let mut offsets = Vec::with_capacity(count);
        for asset in &self.assets {
            let metadata = self
                .entries
                .get(&asset.metadata_path)
                .ok_or_else(|| format!("missing {}", asset.metadata_path))?;
            output.align(4).map_err(|e| e.to_string())?;
            let metadata_at = u32::try_from(output.position()).map_err(|_| "BARS exceeds 4 GiB")?;
            output.write_bytes(metadata);
            offsets.push((metadata_at, u32::MAX));
        }
        for (index, asset) in self.assets.iter().enumerate() {
            if let Some(path) = &asset.audio_path {
                let audio = self
                    .entries
                    .get(path)
                    .ok_or_else(|| format!("missing {path}"))?;
                output.align(0x40).map_err(|e| e.to_string())?;
                offsets[index].1 =
                    u32::try_from(output.position()).map_err(|_| "BARS exceeds 4 GiB")?;
                output.write_bytes(audio);
            }
        }
        for (index, (metadata, audio)) in offsets.into_iter().enumerate() {
            let at = offset_table_at + index * 8;
            output.write_u32_at(at, metadata);
            output.write_u32_at(at + 4, audio);
        }
        let size = u32::try_from(output.len()).map_err(|_| "BARS exceeds 4 GiB")?;
        output.write_u32_at(4, size);
        Ok(output.into_inner())
    }

    fn entries(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.entries
    }
    fn entries_mut(&mut self) -> &mut BTreeMap<String, Vec<u8>> {
        &mut self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_archive() {
        assert!(BarsFile::from_bytes(b"BARS").is_err());
    }

    #[test]
    fn corpus_opens_audio_and_roundtrips() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bars");
        let mut files = 0usize;
        let mut audio = 0usize;
        let mut replacement_codecs = std::collections::BTreeSet::new();
        for item in std::fs::read_dir(&root).expect("failed to read BARS corpus") {
            let path = item.expect("failed to read corpus entry").path();
            if path.extension().and_then(|v| v.to_str()) != Some("bars") {
                continue;
            }
            let bytes = std::fs::read(&path).expect("failed to read BARS file");
            let archive = BarsFile::from_bytes(&bytes)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(
                archive
                    .assets
                    .windows(2)
                    .all(|pair| pair[0].hash <= pair[1].hash),
                "{}: hash table is not sorted",
                path.display()
            );
            assert!(
                archive
                    .entries()
                    .keys()
                    .all(|name| !name.starts_with("Meta Data/asset_")
                        && !name.starts_with("Audio/asset_")),
                "{}: failed to resolve an AMTA name",
                path.display()
            );
            let expected_entries = archive.assets.len()
                + archive
                    .assets
                    .iter()
                    .filter(|asset| asset.audio_path.is_some())
                    .count();
            assert_eq!(
                archive.entries().len(),
                expected_entries,
                "{}: duplicate archive paths",
                path.display()
            );
            for (name, data) in archive.entries() {
                if name.ends_with(".bfwav") || name.ends_with(".bwav") {
                    let decoded = crate::file_format::Audio::decode(data)
                        .unwrap_or_else(|error| panic!("{}::{name}: {error}", path.display()));
                    if crate::Settings::Magic::is_bwav(data)
                        && !decoded.channels.first().is_none_or(Vec::is_empty)
                    {
                        let codec = u16::from_le_bytes([data[0x10], data[0x11]]);
                        if replacement_codecs.insert(codec) {
                            let short = crate::file_format::Audio::DecodedAudio {
                                channels: decoded
                                    .channels
                                    .iter()
                                    .map(|channel| channel.iter().copied().take(28).collect())
                                    .collect(),
                                sample_rate: decoded.sample_rate,
                                looping: false,
                                loop_start: 0,
                            };
                            let replacement =
                                crate::file_format::Audio::encode_replacement(data, &short)
                                    .unwrap_or_else(|error| {
                                        panic!("replacement {}::{name}: {error}", path.display())
                                    });
                            let reopened = crate::file_format::Audio::decode(&replacement)
                                .unwrap_or_else(|error| {
                                    panic!("replacement decode {}::{name}: {error}", path.display())
                                });
                            assert_eq!(reopened.channels.first().map(Vec::len), Some(28));
                        }
                    }
                    audio += 1;
                }
            }
            let rebuilt = archive
                .to_bytes()
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            BarsFile::from_bytes(&rebuilt)
                .unwrap_or_else(|error| panic!("rebuilt {}: {error}", path.display()));
            files += 1;
        }
        assert_eq!(files, 50);
        assert!(audio > 0);
        assert_eq!(replacement_codecs, [0, 1].into_iter().collect());
    }
}
