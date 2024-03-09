use msyt::converter::MsytFile;
use rfd::{FileDialog, MessageDialog};
use roead::{aamp::ParameterIO, byml::Byml};

use crate::{
    file_format::{
        BinTextFile::{BymlFile, FileData, OpenedFile},
        Msbt::MsbtFile,
        Pack::{PackComparer, PackFile},
        TagProduct::TagProduct,
    },
    Settings::Pathlib,
    TotkApp::{InternalFile, SendData},
    Zstd::{is_aamp, is_byml, is_msyt, TotkFileType, TotkZstd},
};
use std::{
    fs::{self, File}, io::{self, Read, Write}, panic::{self, AssertUnwindSafe}, path::{Path, PathBuf}, sync::Arc
};

pub fn open_sarc(file_name: String, zstd: Arc<TotkZstd>) -> Option<(PackComparer, SendData)> {
    let mut data = SendData::default();
    println!("Is {} a sarc?", &file_name);
    let sarc = PackFile::new(file_name.clone(), zstd.clone());
    if sarc.is_ok() {
        println!("{} is a sarc", &file_name);
        let s = sarc.as_ref().unwrap();
        let endian = s.endian.clone();
        let pack = PackComparer::from_pack(sarc.unwrap(), zstd.clone());
        if pack.is_none() {
            return None;
        }
        data.get_sarc_paths(pack.as_ref().unwrap());
        data.status_text = format!("Opened {}", &file_name);
        //internal_file = None;
        data.path = Pathlib::new(file_name.clone());
        data.text = "SARC".to_string();
        data.tab = "SARC".to_string();
        data.get_file_label(TotkFileType::Sarc, Some(endian));
        return Some((pack.unwrap(), data));
    }

    None
}

pub fn open_tag(file_name: String, zstd: Arc<TotkZstd>) -> Option<(OpenedFile, SendData)> {
    let mut opened_file = OpenedFile::default();
    let mut data = SendData::default();
    println!("Is {} a tag?", &file_name);
    if Pathlib::new(file_name.clone())
        .name
        .to_lowercase()
        .starts_with("tag.product")
    {
        println!("{} is a tag", &file_name);
        opened_file.tag = TagProduct::new(file_name.clone(), zstd.clone());
        if let Some(tag) = &mut opened_file.tag {
            opened_file.path = Pathlib::new(file_name.clone());
            opened_file.endian = Some(roead::Endian::Little);
            opened_file.file_type = TotkFileType::TagProduct;
            data.status_text = format!("Opened {}", &file_name);
            data.path = Pathlib::new(file_name.clone());
            data.text = tag.to_text();
            data.get_file_label(TotkFileType::TagProduct, Some(roead::Endian::Little));
            return Some((opened_file, data));
        }
    }
    None
}

pub fn open_byml(file_name: String, zstd: Arc<TotkZstd>) -> Option<(OpenedFile, SendData)> {
    let mut opened_file = OpenedFile::default();
    let mut data = SendData::default();
    println!("Is {} a byml?", &file_name);
    opened_file.byml = BymlFile::new(file_name.clone(), zstd.clone());
    if opened_file.byml.is_some() {
        let b = opened_file.byml.as_ref().unwrap();
        println!("{} is a byml", &file_name);
        opened_file.path = Pathlib::new(file_name.clone());
        opened_file.endian = b.endian;
        opened_file.file_type = b.file_data.file_type.clone();
        data.status_text = format!("Opened {}", &file_name);
        data.path = Pathlib::new(file_name.clone());
        data.text = Byml::to_text(&b.pio);
        data.get_file_label(b.file_data.file_type, b.endian);
        return Some((opened_file, data));
    }
    None
}

pub fn open_msbt(file_name: String) -> Option<(OpenedFile<'static>, SendData)> {
    let mut opened_file = OpenedFile::default();
    let mut data = SendData::default();
    println!("Is {} a msbt?", &file_name);
    opened_file.msyt = MsbtFile::from_filepath(&file_name);
    if opened_file.msyt.is_some() {
        let m = opened_file.msyt.as_ref().unwrap();
        println!("{} is a msbt", &file_name);
        opened_file.path = Pathlib::new(file_name.clone());
        opened_file.endian = Some(m.endian);
        opened_file.file_type = TotkFileType::Msbt;
        data.status_text = format!("Opened {}", &file_name);
        data.path = Pathlib::new(file_name.clone());
        data.text = m.text.clone();
        data.get_file_label(opened_file.file_type, Some(m.endian));
        return Some((opened_file, data));
    }
    None
}

pub fn open_text(file_name: String) -> Option<(OpenedFile<'static>, SendData)> {
    let mut opened_file = OpenedFile::default();
    let mut data = SendData::default();
    println!("Is {} regular text file?", &file_name);
    let mut file = fs::File::open(&file_name).ok()?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;
    if let Ok(text) = String::from_utf8(buffer) {
        println!("{} is a text file", &file_name);
        opened_file.path = Pathlib::new(file_name.clone());
        opened_file.file_type = TotkFileType::Text;
        data.status_text = format!("Opened {}", &file_name);
        data.path = Pathlib::new(file_name.clone());
        data.text = text;
        data.get_file_label(TotkFileType::Text, None);
        return Some((opened_file, data));
    }
    None
}

pub fn open_aamp(file_name: String) -> Option<(OpenedFile<'static>, SendData)> {
    let mut opened_file = OpenedFile::default();
    let mut data = SendData::default();
    println!("Is {} an aamp?", &file_name);
    let raw_data = std::fs::read(&file_name).ok()?;
    if is_aamp(&raw_data) {
        let pio = ParameterIO::from_binary(&raw_data).ok()?; // Parse AAMP from binary data
        println!("{} is an aamp", &file_name);
        opened_file.path = Pathlib::new(file_name.clone());
        opened_file.file_type = TotkFileType::Aamp;
        data.status_text = format!("Opened {}", &file_name);
        data.path = Pathlib::new(file_name.clone());
        data.text = pio.to_text();
        data.get_file_label(TotkFileType::Aamp, None);
        return Some((opened_file, data));
    }
    None
}

pub fn get_string_from_data(
    path: String,
    data: Vec<u8>,
    zstd: Arc<TotkZstd>,
) -> Option<(InternalFile, String)> {
    let mut internal_file = InternalFile::default();
    if data.is_empty() {
        return None;
    }
    if is_byml(&data) {
        if let Ok(file_data) = BymlFile::byml_data_to_bytes(&data, zstd.clone()) {
            if let Ok(byml_file) = BymlFile::from_binary(file_data, zstd.clone(), path.clone()) {
                let text = Byml::to_text(&byml_file.pio);
                internal_file.byml = Some(byml_file);
                let byml_ref = internal_file.byml.as_ref().unwrap(); // Safe due to the line above
                internal_file.endian = byml_ref.endian.clone();
                internal_file.path = Pathlib::new(path);
                internal_file.file_type = byml_ref.file_data.file_type.clone(); // Set file type
                return Some((internal_file, text));
            }
        }
    }

    if is_aamp(&data) {
        let text = ParameterIO::from_binary(&data).ok()?.to_text();
        internal_file.endian = None;
        internal_file.path = Pathlib::new(path.clone());
        internal_file.file_type = TotkFileType::Aamp;
        return Some((internal_file, text));
    }
    if is_msyt(&data) {
        let msbt = MsbtFile::from_binary(data, Some(path.clone()))?;
        internal_file.endian = Some(msbt.endian.clone());
        internal_file.path = Pathlib::new(path.clone());
        internal_file.file_type = TotkFileType::Msbt;
        return Some((internal_file, msbt.text));
    }
    if let Ok(text) = String::from_utf8(data) {
        internal_file.endian = None;
        internal_file.path = Pathlib::new(path.clone());
        internal_file.file_type = TotkFileType::Text;
        return Some((internal_file, text));
    }

    None
}

fn write_data_to_file<P: AsRef<Path>>(path: P, data: Vec<u8>) -> io::Result<()> {
    let path = path.as_ref();

    // Ensure the parent directory exists.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Open the file in write mode, creating it if it doesn't exist.
    let mut file = File::create(path)?;

    // Write the data to the file.
    file.write_all(&data)?;

    Ok(())
}

pub fn save_file_dialog(file_name: Option<String>) -> String {
    let name = file_name.unwrap_or("".to_string());
    let file = FileDialog::new().set_file_name(name).save_file();
    match file {
        Some(res) => {
            return res.to_string_lossy().into_owned();
        }
        None => {
            return "".to_string();
        }
    }
}

pub fn check_if_save_in_romfs(dest_file: &str, zstd: Arc<TotkZstd>) -> bool{
    if !dest_file.is_empty() {
        //check if file is saved in romfs
        if dest_file.starts_with(&zstd.totk_config.romfs.to_string_lossy().to_string()) {
            let m = format!(
                "About to save file:\n{}\nin romfs dump. Continue?",
                &dest_file
            );
            if MessageDialog::new()
                .set_title("Warning")
                .set_description(m)
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                == rfd::MessageDialogResult::Yes
            {
                return true;
            }
        }
    }
    false
}

pub fn get_binary_by_filetype(file_type: TotkFileType, text: &str, endian: roead::Endian) -> Option<Vec<u8>> {
    let mut rawdata: Vec<u8> = Vec::new();
    match file_type {
        TotkFileType::TagProduct => {
            if let Ok(some_data) = TagProduct::to_binary(text) {
                rawdata = some_data;
            }
        }
        TotkFileType::Byml => {
            let pio = Byml::from_text(text).ok()?;
            rawdata = pio.to_binary(endian);
        }
        TotkFileType::Msbt => {
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                MsytFile::text_to_binary(text, endian, None)
            }));
            if let Ok(msbt) = result {
                if let Ok(x) = msbt {
                    rawdata = x;
                }
            }
            //rawdata = MsytFile::text_to_binary(text, endian, None).ok()?;
        }
        TotkFileType::Aamp => {
            let pio = ParameterIO::from_text(text).ok()?;
            rawdata = pio.to_binary();
        }
        TotkFileType::Text => {
            rawdata = text.as_bytes().to_vec();
        }
        _ => {}
    }

    Some(rawdata)
}
