#![allow(non_snake_case, non_camel_case_types)]
use roead;
use roead::sarc::{Sarc, SarcWriter};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

//mod Zstd;

use crate::file_format::BinTextFile::BymlFile;
use crate::Open_and_Save::SendData;
use crate::Settings::Magic;
use crate::Settings::{makedirs, Pathlib};
use crate::TotkConfig::TotkConfig;
use crate::Zstd::{sha256, TotkFileType, TotkZstd, ZstdDictionary};

// use super::SarcEntriesData::get_sarc_entries_data;

pub fn get_sarc_entries_data() -> Arc<HashMap<String, String>> {
    crate::utils::LookupData::sarc_sha256()
}

pub struct PackComparer<'a> {
    pub opened: Option<PackFile<'a>>,
    pub vanila: Option<PackFile<'a>>,
    // pub totk_config: Arc<TotkConfig>,
    pub zstd: Arc<TotkZstd<'a>>,
    pub added: HashMap<String, String>,
    pub modded: HashMap<String, String>,
    pub global_sarc_data: Arc<HashMap<String, String>>,
}

#[allow(dead_code)]
impl<'a> PackComparer<'a> {
    pub fn default_new(zstd: Arc<TotkZstd<'a>>) -> Self {
        PackComparer {
            opened: None,
            vanila: None,
            zstd: zstd.clone(),
            added: Default::default(),
            modded: Default::default(),
            global_sarc_data: Arc::default(),
        }
    }
    pub fn set_zstd(&mut self, zstd: Arc<TotkZstd<'a>>) {
        self.zstd = zstd.clone();
        if let Some(opened) = &mut self.opened {
            opened.zstd = zstd.clone();
        }
        if let Some(vanila) = &mut self.vanila {
            vanila.zstd = zstd;
        }
    }

    pub fn open_sarc_bytes<P: AsRef<Path>>(
        path: P,
        rawdata: &[u8],
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(Self, crate::Open_and_Save::SendData)> {
        let mut data = SendData::default();
        let path_ref = path.as_ref();
        print!("Is {:?} a sarc? ", &path_ref.display());
        let pathlib_var = Pathlib::new(path_ref);
        let Ok(mut sarc) = PackFile::from_binary(&rawdata, zstd.clone()) else {
            println!(" no!");
            return None;
        };
        sarc.path = pathlib_var.clone();

        println!(" yes!");
        data.status_text = format!("Opened {}", &path_ref.to_string_lossy().replace("\\", "/"));
        data.path = pathlib_var.clone();
        data.tab = "SARC".to_string();
        // data.file_label = format!("{}{}[SARC]{}", &pathlib_var.name, &sarc., e_s);
        let Some(pack) = PackComparer::from_pack(sarc, zstd.clone()) else {
            println!(" no!");
            return None;
        };
        data.get_sarc_paths(&pack);
        return Some((pack, data));
    }

    pub fn open_sarc<P: AsRef<Path>>(
        path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(Self, crate::Open_and_Save::SendData)> {
        let rawdata = fs::read(path.as_ref()).ok()?;
        return Self::open_sarc_bytes(path.as_ref(), &rawdata, zstd.clone());

        // let mut data = SendData::default();
        // let path_ref = path.as_ref();
        // print!("Is {:?} a sarc? ", &path_ref.display());
        // let pathlib_var = Pathlib::new(path_ref);
        // if let Ok(sarc) = PackFile::new(path_ref, zstd.clone()) {
        //     let e_s = if sarc.endian == roead::Endian::Little {
        //         " [LE]"
        //     } else {
        //         " [BE]"
        //     };
        //     let yaz0_s = if sarc.is_yaz0 { " [Yaz0] " } else { " " };
        //     if let Some(pack) = PackComparer::from_pack(sarc, zstd.clone()) {
        //         println!(" yes!");
        //         data.get_sarc_paths(&pack);
        //         data.status_text =
        //             format!("Opened {}", &path_ref.to_string_lossy().replace("\\", "/"));
        //         data.path = pathlib_var.clone();
        //         data.tab = "SARC".to_string();
        //         data.file_label = format!("{}{}[SARC]{}", &pathlib_var.name, yaz0_s, e_s);
        //         return Some((pack, data));
        //     }
        // }
        // println!(" no");

        // None
    }
    pub fn from_pack(pack: PackFile<'a>, zstd: Arc<TotkZstd<'a>>) -> Option<Self> {
        let config = zstd.clone().totk_config.clone();
        // let vanila = PackComparer::get_vanila_sarc(&pack.path, zstd.clone());
        //Set to None - rely only on global_sarc_data (except for mals files)
        let mut vanila = PackComparer::get_vanila_mals(&pack.path, zstd.clone());
        if vanila.is_none() {
            if let Some(vanila_path) = config.get_pack_path(&pack.path.stem) {
                if let Ok(van) = PackFile::new(vanila_path, zstd.clone()) {
                    vanila = Some(van);
                }
            }
        }
        let mut pack = Self {
            opened: Some(pack),
            vanila: vanila,
            // totk_config: config,
            zstd: zstd.clone(),
            added: HashMap::default(),
            modded: HashMap::default(),
            global_sarc_data: Arc::default(),
        };
        println!("Comparing and reloading");
        if let Err(error) = pack.compare_and_reload() {
            eprintln!("Failed to initialize SARC comparison: {error}");
            return None;
        }
        Some(pack)
    }

    pub fn extract_all_to_folder<P: AsRef<Path>>(&self, dest_path: P) -> io::Result<String> {
        let mut p = PathBuf::from(dest_path.as_ref());
        if let Some(pack) = &self.opened {
            let mut i: i32 = 0;
            let name = pack.path.stem.as_str();
            p.push(name);
            fs::create_dir_all(&p)?;
            for file in pack.sarc.files() {
                if let Some(file_name) = file.name() {
                    let mut file_path = p.clone();
                    file_path.push(file_name);
                    makedirs(&file_path)?;
                    fs::write(&file_path, file.data)?;
                    i += 1;
                }
            }

            return Ok(format!("Extracted {} files to {}", i, p.display()));
        }

        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "No opened pack",
        ));
    }

    pub fn extract_folder_to<P: AsRef<Path>>(
        &mut self,
        source_folder: &str,
        dest_path: P,
    ) -> io::Result<String> {
        let paths = self.get_sarc_paths().paths;
        let opened = self
            .opened
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "No opened pack"))?;
        let prefix = source_folder.trim_matches(['/', '\\']);
        let prefix_with_separator = if prefix.is_empty() {
            String::new()
        } else {
            format!("{}/", prefix.replace('\\', "/"))
        };
        // When extracting a selected directory, make that directory the root of
        // the result instead of recreating the SARC name and its full parent path.
        // Full-archive extraction has its own path and intentionally keeps the
        // existing `<destination>/<sarc name>/...` layout.
        let output_root = if prefix.is_empty() {
            dest_path.as_ref().join(&opened.path.stem)
        } else {
            let folder_name = prefix
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "Invalid SARC folder path")
                })?;
            dest_path.as_ref().join(folder_name)
        };
        let mut count = 0;

        for path in paths {
            let normalized = path.replace('\\', "/");
            if !prefix_with_separator.is_empty()
                && normalized != prefix
                && !normalized.starts_with(&prefix_with_separator)
            {
                continue;
            }
            if let Some(bytes) = opened.writer.get_file(&path) {
                let relative_path = if prefix_with_separator.is_empty() {
                    normalized.as_str()
                } else {
                    normalized
                        .strip_prefix(&prefix_with_separator)
                        .unwrap_or(normalized.as_str())
                };
                let output_path = output_root.join(relative_path);
                makedirs(&output_path)?;
                fs::write(output_path, bytes)?;
                count += 1;
            }
        }
        Ok(format!(
            "Extracted {} files to {}",
            count,
            output_root.display()
        ))
    }

    pub fn get_sarc_paths(&self) -> SarcPaths {
        let mut paths = SarcPaths::default();
        paths.file_type = "SARC".into();
        if let Some(opened) = &self.opened {
            for file in opened.sarc.files() {
                if let Some(name) = file.name {
                    paths.paths.push(name.to_string());
                }
            }
            for (path, _) in self.added.iter() {
                paths.added_paths.push(path.to_string());
            }
            for (path, _) in self.modded.iter() {
                paths.modded_paths.push(path.to_string());
            }
            let size = paths.paths.len();
            if size == paths.added_paths.len() {
                paths.added_paths.clear();
                paths.modded_paths.clear();
            }
        }
        // println!("Paths: {:?}", paths);
        paths
    }

    pub fn compare_and_reload(&mut self) -> io::Result<()> {
        if let Some(opened) = &mut self.opened {
            opened.reload()?;
            opened.self_populate_hashes();
            // println!("hasehs {:?}\nNow to compare", opened.hashes);
        }
        // println!("Comparing, ready...");
        self.compare();
        Ok(())
    }

    pub fn compare(&mut self) {
        println!("Comparing");
        if let Some(opened) = &self.opened {
            let mut is_compared = false;
            if let Some(vanila) = &mut self.vanila {
                //unreachable unless mals
                println!("Comparing vanila actor");
                let mut added: HashMap<String, String> = HashMap::default();
                let mut modded: HashMap<String, String> = HashMap::default();
                vanila.self_populate_hashes();
                for (file, hash) in opened.hashes.iter() {
                    let van_hash = vanila.hashes.get(file);
                    match van_hash {
                        Some(h) => {
                            if h != hash {
                                modded.insert(file.to_string(), hash.to_string());
                            }
                        }
                        None => {
                            added.insert(file.to_string(), hash.to_string());
                        }
                    }
                }
                self.added = added;
                self.modded = modded;
                is_compared = self.added.len() != opened.hashes.keys().len();
                // println!("Added {:?}\nModded {:?}", self.added.keys(), self.modded.keys());
            }
            if !is_compared {
                //custom actor
                println!("Comparing custom actor");
                if self.global_sarc_data.is_empty() {
                    self.global_sarc_data = get_sarc_entries_data();
                }
                let mut added: HashMap<String, String> = HashMap::default();
                let mut modded: HashMap<String, String> = HashMap::default();
                for (file, hash) in opened.hashes.iter() {
                    let van_hash = self.global_sarc_data.get(file);
                    match van_hash {
                        Some(h) => {
                            if h != hash {
                                modded.insert(file.to_string(), hash.to_string());
                            }
                        }
                        None => {
                            added.insert(file.to_string(), hash.to_string());
                        }
                    }
                }
                self.added = added;
                self.modded = modded;
                // println!("Added {:?}\nModded {:?}", self.added, self.modded);
            }
        }
    }

    pub fn get_vanila_mals(path: &Pathlib, zstd: Arc<TotkZstd<'a>>) -> Option<PackFile<'a>> {
        println!("Getting the mals: {}", &path.stem);
        // let versions: Vec<usize> = (100..130).collect();//130, in case new updates are issued in the future for TOTK
        let romfs = &zstd.clone().totk_config.romfs;
        for version in (99..=130).rev() {
            let mut prob_mals_path = PathBuf::from(romfs);
            prob_mals_path.push(format!("Mals/{}.Product.{}.sarc.zs", &path.stem, version));
            if prob_mals_path.exists() {
                if let Ok(pack) =
                    PackFile::new(prob_mals_path.to_string_lossy().to_string(), zstd.clone())
                {
                    println!("Got the mals! Version: {}", version);
                    return Some(pack);
                }
            }
        }

        // let path = zstd.clone().totk_config.clone().get_mals_path(&path.name);
        // if let Some(path) = &path {
        //     match PackFile::new(path.to_string_lossy().to_string(), zstd.clone()) {
        //         Ok(pack) => {
        //             println!("Got the mals!");
        //             return Some(pack);
        //         }
        //         _ => {
        //             return None;
        //         }
        //     }
        // }
        None
    }
    #[allow(dead_code)]
    pub fn get_vanila_sarc(path: &Pathlib, zstd: Arc<TotkZstd<'a>>) -> Option<PackFile<'a>> {
        let pack = PackComparer::get_vanila_pack(path, zstd.clone());
        if pack.is_some() {
            return pack;
        }
        let mals = PackComparer::get_vanila_mals(path, zstd.clone());
        if mals.is_some() {
            return mals;
        }
        None
    }
    #[allow(dead_code)]
    pub fn get_vanila_pack(path: &Pathlib, zstd: Arc<TotkZstd<'a>>) -> Option<PackFile<'a>> {
        println!("Getting the pack: {}", &path.name);
        let path = zstd.clone().totk_config.clone().get_pack_path(&path.stem);
        if let Some(path) = &path {
            match PackFile::new(path.to_string_lossy().to_string(), zstd.clone()) {
                Ok(pack) => {
                    return Some(pack);
                }
                _ => {
                    return None;
                }
            }
        }
        None
    }
}

pub struct PackFile<'a> {
    pub path: Pathlib,
    pub totk_config: Arc<TotkConfig>,
    zstd: Arc<TotkZstd<'a>>,
    pub data: Vec<u8>,
    pub file_type: TotkFileType,
    pub endian: roead::Endian,
    pub writer: SarcWriter,
    pub hashes: HashMap<String, String>,
    pub sarc: Sarc<'a>,
    pub compression: Option<ZstdDictionary>,
    pub yaz0_alignment: u32,
    pub dirty: bool,
}

#[allow(dead_code)]
impl<'a> PackFile<'a> {
    pub fn default(zstd: Arc<TotkZstd<'a>>) -> io::Result<PackFile<'a>> {
        let mut writer = SarcWriter::new(roead::Endian::Little);
        let sarc = Sarc::new(writer.to_binary().clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(PackFile {
            path: Pathlib::default(),
            totk_config: zstd.totk_config.clone(),
            zstd: zstd.clone(),
            data: Default::default(),
            file_type: TotkFileType::Sarc,
            endian: roead::Endian::Little,
            writer: writer,
            hashes: HashMap::default(),
            sarc: sarc,
            compression: None,
            yaz0_alignment: 0,
            dirty: false,
        })
    }

    pub fn new<P: AsRef<Path>>(
        path: P,
        //totk_config: Arc<TotkConfig>,
        zstd: Arc<TotkZstd<'a>>,
        //decompressor: &'a ZstdDecompressor,
        //compressor: &'a ZstdCompressor
    ) -> io::Result<PackFile<'a>> {
        let bytes = fs::read(path)?;
        let pack = Self::from_binary(&bytes, zstd.clone());
        pack
    }

    pub fn rename(&mut self, old_name: &str, new_name: &str) -> io::Result<()> {
        let some_data = self.writer.get_file(old_name).cloned();
        match some_data {
            Some(data) => {
                self.mutate_writer(|writer| {
                    writer.add_file(new_name, data);
                    writer.remove_file(old_name);
                })?;
            }
            None => {
                let e = format!("File {} absent in sarc {}", &old_name, &self.path.full_path);
                return Err(io::Error::new(io::ErrorKind::InvalidInput, e));
            }
        }
        Ok(())
    }

    pub fn mutate_writer(&mut self, operation: impl FnOnce(&mut SarcWriter)) -> io::Result<()> {
        let mut candidate_writer = self.writer.clone();
        operation(&mut candidate_writer);
        let candidate_sarc = Sarc::new(candidate_writer.to_binary())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let candidate_hashes = PackFile::populate_hashes(&candidate_sarc);
        self.writer = candidate_writer;
        self.sarc = candidate_sarc;
        self.hashes = candidate_hashes;
        self.dirty = true;
        Ok(())
    }

    /// Rebuilds this archive from named entries and reapplies the compression
    /// detected when it was opened. Format-specific callers should use this
    /// instead of constructing SARC data or selecting dictionaries themselves.
    pub fn rebuild_binary(
        &self,
        entries: impl IntoIterator<Item = (String, Vec<u8>)>,
    ) -> io::Result<Vec<u8>> {
        let mut writer = SarcWriter::new(self.endian);
        for (path, data) in entries {
            writer.add_file(&path, data);
        }
        let raw = writer.to_binary();
        Sarc::new(raw.clone())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        match self.compression {
            Some(ZstdDictionary::Yaz0) => {
                TotkZstd::compress_yaz0_with_alignment(&raw, self.yaz0_alignment)
            }
            Some(dictionary) => self.zstd.compress_with_dictionary(&raw, dictionary),
            None => Ok(raw),
        }
    }

    /// Rebuilds the current archive while replacing only the named members.
    /// This retains the source SARC writer's hash multiplier and layout policy,
    /// which format-specific archives such as MALS rely on.
    pub fn rebuild_replacing_entries(
        &self,
        replacements: impl IntoIterator<Item = (String, Vec<u8>)>,
    ) -> io::Result<Vec<u8>> {
        let mut writer = self.writer.clone();
        for (path, data) in replacements {
            writer.add_file(&path, data);
        }
        let raw = writer.to_binary();
        Sarc::new(raw.clone())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        match self.compression {
            Some(ZstdDictionary::Yaz0) => {
                TotkZstd::compress_yaz0_with_alignment(&raw, self.yaz0_alignment)
            }
            Some(dictionary) => self.zstd.compress_with_dictionary(&raw, dictionary),
            None => Ok(raw),
        }
    }

    /// Parses one internal BYML entry through TotkBits' BYML document wrapper.
    pub fn byml_file(&self, path: &str) -> io::Result<BymlFile<'a>> {
        let data = self.sarc.get_data(path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("pack entry is missing: {path}"),
            )
        })?;
        BymlFile::from_binary(data, self.zstd.clone(), path)
    }

    pub fn reload(&mut self) -> io::Result<()> {
        let data: Vec<u8> = self.writer.to_binary();
        self.sarc =
            Sarc::new(data).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Ok(())
    }

    pub fn self_populate_hashes(&mut self) {
        self.hashes = PackFile::populate_hashes(&self.sarc);
    }

    pub fn populate_hashes(sarc: &Sarc) -> HashMap<String, String> {
        let mut hashes: HashMap<String, String> = HashMap::default();
        for file in sarc.files() {
            let file_name = file.name.unwrap_or("");
            if !file_name.is_empty() {
                let hash = sha256(file.data().to_vec());
                hashes.insert(file_name.to_string(), hash);
            }
        }
        hashes
    }

    //Save the sarc file, compress if file ends with .zs, create directory if needed
    pub fn save_default(&mut self) -> io::Result<()> {
        let dest_file = self.path.full_path.clone();
        self.save(dest_file)
    }

    pub fn save(&mut self, dest_file: String) -> io::Result<()> {
        if dest_file.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "SARC save path is empty",
            ));
        }
        makedirs(&PathBuf::from(&dest_file))?;
        let raw_data = self.writer.to_binary();
        let should_compress = self.compression.is_some()
            && (!self.totk_config.ask_for_compression
                || rfd::MessageDialog::new()
                    .set_title("Save compression")
                    .set_description(format!(
                        "{} was opened with {:?} compression.\n\nCompress the saved file?",
                        self.path.name, self.compression
                    ))
                    .set_level(rfd::MessageLevel::Info)
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    == rfd::MessageDialogResult::Yes);
        let data = match (should_compress, self.compression) {
            (true, Some(ZstdDictionary::Yaz0)) => {
                TotkZstd::compress_yaz0_with_alignment(&raw_data, self.yaz0_alignment)?
            }
            (true, Some(dictionary)) => {
                self.zstd.compress_with_dictionary(&raw_data, dictionary)?
            }
            _ => raw_data.clone(),
        };
        if data.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "refusing to save an empty SARC",
            ));
        }
        let mut file_handle: fs::File = fs::File::create(&dest_file)?;
        file_handle.write_all(&data)?;
        self.data = raw_data;
        self.path = Pathlib::new(&dest_file);
        self.dirty = false;
        Ok(())
    }

    // pub fn to_senddata(&self) -> SendData {
    //     let mut data = SendData::default();
    //     data.get_sarc_paths(self);
    //     data.status_text =
    //         format!("Opened {}", &path_ref.to_string_lossy().replace("\\", "/"));
    //     data.path = pathlib_var.clone();
    //     data.tab = "SARC".to_string();
    //     data.file_label = format!("{}{}[SARC]{}", &pathlib_var.name, yaz0_s, e_s);

    //     data
    // }

    pub fn from_binary(data: &[u8], zstd: Arc<TotkZstd<'a>>) -> io::Result<PackFile<'a>> {
        let yaz0_alignment = Magic::is_yaz0(data)
            .then(|| TotkZstd::yaz0_alignment(data))
            .unwrap_or_default();
        let (rawdata, compression) = if Magic::is_sarc(&data) {
            (data.to_vec(), ZstdDictionary::None)
        } else if Magic::is_yaz0(&data) {
            (TotkZstd::decompress_yaz0(data)?, ZstdDictionary::Yaz0)
        } else {
            zstd.try_decompress_all_ordered_safe(data, "some_example.pack.zs")
        };
        if !Magic::is_sarc(&rawdata) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ERROR: Not a sarc data",
            ));
        }
        let res_rawdata = rawdata.clone();
        let sarc =
            Sarc::new(rawdata).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let writer = SarcWriter::from_sarc(&sarc);
        let ftype = match compression {
            ZstdDictionary::Pack => TotkFileType::Sarc,
            ZstdDictionary::Zs => TotkFileType::MalsSarc,
            _ => TotkFileType::Sarc,
        };
        Ok(PackFile {
            path: Pathlib::default(),
            totk_config: zstd.clone().totk_config.clone(),
            zstd: zstd.clone(),
            data: res_rawdata,
            file_type: ftype,
            endian: sarc.endian(),
            writer: writer,
            hashes: HashMap::default(),
            sarc,
            compression: (compression != ZstdDictionary::None).then_some(compression),
            yaz0_alignment,
            dirty: false,
        })
    }

    pub fn sarc_file_to_bytes<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let mut f_handle: fs::File = fs::File::open(&path)?;
        let mut buffer: Vec<u8> = Vec::new();
        f_handle.read_to_end(&mut buffer)?;
        if Magic::is_yaz0(&buffer) {
            self.yaz0_alignment = TotkZstd::yaz0_alignment(&buffer);
            if let Ok(dec_data) = TotkZstd::decompress_yaz0(&buffer) {
                buffer = dec_data;
                self.compression = Some(ZstdDictionary::Yaz0);
            }
            if Magic::is_sarc(&buffer) {
                self.data = buffer.clone();
                self.sarc = Sarc::new(buffer)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                return Ok(());
            }
        }
        if path
            .as_ref()
            .to_string_lossy()
            .to_lowercase()
            .ends_with(".zs")
        {
            if let Ok(dec_data) = self.zstd.decompress_pack(&buffer) {
                if Magic::is_sarc(&dec_data) {
                    self.data = dec_data.clone();
                    self.sarc = Sarc::new(dec_data)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                    self.file_type = TotkFileType::Sarc;
                    self.compression = Some(ZstdDictionary::Pack);
                    return Ok(());
                }
            }
            if let Ok(dec_data) = self.zstd.decompressor.decompress_zs(&buffer) {
                if Magic::is_sarc(&dec_data) {
                    self.data = dec_data.clone();
                    self.sarc = Sarc::new(dec_data)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                    self.file_type = TotkFileType::MalsSarc;
                    self.compression = Some(ZstdDictionary::Zs);
                    return Ok(());
                }
            }
        }
        if Magic::is_sarc(&buffer) {
            self.compression = None;
            self.data = buffer.clone();
            self.sarc =
                Sarc::new(buffer).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            return Ok(());
        }

        Err(io::Error::new(
            io::ErrorKind::Other,
            "Invalid data, not a sarc",
        ))
    }
    //Read sarc file's bytes, decompress if needed
    // fn sarc_file_to_bytes1(path: &PathBuf, zstd: &'a TotkZstd) -> Result<(bool, FileData), io::Error> {
    //     let mut is_yaz0 = false;
    //     let mut f_handle: fs::File = fs::File::open(path)?;
    //     let mut buffer: Vec<u8> = Vec::new();
    //     //let mut returned_result: Vec<u8> = Vec::new();
    //     let mut file_data: FileData = FileData::new();
    //     file_data.file_type = TotkFileType::Sarc;
    //     f_handle.read_to_end(&mut buffer)?;

    //     if buffer.starts_with(b"Yaz0") {
    //         if let Ok(dec_data) = roead::yaz0::decompress(&buffer) {
    //             buffer = dec_data;
    //             is_yaz0 = true;
    //         }
    //     }
    //     if is_sarc(&buffer) {
    //         //buffer.as_slice().starts_with(b"SARC") {
    //         file_data.data = buffer;
    //         return Ok((is_yaz0, file_data));
    //     }
    //     match zstd.decompressor.decompress_pack(&buffer) {
    //         Ok(res) => {
    //             if is_sarc(&res) {
    //                 file_data.data = res;
    //             }
    //         }
    //         Err(_) => {
    //             match zstd.decompressor.decompress_zs(&buffer) {
    //                 //try decompressing with other dicts
    //                 Ok(res) => {
    //                     if is_sarc(&res) {
    //                         file_data.data = res;
    //                         file_data.file_type = TotkFileType::MalsSarc;
    //                     }
    //                 }
    //                 Err(err) => {
    //                     eprintln!(
    //                         "Error during zstd decompress {}",
    //                         &path.to_str().unwrap_or("UNKNOWN")
    //                     );
    //                     return Err(err);
    //                 }
    //             }
    //         }
    //     }
    //     if is_sarc(&file_data.data) {
    //         return Ok((is_yaz0, file_data));
    //     }
    //     return Err(io::Error::new(
    //         io::ErrorKind::Other,
    //         "Invalid data, not a sarc",
    //     ));
    // }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SarcPaths {
    pub paths: Vec<String>,
    pub added_paths: Vec<String>,
    pub modded_paths: Vec<String>,
    pub nested_paths: HashMap<String, Vec<String>>,
    pub read_only: bool,
    pub file_type: String,
    #[serde(default)]
    pub root_name: String,
}
impl Default for SarcPaths {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            added_paths: Vec::new(),
            modded_paths: Vec::new(),
            nested_paths: HashMap::new(),
            read_only: false,
            file_type: String::new(),
            root_name: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opened_sarc_remembers_its_disk_save_path() {
        let config = Arc::new(TotkConfig::default());
        let zstd = Arc::new(TotkZstd::dictionaryless(
            config,
            crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
        ));
        let mut writer = SarcWriter::new(roead::Endian::Little);
        writer.add_file("test.txt", b"test".to_vec());
        let bytes = writer.to_binary();
        let destination =
            std::env::temp_dir().join(format!("totkbits-sarc-save-{}.sarc", std::process::id()));

        let (mut comparer, _) =
            PackComparer::open_sarc_bytes(&destination, &bytes, zstd).expect("open SARC bytes");
        let opened = comparer.opened.as_mut().expect("opened SARC");
        assert_eq!(opened.path.full_path, destination.to_string_lossy());
        opened.save_default().expect("save SARC to remembered path");
        assert!(destination.is_file());
        let saved = fs::read(&destination).expect("read saved SARC");
        let _ = fs::remove_file(destination);
        assert!(Magic::is_sarc(&saved));
    }

    #[test]
    fn yaz0_sarc_save_serializes_writer_and_reapplies_compression() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_ss/yaz0/MarioZombie.szs");
        if !source.is_file() {
            return;
        }
        let config = Arc::new(TotkConfig::default());
        let zstd = Arc::new(TotkZstd::dictionaryless(
            config,
            crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
        ));
        let mut pack = PackFile::new(&source, zstd).unwrap();
        let destination =
            std::env::temp_dir().join(format!("totkbits-yaz0-save-as-{}.szs", std::process::id()));

        pack.save(destination.to_string_lossy().into_owned())
            .unwrap();
        let actual = fs::read(&destination).unwrap();
        let _ = fs::remove_file(destination);
        assert!(Magic::is_yaz0(&actual));
        assert_eq!(TotkZstd::yaz0_alignment(&actual), pack.yaz0_alignment);
        assert_eq!(
            TotkZstd::decompress_yaz0(&actual).unwrap(),
            pack.writer.to_binary()
        );
    }
}
