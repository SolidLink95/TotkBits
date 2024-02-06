
use std::path::PathBuf;
use crate::TotkPath::TotkPath;
use roead::sarc::*;
//use zstd::zstd_safe::CompressionLevel;
use std::fs;
use std::io::{self, Cursor, Read, Write};
use zstd::{dict::CDict, stream::Encoder, stream::Decoder, stream::decode_all};
use zstd::dict::{DDict, DecoderDictionary, EncoderDictionary};
use crate::misc::check_file_exists;

pub struct ZstdDecompressor<'a> {
    totkPath: &'a TotkPath,
    pub packzs: DecoderDictionary<'a>,//Vec<u8>,
    pub zs:  DecoderDictionary<'a>,//Vec<u8>,
    pub bcett: DecoderDictionary<'a>,//Vec<u8>,
    pub empty: DecoderDictionary<'a>
}

impl<'a> ZstdDecompressor<'_> {
    pub fn new(totkPath: &'a TotkPath) -> io::Result<ZstdDecompressor<'a>> {
        let empty_u8 : [u8; 0] = [];
        let mut zs: DecoderDictionary =     DecoderDictionary::copy(&empty_u8);
        let mut bcett: DecoderDictionary =  DecoderDictionary::copy(&empty_u8);
        let mut packzs: DecoderDictionary = DecoderDictionary::copy(&empty_u8);
        let empty: DecoderDictionary =      DecoderDictionary::copy(&empty_u8);
        let sarc = get_sarc(&totkPath)?;
        for file in sarc.files() {
            //println!("{}", file.name().unwrap());
            //let x: DecoderDictionary = DecoderDictionary::copy(file.data());
            match file.name.unwrap() {
                //"zs.zsdic" =>           zs = DDict::create(file.data()),
                "zs.zsdic" =>           zs = DecoderDictionary::copy(file.data()),
                "bcett.byml.zsdic" =>   bcett = DecoderDictionary::copy(file.data()),
                "pack.zsdic" =>         packzs = DecoderDictionary::copy(file.data()),
                _ => (), // pass for other files
            }
        }


        Ok(ZstdDecompressor {
            totkPath: totkPath,
            packzs: packzs,
            zs: zs,
            bcett: bcett,
            empty: empty
        })
    

    }
    
    fn decompress(&self, data: &[u8], ddict: &DecoderDictionary) -> Result<Vec<u8>, String> {
        let mut decoder = Decoder::with_prepared_dictionary(
            data, 
            ddict
        //).map_err(|e| e.to_string())?;
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
    totkPath: &'a TotkPath,
    pub packzs: EncoderDictionary<'a>,//Vec<u8>,
    pub zs:  EncoderDictionary<'a>,//Vec<u8>,
    pub bcett: EncoderDictionary<'a>,//Vec<u8>,
    pub empty: EncoderDictionary<'a>,
    pub comp_level: i32
}

impl<'a> ZstdCompressor<'_> {
    pub fn new(totkPath: &'a TotkPath, comp_level: i32) -> io::Result<ZstdCompressor<'a>> {
        let empty_u8 : [u8; 0] = [];
        let mut zs: EncoderDictionary =     EncoderDictionary::copy(&empty_u8, comp_level);
        let mut bcett: EncoderDictionary =  EncoderDictionary::copy(&empty_u8, comp_level);
        let mut packzs: EncoderDictionary = EncoderDictionary::copy(&empty_u8, comp_level);
        let empty: EncoderDictionary =      EncoderDictionary::copy(&empty_u8, comp_level);
        let sarc = get_sarc(&totkPath)?;
        for file in sarc.files() {
            match file.name.unwrap() {
                "zs.zsdic" =>           zs = EncoderDictionary::copy(file.data(), comp_level),
                "bcett.byml.zsdic" =>   bcett = EncoderDictionary::copy(file.data(), comp_level),
                "pack.zsdic" =>         packzs = EncoderDictionary::copy(file.data(), comp_level),
                _ => (), // pass for other files
            }
        }


        Ok(ZstdCompressor {
            totkPath: totkPath,
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


fn get_sarc(totkPath: &TotkPath) -> io::Result<Sarc> {
    let mut zsdic = totkPath.romfs.clone();
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