use crate::parser::physics::{bphhb::BphhbDocument, hkcl::HkclLeaf};
use std::{io, path::Path};

pub struct BphhbFile {
    pub source_path: Option<String>,
    pub document: BphhbDocument,
}

impl BphhbFile {
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
        opened.file_type = crate::Zstd::TotkFileType::Bphhb;
        opened.path = crate::Settings::Pathlib::new(path);
        opened.bphhb = Some(file);
        let mut internal = crate::InternalFile::InternalFile::new(path.into());
        internal.file_type = crate::Zstd::TotkFileType::Bphhb;
        Some((opened, internal, send_data))
    }

    pub fn from_binary(data: &[u8], path: Option<&Path>) -> io::Result<Self> {
        let document = BphhbDocument::parse(data)?;
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
        if !crate::Settings::Pathlib::is_bphhb_path(path) {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        let file = Self::from_binary(&bytes, Some(path)).ok()?;
        let data = file
            .send_data(path, "Opened read-only BPHHB structure".into())
            .ok()?;
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.file_type = crate::Zstd::TotkFileType::Bphhb;
        opened.path = crate::Settings::Pathlib::new(path);
        opened.bphhb = Some(file);
        Some((opened, data))
    }

    pub fn send_data(
        &self,
        path: &Path,
        status_text: String,
    ) -> io::Result<crate::Open_and_Save::SendData> {
        let root_name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "BPHHB path has no name"))?
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
        data.get_file_label(crate::Zstd::TotkFileType::Bphhb, None);
        data.status_text = status_text;
        Ok(data)
    }

    pub fn leaf(&self, path: &str) -> io::Result<HkclLeaf> {
        let path = path.split_once('/').map(|(_, leaf)| leaf).unwrap_or(path);
        self.document
            .leaves()?
            .into_iter()
            .find(|leaf| leaf.path == path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "BPHHB leaf not found"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_opener_requires_bphhb_extension() {
        assert!(crate::Settings::Pathlib::is_bphhb_path("bones.bphhb"));
        assert!(crate::Settings::Pathlib::is_bphhb_path("bones.BPHHB"));
        assert!(!crate::Settings::Pathlib::is_bphhb_path("bones.bphcl"));
    }
}
