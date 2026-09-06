use super::{BphclDocument, CopiedItemGraph, ImportedRange, ItemRange, Patch};
use crate::parser::binary::{BinaryPatcher, BinaryReader};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    io::{self, ErrorKind},
};

impl BphclDocument {
    pub fn item_ranges(&self) -> io::Result<HashMap<usize, ItemRange>> {
        let data_size = self.data_section()?.size.saturating_sub(8);
        let data_size = u32::try_from(data_size).map_err(|_| invalid("DATA exceeds u32"))?;
        let mut grouped: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (index, item) in self.items.iter().enumerate() {
            // A zero-count array rebuilt by the native editors sits at the
            // very end of DATA; it owns an empty range rather than none.
            if item.data_offset <= data_size {
                grouped.entry(item.data_offset).or_default().push(index);
            }
        }
        let starts: Vec<u32> = grouped.keys().copied().collect();
        let mut result = HashMap::new();
        for (position, start) in starts.iter().copied().enumerate() {
            let end = starts.get(position + 1).copied().unwrap_or(data_size);
            if end < start {
                return Err(invalid("ITEM DATA offsets are not ordered"));
            }
            for index in &grouped[&start] {
                result.insert(*index, ItemRange { start, end });
            }
        }
        Ok(result)
    }

    pub fn collect_item_closure(
        &self,
        roots: impl IntoIterator<Item = usize>,
    ) -> io::Result<Vec<usize>> {
        let ranges = self.item_ranges()?;
        let mut included = HashSet::new();
        let mut queue: VecDeque<usize> = roots.into_iter().collect();
        while let Some(item_index) = queue.pop_front() {
            if !included.insert(item_index) {
                continue;
            }
            let range = ranges
                .get(&item_index)
                .ok_or_else(|| invalid(&format!("ITEM {item_index} has no DATA range")))?;
            for patch in &self.patches {
                for pointer_offset in patch.offsets.iter().copied().filter(|o| range.contains(*o)) {
                    let referenced = self.resolve_patched_item(pointer_offset)?;
                    if !included.contains(&referenced) {
                        queue.push_back(referenced);
                    }
                }
            }
        }
        let mut result: Vec<_> = included.into_iter().collect();
        result.sort_unstable();
        Ok(result)
    }

    pub fn patches_for_items(
        &self,
        item_indices: impl IntoIterator<Item = usize>,
    ) -> io::Result<Vec<Patch>> {
        let ranges = self.item_ranges()?;
        let selected: Vec<ItemRange> = item_indices
            .into_iter()
            .map(|index| {
                ranges
                    .get(&index)
                    .copied()
                    .ok_or_else(|| invalid(&format!("ITEM {index} has no DATA range")))
            })
            .collect::<io::Result<_>>()?;
        Ok(self
            .patches
            .iter()
            .filter_map(|patch| {
                let offsets: Vec<u32> = patch
                    .offsets
                    .iter()
                    .copied()
                    .filter(|offset| selected.iter().any(|range| range.contains(*offset)))
                    .collect();
                (!offsets.is_empty()).then(|| Patch {
                    type_index: patch.type_index,
                    offsets,
                })
            })
            .collect())
    }

    pub fn copy_item_closure(
        &self,
        item_indices: impl IntoIterator<Item = usize>,
        mut destination: Vec<u8>,
    ) -> io::Result<CopiedItemGraph> {
        let ranges = self.item_ranges()?;
        let mut selected: Vec<(usize, ItemRange)> = item_indices
            .into_iter()
            .map(|index| {
                ranges
                    .get(&index)
                    .copied()
                    .map(|range| (index, range))
                    .ok_or_else(|| invalid(&format!("ITEM {index} has no DATA range")))
            })
            .collect::<io::Result<_>>()?;
        selected.sort_by_key(|(_, range)| range.start);
        // Havok allocates each object at its type's declared alignment, and
        // vector or transform payloads need their 16 bytes. Items sharing a
        // DATA range are laid out together, so the range takes the strictest
        // of their alignments.
        let mut alignment_by_type: HashMap<u32, u32> = HashMap::new();
        for body in &self.type_table.bodies {
            alignment_by_type
                .entry(body.type_index)
                .or_insert(body.alignment.unwrap_or(1).max(1));
        }
        let mut required_alignment: HashMap<u32, u32> = HashMap::new();
        for (index, range) in &selected {
            let alignment = self
                .items
                .get(*index)
                .and_then(|item| alignment_by_type.get(&item.type_index))
                .copied()
                .unwrap_or(1);
            let required = required_alignment.entry(range.start).or_insert(1);
            *required = (*required).max(alignment);
        }
        let data = self.graph_data_bytes()?;
        let mut copied = HashMap::new();
        for (_, range) in selected {
            if copied.contains_key(&range.start) {
                continue;
            }
            let alignment = required_alignment
                .get(&range.start)
                .copied()
                .unwrap_or(1)
                .max(1) as usize;
            while destination.len() % alignment != 0 {
                destination.push(0);
            }
            let new_start = u32::try_from(destination.len())
                .map_err(|_| invalid("destination DATA exceeds u32"))?;
            let source = data
                .get(range.start as usize..range.end as usize)
                .ok_or_else(|| invalid("ITEM range exceeds DATA"))?;
            destination.extend_from_slice(source);
            copied.insert(
                range.start,
                ImportedRange {
                    old_start: range.start,
                    old_end: range.end,
                    new_start,
                },
            );
        }
        Ok(CopiedItemGraph {
            data: destination,
            ranges_by_old_start: copied,
        })
    }

    pub fn relocate_copied_pointers(
        &self,
        destination: &mut [u8],
        patches: &[Patch],
        ranges: &HashMap<u32, ImportedRange>,
        imported_item_map: &HashMap<usize, usize>,
        reused_item_map: &HashMap<usize, usize>,
    ) -> io::Result<()> {
        for patch in patches {
            for source_offset in &patch.offsets {
                let range = find_imported_range(ranges, *source_offset)?;
                let target_offset = range
                    .relocate(*source_offset)
                    .ok_or_else(|| invalid("pointer is outside imported range"))?
                    as usize;
                let source_item = self.resolve_patched_item(*source_offset)?;
                let target_item = reused_item_map
                    .get(&source_item)
                    .or_else(|| imported_item_map.get(&source_item))
                    .copied()
                    .ok_or_else(|| {
                        invalid(&format!(
                            "ITEM {source_item} was not included in imported graph"
                        ))
                    })?;
                let target_item = u32::try_from(target_item)
                    .map_err(|_| invalid("target ITEM index exceeds u32"))?;
                BinaryPatcher::new(destination)
                    .write_u32_at(target_offset, target_item)
                    .map_err(|_| invalid("relocated pointer exceeds destination DATA"))?;
            }
        }
        Ok(())
    }

    pub fn validate_item_graph(&self) -> io::Result<()> {
        let ranges = self.item_ranges()?;
        for patch in &self.patches {
            for offset in &patch.offsets {
                let _ = self.resolve_patched_item(*offset)?;
                if !ranges.values().any(|range| range.contains(*offset)) {
                    return Err(invalid(&format!(
                        "PTCH offset {offset:#x} is outside every ITEM range"
                    )));
                }
            }
        }
        Ok(())
    }

    fn data_section(&self) -> io::Result<&super::Section> {
        self.tag
            .find("DATA")
            .ok_or_else(|| invalid("BPHCL has no DATA section"))
    }

    fn graph_data_bytes(&self) -> io::Result<&[u8]> {
        let data = self.data_section()?;
        self.raw
            .get(data.payload_offset..data.payload_end())
            .ok_or_else(|| invalid("DATA exceeds input"))
    }

    fn resolve_patched_item(&self, offset: u32) -> io::Result<usize> {
        if !self
            .patches
            .iter()
            .any(|patch| patch.offsets.contains(&offset))
        {
            return Err(invalid(&format!("DATA+{offset:#x} is not PTCH-backed")));
        }
        let value = BinaryReader::new(self.graph_data_bytes()?)
            .read_u32_at(offset as usize)
            .map_err(|_| invalid("pointer exceeds DATA"))? as usize;
        if value >= self.items.len() {
            return Err(invalid(&format!("pointer references missing ITEM {value}")));
        }
        Ok(value)
    }
}

fn find_imported_range(
    ranges: &HashMap<u32, ImportedRange>,
    offset: u32,
) -> io::Result<ImportedRange> {
    ranges
        .values()
        .find(|range| range.contains(offset))
        .copied()
        .ok_or_else(|| {
            invalid(&format!(
                "pointer DATA+{offset:#x} is outside imported graph"
            ))
        })
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}
