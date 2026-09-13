use crate::{
    file_format::BinTextFile::OpenedFile,
    parser::asb::{Asb, Baev},
    Open_and_Save::SendData,
    Settings::Pathlib,
    Zstd::{TotkFileType, TotkZstd},
};
use serde::{Deserialize, Serialize};
use std::{io, path::Path, sync::Arc};

/// Native representation passed from the ASB parser to the application layer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AsbFile {
    #[serde(flatten)]
    pub document: Asb,
    #[serde(rename = "BAEV", skip_serializing_if = "Option::is_none", default)]
    pub baev: Option<Baev>,
}

impl AsbFile {
    pub fn open_asb<'a, P: AsRef<Path>>(
        path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(OpenedFile<'a>, SendData)> {
        let path = path.as_ref();
        let asb_data = read_maybe_compressed(path, &zstd, b"ASB ").ok()?;
        if !crate::Settings::Magic::is_asb(&asb_data) {
            return None;
        }

        let suggested_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| format!("{}.root.baev", name.split('.').next().unwrap_or(name)))
            .unwrap_or_else(|| "*.baev".to_string());
        let display_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ASB");
        let short_name = if display_name.chars().count() > 48 {
            format!("{}…", display_name.chars().take(47).collect::<String>())
        } else {
            display_name.to_string()
        };
        let mut dialog = rfd::FileDialog::new()
            .set_title(format!("Select optional BAEV for {short_name}"))
            .add_filter("Binary Animation Event", &["baev", "zs"])
            .set_file_name(&suggested_name);
        if let Some(parent) = path.parent() {
            dialog = dialog.set_directory(parent);
        }
        let baev_path = dialog.pick_file();
        let baev_original_data = baev_path
            .as_deref()
            .and_then(|path| std::fs::read(path).ok());
        let baev_data = baev_path
            .as_deref()
            .map(|path| read_maybe_compressed(path, &zstd, b"BFFH"))
            .transpose()
            .ok()?;
        let file = Self::from_binary(&asb_data)
            .and_then(|file| file.with_baev(baev_data.as_deref()))
            .ok()?;
        let text = file.to_yaml().ok()?;
        let mut opened_file = OpenedFile::default();
        opened_file.path = Pathlib::new(path);
        opened_file.file_type = TotkFileType::ASB;
        opened_file.asb = Some(file.document.clone());
        opened_file.asb_baev_path = baev_path;
        opened_file.asb_baev_data = baev_original_data;
        let mut data = SendData {
            status_text: format!("Opened: {}", opened_file.path.full_path),
            path: Pathlib::new(path),
            text,
            ..Default::default()
        };
        data.get_file_label(TotkFileType::ASB, Some(roead::Endian::Little));
        Some((opened_file, data))
    }

    pub fn open_asb_binary<'a, P: AsRef<Path>>(
        binary: &[u8],
        display_path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(OpenedFile<'a>, SendData)> {
        let path = display_path.as_ref();
        let (raw, compression) = zstd.try_decompress_all_ordered_safe(binary, path);
        if !crate::Settings::Magic::is_asb(&raw) {
            return None;
        }
        let file = Self::from_binary(&raw).ok()?;
        let text = file.to_yaml().ok()?;
        let mut opened = OpenedFile::default();
        opened.path = Pathlib::new(path);
        opened.file_type = TotkFileType::ASB;
        opened.asb = Some(file.document.clone());
        opened.compression =
            (compression != crate::Zstd::ZstdDictionary::None).then_some(compression);
        let mut data = SendData {
            path: Pathlib::new(path),
            text,
            status_text: format!("Opened: {}", path.display()),
            ..Default::default()
        };
        data.get_file_label(TotkFileType::ASB, Some(roead::Endian::Little));
        Some((opened, data))
    }

    pub fn from_binary(data: &[u8]) -> io::Result<Self> {
        Ok(Self {
            document: Asb::from_bytes(data)?,
            baev: None,
        })
    }

    pub fn with_baev(mut self, data: Option<&[u8]>) -> io::Result<Self> {
        self.baev = data.map(Baev::from_bytes).transpose()?;
        Ok(self)
    }

    pub fn from_paths(
        asb_path: &Path,
        baev_path: Option<&Path>,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<Self> {
        let asb = read_maybe_compressed(asb_path, &zstd, b"ASB ")?;
        let baev = baev_path
            .map(|path| read_maybe_compressed(path, &zstd, b"BFFH"))
            .transpose()?;
        Self::from_binary(&asb)?.with_baev(baev.as_deref())
    }

    pub fn binary_to_text(data: &[u8]) -> io::Result<String> {
        Asb::from_bytes(data)?.to_yaml()
    }

    pub fn text_to_binary(
        text: &str,
        opened_file: Option<&OpenedFile<'_>>,
        cached_asb: Option<&Asb>,
    ) -> io::Result<Vec<u8>> {
        match serde_yaml::from_str::<Self>(text) {
            Ok(file) => {
                if file.baev.is_some() {
                    Self::offer_baev_save(opened_file);
                }
                if let Some(cached) = cached_asb.filter(|cached| {
                    serde_yaml::to_value(cached).ok() == serde_yaml::to_value(&file.document).ok()
                }) {
                    return Ok(cached.to_bytes());
                }
                file.document.to_native_bytes()
            }
            Err(wrapper_error) => Asb::from_yaml(text)
                .and_then(|document| document.to_native_bytes())
                .map_err(|document_error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "invalid ASB YAML wrapper ({wrapper_error}); invalid ASB document ({document_error})"
                        ),
                    )
                }),
        }
    }

    fn offer_baev_save(opened_file: Option<&OpenedFile<'_>>) {
        let Some(opened_file) = opened_file else {
            return;
        };
        let Some(bytes) = opened_file.asb_baev_data.as_ref() else {
            return;
        };
        let mut dialog = rfd::FileDialog::new()
            .set_title("Save accompanying BAEV file")
            .add_filter("Binary Animation Event", &["baev", "zs"]);
        if let Some(source) = opened_file.asb_baev_path.as_deref() {
            if let Some(parent) = source.parent() {
                dialog = dialog.set_directory(parent);
            }
            if let Some(name) = source.file_name().and_then(|name| name.to_str()) {
                dialog = dialog.set_file_name(name);
            }
        }
        if let Some(destination) = dialog.save_file() {
            if let Err(error) = std::fs::write(&destination, bytes) {
                eprintln!("Unable to save BAEV {}: {error}", destination.display());
            }
        }
    }

    pub fn to_yaml(&self) -> io::Result<String> {
        serde_yaml::to_string(self).map_err(io::Error::other)
    }
}

fn read_maybe_compressed(path: &Path, zstd: &TotkZstd<'_>, magic: &[u8]) -> io::Result<Vec<u8>> {
    let data = std::fs::read(path)?;
    if data.starts_with(magic) {
        Ok(data)
    } else {
        zstd.try_decompress(&data)
    }
}

#[cfg(test)]
mod tests {
    use super::AsbFile;
    use std::{fs, path::Path};

    fn visit_asb_files(dir: &Path, tested: &mut usize) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit_asb_files(&path, tested);
            } else {
                let bytes = fs::read(&path).expect("read ASB corpus file");
                let Ok(file) = AsbFile::from_binary(&bytes) else {
                    continue;
                };
                let yaml = file.to_yaml().expect("serialize ASB YAML");
                let rebuilt = AsbFile::text_to_binary(&yaml, None, Some(&file.document))
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                assert_eq!(rebuilt, bytes, "{}", path.display());
                *tested += 1;
            }
        }
    }

    #[test]
    fn corpus_bare_yaml_saves_to_binary() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/AS");
        let mut tested = 0;
        visit_asb_files(&root, &mut tested);
        assert!(
            tested > 0,
            "no ASB corpus files found at {}",
            root.display()
        );
    }
}
