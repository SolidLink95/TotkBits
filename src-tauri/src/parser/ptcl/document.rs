use super::{
    animation::{AnimKeyFrame, ANIMATION_SLOT_SIZE},
    emitter::{Emitter, EmitterLocation},
    header::{relative, SectionHeader, END},
};
use crate::parser::binary::{BinaryPatcher, BinaryReader};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};

const MAGIC: &[u8; 8] = b"VFXB    ";
const MAX_SECTIONS: usize = 1_000_000;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct PtclDocument(pub BTreeMap<String, BTreeMap<String, Emitter>>);

#[derive(Clone, Debug)]
pub struct Ptcl {
    pub document: PtclDocument,
    original_data: Vec<u8>,
    locations: BTreeMap<(String, String), EmitterLocation>,
}

impl Ptcl {
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        let start = data
            .windows(MAGIC.len())
            .position(|window| window == MAGIC)
            .ok_or_else(|| invalid("PTCL VFXB magic was not found"))?;
        verify_file(data, start)?;
        let mut reader = BinaryReader::new(data);
        reader.seek(start + 0x16)?;
        let header_size = reader.read_u16()? as usize;
        if header_size < 0x18 {
            return Err(invalid("invalid PTCL header size"));
        }
        let first_block = start
            .checked_add(header_size)
            .ok_or_else(|| invalid("PTCL header offset overflow"))?;
        let esta = find_section(data, first_block, "ESTA")?;
        let first_set = relative(
            esta,
            SectionHeader::read(data, esta)?.section_offset,
            "ESET",
        )?;

        let mut document = PtclDocument::default();
        let mut locations = BTreeMap::new();
        walk_sections(data, first_set, "ESET", |set_offset, set_header| {
            let set_data = relative(set_offset, set_header.section_offset, "emitter set data")?;
            let set_name = read_string(data, set_data + 0x10, 0x60)?;
            let mut emitters = BTreeMap::new();
            if set_header.subsection_offset != END {
                let first_emitter = relative(set_offset, set_header.subsection_offset, "emitter")?;
                walk_sections(data, first_emitter, "EMTR", |emitter_offset, header| {
                    let emitter_data =
                        relative(emitter_offset, header.section_offset, "emitter data")?;
                    let name = read_string(data, emitter_data + 0x10, 0x60)?;
                    let emitter = read_emitter(data, emitter_data)?;
                    locations.insert(
                        (set_name.clone(), name.clone()),
                        EmitterLocation {
                            section: emitter_offset,
                            data: emitter_data,
                        },
                    );
                    emitters.insert(name, emitter);
                    Ok(())
                })?;
            }
            document.0.insert(set_name, emitters);
            Ok(())
        })?;
        Ok(Self {
            document,
            original_data: data.to_vec(),
            locations,
        })
    }

    pub fn to_yaml(&self) -> io::Result<String> {
        serde_yaml::to_string(&self.document).map_err(io::Error::other)
    }

    pub fn apply_yaml(&self, yaml: &str) -> io::Result<Vec<u8>> {
        let changes: PtclDocument = serde_yaml::from_str(yaml).map_err(io::Error::other)?;
        self.apply_document(&changes)
    }

    pub fn apply_document(&self, changes: &PtclDocument) -> io::Result<Vec<u8>> {
        let mut output = self.original_data.clone();
        for (set_name, emitters) in &changes.0 {
            for (emitter_name, emitter) in emitters {
                let Some(original) = self
                    .document
                    .0
                    .get(set_name)
                    .and_then(|original| original.get(emitter_name))
                else {
                    continue;
                };
                if original == emitter {
                    continue;
                }
                let Some(location) = self
                    .locations
                    .get(&(set_name.clone(), emitter_name.clone()))
                else {
                    continue;
                };
                let _section = location.section;
                write_emitter(&mut output, location.data, original, emitter)?;
            }
        }
        Ok(output)
    }
}

fn verify_file(data: &[u8], start: usize) -> io::Result<()> {
    let mut reader = BinaryReader::new(data);
    reader.seek(start)?;
    if reader.read_bytes(8)? != MAGIC {
        return Err(invalid("invalid PTCL magic"));
    }
    let _unknown = reader.read_u8()?;
    if reader.read_u8()? != 4 {
        return Err(invalid("unsupported PTCL graphics API version"));
    }
    if reader.read_u16()? != 0x33 {
        return Err(invalid("unsupported PTCL resource version"));
    }
    if reader.read_u16()? != 0xfeff {
        return Err(invalid("only little-endian PTCL is supported"));
    }
    Ok(())
}

fn find_section(data: &[u8], mut current: usize, signature: &str) -> io::Result<usize> {
    for _ in 0..MAX_SECTIONS {
        let header = SectionHeader::read(data, current)?;
        if header.signature == signature {
            return Ok(current);
        }
        if header.next_section_offset == END {
            break;
        }
        if header.next_section_offset == 0 {
            return Err(invalid("zero PTCL section link"));
        }
        current = relative(current, header.next_section_offset, "section")?;
    }
    Err(invalid(&format!("PTCL section {signature} was not found")))
}

fn walk_sections<F>(
    data: &[u8],
    mut current: usize,
    signature: &str,
    mut visit: F,
) -> io::Result<()>
where
    F: FnMut(usize, &SectionHeader) -> io::Result<()>,
{
    for _ in 0..MAX_SECTIONS {
        let header = SectionHeader::read(data, current)?;
        if header.signature != signature {
            return Err(invalid(&format!(
                "expected {signature}, found {}",
                header.signature
            )));
        }
        visit(current, &header)?;
        if header.next_section_offset == END {
            return Ok(());
        }
        if header.next_section_offset == 0 {
            return Err(invalid("zero PTCL section link"));
        }
        current = relative(current, header.next_section_offset, "section")?;
    }
    Err(invalid("too many PTCL sections"))
}

fn read_emitter(data: &[u8], base: usize) -> io::Result<Emitter> {
    let counts = read_u32x6(data, base + 0x80)?;
    let mut animation = base + 0x680;
    let color_anim0 = read_animation(data, &mut animation, counts[0], 1)?;
    let alpha_anim0 = read_animation(data, &mut animation, counts[1], 1)?;
    let color_anim1 = read_animation(data, &mut animation, counts[2], 1)?;
    let alpha_anim1 = read_animation(data, &mut animation, counts[3], 1)?;
    let scale_anim = read_animation(data, &mut animation, counts[5], 4)?;
    Ok(Emitter {
        const_color0: read_f32x4(data, base + 0xf48)?,
        const_color1: read_f32x4(data, base + 0xf58)?,
        color_anim0,
        color_anim1,
        alpha_anim0,
        alpha_anim1,
        scale_anim,
    })
}

fn read_animation(
    data: &[u8],
    offset: &mut usize,
    count: u32,
    base_count: usize,
) -> io::Result<Vec<AnimKeyFrame>> {
    let count = (count as usize)
        .saturating_add(base_count)
        .min(ANIMATION_SLOT_SIZE / 16);
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        let values = read_f32x4(data, *offset + index * 16)?;
        frames.push(AnimKeyFrame {
            value: [values[0], values[1], values[2]],
            keyframe: values[3],
        });
    }
    *offset = offset
        .checked_add(ANIMATION_SLOT_SIZE)
        .ok_or_else(|| invalid("animation offset overflow"))?;
    Ok(frames)
}

fn write_emitter(
    data: &mut [u8],
    base: usize,
    original: &Emitter,
    emitter: &Emitter,
) -> io::Result<()> {
    if emitter.const_color0 != original.const_color0 {
        write_f32x4(data, base + 0xf48, emitter.const_color0)?;
    }
    if emitter.const_color1 != original.const_color1 {
        write_f32x4(data, base + 0xf58, emitter.const_color1)?;
    }
    let animations = [
        (&original.color_anim0, &emitter.color_anim0, 1, 0),
        (&original.alpha_anim0, &emitter.alpha_anim0, 1, 1),
        (&original.color_anim1, &emitter.color_anim1, 1, 2),
        (&original.alpha_anim1, &emitter.alpha_anim1, 1, 3),
        (&original.scale_anim, &emitter.scale_anim, 4, 5),
    ];
    let mut animation = base + 0x680;
    for (original_frames, frames, minimum, count_index) in animations {
        if original_frames == frames {
            animation += ANIMATION_SLOT_SIZE;
            continue;
        }
        if !(minimum..=8).contains(&frames.len()) {
            return Err(invalid(&format!(
                "animation must contain {minimum} to 8 keyframes, found {}",
                frames.len()
            )));
        }
        let count = (frames.len() - minimum) as u32;
        write_u32(data, base + 0x80 + count_index * 4, count)?;
        write_bytes(data, animation, &[0; ANIMATION_SLOT_SIZE])?;
        for (index, frame) in frames.iter().enumerate() {
            let at = animation + index * 16;
            for (component, value) in frame
                .value
                .iter()
                .chain(std::iter::once(&frame.keyframe))
                .enumerate()
            {
                write_f32(data, at + component * 4, *value)?;
            }
        }
        animation += ANIMATION_SLOT_SIZE;
    }
    Ok(())
}

fn read_string(data: &[u8], offset: usize, max_len: usize) -> io::Result<String> {
    let end = offset
        .checked_add(max_len)
        .ok_or_else(|| invalid("string offset overflow"))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| invalid("string exceeds PTCL data"))?;
    let length = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8(bytes[..length].to_vec())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn read_u32x6(data: &[u8], offset: usize) -> io::Result<[u32; 6]> {
    let mut reader = BinaryReader::new(data);
    reader.seek(offset)?;
    Ok([
        reader.read_u32()?,
        reader.read_u32()?,
        reader.read_u32()?,
        reader.read_u32()?,
        reader.read_u32()?,
        reader.read_u32()?,
    ])
}

fn read_f32x4(data: &[u8], offset: usize) -> io::Result<[f32; 4]> {
    let mut reader = BinaryReader::new(data);
    reader.seek(offset)?;
    Ok([
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
    ])
}

fn write_f32x4(data: &mut [u8], offset: usize, values: [f32; 4]) -> io::Result<()> {
    for (index, value) in values.into_iter().enumerate() {
        write_f32(data, offset + index * 4, value)?;
    }
    Ok(())
}

fn write_u32(data: &mut [u8], offset: usize, value: u32) -> io::Result<()> {
    BinaryPatcher::new(data)
        .write_u32_at(offset, value)
        .map_err(write_error)
}

fn write_f32(data: &mut [u8], offset: usize, value: f32) -> io::Result<()> {
    BinaryPatcher::new(data)
        .write_f32_at(offset, value)
        .map_err(write_error)
}

fn write_bytes(data: &mut [u8], offset: usize, bytes: &[u8]) -> io::Result<()> {
    BinaryPatcher::new(data)
        .write_bytes_at(offset, bytes)
        .map_err(write_error)
}

fn write_error(_: io::Error) -> io::Error {
    invalid("write exceeds PTCL data")
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roead::byml::Byml;
    use sha2::{Digest, Sha256};
    use std::{fs, path::PathBuf};

    #[test]
    fn saved_player_beam_contains_valid_ptcl() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repository root")
            .join("tmp/Effect");
        let clean_data =
            fs::read(directory.join("PlayerBeam.Nin_NX_NVN.esetb.byml")).expect("clean ESETB");
        let compressed =
            fs::read(directory.join("PlayerBeam.Nin_NX_NVN.esetb.byml.zs")).expect("saved ESETB");
        let config = crate::TotkConfig::TotkConfig::new(false).expect("TotkBits config");
        let zstd = crate::Zstd::TotkZstd::new(
            std::sync::Arc::new(config),
            crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
        )
        .expect("TotK Zstandard dictionaries");
        let saved_data = zstd
            .decompress_zs(&compressed)
            .expect("saved Zstandard stream");
        let clean = Byml::from_binary(&clean_data).expect("clean BYML");
        let saved = Byml::from_binary(&saved_data).expect("saved BYML");
        fn ptcl_bin(byml: &Byml) -> &Vec<u8> {
            byml.as_map()
                .expect("ESETB root map")
                .get("PtclBin")
                .and_then(|value| match value {
                    Byml::FileData(data) => Some(data),
                    _ => None,
                })
                .expect("PtclBin")
        }
        let clean_ptcl = ptcl_bin(&clean);
        let saved_ptcl = ptcl_bin(&saved);
        let clean_parsed = Ptcl::parse(clean_ptcl).expect("clean PTCL");
        let saved_parsed = Ptcl::parse(saved_ptcl).expect("saved PTCL");
        eprintln!(
            "BYML lengths clean={} saved={}; PTCL lengths clean={} saved={}; PTCL differing bytes={}",
            clean_data.len(),
            saved_data.len(),
            clean_ptcl.len(),
            saved_ptcl.len(),
            clean_ptcl
                .iter()
                .zip(saved_ptcl)
                .filter(|(left, right)| left != right)
                .count()
        );
        let mut changed_emitters = 0;
        for (set_name, clean_emitters) in &clean_parsed.document.0 {
            for (emitter_name, clean_emitter) in clean_emitters {
                let saved_emitter = &saved_parsed.document.0[set_name][emitter_name];
                if clean_emitter != saved_emitter {
                    changed_emitters += 1;
                    eprintln!(
                        "changed {set_name}/{emitter_name}: color0 {:?} -> {:?}; color1 {:?} -> {:?}; animations_changed={}",
                        clean_emitter.const_color0,
                        saved_emitter.const_color0,
                        clean_emitter.const_color1,
                        saved_emitter.const_color1,
                        clean_emitter.color_anim0 != saved_emitter.color_anim0
                            || clean_emitter.alpha_anim0 != saved_emitter.alpha_anim0
                            || clean_emitter.color_anim1 != saved_emitter.color_anim1
                            || clean_emitter.alpha_anim1 != saved_emitter.alpha_anim1
                            || clean_emitter.scale_anim != saved_emitter.scale_anim
                    );
                    assert_eq!(set_name, "Obj_MasterBeam");
                    assert_eq!(emitter_name, "Glow");
                    assert_eq!(clean_emitter.color_anim0, saved_emitter.color_anim0);
                    assert_eq!(clean_emitter.alpha_anim0, saved_emitter.alpha_anim0);
                    assert_eq!(clean_emitter.color_anim1, saved_emitter.color_anim1);
                    assert_eq!(clean_emitter.alpha_anim1, saved_emitter.alpha_anim1);
                    assert_eq!(clean_emitter.scale_anim, saved_emitter.scale_anim);
                }
            }
        }
        assert_eq!(changed_emitters, 1);

        let repaired = crate::file_format::Esetb::serialize_preserving_original(
            &saved,
            &clean_data,
            roead::Endian::Little,
        );
        assert_eq!(repaired.len(), clean_data.len());
        let repaired_byml = Byml::from_binary(&repaired).expect("repaired BYML");
        assert_eq!(ptcl_bin(&repaired_byml), saved_ptcl);
        let recompressed = zstd
            .compress_zs(&repaired)
            .expect("recompress repaired ESETB");
        assert_eq!(
            zstd.decompress_zs(&recompressed)
                .expect("decompress repaired ESETB"),
            repaired
        );
    }

    #[test]
    fn const_colors_edit_only_their_own_binary_fields() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repository root")
            .join("tmp/Effect");
        let mut tested_emitters = 0;

        for entry in fs::read_dir(directory).expect("Effect samples directory") {
            let path = entry.expect("Effect sample").path();
            if !path.is_file() || !path.to_string_lossy().ends_with(".esetb.byml") {
                continue;
            }
            let byml = Byml::from_binary(&fs::read(&path).expect("Effect sample data"))
                .unwrap_or_else(|error| panic!("{}: invalid BYML: {error}", path.display()));
            let ptcl_data = byml
                .as_map()
                .expect("ESETB root map")
                .get("PtclBin")
                .and_then(|value| match value {
                    Byml::FileData(data) => Some(data),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{}: missing PtclBin", path.display()));
            let ptcl = Ptcl::parse(ptcl_data)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));

            for ((set_name, emitter_name), location) in &ptcl.locations {
                for (field_offset, replacement, color_index) in [
                    (0xf48, [0.125, 0.25, 0.5, 1.0], 0),
                    (0xf58, [1.0, 0.5, 0.25, 0.125], 1),
                ] {
                    let mut edited = ptcl.document.clone();
                    let emitter = edited
                        .0
                        .get_mut(set_name)
                        .and_then(|emitters| emitters.get_mut(emitter_name))
                        .expect("located emitter");
                    if color_index == 0 {
                        emitter.const_color0 = replacement;
                    } else {
                        emitter.const_color1 = replacement;
                    }

                    let actual = ptcl.apply_document(&edited).unwrap_or_else(|error| {
                        panic!("{}: failed to edit const color: {error}", path.display())
                    });
                    let mut expected = ptcl_data.clone();
                    write_f32x4(&mut expected, location.data + field_offset, replacement).unwrap();
                    assert_eq!(
                        actual,
                        expected,
                        "{}: const_color{color_index} modified unrelated PTCL bytes",
                        path.display()
                    );
                    let reparsed = Ptcl::parse(&actual).unwrap_or_else(|error| {
                        panic!(
                            "{}: const color edit corrupted PTCL: {error}",
                            path.display()
                        )
                    });
                    assert_eq!(
                        reparsed.document,
                        edited,
                        "{}: const_color{color_index} did not round trip",
                        path.display()
                    );
                }
                tested_emitters += 1;
            }
        }
        assert!(tested_emitters > 0, "no emitters were tested");
    }

    #[test]
    fn parses_and_losslessly_reapplies_all_effect_samples() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repository root")
            .join("tmp/Effect");
        let mut paths: Vec<_> = fs::read_dir(&directory)
            .expect("Effect samples directory")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && path.to_string_lossy().ends_with(".esetb.byml"))
            .collect();
        paths.sort();
        assert!(!paths.is_empty(), "no Effect samples found");

        let mut edit_was_tested = false;
        for path in paths {
            let byml_data =
                fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let byml = Byml::from_binary(&byml_data)
                .unwrap_or_else(|error| panic!("{}: invalid BYML: {error}", path.display()));
            let ptcl_data = byml
                .as_map()
                .ok()
                .and_then(|map| map.get("PtclBin"))
                .and_then(|value| match value {
                    Byml::FileData(data) => Some(data),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{}: missing PtclBin", path.display()));
            let ptcl = Ptcl::parse(ptcl_data)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let yaml = ptcl
                .to_yaml()
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let reparsed: PtclDocument = serde_yaml::from_str(&yaml)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(reparsed, ptcl.document, "{}: YAML mismatch", path.display());
            let rebuilt = ptcl
                .apply_yaml(&yaml)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(
                &rebuilt,
                ptcl_data,
                "{}: PTCL hash mismatch",
                path.display()
            );

            if !edit_was_tested {
                let mut edited = ptcl.document.clone();
                if let Some(emitter) = edited
                    .0
                    .values_mut()
                    .find_map(|emitters| emitters.values_mut().next())
                {
                    emitter.const_color0[0] += 0.125;
                    for frames in [
                        &mut emitter.color_anim0,
                        &mut emitter.alpha_anim0,
                        &mut emitter.color_anim1,
                        &mut emitter.alpha_anim1,
                    ] {
                        frames[0].value[0] += 0.125;
                        if frames.len() < 8 {
                            let mut added = frames.last().unwrap().clone();
                            added.keyframe += 1.0;
                            frames.push(added);
                        } else {
                            frames.pop();
                        }
                    }
                    emitter.scale_anim[0].value[0] += 0.125;
                    if emitter.scale_anim.len() < 8 {
                        let mut added = emitter.scale_anim.last().unwrap().clone();
                        added.keyframe += 1.0;
                        emitter.scale_anim.push(added);
                    } else {
                        emitter.scale_anim.pop();
                    }
                    let edited_binary = ptcl.apply_document(&edited).unwrap_or_else(|error| {
                        panic!("{}: failed to apply edit: {error}", path.display())
                    });
                    assert_ne!(
                        &edited_binary,
                        ptcl_data,
                        "{}: edit changed no bytes",
                        path.display()
                    );
                    let reparsed_edit = Ptcl::parse(&edited_binary).unwrap_or_else(|error| {
                        panic!("{}: failed to reparse edit: {error}", path.display())
                    });
                    assert_eq!(
                        reparsed_edit.document,
                        edited,
                        "{}: edited PTCL mismatch",
                        path.display()
                    );
                    edit_was_tested = true;
                }
            }
        }
        assert!(edit_was_tested, "no emitter was available for edit testing");
    }

    #[test]
    fn entire_effect_esetb_files_rebuild_with_matching_sha256() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repository root")
            .join("tmp/Effect");
        let mut paths: Vec<_> = fs::read_dir(&directory)
            .expect("Effect samples directory")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && path.to_string_lossy().ends_with(".esetb.byml"))
            .collect();
        paths.sort();
        assert!(!paths.is_empty(), "no Effect samples found");

        for path in paths {
            let original =
                fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let endian = if crate::Settings::Magic::is_byml_little_endian(&original) {
                roead::Endian::Big
            } else {
                roead::Endian::Little
            };
            let mut editable = Byml::from_binary(&original)
                .unwrap_or_else(|error| panic!("{}: invalid BYML: {error}", path.display()));
            let map = editable
                .as_mut_map()
                .unwrap_or_else(|error| panic!("{}: root is not a map: {error}", path.display()));
            let ptcl_data = match map.remove("PtclBin") {
                Some(Byml::FileData(data)) => data,
                _ => panic!("{}: missing PtclBin", path.display()),
            };
            let ptcl = Ptcl::parse(&ptcl_data)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let yaml = ptcl
                .to_yaml()
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let yaml_node = Byml::from_text(&yaml)
                .unwrap_or_else(|error| panic!("{}: invalid PTCL YAML: {error}", path.display()));
            map.insert("PTCL_JSON".into(), yaml_node);

            // Exercise the same editable YAML boundary used by the GUI.
            let editable_text = editable.to_text();
            let mut rebuilt_byml = Byml::from_text(&editable_text).unwrap_or_else(|error| {
                panic!("{}: editable YAML failed: {error}", path.display())
            });
            let rebuilt_map = rebuilt_byml.as_mut_map().unwrap_or_else(|error| {
                panic!("{}: rebuilt root is not a map: {error}", path.display())
            });
            let ptcl_yaml = rebuilt_map
                .remove("PTCL_JSON")
                .unwrap_or_else(|| panic!("{}: missing PTCL_JSON", path.display()));
            let rebuilt_ptcl = ptcl
                .apply_yaml(&ptcl_yaml.to_text())
                .unwrap_or_else(|error| panic!("{}: PTCL rebuild failed: {error}", path.display()));
            rebuilt_map.insert("PtclBin".into(), Byml::FileData(rebuilt_ptcl));
            let rebuilt = crate::file_format::Esetb::serialize_preserving_original(
                &rebuilt_byml,
                &original,
                endian,
            );

            assert_eq!(
                Sha256::digest(&rebuilt),
                Sha256::digest(&original),
                "{}: complete ESETB SHA-256 mismatch",
                path.display()
            );
        }
    }
}
