use crate::Pack::PackFile;
use crate::Settings::Pathlib;
use crate::Zstd::{is_byml, is_msyt, FileType, TotkZstd};
use anyhow;
use byteordered::Endianness;
use failure::ResultExt;
use indexmap::IndexMap;
use msbt::builder::MsbtBuilder;
use msbt::section::Atr1;
use msbt::Msbt;
use msyt::model::Msyt;
use msyt::model::{self, Content, Entry, MsbtInfo};
use msyt::Result as MsbtResult;
use roead::byml::Byml;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};

pub struct FileData {
    pub file_type: FileType,
    pub data: Vec<u8>,
}

impl FileData {
    pub fn new() -> Self {
        Self {
            file_type: FileType::None,
            data: Vec::new(),
        }
    }
    pub fn from(data: Vec<u8>, file_type: FileType) -> Self {
        Self {
            file_type: file_type,
            data: data,
        }
    }
}

pub struct MsytFile {
    /*pub endian: Option<roead::Endian>,
    pub file_data: FileData,
    pub path: Pathlib,
    pub msbt: std::pin::Pin<Box<Msbt>>,
    pub zstd: Arc<TotkZstd<'a>>,*/
}

pub fn roead_endian_to_byteordered(endian: roead::Endian) -> Endianness {
    match endian {
        roead::Endian::Big => {return Endianness::Big;},
        roead::Endian::Little => {return Endianness::Little;},
    }
}

impl MsytFile {
    pub fn text_to_binary_file(text: &str, path: &str, endian: roead::Endian) -> MsbtResult<()> {
        let encoding = msbt::Encoding::Utf16;
        let endiannes = roead_endian_to_byteordered(endian);
        let data = MsytFile::text_to_binary(text, endiannes, encoding).expect("Unable to save to msyt");
        //let mut f_handle = fs::File::open(&path).expect(&format!("Failed to open file {}", &path));
        let mut file = OpenOptions::new().write(true).open(&path)?;
        file.write_all(&data).expect("Error writing data to file");
        Ok(())
    }

    pub fn file_to_text(path: String) -> MsbtResult<String> {
        let mut f_handle = fs::File::open(&path).expect(&format!("Failed to open file {}", &path));
        let mut buf: Vec<u8> = Vec::new();
        f_handle.read_to_end(&mut buf);
        let text = MsytFile::binary_to_text(buf)?;
        Ok(text)
    }

    pub fn binary_to_text(data: Vec<u8>) -> MsbtResult<String> {
        if !is_msyt(&data) {
            return Err(failure::format_err!("Not msyt file"));
        }
        let cursor = Cursor::new(&data);
        let reader = BufReader::new(cursor);
        let msbt = Msbt::from_reader(BufReader::new(reader))
            .with_context(|_| format!("Failed to create msbt from reader"))
            .expect(&format!("Failed to create msbt from reader"));

        let lbl1 = match msbt.lbl1() {
            Some(lbl) => lbl,
            None => {
                println!("invalid msbt: missing lbl1: ");
                return Ok("".to_string());
            }
        };

        let mut entries = IndexMap::with_capacity(lbl1.labels().len());

        for label in lbl1.labels() {
            let mut all_content = Vec::new();

            let raw_value = label
                .value_raw()
                .ok_or_else(|| {
                    failure::format_err!(
                        "invalid msbt at : missing string for label {}",
                        label.name(),
                    )
                })
                .expect(&format!(
                    "invalid msbt at : missing string for label {}",
                    label.name(),
                ));
            let mut parts = msyt::botw::parse_controls(msbt.header(), raw_value)
                .expect("Failed to parse controls");
            all_content.append(&mut parts);
            let entry = Entry {
                attributes: msbt.atr1().and_then(|a| {
                    a.strings()
                        .get(label.index() as usize)
                        .map(|s| msyt::util::strip_nul(*s))
                        .map(ToString::to_string)
                }),
                contents: all_content,
            };
            entries.insert(label.name().to_string(), entry);
        }

        entries.sort_keys();

        let msyt = Msyt {
            entries,
            msbt: MsbtInfo {
                group_count: lbl1.group_count(),
                atr1_unknown: msbt.atr1().map(Atr1::unknown_1),
                ato1: msbt.ato1().map(|a| a.unknown_bytes().to_vec()),
                tsy1: msbt.tsy1().map(|a| a.unknown_bytes().to_vec()),
                nli1: msbt.nli1().map(|a| model::Nli1 {
                    id_count: a.id_count(),
                    global_ids: a.global_ids().clone(),
                }),
            },
        };

        let yaml_string =
            serde_yaml::to_string(&msyt).with_context(|_| "could not serialize yaml to string")?;
        Ok(yaml_string)
    }

    pub fn text_to_binary(text: &str, endianness: Endianness, encoding: msbt::Encoding) -> MsbtResult<Vec<u8>> {
        let msyt: Msyt =
            serde_yaml::from_str(&text).expect(&format!("Cannot create msyt from string"));

        let mut builder = MsbtBuilder::new(endianness, encoding, Some(msyt.msbt.group_count));
      if let Some(unknown_bytes) = msyt.msbt.ato1 {
        builder = builder.ato1(msbt::section::Ato1::new_unlinked(unknown_bytes));
      }
      if let Some(unknown_1) = msyt.msbt.atr1_unknown {
        // ATR1 should have exactly the same amount of entries as TXT2. In the BotW files, sometimes
        // an ATR1 section is specified to have that amount but the section is actually empty. For
        // msyt's purposes, if the msyt does not contain the same amount of attributes as it does
        // text entries (i.e. not every label has an `attributes` node), it will be assumed that the
        // ATR1 section should specify that it has the correct amount of entries but actually be
        // empty.
        let strings: Option<Vec<String>> = msyt.entries
          .iter()
          .map(|(_, e)| e.attributes.clone())
          .map(|s| s.map(msyt::util::append_nul))
          .collect();
        let atr_len = match strings {
          Some(ref s) => s.len(),
          None => msyt.entries.len(),
        };
        let strings = strings.unwrap_or_default();
        builder = builder.atr1(msbt::section::Atr1::new_unlinked(atr_len as u32, unknown_1, strings));
      }
      if let Some(unknown_bytes) = msyt.msbt.tsy1 {
        builder = builder.tsy1(msbt::section::Tsy1::new_unlinked(unknown_bytes));
      }
      if let Some(nli1) = msyt.msbt.nli1 {
        builder = builder.nli1(msbt::section::Nli1::new_unlinked(nli1.id_count, nli1.global_ids));
      }
      for (label, entry) in msyt.entries.into_iter() {
        let new_val = Content::write_all(builder.header(), &entry.contents)?;
        builder = builder.add_label(label, new_val);
      }
      let msbt = builder.build();

      let mut dest_buf: Vec<u8> = Vec::new();
      let dest_cursor = Cursor::new(&mut dest_buf);
      //let mut new_msbt = Msbt::from_reader(dest_cursor).expect("Failed to create empty msbt");
      msbt.write_to(dest_cursor).expect(&format!("Error writing to cursor {}", line!()));

        Ok(dest_buf)
    }
}

pub struct BymlFile<'a> {
    pub endian: Option<roead::Endian>,
    pub file_data: FileData,
    pub path: Pathlib,
    pub pio: roead::byml::Byml,
    pub zstd: Arc<TotkZstd<'a>>,
}

impl<'a> BymlFile<'_> {
    pub fn new(path: String, zstd: Arc<TotkZstd<'a>>) -> io::Result<BymlFile<'a>> {
        let data: FileData =
            BymlFile::byml_data_to_bytes(&PathBuf::from(path.clone()), &zstd.clone())?;
        return BymlFile::from_binary(data, zstd, path);
    }

    pub fn save(&self, path: String) -> io::Result<()> {
        //let mut f_handle = OpenOptions::new().write(true).open(&path)?;
        let mut data = self
            .pio
            .to_binary(self.endian.unwrap_or(roead::Endian::Little));
        if path.to_ascii_lowercase().ends_with(".zs") {
            match self.file_data.file_type {
                FileType::Byml => {
                    data = self
                        .zstd
                        .compressor
                        .compress_zs(&data)
                        .expect("Failed to compress with zs");
                }
                FileType::Bcett => {
                    data = self
                        .zstd
                        .compressor
                        .compress_bcett(&data)
                        .expect("Failed to compress with bcett");
                }
                _ => {
                    data = self
                        .zstd
                        .compressor
                        .compress_zs(&data)
                        .expect("Failed to compress with zs");
                }
            }
        }
        //f_handle.write_all(&data);
        bytes_to_file(data, &path);
        Ok(())
    }

    pub fn from_text(content: &str, zstd: Arc<TotkZstd<'a>>) -> io::Result<BymlFile<'a>> {
        let pio: Result<Byml, roead::Error> = Byml::from_text(&content);
        match pio {
            Ok(ok_pio) => Ok(BymlFile {
                endian: Some(roead::Endian::Little), //TODO: add Big endian support
                file_data: FileData::new(),
                path: Pathlib::new("".to_string()),
                pio: ok_pio,
                zstd: zstd.clone(),
            }),
            Err(_err) => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Error for byml file",
                ));
            }
        }
    }

    pub fn from_binary(
        data: FileData,
        zstd: Arc<TotkZstd<'a>>,
        full_path: String,
    ) -> io::Result<BymlFile<'a>> {
        let pio = Byml::from_binary(&data.data);
        match pio {
            Ok(ok_pio) => Ok(BymlFile {
                endian: BymlFile::get_endiannes(&data.data.clone()),
                file_data: data,
                path: Pathlib::new(full_path),
                pio: ok_pio,
                zstd: zstd.clone(),
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
        if self.file_data.data.starts_with(b"BY") {
            return roead::Endian::Big;
        } else if self.file_data.data.starts_with(b"YB") {
            return roead::Endian::Little;
        }
        return roead::Endian::Little;
    }

    pub fn get_endiannes(data: &Vec<u8>) -> Option<roead::Endian> {
        if data.starts_with(b"BY") {
            return Some(roead::Endian::Big);
        }
        if data.starts_with(b"YB") {
            return Some(roead::Endian::Little);
        }
        None
    }

    fn byml_data_to_bytes(path: &PathBuf, zstd: &'a TotkZstd) -> Result<FileData, io::Error> {
        let mut f_handle: fs::File = fs::File::open(path)?;
        let mut buffer: Vec<u8> = Vec::new();
        f_handle.read_to_end(&mut buffer)?;
        //let mut returned_result: Vec<u8> = Vec::new();
        let mut data = FileData::new();
        if is_byml(&buffer) {
            //regular byml file,
            data.data = buffer;
            data.file_type = FileType::Byml;
            return Ok(data);
        } else {
            match zstd.decompressor.decompress_zs(&buffer) {
                //regular byml file compressed with zs
                Ok(res) => {
                    if is_byml(&res) {
                        data.data = res;
                        data.file_type = FileType::Byml;
                    }
                }
                Err(_err) => {}
            }
        }
        if !is_byml(&data.data) {
            match zstd.decompressor.decompress_bcett(&buffer) {
                //bcett map file
                Ok(res) => {
                    data.data = res;
                    data.file_type = FileType::Byml;
                }
                _ => {}
            }
        }

        if !is_byml(&data.data) {
            match zstd.try_decompress(&buffer) {
                //try decompressing with other dicts
                Ok(res) => {
                    data.data = res;
                    data.file_type = FileType::Other;
                }
                Err(err) => {
                    println!("Error during zstd decompress, {}", line!());
                    return Err(err);
                }
            }
        }
        if data.data.starts_with(b"Yaz0") {
            match roead::yaz0::decompress(&data.data) {
                Ok(dec_data) => {
                    data.data = dec_data;
                }
                Err(_) => {}
            }
        }
        if is_byml(&data.data) {
            return Ok(data);
        }
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Invalid data, not a byml",
        ));
    }
}


pub fn bytes_to_file(data: Vec<u8>, path: &str) -> io::Result<()> {
    let mut f = fs::File::create(&path);//TODO check if the ::create is sufficient
    match f {
        Ok(mut f_handle) => {//file does not exist
            f_handle.write_all(&data);
        },
        Err(_) => { //file exist, overwrite
            let mut f_handle = OpenOptions::new().write(true).open(&path)?;
            f_handle.write_all(&data);

        }
    }
    Ok(())
}