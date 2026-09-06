use super::{
    flowchart::Flowchart,
    radix_tree::{read_keys, read_offset_array, read_string},
    timeline::Timeline,
};
use crate::parser::binary::BinaryReader;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::{self, ErrorKind},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct BfevDocument {
    pub file_name: String,
    pub version: String,
    pub flowcharts: IndexMap<String, Flowchart>,
    pub timelines: IndexMap<String, Timeline>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relocation_additions: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relocation_removals: Vec<u32>,
}

impl BfevDocument {
    pub fn from_binary(data: &[u8]) -> io::Result<Self> {
        let mut reader = BinaryReader::new(data);
        if reader.read_bytes(6)? != b"BFEVFL" {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "invalid BFEVFL magic",
            ));
        }
        reader.skip(2)?;
        let version = reader
            .read_bytes(4)?
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(".");
        reader.skip(4)?;
        let file_name_pointer = reader.read_u32()?;
        let file_name_offset = file_name_pointer
            .checked_sub(2)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "invalid file name pointer"))?;
        let file_name = read_string(data, file_name_offset as u64)?;
        reader.skip(12)?;
        let flowchart_count = reader.read_u16()? as usize;
        let timeline_count = reader.read_u16()? as usize;
        reader.skip(4)?;
        let flowchart_offsets_pointer = reader.read_u64()?;
        let flowchart_dictionary_pointer = reader.read_u64()?;
        let timeline_offsets_pointer = reader.read_u64()?;
        let timeline_dictionary_pointer = reader.read_u64()?;
        let names = read_keys(data, flowchart_dictionary_pointer)?;
        let flowchart_offsets =
            read_offset_array(data, flowchart_offsets_pointer, flowchart_count)?;
        if names.len() != flowchart_count || flowchart_offsets.len() != flowchart_count {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "flowchart table length mismatch",
            ));
        }
        let mut flowcharts = IndexMap::new();
        for (name, offset) in names.into_iter().zip(flowchart_offsets.iter().copied()) {
            flowcharts.insert(name, Flowchart::read(data, offset)?);
        }
        let timeline_names = read_keys(data, timeline_dictionary_pointer)?;
        let timeline_offsets = read_offset_array(data, timeline_offsets_pointer, timeline_count)?;
        if timeline_names.len() != timeline_count || timeline_offsets.len() != timeline_count {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "timeline table length mismatch",
            ));
        }
        let mut timelines = IndexMap::new();
        for (name, offset) in timeline_names.into_iter().zip(timeline_offsets) {
            timelines.insert(name, Timeline::read(data, offset)?);
        }
        let mut document = Self {
            file_name,
            version,
            flowcharts,
            timelines,
            relocation_additions: Vec::new(),
            relocation_removals: Vec::new(),
        };
        if !document.flowcharts.is_empty() {
            let baseline = super::writer::write_document(&document)?;
            let baseline_offsets = read_document_flow_offsets(&baseline)?;
            for (((_, flowchart), original_offset), baseline_offset) in document
                .flowcharts
                .iter_mut()
                .zip(flowchart_offsets)
                .zip(baseline_offsets)
            {
                flowchart.empty_entry_point_trailers =
                    infer_empty_entry_trailers(data, original_offset, &baseline, baseline_offset)?;
                let empty_indices = flowchart
                    .entry_points
                    .values()
                    .enumerate()
                    .filter_map(|(index, entry)| {
                        entry
                            .variables
                            .as_ref()
                            .is_none_or(IndexMap::is_empty)
                            .then_some(index)
                    })
                    .collect::<Vec<_>>();
                let variable_indices = flowchart
                    .entry_points
                    .values()
                    .enumerate()
                    .filter_map(|(index, entry)| {
                        entry
                            .variables
                            .as_ref()
                            .is_some_and(|variables| !variables.is_empty())
                            .then_some(index)
                    })
                    .collect::<Vec<_>>();
                if !variable_indices.is_empty()
                    && flowchart.empty_entry_point_trailers.len()
                        == empty_indices.len().saturating_sub(variable_indices.len())
                {
                    flowchart.empty_entry_point_trailers = empty_indices;
                    flowchart.omitted_variable_entry_point_trailers = variable_indices;
                }
            }
        }
        let rebuilt = super::writer::write_document(&document)?;
        let original_pointers = read_relocation_pointers(data)?;
        let rebuilt_pointers = read_relocation_pointers(&rebuilt)?;
        let original_set = original_pointers.iter().copied().collect::<HashSet<_>>();
        let rebuilt_set = rebuilt_pointers.iter().copied().collect::<HashSet<_>>();
        document.relocation_additions = original_pointers
            .iter()
            .filter(|pointer| !rebuilt_set.contains(pointer))
            .copied()
            .collect();
        document.relocation_removals = rebuilt_pointers
            .iter()
            .filter(|pointer| !original_set.contains(pointer))
            .copied()
            .collect();
        Ok(document)
    }

    pub fn to_json(&self) -> io::Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
    }

    pub fn to_binary(&self) -> io::Result<Vec<u8>> {
        super::writer::write_document(self)
    }
}

fn read_u16_at(data: &[u8], offset: usize) -> io::Result<u16> {
    BinaryReader::new(data)
        .read_u16_at(offset)
        .map_err(|_| io::Error::new(ErrorKind::UnexpectedEof, "EVFL u16"))
}

fn read_u32_at(data: &[u8], offset: usize) -> io::Result<u32> {
    BinaryReader::new(data)
        .read_u32_at(offset)
        .map_err(|_| io::Error::new(ErrorKind::UnexpectedEof, "EVFL u32"))
}

fn read_u64_at(data: &[u8], offset: usize) -> io::Result<u64> {
    BinaryReader::new(data)
        .read_u64_at(offset)
        .map_err(|_| io::Error::new(ErrorKind::UnexpectedEof, "EVFL u64"))
}

fn read_document_flow_offsets(data: &[u8]) -> io::Result<Vec<u64>> {
    let count = read_u16_at(data, 32)? as usize;
    let table = read_u64_at(data, 40)?;
    read_offset_array(data, table, count)
}

fn infer_empty_entry_trailers(
    original: &[u8],
    original_flow: u64,
    baseline: &[u8],
    baseline_flow: u64,
) -> io::Result<Vec<usize>> {
    let original_flow = original_flow as usize;
    let baseline_flow = baseline_flow as usize;
    let count = read_u16_at(original, original_flow + 24)? as usize;
    let original_entries = read_u64_at(original, original_flow + 64)? as usize;
    let baseline_entries = read_u64_at(baseline, baseline_flow + 64)? as usize;
    let original_pool = original_flow + read_u32_at(original, original_flow + 4)? as usize;
    let baseline_pool = baseline_flow + read_u32_at(baseline, baseline_flow + 4)? as usize;

    let mut pending = Vec::new();
    let mut result = Vec::new();
    let mut assigned = 0usize;
    for index in 0..count {
        let original_header = original_entries + index * 32;
        let baseline_header = baseline_entries + index * 32;
        let original_marker = [
            read_u64_at(original, original_header)?,
            read_u64_at(original, original_header + 8)?,
            read_u64_at(original, original_header + 16)?,
        ]
        .into_iter()
        .find(|pointer| *pointer != 0);
        let baseline_marker = [
            read_u64_at(baseline, baseline_header)?,
            read_u64_at(baseline, baseline_header + 8)?,
            read_u64_at(baseline, baseline_header + 16)?,
        ]
        .into_iter()
        .find(|pointer| *pointer != 0);
        if let (Some(original_marker), Some(baseline_marker)) = (original_marker, baseline_marker) {
            let delta = original_marker
                .checked_sub(baseline_marker)
                .ok_or_else(|| {
                    io::Error::new(ErrorKind::InvalidData, "negative EVFL trailer delta")
                })? as usize;
            if delta % 0x18 != 0 {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "unaligned EVFL entry trailer delta",
                ));
            }
            let target = delta / 0x18;
            let needed = target.saturating_sub(assigned);
            if needed > pending.len() {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "missing EVFL trailer candidate",
                ));
            }
            result.extend(pending.iter().take(needed).copied());
            assigned += needed;
            pending.clear();
        }
        if read_u16_at(original, original_header + 26)? == 0 {
            pending.push(index);
        }
    }
    let delta = original_pool
        .checked_sub(baseline_pool)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "negative EVFL pool delta"))?;
    if delta % 0x18 != 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "unaligned EVFL pool trailer delta",
        ));
    }
    let target = delta / 0x18;
    let needed = target.saturating_sub(assigned);
    if needed > pending.len() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "missing EVFL pool trailer candidate",
        ));
    }
    result.extend(pending.into_iter().take(needed));
    Ok(result)
}

fn read_relocation_pointers(data: &[u8]) -> io::Result<Vec<u32>> {
    let relocation = read_u32_at(data, 24)? as usize;
    let count = read_u32_at(data, relocation + 36)? as usize;
    let mut pointers = Vec::new();
    for entry in 0..count {
        let base = read_u32_at(data, relocation + 40 + entry * 8)?;
        let flags = read_u32_at(data, relocation + 44 + entry * 8)?;
        for bit in 0..32 {
            if flags & (1 << bit) != 0 {
                pointers.push(base + bit * 8);
            }
        }
    }
    Ok(pointers)
}
