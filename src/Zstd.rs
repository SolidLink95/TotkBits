use crate::TotkPath::TotkPath;
use roead::sarc::*;
use serde::de;
use std::path::PathBuf;
use std::sync::Arc;
//use zstd::zstd_safe::CompressionLevel;
use crate::misc::check_file_exists;
use std::fs;
use std::io::{self, Cursor, Read, Write};
use zstd::dict::{DDict, DecoderDictionary, EncoderDictionary};
use zstd::{dict::CDict, stream::decode_all, stream::Decoder, stream::Encoder};

enum FileType {
    Sarc,
    Byml,
    Aamp,
    Bcett,
    Other,
}

pub struct totk_zstd<'a> {
    pub totk_path: Arc<TotkPath>,
    pub decompressor: ZstdDecompressor<'a>,
    pub compressor: ZstdCompressor<'a>,
}

impl<'a> totk_zstd<'_> {
    pub fn new(totk_path: Arc<TotkPath>, comp_level: i32) -> io::Result<totk_zstd<'a>> {
        let zsdic = Arc::new(ZsDic::new(totk_path.clone())?);
        let decompressor: ZstdDecompressor =
            ZstdDecompressor::new(totk_path.clone(), zsdic.clone())?;
        let compressor: ZstdCompressor = ZstdCompressor::new(totk_path.clone(), zsdic, comp_level)?;

        Ok(totk_zstd {
            totk_path,
            decompressor,
            compressor,
        })
    }

    pub fn identify_file_from_binary(&self, data: Vec<u8>) -> FileType {
        //assume its not compressed
        if is_byml(&data) {
            return FileType::Bcett;
        }
        if is_sarc(&data) {
            return FileType::Sarc;
        }
        if is_aamp(&data) {
            return FileType::Aamp;
        }
        //check if compressed
        match self.decompressor.decompress_bcett(&data) {
            Ok(_) => {
                if is_byml(&data) {
                    return FileType::Bcett;
                }
            }
            Err(_) => {}
        };
        match self.decompressor.decompress_pack(&data) {
            Ok(_) => {
                if is_sarc(&data) {
                    return FileType::Sarc;
                }
            }
            Err(_) => {}
        };
        match self.decompressor.decompress_zs(&data) {
            Ok(_) => {
                if is_byml(&data) {
                    return FileType::Bcett;
                }
                if is_sarc(&data) {
                    return FileType::Sarc;
                }
                if is_aamp(&data) {
                    return FileType::Aamp;
                }
            }
            Err(_) => {}
        };
        match self.decompressor.decompress_empty(&data) {
            Ok(_) => {
                if is_byml(&data) {
                    return FileType::Bcett;
                }
                if is_sarc(&data) {
                    return FileType::Sarc;
                }
                if is_aamp(&data) {
                    return FileType::Aamp;
                }
            }
            Err(_) => {}
        };
        //all validations failed
        return FileType::Other;
    }
}

pub struct ZsDic {
    pub zs_data: Vec<u8>,
    pub bcett_data: Vec<u8>,
    pub packzs_data: Vec<u8>,
    pub empty_data: Vec<u8>,
}

impl ZsDic {
    pub fn new(totk_path: Arc<TotkPath>) -> io::Result<ZsDic> {
        let sarc = ZsDic::get_zsdic_sarc(&totk_path)?;
        let empty_data: Vec<u8> = Vec::new();
        let mut zs_data: Vec<u8> = Vec::new();
        let mut bcett_data: Vec<u8> = Vec::new();
        let mut packzs_data: Vec<u8> = Vec::new();

        for file in sarc.files() {
            match file.name.unwrap_or("") {
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

    fn get_zsdic_sarc(totk_path: &TotkPath) -> io::Result<Sarc> {
        let mut zsdic = totk_path.romfs.clone();
        zsdic.push("Pack/ZsDic.pack.zs");
        let _ = check_file_exists(&zsdic)?; //Path().exists()
        let mut zsFile = fs::File::open(&zsdic)?; //with open() as f
        let mut rawData = Vec::new();
        zsFile.read_to_end(&mut rawData)?; //f.read()
        let cursor = Cursor::new(&rawData);
        let data = decode_all(cursor)?;

        Sarc::new(data).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

pub struct ZstdDecompressor<'a> {
    totk_path: Arc<TotkPath>,
    pub packzs: DecoderDictionary<'a>, //Vec<u8>,
    pub zs: DecoderDictionary<'a>,     //Vec<u8>,
    pub bcett: DecoderDictionary<'a>,  //Vec<u8>,
    pub empty: DecoderDictionary<'a>,
    //pub zsdic: ZsDic<'a>
}

impl<'a> ZstdDecompressor<'_> {
    pub fn new(totk_path: Arc<TotkPath>, zsdic: Arc<ZsDic>) -> io::Result<ZstdDecompressor<'a>> {
        let zs: DecoderDictionary = DecoderDictionary::copy(&zsdic.zs_data);
        let bcett: DecoderDictionary = DecoderDictionary::copy(&zsdic.bcett_data);
        let packzs: DecoderDictionary = DecoderDictionary::copy(&zsdic.packzs_data);
        let empty: DecoderDictionary = DecoderDictionary::copy(&zsdic.empty_data);

        Ok(ZstdDecompressor {
            totk_path: totk_path,
            packzs: packzs,
            zs: zs,
            bcett: bcett,
            empty: empty,
        })
    }

    fn decompress(&self, data: &[u8], ddict: &DecoderDictionary) -> Result<Vec<u8>, io::Error> {
        let mut decoder = Decoder::with_prepared_dictionary(data, ddict);
        match decoder {
            Ok(_) => {}
            Err(err) => {
                eprintln!("Error getting the decoder");
                return Err(err);
            }
        }
        let mut decompressed = Vec::new();
        match decoder.unwrap().read_to_end(&mut decompressed) {
            Ok(_) => {
                return Ok(decompressed);
            }
            Err(err) => {
                eprintln!("Error while decoding");
                return Err(err);
            }
        }
        Ok(decompressed)
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

    pub fn decompress_empty(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        ZstdDecompressor::decompress(&self, &data, &self.empty)
    }
}

pub struct ZstdCompressor<'a> {
    totk_path: Arc<TotkPath>,
    pub packzs: EncoderDictionary<'a>, //Vec<u8>,
    pub zs: EncoderDictionary<'a>,     //Vec<u8>,
    pub bcett: EncoderDictionary<'a>,  //Vec<u8>,
    pub empty: EncoderDictionary<'a>,
    pub comp_level: i32,
}

impl<'a> ZstdCompressor<'_> {
    pub fn new(
        totk_path: Arc<TotkPath>,
        zsdic: Arc<ZsDic>,
        comp_level: i32,
    ) -> io::Result<ZstdCompressor<'a>> {
        let zs: EncoderDictionary = EncoderDictionary::copy(&zsdic.zs_data, comp_level);
        let bcett: EncoderDictionary = EncoderDictionary::copy(&zsdic.bcett_data, comp_level);
        let packzs: EncoderDictionary = EncoderDictionary::copy(&zsdic.packzs_data, comp_level);
        let empty: EncoderDictionary = EncoderDictionary::copy(&zsdic.empty_data, comp_level);

        Ok(ZstdCompressor {
            totk_path: totk_path,
            packzs: packzs,
            zs: zs,
            bcett: bcett,
            empty: empty,
            comp_level: comp_level,
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
}

pub fn is_byml(data: &[u8]) -> bool {
    if data.starts_with(b"BY") || data.starts_with(b"YB") {
        return true;
    }
    return false;
}

pub fn is_sarc(data: &[u8]) -> bool {
    if data.starts_with(b"SARC") {
        return true;
    }
    return false;
}

pub fn is_aamp(data: &[u8]) -> bool {
    if data.starts_with(b"AAMP") {
        return true;
    }
    return false;
}
