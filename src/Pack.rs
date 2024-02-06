use roead;
use std::path::{Path, PathBuf};
use std::fs;
use std::io::{self, Error, ErrorKind, Read, Write};
//mod Zstd;
use crate::Zstd::{ZstdDecompressor, ZstdCompressor};
use crate::TotkPath::TotkPath;
use crate::BymlEntries::ActorParam;

pub struct PackFile<'a> {
    path: &'a PathBuf,
    totkPath: &'a TotkPath,
    decompressor: &'a ZstdDecompressor<'a>,
    compressor: &'a ZstdCompressor<'a>,
    //raw_data: Vec<u8>,
    pub writer: roead::sarc::SarcWriter,
    pub sarc: roead::sarc::Sarc<'a>
}

impl<'a> PackFile<'_> {
    pub fn new(
            path: &'a PathBuf, 
            totkPath: &'a TotkPath,
            decompressor: &'a ZstdDecompressor,
            compressor: &'a ZstdCompressor
        ) -> io::Result<PackFile<'a>> {
        let raw_data = PackFile::sarc_file_to_bytes(path, decompressor)?;
        let sarc: roead::sarc::Sarc = roead::sarc::Sarc::new(raw_data.clone()).expect("Failed");
        let writer: roead::sarc::SarcWriter = roead::sarc::SarcWriter::from_sarc(&sarc);

        Ok(
            PackFile {
                path: path,
                totkPath: totkPath,
                decompressor: decompressor,
                compressor: compressor,
                writer: writer,
                sarc: sarc
            }
        )
        
    }

    //Get totk actor entries recursively


    //Save the sarc file, compress if file ends with .zs, create directory if needed
    pub fn save(&mut self, dest_file: &str) -> io::Result<()> {
        let file_path: &Path = Path::new(dest_file);
        let directory: &Path = file_path.parent().expect("Cannot get parent of the file");
        fs::create_dir_all(directory)?;
        let mut data: Vec<u8> = self.writer.to_binary();
        if dest_file.to_lowercase().ends_with(".zs") {
            data = self.compressor.compress_pack(&data)?;
        }
        let mut file_handle: fs::File = fs::File::create(file_path)?;
        file_handle.write_all(&data)?;
        Ok(())
    }


    //Read sarc file's bytes, decompress if needed
    fn sarc_file_to_bytes(path: &PathBuf, decompressor: &'a ZstdDecompressor) -> io::Result<Vec<u8>> {
        let mut fHandle: fs::File = fs::File::open(path)?;
        let mut buffer: Vec<u8> = Vec::new();
        fHandle.read_to_end(&mut buffer)?;
        if !buffer.as_slice().starts_with(b"SARC") {
            let res: Vec<u8> = decompressor.decompress_pack(&buffer).expect("Failed to decompress pack");
            return Ok(res);
        }
        Ok(buffer)
    }

}