use super::BphclDocument;
use std::{
    collections::HashSet,
    io::{self, ErrorKind},
};

const CLOTH_LIST: u32 = 1_571_872_146;
const COLLIDABLE_LIST: u32 = 107_719_806;
const NAME_PARAMETER: u32 = 4_262_580_536;
const STRING_REFERENCE: u8 = 20;
#[derive(Clone, Debug)]
pub struct AampSection {
    pub offset: usize,
    pub size: usize,
    pub raw: Vec<u8>,
}

#[derive(Clone)]
struct Parameter {
    name_hash: u32,
    kind: u8,
    value: Vec<u8>,
}
#[derive(Clone)]
struct Object {
    name_hash: u32,
    name: String,
    parameters: Vec<Parameter>,
}
struct List {
    offset: usize,
    flags_offset: usize,
    objects: Vec<Object>,
}
struct Archive {
    bytes: Vec<u8>,
    root_offset: usize,
}

/// Bounded writer for BPHCL cloth and collidable registration lists.
pub struct AampRegistrationMerger;

impl AampRegistrationMerger {
    pub fn original(document: &BphclDocument) -> io::Result<Vec<u8>> {
        Ok(Archive::from_document(document)?.bytes)
    }

    pub fn append_cloth(
        target: &BphclDocument,
        source: &BphclDocument,
        cloth_name: &str,
    ) -> io::Result<Vec<u8>> {
        let target = Archive::from_document(target)?;
        let source = Archive::from_document(source)?;
        append_entries(
            target.bytes,
            &source,
            CLOTH_LIST,
            "cloth_mesh_",
            [cloth_name],
            true,
        )
    }

    pub fn append_collidables<'a>(
        target_aamp: Vec<u8>,
        source: &BphclDocument,
        names: impl IntoIterator<Item = &'a str>,
    ) -> io::Result<Vec<u8>> {
        let source = Archive::from_document(source)?;
        append_entries(
            target_aamp,
            &source,
            COLLIDABLE_LIST,
            "collidable_",
            names,
            false,
        )
    }

    pub fn remove_cloth(document: &BphclDocument, name: &str) -> io::Result<Vec<u8>> {
        remove_entry(document, CLOTH_LIST, name)
    }

    pub fn remove_collidable(document: &BphclDocument, name: &str) -> io::Result<Vec<u8>> {
        remove_entry(document, COLLIDABLE_LIST, name)
    }

    /// Keeps only the collidable registrations whose names are listed.
    pub fn keep_collidables<'a>(
        document: &BphclDocument,
        names: impl IntoIterator<Item = &'a str>,
    ) -> io::Result<Vec<u8>> {
        let names: HashSet<String> = names.into_iter().map(str::to_owned).collect();
        retain_entries(document, COLLIDABLE_LIST, |object| {
            names.contains(&object.name)
        })
    }

    /// Names registered in the cloth_mesh_list.
    pub fn cloth_entry_names(document: &BphclDocument) -> io::Result<Vec<String>> {
        let archive = Archive::from_document(document)?;
        Ok(archive
            .find_list(CLOTH_LIST)?
            .objects
            .iter()
            .map(|object| object.name.clone())
            .collect())
    }

    pub fn rename_template_entries(
        document: &BphclDocument,
        cloth_name: (&str, &str),
        collidable_names: impl IntoIterator<Item = (String, String)>,
    ) -> io::Result<Vec<u8>> {
        let mut bytes = Archive::from_document(document)?.bytes;
        bytes = rename_entry(bytes, CLOTH_LIST, cloth_name.0, cloth_name.1, true)?;
        for (old_name, new_name) in collidable_names {
            bytes = rename_entry(bytes, COLLIDABLE_LIST, &old_name, &new_name, false)?;
        }
        Ok(bytes)
    }
}

fn remove_entry(document: &BphclDocument, list_hash: u32, name: &str) -> io::Result<Vec<u8>> {
    retain_entries(document, list_hash, |object| object.name != name)
}

fn retain_entries(
    document: &BphclDocument,
    list_hash: u32,
    keep: impl Fn(&Object) -> bool,
) -> io::Result<Vec<u8>> {
    let archive = Archive::from_document(document)?;
    let list = archive.find_list(list_hash)?;
    let removed: Vec<_> = list.objects.iter().filter(|object| !keep(object)).collect();
    if removed.is_empty() {
        return Ok(archive.bytes);
    }
    let object_delta = -(removed.len() as i64);
    let parameter_delta = -removed
        .iter()
        .map(|object| object.parameters.len() as i64)
        .sum::<i64>();
    let string_delta = -removed
        .iter()
        .flat_map(|object| object.parameters.iter())
        .filter(|parameter| parameter.kind == STRING_REFERENCE)
        .map(|parameter| parameter.value.len() as i64)
        .sum::<i64>();
    let objects: Vec<_> = list
        .objects
        .iter()
        .filter(|object| keep(object))
        .cloned()
        .collect();
    rebuild_list(
        &archive,
        &list,
        &objects,
        object_delta,
        parameter_delta,
        string_delta,
    )
}

fn rename_entry(
    bytes: Vec<u8>,
    list_hash: u32,
    old_name: &str,
    new_name: &str,
    required: bool,
) -> io::Result<Vec<u8>> {
    if old_name == new_name {
        return Ok(bytes);
    }
    if new_name.is_empty() || new_name.contains('\0') {
        return Err(invalid("AAMP registration name is empty or contains NUL"));
    }
    let archive = Archive::read(&bytes, 0, bytes.len())?;
    let list = archive.find_list(list_hash)?;
    let Some(index) = list
        .objects
        .iter()
        .position(|object| object.name == old_name)
    else {
        return if required {
            Err(invalid(&format!(
                "BPHCL AAMP has no registration named '{old_name}'"
            )))
        } else {
            Ok(bytes)
        };
    };
    if list
        .objects
        .iter()
        .enumerate()
        .any(|(other, object)| other != index && object.name == new_name)
    {
        return Err(invalid(&format!(
            "BPHCL AAMP already has a registration named '{new_name}'"
        )));
    }
    let mut objects = list.objects.clone();
    let object = &mut objects[index];
    let name_parameter = object
        .parameters
        .iter_mut()
        .find(|parameter| parameter.name_hash == NAME_PARAMETER)
        .ok_or_else(|| invalid("BPHCL AAMP registration has no name parameter"))?;
    let old_size = name_parameter.value.len();
    name_parameter.value = new_name.as_bytes().to_vec();
    name_parameter.value.push(0);
    object.name = new_name.to_owned();
    let string_delta = i64::try_from(name_parameter.value.len())
        .and_then(|new_size| i64::try_from(old_size).map(|old_size| new_size - old_size))
        .map_err(|_| invalid("AAMP string size exceeds i64"))?;
    rebuild_list(&archive, &list, &objects, 0, 0, string_delta)
}

fn append_entries<'a>(
    mut target: Vec<u8>,
    source: &Archive,
    list_hash: u32,
    prefix: &str,
    names: impl IntoIterator<Item = &'a str>,
    required: bool,
) -> io::Result<Vec<u8>> {
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(name.to_string()) {
            continue;
        }
        let target_archive = Archive::read(&target, 0, target.len())?;
        let target_list = target_archive.find_list(list_hash)?;
        let source_list = source.find_list(list_hash)?;
        let donor = source_list
            .objects
            .iter()
            .find(|object| object.name == name)
            .cloned();
        let Some(mut donor) = donor else {
            if required {
                return Err(invalid(&format!(
                    "donor AAMP has no registration named '{name}'"
                )));
            }
            continue;
        };
        if target_list.objects.iter().any(|object| object.name == name) {
            continue;
        }
        donor.name_hash = allocate_hash(&target_list.objects, prefix)?;
        let parameter_delta = donor.parameters.len() as i64;
        let string_delta = donor
            .parameters
            .iter()
            .filter(|p| p.kind == STRING_REFERENCE)
            .map(|p| p.value.len() as i64)
            .sum();
        let mut objects = target_list.objects.clone();
        objects.push(donor);
        target = rebuild_list(
            &target_archive,
            &target_list,
            &objects,
            1,
            parameter_delta,
            string_delta,
        )?;
    }
    Ok(target)
}

impl Archive {
    fn from_document(document: &BphclDocument) -> io::Result<Self> {
        Self::read(
            &document.raw,
            document.header.parameter_offset as usize,
            document.header.parameter_size as usize,
        )
    }

    fn read(source: &[u8], offset: usize, size: usize) -> io::Result<Self> {
        let wrapper = source
            .get(
                offset
                    ..offset
                        .checked_add(size)
                        .ok_or_else(|| invalid("AAMP range overflow"))?,
            )
            .ok_or_else(|| invalid("AAMP range exceeds BPHCL"))?;
        if wrapper.len() < 0x30 || !crate::Settings::Magic::is_aamp(wrapper) {
            return Err(invalid("BPHCL parameter archive is not AAMP"));
        }
        let declared = read_u32(wrapper, 0x0c)? as usize;
        if declared < 0x30 || declared > wrapper.len() {
            return Err(invalid("AAMP declared size exceeds wrapper"));
        }
        let bytes = wrapper[..declared].to_vec();
        let root_offset = 0x30usize
            .checked_add(read_u32(&bytes, 0x14)? as usize)
            .ok_or_else(|| invalid("AAMP root overflow"))?;
        ensure(&bytes, root_offset, 12)?;
        Ok(Self { bytes, root_offset })
    }

    fn find_list(&self, hash: u32) -> io::Result<List> {
        self.find_list_at(self.root_offset, hash, &mut HashSet::new())?
            .ok_or_else(|| invalid(&format!("BPHCL AAMP has no required list {hash}")))
    }

    fn find_list_at(
        &self,
        offset: usize,
        hash: u32,
        visited: &mut HashSet<usize>,
    ) -> io::Result<Option<List>> {
        if !visited.insert(offset) {
            return Ok(None);
        }
        ensure(&self.bytes, offset, 12)?;
        let child_flags = read_u32(&self.bytes, offset + 4)?;
        let child_offset = offset
            .checked_add((child_flags as usize & 0xffff) * 4)
            .ok_or_else(|| invalid("AAMP child offset overflow"))?;
        let child_count = (child_flags >> 16) as usize;
        ensure(
            &self.bytes,
            child_offset,
            child_count
                .checked_mul(12)
                .ok_or_else(|| invalid("AAMP child size overflow"))?,
        )?;
        for index in 0..child_count {
            if let Some(found) = self.find_list_at(child_offset + index * 12, hash, visited)? {
                return Ok(Some(found));
            }
        }
        if read_u32(&self.bytes, offset)? != hash {
            return Ok(None);
        }
        let flags_offset = offset + 8;
        let flags = read_u32(&self.bytes, flags_offset)?;
        let object_offset = offset
            .checked_add((flags as usize & 0xffff) * 4)
            .ok_or_else(|| invalid("AAMP object offset overflow"))?;
        let count = (flags >> 16) as usize;
        ensure(
            &self.bytes,
            object_offset,
            count
                .checked_mul(8)
                .ok_or_else(|| invalid("AAMP object size overflow"))?,
        )?;
        let objects = (0..count)
            .map(|index| self.read_object(object_offset + index * 8))
            .collect::<io::Result<_>>()?;
        Ok(Some(List {
            offset,
            flags_offset,
            objects,
        }))
    }

    fn read_object(&self, offset: usize) -> io::Result<Object> {
        let name_hash = read_u32(&self.bytes, offset)?;
        let flags = read_u32(&self.bytes, offset + 4)?;
        let parameter_offset = offset
            .checked_add((flags as usize & 0xffff) * 4)
            .ok_or_else(|| invalid("AAMP parameter offset overflow"))?;
        let count = (flags >> 16) as usize;
        ensure(
            &self.bytes,
            parameter_offset,
            count
                .checked_mul(8)
                .ok_or_else(|| invalid("AAMP parameter size overflow"))?,
        )?;
        let mut parameters = Vec::with_capacity(count);
        for index in 0..count {
            let entry = parameter_offset + index * 8;
            let parameter_flags = read_u32(&self.bytes, entry + 4)?;
            let kind = (parameter_flags >> 24) as u8;
            let value_offset = entry
                .checked_add((parameter_flags as usize & 0x00ff_ffff) * 4)
                .ok_or_else(|| invalid("AAMP value offset overflow"))?;
            let size = value_size(&self.bytes, kind, value_offset)?;
            parameters.push(Parameter {
                name_hash: read_u32(&self.bytes, entry)?,
                kind,
                value: self.bytes[value_offset..value_offset + size].to_vec(),
            });
        }
        let name = parameters
            .iter()
            .find(|p| p.name_hash == NAME_PARAMETER)
            .map(|p| {
                String::from_utf8_lossy(&p.value)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        Ok(Object {
            name_hash,
            name,
            parameters,
        })
    }
}

fn rebuild_list(
    archive: &Archive,
    list: &List,
    objects: &[Object],
    object_delta: i64,
    parameter_delta: i64,
    string_delta: i64,
) -> io::Result<Vec<u8>> {
    let mut output = archive.bytes.clone();
    align(&mut output, 4);
    let object_offset = output.len();
    output.resize(
        object_offset
            .checked_add(
                objects
                    .len()
                    .checked_mul(8)
                    .ok_or_else(|| invalid("AAMP objects overflow"))?,
            )
            .ok_or_else(|| invalid("AAMP objects overflow"))?,
        0,
    );
    for (object_index, object) in objects.iter().enumerate() {
        align(&mut output, 4);
        let parameter_offset = output.len();
        output.resize(
            parameter_offset
                .checked_add(
                    object
                        .parameters
                        .len()
                        .checked_mul(8)
                        .ok_or_else(|| invalid("AAMP parameters overflow"))?,
                )
                .ok_or_else(|| invalid("AAMP parameters overflow"))?,
            0,
        );
        for (parameter_index, parameter) in object.parameters.iter().enumerate() {
            align(&mut output, 4);
            let value_offset = output.len();
            output.extend_from_slice(&parameter.value);
            let entry = parameter_offset + parameter_index * 8;
            write_u32(&mut output, entry, parameter.name_hash)?;
            let relative = (value_offset - entry) / 4;
            if relative > 0x00ff_ffff {
                return Err(invalid("AAMP value offset exceeds 24 bits"));
            }
            write_u32(
                &mut output,
                entry + 4,
                ((parameter.kind as u32) << 24) | relative as u32,
            )?;
        }
        let entry = object_offset + object_index * 8;
        let relative = (parameter_offset - entry) / 4;
        if relative > u16::MAX as usize || object.parameters.len() > u16::MAX as usize {
            return Err(invalid("AAMP object layout exceeds 16 bits"));
        }
        write_u32(&mut output, entry, object.name_hash)?;
        write_u32(
            &mut output,
            entry + 4,
            ((object.parameters.len() as u32) << 16) | relative as u32,
        )?;
    }
    let relative = (object_offset - list.offset) / 4;
    if relative > u16::MAX as usize || objects.len() > u16::MAX as usize {
        return Err(invalid("AAMP list layout exceeds 16 bits"));
    }
    write_u32(
        &mut output,
        list.flags_offset,
        ((objects.len() as u32) << 16) | relative as u32,
    )?;
    let logical_size = output.len() as u32;
    write_u32(&mut output, 0x0c, logical_size)?;
    apply_delta(&mut output, 0x1c, object_delta)?;
    apply_delta(&mut output, 0x20, parameter_delta)?;
    apply_delta(&mut output, 0x28, string_delta)?;
    align(&mut output, 8);
    Ok(output)
}

fn value_size(bytes: &[u8], kind: u8, offset: usize) -> io::Result<usize> {
    let size = match kind {
        0 | 1 | 2 | 17 => 4,
        3 => 8,
        4 => 12,
        5 | 6 | 16 => 16,
        7 => 32,
        8 => 64,
        15 => 256,
        STRING_REFERENCE => bytes
            .get(offset..)
            .and_then(|tail| tail.iter().position(|b| *b == 0))
            .map(|n| n + 1)
            .ok_or_else(|| invalid("AAMP StringRef is not terminated"))?,
        _ => return Err(invalid(&format!("unsupported AAMP parameter type {kind}"))),
    };
    ensure(bytes, offset, size)?;
    Ok(size)
}

fn allocate_hash(objects: &[Object], prefix: &str) -> io::Result<u32> {
    let used: HashSet<_> = objects.iter().map(|object| object.name_hash).collect();
    (0..65_536)
        .map(|index| crc32(&format!("{prefix}{index}")))
        .find(|hash| !used.contains(hash))
        .ok_or_else(|| invalid("AAMP has no available object key"))
}
fn crc32(value: &str) -> u32 {
    let mut crc = u32::MAX;
    for byte in value.bytes() {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 0 { 0 } else { 0xedb8_8320 };
        }
    }
    !crc
}
fn align(bytes: &mut Vec<u8>, alignment: usize) {
    while bytes.len() % alignment != 0 {
        bytes.push(0);
    }
}
fn ensure(bytes: &[u8], offset: usize, size: usize) -> io::Result<()> {
    if offset.checked_add(size).is_none_or(|end| end > bytes.len()) {
        Err(invalid("AAMP pointer exceeds archive"))
    } else {
        Ok(())
    }
}
fn read_u32(bytes: &[u8], offset: usize) -> io::Result<u32> {
    crate::parser::binary::BinaryReader::new(bytes).read_u32_at(offset)
}
fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> io::Result<()> {
    bytes
        .get_mut(offset..offset + 4)
        .ok_or_else(|| invalid("AAMP write exceeds archive"))?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}
fn apply_delta(bytes: &mut [u8], offset: usize, delta: i64) -> io::Result<()> {
    let value = i64::from(read_u32(bytes, offset)?)
        .checked_add(delta)
        .ok_or_else(|| invalid("AAMP counter overflow"))?;
    write_u32(
        bytes,
        offset,
        u32::try_from(value).map_err(|_| invalid("AAMP counter exceeds u32"))?,
    )
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive_with_name(list_hash: u32, object_hash: u32, name: &str) -> Vec<u8> {
        let mut bytes = vec![0; 76];
        bytes[..4].copy_from_slice(b"AAMP");
        bytes[0x14..0x18].copy_from_slice(&0u32.to_le_bytes());
        bytes[0x1c..0x20].copy_from_slice(&1u32.to_le_bytes());
        bytes[0x20..0x24].copy_from_slice(&1u32.to_le_bytes());
        bytes[0x2c] = 0x5a;
        bytes[48..52].copy_from_slice(&list_hash.to_le_bytes());
        bytes[56..60].copy_from_slice(&0x0001_0003u32.to_le_bytes());
        bytes[60..64].copy_from_slice(&object_hash.to_le_bytes());
        bytes[64..68].copy_from_slice(&0x0001_0002u32.to_le_bytes());
        bytes[68..72].copy_from_slice(&NAME_PARAMETER.to_le_bytes());
        bytes[72..76].copy_from_slice(&0x1400_0002u32.to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0);
        let declared = bytes.len() as u32;
        bytes[0x0c..0x10].copy_from_slice(&declared.to_le_bytes());
        bytes[0x28..0x2c].copy_from_slice(&(name.len() as u32 + 1).to_le_bytes());
        bytes
    }

    #[test]
    fn appending_registration_relocates_only_selected_list() {
        let target = archive_with_name(CLOTH_LIST, crc32("cloth_mesh_0"), "Target");
        let source_bytes = archive_with_name(CLOTH_LIST, crc32("cloth_mesh_0"), "Donor");
        let source = Archive::read(&source_bytes, 0, source_bytes.len()).unwrap();
        let merged =
            append_entries(target, &source, CLOTH_LIST, "cloth_mesh_", ["Donor"], true).unwrap();
        let archive = Archive::read(&merged, 0, merged.len()).unwrap();
        let list = archive.find_list(CLOTH_LIST).unwrap();

        assert_eq!(
            list.objects
                .iter()
                .map(|object| object.name.as_str())
                .collect::<Vec<_>>(),
            ["Target", "Donor"]
        );
        assert_ne!(list.objects[0].name_hash, list.objects[1].name_hash);
        assert_eq!(read_u32(&merged, 0x1c).unwrap(), 2);
        assert_eq!(read_u32(&merged, 0x20).unwrap(), 2);
        assert_eq!(merged[0x2c], 0x5a);
    }

    #[test]
    fn duplicate_registration_is_not_added_twice() {
        let target = archive_with_name(CLOTH_LIST, crc32("cloth_mesh_0"), "Same");
        let source_bytes = target.clone();
        let source = Archive::read(&source_bytes, 0, source_bytes.len()).unwrap();
        let merged = append_entries(
            target.clone(),
            &source,
            CLOTH_LIST,
            "cloth_mesh_",
            ["Same"],
            true,
        )
        .unwrap();
        assert_eq!(merged, target);
    }

    #[test]
    fn renaming_registration_rebuilds_its_string_value() {
        let bytes = archive_with_name(CLOTH_LIST, crc32("cloth_mesh_0"), "Template");
        let renamed = rename_entry(bytes, CLOTH_LIST, "Template", "ImportedCloth", true).unwrap();
        let archive = Archive::read(&renamed, 0, renamed.len()).unwrap();
        let list = archive.find_list(CLOTH_LIST).unwrap();

        assert_eq!(list.objects[0].name, "ImportedCloth");
        assert_eq!(list.objects[0].name_hash, crc32("cloth_mesh_0"));
        assert_eq!(read_u32(&renamed, 0x1c).unwrap(), 1);
        assert_eq!(read_u32(&renamed, 0x20).unwrap(), 1);
    }
}
impl AampSection {
    pub fn read(data: &[u8], offset: u32, size: u32) -> io::Result<Option<Self>> {
        if size == 0 {
            return Ok(None);
        }
        let o = offset as usize;
        let s = size as usize;
        let raw = data
            .get(
                o..o.checked_add(s)
                    .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "AAMP range overflow"))?,
            )
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "AAMP outside BPHCL"))?;
        if !crate::Settings::Magic::is_aamp(raw) {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "parameter section is not AAMP",
            ));
        }
        Ok(Some(Self {
            offset: o,
            size: s,
            raw: raw.to_vec(),
        }))
    }
}
