use std::{collections::HashMap, io, sync::Arc};

use roead::sarc::{Sarc, SarcWriter};

use crate::{
    file_format::Archive::{
        detect_archive_magic, ArchiveCodec, ArchiveMagic, Rar::RarFile, RootArchive,
        SevenZipCmd::SevenZipCmd, Zip::ZipFile,
    },
    Settings::Magic,
    Zstd::TotkZstd,
};

#[derive(Clone, Copy, Debug)]
enum NestedEncoding {
    Raw,
    Yaz0(u32),
    ZstdPack,
    Zstd,
}

pub struct NestedArchive {
    kind: NestedKind,
}

enum NestedKind {
    Sarc {
        writer: SarcWriter,
        encoding: NestedEncoding,
    },
    Generic(RootArchive),
}

impl NestedArchive {
    pub fn parse_named(_name: &str, data: &[u8], zstd: Arc<TotkZstd<'_>>) -> io::Result<Self> {
        let generic = match detect_archive_magic(data) {
            Some(ArchiveMagic::Zip) => Some(ZipFile::from_bytes(data).map(RootArchive::Zip)),
            Some(ArchiveMagic::SevenZip) => {
                Some(SevenZipCmd::from_bytes(data).map(RootArchive::SevenZip))
            }
            Some(ArchiveMagic::Rar) => Some(RarFile::from_bytes(data).map(RootArchive::Rar)),
            Some(ArchiveMagic::Bars) => Some(
                crate::file_format::Archive::Bars::BarsFile::from_bytes(data)
                    .map(RootArchive::Bars),
            ),
            Some(ArchiveMagic::RflDb) => {
                Some(crate::file_format::Mii::RflDbFile::from_bytes(data).map(RootArchive::RflDb))
            }
            None => None,
        };
        if let Some(archive) = generic {
            let archive =
                archive.map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            return Ok(Self {
                kind: NestedKind::Generic(archive),
            });
        }
        let (raw, encoding) = if Magic::is_sarc(data) {
            (data.to_vec(), NestedEncoding::Raw)
        } else if Magic::is_yaz0(data) {
            (
                roead::yaz0::decompress(data)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
                NestedEncoding::Yaz0(crate::Zstd::TotkZstd::yaz0_alignment(data)),
            )
        } else if let Ok(raw) = zstd.decompressor.decompress_pack(data) {
            (raw, NestedEncoding::ZstdPack)
        } else if let Ok(raw) = zstd.decompressor.decompress_zs(data) {
            (raw, NestedEncoding::Zstd)
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "entry is not a raw, Yaz0, or Zstd SARC",
            ));
        };
        if !Magic::is_sarc(&raw) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "decoded entry does not contain SARC data",
            ));
        }
        let sarc =
            Sarc::new(raw).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Ok(Self {
            kind: NestedKind::Sarc {
                writer: SarcWriter::from_sarc(&sarc),
                encoding,
            },
        })
    }

    pub fn parse(data: &[u8], zstd: Arc<TotkZstd<'_>>) -> io::Result<Self> {
        Self::parse_named("nested.sarc", data, zstd)
    }

    pub fn paths(&self) -> Vec<String> {
        let mut paths: Vec<_> = match &self.kind {
            NestedKind::Sarc { writer, .. } => writer.files.keys().cloned().collect(),
            NestedKind::Generic(a) => a.entries().keys().cloned().collect(),
        };
        paths.sort_by_key(|path| path.to_lowercase());
        paths
    }

    pub fn get(&mut self, path: &str) -> Option<&[u8]> {
        match &mut self.kind {
            NestedKind::Sarc { writer, .. } => writer.get_file(path).map(Vec::as_slice),
            NestedKind::Generic(a) => a.get(path),
        }
    }
    pub fn set(&mut self, path: &str, data: Vec<u8>) {
        match &mut self.kind {
            NestedKind::Sarc { writer, .. } => writer.add_file(path, data),
            NestedKind::Generic(a) => {
                a.entries_mut().insert(path.into(), data);
            }
        }
    }

    pub fn remove_prefix(&mut self, path: &str) -> io::Result<usize> {
        let prefix = format!("{}/", path.trim_end_matches('/'));
        let keys: Vec<_> = self
            .paths()
            .into_iter()
            .filter(|p| p == path || p.starts_with(&prefix))
            .collect();
        if keys.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, path));
        }
        for key in &keys {
            match &mut self.kind {
                NestedKind::Sarc { writer, .. } => {
                    writer.remove_file(key);
                }
                NestedKind::Generic(a) => {
                    a.entries_mut().remove(key);
                }
            }
        }
        Ok(keys.len())
    }

    pub fn rename_prefix(&mut self, from: &str, to: &str) -> io::Result<usize> {
        let prefix = format!("{}/", from.trim_end_matches('/'));
        let keys: Vec<_> = self
            .paths()
            .into_iter()
            .filter(|p| p == from || p.starts_with(&prefix))
            .collect();
        if keys.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, from));
        }
        let mut replacements = Vec::new();
        for old in &keys {
            let new = if old == from {
                to.into()
            } else {
                format!("{}{}", to.trim_end_matches('/'), &old[from.len()..])
            };
            let bytes = self.get(old).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("missing archive entry {old}"),
                )
            })?;
            replacements.push((new, bytes.to_vec()));
        }
        self.remove_prefix(from)?;
        for (new, bytes) in replacements {
            self.set(&new, bytes);
        }
        Ok(keys.len())
    }

    pub fn to_encoded(&mut self, zstd: Arc<TotkZstd<'_>>) -> io::Result<Vec<u8>> {
        let (writer, encoding) = match &mut self.kind {
            NestedKind::Generic(a) => {
                return a
                    .to_bytes()
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            }
            NestedKind::Sarc { writer, encoding } => (writer, *encoding),
        };
        let raw = writer.to_binary();
        match encoding {
            NestedEncoding::Raw => Ok(raw),
            NestedEncoding::Yaz0(alignment) => {
                crate::Zstd::TotkZstd::compress_yaz0_with_alignment(&raw, alignment)
            }
            NestedEncoding::ZstdPack => zstd.compress_pack(&raw),
            NestedEncoding::Zstd => zstd.compress_zs(&raw),
        }
    }
}

pub type NestedArchives = HashMap<String, NestedArchive>;

#[cfg(test)]
mod tests {
    use roead::{
        sarc::{Sarc, SarcWriter},
        Endian,
    };

    #[test]
    fn raw_writer_roundtrip_preserves_nested_edit() {
        let mut writer = SarcWriter::new(Endian::Little);
        writer.add_file("a/test.txt", b"before".to_vec());
        let sarc = Sarc::new(writer.to_binary()).unwrap();
        let mut nested = SarcWriter::from_sarc(&sarc);
        nested.add_file("a/test.txt", b"after".to_vec());
        let result = Sarc::new(nested.to_binary()).unwrap();
        assert_eq!(result.get_data("a/test.txt"), Some(&b"after"[..]));
    }
}
