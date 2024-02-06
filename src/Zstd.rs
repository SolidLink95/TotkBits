use std::sync::Arc;
use std::path::PathBuf;
use crate::TotkPath::TotkPath;
use roead::sarc::*;
//use zstd::zstd_safe::CompressionLevel;
use std::fs;
use std::io::{self, Cursor, Read, Write};
use zstd::{dict::CDict, stream::Encoder, stream::Decoder, stream::decode_all};
use zstd::dict::{DDict, DecoderDictionary, EncoderDictionary};
use crate::misc::check_file_exists;

pub struct totk_zstd<'a> {
    pub decompressor: ZstdDecompressor<'a>,
    pub compressor: ZstdCompressor<'a>
}

impl<'a> totk_zstd<'_> {
    pub fn new(totk_path: &TotkPath, comp_level: i32) -> io::Result<totk_zstd> {
        let zsdic = Arc::new(ZsDic::new(totk_path)?);
        let decompressor: ZstdDecompressor = ZstdDecompressor::new(totk_path, zsdic.clone())?;
        let compressor: ZstdCompressor = ZstdCompressor::new(totk_path, zsdic, comp_level)?;

        Ok(totk_zstd {
            decompressor: decompressor,
            compressor: compressor
        })
    }
}


pub struct ZsDic {
    pub zs_data: Vec<u8>,
    pub bcett_data: Vec<u8>,
    pub packzs_data: Vec<u8>,
    pub empty_data: Vec<u8>
}

impl ZsDic {
    pub fn new(totk_path: &TotkPath) -> io::Result<ZsDic> {
        let sarc = ZsDic::get_zsdic_sarc(&totk_path)?;
        let empty_data: Vec<u8> = Vec::new();
        let mut zs_data: Vec<u8> = Vec::new();
        let mut bcett_data: Vec<u8> = Vec::new();
        let mut packzs_data: Vec<u8> = Vec::new();

        for file in sarc.files() {
            match file.name.unwrap_or("") {
                "zs.zsdic" =>           zs_data = file.data().to_vec(),
                "bcett.byml.zsdic" =>   bcett_data = file.data().to_vec(),
                "pack.zsdic" =>         packzs_data = file.data().to_vec(),
                _ => (), // pass for other files
            }
        }
        Ok(ZsDic {
            zs_data: zs_data,
            bcett_data: bcett_data,
            packzs_data: packzs_data,
            empty_data: empty_data
        })
    
    }

    //pub fn clone(&self) -> io::Result<ZsDic>{
    //    Ok(ZsDic {
    //        zs_data: self.zs_data.clone(),
    //        bcett_data: self.bcett_data.clone(),
    //        packzs_data: self.packzs_data.clone(),
    //        empty_data: self.empty_data.clone()
    //    })
    //}

    fn get_zsdic_sarc(totk_path: &TotkPath) -> io::Result<Sarc> {
        let mut zsdic = totk_path.romfs.clone();
        zsdic.push("Pack/ZsDic.pack.zs");
        let _ = check_file_exists(&zsdic)?; //Path().exists()
        let mut zsFile = fs::File::open(&zsdic)?; //with open() as f
        let mut rawData = Vec::new();
        zsFile.read_to_end(&mut rawData)?; //f.read()
        let cursor = Cursor::new(&rawData);
        let data = decode_all(cursor)?;
    
        Sarc::new(data).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e)
        }) 
    }
}


pub struct ZstdDecompressor<'a> {
    totk_path: &'a TotkPath,
    pub packzs: DecoderDictionary<'a>,//Vec<u8>,
    pub zs:  DecoderDictionary<'a>,//Vec<u8>,
    pub bcett: DecoderDictionary<'a>,//Vec<u8>,
    pub empty: DecoderDictionary<'a>,
    //pub zsdic: ZsDic<'a>
}

impl<'a> ZstdDecompressor<'_> {
    pub fn new(totk_path: &'a TotkPath, zsdic: Arc<ZsDic>) -> io::Result<ZstdDecompressor<'a>> {
        let zs: DecoderDictionary =     DecoderDictionary::copy(&zsdic.zs_data);
        let bcett: DecoderDictionary =  DecoderDictionary::copy(&zsdic.bcett_data);
        let packzs: DecoderDictionary = DecoderDictionary::copy(&zsdic.packzs_data);
        let empty: DecoderDictionary =      DecoderDictionary::copy(&zsdic.empty_data);

        Ok(ZstdDecompressor {
            totk_path: totk_path,
            packzs: packzs,
            zs: zs,
            bcett: bcett,
            empty: empty,
        })
    

    }
    
    fn decompress(&self, data: &[u8], ddict: &DecoderDictionary) -> Result<Vec<u8>, String> {
        let mut decoder = Decoder::with_prepared_dictionary(
            data, 
            ddict
        ).expect("Error");
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| e.to_string())?;
        Ok(decompressed)
    }

    pub fn decompress_zs(&self, data: &[u8]) -> Result<Vec<u8>, String>{
        ZstdDecompressor::decompress(&self, &data, &self.zs)
    }

    pub fn decompress_pack(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        ZstdDecompressor::decompress(&self, &data, &self.packzs)
    }

    pub fn decompress_bcett(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        ZstdDecompressor::decompress(&self, &data, &self.bcett)
    }


}




pub struct ZstdCompressor<'a> {
    totk_path: &'a TotkPath,
    pub packzs: EncoderDictionary<'a>,//Vec<u8>,
    pub zs:  EncoderDictionary<'a>,//Vec<u8>,
    pub bcett: EncoderDictionary<'a>,//Vec<u8>,
    pub empty: EncoderDictionary<'a>,
    pub comp_level: i32
}

impl<'a> ZstdCompressor<'_> {
    pub fn new(totk_path: &'a TotkPath, zsdic: Arc<ZsDic>, comp_level: i32) -> io::Result<ZstdCompressor<'a>> {
        let zs: EncoderDictionary =     EncoderDictionary::copy(&zsdic.zs_data, comp_level);
        let bcett: EncoderDictionary =  EncoderDictionary::copy(&zsdic.bcett_data, comp_level);
        let packzs: EncoderDictionary = EncoderDictionary::copy(&zsdic.packzs_data, comp_level);
        let empty: EncoderDictionary =      EncoderDictionary::copy(&zsdic.empty_data, comp_level);
        

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
        let mut encoder = Encoder::with_prepared_dictionary(
            &mut buffer, 
            cdict
        )?;
        encoder.write_all(data)?;
        let compressed_data = encoder.finish()?;
        Ok(compressed_data.to_vec())

    }


    pub fn compress_zs(&self, data: &[u8]) -> io::Result<Vec<u8>>{
        ZstdCompressor::compress(&self, &data, &self.zs)
    }
    
    pub fn compress_pack(&self, data: &[u8]) -> io::Result<Vec<u8>>{
        ZstdCompressor::compress(&self, &data, &self.packzs)
    }

    pub fn compress_bcett(&self, data: &[u8]) -> io::Result<Vec<u8>>{
        ZstdCompressor::compress(&self, &data, &self.bcett)
    }


}


