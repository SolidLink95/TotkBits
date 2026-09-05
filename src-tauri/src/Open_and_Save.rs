use crate::Settings::Magic;
use crate::{
    file_format::{
        asb::AsbFile,
        msbt::MsbtFile,
        Ainb::AinbFile,
        BfevFile::BfevFile,
        BinTextFile::{replace_rotate_deg_to_rad, BymlFile, OpenedFile},
        Esetb::Esetb,
        GameDataList::GameDataList,
        Pack::{PackComparer, SarcPaths},
        Rstb::Restbl,
        TagProduct::TagProduct,
        Xlink::Xlink_rs,
        SMO::SmoSaveFile::SmoSaveFile,
    },
    Comparer::DiffComparer,
    InternalFile::InternalFile,
    Settings::Pathlib,
    Zstd::{is_esetb, is_gamedatalist, is_tagproduct, TotkFileType, TotkZstd, ZstdDictionary},
};
use rfd::{FileDialog, MessageDialog};
use roead::{aamp::ParameterIO, byml::Byml};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Write},
    path::Path,
    sync::Arc,
};

pub fn get_string_from_data<P: AsRef<Path>>(
    filepath: P,
    data: Vec<u8>,
    zstd: Arc<TotkZstd>,
) -> Option<(InternalFile, String)> {
    let path = filepath.as_ref().to_string_lossy().into_owned();
    let yaz0_alignment = Magic::is_yaz0(&data)
        .then(|| TotkZstd::yaz0_alignment(&data))
        .unwrap_or_default();
    let (mut internal_file, text) = get_string_from_decoded_data(&path, data, zstd)?;
    internal_file.yaz0_alignment = yaz0_alignment;
    Some((internal_file, text))
}

fn get_string_from_decoded_data<P: AsRef<Path>>(
    filepath: P,
    data: Vec<u8>,
    zstd: Arc<TotkZstd>,
) -> Option<(InternalFile, String)> {
    let mut internal_file = InternalFile::default();
    if data.is_empty() {
        return None;
    }
    let path = filepath.as_ref().to_string_lossy().into_owned();
    let (rawdata, compression) = zstd.try_decompress_all_ordered_safe(&data, &path);
    // .try_decompress_all_ordered(&data, &path)
    // .unwrap_or((data.clone(), ZstdDictionary::None));
    println!("[COMPRESSION] {:?} for file {}", compression, &path);
    let compression = (compression != ZstdDictionary::None).then_some(compression);

    // Archive entries must be dispatched by their full, lower-cased name before
    // generic magic checks. Several specialized TOTK formats are BYML containers.
    if is_tagproduct(&path) {
        if let Some(mut tag) = TagProduct::from_binary(&rawdata, &path, zstd.clone()) {
            internal_file.endian = Some(roead::Endian::Little);
            internal_file.path = Pathlib::new(path.clone());
            internal_file.file_type = TotkFileType::TagProduct;
            let text = tag.to_text();
            internal_file.tag = Some(tag);
            internal_file.compression = compression;
            return Some((internal_file, text));
        }
        println!("Unable to parse {path} as TagProduct; falling back to generic BYML handling");
    }

    if is_esetb(&filepath) {
        if let Ok(esetb) = Esetb::from_binary(&rawdata, zstd.clone()) {
            internal_file.endian = Some(roead::Endian::Little);
            internal_file.path = Pathlib::new(path.clone());
            internal_file.file_type = TotkFileType::Esetb;
            let text = esetb.to_string();
            internal_file.esetb = Some(esetb);
            internal_file.compression = compression;
            return Some((internal_file, text));
        }
    }

    if Magic::is_byml(&rawdata) {
        if let Ok(byml_file) = BymlFile::from_binary(&rawdata, zstd.clone(), path.clone()) {
            let text = byml_file.to_string();
            internal_file.endian = byml_file.endian;
            internal_file.path = Pathlib::new(path.clone());
            internal_file.file_type = byml_file.file_data.file_type;
            internal_file.byml = Some(byml_file);
            internal_file.compression = compression;
            return Some((internal_file, text));
        }
        println!("Unable to parse named BYML archive entry {path}");
        return None;
    }
    if Magic::is_asb(&rawdata) {
        if let Ok(asb) = AsbFile::from_binary(&rawdata) {
            let text = asb.to_yaml().ok()?;
            internal_file.endian = Some(roead::Endian::Little);
            internal_file.path = Pathlib::new(path.clone());
            internal_file.file_type = TotkFileType::ASB;
            internal_file.asb = Some(asb.document);
            internal_file.compression = compression;
            return Some((internal_file, text));
        }
    }

    if Magic::is_ainb(&rawdata) {
        if let Ok(text) = AinbFile::binary_to_text(&rawdata) {
            internal_file.endian = Some(roead::Endian::Little);
            internal_file.path = Pathlib::new(path.clone());
            internal_file.file_type = TotkFileType::AINB;
            internal_file.compression = compression;
            return Some((internal_file, text));
        }
    }
    if Magic::is_evfl(&rawdata) {
        if let Ok(text) = BfevFile::binary_to_text(&rawdata) {
            internal_file.endian = Some(roead::Endian::Little);
            internal_file.path = Pathlib::new(path.clone());
            internal_file.file_type = TotkFileType::Evfl;
            internal_file.compression = compression;
            return Some((internal_file, text));
        }
    }
    if let Some(xlink) = Xlink_rs::open_internal(&path, &rawdata, zstd.clone(), compression) {
        return Some(xlink);
    }

    if Magic::is_aamp(&rawdata) {
        let pio = ParameterIO::from_binary(&rawdata).ok()?;
        let text = crate::file_format::bphcl::safe_aamp_yaml(&pio).ok()?;
        internal_file.endian = None;
        internal_file.path = Pathlib::new(path.clone());
        internal_file.file_type = TotkFileType::Aamp;
        internal_file.compression = compression;
        return Some((internal_file, text));
    }
    if let Some(msbt) = MsbtFile::open_internal(&path, &rawdata, compression) {
        return Some(msbt);
    }
    if let Ok(text) = String::from_utf8(rawdata) {
        internal_file.endian = None;
        internal_file.path = Pathlib::new(path.clone());
        internal_file.file_type = TotkFileType::Text;
        internal_file.compression = compression;
        return Some((internal_file, text));
    }

    None
}

pub fn check_if_save_in_romfs(dest_file: &str, zstd: Arc<TotkZstd>) -> bool {
    if !dest_file.is_empty() {
        //check if file is saved in romfs
        // if dest_file.starts_with(&zstd.totk_config.romfs.to_string_lossy().to_string()) {
        if !&zstd.totk_config.romfs.is_empty() && dest_file.starts_with(&zstd.totk_config.romfs) {
            let m = format!(
                "About to save file:\n{}\nin romfs dump. Continue?",
                &dest_file
            );
            if MessageDialog::new()
                .set_title("Warning")
                .set_description(m)
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                == rfd::MessageDialogResult::Yes
            {
                return true;
            }
        }
    }
    false
}

pub fn get_binary_by_filetype(
    file_type: TotkFileType,
    text: &str,
    endian: roead::Endian,
    zstd: Arc<TotkZstd>,
    file_path: &str,
    opened_file: &mut OpenedFile<'_>,
    compression: Option<ZstdDictionary>,
    yaz0_alignment: u32,
    internal_msbt: Option<&crate::parser::msbt::Msbt>,
    internal_ainb: Option<&crate::parser::ainb::AinbDocument>,
    internal_asb: Option<&crate::parser::asb::Asb>,
    internal_tag: Option<&TagProduct<'_>>,
    is_internal: bool,
) -> Option<Vec<u8>> {
    let mut rawdata: Vec<u8> = Vec::new();
    match file_type {
        TotkFileType::Bphcl => {
            rawdata = opened_file.bphcl.as_ref()?.raw_binary();
        }
        TotkFileType::Hkcl => return None,
        TotkFileType::Bphhb => return None,
        TotkFileType::Hkrg => return None,
        TotkFileType::Bphyssb => return None,
        TotkFileType::Xlink => {
            rawdata = Xlink_rs::text_to_binary(text, file_path, zstd.clone(), None)?;
        }
        TotkFileType::Evfl => {
            rawdata = BfevFile::text_to_binary(text).ok()?;
        }
        TotkFileType::Esetb => {
            if let Some(esetb) = &mut opened_file.esetb {
                esetb.update_from_text(text).ok()?;
                rawdata = esetb.to_binary();
            }
        }
        TotkFileType::ASB => {
            let cached_asb = internal_asb.or(opened_file.asb.as_ref());
            if let Ok(some_data) = AsbFile::text_to_binary(text, Some(opened_file), cached_asb) {
                rawdata = some_data;
            }
        }
        TotkFileType::AINB => {
            let _ = internal_ainb;
            if let Ok(some_data) = AinbFile::text_to_binary(text) {
                rawdata = some_data;
            }
        }
        TotkFileType::TagProduct => {
            let rank_table = if is_internal {
                internal_tag?.rank_table_bytes()
            } else {
                opened_file.tag.as_ref()?.rank_table_bytes()
            };
            if let Ok(some_data) = TagProduct::to_binary(text, rank_table) {
                rawdata = some_data;
            }
        }
        TotkFileType::Byml => {
            if is_gamedatalist(file_path) {
                rawdata = GameDataList::text_to_binary(text)
                    .map_err(|e| println!("Unable to encode GameDataList: {e}"))
                    .ok()?;
            }
            if rawdata.is_empty() {
                let processed_text =
                    if Pathlib::is_banc_path(&file_path) && zstd.totk_config.rotation_deg {
                        &replace_rotate_deg_to_rad(&text)
                    } else {
                        text
                    };

                let pio = Byml::from_text(processed_text).ok()?;
                rawdata = pio.to_binary(endian);
            }
        }
        TotkFileType::Bcett => {
            let processed_text = if zstd.totk_config.rotation_deg {
                &replace_rotate_deg_to_rad(&text)
            } else {
                text
            };
            let pio = Byml::from_text(processed_text).ok()?;
            rawdata = pio.to_binary(endian);
        }
        TotkFileType::Msbt => {
            rawdata = MsbtFile::text_to_binary(
                text,
                file_path,
                internal_msbt.or(opened_file.msyt.as_ref()),
                is_internal,
            )?;
        }
        TotkFileType::Aamp => {
            let pio = crate::file_format::bphcl::aamp_from_yaml(text).ok()?;
            rawdata = pio.to_binary();
        }
        TotkFileType::SmoSaveFile => {
            let mut smo_file = SmoSaveFile::from_string(text, zstd.clone()).ok()?;
            smo_file.endian = endian;
            rawdata = smo_file.to_binary().ok()?;
        }
        TotkFileType::Text => {
            rawdata = text.as_bytes().to_vec();
        }
        _ => {}
    }

    if rawdata.is_empty() {
        return None;
    }
    if let Some(dictionary) = compression {
        rawdata = apply_remembered_compression(&rawdata, &zstd, dictionary, yaz0_alignment).ok()?;
    }
    Some(rawdata)
}

fn apply_remembered_compression(
    data: &[u8],
    zstd: &TotkZstd<'_>,
    compression: ZstdDictionary,
    yaz0_alignment: u32,
) -> io::Result<Vec<u8>> {
    if compression == ZstdDictionary::Yaz0 {
        TotkZstd::compress_yaz0_with_alignment(data, yaz0_alignment)
    } else {
        zstd.compress_with_dictionary(data, compression)
    }
}

pub struct SaveFileDialog<'a> {
    pub tab: String,
    pub pack: &'a Option<PackComparer<'a>>,
    pub opened_file: &'a OpenedFile<'a>,
    pub title: String,
    pub name: Option<String>,
    pub filters: BTreeMap<String, Vec<String>>,
    pub isText: bool,
}
impl SaveFileDialog<'_> {
    pub fn new<'a>(
        tab: String,
        pack: &'a Option<PackComparer<'a>>,
        opened_file: &'a OpenedFile<'a>,
        title: String,
    ) -> SaveFileDialog<'a> {
        SaveFileDialog {
            tab: tab,
            pack: pack,
            opened_file: opened_file,
            title: title,
            name: None,
            filters: Default::default(),
            isText: false,
        }
    }
    pub fn process_name(&mut self) {
        self.name = None;
        match self.tab.as_str() {
            "SARC" => {
                if self.opened_file.bphcl.is_some() {
                    self.name = Some(self.opened_file.path.name.clone());
                } else if let Some(pack) = self.pack {
                    if let Some(opened) = &pack.opened {
                        self.name = Some(opened.path.name.clone());
                    }
                }
            }
            "YAML" => {
                self.name = Some(self.opened_file.path.name.clone());
            }
            _ => {}
        }
    }

    pub fn filters_from_path(&mut self, file_path: &str) {
        let path = Pathlib::new(file_path.to_string());
        let x = if path.ext_last.is_empty() {
            vec![path.extension.clone()]
        } else {
            vec![path.extension.clone(), path.ext_last.clone()]
        };
        let y = if path.ext_last.is_empty() {
            path.extension.clone().to_uppercase()
        } else {
            path.ext_last.clone().to_uppercase()
        };

        self.filters.insert(y, x);
    }

    pub fn generate_filters(&mut self) {
        let mut filters: BTreeMap<String, Vec<String>> = BTreeMap::new();
        match self.tab.as_str() {
            "SARC" => {
                if self.opened_file.bphcl.is_some() {
                    filters.insert("BPHCL".to_string(), vec!["bphcl".to_string()]);
                } else {
                    filters.insert(
                        "SARC".to_string(),
                        vec![
                            "pack".to_string(),
                            "sarc".to_string(),
                            "pack.zs".to_string(),
                            "sarc.zs".to_string(),
                        ],
                    );
                }
            }
            "YAML" => {
                let exts = if self.opened_file.path.ext_last.is_empty() {
                    vec![self.opened_file.path.extension.clone()]
                } else {
                    vec![
                        self.opened_file.path.extension.clone(),
                        self.opened_file.path.ext_last.clone(),
                    ]
                };
                filters.insert(
                    //own extension
                    format!("{:?}", self.opened_file.file_type),
                    exts,
                );
                filters.insert(
                    "Text Files".to_string(),
                    vec![
                        "yaml".to_string(),
                        "json".to_string(),
                        "yml".to_string(),
                        "txt".to_string(),
                    ],
                );
            }
            _ => {} // Add a wildcard pattern to cover all other cases
        }
        // filters.insert("All Files".to_string(), vec!["*".to_string()]);
        self.filters = filters;
    }
    pub fn generate_filters_and_name(&mut self) {
        self.generate_filters();
        self.process_name();
    }

    pub fn show(&mut self) -> String {
        // self.generate_filters();
        // self.process_name();
        let mut result = String::new();
        let mut dialog = FileDialog::new()
            .set_file_name(self.name.clone().unwrap_or_default())
            .set_title(&self.title);
        for (key, value) in &self.filters {
            dialog = dialog.add_filter(key, value);
        }
        let file = dialog
            .add_filter("All files", &vec!["*".to_string()])
            .save_file();
        if let Some(res) = file {
            result = res.to_string_lossy().into_owned();
        }
        self.isText = vec![".txt", ".yaml", ".json", ".yml"]
            .iter()
            .any(|ext| result.to_lowercase().ends_with(ext));
        result.replace("\\", "/")
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SendData {
    pub text: String,
    pub path: Pathlib,
    pub file_label: String,
    pub file_metadata: String,
    pub file_type: TotkFileType,
    pub status_text: String,
    pub tab: String,
    pub rstb_paths: Vec<serde_json::Value>,
    pub sarc_paths: SarcPaths,
    pub lang: String,
    pub compare_data: DiffComparer,
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_modded_path: Option<String>,
}

impl Default for SendData {
    fn default() -> Self {
        Self {
            text: "".to_string(),
            path: Pathlib::default(),
            file_label: "".to_string(),
            file_metadata: "".to_string(),
            file_type: TotkFileType::None,
            status_text: "".to_string(),
            tab: "YAML".to_string(),
            rstb_paths: Vec::default(),
            sarc_paths: SarcPaths::default(),
            lang: "yaml".to_string(),
            compare_data: DiffComparer::default(),
            read_only: false,
            parent_modded_path: None,
        }
    }
}
impl SendData {
    pub fn get_file_label(&mut self, filetype: TotkFileType, endian: Option<roead::Endian>) {
        self.set_file_metadata(filetype, None);
        let mut e = String::new();
        if let Some(endian) = endian {
            e = match endian {
                roead::Endian::Big => "BE".to_string(),
                roead::Endian::Little => "LE".to_string(),
            };
        }
        if !e.is_empty() {
            self.file_label = format!("{} [{:?}] [{}]", self.path.name, filetype, e)
        } else {
            self.file_label = format!("{} [{:?}]", self.path.name, filetype)
        }
    }

    pub fn set_file_metadata(
        &mut self,
        filetype: TotkFileType,
        dictionary: Option<ZstdDictionary>,
    ) {
        self.file_type = filetype;
        self.file_metadata = match dictionary {
            Some(ZstdDictionary::Yaz0) => format!("[{filetype:?}] [Yaz0]"),
            Some(ZstdDictionary::None) => format!("[{filetype:?}]"),
            Some(dictionary) => format!("[{filetype:?}] [ZSTD: {dictionary:?}]"),
            None => format!("[{filetype:?}]"),
        };
    }
    pub fn get_sarc_paths(&mut self, pack: &PackComparer<'_>) {
        if let Some(opened) = &pack.opened {
            for file in opened.sarc.files() {
                if let Some(name) = file.name {
                    self.sarc_paths.paths.push(name.into());
                }
            }
            for (path, _) in pack.added.iter() {
                self.sarc_paths.added_paths.push(path.into());
            }
            for (path, _) in pack.modded.iter() {
                self.sarc_paths.modded_paths.push(path.into());
            }
            self.sarc_paths
                .paths
                .sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

            if self.sarc_paths.paths.len() == self.sarc_paths.added_paths.len() {
                self.sarc_paths.added_paths.clear(); //avoid all files be lit blue as added
                self.sarc_paths.modded_paths.clear(); //redundant
                return; //skip sorting empty lists
            }

            self.sarc_paths
                .added_paths
                .sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
            self.sarc_paths
                .modded_paths
                .sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        }
        //println!("Sarc paths: {:?}", self.sarc_paths);
    }
}

pub fn open_file_from_disk_name_guess<P: AsRef<Path>>(
    path: P,
    zstd: Arc<TotkZstd>,
) -> Option<(OpenedFile, SendData)> {
    let path = path.as_ref();
    let file_name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    let uncompressed_name = file_name.strip_suffix(".zs").unwrap_or(&file_name);

    if !zstd.totk_config.mii_renderer && crate::tools::mii::is_mii_binary_path(path) {
        let mut data = SendData::default();
        data.path = Pathlib::new(path);
        data.tab = "ERROR".into();
        data.status_text = "Error: Failed to parse file".into();
        data.text = data.status_text.clone();
        return Some((OpenedFile::default(), data));
    }

    let mii_result = if uncompressed_name.ends_with(".glb") || !zstd.totk_config.mii_renderer {
        None
    } else {
        crate::tools::mii::open(path)
    };
    let result = if uncompressed_name.ends_with(".glb") {
        crate::tools::mii::open_glb(path).ok()
    } else if let Some(result) = mii_result {
        Some(result)
    } else if file_name.starts_with("tag.product") {
        TagProduct::open_tag(path, zstd)
    } else if file_name.ends_with(".belnk") || file_name.ends_with(".belnk.zs") {
        Xlink_rs::open_xlink(path, zstd)
    } else if file_name.ends_with(".esetb.byml") || file_name.ends_with(".esetb.byml.zs") {
        Esetb::open_esetb(path, zstd)
    } else if uncompressed_name.ends_with(".rsizetable") {
        Restbl::open_restbl(path, zstd)
    } else if Pathlib::is_banc_path(&uncompressed_name) || Pathlib::is_byml_path(&uncompressed_name)
    {
        BymlFile::open_byml(path, zstd)
    } else if uncompressed_name.ends_with(".asb") {
        AsbFile::open_asb(path, zstd)
    } else if uncompressed_name.ends_with(".ainb") {
        AinbFile::open_ainb(path, zstd)
    } else if uncompressed_name.ends_with(".bfevfl") {
        BfevFile::open_bfev(path, zstd)
    } else if uncompressed_name.ends_with(".msbt") || uncompressed_name.ends_with(".msyt") {
        MsbtFile::open_mstb(path)
    } else if uncompressed_name.ends_with(".bphcl") {
        crate::file_format::bphcl::BphclFile::open(path)
    } else if uncompressed_name.ends_with(".hkcl") {
        crate::file_format::hkcl::HkclFile::open(path)
    } else if uncompressed_name.ends_with(".bphhb") {
        crate::file_format::bphhb::BphhbFile::open(path)
    } else if uncompressed_name.ends_with(".bphyssb") {
        crate::file_format::bphyssb::BphyssbFile::open(path)
    } else if uncompressed_name.ends_with(".hkrg") {
        crate::file_format::hkrg::HkrgFile::open(path)
    } else if uncompressed_name.ends_with(".bfres") || file_name.ends_with(".bfres.mc") {
        crate::file_format::Model3D::bfres::BfresFile::open(path, zstd)
    } else if uncompressed_name.ends_with(".g1m") {
        crate::parser::AOC::g1m::G1mFile::open(path)
    } else if uncompressed_name.ends_with(".g1t") {
        crate::file_format::Image::ImageDocument::open(path, &zstd)
    } else if uncompressed_name.ends_with(".fbx") {
        crate::parser::fbx::FbxFile::open(path)
    } else if uncompressed_name.ends_with(".bntx")
        || uncompressed_name.ends_with(".dds")
        || uncompressed_name.ends_with(".png")
        || uncompressed_name.ends_with(".jpg")
        || uncompressed_name.ends_with(".jpeg")
        || uncompressed_name.ends_with(".tga")
        || uncompressed_name.ends_with(".bmp")
    {
        crate::file_format::Image::ImageDocument::open(path, &zstd)
    } else if uncompressed_name.ends_with(".baamp")
        || uncompressed_name.ends_with(".bparam")
        || uncompressed_name.ends_with(".aamp")
    {
        crate::file_format::SimpleOpeners::AampFile::open_aamp(path)
    } else if uncompressed_name.ends_with(".byml")
        || uncompressed_name.ends_with(".bgyml")
        || uncompressed_name.ends_with(".sbyml")
        || uncompressed_name.starts_with("gamedatalist")
    {
        BymlFile::open_byml(path, zstd)
    } else if uncompressed_name.ends_with(".yaml")
        || uncompressed_name.ends_with(".yml")
        || uncompressed_name.ends_with(".json")
        || uncompressed_name.ends_with(".txt")
    {
        crate::file_format::SimpleOpeners::TextFile::open_text(path)
    } else {
        None
    };

    if let Some((opened_file, _)) = &result {
        println!(
            "[GUESS] [OPEN] {} type: {:?}",
            opened_file.path.full_path, opened_file.file_type
        );
    } else {
        println!("[GUESS] [OPEN] FAILED TO OPEN {}", path.display());
    }

    result
}

pub fn file_from_bytes_to_senddata<'a>(
    path_hint: &str,
    bytes: &[u8],
    zstd: Arc<TotkZstd<'a>>,
) -> Option<(OpenedFile<'a>, SendData)> {
    let (mut opened, data) = file_from_bytes_name_guess(path_hint, bytes, zstd)?;
    if matches!(data.tab.as_str(), "IMAGE" | "3D") {
        if opened.visual_data.is_none() {
            opened.visual_data = Some(bytes.to_vec());
        }
    }
    Some((opened, data))
}

fn file_from_bytes_name_guess<'a>(
    path_hint: &str,
    bytes: &[u8],
    zstd: Arc<TotkZstd<'a>>,
) -> Option<(OpenedFile<'a>, SendData)> {
    let file_path = Path::new(path_hint);
    let file_name = file_path
        .file_name()?
        .to_string_lossy()
        .to_ascii_lowercase();
    let uncompressed_name = file_name.strip_suffix(".zs").unwrap_or(&file_name);
    let path_ref = file_path;

    if zstd.totk_config.mii_renderer {
        if let Some(result) = crate::tools::mii::open_binary(path_ref, bytes) {
            return Some(result);
        }
    }

    if is_tagproduct(path_ref) {
        return TagProduct::open_tag_binary(bytes, path_ref, zstd);
    }
    if file_name.ends_with(".belnk") || file_name.ends_with(".belnk.zs") {
        if let Some(result) = Xlink_rs::open_xlink_binary(bytes, path_ref, zstd.clone()) {
            return Some(result);
        }
    }
    if file_name.ends_with(".esetb.byml") || file_name.ends_with(".esetb.byml.zs") {
        if let Some(result) = Esetb::open_esetb_binary(bytes, path_ref, zstd.clone()) {
            return Some(result);
        }
    }
    if uncompressed_name.ends_with(".rsizetable") || file_name.ends_with(".rsizetable.byml") {
        if let Some(result) = Restbl::open_restbl_binary(bytes, path_ref, zstd.clone()) {
            return Some(result);
        }
    }
    if Pathlib::is_banc_path(&uncompressed_name) || Pathlib::is_byml_path(&uncompressed_name) {
        if let Some(result) = BymlFile::open_byml_binary(bytes, path_ref, zstd.clone()) {
            return Some(result);
        }
    }
    if uncompressed_name.ends_with(".asb") {
        if let Some(result) = AsbFile::open_asb_binary(bytes, path_ref, zstd.clone()) {
            return Some(result);
        }
    }
    if uncompressed_name.ends_with(".ainb") {
        if let Some(result) = AinbFile::open_ainb_binary(bytes, path_ref, zstd.clone()) {
            return Some(result);
        }
    }
    if uncompressed_name.ends_with(".bfevfl") {
        if let Some(result) = BfevFile::open_bfev_binary(bytes, path_ref, zstd.clone()) {
            return Some(result);
        }
    }
    if uncompressed_name.ends_with(".msbt") || uncompressed_name.ends_with(".msyt") {
        if let Some(result) = MsbtFile::open_mstb_binary(bytes, path_ref, zstd.clone()) {
            return Some(result);
        }
    }
    if uncompressed_name.ends_with(".bfres") || file_name.ends_with(".bfres.mc") {
        if let Some(result) = crate::file_format::Model3D::bfres::BfresFile::open_binary(
            bytes,
            path_ref,
            zstd.clone(),
        ) {
            return Some(result);
        }
    }
    if uncompressed_name.ends_with(".g1m") {
        if let Some(result) = crate::parser::AOC::g1m::G1mFile::open_binary(path_ref, bytes) {
            return Some(result);
        }
    }
    if uncompressed_name.ends_with(".g1t") {
        if let Some(result) =
            crate::file_format::Image::ImageDocument::open_binary(bytes, path_ref, &zstd)
        {
            return Some(result);
        }
    }
    if uncompressed_name.ends_with(".fbx") {
        if let Some(result) = crate::parser::fbx::FbxFile::open_binary(path_ref, bytes) {
            return Some(result);
        }
    }
    if let Some(result) =
        crate::file_format::Image::ImageDocument::open_binary(bytes, path_ref, &zstd)
    {
        return Some(result);
    }
    if let Some(result) =
        crate::file_format::SimpleOpeners::AampFile::open_aamp_binary(bytes, path_ref, zstd.clone())
    {
        return Some(result);
    }
    if let Some(result) = SmoSaveFile::open_smo_save_file_binary(bytes, path_ref, zstd.clone()) {
        return Some(result);
    }
    if let Some(result) =
        crate::file_format::SimpleOpeners::TextFile::open_text_binary(bytes, path_ref, zstd.clone())
    {
        return Some(result);
    }
    None
}

#[cfg(test)]
mod remembered_compression_tests {
    use super::*;

    #[test]
    fn send_data_serializes_file_type_as_uppercase() {
        let mut data = SendData::default();
        data.set_file_metadata(TotkFileType::TagProduct, None);
        let json = serde_json::to_value(data).unwrap();
        assert_eq!(json["file_type"], "TAGPRODUCT");
    }

    #[test]
    fn disabled_mii_open_returns_only_generic_parse_error() {
        let app = crate::TotkApp::TotkBitsApp::default();
        let (opened, data) =
            file_from_disk_to_senddata(Path::new("private-name.charinfo"), app.zstd.clone())
                .unwrap();
        assert_eq!(opened.file_type, TotkFileType::None);
        assert_eq!(data.tab, "ERROR");
        assert_eq!(data.status_text, "Error: Failed to parse file");
        assert!(!data.status_text.to_ascii_lowercase().contains("mii"));
        assert!(!data.status_text.contains("private-name"));
    }

    #[test]
    fn reapplies_yaz0_with_remembered_alignment() {
        let app = crate::TotkApp::TotkBitsApp::default();
        let raw = b"YB\x07\x00remembered compression";
        let compressed =
            apply_remembered_compression(raw, &app.zstd, ZstdDictionary::Yaz0, 0x80).unwrap();

        assert!(Magic::is_yaz0(&compressed));
        assert_eq!(TotkZstd::yaz0_alignment(&compressed), 0x80);
        assert_eq!(TotkZstd::decompress_yaz0(&compressed).unwrap(), raw);
    }

    #[test]
    fn opens_png_and_g1m_archive_children_from_memory() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/1");
        let app = crate::TotkApp::TotkBitsApp::default();

        let png = std::fs::read(root.join("0.png")).unwrap();
        let (png_opened, png_data) =
            file_from_bytes_to_senddata("0.png", &png, app.zstd.clone()).unwrap();
        assert_eq!(png_data.tab, "IMAGE");
        assert_eq!(png_opened.visual_data.as_deref(), Some(png.as_slice()));
        crate::file_format::Image::ImageDocument::render_bytes_selection_with_zstd(
            png_opened.visual_data.as_deref().unwrap(),
            "0.png",
            0,
            0,
            0,
            Some(&app.zstd),
        )
        .unwrap();

        let g1m = std::fs::read(root.join("231ccec8.g1m")).unwrap();
        let (g1m_opened, g1m_data) =
            file_from_bytes_to_senddata("231ccec8.g1m", &g1m, app.zstd.clone()).unwrap();
        assert_eq!(g1m_data.tab, "3D");
        assert_eq!(g1m_opened.visual_data.as_deref(), Some(g1m.as_slice()));
        crate::parser::AOC::g1m::G1mFile::parse(&g1m, "231ccec8").unwrap();
    }
}

#[cfg(test)]
mod disk_open_router_tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/open_router")
    }

    /// The fallback router must identify files by content when the extension
    /// says nothing, reading the file once instead of once per opener.
    #[test]
    fn fallback_chain_identifies_disguised_files_by_content() {
        let app = crate::TotkApp::TotkBitsApp::default();
        let dir = fixture_dir();
        // The compressed fixtures require the TOTK dictionaries.
        if !dir.is_dir() || app.zstd.decompressor.bcett.is_none() {
            return;
        }

        let (opened, data) =
            file_from_disk_to_senddata(dir.join("disguised_bcett.bin"), app.zstd.clone()).unwrap();
        assert_eq!(data.tab, "YAML");
        assert_eq!(opened.file_type, TotkFileType::Bcett);
        assert!(!data.text.is_empty());

        let (opened, data) =
            file_from_disk_to_senddata(dir.join("disguised_ainb.bin"), app.zstd.clone()).unwrap();
        assert_eq!(data.tab, "YAML");
        assert_eq!(opened.file_type, TotkFileType::AINB);
        assert!(!data.text.is_empty());

        let (opened, data) =
            file_from_disk_to_senddata(dir.join("disguised_bfev.bin"), app.zstd.clone()).unwrap();
        assert_eq!(data.tab, "YAML");
        assert_eq!(opened.file_type, TotkFileType::Evfl);
        assert_eq!(data.lang, "json");
        assert!(!data.text.is_empty());

        let (opened, data) =
            file_from_disk_to_senddata(dir.join("disguised_text.bin"), app.zstd.clone()).unwrap();
        assert_eq!(opened.file_type, TotkFileType::Text);
        assert!(data.text.contains("plain text disguised as binary"));
    }

    #[test]
    fn extension_router_still_handles_recognized_names() {
        let app = crate::TotkApp::TotkBitsApp::default();
        let source = fixture_dir().join("Eldin_SkyIsland01_Dynamic.bcett.byml.zs");
        if !source.is_file() || app.zstd.decompressor.bcett.is_none() {
            return;
        }
        let (opened, data) = file_from_disk_to_senddata(&source, app.zstd.clone()).unwrap();
        assert_eq!(data.tab, "YAML");
        assert_eq!(opened.file_type, TotkFileType::Bcett);
        assert!(!data.text.is_empty());
    }

    #[test]
    fn missing_file_fails_without_opener_probing() {
        let app = crate::TotkApp::TotkBitsApp::default();
        assert!(file_from_disk_to_senddata(
            fixture_dir().join("does_not_exist.bin"),
            app.zstd.clone()
        )
        .is_none());
    }
}

pub fn file_from_disk_to_senddata<P: AsRef<Path>>(
    path: P,
    zstd: Arc<TotkZstd>,
) -> Option<(OpenedFile, SendData)> {
    let file_name = path.as_ref(); //.to_string_lossy().to_string().replace("\\", "/");
    println!(
        "[OPEN ROUTER] beginning disk opener chain for {} (RSTB follows Xlink, TagProduct, and Esetb)",
        file_name.display()
    );
    open_file_from_disk_name_guess(file_name, zstd.clone())
        .or_else(|| file_from_disk_content_guess(file_name, zstd.clone()))
        .map(|(opened_file, data)| {
            println!(
                "[OPEN ROUTER] opener chain accepted {} as {:?}",
                file_name.display(),
                opened_file.file_type
            );
            (opened_file, data)
        })
}

/// Fallback for files the extension router rejects. The file is read and
/// decompressed once up front; each opener then runs only when its magic bytes
/// (or file name, for name-bound formats) say it can succeed, instead of every
/// opener repeating its own disk read and decompression. Openers with
/// disk-only behavior (ASB's BAEV prompt, BYML's full dictionary handling,
/// G1M's AOC display names, images, BPHCL) reread the file, but only after
/// their magic matched.
fn file_from_disk_content_guess<'a>(
    file_name: &Path,
    zstd: Arc<TotkZstd<'a>>,
) -> Option<(OpenedFile<'a>, SendData)> {
    let bytes = std::fs::read(file_name).ok()?;
    // Single decompression probe shared by the magic gates below; for
    // uncompressed files this is the raw content.
    let (probe, _) = zstd.try_decompress_all_ordered_safe(&bytes, file_name);

    if Magic::is_xlink(&probe) {
        if let Some(result) = Xlink_rs::open_xlink_binary(&bytes, file_name, zstd.clone()) {
            return Some(result);
        }
    }
    // Name-bound openers below reject non-matching files before any disk I/O.
    if let Some(result) = TagProduct::open_tag(file_name, zstd.clone()) {
        return Some(result);
    }
    if let Some(result) = Esetb::open_esetb(file_name, zstd.clone()) {
        return Some(result);
    }
    if let Some(result) = Restbl::open_restbl(file_name, zstd.clone()) {
        return Some(result);
    }
    if Magic::is_asb(&probe) {
        // The disk opener also prompts for the optional companion BAEV file.
        if let Some(result) = AsbFile::open_asb(file_name, zstd.clone()) {
            return Some(result);
        }
    }
    if Magic::is_ainb(&probe) {
        if let Some(result) = AinbFile::open_ainb_binary(&bytes, file_name, zstd.clone()) {
            return Some(result);
        }
    }
    if Magic::is_byml(&probe) {
        // The disk opener resolves every TOTK dictionary and the Bcett file
        // type; the in-memory BYML opener only knows the Zs dictionary.
        if let Some(result) = BymlFile::open_byml(file_name, zstd.clone()) {
            return Some(result);
        }
    }
    if let Some(result) = MsbtFile::open_mstb_binary(&bytes, file_name, zstd.clone()) {
        return Some(result);
    }
    if Magic::is_bfres(&probe) {
        if let Some(result) = crate::file_format::Model3D::bfres::BfresFile::open_binary(
            &bytes,
            file_name,
            zstd.clone(),
        ) {
            return Some(result);
        }
    }
    if Magic::is_g1m(&bytes) {
        // The disk opener resolves AOC display names for the tab label.
        if let Some(result) = crate::parser::AOC::g1m::G1mFile::open(file_name) {
            return Some(result);
        }
    }
    if let Some(result) = crate::parser::fbx::FbxFile::open_binary(file_name, &bytes) {
        return Some(result);
    }
    // Structured formats must run before the generic image detector. In
    // particular, many BFEVFL files use the shared `.zs` compression suffix.
    if Magic::is_evfl(&probe) {
        if let Some(result) = BfevFile::open_bfev_binary(&bytes, file_name, zstd.clone()) {
            return Some(result);
        }
    }
    if crate::file_format::Image::ImageDocument::supports(file_name, &bytes, Some(zstd.as_ref())) {
        if let Some(result) = crate::file_format::Image::ImageDocument::open(file_name, &zstd) {
            return Some(result);
        }
    }
    if Magic::is_bphcl(&bytes) {
        if let Some(result) = crate::file_format::bphcl::BphclFile::open(file_name) {
            return Some(result);
        }
    }
    if let Some(result) = crate::file_format::hkrg::HkrgFile::open(file_name) {
        return Some(result);
    }
    if let Some(result) = crate::file_format::hkcl::HkclFile::open(file_name) {
        return Some(result);
    }
    if let Some(result) = crate::file_format::bphhb::BphhbFile::open(file_name) {
        return Some(result);
    }
    if let Some(result) = crate::file_format::bphyssb::BphyssbFile::open(file_name) {
        return Some(result);
    }
    if let Some(result) = crate::file_format::SimpleOpeners::AampFile::open_aamp_binary(
        &bytes,
        file_name,
        zstd.clone(),
    ) {
        return Some(result);
    }
    if let Some(result) = SmoSaveFile::open_smo_save_file_binary(&bytes, file_name, zstd.clone()) {
        return Some(result);
    }
    crate::file_format::SimpleOpeners::TextFile::open_text_binary(&bytes, file_name, zstd)
}
