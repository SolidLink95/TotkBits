use crate::parser::physics::hkcl::{HkclDocument, HkclLeaf};
use std::io::ErrorKind;
use std::{io, path::Path};

pub struct HkclFile {
    pub source_path: Option<String>,
    pub document: HkclDocument,
}

impl HkclFile {
    pub fn open_internal(
        data: &[u8],
        path: &str,
        outer_path: Option<&str>,
    ) -> Option<(
        crate::file_format::BinTextFile::OpenedFile<'static>,
        crate::InternalFile::InternalFile<'static>,
        crate::Open_and_Save::SendData,
    )> {
        let file = Self::from_binary(data, Some(Path::new(path))).ok()?;
        let status = match outer_path {
            Some(outer) => format!("Opened {path} inside {outer}"),
            None => format!("Opened {path} from archive"),
        };
        let send_data = file.send_data(Path::new(path), status).ok()?;
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.file_type = crate::Zstd::TotkFileType::Hkcl;
        opened.path = crate::Settings::Pathlib::new(path);
        opened.hkcl = Some(file);
        let mut internal = crate::InternalFile::InternalFile::new(path.into());
        internal.file_type = crate::Zstd::TotkFileType::Hkcl;
        Some((opened, internal, send_data))
    }

    pub fn from_binary(data: &[u8], path: Option<&Path>) -> io::Result<Self> {
        if !crate::Settings::Magic::is_hkcl(data) {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "missing HKCL Havok packfile header",
            ));
        }
        let document = HkclDocument::parse(data)?;
        document.validate()?;
        Ok(Self {
            source_path: path.map(|path| path.to_string_lossy().into_owned()),
            document,
        })
    }

    pub fn open(
        path: &Path,
    ) -> Option<(
        crate::file_format::BinTextFile::OpenedFile<'static>,
        crate::Open_and_Save::SendData,
    )> {
        if !crate::Settings::Pathlib::is_hkcl_path(path) {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        let file = Self::from_binary(&bytes, Some(path)).ok()?;
        let data = file
            .send_data(path, "Opened read-only HKCL structure".into())
            .ok()?;
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.file_type = crate::Zstd::TotkFileType::Hkcl;
        opened.path = crate::Settings::Pathlib::new(path);
        opened.hkcl = Some(file);
        Some((opened, data))
    }

    pub fn send_data(
        &self,
        path: &Path,
        status_text: String,
    ) -> io::Result<crate::Open_and_Save::SendData> {
        let root_name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "HKCL path has no name"))?
            .to_string_lossy();
        let mut data = crate::Open_and_Save::SendData::default();
        data.path = crate::Settings::Pathlib::new(path);
        data.tab = "SARC".into();
        data.read_only = true;
        data.sarc_paths.paths = self
            .document
            .leaves()?
            .into_iter()
            .map(|leaf| format!("{root_name}/{}", leaf.path))
            .collect();
        data.sarc_paths.read_only = true;
        data.get_file_label(crate::Zstd::TotkFileType::Hkcl, None);
        data.status_text = status_text;
        Ok(data)
    }

    pub fn leaf(&self, path: &str) -> io::Result<HkclLeaf> {
        let path = path.split_once('/').map(|(_, leaf)| leaf).unwrap_or(path);
        self.document
            .leaves()?
            .into_iter()
            .find(|leaf| leaf.path == path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HKCL leaf not found"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn opens_corpus_files_as_read_only_trees() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/hkcl");
        let mut count = 0;
        for entry in fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if !crate::Settings::Pathlib::is_hkcl_path(&path) {
                continue;
            }
            count += 1;
            let (opened, data) = HkclFile::open(&path)
                .unwrap_or_else(|| panic!("failed to open {}", path.display()));
            assert_eq!(opened.file_type, crate::Zstd::TotkFileType::Hkcl);
            assert!(opened.hkcl.is_some());
            assert_eq!(data.tab, "SARC");
            assert!(data.read_only && data.sarc_paths.read_only);
            assert!(!data.sarc_paths.paths.is_empty());
            let leaf = opened
                .hkcl
                .as_ref()
                .unwrap()
                .leaf(&data.sarc_paths.paths[0])
                .unwrap();
            assert!(leaf.read_only);
        }
        assert!(count > 0, "HKCL corpus is empty");
    }

    #[test]
    fn disk_opener_requires_hkcl_extension() {
        assert!(crate::Settings::Pathlib::is_hkcl_path("cloth.hkcl"));
        assert!(crate::Settings::Pathlib::is_hkcl_path("cloth.HKCL"));
        assert!(!crate::Settings::Pathlib::is_hkcl_path("cloth.bphcl"));
    }
}
