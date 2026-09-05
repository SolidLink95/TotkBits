use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use roead::aamp::ParameterIO;

use crate::{
    file_format::{
        asb::AsbFile,
        bphcl::BphclFile,
        bphhb::BphhbFile,
        bphyssb::BphyssbFile,
        hkcl::HkclFile,
        hkrg::HkrgFile,
        msbt::MsbtFile,
        Ainb::AinbFile,
        Archive::ArchiveDocument,
        BfevFile::BfevFile,
        BinTextFile::{BymlFile, FileData},
        Esetb::Esetb,
        Image::ImageDocument,
        Model3D::bfres::BfresFile,
        Pack::PackComparer,
        Rstb::Restbl,
        TagProduct::TagProduct,
        Xlink::Xlink_rs,
        SMO::SmoSaveFile::SmoSaveFile,
    },
    parser::{fbx::FbxFile, AOC::g1m::G1mFile},
    Open_and_Save::SendData,
    Settings::{Magic, Pathlib},
    Zstd::{is_esetb, is_tagproduct, TotkFileType, TotkZstd, ZstdDictionary},
};

pub enum FileGenesis {
    Disk,
    Archive,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileProperties {
    ReadOnly,
    Regular,
}

pub enum TotkEndian {
    LE,
    BE,
    None,
}

pub struct CacheText<'a> {
    pub byml: Option<BymlFile<'a>>,
    pub ainb: Option<crate::parser::ainb::AinbDocument>,
    pub asb: Option<AsbFile>,
    pub aamp: Option<ParameterIO>,
    pub asb_baev_path: Option<PathBuf>,
    pub asb_baev_data: Option<Vec<u8>>,
    pub smo: Option<SmoSaveFile<'a>>,
    pub esetb: Option<Esetb<'a>>,
    pub tag: Option<TagProduct<'a>>,
    pub msbt: Option<crate::parser::msbt::Msbt>,
}

impl Default for CacheText<'_> {
    fn default() -> Self {
        CacheText {
            byml: None,
            ainb: None,
            asb: None,
            aamp: None,
            asb_baev_path: None,
            asb_baev_data: None,
            smo: None,
            esetb: None,
            tag: None,
            msbt: None,
        }
    }
}

pub struct CacheArchive<'a> {
    pub sarc: PackComparer<'a>,
    pub archive: Option<ArchiveDocument>,
}

impl<'a> CacheArchive<'a> {
    pub fn default_new(zstd: Arc<TotkZstd<'a>>) -> Self {
        let sarc = PackComparer::default_new(zstd.clone());
        CacheArchive {
            sarc: sarc,
            archive: None,
        }
    }
}

pub struct CacheOther<'a> {
    pub rstb: Option<Restbl<'a>>,
    pub bphcl: Option<BphclFile>,
    pub bphhb: Option<BphhbFile>,
    pub bphyssb: Option<BphyssbFile>,
    pub hkcl: Option<HkclFile>,
    pub hkrg: Option<HkrgFile>,
    pub audio_data: Option<Vec<u8>>,
    pub physics_data: Option<Vec<u8>>,
}

impl Default for CacheOther<'_> {
    fn default() -> Self {
        CacheOther {
            rstb: None,
            bphcl: None,
            bphhb: None,
            bphyssb: None,
            hkcl: None,
            hkrg: None,
            audio_data: None,
            physics_data: None,
        }
    }
}
pub struct Cache3D {
    pub g1m: Option<G1mFile>,
    pub bfres: Option<BfresFile>,
    pub fbx: Option<FbxFile>,
    pub image: Option<ImageDocument>,
    pub source_data: Option<Vec<u8>>,
    pub custom_g1m: Option<Vec<u8>>,
}

impl Default for Cache3D {
    fn default() -> Self {
        Cache3D {
            g1m: None,
            bfres: None,
            fbx: None,
            image: None,
            source_data: None,
            custom_g1m: None,
        }
    }
}

pub struct TotkFile<'a> {
    uuid: String,
    parent_uuid: String,
    pub zstd: Arc<TotkZstd<'a>>,
    pub file_type: TotkFileType,
    pub endian: TotkEndian,
    pub compression: ZstdDictionary,
    pub yaz0_alignment: u32,
    pub genesis: FileGenesis,
    pub properties: FileProperties,
    pub path: Pathlib,
    pub binary_raw: Vec<u8>,
    pub text: String,
    //cache
    pub cache_arc: CacheArchive<'a>,
    pub cache_text: CacheText<'a>,
    pub cache_3d: Cache3D,
    pub cache_misc: CacheOther<'a>,
}

impl<'a> TotkFile<'a> {
    pub fn default(zstd: Arc<TotkZstd<'a>>) -> Self {
        Self {
            uuid: uuid::Uuid::new_v4().to_string(),
            parent_uuid: Default::default(),
            zstd: zstd.clone(),
            file_type: TotkFileType::None,
            endian: TotkEndian::None,
            compression: ZstdDictionary::None,
            yaz0_alignment: 0,
            genesis: FileGenesis::None,
            properties: FileProperties::Regular,
            path: Pathlib::default(),
            binary_raw: Default::default(),
            text: Default::default(),
            cache_arc: CacheArchive::default_new(zstd),
            cache_text: Default::default(),
            cache_3d: Default::default(),
            cache_misc: Default::default(),
        }
    }

    pub fn id(&self) -> String {
        self.uuid.clone()
    }

    pub fn magic(&self) -> TotkFileType {
        Magic::from_binary(&self.binary_raw)
    }

    pub fn parent(&self) -> String {
        self.parent_uuid.clone()
    }

    pub fn is_read_only(&self) -> bool {
        self.properties == FileProperties::ReadOnly
    }

    pub fn update_properties_from_file_type(&mut self) {
        self.properties = match self.file_type {
            TotkFileType::Fbx
            | TotkFileType::Glb
            | TotkFileType::Mii
            | TotkFileType::Image
            | TotkFileType::Bntx
            | TotkFileType::Bphhb
            | TotkFileType::Bphyssb
            | TotkFileType::Hkcl
            | TotkFileType::Hkrg
            | TotkFileType::Bwav
            | TotkFileType::Bfwav
            | TotkFileType::Amta
            | TotkFileType::Riff
            | TotkFileType::Other => FileProperties::ReadOnly,
            _ => FileProperties::Regular,
        };
    }

    pub fn reset(&mut self) {
        let zstd = self.zstd.clone();
        *self = Self::default(zstd);
    }

    pub fn tab(&self) -> &'static str {
        match self.file_type {
            TotkFileType::Sarc
            | TotkFileType::MalsSarc
            | TotkFileType::Bphcl
            | TotkFileType::Bphhb
            | TotkFileType::Bphyssb
            | TotkFileType::Hkcl
            | TotkFileType::Hkrg
            | TotkFileType::Archive
            | TotkFileType::Bars => "SARC",
            TotkFileType::Restbl => {
                if self.zstd.totk_config.rstb_view == "json" {
                    "YAML"
                } else {
                    "RSTB"
                }
            }
            TotkFileType::Bfres | TotkFileType::Fbx | TotkFileType::G1M => "3D",
            TotkFileType::Glb => "3D",
            TotkFileType::Bntx | TotkFileType::Image | TotkFileType::Mii => "IMAGE",
            TotkFileType::Amta => "AMTA",
            TotkFileType::Bwav | TotkFileType::Bfwav | TotkFileType::Riff => "AUDIO",
            TotkFileType::Compressed | TotkFileType::None | TotkFileType::Other => "ERROR",
            _ => "YAML",
        }
    }

    pub fn metadata(&self) -> String {
        let file_type = if self.file_type == TotkFileType::Archive {
            self.cache_arc
                .archive
                .as_ref()
                .map(ArchiveDocument::kind)
                .map(str::to_owned)
                .unwrap_or_else(|| "ARCHIVE".into())
        } else {
            format!("{:?}", self.file_type).to_ascii_uppercase()
        };
        let mut metadata = format!("[{file_type}]");
        match self.compression {
            ZstdDictionary::Zs
            | ZstdDictionary::Pack
            | ZstdDictionary::Empty
            | ZstdDictionary::Bcett => metadata.push_str(&format!(
                " [Zstd: {}]",
                format!("{:?}", self.compression).to_ascii_uppercase()
            )),
            ZstdDictionary::Yaz0 | ZstdDictionary::Mcpk => metadata.push_str(&format!(
                " [{}]",
                format!("{:?}", self.compression).to_ascii_uppercase()
            )),
            ZstdDictionary::None => {}
        }
        match self.endian {
            TotkEndian::LE => metadata.push_str(" [LE]"),
            TotkEndian::BE => metadata.push_str(" [BE]"),
            TotkEndian::None => {}
        }
        if cfg!(debug_assertions) {
            metadata.push_str(" [Debug]");
        }
        metadata
    }

    pub fn monaco_lang(&self) -> String {
        match self.file_type {
            TotkFileType::TagProduct | TotkFileType::Evfl => "json",
            TotkFileType::Restbl if self.zstd.totk_config.rstb_view == "json" => "json",
            TotkFileType::Xlink if self.zstd.totk_config.xlink_format == "modern" => "xlink",
            _ => "yaml",
        }
        .into()
    }

    pub fn send_data(&mut self) -> SendData {
        let mut data = SendData::default();
        data.path = self.path.clone();
        data.file_type = self.file_type;
        data.file_metadata = self.metadata();
        data.file_label = if data.path.name.is_empty() {
            data.file_metadata.clone()
        } else {
            format!("{} {}", data.path.name, data.file_metadata)
        };
        data.status_text = if data.path.full_path.is_empty() {
            format!("Opened {}", data.path.name)
        } else {
            format!("Opened {}", data.path.full_path)
        };
        data.tab = self.tab().into();
        data.lang = self.monaco_lang();
        data.read_only = self.is_read_only();

        if data.tab == "YAML" {
            data.text = std::mem::take(&mut self.text);
        }

        if let Some(archive) = &self.cache_arc.archive {
            data.tab = "SARC".into();
            data.file_type = if archive.kind() == "BARS" {
                TotkFileType::Bars
            } else {
                TotkFileType::Archive
            };
            data.sarc_paths.paths = archive.paths();
            data.sarc_paths.added_paths = archive.added.iter().cloned().collect();
            data.sarc_paths.modded_paths = archive.modified.iter().cloned().collect();
            data.sarc_paths.file_type = archive.kind().into();
            data.sarc_paths.root_name = data.path.name.clone();
            data.sarc_paths.read_only = data.read_only;
        } else if matches!(self.file_type, TotkFileType::Sarc | TotkFileType::MalsSarc) {
            data.get_sarc_paths(&self.cache_arc.sarc);
            data.sarc_paths.file_type = "SARC".into();
            data.sarc_paths.read_only = data.read_only;
        }

        self.binary_raw.clear();
        self.text.clear();
        data
    }

    fn apply_compression(&self, data: Vec<u8>) -> io::Result<Vec<u8>> {
        match self.compression {
            ZstdDictionary::None => Ok(data),
            ZstdDictionary::Yaz0 => {
                TotkZstd::compress_yaz0_with_alignment(&data, self.yaz0_alignment)
            }
            ZstdDictionary::Mcpk => self.zstd.compress_mcpk(&data),
            dictionary => self.zstd.compress_with_dictionary(&data, dictionary),
        }
    }

    pub fn to_binary(&mut self, edited_text: Option<&str>) -> io::Result<Vec<u8>> {
        let text = edited_text.unwrap_or(&self.text);
        let raw = match self.file_type {
            TotkFileType::Byml | TotkFileType::Bcett => {
                let byml = roead::byml::Byml::from_text(text)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                let endian = match self.endian {
                    TotkEndian::BE => roead::Endian::Big,
                    _ => roead::Endian::Little,
                };
                byml.to_binary(endian)
            }
            TotkFileType::ASB => AsbFile::text_to_binary(
                text,
                None,
                self.cache_text.asb.as_ref().map(|file| &file.document),
            )?,
            TotkFileType::AINB => AinbFile::text_to_binary(text)?,
            TotkFileType::Evfl => BfevFile::text_to_binary(text)?,
            TotkFileType::Xlink => {
                Xlink_rs::text_to_binary(text, &self.path.full_path, self.zstd.clone(), None)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "invalid XLink text")
                    })?
            }
            TotkFileType::Aamp => crate::file_format::bphcl::aamp_from_yaml(text)?.to_binary(),
            TotkFileType::Msbt => MsbtFile::text_to_binary(
                text,
                &self.path.full_path,
                self.cache_text.msbt.as_ref(),
                matches!(self.genesis, FileGenesis::Archive),
            )
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid MSBT text"))?,
            TotkFileType::Esetb => {
                let esetb = self.cache_text.esetb.as_mut().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "missing ESETB cache")
                })?;
                esetb.update_from_text(text)?;
                esetb.to_binary()
            }
            TotkFileType::TagProduct => {
                let tag = self.cache_text.tag.as_ref().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "missing TagProduct cache")
                })?;
                TagProduct::to_binary(text, tag.rank_table_bytes())?
            }
            TotkFileType::SmoSaveFile => {
                let smo = SmoSaveFile::from_string(text, self.zstd.clone())?;
                let data = smo.to_binary()?;
                self.cache_text.smo = Some(smo);
                data
            }
            TotkFileType::Text => text.as_bytes().to_vec(),
            TotkFileType::Bphcl => self
                .cache_misc
                .bphcl
                .as_ref()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing BPHCL cache"))?
                .raw_binary(),
            TotkFileType::Sarc | TotkFileType::MalsSarc => {
                let opened = self.cache_arc.sarc.opened.as_ref().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "missing SARC cache")
                })?;
                let entries = opened
                    .sarc
                    .files()
                    .filter_map(|file| file.name.map(|name| (name.to_owned(), file.data.to_vec())));
                return opened.rebuild_binary(entries);
            }
            TotkFileType::Archive | TotkFileType::Bars => {
                let archive = self.cache_arc.archive.as_ref().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "missing archive cache")
                })?;
                let bytes = archive
                    .to_bytes()
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                return self.apply_compression(bytes);
            }
            TotkFileType::Restbl => {
                let rstb = self.cache_misc.rstb.as_ref().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "missing RSTB cache")
                })?;
                let bytes = rstb
                    .table
                    .to_bytes()
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                return self.apply_compression(bytes);
            }
            TotkFileType::G1M => {
                return self
                    .cache_3d
                    .custom_g1m
                    .as_ref()
                    .or(self.cache_3d.source_data.as_ref())
                    .cloned()
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "missing G1M source")
                    });
            }
            TotkFileType::Bfres
            | TotkFileType::Fbx
            | TotkFileType::Glb
            | TotkFileType::Mii
            | TotkFileType::Image
            | TotkFileType::Bntx => {
                return self.cache_3d.source_data.clone().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "missing visual source")
                });
            }
            TotkFileType::Bwav | TotkFileType::Bfwav | TotkFileType::Amta | TotkFileType::Riff => {
                return self.cache_misc.audio_data.clone().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "missing audio source")
                });
            }
            TotkFileType::Other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unsupported file type",
                ));
            }
            TotkFileType::Bphhb
            | TotkFileType::Bphyssb
            | TotkFileType::Hkcl
            | TotkFileType::Hkrg => {
                return self.cache_misc.physics_data.clone().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "missing physics source")
                });
            }
            TotkFileType::Compressed | TotkFileType::None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "file is not parsed",
                ));
            }
        };
        self.apply_compression(raw)
    }

    pub fn save(&mut self, path: impl AsRef<Path>, edited_text: Option<&str>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(archive) = &mut self.cache_arc.archive {
            archive
                .save_atomic_with_zstd(path, &self.zstd)
                .map_err(io::Error::other)?;
            self.path = Pathlib::new(path);
            self.genesis = FileGenesis::Disk;
            return Ok(());
        }
        let bytes = self.to_binary(edited_text)?;
        std::fs::write(path, bytes)?;
        self.path = Pathlib::new(path);
        self.genesis = FileGenesis::Disk;
        Ok(())
    }

    pub fn visual_source(&self) -> Option<&[u8]> {
        self.cache_3d.source_data.as_deref()
    }

    pub fn audio_source(&self) -> Option<&[u8]> {
        self.cache_misc.audio_data.as_deref()
    }

    pub fn render_image(
        &self,
        texture_index: usize,
        array_index: u32,
        mip_index: u32,
    ) -> io::Result<crate::file_format::Image::RenderedImage> {
        let source = self
            .visual_source()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing image source"))?;
        ImageDocument::render_bytes_selection_with_zstd(
            source,
            &self.path.full_path,
            texture_index,
            array_index,
            mip_index,
            Some(&self.zstd),
        )
    }

    pub fn inspect_3d_json(&self) -> io::Result<serde_json::Value> {
        if let Some(bfres) = &self.cache_3d.bfres {
            let textures = crate::TauriCommands::visuals::resolve_bfres_textures(
                bfres,
                Path::new(&self.path.full_path),
                self.visual_source(),
                Path::new(&self.zstd.totk_config.romfs),
                Path::new(&self.zstd.totk_config.tomodachi_path),
                Some(&self.zstd),
            );
            let mut value = serde_json::to_value(bfres).map_err(io::Error::other)?;
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "resolvedTextures".into(),
                    serde_json::to_value(textures).map_err(io::Error::other)?,
                );
            }
            return Ok(value);
        }
        if let Some(fbx) = &self.cache_3d.fbx {
            return serde_json::to_value(fbx).map_err(io::Error::other);
        }
        if let Some(g1m) = &self.cache_3d.g1m {
            let resolution = g1m.resolve_textures(
                Path::new(&self.path.full_path),
                Path::new(&self.zstd.totk_config.aoc_path),
            );
            let mut value = serde_json::to_value(g1m).map_err(io::Error::other)?;
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "resolvedTextures".into(),
                    serde_json::to_value(resolution.textures).map_err(io::Error::other)?,
                );
                object.insert(
                    "textureStats".into(),
                    serde_json::json!({
                        "total": resolution.total,
                        "skipped": resolution.skipped,
                    }),
                );
            }
            return Ok(value);
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file has no parsed 3D model",
        ))
    }

    pub fn audio_preview(
        &self,
        entry_path: Option<&str>,
    ) -> Result<crate::TauriCommands::BfwavPreview, String> {
        use base64::Engine;
        let bytes = match entry_path {
            Some(path) => self
                .entry_bytes(path)
                .ok_or_else(|| format!("archive entry not found: {path}"))?,
            None => self
                .audio_source()
                .ok_or_else(|| "file has no audio payload".to_string())?,
        };
        let decoded = crate::file_format::Audio::decode(bytes)?;
        let wav = crate::file_format::Audio::to_wav(bytes)?;
        Ok(crate::TauriCommands::BfwavPreview {
            path: entry_path.unwrap_or(&self.path.full_path).into(),
            data_url: format!(
                "data:audio/wav;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(wav)
            ),
            size: bytes.len(),
            sample_rate: decoded.sample_rate,
            channels: decoded.channels.len(),
            samples: decoded.channels.first().map_or(0, Vec::len),
            looping: decoded.looping,
        })
    }

    pub fn replace_audio_entry(
        &mut self,
        path: &str,
        source: &Path,
        fit_to_original: bool,
        maximum_size: Option<usize>,
        dry_run: bool,
    ) -> Result<crate::TauriCommands::BfwavReplacement, String> {
        let original = self
            .entry_bytes(path)
            .ok_or_else(|| format!("archive entry not found: {path}"))?
            .to_vec();
        let old_size = original.len();
        let extension = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !extension.eq_ignore_ascii_case("wav") && !extension.eq_ignore_ascii_case("mp3") {
            return Err("choose an MP3 or WAV file".into());
        }
        let source = crate::file_format::Audio::Bfwav::decode_source(source)?;
        let encoded = if fit_to_original {
            crate::file_format::Audio::encode_replacement_to_limit(
                &original,
                &source,
                maximum_size.unwrap_or(old_size),
            )?
        } else {
            crate::file_format::Audio::encode_replacement(&original, &source)?
        };
        let new_size = encoded.len();
        let sample_rate = crate::file_format::Audio::decode(&encoded)?.sample_rate;
        if !dry_run {
            self.set_entry(path, encoded)
                .map_err(|error| error.to_string())?;
        }
        Ok(crate::TauriCommands::BfwavReplacement {
            old_size,
            new_size,
            increased: new_size > old_size,
            compressed: fit_to_original,
            sample_rate,
        })
    }

    pub fn replace_bars_audio_from_folder(
        &mut self,
        folder: &Path,
        fit_to_original: bool,
        dry_run: bool,
    ) -> Result<crate::TauriCommands::BarsFolderReplacement, String> {
        use std::collections::HashMap;
        if !folder.is_dir() {
            return Err(format!(
                "audio replacement folder does not exist: {}",
                folder.display()
            ));
        }
        let mut sources = HashMap::new();
        for item in std::fs::read_dir(folder).map_err(|error| error.to_string())? {
            let path = item.map_err(|error| error.to_string())?.path();
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if !extension.eq_ignore_ascii_case("wav") && !extension.eq_ignore_ascii_case("mp3") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
                sources.entry(stem.to_ascii_lowercase()).or_insert(path);
            }
        }
        let targets: Vec<String> = self
            .cache_arc
            .archive
            .as_ref()
            .ok_or("no archive is open")?
            .paths()
            .into_iter()
            .filter(|path| {
                path.starts_with("Audio/") && (path.ends_with(".bfwav") || path.ends_with(".bwav"))
            })
            .collect();
        let mut result = crate::TauriCommands::BarsFolderReplacement {
            replaced: Vec::new(),
            skipped: Vec::new(),
            failed: Vec::new(),
            oversized: Vec::new(),
        };
        for target in targets {
            let stem = Path::new(&target)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let Some(source) = sources.get(&stem) else {
                result.skipped.push(target);
                continue;
            };
            let maximum = self.entry_bytes(&target).map(<[u8]>::len);
            match self.replace_audio_entry(&target, source, fit_to_original, maximum, dry_run) {
                Ok(replacement) => {
                    if replacement.increased {
                        result.oversized.push(target.clone());
                    }
                    result.replaced.push(target);
                }
                Err(error) => result.failed.push(format!("{target}: {error}")),
            }
        }
        Ok(result)
    }

    pub fn available_g1a_animations(
        &self,
    ) -> Vec<crate::file_format::Animation::g1a::AvailableG1aAnimation> {
        let Some(g1m) = &self.cache_3d.g1m else {
            return Vec::new();
        };
        if self.zstd.totk_config.aoc_path.is_empty() {
            return Vec::new();
        }
        crate::file_format::Animation::g1a::available_animations(
            &g1m.model_hash,
            Path::new(&self.zstd.totk_config.aoc_path),
        )
    }

    pub fn inspect_g1a_animation(
        &self,
        path: impl AsRef<Path>,
    ) -> io::Result<crate::file_format::Animation::g1a::G1aFile> {
        if self.cache_3d.g1m.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file is not G1M",
            ));
        }
        crate::file_format::Animation::g1a::G1aFile::from_path(path)
    }

    fn location(&self) -> &'static str {
        match self.genesis {
            FileGenesis::Disk => "disk",
            FileGenesis::Archive => "archive",
            FileGenesis::None => "memory",
        }
    }

    pub fn bphcl_document_json(
        &self,
        document_id: impl Into<String>,
    ) -> Option<crate::DocumentState::OpenBphclDocument> {
        let file = self.cache_misc.bphcl.as_ref()?;
        Some(crate::DocumentState::OpenBphclDocument {
            document_id: document_id.into(),
            label: self.path.name.clone(),
            path: self.path.full_path.clone(),
            location: self.location().into(),
            cloth_count: file.document.cloth.len(),
            collidable_count: file.document.collidables.len(),
        })
    }

    pub fn bphcl_selectable_nodes(
        &self,
        document_id: &str,
    ) -> Vec<crate::DocumentState::BphclSelectableNode> {
        let Some(file) = &self.cache_misc.bphcl else {
            return Vec::new();
        };
        file.document
            .cloth
            .iter()
            .map(|cloth| crate::DocumentState::BphclSelectableNode {
                document_id: document_id.into(),
                node_id: format!("cloth:{}", cloth.index),
                kind: "cloth".into(),
                index: cloth.index,
                name: cloth.name.clone(),
                item_index: cloth.item_index,
            })
            .chain(file.document.collidables.iter().map(|collidable| {
                crate::DocumentState::BphclSelectableNode {
                    document_id: document_id.into(),
                    node_id: format!("collidable:{}", collidable.index),
                    kind: "collidable".into(),
                    index: collidable.index,
                    name: collidable.name.clone(),
                    item_index: collidable.item_index,
                }
            }))
            .collect()
    }

    pub fn hkcl_document_json(
        &self,
        document_id: impl Into<String>,
    ) -> Option<crate::DocumentState::OpenHkclDocument> {
        let file = self.cache_misc.hkcl.as_ref()?;
        Some(crate::DocumentState::OpenHkclDocument {
            document_id: document_id.into(),
            label: self.path.name.clone(),
            path: self.path.full_path.clone(),
            location: self.location().into(),
            skeleton_count: file.document.physics.skeletons.len(),
            cloth_count: file.document.physics.cloths.len(),
            constraint_count: file.document.physics.constraints.len(),
            collidable_count: file.document.physics.collidables.len(),
        })
    }

    pub fn hkcl_selectable_nodes(
        &self,
        document_id: &str,
    ) -> Vec<crate::DocumentState::HkclSelectableNode> {
        let Some(file) = &self.cache_misc.hkcl else {
            return Vec::new();
        };
        file.document
            .physics
            .cloths
            .iter()
            .enumerate()
            .map(|(index, cloth)| crate::DocumentState::HkclSelectableNode {
                document_id: document_id.into(),
                node_id: format!("cloth:{index}"),
                kind: "cloth".into(),
                index,
                name: cloth
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("Cloth {index}")),
                section_index: cloth.key.section_index,
                data_offset: cloth.key.offset,
            })
            .chain(file.document.physics.collidables.iter().enumerate().map(
                |(index, collidable)| {
                    crate::DocumentState::HkclSelectableNode {
                        document_id: document_id.into(),
                        node_id: format!("collidable:{index}"),
                        kind: "collidable".into(),
                        index,
                        name: collidable
                            .name
                            .clone()
                            .unwrap_or_else(|| format!("Collidable {index}")),
                        section_index: collidable.key.section_index,
                        data_offset: collidable.key.offset,
                    }
                },
            ))
            .collect()
    }

    pub fn bphhb_document_json(
        &self,
        document_id: impl Into<String>,
    ) -> Option<crate::DocumentState::OpenBphhbDocument> {
        let file = self.cache_misc.bphhb.as_ref()?;
        Some(crate::DocumentState::OpenBphhbDocument {
            document_id: document_id.into(),
            label: self.path.name.clone(),
            path: self.path.full_path.clone(),
            location: self.location().into(),
            bone_count: file.document.bones.len(),
        })
    }

    pub fn replace_g1m_source(&mut self, data: Vec<u8>) -> io::Result<()> {
        if !Magic::is_g1m(&data) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid G1M source",
            ));
        }
        let name = self
            .path
            .stem
            .split('.')
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or("G1M");
        self.cache_3d.g1m = Some(G1mFile::parse(&data, name)?);
        self.cache_3d.custom_g1m = Some(data);
        self.file_type = TotkFileType::G1M;
        Ok(())
    }

    pub fn replace_bfres_source(&mut self, data: Vec<u8>) -> io::Result<()> {
        let (raw, _) = self
            .zstd
            .try_decompress_all_ordered_safe(&data, &self.path.full_path);
        let bfres = BfresFile::from_bytes(&raw)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        self.cache_3d.bfres = Some(bfres);
        self.cache_3d.source_data = Some(data);
        self.file_type = TotkFileType::Bfres;
        Ok(())
    }

    pub fn save_asb_baev(&self, destination: impl AsRef<Path>) -> io::Result<()> {
        if self.file_type != TotkFileType::ASB {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file is not ASB",
            ));
        }
        let data = self.cache_text.asb_baev_data.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "ASB has no selected BAEV companion",
            )
        })?;
        std::fs::write(destination, data)
    }

    pub fn entry_bytes(&self, path: &str) -> Option<&[u8]> {
        if let Some(archive) = &self.cache_arc.archive {
            return archive.get(path);
        }
        self.cache_arc.sarc.opened.as_ref()?.sarc.get_data(path)
    }

    pub fn set_entry(&mut self, path: &str, bytes: Vec<u8>) -> io::Result<()> {
        if let Some(archive) = &mut self.cache_arc.archive {
            return archive
                .set(path, bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error));
        }
        let pack =
            self.cache_arc.sarc.opened.as_mut().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "file is not an archive")
            })?;
        let existed = pack.sarc.get_data(path).is_some();
        let hash = crate::Zstd::sha256(bytes.clone());
        pack.mutate_writer(|writer| writer.add_file(path, bytes))?;
        if existed {
            self.cache_arc.sarc.modded.insert(path.into(), hash);
        } else {
            self.cache_arc.sarc.added.insert(path.into(), hash);
        }
        Ok(())
    }

    pub fn remove_entry(&mut self, path: &str) -> io::Result<()> {
        if let Some(archive) = &mut self.cache_arc.archive {
            let removed = archive
                .remove_prefix(path)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
            return (removed > 0)
                .then_some(())
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "archive entry not found"));
        }
        let pack =
            self.cache_arc.sarc.opened.as_mut().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "file is not an archive")
            })?;
        if pack.sarc.get_data(path).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "SARC entry not found",
            ));
        }
        pack.mutate_writer(|writer| writer.remove_file(path))?;
        self.cache_arc.sarc.added.remove(path);
        self.cache_arc.sarc.modded.remove(path);
        Ok(())
    }

    pub fn rename_entry(&mut self, from: &str, to: &str) -> io::Result<()> {
        if let Some(archive) = &mut self.cache_arc.archive {
            let renamed = archive
                .rename_prefix(from, to)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
            return (renamed > 0)
                .then_some(())
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "archive entry not found"));
        }
        let pack =
            self.cache_arc.sarc.opened.as_mut().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "file is not an archive")
            })?;
        pack.rename(from, to)?;
        if let Some(value) = self.cache_arc.sarc.added.remove(from) {
            self.cache_arc.sarc.added.insert(to.into(), value);
        } else {
            self.cache_arc.sarc.modded.insert(to.into(), String::new());
        }
        self.cache_arc.sarc.modded.remove(from);
        Ok(())
    }

    pub fn filetype_from_path(path: impl AsRef<Path>) -> TotkFileType {
        let path = path.as_ref();
        if is_tagproduct(path) {
            return TotkFileType::TagProduct;
        }
        if is_esetb(path) {
            return TotkFileType::Esetb;
        }
        if Pathlib::is_rstb_path(path) {
            return TotkFileType::Restbl;
        }
        if Pathlib::is_bphhb_path(path) {
            return TotkFileType::Bphhb;
        }
        if Pathlib::is_hkcl_path(path) {
            return TotkFileType::Hkcl;
        }
        if Pathlib::is_hkrg_path(path) {
            return TotkFileType::Hkrg;
        }
        if Pathlib::is_bphyssb_path(path) {
            return TotkFileType::Bphyssb;
        }
        TotkFileType::None
    }

    pub fn from_folder(path: impl AsRef<Path>, zstd: Arc<TotkZstd<'a>>) -> io::Result<Self> {
        let path = path.as_ref();
        let mut result = Self::default(zstd);
        result.path = Pathlib::new(path);
        result.file_type = TotkFileType::Archive;
        result.genesis = FileGenesis::Disk;
        result.cache_arc.archive = Some(
            ArchiveDocument::open_folder(path)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
        );
        Ok(result)
    }

    pub fn from_file(path: impl AsRef<Path>, zstd: Arc<TotkZstd<'a>>) -> io::Result<Self> {
        let path = path.as_ref();
        if path.is_dir() {
            return Self::from_folder(path, zstd);
        }
        let data = std::fs::read(path)?;
        let mut result = Self::from_binary(&data, zstd.clone(), path)?;
        result.genesis = FileGenesis::Disk;
        if result.file_type == TotkFileType::ASB {
            let suggested_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| format!("{}.root.baev", name.split('.').next().unwrap_or(name)))
                .unwrap_or_else(|| "*.baev".into());
            let mut dialog = rfd::FileDialog::new()
                .set_title("Select optional BAEV")
                .add_filter("Binary Animation Event", &["baev", "zs"])
                .set_file_name(&suggested_name);
            if let Some(parent) = path.parent() {
                dialog = dialog.set_directory(parent);
            }
            if let Some(baev_path) = dialog.pick_file() {
                let source = std::fs::read(&baev_path)?;
                let (baev, _) = zstd.try_decompress_all_ordered_safe(&source, &baev_path);
                let asb = result
                    .cache_text
                    .asb
                    .take()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing ASB cache"))?
                    .with_baev(Some(&baev))?;
                result.text = asb.to_yaml()?;
                result.cache_text.asb = Some(asb);
                result.cache_text.asb_baev_path = Some(baev_path);
                result.cache_text.asb_baev_data = Some(source);
            }
        }
        Ok(result)
    }

    pub fn from_archive_bytes(
        path: impl AsRef<Path>,
        data: &[u8],
        zstd: Arc<TotkZstd<'a>>,
    ) -> io::Result<Self> {
        let mut result = Self::from_binary(data, zstd, path)?;
        result.genesis = FileGenesis::Archive;
        Ok(result)
    }

    pub fn from_opened_file(
        opened: crate::file_format::BinTextFile::OpenedFile<'a>,
        text: String,
        zstd: Arc<TotkZstd<'a>>,
    ) -> io::Result<Self> {
        let mut result = Self::default(zstd);
        result.file_type = opened.file_type;
        result.path = opened.path;
        result.compression = opened.compression.unwrap_or(ZstdDictionary::None);
        result.yaz0_alignment = opened.yaz0_alignment;
        result.update_endian(opened.endian);
        result.text = text;
        result.cache_text.byml = opened.byml;
        result.cache_text.msbt = opened.msyt;
        result.cache_text.tag = opened.tag;
        result.cache_text.esetb = opened.esetb;
        result.cache_text.asb_baev_path = opened.asb_baev_path;
        result.cache_text.asb_baev_data = opened.asb_baev_data;
        result.cache_misc.rstb = opened.restbl;
        result.cache_misc.bphcl = opened.bphcl;
        result.cache_misc.bphhb = opened.bphhb;
        result.cache_misc.bphyssb = opened.bphyssb;
        result.cache_misc.hkcl = opened.hkcl;
        result.cache_misc.hkrg = opened.hkrg;
        result.cache_3d.bfres = opened.bfres;
        result.cache_3d.source_data = opened.visual_data.or(opened.bfres_data);
        result.cache_3d.custom_g1m = opened.custom_g1m;
        result.cache_text.asb = opened.asb.map(|document| AsbFile {
            document,
            baev: None,
        });
        if result.file_type == TotkFileType::ASB
            && result.cache_text.asb.is_none()
            && !result.text.is_empty()
        {
            let binary = AsbFile::text_to_binary(&result.text, None, None)?;
            result.cache_text.asb = Some(AsbFile::from_binary(&binary)?);
        }
        if result.file_type == TotkFileType::AINB && !result.text.is_empty() {
            result.cache_text.ainb =
                Some(crate::parser::ainb::AinbDocument::from_yaml(&result.text)?);
        }
        if result.file_type == TotkFileType::Aamp && !result.text.is_empty() {
            result.cache_text.aamp = Some(crate::file_format::bphcl::aamp_from_yaml(&result.text)?);
        }
        result.update_properties_from_file_type();
        Ok(result)
    }

    pub fn from_internal_file(
        internal: crate::InternalFile::InternalFile<'a>,
        text: String,
        zstd: Arc<TotkZstd<'a>>,
    ) -> io::Result<Self> {
        let mut result = Self::default(zstd);
        result.file_type = internal.file_type;
        result.path = internal.path;
        result.compression = internal.compression.unwrap_or(ZstdDictionary::None);
        result.yaz0_alignment = internal.yaz0_alignment;
        result.update_endian(internal.endian);
        result.genesis = FileGenesis::Archive;
        result.text = text;
        result.cache_text.byml = internal.byml;
        result.cache_text.ainb = internal.ainb;
        result.cache_text.msbt = internal.msyt;
        result.cache_text.esetb = internal.esetb;
        result.cache_text.tag = internal.tag;
        result.cache_text.asb = internal.asb.map(|document| AsbFile {
            document,
            baev: None,
        });
        if let Some(aamp) = internal.aamp {
            result.cache_text.aamp = Some(crate::file_format::bphcl::aamp_from_yaml(&aamp)?);
        }
        if result.file_type == TotkFileType::ASB
            && result.cache_text.asb.is_none()
            && !result.text.is_empty()
        {
            let binary = AsbFile::text_to_binary(&result.text, None, None)?;
            result.cache_text.asb = Some(AsbFile::from_binary(&binary)?);
        }
        Ok(result)
    }

    pub fn raise_err_template<T>(&self, msg: String) -> io::Result<T> {
        let _ = rfd::MessageDialog::new()
            .set_title("Error")
            .set_description(msg.as_str())
            .show();
        Err(io::Error::new(io::ErrorKind::InvalidData, msg))
    }

    pub fn raise_err_path_filetype<T>(&self) -> io::Result<T> {
        let msg = format!(
            "ERROR: Unable to parse path based file {} of type {:?}!",
            &self.path.full_path, &self.file_type
        );
        self.raise_err_template(msg)
    }

    pub fn raise_err_magic_filetype<T>(&self) -> io::Result<T> {
        let msg = format!(
            "ERROR: Unable to parse magic based file {:?} of type {:?}!",
            Magic::magic_to_str(&self.binary_raw),
            &self.file_type
        );
        self.raise_err_template(msg)
    }

    pub fn update_endian_from_byml(&mut self, byml: &BymlFile) {
        self.update_endian(byml.endian);
    }

    pub fn update_endian(&mut self, endian: Option<roead::Endian>) {
        self.endian = match endian {
            Some(roead::Endian::Little) => TotkEndian::LE,
            Some(roead::Endian::Big) => TotkEndian::BE,
            None => TotkEndian::None,
        };
    }

    pub fn raise_not_implemented<T>(&self) -> io::Result<T> {
        self.raise_err_template("NOT implemented feature! FIX!".to_string())
    }

    pub fn from_binary(
        data: &[u8],
        zstd: Arc<TotkZstd<'a>>,
        path: impl AsRef<Path>,
    ) -> io::Result<Self> {
        let mut res = Self::default(zstd.clone());
        let path = path.as_ref();
        res.yaz0_alignment = Magic::is_yaz0(data)
            .then(|| TotkZstd::yaz0_alignment(data))
            .unwrap_or_default();
        (res.binary_raw, res.compression) = zstd.try_decompress_all_ordered_safe(data, path);
        res.path = Pathlib::new(path);
        let filetype_path = Self::filetype_from_path(path);
        let filetype_magic = res.magic();
        match filetype_path {
            TotkFileType::Bphhb => {
                res.cache_misc.bphhb = Some(
                    BphhbFile::from_binary(&res.binary_raw, Some(path))
                        .or_else(|_| res.raise_err_path_filetype())?,
                );
                res.file_type = TotkFileType::Bphhb;
                res.cache_misc.physics_data = Some(data.to_vec());
                res.update_properties_from_file_type();
                return Ok(res);
            }
            TotkFileType::Hkcl => {
                res.cache_misc.hkcl = Some(
                    HkclFile::from_binary(&res.binary_raw, Some(path))
                        .or_else(|_| res.raise_err_path_filetype())?,
                );
                res.file_type = TotkFileType::Hkcl;
                res.cache_misc.physics_data = Some(data.to_vec());
                res.update_properties_from_file_type();
                return Ok(res);
            }
            TotkFileType::Hkrg => {
                res.cache_misc.hkrg = Some(
                    HkrgFile::from_binary(&res.binary_raw, Some(path))
                        .or_else(|_| res.raise_err_path_filetype())?,
                );
                res.file_type = TotkFileType::Hkrg;
                res.cache_misc.physics_data = Some(data.to_vec());
                res.update_properties_from_file_type();
                return Ok(res);
            }
            TotkFileType::Bphyssb => {
                res.cache_misc.bphyssb = Some(
                    BphyssbFile::from_binary(&res.binary_raw, Some(path))
                        .or_else(|_| res.raise_err_path_filetype())?,
                );
                res.file_type = TotkFileType::Bphyssb;
                res.cache_misc.physics_data = Some(data.to_vec());
                res.update_properties_from_file_type();
                return Ok(res);
            }
            _ => {}
        }
        if filetype_magic == TotkFileType::None {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid magic: {}", Magic::magic_to_str(&res.binary_raw)),
            ));
        }
        res.file_type = filetype_path;
        match filetype_path {
            TotkFileType::TagProduct => {
                if let Some(mut tag) = TagProduct::from_binary(&res.binary_raw, path, zstd.clone())
                {
                    res.endian = TotkEndian::LE;
                    res.file_type = TotkFileType::TagProduct;
                    res.text = tag.to_text();
                    res.cache_text.tag = Some(tag);
                    return Ok(res);
                }
                return res.raise_err_path_filetype();
            }
            TotkFileType::Esetb => {
                if let Ok(esetb) = Esetb::from_binary(&res.binary_raw, zstd.clone()) {
                    res.update_endian_from_byml(&esetb.byml);
                    res.file_type = TotkFileType::Esetb;
                    res.text = esetb.to_string();
                    res.cache_text.esetb = Some(esetb);
                    return Ok(res);
                }
                return res.raise_err_path_filetype();
            }
            TotkFileType::Restbl => {
                if let Some(restbl) = Restbl::from_binary(data, zstd.clone(), path) {
                    res.endian = TotkEndian::LE;
                    res.file_type = TotkFileType::Restbl;
                    res.path = Pathlib::new(path);
                    if zstd.totk_config.rstb_view == "json" {
                        res.text = match restbl.to_json() {
                            Ok(text) => text,
                            Err(error) => {
                                return res.raise_err_template(format!(
                                    "Detected Restbl for {}, but parsing failed: {error}",
                                    path.display()
                                ));
                            }
                        };
                    }
                    res.cache_misc.rstb = Some(restbl);
                    return Ok(res);
                }
                return res.raise_err_path_filetype();
            }
            TotkFileType::None => {}
            _ => {
                // return res.raise_not_implemented();
            }
        }
        res.file_type = filetype_magic;
        macro_rules! magic_result {
            ($value:expr) => {
                match $value {
                    Ok(value) => value,
                    Err(_) => return res.raise_err_magic_filetype(),
                }
            };
        }
        macro_rules! magic_option {
            ($value:expr) => {
                match $value {
                    Some(value) => value,
                    None => return res.raise_err_magic_filetype(),
                }
            };
        }
        match filetype_magic {
            TotkFileType::Byml => {
                let file_type = if res.compression == ZstdDictionary::Bcett {
                    TotkFileType::Bcett
                } else {
                    TotkFileType::Byml
                };
                let file_data = FileData {
                    file_type,
                    data: res.binary_raw.clone(),
                    compression: (res.compression != ZstdDictionary::None)
                        .then_some(res.compression),
                    yaz0_alignment: 0,
                };
                let mut byml =
                    magic_result!(BymlFile::from_binary(&res.binary_raw, zstd.clone(), path));
                byml.file_type = file_type;
                byml.file_data = file_data;
                res.update_endian_from_byml(&byml);
                res.file_type = file_type;
                res.text = byml.to_string();
                res.cache_text.byml = Some(byml);
            }
            TotkFileType::ASB => {
                let asb = magic_result!(AsbFile::from_binary(&res.binary_raw));
                res.text = magic_result!(asb.to_yaml());
                res.endian = TotkEndian::LE;
                res.file_type = TotkFileType::ASB;
                res.cache_text.asb = Some(asb);
            }
            TotkFileType::AINB => {
                let ainb = magic_result!(crate::parser::ainb::AinbDocument::from_bytes(
                    &res.binary_raw
                ));
                res.text = magic_result!(ainb.to_yaml());
                res.endian = TotkEndian::LE;
                res.file_type = TotkFileType::AINB;
                res.cache_text.ainb = Some(ainb);
            }
            TotkFileType::Evfl => {
                let bfev = magic_result!(BfevFile::from_binary(&res.binary_raw));
                res.text = magic_result!(bfev.document.to_json());
                res.endian = TotkEndian::LE;
                res.file_type = TotkFileType::Evfl;
            }
            TotkFileType::Xlink => {
                let compression =
                    (res.compression != ZstdDictionary::None).then_some(res.compression);
                let (_, text) = magic_option!(Xlink_rs::open_internal(
                    path,
                    &res.binary_raw,
                    zstd.clone(),
                    compression
                ));
                res.text = text;
                res.endian = TotkEndian::LE;
                res.file_type = TotkFileType::Xlink;
            }
            TotkFileType::Aamp => {
                let pio = magic_result!(ParameterIO::from_binary(&res.binary_raw));
                res.text = magic_result!(crate::file_format::bphcl::safe_aamp_yaml(&pio));
                res.endian = TotkEndian::None;
                res.file_type = TotkFileType::Aamp;
                res.cache_text.aamp = Some(pio);
            }
            TotkFileType::Msbt => {
                let msbt = magic_result!(crate::parser::msbt::Msbt::from_bytes(&res.binary_raw));
                res.endian = match msbt.header.endian {
                    crate::parser::binary::Endian::Little => TotkEndian::LE,
                    crate::parser::binary::Endian::Big => TotkEndian::BE,
                };
                res.text = crate::parser::msbt::editable::serialize(&msbt);
                res.file_type = TotkFileType::Msbt;
                res.cache_text.msbt = Some(msbt);
            }
            TotkFileType::Text => {
                res.text = magic_result!(String::from_utf8(res.binary_raw.clone()));
                res.endian = TotkEndian::None;
                res.file_type = TotkFileType::Text;
            }
            TotkFileType::Restbl => {
                let restbl = magic_option!(Restbl::from_binary(data, zstd.clone(), path));
                if zstd.totk_config.rstb_view == "json" {
                    res.text = magic_result!(restbl.to_json());
                }
                res.endian = TotkEndian::LE;
                res.file_type = TotkFileType::Restbl;
                res.cache_misc.rstb = Some(restbl);
            }
            TotkFileType::Sarc | TotkFileType::MalsSarc => {
                let pack = magic_result!(crate::file_format::Pack::PackFile::from_binary(
                    data,
                    zstd.clone()
                ));
                res.endian = match pack.endian {
                    roead::Endian::Little => TotkEndian::LE,
                    roead::Endian::Big => TotkEndian::BE,
                };
                res.file_type = filetype_magic;
                res.cache_arc.sarc = magic_option!(PackComparer::from_pack(pack, zstd.clone()));
            }
            TotkFileType::Archive | TotkFileType::Bars => {
                let dictionary =
                    (res.compression != ZstdDictionary::None).then_some(res.compression);
                let archive = ArchiveDocument::from_binary(&res.binary_raw, path, dictionary)
                    .map_err(|_| ())
                    .ok()
                    .flatten();
                res.cache_arc.archive = Some(magic_option!(archive));
                res.file_type = filetype_magic;
            }
            TotkFileType::Bfres => {
                res.cache_3d.bfres = Some(magic_result!(BfresFile::from_bytes(&res.binary_raw)));
                res.cache_3d.source_data = Some(data.to_vec());
                res.file_type = TotkFileType::Bfres;
            }
            TotkFileType::Fbx => {
                let name = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("FBX");
                res.cache_3d.fbx = Some(magic_result!(FbxFile::parse(&res.binary_raw, name)));
                res.cache_3d.source_data = Some(data.to_vec());
                res.file_type = TotkFileType::Fbx;
                res.update_properties_from_file_type();
            }
            TotkFileType::G1M => {
                let name = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("G1M");
                res.cache_3d.g1m = Some(magic_result!(G1mFile::parse(&res.binary_raw, name)));
                res.cache_3d.source_data = Some(data.to_vec());
                res.file_type = TotkFileType::G1M;
            }
            TotkFileType::Image | TotkFileType::Bntx => {
                if ImageDocument::open_binary(&res.binary_raw, path, &zstd).is_none() {
                    return res.raise_err_magic_filetype();
                }
                res.cache_3d.image = Some(ImageDocument);
                res.cache_3d.source_data = Some(data.to_vec());
                res.file_type = filetype_magic;
                res.update_properties_from_file_type();
            }
            TotkFileType::Bphcl => {
                res.cache_misc.bphcl = Some(magic_result!(BphclFile::from_binary(
                    &res.binary_raw,
                    Some(path)
                )));
                res.file_type = TotkFileType::Bphcl;
            }
            TotkFileType::Hkrg => {
                res.cache_misc.hkrg = Some(magic_result!(HkrgFile::from_binary(
                    &res.binary_raw,
                    Some(path)
                )));
                res.file_type = TotkFileType::Hkrg;
                res.cache_misc.physics_data = Some(data.to_vec());
                res.update_properties_from_file_type();
            }
            TotkFileType::Bphyssb => {
                res.cache_misc.bphyssb = Some(magic_result!(BphyssbFile::from_binary(
                    &res.binary_raw,
                    Some(path)
                )));
                res.file_type = TotkFileType::Bphyssb;
                res.cache_misc.physics_data = Some(data.to_vec());
                res.update_properties_from_file_type();
            }
            TotkFileType::Hkcl => {
                res.cache_misc.hkcl = Some(magic_result!(HkclFile::from_binary(
                    &res.binary_raw,
                    Some(path)
                )));
                res.file_type = TotkFileType::Hkcl;
                res.cache_misc.physics_data = Some(data.to_vec());
                res.update_properties_from_file_type();
            }
            TotkFileType::Bwav | TotkFileType::Bfwav | TotkFileType::Amta | TotkFileType::Riff => {
                res.file_type = filetype_magic;
                res.update_properties_from_file_type();
                res.cache_misc.audio_data = Some(data.to_vec());
            }
            TotkFileType::Other => {
                res.file_type = TotkFileType::Other;
                res.update_properties_from_file_type();
            }
            TotkFileType::SmoSaveFile => {
                let mut smo = magic_result!(SmoSaveFile::from_binary(
                    &res.binary_raw,
                    zstd.clone(),
                    path
                ));
                res.text = magic_result!(smo.to_string());
                res.endian = TotkEndian::LE;
                res.file_type = TotkFileType::SmoSaveFile;
                res.cache_text.smo = Some(smo);
            }
            _ => return res.raise_err_magic_filetype(),
        }

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};

    fn test_zstd() -> Arc<TotkZstd<'static>> {
        Arc::new(TotkZstd::dictionaryless(
            Arc::new(TotkConfig::default()),
            TOTK_ZSTD_COMPRESSION_LEVEL,
        ))
    }

    #[test]
    fn id_is_a_read_only_uuid_v4_copy() {
        let file = TotkFile::default(test_zstd());
        let second = TotkFile::default(test_zstd());
        let mut id = file.id();

        assert_eq!(uuid::Uuid::parse_str(&id).unwrap().get_version_num(), 4);
        assert_ne!(id, second.id());
        id.push_str("-changed");
        assert_ne!(id, file.id());
        assert_eq!(
            uuid::Uuid::parse_str(&file.id()).unwrap().get_version_num(),
            4
        );
    }

    #[test]
    fn file_properties_default_to_regular() {
        let mut file = TotkFile::default(test_zstd());
        assert_eq!(file.properties, FileProperties::Regular);
        assert!(!file.is_read_only());

        file.properties = FileProperties::ReadOnly;
        assert!(file.is_read_only());
    }

    #[test]
    fn tab_matches_file_type_category() {
        let mut file = TotkFile::default(test_zstd());
        for (file_type, expected) in [
            (TotkFileType::Sarc, "SARC"),
            (TotkFileType::Byml, "YAML"),
            (TotkFileType::Restbl, "RSTB"),
            (TotkFileType::G1M, "3D"),
            (TotkFileType::Image, "IMAGE"),
            (TotkFileType::None, "ERROR"),
        ] {
            file.file_type = file_type;
            assert_eq!(file.tab(), expected);
        }

        let mut config = TotkConfig::default();
        config.rstb_view = "json".into();
        let mut json_rstb = TotkFile::default(Arc::new(TotkZstd::dictionaryless(
            Arc::new(config),
            TOTK_ZSTD_COMPRESSION_LEVEL,
        )));
        json_rstb.file_type = TotkFileType::Restbl;
        assert_eq!(json_rstb.tab(), "YAML");
    }

    #[test]
    fn metadata_describes_file_type_compression_and_build() {
        let mut file = TotkFile::default(test_zstd());
        file.file_type = TotkFileType::Byml;
        file.compression = ZstdDictionary::Pack;
        let debug_suffix = if cfg!(debug_assertions) {
            " [Debug]"
        } else {
            ""
        };
        assert_eq!(
            file.metadata(),
            format!("[BYML] [Zstd: PACK]{debug_suffix}")
        );

        file.compression = ZstdDictionary::Yaz0;
        assert_eq!(file.metadata(), format!("[BYML] [YAZ0]{debug_suffix}"));

        file.compression = ZstdDictionary::None;
        assert_eq!(file.metadata(), format!("[BYML]{debug_suffix}"));

        file.endian = TotkEndian::LE;
        assert_eq!(file.metadata(), format!("[BYML] [LE]{debug_suffix}"));

        file.compression = ZstdDictionary::Bcett;
        file.endian = TotkEndian::BE;
        assert_eq!(
            file.metadata(),
            format!("[BYML] [Zstd: BCETT] [BE]{debug_suffix}")
        );

        file.file_type = TotkFileType::Archive;
        file.compression = ZstdDictionary::None;
        file.endian = TotkEndian::None;
        file.cache_arc.archive = Some(ArchiveDocument {
            archive: crate::file_format::Archive::RootArchive::SevenZip(Default::default()),
            path: String::new(),
            added: Default::default(),
            modified: Default::default(),
            dictionary: None,
        });
        assert_eq!(file.metadata(), format!("[7Z]{debug_suffix}"));
    }

    #[test]
    fn monaco_lang_follows_file_type_and_config() {
        let mut file = TotkFile::default(test_zstd());
        assert_eq!(file.monaco_lang(), "yaml");

        file.file_type = TotkFileType::TagProduct;
        assert_eq!(file.monaco_lang(), "json");

        file.file_type = TotkFileType::Xlink;
        assert_eq!(file.monaco_lang(), "xlink");

        let mut config = TotkConfig::default();
        config.rstb_view = "json".into();
        config.xlink_format = "legacy".into();
        let mut configured = TotkFile::default(Arc::new(TotkZstd::dictionaryless(
            Arc::new(config),
            TOTK_ZSTD_COMPRESSION_LEVEL,
        )));
        configured.file_type = TotkFileType::Restbl;
        assert_eq!(configured.monaco_lang(), "json");
        configured.file_type = TotkFileType::Xlink;
        assert_eq!(configured.monaco_lang(), "yaml");
    }

    #[test]
    fn parses_visual_files_and_folder_like_current_openers() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/1");

        let png_path = root.join("0.png");
        let png = TotkFile::from_binary(&std::fs::read(&png_path).unwrap(), test_zstd(), &png_path)
            .unwrap();
        assert_eq!(png.file_type, TotkFileType::Image);
        assert!(png.cache_3d.image.is_some());
        assert!(png.is_read_only());

        let g1m_path = root.join("231ccec8.g1m");
        let g1m = TotkFile::from_binary(&std::fs::read(&g1m_path).unwrap(), test_zstd(), &g1m_path)
            .unwrap();
        assert_eq!(g1m.file_type, TotkFileType::G1M);
        assert!(g1m.cache_3d.g1m.is_some());

        let folder = TotkFile::from_folder(&root, test_zstd()).unwrap();
        assert_eq!(folder.file_type, TotkFileType::Archive);
        assert_eq!(folder.metadata().split(']').next(), Some("[FOLDER"));
    }

    #[test]
    fn send_data_covers_text_visual_archive_and_audio_tabs() {
        let mut text = TotkFile::default(test_zstd());
        text.file_type = TotkFileType::Text;
        text.path = Pathlib::new("notes.txt");
        text.text = "hello".into();
        let text_data = text.send_data();
        assert_eq!(text_data.tab, "YAML");
        assert_eq!(text_data.lang, "yaml");
        assert_eq!(text_data.text, "hello");

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/1");
        let png_path = root.join("0.png");
        let mut png_file =
            TotkFile::from_binary(&std::fs::read(&png_path).unwrap(), test_zstd(), &png_path)
                .unwrap();
        let png = png_file.send_data();
        assert_eq!(png.tab, "IMAGE");
        assert!(png.read_only);
        assert!(png_file.render_image(0, 0, 0).is_ok());
        assert_eq!(
            png_file.to_binary(None).unwrap(),
            std::fs::read(&png_path).unwrap()
        );

        let mut folder_file = TotkFile::from_folder(&root, test_zstd()).unwrap();
        let folder = folder_file.send_data();
        assert_eq!(folder.tab, "SARC");
        assert_eq!(folder.sarc_paths.file_type, "FOLDER");
        assert!(!folder.sarc_paths.paths.is_empty());

        let mut audio_file =
            TotkFile::from_binary(b"BWAVpayload", test_zstd(), "sound.bwav").unwrap();
        let audio = audio_file.send_data();
        assert_eq!(audio.tab, "AUDIO");
        assert!(audio.read_only);
        assert_eq!(audio_file.to_binary(None).unwrap(), b"BWAVpayload");

        let mut amta_file =
            TotkFile::from_binary(b"AMTApayload", test_zstd(), "sound.amta").unwrap();
        let amta = amta_file.send_data();
        assert_eq!(amta.tab, "AMTA");

        assert!(text.text.is_empty());
        assert!(png_file.binary_raw.is_empty());
        assert!(audio_file.binary_raw.is_empty());
        assert!(amta_file.binary_raw.is_empty());

        let mut internal =
            TotkFile::from_archive_bytes("notes.txt", b"internal", test_zstd()).unwrap();
        assert!(matches!(internal.genesis, FileGenesis::Archive));
        let internal_data = internal.send_data();
        assert_eq!(internal_data.text, "internal");

        let g1m_path = root.join("231ccec8.g1m");
        let g1m_bytes = std::fs::read(&g1m_path).unwrap();
        let mut g1m = TotkFile::from_binary(&g1m_bytes, test_zstd(), &g1m_path).unwrap();
        assert_eq!(g1m.send_data().tab, "3D");
        assert!(g1m.binary_raw.is_empty());
        assert_eq!(g1m.to_binary(None).unwrap(), g1m_bytes);
    }

    #[test]
    fn from_file_reads_disk_file_and_preserves_path() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/1/0.png");
        let file = TotkFile::from_file(&path, test_zstd()).unwrap();
        assert_eq!(file.file_type, TotkFileType::Image);
        assert_eq!(file.path.full_path, path.to_string_lossy());
        assert!(matches!(file.genesis, FileGenesis::Disk));
    }

    #[test]
    fn archive_entries_can_be_mutated_and_rebuilt_in_memory() {
        let mut file = TotkFile::default(test_zstd());
        file.file_type = TotkFileType::Archive;
        file.cache_arc.archive = Some(ArchiveDocument {
            archive: crate::file_format::Archive::RootArchive::Zip(Default::default()),
            path: "memory.zip".into(),
            added: Default::default(),
            modified: Default::default(),
            dictionary: None,
        });
        file.set_entry("totkfile-test.bin", b"test".to_vec())
            .unwrap();
        assert_eq!(
            file.entry_bytes("totkfile-test.bin"),
            Some(b"test".as_slice())
        );
        file.rename_entry("totkfile-test.bin", "totkfile-renamed.bin")
            .unwrap();
        assert_eq!(
            file.entry_bytes("totkfile-renamed.bin"),
            Some(b"test".as_slice())
        );
        assert!(!file.to_binary(None).unwrap().is_empty());
        file.remove_entry("totkfile-renamed.bin").unwrap();
        assert!(file.entry_bytes("totkfile-renamed.bin").is_none());
    }

    #[test]
    fn consumes_legacy_opened_and_internal_file_state() {
        let mut opened = crate::file_format::BinTextFile::OpenedFile::default();
        opened.file_type = TotkFileType::Text;
        opened.path = Pathlib::new("disk.txt");
        let converted = TotkFile::from_opened_file(opened, "disk".into(), test_zstd()).unwrap();
        assert_eq!(converted.text, "disk");
        assert_eq!(converted.path.name, "disk.txt");

        let mut internal = crate::InternalFile::InternalFile::default();
        internal.file_type = TotkFileType::Text;
        internal.path = Pathlib::new("inside.txt");
        let converted =
            TotkFile::from_internal_file(internal, "inside".into(), test_zstd()).unwrap();
        assert!(matches!(converted.genesis, FileGenesis::Archive));
        assert_eq!(converted.text, "inside");
    }

    #[test]
    fn produces_frontend_json_view_models() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/1");
        let png_path = root.join("0.png");
        let png = TotkFile::from_file(&png_path, test_zstd()).unwrap();
        let rendered = png.render_image(0, 0, 0).unwrap();
        let rendered_json = serde_json::to_value(rendered).unwrap();
        assert!(rendered_json["dataUrl"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png"));

        let g1m_path = root.join("231ccec8.g1m");
        let g1m = TotkFile::from_file(&g1m_path, test_zstd()).unwrap();
        let model = g1m.inspect_3d_json().unwrap();
        assert_eq!(model["format"], "G1M");
        assert!(model.get("resolvedTextures").is_some());
        assert!(model.get("textureStats").is_some());
        assert!(g1m.available_g1a_animations().is_empty());

        let plain = TotkFile::default(test_zstd());
        assert!(plain.bphcl_document_json("doc").is_none());
        assert!(plain.bphcl_selectable_nodes("doc").is_empty());
        assert!(plain.hkcl_document_json("doc").is_none());
        assert!(plain.hkcl_selectable_nodes("doc").is_empty());
        assert!(plain.bphhb_document_json("doc").is_none());
    }
}
