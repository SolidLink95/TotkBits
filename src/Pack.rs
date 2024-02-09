use roead;
use roead::sarc::{Sarc, SarcWriter};
use std::fs;
use std::io::{self, Error, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
//mod Zstd;
use crate::BymlEntries::ActorParam;
use crate::TotkPath::TotkPath;
use crate::Zstd::{totk_zstd, ZstdCompressor, ZstdDecompressor};

pub struct PackFile<'a> {
    path: String,
    totk_path: Arc<TotkPath>,
    zstd:  Arc<totk_zstd<'a>>,
    //decompressor: &'a ZstdDecompressor<'a>,
    //compressor: &'a ZstdCompressor<'a>,
    //raw_data: Vec<u8>,
    pub writer: SarcWriter,
    pub sarc: Sarc<'a>,
}


impl<'a> PackFile<'_> {
    pub fn new(
        path: String,
        //totk_path: Arc<TotkPath>,
        zstd:  Arc<totk_zstd<'a>>,
        //decompressor: &'a ZstdDecompressor,
        //compressor: &'a ZstdCompressor
    ) -> io::Result<PackFile<'a>> {
        let raw_data = PackFile::sarc_file_to_bytes(&PathBuf::from(path.clone()), &zstd.clone())?;
        let sarc: Sarc = Sarc::new(raw_data.clone()).expect("Failed");
        let writer: SarcWriter = SarcWriter::from_sarc(&sarc);

        Ok(PackFile {
            path: path,
            totk_path: zstd.totk_path.clone(),
            zstd: zstd.clone(),
            //decompressor: decompressor,
            //compressor: compressor,
            writer: writer,
            sarc: sarc,
        })
    }



    //Get totk actor entries recursively

    //Save the sarc file, compress if file ends with .zs, create directory if needed
    pub fn save(&mut self, dest_file: String) -> io::Result<()> {
        let file_path: &Path = Path::new(&dest_file);
        let directory: &Path = file_path.parent().expect("Cannot get parent of the file");
        fs::create_dir_all(directory)?;
        let mut data: Vec<u8> = self.writer.to_binary();
        if dest_file.to_lowercase().ends_with(".zs") {
            data = self.zstd.compressor.compress_pack(&data)?;
        }
        let mut file_handle: fs::File = fs::File::create(file_path)?;
        file_handle.write_all(&data)?;
        Ok(())
    }

    //Read sarc file's bytes, decompress if needed
    fn sarc_file_to_bytes(path: &PathBuf, zstd: &'a totk_zstd) -> Result<Vec<u8>, io::Error> {
        let mut fHandle: fs::File = fs::File::open(path)?;
        let mut buffer: Vec<u8> = Vec::new();
        fHandle.read_to_end(&mut buffer)?;
        if !buffer.as_slice().starts_with(b"SARC") {
            match zstd
                .decompressor
                .decompress_pack(&buffer) {
                Ok(res) => {
                    return Ok(res);
                },
                Err(err) => {
                    eprintln!("Error during zstd decompress");
                    return Err(err);
                }
            }
        }
        Ok(buffer)
    }
}


struct opened_file {
    file_path: String,

}