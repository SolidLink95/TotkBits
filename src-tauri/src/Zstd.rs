use crate::file_format::Pack::PackFile;
use crate::Open_and_Save::get_string_from_data;
use crate::Settings::{Magic, Pathlib};
use crate::TotkConfig::TotkConfig;
use digest::Digest;
use roead::sarc::*;
use sha2::Sha256;
use zstd::zstd_safe::zstd_sys::{
    ZSTD_CCtx, ZSTD_CDict_s, ZSTD_compressBound, ZSTD_compress_usingCDict, ZSTD_createCCtx,
    ZSTD_createCDict, ZSTD_freeCCtx, ZSTD_freeCDict, ZSTD_isError,
};

use std::collections::HashMap;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

//use zstd::zstd_safe::CompressionLevel;
use std::io::{self, Read, Write};
use std::{env, fs};
use zstd::dict::{DecoderDictionary, EncoderDictionary};
use zstd::{stream::Decoder, stream::Encoder};

pub const TOTK_ZSTD_COMPRESSION_LEVEL: i32 = 16;

#[derive(Debug, PartialEq, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TotkFileType {
    AINB,
    ASB,
    Restbl,
    TagProduct,
    Sarc,
    MalsSarc,
    Byml,
    Aamp,
    Bntx,
    Bphcl,
    Bphhb,
    Hkcl,
    Hkrg,
    Bphyssb,
    Msbt,
    Bcett,
    Esetb,
    Evfl,
    Xlink,
    Text,
    Other,
    //SMO
    SmoSaveFile,
    //3D
    Bfres,
    Fbx,
    G1M,
    Glb,
    Mii,
    Image,
    Archive,
    Bars,
    Bwav,
    Bfwav,
    Amta,
    Riff,
    Compressed,
    None,
}

pub struct ZstdCppCompressor {
    pub zs_cpp: Option<*mut ZSTD_CDict_s>,
    pub bcett_cpp: Option<*mut ZSTD_CDict_s>,
    pub packzs_cpp: Option<*mut ZSTD_CDict_s>,
    pub empty_cpp: *mut ZSTD_CDict_s,
    pub cctx: Mutex<*mut ZSTD_CCtx>,
}

// CDicts are immutable after construction. Access to the mutable CCtx is serialized.
unsafe impl Send for ZstdCppCompressor {}
unsafe impl Sync for ZstdCppCompressor {}

impl ZstdCppCompressor {
    #[allow(dead_code)]
    pub fn from_totk_zstd(zstd: Arc<TotkZstd>) -> ZstdCppCompressor {
        Self::from_zsdic(
            zstd.clone().zsdic.clone(),
            zstd.clone().compressor.comp_level,
        )
    }
    pub fn from_zsdic(zsdic: Arc<ZsDic>, comp_level: i32) -> ZstdCppCompressor {
        // let zsdic = &zstd.zsdic;
        // let comp_level = zstd.compressor.comp_level;
        let zs_cpp = zsdic.zs_data.as_ref().map(|data| unsafe {
            ZSTD_createCDict(
                data.as_ptr() as *const std::ffi::c_void,
                data.len(),
                comp_level,
            )
        });
        let bcett_cpp = zsdic.bcett_data.as_ref().map(|data| unsafe {
            ZSTD_createCDict(
                data.as_ptr() as *const std::ffi::c_void,
                data.len(),
                comp_level,
            )
        });
        let packzs_cpp = zsdic.packzs_data.as_ref().map(|data| unsafe {
            ZSTD_createCDict(
                data.as_ptr() as *const std::ffi::c_void,
                data.len(),
                comp_level,
            )
        });
        let empty_cpp = unsafe {
            ZSTD_createCDict(
                zsdic.empty_data.as_ptr() as *const std::ffi::c_void,
                zsdic.empty_data.len(),
                comp_level,
            )
        };
        let cctx = unsafe { ZSTD_createCCtx() };
        ZstdCppCompressor {
            zs_cpp: zs_cpp,
            bcett_cpp: bcett_cpp,
            packzs_cpp: packzs_cpp,
            empty_cpp: empty_cpp,
            cctx: Mutex::new(cctx),
        }
    }
    pub fn compress(&self, data: &[u8], cdict: *mut ZSTD_CDict_s) -> io::Result<Vec<u8>> {
        if cdict.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to create ZSTD dictionary",
            ));
        }
        // Calculate the maximum compressed size and allocate the buffer
        let dst_capacity = unsafe { ZSTD_compressBound(data.len()) as usize };
        let mut buffer: Vec<u8> = vec![0; dst_capacity]; // Allocate space

        // Perform the compression
        let cctx = self
            .cctx
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "ZSTD context lock poisoned"))?;
        if cctx.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to create ZSTD context",
            ));
        }
        let res_size = unsafe {
            ZSTD_compress_usingCDict(
                *cctx,
                buffer.as_mut_ptr() as *mut core::ffi::c_void,
                dst_capacity,
                data.as_ptr() as *const core::ffi::c_void,
                data.len(),
                cdict,
            )
        };

        // Check for errors
        if unsafe { ZSTD_isError(res_size) } != 0 {
            // Handle error (for simplicity, returning a generic I/O error here)
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "ZSTD compression failed",
            ));
        }

        // Truncate the buffer to the actual size of the compressed data
        buffer.truncate(res_size as usize);

        Ok(buffer)
    }

    pub fn compress_zs(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        self.compress(data, self.zs_cpp.ok_or_else(missing_dictionary)?)
    }

    pub fn compress_pack(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        self.compress(data, self.packzs_cpp.ok_or_else(missing_dictionary)?)
    }

    pub fn compress_bcett(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        self.compress(data, self.bcett_cpp.ok_or_else(missing_dictionary)?)
    }

    pub fn compress_empty(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        ZstdCppCompressor::compress(&self, &data, self.empty_cpp)
    }
}

fn missing_dictionary() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        "Requested ZSTD dictionary is unavailable",
    )
}

impl Drop for ZstdCppCompressor {
    fn drop(&mut self) {
        unsafe {
            if let Some(dict) = self.zs_cpp {
                ZSTD_freeCDict(dict);
            }
            if let Some(dict) = self.bcett_cpp {
                ZSTD_freeCDict(dict);
            }
            if let Some(dict) = self.packzs_cpp {
                ZSTD_freeCDict(dict);
            }
            ZSTD_freeCDict(self.empty_cpp);
            if let Ok(cctx) = self.cctx.get_mut() {
                ZSTD_freeCCtx(*cctx);
            }
        }
    }
}

pub struct TotkZstd<'a> {
    pub totk_config: Arc<TotkConfig>,
    pub decompressor: ZstdDecompressor<'a>,
    pub compressor: ZstdCompressor<'a>,
    pub zsdic: Arc<ZsDic>,
    pub cpp_compressor: ZstdCppCompressor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZstdDictionary {
    Zs,
    Pack,
    Empty,
    Bcett,
    Yaz0,
    Mcpk,
    None,
}

pub const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

impl<'a> TotkZstd<'_> {
    pub fn compress_mcpk(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        crate::compression::meshcodec::MeshCodec::compress(data)
    }
    pub fn decompress_mcpk(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        if !Magic::is_mcpk(data) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "input does not have MCPK magic",
            ));
        }
        crate::compression::meshcodec::MeshCodec::decompress(data)
    }

    pub fn decompress_empty(data: &[u8]) -> io::Result<Vec<u8>> {
        if !Magic::is_zstd(data) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "input does not have Zstandard frame magic",
            ));
        }
        let dictionary = DecoderDictionary::copy(&[]);
        let mut decoder = Decoder::with_prepared_dictionary(data, &dictionary)?;
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)?;
        Ok(decompressed)
    }

    pub fn new(totk_config: Arc<TotkConfig>, comp_level: i32) -> io::Result<TotkZstd<'a>> {
        let zsdic: Arc<ZsDic> = Arc::new(ZsDic::new(totk_config.clone())?);
        let decompressor: ZstdDecompressor =
            ZstdDecompressor::new(totk_config.clone(), zsdic.clone())?;
        let compressor: ZstdCompressor =
            ZstdCompressor::new(totk_config.clone(), zsdic.clone(), comp_level)?;

        Ok(TotkZstd {
            totk_config,
            decompressor,
            compressor,
            zsdic: zsdic.clone(),
            cpp_compressor: ZstdCppCompressor::from_zsdic(zsdic.clone(), comp_level),
        })
    }

    /// Builds a usable, dictionary-free codec state for degraded application startup.
    /// Plain files and empty-dictionary Zstandard data remain supported; operations
    /// requiring a game dictionary return `NotFound` through their existing APIs.
    pub fn dictionaryless(totk_config: Arc<TotkConfig>, comp_level: i32) -> TotkZstd<'a> {
        let zsdic = Arc::new(ZsDic::empty());
        let decompressor = ZstdDecompressor {
            totk_config: totk_config.clone(),
            packzs: None,
            zs: None,
            bcett: None,
            empty: Arc::new(DecoderDictionary::copy(&[])),
        };
        let compressor = ZstdCompressor {
            totk_config: totk_config.clone(),
            packzs: None,
            zs: None,
            bcett: None,
            empty: Arc::new(EncoderDictionary::copy(&[], comp_level)),
            comp_level,
        };
        TotkZstd {
            totk_config,
            decompressor,
            compressor,
            zsdic: zsdic.clone(),
            cpp_compressor: ZstdCppCompressor::from_zsdic(zsdic, comp_level),
        }
    }
    pub fn try_decompress(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        if Magic::is_mcpk(data) {
            return crate::compression::meshcodec::MeshCodec::decompress(data);
        }
        self.try_decompress_with_dictionary(data)
            .map(|(data, _)| data)
    }

    /// Returns a decompressed payload when the input uses any supported
    /// compression, or an owned copy of the original payload when it does not.
    /// This is intended for content-based format detection where an ordinary
    /// uncompressed file is not an error.
    pub fn try_decompress_safe(&self, data: &[u8]) -> Vec<u8> {
        self.try_decompress(data).unwrap_or_else(|_| data.to_vec())
    }

    pub fn try_decompress_with_dictionary(
        &self,
        data: &[u8],
    ) -> Result<(Vec<u8>, ZstdDictionary), io::Error> {
        if Magic::is_yaz0(data) {
            return Self::decompress_yaz0(data).map(|data| (data, ZstdDictionary::Yaz0));
        }
        self.try_decompress_zstd_ordered(data, None)
    }

    pub fn try_decompress_for_path(
        &self,
        path: impl AsRef<Path>,
        data: &[u8],
    ) -> Result<(Vec<u8>, ZstdDictionary), io::Error> {
        if Magic::is_yaz0(data) {
            return Self::decompress_yaz0(data).map(|data| (data, ZstdDictionary::Yaz0));
        }
        self.try_decompress_zstd_ordered(data, preferred_dictionary_for_path(path))
    }

    pub fn get_preffered_dict(file_path: impl AsRef<Path>) -> Option<ZstdDictionary> {
        let path = file_path
            .as_ref()
            .to_string_lossy()
            .into_owned()
            .to_ascii_lowercase();
        if Pathlib::is_rstb_path(&path) {
            return Some(ZstdDictionary::Empty);
        }
        if Pathlib::is_banc_path(&path) {
            return Some(ZstdDictionary::Bcett);
        }
        if Pathlib::is_bfres_path(&path) {
            return Some(ZstdDictionary::Mcpk);
        }
        if Pathlib::is_sarc_path(&path) {
            return Some(ZstdDictionary::Pack);
        }
        if is_tagproduct(&path)
            || is_gamedatalist(&path)
            || Pathlib::is_esetb_path(&path)
            || Pathlib::is_ainb_path(&path)
            || Pathlib::is_asb_path(&path)
            || Pathlib::is_evfl_path(&path)
            || Pathlib::is_xlink_path(&path)
            || Pathlib::is_byml_path(&path)
            || Pathlib::is_bntx_path(&path)
            || Pathlib::is_bars_path(&path)
        {
            return Some(ZstdDictionary::Zs);
        }
        if Pathlib::is_yaz0_path(&path) {
            return Some(ZstdDictionary::Yaz0);
        }

        None
    }

    pub fn try_decompress_all_ordered_safe(
        &self,
        data: &[u8],
        file_path: impl AsRef<Path>,
    ) -> (Vec<u8>, ZstdDictionary) {
        if !Magic::is_compressed(&data) {
            return (data.to_vec(), ZstdDictionary::None);
        }
        let res = self
            .try_decompress_all_ordered(&data, file_path.as_ref())
            .unwrap_or((data.to_vec(), ZstdDictionary::None));
        res
    }
    pub fn try_decompress_all_ordered(
        &self,
        data: &[u8],
        file_path: impl AsRef<Path>,
    ) -> Result<(Vec<u8>, ZstdDictionary), io::Error> {
        let path = file_path.as_ref();
        let preffered: Option<ZstdDictionary> = Self::get_preffered_dict(&path);
        let mut tried = ZstdDictionary::None;
        if let Some(_dict) = preffered {
            match _dict {
                ZstdDictionary::Mcpk => {
                    if let Ok(newdata) = self.decompress_mcpk(data) {
                        return Ok((newdata, ZstdDictionary::Mcpk));
                    }
                    tried = ZstdDictionary::Mcpk;
                }
                ZstdDictionary::Yaz0 => {
                    if let Ok(newdata) = Self::decompress_yaz0(data) {
                        return Ok((newdata, ZstdDictionary::Yaz0));
                    }
                    tried = ZstdDictionary::Yaz0;
                }
                _ => {}
            }
        }
        if tried != ZstdDictionary::Mcpk {
            if let Ok(newdata) = self.decompress_mcpk(data) {
                return Ok((newdata, ZstdDictionary::Mcpk));
            }
        }
        if tried != ZstdDictionary::Yaz0 {
            if let Ok(newdata) = Self::decompress_yaz0(data) {
                return Ok((newdata, ZstdDictionary::Yaz0));
            }
        }

        self.try_decompress_zstd_ordered(data, preffered)
    }
    fn try_decompress_zstd_ordered(
        &self,
        data: &[u8],
        preferred: Option<ZstdDictionary>,
    ) -> Result<(Vec<u8>, ZstdDictionary), io::Error> {
        if !Magic::is_zstd(data) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "input does not have Zstandard frame magic",
            ));
        }
        let defaults = [
            ZstdDictionary::Zs,
            ZstdDictionary::Pack,
            ZstdDictionary::Bcett,
        ];
        let mut attempted = Vec::with_capacity(defaults.len());
        // A dictionaryless frame can also decode successfully when an unrelated
        // dictionary is supplied. Probe Empty first so the returned kind records
        // what the frame actually requires instead of the first usable decoder.
        for kind in std::iter::once(ZstdDictionary::Empty)
            .chain(preferred)
            .chain(defaults)
        {
            if kind == ZstdDictionary::Yaz0 || attempted.contains(&kind) {
                continue;
            }
            attempted.push(kind);
            if let Ok(decoded) = self.try_decompress_using(data, kind) {
                return Ok((decoded, kind));
            }
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unable to decompress with any dictionary!",
        ))
    }

    pub fn try_decompress_using(
        &self,
        data: &[u8],
        dictionary: ZstdDictionary,
    ) -> Result<Vec<u8>, io::Error> {
        if dictionary == ZstdDictionary::None {
            return Ok(data.to_vec());
        }
        if dictionary == ZstdDictionary::Yaz0 {
            return Self::decompress_yaz0(data);
        }
        if dictionary == ZstdDictionary::Mcpk {
            return self.decompress_mcpk(data);
        }
        if !Magic::is_zstd(data) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "input does not have Zstandard frame magic",
            ));
        }
        let dict = match dictionary {
            ZstdDictionary::Zs => self.decompressor.zs.as_deref(),
            ZstdDictionary::Pack => self.decompressor.packzs.as_deref(),
            ZstdDictionary::Empty => Some(self.decompressor.empty.as_ref()),
            ZstdDictionary::Bcett => self.decompressor.bcett.as_deref(),
            ZstdDictionary::Yaz0 | ZstdDictionary::Mcpk | ZstdDictionary::None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "compression format does not use a Zstandard dictionary",
                ));
            }
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "dictionary is unavailable"))?;
        self.decompressor.decompress(data, dict)
    }

    pub fn compress_with_dictionary(
        &self,
        data: &[u8],
        dictionary: ZstdDictionary,
    ) -> io::Result<Vec<u8>> {
        match dictionary {
            ZstdDictionary::Zs => self.compress_zs(data),
            ZstdDictionary::Pack => self.compress_pack(data),
            ZstdDictionary::Empty => self.compress_empty(data),
            ZstdDictionary::Bcett => self.compress_bcett(data),
            ZstdDictionary::Yaz0 => Self::compress_yaz0(data),
            ZstdDictionary::Mcpk => self.compress_mcpk(data),
            ZstdDictionary::None => Ok(data.to_vec()),
        }
    }

    pub fn decompress_yaz0(data: &[u8]) -> io::Result<Vec<u8>> {
        if !Magic::is_yaz0(data) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "input does not have Yaz0 magic",
            ));
        }
        roead::yaz0::decompress(data)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn compress_yaz0(data: &[u8]) -> io::Result<Vec<u8>> {
        Ok(roead::yaz0::compress(data))
    }

    pub fn compress_yaz0_with_alignment(data: &[u8], alignment: u32) -> io::Result<Vec<u8>> {
        Ok(roead::yaz0::compress_with_options(
            data,
            roead::yaz0::CompressOptions {
                alignment,
                compression_level: 3,
            },
        ))
    }

    pub fn yaz0_alignment(data: &[u8]) -> u32 {
        roead::yaz0::get_header(data)
            .filter(|header| &header.magic == b"Yaz0")
            .map(|header| header.data_alignment)
            .unwrap_or(0)
    }
    pub fn find_vanila_internal_file_path_in_romfs<P: AsRef<Path>>(
        &self,
        internal_path: P,
    ) -> io::Result<String> {
        //parse json
        let res = crate::utils::LookupData::internal_filepaths();
        //find the sarc file
        let int_path_str = internal_path.as_ref().to_string_lossy().to_string();
        let sarc_localpath = res
            .get(&int_path_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "File not found"))?;
        let sarc_filepath = PathBuf::from(&self.totk_config.romfs).join(sarc_localpath);
        Ok(sarc_filepath.to_string_lossy().to_string())
    }
    pub fn find_vanila_internal_file_data_from_path<P: AsRef<Path>>(
        &self,
        internal_path: P,
        sarc_filepath: String,
        zstd: Arc<TotkZstd>,
    ) -> io::Result<String> {
        let int_path_str = internal_path.as_ref().to_string_lossy().to_string();
        let sarc_var = PackFile::new(&sarc_filepath, zstd.clone())?;
        //get the bytes
        let rawdata = sarc_var.sarc.get_data(&int_path_str).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "File: {} not found in sarc: {:?}",
                    &int_path_str, &sarc_filepath
                ),
            )
        })?;
        //parse to string
        let (_, result) = get_string_from_data(&int_path_str, rawdata.to_vec(), zstd.clone())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "Unable to parse file:\n{}\n from sarc:\n{:?}",
                        &int_path_str, &sarc_filepath
                    ),
                )
            })?;
        Ok(result)
    }
    pub fn find_vanila_internal_file_data_in_romfs<P: AsRef<Path>>(
        &self,
        internal_path: P,
        zstd: Arc<TotkZstd>,
    ) -> io::Result<String> {
        //parse json
        // let json_zlibdata = fs::read("misc/totk_internal_filepaths.bin")?;
        // let mut decoder = ZlibDecoder::new(&json_zlibdata[..]);
        // let mut json_str = String::new();
        // decoder.read_to_string(&mut json_str)?;
        // let res: HashMap<String, String> = serde_json::from_str(&json_str)?;
        // //find the sarc file
        // let int_path_str = internal_path.as_ref().to_string_lossy().to_string();
        // let sarc_localpath = res.get(&int_path_str).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "File not found"))?;
        // let sarc_filepath = PathBuf::from(&self.totk_config.romfs).join(sarc_localpath);
        let sarc_filepath = self.find_vanila_internal_file_path_in_romfs(&internal_path)?;
        self.find_vanila_internal_file_data_from_path(&internal_path, sarc_filepath, zstd.clone())
        // let sarc_var = PackFile::new(&sarc_filepath, zstd.clone())?;
        // //get the bytes
        // let rawdata = sarc_var.sarc.get_data(&int_path_str).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("File: {} not found in sarc: {:?}", &int_path_str, &sarc_filepath)))?;
        // //parse to string
        // let (_, result) = get_string_from_data("".to_string(), rawdata.to_vec(), zstd.clone())
        //     .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("Unable to parse file:\n{}\n from sarc:\n{:?}", &int_path_str, &sarc_filepath)))?;;
        // Ok(result)
    }

    pub fn decompress_bcett(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        self.decompressor.decompress_bcett(data)
    }
    pub fn decompress_pack(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        self.decompressor.decompress_pack(data)
    }
    pub fn decompress_zs(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        self.decompressor.decompress_zs(data)
    }
    pub fn compress_zs(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        self.cpp_compressor.compress_zs(data)
    }
    pub fn compress_pack(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        self.cpp_compressor.compress_pack(data)
    }
    pub fn compress_bcett(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        self.cpp_compressor.compress_bcett(data)
    }
    pub fn compress_empty(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        self.compressor.compress_empty(data)
    }
    /// Dictionaryless compression at an explicit level instead of the
    /// configured one (e.g. to match another tool's output).
    pub fn compress_empty_with_level(&self, data: &[u8], level: i32) -> io::Result<Vec<u8>> {
        self.compressor.compress_empty_with_level(data, level)
    }
}

pub fn preferred_dictionary_for_path(path: impl AsRef<Path>) -> Option<ZstdDictionary> {
    let name = path
        .as_ref()
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    if name.ends_with(".bcett.byml") || name.ends_with(".bcett.byml.zs") {
        Some(ZstdDictionary::Bcett)
    } else if name.starts_with("pack") || name.ends_with(".pack.zs") {
        Some(ZstdDictionary::Pack)
    // } else if name.starts_with("empty") {
    //     Some(ZstdDictionary::Empty)
    } else if name.ends_with(".zs") {
        Some(ZstdDictionary::Zs)
    } else {
        None
    }
}

pub struct ZsDic {
    pub zs_data: Option<Vec<u8>>,
    pub bcett_data: Option<Vec<u8>>,
    pub packzs_data: Option<Vec<u8>>,
    pub empty_data: Vec<u8>,
}

impl ZsDic {
    const CACHE_FILES: [(&'static str, fn(&ZsDic) -> Option<&Vec<u8>>, &'static str); 3] = [
        (
            "zs.zsdic",
            |dictionaries| dictionaries.zs_data.as_ref(),
            "64FD3636E13E7A741993637A66F7207B56B95BCD8240A75F41D942CB0119006F",
        ),
        (
            "bcett.byml.zsdic",
            |dictionaries| dictionaries.bcett_data.as_ref(),
            "EE052D20D48FEAA75E7AEABC766F0E0EFB3005ACFFBE91208EB8287C51DC48DD",
        ),
        (
            "pack.zsdic",
            |dictionaries| dictionaries.packzs_data.as_ref(),
            "280E1D767DA4DD1113DFE4E73DAA8B9CAB3DB8FF1CD013613D6C6624B6DD06FF",
        ),
    ];

    pub fn empty() -> ZsDic {
        ZsDic {
            zs_data: None,
            bcett_data: None,
            packzs_data: None,
            empty_data: Vec::new(),
        }
    }

    pub fn new(totk_config: Arc<TotkConfig>) -> io::Result<ZsDic> {
        match Self::load_cache() {
            Ok(dictionaries) => Ok(dictionaries),
            Err(cache_error) => match Self::load_from_romfs(&totk_config) {
                Ok(dictionaries) => {
                    dictionaries.validate()?;
                    // Caching is an optimization. A read-only install must still
                    // be able to use the dictionaries loaded from ROMFS.
                    let _ = dictionaries.save_cache();
                    Ok(dictionaries)
                }
                Err(romfs_error) => Err(io::Error::new(
                    romfs_error.kind(),
                    format!(
                        "cached ZSTD dictionaries are unavailable: {cache_error}; {romfs_error}"
                    ),
                )),
            },
        }
    }

    fn load_from_romfs(totk_config: &TotkConfig) -> io::Result<ZsDic> {
        let sarc = ZsDic::get_zsdic_sarc(totk_config)?;
        let empty_data: Vec<u8> = Vec::new();
        let mut zs_data = None;
        let mut bcett_data = None;
        let mut packzs_data = None;

        for file in sarc.files() {
            match file.name.unwrap_or_default() {
                "zs.zsdic" => zs_data = Some(file.data().to_vec()),
                "bcett.byml.zsdic" => bcett_data = Some(file.data().to_vec()),
                "pack.zsdic" => packzs_data = Some(file.data().to_vec()),
                _ => (), // pass for other files
            }
        }
        Ok(ZsDic {
            zs_data,
            bcett_data,
            packzs_data,
            empty_data: empty_data,
        })
    }

    fn is_complete(&self) -> bool {
        self.zs_data.is_some() && self.bcett_data.is_some() && self.packzs_data.is_some()
    }

    fn validate(&self) -> io::Result<()> {
        const ZSTD_DICTIONARY_MAGIC: [u8; 4] = [0x37, 0xA4, 0x30, 0xEC];
        if !self.is_complete() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ZSTD dictionary set is incomplete",
            ));
        }
        for (name, dictionary, expected_sha256) in Self::CACHE_FILES {
            let data = dictionary(self).unwrap();
            if !data.starts_with(&ZSTD_DICTIONARY_MAGIC) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{name} does not have Zstandard dictionary magic"),
                ));
            }
            let actual_sha256 = dictionary_sha256(data);
            if actual_sha256 != expected_sha256 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "{name} failed SHA-256 validation: expected {expected_sha256}, got {actual_sha256}"
                    ),
                ));
            }
        }
        Ok(())
    }

    fn cache_directory() -> PathBuf {
        crate::utils::cache_directory().join("zstd")
    }

    fn save_cache(&self) -> io::Result<()> {
        self.save_to_directory(&Self::cache_directory())
    }

    fn save_to_directory(&self, directory: &Path) -> io::Result<()> {
        self.validate()?;
        fs::create_dir_all(directory)?;
        for (name, dictionary, _) in Self::CACHE_FILES {
            let data = dictionary(self).unwrap();
            fs::write(directory.join(name), data)?;
        }
        Ok(())
    }

    fn load_cache() -> io::Result<ZsDic> {
        Self::load_from_directory(&Self::cache_directory())
    }

    fn load_from_directory(directory: &Path) -> io::Result<ZsDic> {
        let read = |name: &str| fs::read(directory.join(name));
        let dictionaries = ZsDic {
            zs_data: Some(read("zs.zsdic")?),
            bcett_data: Some(read("bcett.byml.zsdic")?),
            packzs_data: Some(read("pack.zsdic")?),
            empty_data: Vec::new(),
        };
        dictionaries.validate()?;
        Ok(dictionaries)
    }

    fn get_zsdic_sarc(totk_config: &TotkConfig) -> io::Result<Sarc> {
        let mut zsdic = PathBuf::from(&totk_config.romfs);
        zsdic.push("Pack/ZsDic.pack.zs");
        if !zsdic.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Zsdic file not found: {:?}", &zsdic),
            ));
        }
        // let _ = check_file_exists(&zsdic)?; //Path().exists()
        let mut zs_file = fs::File::open(&zsdic)?; //with open() as f
        let mut raw_data = Vec::new();
        zs_file.read_to_end(&mut raw_data)?; //f.read()
        let data = TotkZstd::decompress_empty(&raw_data)?;

        Sarc::new(data).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

fn dictionary_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:X}", hasher.finalize())
}

#[allow(dead_code)]
pub struct ZstdDecompressor<'a> {
    totk_config: Arc<TotkConfig>,
    pub packzs: Option<Arc<DecoderDictionary<'a>>>,
    pub zs: Option<Arc<DecoderDictionary<'a>>>,
    pub bcett: Option<Arc<DecoderDictionary<'a>>>,
    pub empty: Arc<DecoderDictionary<'a>>,
    //pub zsdic: ZsDic<'a>
}

impl<'a> ZstdDecompressor<'_> {
    pub fn new(
        totk_config: Arc<TotkConfig>,
        zsdic: Arc<ZsDic>,
    ) -> io::Result<ZstdDecompressor<'a>> {
        let zs = zsdic
            .zs_data
            .as_ref()
            .map(|data| Arc::new(DecoderDictionary::copy(data)));
        let bcett = zsdic
            .bcett_data
            .as_ref()
            .map(|data| Arc::new(DecoderDictionary::copy(data)));
        let packzs = zsdic
            .packzs_data
            .as_ref()
            .map(|data| Arc::new(DecoderDictionary::copy(data)));
        let empty: Arc<DecoderDictionary> = Arc::new(DecoderDictionary::copy(&zsdic.empty_data));

        Ok(ZstdDecompressor {
            totk_config: totk_config,
            packzs: packzs,
            zs: zs,
            bcett: bcett,
            empty: empty,
        })
    }

    fn decompress(&self, data: &[u8], ddict: &DecoderDictionary) -> Result<Vec<u8>, io::Error> {
        if !Magic::is_zstd(data) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "input does not have Zstandard frame magic",
            ));
        }
        if let Ok(decoder) = &mut Decoder::with_prepared_dictionary(data, ddict) {
            let mut decompressed: Vec<u8> = Vec::new();
            if let Ok(_) = decoder.read_to_end(&mut decompressed) {
                return Ok(decompressed);
            }
        }
        let err = format!("Failed to decompress!  line:{}", line!());
        Err(io::Error::new(io::ErrorKind::Other, err))
    }

    pub fn decompress_zs(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        self.decompress(data, self.zs.as_deref().ok_or_else(missing_dictionary)?)
    }

    pub fn decompress_pack(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        self.decompress(data, self.packzs.as_deref().ok_or_else(missing_dictionary)?)
    }

    pub fn decompress_bcett(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        self.decompress(data, self.bcett.as_deref().ok_or_else(missing_dictionary)?)
    }

    #[allow(dead_code)]
    pub fn decompress_empty(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        ZstdDecompressor::decompress(&self, &data, &self.empty)
    }
}

#[allow(dead_code)]
pub struct ZstdCompressor<'a> {
    totk_config: Arc<TotkConfig>,
    pub packzs: Option<Arc<EncoderDictionary<'a>>>,
    pub zs: Option<Arc<EncoderDictionary<'a>>>,
    pub bcett: Option<Arc<EncoderDictionary<'a>>>,
    pub empty: Arc<EncoderDictionary<'a>>,
    // pub zs_cpp: *mut ZSTD_CDict_s,
    // pub bcett_cpp: *mut ZSTD_CDict_s,
    // pub packzs_cpp: *mut ZSTD_CDict_s,
    // pub empty_cpp: *mut ZSTD_CDict_s,
    pub comp_level: i32,
}

#[allow(dead_code)]
impl<'a> ZstdCompressor<'_> {
    pub fn new(
        totk_config: Arc<TotkConfig>,
        zsdic: Arc<ZsDic>,
        comp_level: i32,
    ) -> io::Result<ZstdCompressor<'a>> {
        let zs = zsdic
            .zs_data
            .as_ref()
            .map(|data| Arc::new(EncoderDictionary::copy(data, comp_level)));
        let bcett = zsdic
            .bcett_data
            .as_ref()
            .map(|data| Arc::new(EncoderDictionary::copy(data, comp_level)));
        let packzs = zsdic
            .packzs_data
            .as_ref()
            .map(|data| Arc::new(EncoderDictionary::copy(data, comp_level)));
        let empty: Arc<EncoderDictionary> =
            Arc::new(EncoderDictionary::copy(&zsdic.empty_data, comp_level));

        Ok(ZstdCompressor {
            totk_config: totk_config,
            packzs: packzs,
            zs: zs,
            bcett: bcett,
            empty: empty,
            comp_level: comp_level,
            // zs_cpp: zs_cpp,
            // bcett_cpp: bcett_cpp,
            // packzs_cpp: packzs_cpp,
            // empty_cpp: empty_cpp,
        })
    }

    fn compress(&self, data: &[u8], cdict: &EncoderDictionary) -> io::Result<Vec<u8>> {
        let mut buffer: Vec<u8> = Vec::new();
        let mut encoder = Encoder::with_prepared_dictionary(&mut buffer, cdict)?;
        // The game reads the decompressed size from the frame header, so the
        // streaming encoder must be told the input length up front.
        encoder.set_pledged_src_size(Some(data.len() as u64))?;
        encoder.include_contentsize(true)?;
        encoder.write_all(data)?;
        let compressed_data = encoder.finish()?;
        Ok(compressed_data.to_vec())
    }

    pub fn compress_zs(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        self.compress(data, self.zs.as_deref().ok_or_else(missing_dictionary)?)
    }

    pub fn compress_pack(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        self.compress(data, self.packzs.as_deref().ok_or_else(missing_dictionary)?)
    }

    pub fn compress_bcett(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        self.compress(data, self.bcett.as_deref().ok_or_else(missing_dictionary)?)
    }

    pub fn compress_empty(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        ZstdCompressor::compress(&self, &data, &self.empty)
    }

    pub fn compress_empty_with_level(&self, data: &[u8], level: i32) -> io::Result<Vec<u8>> {
        if level == self.comp_level {
            return self.compress_empty(data);
        }
        let mut buffer: Vec<u8> = Vec::new();
        let mut encoder = Encoder::new(&mut buffer, level)?;
        encoder.set_pledged_src_size(Some(data.len() as u64))?;
        encoder.include_contentsize(true)?;
        encoder.write_all(data)?;
        encoder.finish()?;
        Ok(buffer)
    }

    pub fn find_vanila_file_in_romfs<P: AsRef<Path>>(&self, path: P) -> io::Result<String> {
        let res = crate::utils::LookupData::filename_to_localpath();
        let filename = path
            .as_ref()
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default();
        let file_in_romfs_path = res
            .get(filename)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "File not found"))?;
        let result = PathBuf::from(&self.totk_config.romfs).join(file_in_romfs_path);
        Ok(result.to_string_lossy().to_string())
    }
}

#[inline]
pub fn is_gamedatalist<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref()
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase()
        .starts_with("gamedatalist")
    // path.ends_with("GameDataList.Product.110.byml.zs")
}
#[inline]
pub fn is_tagproduct<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref()
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase()
        .starts_with("tag.product")
    // path.ends_with("GameDataList.Product.110.byml.zs")
}

#[inline]
pub fn is_little_endian(data: &[u8]) -> bool {
    data.len() >= 14 && data.get(12..14) == Some(&[0xFF, 0xFE])
}

pub fn sha256(data: Vec<u8>) -> String {
    // Create a Sha256 object
    let mut hasher = Sha256::new();

    // Write input data
    hasher.update(&data);

    // Read hash digest and consume hasher
    let result = hasher.finalize();
    format!("{:X}", result)
}

#[inline]
pub fn is_esetb<P: AsRef<Path>>(path: P) -> bool {
    let tmp = path.as_ref().to_string_lossy().to_ascii_lowercase();
    tmp.ends_with(".esetb.byml") || tmp.ends_with(".esetb.byml.zs")
}
pub fn get_executable_dir() -> String {
    if cfg!(debug_assertions) {
        let cwd = env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
            .replace("\\", "/");
        if cwd.ends_with("src-tauri") {
            return cwd;
        }
        return "W:/coding/TotkBits/src-tauri".to_string();
    }
    if let Ok(exe_path) = env::current_exe() {
        // Get the directory of the executable
        if let Some(exe_dir) = exe_path.parent() {
            if let Some(exe_dir) = exe_dir.to_str() {
                return exe_dir.to_string().replace("\\", "/");
            }
        }
    }
    return String::new();
}

#[cfg(test)]
mod yaz0_tests {
    use super::{
        preferred_dictionary_for_path, sha256, TotkZstd, ZsDic, ZstdDictionary,
        TOTK_ZSTD_COMPRESSION_LEVEL,
    };
    use crate::TotkConfig::TotkConfig;
    use std::io;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn yaz0_roundtrip_varied_binary_data() {
        let data: Vec<u8> = (0..131_071)
            .map(|index| ((index * 73 + index / 251) & 0xff) as u8)
            .collect();
        let compressed = TotkZstd::compress_yaz0(&data).unwrap();
        assert!(crate::Settings::Magic::is_yaz0(&compressed));
        assert_eq!(TotkZstd::decompress_yaz0(&compressed).unwrap(), data);
    }

    #[test]
    fn yaz0_matches_switch_toolbox_on_mario_zombie_fixture() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_ss/yaz0/MarioZombie.szs");
        if !path.is_file() {
            return;
        }
        let source = std::fs::read(path).unwrap();
        let alignment = TotkZstd::yaz0_alignment(&source);
        let raw = TotkZstd::decompress_yaz0(&source).unwrap();
        let compressed = TotkZstd::compress_yaz0_with_alignment(&raw, alignment).unwrap();
        assert_eq!(compressed, source);
        assert_eq!(
            sha256(compressed),
            "CD0EB66CBED6FCA64011E6964CFB7831AF486E1B88E3D399D4375399FE64247E"
        );
    }

    #[test]
    fn yaz0_roundtrips_switch_toolbox_fixture_directory() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_ss/yaz0");
        if !directory.is_dir() {
            return;
        }

        let mut tested = 0;
        let mut exact_matches = 0;
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if !path.is_file() {
                continue;
            }

            let source = std::fs::read(&path).unwrap();
            assert!(
                crate::Settings::Magic::is_yaz0(&source),
                "{} is not a Yaz0 stream",
                path.display()
            );
            let alignment = TotkZstd::yaz0_alignment(&source);
            let raw = TotkZstd::decompress_yaz0(&source).unwrap();
            let recompressed = TotkZstd::compress_yaz0_with_alignment(&raw, alignment).unwrap();
            let recompressed_hash = sha256(recompressed.clone());
            assert_eq!(
                TotkZstd::decompress_yaz0(&recompressed).unwrap(),
                raw,
                "{} failed its Yaz0 round trip",
                path.display()
            );
            let expected_hash = match path.file_name().and_then(|name| name.to_str()) {
                Some("MarioZombie.szs") => {
                    Some("CD0EB66CBED6FCA64011E6964CFB7831AF486E1B88E3D399D4375399FE64247E")
                }
                Some("MarioZombieEye.szs") => {
                    Some("FDF714891282076C7C5FCE30439710F1C9F90E96306CB5D2244483B94EB1808B")
                }
                Some("MarioZombieFace.szs") => {
                    Some("44E79AB1D53671153F355558176745000F5C04156E7815F1C325F1F6840ADE32")
                }
                _ => None,
            };
            if let Some(expected_hash) = expected_hash {
                assert_eq!(
                    recompressed_hash,
                    expected_hash,
                    "{} changed from the pre-optimization Yaz0 output",
                    path.display()
                );
            }
            if recompressed == source {
                exact_matches += 1;
            } else {
                let first_difference = recompressed
                    .iter()
                    .zip(&source)
                    .position(|(left, right)| left != right)
                    .unwrap_or(recompressed.len().min(source.len()));
                eprintln!(
                    "{}: valid round trip, but level-3 output differs at byte {first_difference:#x} \
                     (fixture {} bytes, recompressed {} bytes, fixture SHA-256 {}, \
                     recompressed SHA-256 {})",
                    path.display(),
                    source.len(),
                    recompressed.len(),
                    sha256(source.clone()),
                    recompressed_hash,
                );
            }
            tested += 1;
        }
        assert!(tested > 0, "the Yaz0 fixture directory is empty");
        eprintln!(
            "{tested} Yaz0 fixtures passed round-trip testing; {exact_matches} exact matches"
        );
    }

    #[test]
    #[ignore = "diagnostic for identifying the Switch Toolbox fixture compression level"]
    fn identify_switch_toolbox_fixture_levels() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_ss/yaz0");
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if !path.is_file() {
                continue;
            }
            let source = std::fs::read(&path).unwrap();
            let raw = TotkZstd::decompress_yaz0(&source).unwrap();
            let alignment = TotkZstd::yaz0_alignment(&source);
            let matches: Vec<_> = (0..=9)
                .filter(|level| {
                    roead::yaz0::compress_with_options(
                        &raw,
                        roead::yaz0::CompressOptions {
                            alignment,
                            compression_level: *level,
                        },
                    ) == source
                })
                .collect();
            eprintln!("{} exact levels: {matches:?}", path.display());
        }
    }

    #[test]
    fn dictionaryless_codec_roundtrips_empty_dictionary_zstd() {
        let codec =
            TotkZstd::dictionaryless(Arc::new(TotkConfig::default()), TOTK_ZSTD_COMPRESSION_LEVEL);
        let data = b"lightweight CLI compression";
        let compressed = codec.compress_empty(data).unwrap();
        assert_eq!(TotkZstd::decompress_empty(&compressed).unwrap(), data);
    }

    #[test]
    fn dictionaryless_zsdic_pack_reports_empty_metadata() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/1/ZsDic.pack.zs");
        if !path.is_file() {
            return;
        }
        let codec =
            TotkZstd::dictionaryless(Arc::new(TotkConfig::default()), TOTK_ZSTD_COMPRESSION_LEVEL);
        let source = std::fs::read(&path).unwrap();
        let (decoded, dictionary) = codec.try_decompress_for_path(&path, &source).unwrap();
        assert_eq!(dictionary, ZstdDictionary::Empty);
        assert!(crate::Settings::Magic::is_sarc(&decoded));

        let mut metadata = crate::Open_and_Save::SendData::default();
        metadata.set_file_metadata(super::TotkFileType::Sarc, Some(dictionary));
        assert_eq!(metadata.file_metadata, "[Sarc] [ZSTD: Empty]");
    }

    #[test]
    fn complete_dictionary_cache_roundtrips_and_rejects_partial_sets() {
        let Ok(dictionaries) = ZsDic::load_cache() else {
            return;
        };
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("zstd-cache-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        dictionaries.save_to_directory(&directory).unwrap();
        let cached = ZsDic::load_from_directory(&directory).unwrap();
        assert_eq!(cached.zs_data, dictionaries.zs_data);
        assert_eq!(cached.bcett_data, dictionaries.bcett_data);
        assert_eq!(cached.packzs_data, dictionaries.packzs_data);

        std::fs::write(directory.join("zs.zsdic"), [0x37, 0xA4, 0x30, 0xEC, 9]).unwrap();
        let error = match ZsDic::load_from_directory(&directory) {
            Ok(_) => panic!("corrupted cached dictionary unexpectedly passed validation"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("SHA-256"));
        dictionaries.save_to_directory(&directory).unwrap();

        std::fs::remove_file(directory.join("pack.zsdic")).unwrap();
        assert!(ZsDic::load_from_directory(&directory).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recognizes_only_standard_zstd_frame_magic() {
        assert!(crate::Settings::Magic::is_zstd(&[
            0x28, 0xB5, 0x2F, 0xFD, 0
        ]));
        assert!(!crate::Settings::Magic::is_zstd(b"Yaz0"));
        assert!(!crate::Settings::Magic::is_zstd(b"plain text"));
    }

    #[test]
    fn filename_selects_preferred_dictionary() {
        assert_eq!(
            preferred_dictionary_for_path("Map.bcett.byml.zs"),
            Some(ZstdDictionary::Bcett)
        );
        assert_eq!(
            preferred_dictionary_for_path("Actor.pack.zs"),
            Some(ZstdDictionary::Pack)
        );
        assert_eq!(
            preferred_dictionary_for_path("PackAnything"),
            Some(ZstdDictionary::Pack)
        );
        assert_eq!(
            preferred_dictionary_for_path("Anything.zs"),
            Some(ZstdDictionary::Zs)
        );
        assert_eq!(preferred_dictionary_for_path("Anything.byml"), None);
    }

    #[test]
    fn dictionary_decompression_rejects_non_zstd_before_decoding() {
        let codec =
            TotkZstd::dictionaryless(Arc::new(TotkConfig::default()), TOTK_ZSTD_COMPRESSION_LEVEL);
        let error = codec
            .try_decompress_using(b"not compressed", ZstdDictionary::Empty)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("frame magic"));
    }
}
