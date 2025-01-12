use crate::file_format::Pack::PackFile;
use crate::Open_and_Save::get_string_from_data;
use crate::TotkConfig::TotkConfig;
use digest::Digest;
use flate2::read::ZlibDecoder;
use roead::sarc::*;
use sha2::Sha256;
use zstd::zstd_safe::zstd_sys::{
    ZSTD_CCtx, ZSTD_CDict_s, ZSTD_compressBound, ZSTD_compress_usingCDict, ZSTD_createCCtx, ZSTD_createCDict, ZSTD_isError
};

use std::collections::HashMap;


use std::path::{Path, PathBuf};
use std::sync::Arc;

//use zstd::zstd_safe::CompressionLevel;
use std::{env, fs};
use std::io::{self, Cursor, Read, Write};
use zstd::dict::{DecoderDictionary, EncoderDictionary};
use zstd::{stream::decode_all, stream::Decoder, stream::Encoder};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TotkFileType {
    AINB,
    ASB,
    Restbl,
    TagProduct,
    Sarc,
    MalsSarc,
    Byml,
    Aamp,
    Msbt,
    Bcett,
    Esetb,
    Text,
    Other,
    //SMO
    SmoSaveFile,
    None,
}

pub struct ZstdCppCompressor {
    pub zs_cpp: *mut ZSTD_CDict_s,
    pub bcett_cpp: *mut ZSTD_CDict_s,
    pub packzs_cpp: *mut ZSTD_CDict_s,
    pub empty_cpp: *mut ZSTD_CDict_s,
    pub cctx: *mut ZSTD_CCtx,
}

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
        let zs_cpp = unsafe {
            ZSTD_createCDict(
                zsdic.zs_data.as_ptr() as *const std::ffi::c_void,
                zsdic.zs_data.len(),
                comp_level,
            )
        };
        let bcett_cpp = unsafe {
            ZSTD_createCDict(
                zsdic.bcett_data.as_ptr() as *const std::ffi::c_void,
                zsdic.bcett_data.len(),
                comp_level,
            )
        };
        let packzs_cpp = unsafe {
            ZSTD_createCDict(
                zsdic.packzs_data.as_ptr() as *const std::ffi::c_void,
                zsdic.packzs_data.len(),
                comp_level,
            )
        };
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
            cctx: cctx,
        }
    }
    pub fn compress(&self, data: &[u8], cdict: *mut ZSTD_CDict_s) -> io::Result<Vec<u8>> {
        // Calculate the maximum compressed size and allocate the buffer
        let dst_capacity = unsafe { ZSTD_compressBound(data.len()) as usize };
        let mut buffer: Vec<u8> = vec![0; dst_capacity]; // Allocate space
        
        // Perform the compression
        let res_size = unsafe {
            ZSTD_compress_usingCDict(
                self.cctx,
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
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "ZSTD compression failed"));
        }
    
        // Truncate the buffer to the actual size of the compressed data
        buffer.truncate(res_size as usize);
    
        Ok(buffer)
    }

    pub fn compress_zs(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        ZstdCppCompressor::compress(&self, &data, self.zs_cpp)
    }

    pub fn compress_pack(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        self.compress( &data, self.packzs_cpp)
    }

    pub fn compress_bcett(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        ZstdCppCompressor::compress(&self, &data, self.bcett_cpp)
    }

    pub fn compress_empty(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        ZstdCppCompressor::compress(&self, &data, self.empty_cpp)
    }
}

pub struct TotkZstd<'a> {
    pub totk_config: Arc<TotkConfig>,
    pub decompressor: ZstdDecompressor<'a>,
    pub compressor: ZstdCompressor<'a>,
    pub zsdic: Arc<ZsDic>,
    pub cpp_compressor: ZstdCppCompressor,
}

impl<'a> TotkZstd<'_> {
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
    pub fn try_decompress(&self, data: &Vec<u8>) -> Result<Vec<u8>, io::Error> {
        // println!("Trying to decompress...");
        let mut dicts: HashMap<String, Arc<DecoderDictionary>> = Default::default();
        dicts.insert("zs".to_string(), self.decompressor.zs.clone());
        dicts.insert("packzs".to_string(), self.decompressor.packzs.clone());
        dicts.insert("empty".to_string(), self.decompressor.empty.clone());
        dicts.insert("bcett".to_string(), self.decompressor.bcett.clone());

        for (name, dictt) in dicts.iter() {
            if let Ok(dec_data) = self.decompressor.decompress(&data, &dictt) {
                // println!("Finally decompressed! Its {} dictionary", name);
                return Ok(dec_data);
            }
        }
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Unable to decompress with any dictionary!",
        ));
    }
    pub fn find_vanila_internal_file_path_in_romfs<P: AsRef<Path>>(&self, internal_path: P) -> io::Result<String> {
        //parse json
        let json_zlibdata = fs::read("bin/totk_internal_filepaths.bin")?;
        let mut decoder = ZlibDecoder::new(&json_zlibdata[..]);
        let mut json_str = String::new();
        decoder.read_to_string(&mut json_str)?;
        let res: HashMap<String, String> = serde_json::from_str(&json_str)?;
        //find the sarc file
        let int_path_str = internal_path.as_ref().to_string_lossy().to_string();
        let sarc_localpath = res.get(&int_path_str).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "File not found"))?;
        let sarc_filepath = PathBuf::from(&self.totk_config.romfs).join(sarc_localpath);
        Ok(sarc_filepath.to_string_lossy().to_string())
    }
    pub fn find_vanila_internal_file_data_from_path<P: AsRef<Path>>(&self, internal_path: P, sarc_filepath: String, zstd: Arc<TotkZstd>) -> io::Result<String> {

        let int_path_str = internal_path.as_ref().to_string_lossy().to_string();
        let sarc_var = PackFile::new(&sarc_filepath, zstd.clone())?;
        //get the bytes
        let rawdata = sarc_var.sarc.get_data(&int_path_str).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("File: {} not found in sarc: {:?}", &int_path_str, &sarc_filepath)))?;
        //parse to string
        let (_, result) = get_string_from_data("".to_string(), rawdata.to_vec(), zstd.clone())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("Unable to parse file:\n{}\n from sarc:\n{:?}", &int_path_str, &sarc_filepath)))?;;
        Ok(result)
    }
    pub fn find_vanila_internal_file_data_in_romfs<P: AsRef<Path>>(&self, internal_path: P, zstd: Arc<TotkZstd>) -> io::Result<String> {
        //parse json
        // let json_zlibdata = fs::read("bin/totk_internal_filepaths.bin")?;
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

}

pub struct ZsDic {
    pub zs_data: Vec<u8>,
    pub bcett_data: Vec<u8>,
    pub packzs_data: Vec<u8>,
    pub empty_data: Vec<u8>,
}

impl ZsDic {
    pub fn new(totk_config: Arc<TotkConfig>) -> io::Result<ZsDic> {
        let sarc = ZsDic::get_zsdic_sarc(&totk_config)?;
        let empty_data: Vec<u8> = Vec::new();
        let mut zs_data: Vec<u8> = Vec::new();
        let mut bcett_data: Vec<u8> = Vec::new();
        let mut packzs_data: Vec<u8> = Vec::new();

        for file in sarc.files() {
            match file.name.unwrap_or_default() {
                "zs.zsdic" => zs_data = file.data().to_vec(),
                "bcett.byml.zsdic" => bcett_data = file.data().to_vec(),
                "pack.zsdic" => packzs_data = file.data().to_vec(),
                _ => (), // pass for other files
            }
        }
        Ok(ZsDic {
            zs_data: zs_data,
            bcett_data: bcett_data,
            packzs_data: packzs_data,
            empty_data: empty_data,
        })
    }

    fn get_zsdic_sarc(totk_config: &TotkConfig) -> io::Result<Sarc> {
        let mut zsdic = PathBuf::from(&totk_config.romfs);
        zsdic.push("Pack/ZsDic.pack.zs");
        if !zsdic.exists() {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("Zsdic file not found: {:?}", &zsdic)));
        }
        // let _ = check_file_exists(&zsdic)?; //Path().exists()
        let mut zs_file = fs::File::open(&zsdic)?; //with open() as f
        let mut raw_data = Vec::new();
        zs_file.read_to_end(&mut raw_data)?; //f.read()
        let cursor = Cursor::new(&raw_data);
        let data = decode_all(cursor)?;

        Sarc::new(data).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

#[allow(dead_code)]
pub struct ZstdDecompressor<'a> {
    totk_config: Arc<TotkConfig>,
    pub packzs: Arc<DecoderDictionary<'a>>, //Vec<u8>,
    pub zs: Arc<DecoderDictionary<'a>>,     //Vec<u8>,
    pub bcett: Arc<DecoderDictionary<'a>>,  //Vec<u8>,
    pub empty: Arc<DecoderDictionary<'a>>,
    //pub zsdic: ZsDic<'a>
}

impl<'a> ZstdDecompressor<'_> {
    pub fn new(
        totk_config: Arc<TotkConfig>,
        zsdic: Arc<ZsDic>,
    ) -> io::Result<ZstdDecompressor<'a>> {
        let zs: Arc<DecoderDictionary> = Arc::new(DecoderDictionary::copy(&zsdic.zs_data));
        let bcett: Arc<DecoderDictionary> = Arc::new(DecoderDictionary::copy(&zsdic.bcett_data));
        let packzs: Arc<DecoderDictionary> = Arc::new(DecoderDictionary::copy(&zsdic.packzs_data));
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
        ZstdDecompressor::decompress(&self, &data, &self.zs)
    }

    pub fn decompress_pack(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        ZstdDecompressor::decompress(&self, &data, &self.packzs)
    }

    pub fn decompress_bcett(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        ZstdDecompressor::decompress(&self, &data, &self.bcett)
    }

    #[allow(dead_code)]
    pub fn decompress_empty(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        ZstdDecompressor::decompress(&self, &data, &self.empty)
    }
}

#[allow(dead_code)]
pub struct ZstdCompressor<'a> {
    totk_config: Arc<TotkConfig>,
    pub packzs: Arc<EncoderDictionary<'a>>, //Vec<u8>,
    pub zs: Arc<EncoderDictionary<'a>>,     //Vec<u8>,
    pub bcett: Arc<EncoderDictionary<'a>>,  //Vec<u8>,
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
        let zs: Arc<EncoderDictionary> =
            Arc::new(EncoderDictionary::copy(&zsdic.zs_data, comp_level));
        let bcett: Arc<EncoderDictionary> =
            Arc::new(EncoderDictionary::copy(&zsdic.bcett_data, comp_level));
        let packzs: Arc<EncoderDictionary> =
            Arc::new(EncoderDictionary::copy(&zsdic.packzs_data, comp_level));
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
        encoder.write_all(data)?;
        let compressed_data = encoder.finish()?;
        Ok(compressed_data.to_vec())
    }

    pub fn compress_zs(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        ZstdCompressor::compress(&self, &data, &self.zs)
    }

    pub fn compress_pack(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        ZstdCompressor::compress(&self, &data, &self.packzs)
    }

    pub fn compress_bcett(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        ZstdCompressor::compress(&self, &data, &self.bcett)
    }

    pub fn compress_empty(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        ZstdCompressor::compress(&self, &data, &self.empty)
    }

    pub fn find_vanila_file_in_romfs<P: AsRef<Path>>(&self, path: P) -> io::Result<String> {
        let json_zlibdata = fs::read("bin/totk_filename_to_localpath.bin")?;
        let mut decoder = ZlibDecoder::new(&json_zlibdata[..]);
        let mut json_str = String::new();
        decoder.read_to_string(&mut json_str)?;
        let res: HashMap<String, String> = serde_json::from_str(&json_str)?;
        let filename = path.as_ref().file_name().unwrap_or_default().to_str().unwrap_or_default();
        let file_in_romfs_path = res.get(filename).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "File not found"))?;
        let result = PathBuf::from(&self.totk_config.romfs).join(file_in_romfs_path);
        Ok(result.to_string_lossy().to_string())
    }

   
}

#[inline]
pub fn is_byml(data: &[u8]) -> bool {
    data.starts_with(b"BY") || data.starts_with(b"YB")
}

#[inline]
pub fn is_sarc(data: &[u8]) -> bool {
    data.starts_with(b"SARC")
}

#[inline]
pub fn is_aamp(data: &[u8]) -> bool {
    data.starts_with(b"AAMP")
}

#[inline]
pub fn is_msyt(data: &[u8]) -> bool {
    data.starts_with(b"MsgStd")
}
#[inline]
pub fn is_ainb(data: &[u8]) -> bool {
    data.starts_with(b"AIB")
}
#[inline]
pub fn is_asb(data: &[u8]) -> bool {
    data.starts_with(b"ASB ")
}

#[inline]
pub fn is_evfl(data: &[u8]) -> bool {
    data.starts_with(b"BFEVFL")
}

#[inline]
pub fn is_restbl(data: &[u8]) -> bool {
    data.starts_with(b"RSTB") || data.starts_with(b"REST")
}

#[inline]
pub fn is_gamedatalist<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase().starts_with("gamedatalist")
    // path.ends_with("GameDataList.Product.110.byml.zs")
}
#[inline]
pub fn is_tagproduct<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase().starts_with("tag.product")
    // path.ends_with("GameDataList.Product.110.byml.zs")
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
        let cwd = env::current_dir().unwrap_or_default().to_string_lossy().to_string().replace("\\", "/");
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