//use crate::msyt::model::Msyt;
use crate::Settings::Pathlib;
use crate::Zstd::{is_byml, FileType, TotkZstd};
use anyhow;
use msbt::Msbt;
use msyt::model::Content;
use msyt::Msyt;
use roead::byml::Byml;
use std::io::{BufReader, Cursor, Read, Write};
use std::path::PathBuf;
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

pub struct MsytFile<'a> {
    pub endian: Option<roead::Endian>,
    pub file_data: FileData,
    pub path: Pathlib,
    pub msbt: std::pin::Pin<Box<Msbt>>,
    pub zstd: Arc<TotkZstd<'a>>,
}

impl<'a> MsytFile<'_> {
    pub fn new(path: String, zstd: Arc<TotkZstd<'a>>) -> io::Result<MsytFile<'a>> {
        let mut f_handle = fs::File::open(&path)?;
        let mut data: Vec<u8> = Vec::new();
        f_handle.read_to_end(&mut data);
        MsytFile::from_binary(FileData::from(data, FileType::Msbt), zstd)
    }

    /*pub fn to_text() -> io::Result<()> {
        {
            let mut data: Vec<u8> = Vec::new();
            let cursor = Cursor::new(&data.data);
            let mut reader = BufReader::new(cursor);
            let msbt = Msbt::from_reader(reader).unwrap();
            /*let msbt_file = File::open(&path)
                .with_context(|| format!("could not open {}", path.to_string_lossy()))?;
            let msbt = Msbt::from_reader(BufReader::new(msbt_file)).with_context(|| {
                format!("could not read msbt file at {}", path.to_string_lossy())
            })?;*/

            let lbl1 = match msbt.lbl1() {
                Some(lbl) => lbl,
                None => {
                    return Err(anyhow::anyhow!(
                        "invalid msbt: missing lbl1: {}",
                        path.to_string_lossy()
                    ))
                }
            };

            let mut entries = IndexMap::with_capacity(lbl1.labels().len());

            for label in lbl1.labels() {
                let mut all_content = Vec::new();

                let raw_value = label.value_raw().ok_or_else(|| {
                    anyhow::format_err!(
                        "invalid msbt at {}: missing string for label {}",
                        path.to_string_lossy(),
                        label.name(),
                    )
                })?;
                let mut parts = crate::botw::parse_controls(msbt.header(), raw_value)
                    .with_context(|| {
                        format!(
                            "could not parse control sequences in {}",
                            path.to_string_lossy()
                        )
                    })?;
                all_content.append(&mut parts);
                let entry = Entry {
                    attributes: msbt.atr1().and_then(|a| {
                        a.strings()
                            .get(label.index() as usize)
                            .map(|s| crate::util::strip_nul(s))
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
                    nli1: msbt.nli1().map(|a| crate::model::Nli1 {
                        id_count: a.id_count(),
                        global_ids: a.global_ids().clone(),
                    }),
                },
            };

            let dest = match output_path {
                Some(output) => {
                    let stripped_path = match input_paths
                        .iter()
                        .flat_map(|input| path.strip_prefix(input))
                        .next()
                    {
                        Some(s) => s,
                        None => {
                            return Err(anyhow::anyhow!(
                                "no input path works as a prefix on {}",
                                path.to_string_lossy()
                            ))
                        }
                    };
                    output.join(stripped_path).with_extension("msyt")
                }
                None => path.with_extension("msyt"),
            };
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "could not create parent directories for {}",
                        parent.to_string_lossy()
                    )
                })?;
            }
            let mut writer = BufWriter::new(File::create(dest)?);
            serde_yaml::to_writer(&mut writer, &msyt)
                .with_context(|| "could not write yaml to file")?;
            // add final newline
            writer
                .write_all(b"\n")
                .with_context(|| "could not write final newline to file")?;

            Ok(())
        }
    }*/

    pub fn from_text(text: String, zstd: Arc<TotkZstd<'a>>) -> io::Result<MsytFile<'a>> {
        let msyt: Msyt = serde_yaml::from_str(&text).unwrap();
        let mut data: Vec<u8> = Vec::new();
        let cursor = Cursor::new(&data);
        let mut reader = BufReader::new(cursor);
        let mut msbt = Msbt::from_reader(reader).unwrap();
        let mut file_data = FileData::new();
        file_data.file_type = FileType::Msbt;
        for (key, entry) in msyt.entries {
            let new_val = Content::write_all(msbt.header(), &entry.contents).unwrap();
            if let Some(ref mut lbl1) = msbt.lbl1_mut() {
                if let Some(label) = lbl1.labels_mut().iter_mut().find(|x| x.name() == key) {
                    if let Err(()) = label.set_value_raw(new_val) {
                        eprintln!("could not set raw string at index {}", label.index());
                    }
                }
            }
        }

        Ok(MsytFile {
            endian: Some(roead::Endian::Little),
            file_data: file_data,
            path: Pathlib::new("".to_string()),
            msbt: msbt,
            zstd: zstd.clone(),
        })
    }
    pub fn from_binary(data: FileData, zstd: Arc<TotkZstd<'a>>) -> io::Result<MsytFile<'a>> {
        let cursor = Cursor::new(&data.data);
        let mut reader = BufReader::new(cursor);
        let msbt = Msbt::from_reader(reader).unwrap();

        return Ok(MsytFile {
            endian: Some(roead::Endian::Little),
            file_data: FileData::from(data.data, FileType::Msbt),
            path: Pathlib::new("".to_string()),
            msbt: msbt,
            zstd: zstd.clone(),
        });
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
        let mut f_handle = fs::File::open(path.clone())?;
        let mut data = self
            .pio
            .to_binary(self.endian.unwrap_or(roead::Endian::Little));
        if path.to_ascii_lowercase().ends_with(".zs") {
            match self.file_data.file_type {
                FileType::Byml => {
                    data = self.zstd.compressor.compress_zs(&data).unwrap();
                }
                FileType::Bcett => {
                    data = self.zstd.compressor.compress_bcett(&data).unwrap();
                }
                _ => {
                    data = self.zstd.compressor.compress_zs(&data).unwrap();
                }
            }
        }
        f_handle.write_all(&data);
        Ok(())
    }

    pub fn from_text(content: String, zstd: Arc<TotkZstd<'a>>) -> io::Result<BymlFile<'a>> {
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
