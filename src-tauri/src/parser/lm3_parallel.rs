//! Multithreaded Luigi's Mansion 3 slot reading.
//!
//! Opening a slot spends nearly all of its time inflating a handful of large
//! zlib entries out of `*.data`. Those inflations are independent, so they are
//! split across three worker threads grouped by what they feed:
//!
//! * **model** — entry 54, the vertex and index buffers.
//! * **textures** — entries 63 and 65, the texture headers and image data.
//! * **rest** — entry 53, the skeleton.
//!
//! Entries 0 (the chunk sub-entry table) and 52 (model/material data) are read
//! first on the calling thread because every worker's consumer needs them:
//! the table locates all chunks, and 52 holds both the mesh descriptors and
//! the material records that texture resolution walks. Parsing then runs
//! through the same code path as the sequential reader.

use super::lm3::{Lm3Archive, Lm3Model, Lm3SlotFiles, Lm3TextureSource};
use crate::parser::AOC::g1m::ResolvedG1tTexture;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

/// Where the time went, for comparing the two readers.
#[derive(Clone, Copy, Debug, Default)]
pub struct Lm3ParseTiming {
    pub open: Duration,
    /// Entries every worker's consumer needs, always read on one thread.
    pub shared: Duration,
    /// The remaining entries: concurrent for the parallel reader.
    pub workers: Duration,
    pub decompress: Duration,
    pub parse: Duration,
    pub total: Duration,
}

impl Lm3ParseTiming {
    pub fn describe(&self, label: &str) -> String {
        format!(
            "{label}: total {:.1}ms (open {:.1}ms, decompress {:.1}ms \
             [shared {:.1}ms + rest {:.1}ms], parse {:.1}ms)",
            self.total.as_secs_f64() * 1000.0,
            self.open.as_secs_f64() * 1000.0,
            self.decompress.as_secs_f64() * 1000.0,
            self.shared.as_secs_f64() * 1000.0,
            self.workers.as_secs_f64() * 1000.0,
            self.parse.as_secs_f64() * 1000.0,
        )
    }
}

type SlotResult = (Lm3Model, Vec<ResolvedG1tTexture>);

/// Textures decoded per worker thread once the entries are inflated.
pub const IMAGES_PER_THREAD: usize = 8;

/// Reads a slot the fastest way: three worker threads inflate the entries,
/// then geometry parses on one thread while textures decode on one thread
/// per [`IMAGES_PER_THREAD`] images.
pub fn parse_slot_parallel(
    dict_path: &Path,
    archive_name: &str,
    slot: usize,
) -> io::Result<SlotResult> {
    parse_slot_pipelined_timed(dict_path, archive_name, slot, IMAGES_PER_THREAD)
        .map(|(result, _)| result)
}

/// Three inflate workers, then everything parses on the calling thread.
pub fn parse_slot_parallel_timed(
    dict_path: &Path,
    archive_name: &str,
    slot: usize,
) -> io::Result<(SlotResult, Lm3ParseTiming)> {
    parse_slot_timed_with(dict_path, archive_name, slot, None)
}

/// Three inflate workers, then geometry and texture decoding run on their
/// own threads (`images_per_thread` textures per decode thread).
pub fn parse_slot_pipelined_timed(
    dict_path: &Path,
    archive_name: &str,
    slot: usize,
    images_per_thread: usize,
) -> io::Result<(SlotResult, Lm3ParseTiming)> {
    parse_slot_timed_with(dict_path, archive_name, slot, Some(images_per_thread))
}

fn parse_slot_timed_with(
    dict_path: &Path,
    archive_name: &str,
    slot: usize,
    images_per_thread: Option<usize>,
) -> io::Result<(SlotResult, Lm3ParseTiming)> {
    let started = Instant::now();
    let archive = Lm3Archive::open(dict_path)?;
    let opened = started.elapsed();

    let decompress_started = Instant::now();
    // Shared prerequisites: the chunk table and the model/material entry are
    // needed by more than one worker, so reading them here avoids either
    // duplicating the work or serialising the workers behind each other.
    let table = archive.entry_bytes(super::lm3::TABLE_ENTRY)?;
    let file52 = archive.entry_bytes(super::lm3::MODEL_DATA_ENTRY)?;
    let shared_elapsed = decompress_started.elapsed();

    let workers_started = Instant::now();
    // A worker panic must not take the app down with it; joining reports it
    // as an ordinary read failure instead.
    let worker_panicked = || {
        io::Error::new(
            io::ErrorKind::Other,
            "an LM3 archive worker thread panicked",
        )
    };
    let (file54, textures, file53, shared): (_, _, _, Option<Lm3TextureSource>) =
        std::thread::scope(|scope| {
            let archive = &archive;
            let model = scope.spawn(move || archive.entry_bytes(super::lm3::BUFFER_ENTRY));
            let textures = scope.spawn(move || {
                (
                    archive.entry_bytes(super::lm3::TEXTURE_HEADER_ENTRY).ok(),
                    archive.entry_bytes(super::lm3::TEXTURE_DATA_ENTRY).ok(),
                )
            });
            let rest = scope.spawn(move || archive.entry_bytes(super::lm3::SKELETON_ENTRY).ok());
            // Non-global archives borrow textures from `global`; reading that store
            // alongside the local entries keeps it off the critical path.
            let shared =
                scope.spawn(move || super::lm3::shared_texture_source(dict_path, archive_name));
            (
                model.join().unwrap_or_else(|_| Err(worker_panicked())),
                textures.join().unwrap_or((None, None)),
                rest.join().unwrap_or(None),
                shared.join().unwrap_or(None),
            )
        });
    let workers = workers_started.elapsed();
    let (file63, file65) = textures;
    let files = Lm3SlotFiles {
        table,
        file52,
        file53,
        file54: file54?,
        file63,
        file65,
    };
    let decompressed = decompress_started.elapsed();

    let parse_started = Instant::now();
    let result = super::lm3::parse_slot_from_files_with(
        &files,
        archive_name,
        slot,
        shared.as_ref(),
        images_per_thread,
    )?;
    Ok((
        result,
        Lm3ParseTiming {
            open: opened,
            shared: shared_elapsed,
            workers,
            decompress: decompressed,
            parse: parse_started.elapsed(),
            total: started.elapsed(),
        },
    ))
}

/// Reads a slot on one thread, timed the same way for comparison.
pub fn parse_slot_sequential_timed(
    dict_path: &Path,
    archive_name: &str,
    slot: usize,
) -> io::Result<(SlotResult, Lm3ParseTiming)> {
    let started = Instant::now();
    let archive = Lm3Archive::open(dict_path)?;
    let opened = started.elapsed();

    // Split the same way as the parallel reader so the two are comparable.
    let decompress_started = Instant::now();
    let table = archive.entry_bytes(super::lm3::TABLE_ENTRY)?;
    let file52 = archive.entry_bytes(super::lm3::MODEL_DATA_ENTRY)?;
    let shared = decompress_started.elapsed();

    let workers_started = Instant::now();
    let file54 = archive.entry_bytes(super::lm3::BUFFER_ENTRY)?;
    let file63 = archive.entry_bytes(super::lm3::TEXTURE_HEADER_ENTRY).ok();
    let file65 = archive.entry_bytes(super::lm3::TEXTURE_DATA_ENTRY).ok();
    let file53 = archive.entry_bytes(super::lm3::SKELETON_ENTRY).ok();
    let workers = workers_started.elapsed();
    let files = Lm3SlotFiles {
        table,
        file52,
        file53,
        file54,
        file63,
        file65,
    };
    let decompressed = decompress_started.elapsed();

    let parse_started = Instant::now();
    let fallback = super::lm3::shared_texture_source(dict_path, archive_name);
    let result = super::lm3::parse_slot_from_files(&files, archive_name, slot, fallback.as_ref())?;
    Ok((
        result,
        Lm3ParseTiming {
            open: opened,
            shared,
            workers,
            decompress: decompressed,
            parse: parse_started.elapsed(),
            total: started.elapsed(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn romfs() -> Option<std::path::PathBuf> {
        ["E:/Yuzu/dumps/LM3/romfs", "W:/coding/TotkBits/tmp/lm3"]
            .iter()
            .map(std::path::PathBuf::from)
            .find(|path| path.join("global.dict").is_file())
    }

    /// Slot 13 stores its two materials in a small per-slot chunk, so one of
    /// them sits at offset zero. Both share a base colour, which only resolves
    /// when a zero pointer is read as an offset rather than a "no material"
    /// marker.
    #[test]
    fn small_material_chunks_resolve_a_zero_pointer() {
        let Some(romfs) = romfs() else { return };
        let (model, _) = parse_slot_parallel(&romfs.join("global.dict"), "global", 13).unwrap();
        assert_eq!(model.materials.len(), 2);
        for (index, material) in model.materials.iter().enumerate() {
            let base = material
                .texture_slots
                .iter()
                .find(|slot| slot.texture_type == "Base color")
                .map(|slot| slot.name.as_str());
            assert_eq!(base, Some("6F96328D"), "slot 13 material {index}");
        }
    }

    /// Scarescraper costumes borrow textures from `global`, and their texture
    /// entry is truncated; both must degrade to a fully textured model.
    #[test]
    fn scarescraper_slots_resolve_shared_textures() {
        let Some(romfs) = romfs() else { return };
        let dict = romfs.join("Scarescraper/Persistent.dict");
        if !dict.is_file() {
            return;
        }
        let (model, textures) = parse_slot_parallel(&dict, "persistent", 38).unwrap();
        assert_eq!(model.materials.len(), 13);
        assert!(
            model.materials.iter().all(|material| material
                .texture_slots
                .iter()
                .any(|slot| slot.texture_type == "Base color")),
            "every Scarescraper material needs a base colour"
        );
        assert!(
            textures.len() >= 30,
            "expected the shared global textures to decode, got {}",
            textures.len()
        );
    }

    /// Diagnostic sweep: reports how many meshes of each slot resolve a base
    /// colour, so a regression in material resolution is visible per archive.
    #[test]
    #[ignore = "diagnostic sweep over the configured LM3 romfs"]
    fn report_material_resolution_across_slots() {
        let Some(romfs) = romfs() else { return };
        for (archive, dict, slots) in [
            (
                "global",
                romfs.join("global.dict"),
                vec![13usize, 27, 7, 0, 1],
            ),
            (
                "persistent",
                romfs.join("Scarescraper/Persistent.dict"),
                vec![38usize, 42, 48, 50, 54],
            ),
        ] {
            if !dict.is_file() {
                eprintln!("{archive}: archive missing");
                continue;
            }
            for slot in slots {
                match parse_slot_parallel(&dict, archive, slot) {
                    Ok((model, textures)) => {
                        let with_base = model
                            .materials
                            .iter()
                            .filter(|material| {
                                material
                                    .texture_slots
                                    .iter()
                                    .any(|s| s.texture_type == "Base color")
                            })
                            .count();
                        eprintln!(
                            "{archive}_{slot}: {} meshes, {} materials, {with_base} with base colour, \
                             {} decoded, {} bones",
                            model.render.meshes.len(),
                            model.materials.len(),
                            textures.len(),
                            model.render.bones.len()
                        );
                        for (index, material) in model.materials.iter().enumerate().take(4) {
                            eprintln!(
                                "    material {index}: {:?}",
                                material
                                    .texture_slots
                                    .iter()
                                    .map(|s| format!("{}={}", s.texture_type, s.name))
                                    .collect::<Vec<_>>()
                            );
                        }
                    }
                    Err(error) => eprintln!("{archive}_{slot}: FAILED {error}"),
                }
            }
        }
    }

    /// Both readers must agree exactly; the threads only change when entries
    /// are inflated, never what is parsed out of them.
    #[test]
    fn parallel_and_sequential_readers_agree_and_are_timed() {
        let candidates = [
            "../../LuigiMansion3Mods/tmp/ghost_luigi/installer_test_reference/global.dict",
            "../tmp/lm3/global.dict",
        ];
        let Some(path) = candidates
            .iter()
            .map(|candidate| Path::new(env!("CARGO_MANIFEST_DIR")).join(candidate))
            .find(|path| path.is_file())
        else {
            return;
        };

        let ((sequential, sequential_textures), sequential_timing) =
            parse_slot_sequential_timed(&path, "global", 27).unwrap();
        let ((parallel, parallel_textures), parallel_timing) =
            parse_slot_parallel_timed(&path, "global", 27).unwrap();

        assert_eq!(sequential.name, parallel.name);
        assert_eq!(
            sequential.render.meshes.len(),
            parallel.render.meshes.len(),
            "mesh count must not depend on threading"
        );
        assert_eq!(sequential.render.bones.len(), parallel.render.bones.len());
        assert_eq!(sequential.materials.len(), parallel.materials.len());
        assert_eq!(sequential_textures.len(), parallel_textures.len());
        for (left, right) in sequential.render.meshes.iter().zip(&parallel.render.meshes) {
            assert_eq!(left.name, right.name);
            assert_eq!(left.positions, right.positions);
            assert_eq!(left.indices, right.indices);
        }
        for (left, right) in sequential_textures.iter().zip(&parallel_textures) {
            assert_eq!(left.name, right.name);
            assert_eq!(left.data_url, right.data_url);
        }

        // Known-good material assignments for this slot, from in-game truth.
        let slot_of = |index: usize| {
            sequential.materials[index]
                .texture_slots
                .iter()
                .map(|slot| (slot.texture_type.as_str(), slot.name.as_str()))
                .collect::<Vec<_>>()
        };
        assert!(
            slot_of(7).contains(&("Base color", "8FA37F4F")),
            "mesh 7 base colour: {:?}",
            slot_of(7)
        );
        assert!(
            slot_of(7).contains(&("Normal", "908EBDDC")),
            "mesh 7 normal: {:?}",
            slot_of(7)
        );
        assert!(
            slot_of(12).contains(&("Base color", "198CFD0B")),
            "mesh 12 base colour: {:?}",
            slot_of(12)
        );
        assert_eq!(sequential.render.bones.len(), 223, "slot 27 skeleton");

        eprintln!("{}", sequential_timing.describe("sequential"));
        eprintln!("{}", parallel_timing.describe("3 threads  "));
    }

    /// The pipelined reader must produce exactly what the single-thread parse
    /// produces, and this reports how much faster it is on the reference
    /// slots (run with `--nocapture`).
    #[test]
    fn pipelined_reader_matches_and_benchmarks_reference_slots() {
        let Some(romfs) = romfs() else { return };
        let dict = romfs.join("global.dict");
        const ROUNDS: usize = 3;
        let mut old_total = Duration::ZERO;
        let mut new_total = Duration::ZERO;
        for slot in [27usize, 30, 34] {
            let mut old_best = Duration::MAX;
            let mut new_best = Duration::MAX;
            let mut old_parse = Duration::MAX;
            let mut new_parse = Duration::MAX;
            let mut old_result = None;
            let mut new_result = None;
            for _ in 0..ROUNDS {
                let (result, timing) = parse_slot_parallel_timed(&dict, "global", slot).unwrap();
                old_best = old_best.min(timing.total);
                old_parse = old_parse.min(timing.parse);
                old_result = Some(result);
                let (result, timing) =
                    parse_slot_pipelined_timed(&dict, "global", slot, IMAGES_PER_THREAD).unwrap();
                if timing.total < new_best {
                    eprintln!("{}", timing.describe(&format!("global_{slot} new")));
                }
                new_best = new_best.min(timing.total);
                new_parse = new_parse.min(timing.parse);
                new_result = Some(result);
            }
            let (old_model, old_textures) = old_result.unwrap();
            let (new_model, new_textures) = new_result.unwrap();
            assert_eq!(old_model.render.meshes.len(), new_model.render.meshes.len());
            assert_eq!(old_model.render.bones.len(), new_model.render.bones.len());
            assert_eq!(old_model.materials.len(), new_model.materials.len());
            for (left, right) in old_model.materials.iter().zip(&new_model.materials) {
                let slots = |material: &crate::parser::AOC::g1m::G1mMaterial| {
                    material
                        .texture_slots
                        .iter()
                        .map(|slot| (slot.texture_type.clone(), slot.name.clone()))
                        .collect::<Vec<_>>()
                };
                assert_eq!(slots(left), slots(right), "slot {slot} materials");
            }
            for (left, right) in old_model.render.meshes.iter().zip(&new_model.render.meshes) {
                assert_eq!(
                    left.positions, right.positions,
                    "slot {slot} mesh {}",
                    left.name
                );
                assert_eq!(left.bone_weights, right.bone_weights);
            }
            assert_eq!(
                old_textures.len(),
                new_textures.len(),
                "slot {slot} textures"
            );
            for (left, right) in old_textures.iter().zip(&new_textures) {
                assert_eq!(left.name, right.name);
                assert_eq!(left.data_url, right.data_url);
            }
            eprintln!(
                "global_{slot}: {} textures | old {:.1}ms (parse {:.1}ms) | new {:.1}ms (parse {:.1}ms) | {:.2}x",
                new_textures.len(),
                old_best.as_secs_f64() * 1000.0,
                old_parse.as_secs_f64() * 1000.0,
                new_best.as_secs_f64() * 1000.0,
                new_parse.as_secs_f64() * 1000.0,
                old_best.as_secs_f64() / new_best.as_secs_f64().max(1e-9),
            );
            old_total += old_best;
            new_total += new_best;
        }
        eprintln!(
            "total: old {:.1}ms | new {:.1}ms | {:.2}x",
            old_total.as_secs_f64() * 1000.0,
            new_total.as_secs_f64() * 1000.0,
            old_total.as_secs_f64() / new_total.as_secs_f64().max(1e-9),
        );
    }
}
