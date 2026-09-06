use crate::parser::physics::{hkcl::HkclLeaf, HkrgDocument};
use std::io::ErrorKind;
use std::{io, path::Path};

/// A BotW Havok ragdoll opened as a read-only tree of YAML leaves.
pub struct HkrgFile {
    pub source_path: Option<String>,
    pub document: HkrgDocument,
}

impl HkrgFile {
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
        opened.file_type = crate::Zstd::TotkFileType::Hkrg;
        opened.path = crate::Settings::Pathlib::new(path);
        opened.hkrg = Some(file);
        let mut internal = crate::InternalFile::InternalFile::new(path.into());
        internal.file_type = crate::Zstd::TotkFileType::Hkrg;
        Some((opened, internal, send_data))
    }

    pub fn from_binary(data: &[u8], path: Option<&Path>) -> io::Result<Self> {
        if !crate::Settings::Magic::is_hkrg(data) {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "missing HKRG Havok ragdoll packfile",
            ));
        }
        let document = HkrgDocument::parse(data)?;
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
        if !crate::Settings::Pathlib::is_hkrg_path(path) {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        let file = Self::from_binary(&bytes, Some(path)).ok()?;
        let data = file
            .send_data(path, "Opened read-only HKRG ragdoll".into())
            .ok()?;
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.file_type = crate::Zstd::TotkFileType::Hkrg;
        opened.path = crate::Settings::Pathlib::new(path);
        opened.hkrg = Some(file);
        Some((opened, data))
    }

    pub fn send_data(
        &self,
        path: &Path,
        status_text: String,
    ) -> io::Result<crate::Open_and_Save::SendData> {
        let root_name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "HKRG path has no name"))?
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
        data.get_file_label(crate::Zstd::TotkFileType::Hkrg, None);
        data.status_text = status_text;
        Ok(data)
    }

    pub fn leaf(&self, path: &str) -> io::Result<HkclLeaf> {
        let path = path.split_once('/').map(|(_, leaf)| leaf).unwrap_or(path);
        self.document
            .leaves()?
            .into_iter()
            .find(|leaf| leaf.path == path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HKRG leaf not found"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn opens_ragdoll_corpus_as_read_only_trees() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/hkrg");
        let Ok(entries) = fs::read_dir(&directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !crate::Settings::Pathlib::is_hkrg_path(&path) {
                continue;
            }
            let (opened, data) = HkrgFile::open(&path)
                .unwrap_or_else(|| panic!("failed to open {}", path.display()));
            assert_eq!(opened.file_type, crate::Zstd::TotkFileType::Hkrg);
            assert_eq!(data.tab, "SARC");
            assert!(data.read_only);
            assert!(data
                .sarc_paths
                .paths
                .iter()
                .any(|leaf| leaf.contains("RigidBodies/")));
            let leaf = opened
                .hkrg
                .as_ref()
                .unwrap()
                .leaf(&data.sarc_paths.paths[0])
                .unwrap();
            assert!(leaf.read_only);
        }
    }

    #[test]
    fn disk_opener_requires_hkrg_extension() {
        assert!(crate::Settings::Pathlib::is_hkrg_path("ragdoll.hkrg"));
        assert!(crate::Settings::Pathlib::is_hkrg_path("ragdoll.HKRG"));
        assert!(!crate::Settings::Pathlib::is_hkrg_path("cloth.hkcl"));
    }
}
