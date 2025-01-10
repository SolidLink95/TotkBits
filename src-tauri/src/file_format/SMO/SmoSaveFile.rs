use std::{fs::OpenOptions, io::{self, Write}, path::Path, sync::Arc};
use roead::byml::{self, Byml};
use crate::{file_format::BinTextFile::{BymlFile, FileData}, Zstd::{is_byml, TotkFileType, TotkZstd}};


const SMO_SAVE_FILE_SIZE : usize = 0x20000C;
const SMO_HEADER_SIZE : usize = 16;

//zstd isnt needed here but BymlFile demands it
//Lots of data after byml section
//TODO: add a way to handle those extra data
pub struct SmoSaveFile<'a> {
    pub header: Vec<u8>,
    #[allow(dead_code)]
    pub byml_file: BymlFile<'a>,
    pub endian: roead::Endian,
}

impl<'a> SmoSaveFile<'a> {
    pub fn from_binary<P: AsRef<Path>>(data: &[u8], zstd: Arc<TotkZstd<'a>>, path: P) -> io::Result<Self> {
        if !Self::is_smo_save_binary(data) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Data is not a valid SMO save file"));
        }
        let header  = data[0..SMO_HEADER_SIZE].to_vec();
        let file_data = FileData{
            file_type: TotkFileType::SmoSaveFile,
            data: data[SMO_HEADER_SIZE..].to_vec()
        };
        let byml_file = BymlFile::from_binary(file_data, zstd.clone(), path.as_ref())?;
        if let Some(endian) = byml_file.endian {
            if endian != roead::Endian::Little {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "SMO save file must be little endian"));
            }
        }
        Ok(Self {
            header,
            byml_file,
            endian: roead::Endian::Little
        })
    }

    pub fn to_binary(&self) -> io::Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(&self.header);
        let byml_binary_data = self.byml_file.pio.to_binary(self.endian);
        data.extend_from_slice(&byml_binary_data);
        //padding
        let written_size = SMO_HEADER_SIZE + byml_binary_data.len();
        if written_size > SMO_SAVE_FILE_SIZE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "SMO save file size exceeded"));
        }
        let size_to_write = (SMO_SAVE_FILE_SIZE - written_size) as usize;
        let padding: Vec<u8> = vec![0u8; size_to_write];
        data.extend_from_slice(&padding);
        Ok(data)
    }

    pub fn from_file<P: AsRef<Path>>(path: P, zstd: Arc<TotkZstd<'a>>) -> io::Result<Self> {
        let path_ref = path.as_ref();
        let data = std::fs::read(path_ref)?;
        let result = Self::from_binary(&data, zstd, path_ref);
        if result.is_ok() {
            Self::backup_file(path_ref)?;
        }
        result
    }

    pub fn to_string(&mut self) -> io::Result<String> {
        
        let pio_map = self.byml_file.pio.as_mut_map().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let header_array: [u8; SMO_HEADER_SIZE] = self.header[..SMO_HEADER_SIZE].try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Header size mismatch"))?;
        let header_hex_str = bytes_to_hex_uppercase(&header_array);
        pio_map.insert("_Header".into(), header_hex_str.into());
        // let pio = byml::Byml::Map(pio_map);
        let pio = Byml::Map(pio_map.clone());
        Ok(Byml::to_text(&pio))
    }
    pub fn from_string(text: &str,zstd: Arc<TotkZstd<'a>>) -> io::Result<Self> {
        let mut byml_file = BymlFile::from_text(&text, zstd.clone())?;
        let mut pio_map = byml_file.pio.as_mut_map().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        if !pio_map.contains_key("_Header")  {
            return Err(io::Error::new(io::ErrorKind::Other, "SMO save file does not contain Header key"));
        }
        let mut header_hex_str = String::new();
        if let Some(pio_hdr) = pio_map.get("_Header") {
            header_hex_str = pio_hdr.clone().into_string().unwrap_or_default().as_str().to_string();
        }
        let header = hex_to_bytes(&header_hex_str)?.to_vec();
        pio_map.remove("_Header");
        // byml_file.pio = byml::Byml::Map(pio_map);
        if let Some(endian) = byml_file.endian {
            if endian != roead::Endian::Little {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "SMO save file must be little endian"));
            }
        }
        Ok(Self {
            header,
            byml_file,
            endian: roead::Endian::Little
        })
    }
    pub fn save<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let mut buffer: Vec<u8> = Vec::new();
        //header
        if self.header.len() != SMO_HEADER_SIZE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Header size mismatch"));
        }
        buffer.extend_from_slice(&self.header);
        //byml data
        let byml_binary_data = self.byml_file.pio.to_binary(self.endian);
        buffer.extend_from_slice(&byml_binary_data);
        //padding
        let written_size = SMO_HEADER_SIZE + byml_binary_data.len();
        if written_size > SMO_SAVE_FILE_SIZE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "SMO save file size exceeded"));
        }
        let size_to_write = (SMO_SAVE_FILE_SIZE - written_size) as usize;
        
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(path.as_ref())?;
        
        file.write_all(&buffer)?;
        let padding: Vec<u8> = vec![0u8; size_to_write];
        file.write_all(&padding)?;
        Ok(())
    }

    #[inline]
    pub fn is_smo_save_binary(data: &[u8]) -> bool {
        data.len() == SMO_SAVE_FILE_SIZE && is_byml(&data[SMO_HEADER_SIZE..])
    }

    pub fn backup_file<P: AsRef<Path>>(path: P) -> io::Result<()> {
        let backup_path = path.as_ref().with_extension("bak").to_string_lossy().to_string();
        for i in 0..100 {
            let backup_path = if i == 0 {
                backup_path.clone()
            } else {
                format!("{}{}", &backup_path, i)
            };
            if !Path::new(&backup_path).exists() {
                if let Ok(_) = std::fs::copy(path.as_ref(), backup_path) {
                    return Ok(());
                }
            }
        }
        std::fs::copy(path.as_ref(), backup_path).unwrap_or_default();
        Ok(())
    }
}


fn bytes_to_hex_uppercase(bytes: &[u8; SMO_HEADER_SIZE]) -> String {
    bytes.iter()
         .map(|b| format!("{:02X}", b)) // Format each byte as two-digit uppercase hex
         .collect::<Vec<_>>()
         .join("")
}

fn hex_to_bytes(hex: &str) -> io::Result<[u8; SMO_HEADER_SIZE]> {
    if hex.len() != 32 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Hex string must be 32 characters long"));
    }

    let mut bytes = [0u8; SMO_HEADER_SIZE];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid hex character"))?;
    }
    Ok(bytes)
}