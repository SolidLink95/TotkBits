#![allow(non_snake_case, non_camel_case_types)]
use super::Esetb::Esetb;
use super::Rstb::Restbl;
use crate::file_format::TagProduct::TagProduct;
use crate::parser::msbt::Msbt;
use crate::Open_and_Save::SendData;
use crate::Settings::Magic;
use crate::Settings::Pathlib;
use crate::Zstd::{is_gamedatalist, TotkFileType, TotkZstd, ZstdDictionary};
use regex::Regex;
use roead::byml::Byml;
use std::any::type_name;
use std::fs::OpenOptions;
use std::io::{BufWriter, Cursor, Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::{fs, io};

// const FLOAT_PRECISION: i32 = 5;

#[derive(Debug)]
pub struct FileData {
    pub file_type: TotkFileType,
    pub data: Vec<u8>,
    pub compression: Option<ZstdDictionary>,
    pub yaz0_alignment: u32,
}

impl FileData {
    pub fn new() -> Self {
        Self {
            file_type: TotkFileType::None,
            data: Vec::new(),
            compression: None,
            yaz0_alignment: 0,
        }
    }
}

pub struct BymlFile<'a> {
    pub endian: Option<roead::Endian>,
    pub file_data: FileData,
    pub path: Pathlib,
    pub pio: roead::byml::Byml,
    pub zstd: Arc<TotkZstd<'a>>,
    pub file_type: TotkFileType,
}

#[allow(dead_code, unused_variables, unused_assignments)]
impl<'a> BymlFile<'_> {
    pub fn open_byml<P: AsRef<Path>>(
        path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(OpenedFile<'a>, crate::Open_and_Save::SendData)> {
        let mut opened_file = OpenedFile::default();
        let mut data = SendData::default();
        let path_ref = path.as_ref();
        let pathlib_var = Pathlib::new(path_ref);
        print!("Is {} a byml? ", &pathlib_var.full_path);
        opened_file.byml = BymlFile::new(path_ref, zstd.clone());
        // if opened_file.byml.is_some() {
        if let Some(b) = &opened_file.byml {
            // let b = opened_file.byml.as_ref().unwrap();
            let gamedatalist = if is_gamedatalist(path_ref) {
                "(GameDataList) "
            } else {
                ""
            };
            println!("yes {}!", gamedatalist);
            opened_file.path = pathlib_var.clone();
            opened_file.endian = b.endian;
            opened_file.file_type = b.file_data.file_type.clone();
            data.status_text = format!("Opened {}", &pathlib_var.full_path);
            data.path = pathlib_var;
            // data.text = Byml::to_text(&b.pio);
            data.text = b.to_string();
            data.get_file_label(b.file_data.file_type, b.endian);
            return Some((opened_file, data));
        }
        println!(" no");
        None
    }

    pub fn open_byml_binary<P: AsRef<Path>>(
        binary: &[u8],
        display_path: P,
        zstd: Arc<TotkZstd<'a>>,
    ) -> Option<(OpenedFile<'a>, SendData)> {
        let path = display_path.as_ref();
        let file_data = Self::byml_data_to_bytes(&binary.to_vec(), zstd.clone()).ok()?;
        let mut byml = Self::from_binary(&file_data.data, zstd, path).ok()?;
        byml.file_type = file_data.file_type;
        byml.file_data = file_data;
        let endian = byml.endian;
        let file_type = byml.file_data.file_type;
        let text = byml.to_string();
        let mut opened = OpenedFile::default();
        opened.path = Pathlib::new(path);
        opened.endian = endian;
        opened.file_type = file_type;
        opened.byml = Some(byml);
        let mut data = SendData::default();
        data.path = Pathlib::new(path);
        data.text = text;
        data.status_text = format!("Opened {}", path.display());
        data.get_file_label(file_type, endian);
        Some((opened, data))
    }
}

#[allow(dead_code, unused_variables, unused_assignments)]
impl<'a> BymlFile<'_> {
    pub fn new<P: AsRef<Path>>(path: P, zstd: Arc<TotkZstd<'a>>) -> Option<BymlFile<'a>> {
        let file_data = BymlFile::byml_file_to_bytes(path.as_ref(), zstd.clone()).ok()?;
        let mut byml = BymlFile::from_binary(&file_data.data, zstd, path.as_ref()).ok()?;
        byml.file_type = file_data.file_type;
        byml.file_data = file_data;
        Some(byml)
    }

    pub fn save(&self, path: String) -> io::Result<()> {
        let mut data = self.to_binary_preserving_header()?;
        if let Some(compression) = self.file_data.compression {
            data = if compression == ZstdDictionary::Yaz0 {
                TotkZstd::compress_yaz0_with_alignment(&data, self.file_data.yaz0_alignment)?
            } else {
                self.zstd.compress_with_dictionary(&data, compression)?
            };
        }
        //f_handle.write_all(&data);
        bytes_to_file(data, &path)?;
        Ok(())
    }

    /// Serializes the edited document using the endian and BYML version of the
    /// source file. RSDB and other versioned tables must not silently fall back
    /// to roead's default version.
    pub fn to_binary_preserving_header(&self) -> io::Result<Vec<u8>> {
        let endian = self.endian.unwrap_or(roead::Endian::Little);
        let version = self
            .file_data
            .data
            .get(2..4)
            .and_then(|bytes| <&[u8; 2]>::try_from(bytes).ok())
            .map(|bytes| match endian {
                roead::Endian::Little => u16::from_le_bytes(*bytes),
                roead::Endian::Big => u16::from_be_bytes(*bytes),
            })
            .unwrap_or(4);
        let mut data = Vec::new();
        self.pio
            .write(&mut Cursor::new(&mut data), endian, version.min(4))
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("BYML serialization failed: {error}"),
                )
            })?;
        let version_bytes = match endian {
            roead::Endian::Little => version.to_le_bytes(),
            roead::Endian::Big => version.to_be_bytes(),
        };
        let header = data.get_mut(2..4).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "serialized BYML header is truncated",
            )
        })?;
        header.copy_from_slice(&version_bytes);
        Ok(data)
    }

    pub fn from_text(content: &str, zstd: Arc<TotkZstd<'a>>) -> io::Result<BymlFile<'a>> {
        let pio: Result<Byml, roead::Error> = Byml::from_text(&content);
        match pio {
            Ok(ok_pio) => Ok(BymlFile {
                endian: Some(roead::Endian::Little), //TODO: add Big endian support
                file_data: FileData::new(),
                path: Pathlib::default(),
                pio: ok_pio,
                zstd: zstd.clone(),
                file_type: TotkFileType::Byml,
            }),
            Err(_err) => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Error for byml file",
                ));
            }
        }
    }

    pub fn from_binary<P: AsRef<Path>>(
        data: &[u8],
        zstd: Arc<TotkZstd<'a>>,
        full_path: P,
    ) -> io::Result<BymlFile<'a>> {
        let endian = BymlFile::get_endiannes(data).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "BYML data does not begin with YB or BY magic",
            )
        })?;
        let pio = Byml::from_binary(data);
        match pio {
            Ok(ok_pio) => Ok(BymlFile {
                endian: Some(endian),
                file_data: FileData {
                    file_type: TotkFileType::Byml,
                    data: data.to_vec(),
                    compression: None,
                    yaz0_alignment: 0,
                },
                path: Pathlib::new(full_path),
                pio: ok_pio,
                zstd,
                file_type: TotkFileType::Byml,
            }),
            Err(_err) => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Error for byml file",
                ));
            }
        }
    }

    pub fn get_endiannes_from_self(&self) -> roead::Endian {
        if Magic::is_byml_big_endian(&self.file_data.data) {
            return roead::Endian::Big;
        } else if Magic::is_byml_little_endian(&self.file_data.data) {
            return roead::Endian::Little;
        }
        return roead::Endian::Little;
    }

    pub fn get_endiannes(data: &[u8]) -> Option<roead::Endian> {
        if Magic::is_byml_big_endian(data) {
            return Some(roead::Endian::Big);
        }
        if Magic::is_byml_little_endian(data) {
            return Some(roead::Endian::Little);
        }
        None
    }

    pub fn byml_data_to_bytes(
        rawdata: &Vec<u8>,
        zstd: Arc<TotkZstd>,
    ) -> Result<FileData, io::Error> {
        let mut buffer = rawdata.clone();
        let mut data = FileData::new();
        if Magic::is_yaz0(&buffer) {
            data.yaz0_alignment = TotkZstd::yaz0_alignment(&buffer);
            if let Ok(dec_data) = TotkZstd::decompress_yaz0(&buffer) {
                buffer = dec_data;
                data.compression = Some(ZstdDictionary::Yaz0);
            }
        }
        if Magic::is_byml(&buffer) {
            //regular byml file,
            data.data = buffer;
            data.file_type = TotkFileType::Byml;
            return Ok(data);
        } else {
            match zstd.decompressor.decompress_zs(&buffer) {
                //regular byml file compressed with zs
                Ok(res) => {
                    if Magic::is_byml(&res) {
                        data.data = res;
                        data.file_type = TotkFileType::Byml;
                        data.compression = Some(ZstdDictionary::Zs);
                    }
                }
                Err(_err) => {}
            }
        }
        if !Magic::is_byml(&data.data) {
            match zstd.decompressor.decompress_bcett(&buffer) {
                //bcett map file
                Ok(res) => {
                    data.data = res;
                    data.file_type = TotkFileType::Byml;
                    if Magic::is_byml(&data.data) {
                        data.file_type = TotkFileType::Bcett;
                        data.compression = Some(ZstdDictionary::Bcett);
                    }
                }
                _ => {}
            }
        }

        if !Magic::is_byml(&data.data) {
            match zstd.try_decompress(&buffer) {
                //try decompressing with other dicts
                Ok(res) => {
                    data.data = res;
                    data.file_type = TotkFileType::Other;
                }
                Err(_err) => {}
            }
        }
        if Magic::is_byml(&data.data) {
            if data.file_type != TotkFileType::Bcett {
                data.file_type = TotkFileType::Byml;
            }
            return Ok(data);
        }
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Invalid data, not a byml",
        ));
    }

    pub fn byml_file_to_bytes<P: AsRef<Path>>(
        path: P,
        zstd: Arc<TotkZstd>,
    ) -> Result<FileData, io::Error> {
        let path = path.as_ref();
        let mut f_handle: fs::File = fs::File::open(path)?;
        let mut buffer: Vec<u8> = Vec::new();
        f_handle.read_to_end(&mut buffer)?;
        let (rawdata, compression) = zstd.try_decompress_all_ordered_safe(&buffer, &path);
        println!(
            "[BYML] Binary comp dict {:?}, is byml? {}",
            &compression,
            Magic::is_byml(&rawdata)
        );

        if !Magic::is_byml(&rawdata) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("File {} is not a byml file", &path.display()),
            ));
        }
        let ftype = if compression == ZstdDictionary::Bcett {
            TotkFileType::Bcett
        } else {
            TotkFileType::Byml
        };

        return Ok(FileData {
            file_type: ftype,
            data: rawdata,
            compression: Some(compression),
            yaz0_alignment: 0,
        });
    }

    pub fn is_banc(&self) -> bool {
        return self.file_type == TotkFileType::Bcett;
        // Pathlib::is_banc_path(&self.path.full_path)
    }

    pub fn to_string(&self) -> String {
        let float_prec = if self.zstd.totk_config.lower_float_prec {
            Some(4)
        } else {
            None
        };
        let max_inl =
            if is_gamedatalist(&self.path.full_path) && self.zstd.totk_config.yaml_max_inl < 5 {
                5
            } else {
                self.zstd.totk_config.yaml_max_inl
            };
        // println!("max_inl: {}", max_inl);
        let mut text = Byml::to_text_advanced(&self.pio, max_inl, float_prec);
        // let mut text = Byml::to_text(&self.pio);
        if self.is_banc() {
            if self.zstd.totk_config.rotation_deg {
                text = process_Rotate_in_banc(&text, self.zstd.totk_config.rotation_deg);
            }
        }
        // Byml::to_text(&self.pio)
        // lower_float_precision(&text)
        // process_inline_content(Byml::to_text(&self.pio), self.zstd.totk_config.yaml_max_inl)
        text
    }
}

#[cfg(test)]
mod bcett_tests {
    use super::BymlFile;
    use crate::{
        TotkConfig::TotkConfig,
        Zstd::{TotkZstd, ZstdDictionary},
    };
    use std::{path::Path, sync::Arc};

    #[test]
    fn configured_romfs_bcett_file_uses_bcett_dictionary() {
        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let sample = romfs.join("Banc/BossVehicle/Craft_AutoCar_Koga_Spike_2_Static.bcett.byml.zs");
        if !sample.is_file() || !romfs.join("Pack/ZsDic.pack.zs").is_file() {
            return;
        }
        let mut config = TotkConfig::default();
        config.romfs = romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ZSTD dictionaries"),
        );
        let file = BymlFile::new(&sample, zstd).expect("open BCETT BYML");
        assert_eq!(file.file_data.compression, Some(ZstdDictionary::Bcett));
    }

    #[test]
    fn tmp_bcett_fixtures_open_with_bcett_dictionary() {
        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        if !romfs.join("Pack/ZsDic.pack.zs").is_file() {
            return;
        }
        let mut config = TotkConfig::default();
        config.romfs = romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ZSTD dictionaries"),
        );
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/bcett");
        let mut tested = 0;
        for entry in std::fs::read_dir(fixtures).expect("read BCETT fixtures") {
            let path = entry.expect("read BCETT fixture entry").path();
            if !path.is_file() {
                continue;
            }
            assert!(
                crate::file_format::Archive::ArchiveDocument::open_with_zstd(&path, &zstd)
                    .expect("archive routing must not reject BCETT compression")
                    .is_none(),
                "BCETT was incorrectly detected as an archive: {}",
                path.display()
            );
            let file = BymlFile::new(&path, zstd.clone())
                .unwrap_or_else(|| panic!("failed to open {}", path.display()));
            assert_eq!(file.file_data.compression, Some(ZstdDictionary::Bcett));
            tested += 1;
        }
        assert!(tested > 0, "no BCETT fixtures were tested");
    }
}

#[inline]
fn rad_to_deg(rad: f64) -> f64 {
    rad * (180.0 / std::f64::consts::PI)
}

/// Converts degrees to radians
#[inline]
fn deg_to_rad(deg: f64) -> f64 {
    deg * (std::f64::consts::PI / 180.0)
}

#[allow(dead_code)]
fn lower_float_precision(input: &str) -> String {
    let Ok(re) = Regex::new(r"\[([^\]]+)\]") else {
        return input.to_string();
    };
    let text = re
        .replace_all(input, |caps: &regex::Captures| {
            let inner = caps.get(1).map_or("", |capture| capture.as_str());
            // Process each float within the brackets
            format!(
                "[{}]",
                inner
                    .split(',')
                    .map(|s| {
                        s.trim()
                            .parse::<f64>()
                            .map(|num| {
                                if num == 0.0 {
                                    "0.0".to_string()
                                } else {
                                    format!("{:.5}", num)
                                        .trim_end_matches('0')
                                        .trim_end_matches('.')
                                        .to_string()
                                }
                            }) // Lower precision to 5
                            .unwrap_or_else(|_| s.to_string()) // Keep non-float values as is
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ) // Rejoin the floats with commas
        })
        .into_owned();

    let Ok(re) = Regex::new(r"\{([^}]+)\}") else {
        return text;
    };
    re.replace_all(&text, |caps: &regex::Captures| {
        let inner = caps.get(1).map_or("", |capture| capture.as_str());
        // Process each key-value pair within the braces
        format!(
            "{{{}}}",
            inner
                .split(',')
                .map(|pair| {
                    let mut parts = pair.split(':');
                    let key = parts.next().unwrap_or("").trim();
                    let value = parts.next().unwrap_or("").trim();
                    let formatted_value = value
                        .parse::<f64>()
                        .map(|num| {
                            if num == 0.0 {
                                "0.0".to_string()
                            } else {
                                format!("{:.5}", num)
                                    .trim_end_matches('0')
                                    .trim_end_matches('.')
                                    .to_string()
                            }
                        })
                        .unwrap_or_else(|_| value.to_string()); // Keep non-float values as is
                    format!("{}: {}", key, formatted_value)
                })
                .collect::<Vec<_>>()
                .join(", ")
        ) // Rejoin the key-value pairs
    })
    .into_owned()

    // text
}

/// Finds and replaces `Rotate: [...]` with radian values converted to degrees
fn process_Rotate_in_banc(input: &str, deg_to_rad: bool) -> String {
    let Ok(re) = Regex::new(r"Rotate:\s*\[([^\]]+)\]") else {
        return input.to_string();
    };
    re.replace_all(input, |caps: &regex::Captures| {
        let inner = caps.get(1).map_or("", |capture| capture.as_str());
        let array: Vec<f64> = if deg_to_rad {
            inner
                .split(',')
                .filter_map(|s| s.trim().parse::<f64>().ok())
                .map(rad_to_deg)
                .collect()
        } else {
            inner
                .split(',')
                .filter_map(|s| s.trim().parse::<f64>().ok())
                .collect()
        };
        // format!("Rotate: [{}]", array.iter().map(|v| format!("{:.6}", v)).collect::<Vec<_>>().join(","))
        format!(
            "Rotate: [{}]",
            array
                .iter()
                .map(|v| {
                    if *v == 0.0 {
                        "0.0".to_string()
                    } else {
                        format!("{:.4}", v)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
    .into_owned()
}

/// Finds and replaces `Rotate: [...]` with degree values converted to radians
pub fn replace_rotate_deg_to_rad(input: &str) -> String {
    let Ok(re) = Regex::new(r"Rotate:\s*\[([^\]]+)\]") else {
        return input.to_string();
    };
    re.replace_all(input, |caps: &regex::Captures| {
        let inner = caps.get(1).map_or("", |capture| capture.as_str());
        let array: Vec<f64> = inner
            .split(',')
            .filter_map(|s| s.trim().parse::<f64>().ok())
            .map(deg_to_rad)
            .collect();
        format!(
            "Rotate: [{}]",
            array
                .iter()
                .map(|v| format!("{:.5}", v))
                .collect::<Vec<_>>()
                .join(",")
        )
    })
    .into_owned()
}

pub fn bytes_to_file(data: Vec<u8>, path: &str) -> io::Result<()> {
    if data.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "refusing to write an empty save result",
        ));
    }
    let f = fs::File::create(&path); //TODO check if the ::create is sufficient
    match f {
        Ok(mut f_handle) => {
            //file does not exist
            f_handle.write_all(&data)?;
        }
        Err(_) => {
            //file exist, overwrite
            let mut f_handle = OpenOptions::new().write(true).open(&path)?;
            f_handle.write_all(&data)?;
        }
    }
    Ok(())
}

//#[derive(Serialise, Deserialise)]
pub struct OpenedFile<'a> {
    pub file_type: TotkFileType,
    pub path: Pathlib,
    pub compression: Option<ZstdDictionary>,
    pub yaz0_alignment: u32,
    pub byml: Option<BymlFile<'a>>,
    // pub ainb: Option<crate::parser::ainb::AinbDocument>,
    pub endian: Option<roead::Endian>,
    // pub msyt: Option<MsbtFile>,
    pub msyt: Option<Msbt>,
    // pub aamp: Option<()>,
    pub bfres: Option<crate::file_format::Model3D::bfres::BfresFile>,
    pub bfres_data: Option<Vec<u8>>,
    pub custom_g1m: Option<Vec<u8>>,
    pub visual_data: Option<Vec<u8>>,
    pub mii_data: Option<Vec<u8>>,
    pub bphcl: Option<crate::file_format::bphcl::BphclFile>,
    pub bphhb: Option<crate::file_format::bphhb::BphhbFile>,
    pub hkcl: Option<crate::file_format::hkcl::HkclFile>,
    pub tag: Option<TagProduct<'a>>,
    pub restbl: Option<Restbl<'a>>,
    pub esetb: Option<Esetb<'a>>,
    pub asb: Option<crate::parser::asb::Asb>,
    pub asb_baev_path: Option<std::path::PathBuf>,
    pub asb_baev_data: Option<Vec<u8>>,
}

impl Default for OpenedFile<'_> {
    fn default() -> Self {
        Self {
            file_type: TotkFileType::None,
            path: Pathlib::default(),
            compression: None,
            yaz0_alignment: 0,
            byml: None,
            // ainb: None,
            endian: None,
            msyt: None,
            // aamp: None,
            bfres: None,
            bfres_data: None,
            custom_g1m: None,
            visual_data: None,
            mii_data: None,
            bphcl: None,
            bphhb: None,
            hkcl: None,
            tag: None,
            restbl: None,
            esetb: None,
            asb: None,
            asb_baev_path: None,
            asb_baev_data: None,
        }
    }
}

#[allow(dead_code, unused_variables)]
impl<'a> OpenedFile<'_> {
    pub fn new(
        path: String,
        file_type: TotkFileType,
        endian: Option<roead::Endian>,
        msyt: Option<Msbt>,
    ) -> Self {
        Self {
            file_type: file_type,
            path: Pathlib::new(path),
            compression: None,
            yaz0_alignment: 0,
            byml: None,
            // ainb: None,
            endian: endian,
            msyt: msyt,
            // aamp: None,
            bfres: None,
            bfres_data: None,
            custom_g1m: None,
            visual_data: None,
            mii_data: None,
            bphcl: None,
            bphhb: None,
            hkcl: None,
            tag: None,
            restbl: None,
            esetb: None,
            asb: None,
            asb_baev_path: None,
            asb_baev_data: None,
        }
    }

    pub fn from_path(path: String, file_type: TotkFileType) -> Self {
        Self {
            file_type: file_type,
            path: Pathlib::new(path),
            compression: None,
            yaz0_alignment: 0,
            byml: None,
            // ainb: None,
            endian: None,
            msyt: None,
            // aamp: None,
            bfres: None,
            bfres_data: None,
            custom_g1m: None,
            visual_data: None,
            mii_data: None,
            bphcl: None,
            bphhb: None,
            hkcl: None,
            tag: None,
            restbl: None,
            esetb: None,
            asb: None,
            asb_baev_path: None,
            asb_baev_data: None,
        }
    }

    pub fn reset(&mut self) {
        self.file_type = TotkFileType::None;
        self.path = Pathlib::default();
        self.byml = None;
        // self.ainb = None;
        self.endian = None;
        self.msyt = None;
        // self.aamp = None;
        self.bfres = None;
        self.bphcl = None;
        self.bphhb = None;
        self.hkcl = None;
        self.tag = None;
        self.asb = None;
        self.asb_baev_path = None;
        self.asb_baev_data = None;
    }

    pub fn get_endian_label(&self) -> String {
        match self.endian {
            Some(endian) => match endian {
                roead::Endian::Big => {
                    return "BE".to_string();
                }
                roead::Endian::Little => {
                    return "LE".to_string();
                }
            },
            None => {
                return "".to_string();
            }
        }
    }

    // pub fn open(&mut self, file_path: &str, zstd: Arc<TotkZstd>) -> String {
    //     let res = String::new();
    //     let path = Pathlib::new(file_path.to_string());
    //     if self.open_tag(&path, zstd.clone()) {
    //         if let Some(tag) = &self.tag {
    //             return tag.text.to_string();
    //         }
    //     }
    //     res
    // }

    pub fn open_tag(&mut self, path: &Pathlib, zstd: Arc<TotkZstd>) -> bool {
        if path.name.to_lowercase().starts_with("tag.product") {
            let tag = TagProduct::new(path.full_path.clone(), zstd.clone());
            match tag {
                Some(mut tag) => {
                    match tag.parse() {
                        Ok(_) => {
                            println!("Tag parsed!");
                        }
                        Err(err) => {
                            eprintln!("Error parsing tag! {:?}", err);
                            return false;
                        }
                    }
                    self.reset();
                    //self.tag = Some(tag);
                    self.file_type = TotkFileType::TagProduct;
                    self.path = path.clone();
                    self.endian = Some(roead::Endian::Little);
                    return true;
                }
                None => {
                    return false;
                }
            }
        }
        false
    }
}

#[allow(dead_code)]
fn print_type_of<T>(_: &T) {
    println!("{}", type_name::<T>());
}

pub fn write_string_to_file(path: &str, content: &str) -> io::Result<()> {
    let file = fs::File::create(path)?;
    let mut writer = BufWriter::new(file);

    writer.write_all(content.as_bytes())?;

    // The buffer is automatically flushed when writer goes out of scope,
    // but you can manually flush it if needed.
    writer.flush()?;

    Ok(())
}

#[allow(dead_code)]
pub fn read_string_from_file(path: &str) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    Ok(contents)
}
