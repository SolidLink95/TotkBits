//! Removes unreachable DATA/ITEM records from a BPHCL after native edits.
//!
//! The ordinary merge and delete paths keep every allocation in place and
//! only rewrite root references, so a removed cloth leaves its data behind.
//! Compaction walks the live graph from the root container, copies only the
//! reachable ranges at their declared type alignment, renumbers the ITEM
//! table, and rewrites every PTCH pointer to the new indices. Two diagnostic
//! variants renumber items without moving data; they exist to prove whether
//! the game depends on ITEM identities outside the relocation graph.

use crate::parser::bphcl::{BphclBuilder, BphclDocument, Item, Patch};
use std::{
    collections::{BTreeMap, HashMap},
    io::{self, ErrorKind},
};

#[derive(Clone, Copy, Debug)]
struct CompactRange {
    old_start: u32,
    old_end: u32,
    new_start: u32,
}

/// Drops every unreachable allocation and renumbers the surviving items.
pub fn compact(document: &BphclDocument) -> io::Result<Vec<u8>> {
    compact_with(document, false)
}

/// Diagnostic: moves only the reachable DATA ranges while keeping the
/// original ITEM identities, so a dead slot stays a dead slot.
pub fn compact_preserving_item_indices(document: &BphclDocument) -> io::Result<Vec<u8>> {
    compact_with(document, true)
}

/// Diagnostic: keeps every allocation but reverses every non-null ITEM
/// identity. If the result fails in-game, ITEM indices matter outside the
/// PTCH relocation graph.
pub fn reindex_all_items_for_diagnostic(document: &BphclDocument) -> io::Result<Vec<u8>> {
    ensure_no_external_patches(document)?;
    ensure_null_sentinel(document)?;
    let count = document.items.len();
    let mut map: HashMap<usize, usize> = HashMap::from([(0, 0)]);
    for old in 1..count {
        map.insert(old, count - old);
    }
    let mut items = vec![
        Item {
            flags: 0,
            type_index: 0,
            data_offset: 0,
            count: 0,
        };
        count
    ];
    for (old, new) in &map {
        items[*new] = document.items[*old].clone();
    }
    let mut data = data_payload(document)?.to_vec();
    for patch in &document.patches {
        for offset in &patch.offsets {
            let old = read_pointer(&data, *offset)?;
            let new = map.get(&old).ok_or_else(|| {
                invalid(&format!(
                    "PTCH at DATA+{offset:#x} has an invalid ITEM target"
                ))
            })?;
            write_pointer(&mut data, *offset, *new)?;
        }
    }
    rebuild(document, data, items, document.patches.clone())
}

/// Diagnostic: leaves the active root graph's ITEM identities alone and
/// shuffles only inactive records.
pub fn reindex_inactive_items_for_diagnostic(document: &BphclDocument) -> io::Result<Vec<u8>> {
    ensure_no_external_patches(document)?;
    ensure_null_sentinel(document)?;
    let live = document.collect_item_closure(root_items(document)?)?;
    let inactive: Vec<usize> = (1..document.items.len())
        .filter(|index| !live.contains(index))
        .collect();
    if inactive.len() < 2 {
        return Err(invalid(
            "BPHCL has fewer than two inactive ITEM records to shuffle",
        ));
    }
    let mut map: Vec<usize> = (0..document.items.len()).collect();
    for (position, old) in inactive.iter().enumerate() {
        map[*old] = inactive[inactive.len() - 1 - position];
    }
    let mut items = document.items.clone();
    for old in &inactive {
        items[map[*old]] = document.items[*old].clone();
    }
    let mut data = data_payload(document)?.to_vec();
    for patch in &document.patches {
        for offset in &patch.offsets {
            let old = read_pointer(&data, *offset)?;
            let new = map.get(old).ok_or_else(|| {
                invalid(&format!(
                    "PTCH at DATA+{offset:#x} has an invalid ITEM target"
                ))
            })?;
            write_pointer(&mut data, *offset, *new)?;
        }
    }
    rebuild(document, data, items, document.patches.clone())
}

fn compact_with(document: &BphclDocument, preserve_item_indices: bool) -> io::Result<Vec<u8>> {
    ensure_no_external_patches(document)?;
    ensure_null_sentinel(document)?;
    // ITEM 0 is a real on-disk sentinel: zero in an unrelocated pointer field
    // means null. Never assign a compacted object to that index.
    let live: Vec<usize> = document
        .collect_item_closure(root_items(document)?)?
        .into_iter()
        .filter(|index| *index != 0)
        .collect();
    if live.is_empty() {
        return Err(invalid(
            "BPHCL compaction found no live non-null ITEM records",
        ));
    }
    let live_patches = document.patches_for_items(live.iter().copied())?;
    let ranges = document.item_ranges()?;
    let payload = data_payload(document)?;
    let range_of = |index: usize| {
        ranges
            .get(&index)
            .copied()
            .ok_or_else(|| invalid(&format!("ITEM {index} has no DATA range")))
    };
    let first_live_start = live
        .iter()
        .map(|index| range_of(*index).map(|range| range.start))
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .min()
        .unwrap_or(0);
    let mut alignment_by_type: HashMap<u32, u32> = HashMap::new();
    for body in &document.type_table.bodies {
        alignment_by_type
            .entry(body.type_index)
            .or_insert(body.alignment.unwrap_or(1).max(1));
    }
    let mut item_index_map: HashMap<usize, usize> = HashMap::from([(0, 0)]);
    for old in &live {
        let new = if preserve_item_indices {
            *old
        } else {
            item_index_map.len()
        };
        item_index_map.insert(*old, new);
    }

    // Group live items by range start; items sharing a start share one copy
    // laid out at the strictest of their alignments.
    let mut groups: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for index in &live {
        groups
            .entry(range_of(*index)?.start)
            .or_default()
            .push(*index);
    }
    // DATA begins with a small root pointer prefix rather than an ITEM.
    // Preserve it so the loader can still find hkRootLevelContainer's
    // named-variant array after every ITEM is renumbered.
    let mut data = payload
        .get(..first_live_start as usize)
        .ok_or_else(|| invalid("BPHCL first live ITEM lies outside DATA"))?
        .to_vec();
    let mut copies: HashMap<u32, CompactRange> = HashMap::new();
    for (start, members) in &groups {
        let range = range_of(members[0])?;
        if range.end < range.start || range.end as usize > payload.len() {
            return Err(invalid("BPHCL live ITEM range exceeds DATA"));
        }
        let alignment = members
            .iter()
            .map(|index| {
                document
                    .items
                    .get(*index)
                    .and_then(|item| alignment_by_type.get(&item.type_index))
                    .copied()
                    .unwrap_or(1)
            })
            .max()
            .unwrap_or(1)
            .max(1) as usize;
        while data.len() % alignment != 0 {
            data.push(0);
        }
        let new_start =
            u32::try_from(data.len()).map_err(|_| invalid("compacted DATA exceeds u32"))?;
        data.extend_from_slice(&payload[range.start as usize..range.end as usize]);
        copies.insert(
            *start,
            CompactRange {
                old_start: range.start,
                old_end: range.end,
                new_start,
            },
        );
    }

    let placeholder = Item {
        flags: 0,
        type_index: 0,
        data_offset: 0,
        count: 0,
    };
    let mut items = vec![
        placeholder;
        if preserve_item_indices {
            document.items.len()
        } else {
            live.len() + 1
        }
    ];
    items[0] = document.items[0].clone();
    for old in &live {
        let item = &document.items[*old];
        let copied = copies[&range_of(*old)?.start];
        let mut moved = item.clone();
        moved.data_offset = copied.new_start + item.data_offset - copied.old_start;
        items[item_index_map[old]] = moved;
    }

    let mut patches = Vec::new();
    for patch in &live_patches {
        let mut offsets = Vec::with_capacity(patch.offsets.len());
        for old_offset in &patch.offsets {
            let copied = find_range(&copies, *old_offset)?;
            let new_offset = copied.new_start + old_offset - copied.old_start;
            let original = read_pointer(payload, *old_offset)?;
            let new_index = item_index_map.get(&original).ok_or_else(|| {
                invalid(&format!(
                    "live BPHCL pointer at DATA+{old_offset:#x} reaches an item outside the compacted graph"
                ))
            })?;
            write_pointer(&mut data, new_offset, *new_index)?;
            offsets.push(new_offset);
        }
        patches.push(Patch {
            type_index: patch.type_index,
            offsets,
        });
    }
    // The prefix pointer(s) are not inside an ITEM range, so they are not
    // part of the live patches. They still point into the live graph and
    // must be remapped to the compact ITEM indices.
    for patch in &document.patches {
        let mut offsets = Vec::new();
        for offset in patch
            .offsets
            .iter()
            .filter(|offset| **offset < first_live_start)
        {
            let original = read_pointer(payload, *offset)?;
            let new_index = item_index_map.get(&original).ok_or_else(|| {
                invalid(&format!(
                    "BPHCL root pointer at DATA+{offset:#x} reaches an item outside the compacted graph"
                ))
            })?;
            write_pointer(&mut data, *offset, *new_index)?;
            offsets.push(*offset);
        }
        if !offsets.is_empty() {
            patches.push(Patch {
                type_index: patch.type_index,
                offsets,
            });
        }
    }
    rebuild(document, data, items, patches)
}

fn rebuild(
    document: &BphclDocument,
    data: Vec<u8>,
    items: Vec<Item>,
    patches: Vec<Patch>,
) -> io::Result<Vec<u8>> {
    let mut builder = BphclBuilder::new(document)?;
    builder.data = data;
    builder.items = items;
    builder.patches = patches;
    builder.build()
}

/// hkRootLevelContainer is the implicit object at DATA+0. The loader starts
/// there directly; it is not itself the target of a PTCH entry. Keep its ITEM
/// record as a real root alongside its named-variant data.
fn root_items(document: &BphclDocument) -> io::Result<Vec<usize>> {
    let named_variants = referenced_item(document, 0)
        .ok_or_else(|| invalid("BPHCL root named-variant array could not be resolved"))?;
    let containers: Vec<usize> = document
        .items
        .iter()
        .enumerate()
        .filter(|(index, item)| {
            *index != 0
                && item.data_offset == 0
                && document
                    .type_names
                    .get(item.type_index as usize)
                    .map(String::as_str)
                    == Some("hkRootLevelContainer")
        })
        .map(|(index, _)| index)
        .collect();
    let [container] = containers.as_slice() else {
        return Err(invalid(&format!(
            "BPHCL expected one implicit hkRootLevelContainer ITEM at DATA+0, found {}",
            containers.len()
        )));
    };
    let mut roots = vec![*container, named_variants];
    roots.extend(document.variants.iter().map(|variant| variant.item_index));
    Ok(roots)
}

fn referenced_item(document: &BphclDocument, offset: u32) -> Option<usize> {
    if !document
        .patches
        .iter()
        .any(|patch| patch.offsets.contains(&offset))
    {
        return None;
    }
    let payload = data_payload(document).ok()?;
    let index = read_pointer(payload, offset).ok()?;
    (index < document.items.len()).then_some(index)
}

fn data_payload(document: &BphclDocument) -> io::Result<&[u8]> {
    let section = document
        .tag
        .find("DATA")
        .ok_or_else(|| invalid("BPHCL has no DATA section"))?;
    document
        .raw
        .get(section.payload_offset..section.payload_end())
        .ok_or_else(|| invalid("BPHCL DATA exceeds input"))
}

fn ensure_null_sentinel(document: &BphclDocument) -> io::Result<()> {
    if document
        .items
        .first()
        .is_none_or(|item| item.type_index != 0)
    {
        return Err(invalid(
            "BPHCL ITEM 0 is expected to be the null-pointer sentinel",
        ));
    }
    Ok(())
}

/// External PTCH records (the group after the zero terminator) relocate data
/// the compactor does not track, so their presence is refused rather than
/// silently dropped.
fn ensure_no_external_patches(document: &BphclDocument) -> io::Result<()> {
    let section = document
        .tag
        .find("PTCH")
        .ok_or_else(|| invalid("BPHCL INDX has no PTCH section"))?;
    let bytes = &document.raw;
    let mut cursor = section.payload_offset;
    let end = section.payload_end();
    let word = |offset: usize| -> io::Result<u32> {
        bytes
            .get(offset..offset + 4)
            .map(|slice| u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
            .ok_or_else(|| invalid("truncated BPHCL PTCH"))
    };
    while cursor + 4 <= end {
        let type_index = word(cursor)?;
        cursor += 4;
        if type_index == 0 {
            if cursor != end {
                return Err(io::Error::new(
                    ErrorKind::Unsupported,
                    "BPHCL compaction does not support external PTCH records",
                ));
            }
            return Ok(());
        }
        if cursor + 4 > end {
            return Err(invalid("truncated BPHCL PTCH count"));
        }
        let count = word(cursor)? as usize;
        cursor = cursor
            .checked_add(4 + count * 4)
            .ok_or_else(|| invalid("BPHCL PTCH entry overflows"))?;
        if cursor > end {
            return Err(invalid("BPHCL PTCH entry exceeds its section"));
        }
    }
    if cursor != end {
        return Err(invalid("BPHCL PTCH ends in a truncated patch group"));
    }
    Ok(())
}

fn find_range(copies: &HashMap<u32, CompactRange>, offset: u32) -> io::Result<CompactRange> {
    copies
        .values()
        .find(|range| offset >= range.old_start && offset < range.old_end)
        .copied()
        .ok_or_else(|| {
            invalid(&format!(
                "BPHCL patch at DATA+{offset:#x} is outside the compacted graph"
            ))
        })
}

fn read_pointer(data: &[u8], offset: u32) -> io::Result<usize> {
    let start = offset as usize;
    let bytes = data
        .get(start..start + 4)
        .ok_or_else(|| invalid(&format!("BPHCL pointer at DATA+{offset:#x} exceeds DATA")))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize)
}

fn write_pointer(data: &mut [u8], offset: u32, item_index: usize) -> io::Result<()> {
    let start = offset as usize;
    let value = u32::try_from(item_index).map_err(|_| invalid("ITEM index exceeds u32"))?;
    data.get_mut(start..start + 4)
        .ok_or_else(|| invalid(&format!("BPHCL pointer at DATA+{offset:#x} exceeds DATA")))?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

impl BphclDocument {
    /// Removes a cloth, prunes its orphaned colliders, compacts the DATA
    /// section, and checks the result the way PhysicsTool's save path does.
    pub fn remove_cloth_and_compact(&self, cloth_index: usize) -> io::Result<Vec<u8>> {
        let removed = self.remove_cloth(cloth_index)?;
        let compacted = compact(&BphclDocument::parse(&removed)?)?;
        let result = BphclDocument::parse(&compacted)?;
        result.validate_item_graph()?;
        if result.cloth.len() != self.cloth.len() - 1 {
            return Err(invalid(&format!(
                "BPHCL compaction produced {} cloths; expected {}",
                result.cloth.len(),
                self.cloth.len() - 1
            )));
        }
        let referenced = result.referenced_collidable_items();
        if result
            .collidables
            .iter()
            .any(|collider| !referenced.contains(&collider.item_index))
        {
            return Err(invalid(
                "BPHCL compaction left unreferenced colliders in the active collider array",
            ));
        }
        Ok(compacted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn corpus() -> Vec<(String, BphclDocument)> {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/_bphcl");
        let Ok(entries) = fs::read_dir(&directory) else {
            eprintln!("skipping: {} is missing", directory.display());
            return Vec::new();
        };
        let mut paths: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|value| value == "bphcl"))
            .collect();
        paths.sort();
        paths
            .into_iter()
            .map(|path| {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                let document = BphclDocument::parse(&fs::read(&path).unwrap())
                    .unwrap_or_else(|error| panic!("{name}: {error}"));
                (name, document)
            })
            .collect()
    }

    fn signature(document: &BphclDocument) -> Vec<String> {
        let mut values: Vec<String> = document
            .cloth
            .iter()
            .map(|cloth| {
                format!(
                    "cloth {} sims={} particles={:?}",
                    cloth.name,
                    cloth.simulations.len(),
                    cloth
                        .simulations
                        .iter()
                        .map(|simulation| simulation.particles.len())
                        .collect::<Vec<_>>()
                )
            })
            .collect();
        values.extend(
            document
                .collidables
                .iter()
                .map(|collider| format!("collider {} {:?}", collider.name, collider.shape)),
        );
        values.extend(
            document.skeletons.iter().map(|skeleton| {
                format!("skeleton {} bones={}", skeleton.name, skeleton.bones.len())
            }),
        );
        values.push(format!("variants {}", document.variants.len()));
        values
    }

    #[test]
    fn compaction_keeps_the_live_graph_and_is_idempotent() {
        let mut compacted_count = 0;
        let mut saved_bytes = 0usize;
        for (name, document) in corpus() {
            let compacted = match compact(&document) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == ErrorKind::Unsupported => continue,
                Err(error) => panic!("{name}: {error}"),
            };
            let result = BphclDocument::parse(&compacted)
                .unwrap_or_else(|error| panic!("{name}: compacted file failed to parse: {error}"));
            result
                .validate_item_graph()
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(
                signature(&result),
                signature(&document),
                "{name}: live graph changed"
            );
            assert!(
                compacted.len() <= document.raw.len(),
                "{name}: compaction grew the file"
            );
            assert_eq!(
                result.items[0].type_index, 0,
                "{name}: null sentinel survives"
            );
            assert!(
                result.items.len() <= document.items.len(),
                "{name}: compaction never adds items"
            );
            let again = compact(&result).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(again, compacted, "{name}: compaction is idempotent");
            saved_bytes += document.raw.len() - compacted.len();
            compacted_count += 1;
        }
        eprintln!("compacted {compacted_count} files, reclaimed {saved_bytes} bytes");
    }

    #[test]
    fn preserving_item_indices_keeps_identities_and_moves_only_data() {
        for (name, document) in corpus().into_iter().take(40) {
            let compacted = match compact_preserving_item_indices(&document) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == ErrorKind::Unsupported => continue,
                Err(error) => panic!("{name}: {error}"),
            };
            let result = BphclDocument::parse(&compacted).unwrap();
            result.validate_item_graph().unwrap();
            assert_eq!(
                result.items.len(),
                document.items.len(),
                "{name}: item table length"
            );
            for cloth in &document.cloth {
                let same = result
                    .cloth
                    .iter()
                    .find(|candidate| candidate.name == cloth.name)
                    .unwrap();
                assert_eq!(
                    same.item_index, cloth.item_index,
                    "{name}: cloth ITEM identity"
                );
            }
            assert_eq!(signature(&result), signature(&document));
        }
    }

    #[test]
    fn diagnostic_reindexing_keeps_the_graph_readable() {
        for (name, document) in corpus().into_iter().take(40) {
            let reversed = match reindex_all_items_for_diagnostic(&document) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == ErrorKind::Unsupported => continue,
                Err(error) => panic!("{name}: {error}"),
            };
            let result = BphclDocument::parse(&reversed).unwrap();
            result.validate_item_graph().unwrap();
            assert_eq!(
                signature(&result),
                signature(&document),
                "{name}: reversed identities"
            );
            assert_eq!(reversed.len(), document.raw.len());
            match reindex_inactive_items_for_diagnostic(&document) {
                Ok(bytes) => {
                    let result = BphclDocument::parse(&bytes).unwrap();
                    result.validate_item_graph().unwrap();
                    assert_eq!(
                        signature(&result),
                        signature(&document),
                        "{name}: shuffled inactive"
                    );
                }
                Err(error) => assert!(
                    error.to_string().contains("fewer than two inactive"),
                    "{name}: {error}"
                ),
            }
        }
    }

    #[test]
    fn removing_a_cloth_then_compacting_drops_its_data() {
        let mut checked = 0;
        for (name, document) in corpus() {
            if document.cloth.len() < 2 || document.collidables.is_empty() {
                continue;
            }
            let compacted = match document.remove_cloth_and_compact(1) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == ErrorKind::Unsupported => continue,
                Err(error) => panic!("{name}: {error}"),
            };
            let removed = document.remove_cloth(1).unwrap();
            assert!(
                compacted.len() < removed.len(),
                "{name}: compaction reclaims the removed cloth"
            );
            let result = BphclDocument::parse(&compacted).unwrap();
            assert_eq!(result.cloth.len(), document.cloth.len() - 1);
            assert!(!result
                .cloth
                .iter()
                .any(|cloth| cloth.name == document.cloth[1].name));
            checked += 1;
        }
        eprintln!("checked cloth removal with compaction on {checked} files");
    }
}
