use crate::parser::{hkcl::HkclLeaf, physics::SupportBoneDocument};
use serde::Serialize;
use std::{io, path::Path};

/// A BotW support-bone sidecar opened as a read-only tree of YAML leaves.
pub struct BphyssbFile {
    pub source_path: Option<String>,
    pub document: SupportBoneDocument,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Metadata<'a> {
    format: &'static str,
    header: &'a crate::parser::physics::bphhb::SidecarHeader,
    summary: crate::parser::physics::SupportBoneSummary,
    bone_names: Vec<String>,
}

impl BphyssbFile {
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
        opened.file_type = crate::Zstd::TotkFileType::Bphyssb;
        opened.path = crate::Settings::Pathlib::new(path);
        opened.bphyssb = Some(file);
        let mut internal = crate::InternalFile::InternalFile::new(path.into());
        internal.file_type = crate::Zstd::TotkFileType::Bphyssb;
        Some((opened, internal, send_data))
    }

    pub fn from_binary(data: &[u8], path: Option<&Path>) -> io::Result<Self> {
        let document = SupportBoneDocument::parse(data)?;
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
        if !crate::Settings::Pathlib::is_bphyssb_path(path) {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        let file = Self::from_binary(&bytes, Some(path)).ok()?;
        let data = file
            .send_data(path, "Opened read-only BPHYSSB structure".into())
            .ok()?;
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.file_type = crate::Zstd::TotkFileType::Bphyssb;
        opened.path = crate::Settings::Pathlib::new(path);
        opened.bphyssb = Some(file);
        Some((opened, data))
    }

    pub fn send_data(
        &self,
        path: &Path,
        status_text: String,
    ) -> io::Result<crate::Open_and_Save::SendData> {
        let root_name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "BPHYSSB path has no name"))?
            .to_string_lossy();
        let mut data = crate::Open_and_Save::SendData::default();
        data.path = crate::Settings::Pathlib::new(path);
        data.tab = "SARC".into();
        data.read_only = true;
        data.sarc_paths.paths = self
            .leaves()?
            .into_iter()
            .map(|leaf| format!("{root_name}/{}", leaf.path))
            .collect();
        data.sarc_paths.read_only = true;
        data.get_file_label(crate::Zstd::TotkFileType::Bphyssb, None);
        data.status_text = status_text;
        Ok(data)
    }

    pub fn leaves(&self) -> io::Result<Vec<HkclLeaf>> {
        let summary = self.document.validate()?;
        let mut leaves = vec![yaml_leaf(
            "Metadata.bin",
            "Metadata",
            &Metadata {
                format: "BPHYSSB",
                header: &self.document.header,
                summary,
                bone_names: self.document.bone_names(),
            },
        )?];
        if let Some(graph) = &self.document.graph {
            leaves.push(yaml_leaf("Graph.bin", "SupportBoneGraph", graph)?);
        }
        for group in self.document.driver_groups()? {
            let name = group.display_name().replace(['/', '\\', ':'], "_");
            leaves.push(yaml_leaf(
                &format!("DriverGroups/{:03} {name}.bin", group.index),
                "DriverGroup",
                &group,
            )?);
        }
        Ok(leaves)
    }

    pub fn leaf(&self, path: &str) -> io::Result<HkclLeaf> {
        let path = path.split_once('/').map(|(_, leaf)| leaf).unwrap_or(path);
        self.leaves()?
            .into_iter()
            .find(|leaf| leaf.path == path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "BPHYSSB leaf not found"))
    }
}

fn yaml_leaf<T: Serialize>(path: &str, viewer_type: &str, value: &T) -> io::Result<HkclLeaf> {
    Ok(HkclLeaf {
        path: path.to_owned(),
        yaml: serde_yaml::to_string(value).map_err(io::Error::other)?,
        viewer_type: viewer_type.to_owned(),
        read_only: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn opens_support_bone_corpus_as_read_only_trees() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/bphyssb");
        let Ok(entries) = fs::read_dir(&directory) else {
            return;
        };
        for entry in entries.flatten().take(10) {
            let path = entry.path();
            if !crate::Settings::Pathlib::is_bphyssb_path(&path) {
                continue;
            }
            let (opened, data) = BphyssbFile::open(&path)
                .unwrap_or_else(|| panic!("failed to open {}", path.display()));
            assert_eq!(opened.file_type, crate::Zstd::TotkFileType::Bphyssb);
            assert!(data
                .sarc_paths
                .paths
                .iter()
                .any(|leaf| leaf.contains("DriverGroups/")));
            let leaf = opened
                .bphyssb
                .as_ref()
                .unwrap()
                .leaf(&data.sarc_paths.paths[0])
                .unwrap();
            assert!(leaf.read_only && leaf.yaml.contains("BPHYSSB"));
        }
    }

    #[test]
    fn disk_opener_requires_bphyssb_extension() {
        assert!(crate::Settings::Pathlib::is_bphyssb_path("bones.bphyssb"));
        assert!(!crate::Settings::Pathlib::is_bphyssb_path("bones.bphhb"));
    }
}
