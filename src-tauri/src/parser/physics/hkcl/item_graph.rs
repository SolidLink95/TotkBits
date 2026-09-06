use super::{item_range::ItemRange, HkclDocument, Patch};
use crate::parser::binary::BinaryReader;
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    io::{self, ErrorKind},
    ops::Range,
};

impl HkclDocument {
    pub fn data_range(&self) -> io::Result<Range<usize>> {
        self.data_payload()?
            .ok_or_else(|| invalid("HKCL has no DATA section"))
    }

    pub fn data_bytes(&self, offset: u32, length: usize) -> io::Result<&[u8]> {
        let data = self.data_range()?;
        let start = data
            .start
            .checked_add(offset as usize)
            .ok_or_else(|| invalid("HKCL DATA offset overflows"))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| invalid("HKCL DATA length overflows"))?;
        self.raw
            .get(start..end)
            .ok_or_else(|| invalid("HKCL DATA read exceeds section"))
    }

    pub fn item_ranges(&self) -> io::Result<HashMap<usize, ItemRange>> {
        let data = self
            .data_payload()?
            .ok_or_else(|| invalid("HKCL has no DATA section"))?;
        let data_size = u32::try_from(data.len()).map_err(|_| invalid("HKCL DATA exceeds u32"))?;
        let mut grouped: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (index, item) in self.items.iter().enumerate() {
            if item.data_offset < data_size {
                grouped.entry(item.data_offset).or_default().push(index);
            }
        }
        let starts: Vec<u32> = grouped.keys().copied().collect();
        let mut result = HashMap::new();
        for (position, start) in starts.iter().copied().enumerate() {
            let end = starts.get(position + 1).copied().unwrap_or(data_size);
            if end < start {
                return Err(invalid("HKCL ITEM DATA offsets are not ordered"));
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
                .ok_or_else(|| invalid(&format!("HKCL ITEM {item_index} has no DATA range")))?;
            for patch in &self.patches {
                for pointer_offset in patch
                    .offsets
                    .iter()
                    .copied()
                    .filter(|offset| range.contains(*offset))
                {
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
                    .ok_or_else(|| invalid(&format!("HKCL ITEM {index} has no DATA range")))
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

    pub fn referenced(&self, offset: u32) -> Option<usize> {
        if !self
            .patches
            .iter()
            .any(|patch| patch.offsets.contains(&offset))
        {
            return None;
        }
        let data = self.data_range().ok()?;
        let start = data.start.checked_add(offset as usize)?;
        let index = BinaryReader::with_endian(&self.raw, self.header.layout.endian)
            .read_u32_at(start)
            .ok()? as usize;
        (index < self.items.len()).then_some(index)
    }

    pub fn reference_item_indices(&self, field: u32) -> io::Result<Vec<usize>> {
        let storage = self
            .referenced(field)
            .ok_or_else(|| invalid(&format!("array at DATA+{field:#x} has no ITEM reference")))?;
        let item = self
            .items
            .get(storage)
            .ok_or_else(|| invalid("array storage ITEM is missing"))?;
        (0..item.count)
            .map(|index| {
                let offset = item.data_offset + index * 8;
                self.referenced(offset).ok_or_else(|| {
                    invalid(&format!("unresolved array pointer at DATA+{offset:#x}"))
                })
            })
            .collect()
    }

    pub fn validate_item_graph(&self) -> io::Result<()> {
        let ranges = self.item_ranges()?;
        for patch in &self.patches {
            for offset in &patch.offsets {
                let _ = self.resolve_patched_item(*offset)?;
                if !ranges.values().any(|range| range.contains(*offset)) {
                    return Err(invalid(&format!(
                        "HKCL PTCH offset {offset:#x} is outside every ITEM range"
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn resolve_patched_item(&self, offset: u32) -> io::Result<usize> {
        if !self
            .patches
            .iter()
            .any(|patch| patch.offsets.contains(&offset))
        {
            return Err(invalid(&format!(
                "HKCL DATA+{offset:#x} is not PTCH-backed"
            )));
        }
        let data_payload = self.data_range()?;
        let pointer = data_payload
            .start
            .checked_add(offset as usize)
            .ok_or_else(|| invalid("HKCL pointer offset overflows"))?;
        let value = usize::try_from(
            BinaryReader::with_endian(&self.raw, self.header.layout.endian)
                .read_u32_at(pointer)
                .map_err(|_| invalid("HKCL pointer exceeds DATA"))?,
        )
        .map_err(|_| invalid("HKCL ITEM index does not fit usize"))?;
        if value >= self.items.len() {
            return Err(invalid(&format!(
                "HKCL pointer references missing ITEM {value}"
            )));
        }
        Ok(value)
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::binary::{BinaryWriter, Endian};
    use crate::parser::physics::hkcl::header::HEADER_SIZE;
    use crate::parser::physics::hkcl::section::SECTION_HEADER_SIZE;

    fn packfile_with_sections(header_endian: Endian, sections: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
        let mut writer = BinaryWriter::with_endian(header_endian);
        writer.write_bytes(&[0x57, 0xe0, 0xe0, 0x57, 0x10, 0xc0, 0xc0, 0x10]);
        writer.write_u32(0);
        writer.write_u32(11);
        writer.write_bytes(&[
            4,
            if header_endian == Endian::Little {
                1
            } else {
                0
            },
            1,
            1,
        ]);
        writer.write_i32(sections.len() as i32);
        writer.write_i32(-1);
        writer.write_u32(0);
        writer.write_i32(-1);
        writer.write_u32(0);
        let mut version = [0u8; 16];
        let version_text = b"hk_2014.1.0-r1";
        version[..version_text.len()].copy_from_slice(version_text);
        writer.write_bytes(&version);
        writer.write_u32(0);
        writer.write_u16(0);
        writer.write_u16(0);

        let section_count = sections.len();
        let mut section_meta = Vec::new();
        let start = HEADER_SIZE + section_count * SECTION_HEADER_SIZE;
        let mut cursor = start;
        let write_u32 = |value: u32| header_endian.u32_to_bytes(value);
        for (tag, payload) in sections.iter() {
            let mut tag_bytes = [0u8; 16];
            let signature = tag.as_bytes();
            let name_len = signature.len().min(16);
            tag_bytes[..name_len].copy_from_slice(&signature[..name_len]);
            section_meta.extend_from_slice(&tag_bytes);
            section_meta.extend_from_slice(&write_u32(0));
            section_meta.extend_from_slice(&write_u32(cursor as u32));
            let payload_len = payload.len() as u32;
            section_meta.extend_from_slice(&write_u32(payload_len));
            section_meta.extend_from_slice(&write_u32(payload_len));
            section_meta.extend_from_slice(&write_u32(payload_len));
            section_meta.extend_from_slice(&write_u32(payload_len));
            section_meta.extend_from_slice(&write_u32(payload_len));
            section_meta.extend_from_slice(&write_u32(payload_len));
            section_meta.extend_from_slice(&[0; 16]);
            cursor += payload.len();
        }
        writer.write_bytes(&section_meta);
        for (_, payload) in &sections {
            writer.write_bytes(&payload);
        }
        writer.into_inner()
    }

    fn encode_words(endian: Endian, words: &[u32]) -> Vec<u8> {
        let mut writer = BinaryWriter::with_endian(endian);
        for word in words {
            writer.write_u32(*word);
        }
        writer.into_inner()
    }

    #[test]
    fn tracks_patched_item_closure() {
        for endian in [Endian::Little, Endian::Big] {
            let item_payload = encode_words(endian, &[0, 0, 0, 1, 8, 1, 2, 8, 1]);
            let patch_payload = encode_words(endian, &[1, 1, 8]);
            let data_payload = encode_words(endian, &[0, 0, 2, 0]);
            let bytes = packfile_with_sections(
                endian,
                vec![
                    ("ITEM", item_payload),
                    ("PTCH", patch_payload),
                    ("DATA", data_payload.clone()),
                ],
            );
            let document = crate::parser::physics::hkcl::HkclDocument::parse(&bytes).unwrap();
            let data = document.data_range().unwrap();
            assert_eq!(document.header.layout.endian, endian);
            assert_eq!(data.len(), data_payload.len());
            assert_eq!(document.reference_item_indices(8).unwrap(), vec![2]);
            assert_eq!(document.collect_item_closure([1]).unwrap(), vec![1, 2]);
            assert!(document.referenced(12).is_none());
            assert_eq!(document.resolve_patched_item(8).unwrap(), 2);
        }
    }

    #[test]
    fn rejects_pointer_read_that_exceeds_data() {
        let item_payload = [
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // count
        ];
        let patch_payload = [1, 0, 0, 0, 1, 0, 0, 0, 12, 0, 0, 0];
        let data_payload = [0u8, 0, 0, 0];
        let bytes = packfile_with_sections(
            Endian::Little,
            vec![
                ("ITEM", item_payload.to_vec()),
                ("PTCH", patch_payload.to_vec()),
                ("DATA", data_payload.to_vec()),
            ],
        );
        let document = crate::parser::physics::hkcl::HkclDocument::parse(&bytes).unwrap();
        assert!(document.resolve_patched_item(12).is_err());
    }
}
