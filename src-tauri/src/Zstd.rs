use crate::TotkConfig::TotkConfig;
use digest::Digest;
use roead::sarc::*;
use sha2::Sha256;

use std::collections::HashMap;

use std::sync::Arc;

//use zstd::zstd_safe::CompressionLevel;
use crate::Settings::check_file_exists;
use std::fs;
use std::io::{self, Cursor, Read, Write};
use zstd::dict::{DecoderDictionary, EncoderDictionary};
use zstd::{stream::decode_all, stream::Decoder, stream::Encoder};

#[derive(Debug,PartialEq, Clone, Copy)]
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
    Text,
    Other,
    None,
}

pub struct TotkZstd<'a> {
    pub totk_config: Arc<TotkConfig>,
    pub decompressor: ZstdDecompressor<'a>,
    pub compressor: ZstdCompressor<'a>,
}

impl<'a> TotkZstd<'_> {
    pub fn new(totk_config: Arc<TotkConfig>, comp_level: i32) -> io::Result<TotkZstd<'a>> {
        let zsdic: Arc<ZsDic> = Arc::new(ZsDic::new(totk_config.clone())?);
        let decompressor: ZstdDecompressor =
            ZstdDecompressor::new(totk_config.clone(), zsdic.clone())?;
        let compressor: ZstdCompressor = ZstdCompressor::new(totk_config.clone(), zsdic, comp_level)?;

        Ok(TotkZstd {
            totk_config,
            decompressor,
            compressor,
        })
    }
    pub fn try_decompress(&self, data: &Vec<u8>) -> Result<Vec<u8>, io::Error> {
        println!("Trying to decompress...");
        let mut dicts: HashMap<String, Arc<DecoderDictionary>> = Default::default();
        dicts.insert("zs".to_string(), self.decompressor.zs.clone());
        dicts.insert("packzs".to_string(), self.decompressor.packzs.clone());
        dicts.insert("empty".to_string(), self.decompressor.empty.clone());
        dicts.insert("bcett".to_string(), self.decompressor.bcett.clone());

        for (name, dictt) in dicts.iter() {
            if let Ok(dec_data) = self.decompressor.decompress(&data, &dictt) {
                println!("Finally decompressed! Its {} dictionary", name);
                return Ok(dec_data);
            }
        }
        return Err(io::Error::new(io::ErrorKind::Other, "Unable to decompress with any dictionary!"));
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
        let mut zsdic = totk_config.romfs.clone();
        zsdic.push("Pack/ZsDic.pack.zs");
        let _ = check_file_exists(&zsdic)?; //Path().exists()
        let mut zs_file = fs::File::open(&zsdic)?; //with open() as f
        let mut raw_data = Vec::new();
        zs_file.read_to_end(&mut raw_data)?; //f.read()
        let cursor = Cursor::new(&raw_data);
        let data = decode_all(cursor)?;

        Sarc::new(data).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

pub struct ZstdDecompressor<'a> {
    totk_config: Arc<TotkConfig>,
    pub packzs: Arc<DecoderDictionary<'a>>, //Vec<u8>,
    pub zs: Arc<DecoderDictionary<'a>>,     //Vec<u8>,
    pub bcett: Arc<DecoderDictionary<'a>>,  //Vec<u8>,
    pub empty: Arc<DecoderDictionary<'a>>,
    //pub zsdic: ZsDic<'a>
}

impl<'a> ZstdDecompressor<'_> {
    pub fn new(totk_config: Arc<TotkConfig>, zsdic: Arc<ZsDic>) -> io::Result<ZstdDecompressor<'a>> {
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
        if let Ok(decoder) = &mut Decoder::with_prepared_dictionary(data, ddict){
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

    pub fn decompress_empty(&self, data: &[u8]) -> Result<Vec<u8>, io::Error> {
        ZstdDecompressor::decompress(&self, &data, &self.empty)
    }
}

pub struct ZstdCompressor<'a> {
    totk_config: Arc<TotkConfig>,
    pub packzs: Arc<EncoderDictionary<'a>>, //Vec<u8>,
    pub zs: Arc<EncoderDictionary<'a>>,     //Vec<u8>,
    pub bcett: Arc<EncoderDictionary<'a>>,  //Vec<u8>,
    pub empty: Arc<EncoderDictionary<'a>>,
    pub comp_level: i32,
}

impl<'a> ZstdCompressor<'_> {
    pub fn new(
        totk_config: Arc<TotkConfig>,
        zsdic: Arc<ZsDic>,
        comp_level: i32,
    ) -> io::Result<ZstdCompressor<'a>> {
        let zs: Arc<EncoderDictionary> = Arc::new(EncoderDictionary::copy(&zsdic.zs_data, comp_level));
        let bcett: Arc<EncoderDictionary> = Arc::new(EncoderDictionary::copy(&zsdic.bcett_data, comp_level));
        let packzs: Arc<EncoderDictionary> = Arc::new(EncoderDictionary::copy(&zsdic.packzs_data, comp_level));
        let empty: Arc<EncoderDictionary> = Arc::new(EncoderDictionary::copy(&zsdic.empty_data, comp_level));

        Ok(ZstdCompressor {
            totk_config: totk_config,
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
    data.starts_with(b"BY") || data.starts_with(b"YB") 
}

pub fn is_sarc(data: &[u8]) -> bool {
    data.starts_with(b"SARC") 
}

pub fn is_aamp(data: &[u8]) -> bool {
    data.starts_with(b"AAMP") 
}

pub fn is_msyt(data: &[u8]) -> bool {
    data.starts_with(b"MsgStd") 
}
pub fn is_ainb(data: &[u8]) -> bool {
    data.starts_with(b"AIB ") 
}
pub fn is_asb(data: &[u8]) -> bool {
    data.starts_with(b"ASB ") 
}

pub fn is_restbl(data: &[u8]) -> bool {
    data.starts_with(b"RSTB") || data.starts_with(b"REST")  
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