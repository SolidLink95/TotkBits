use std::{ffi::CStr, io, path::Path, sync::Arc};

use libloading::{Library, Symbol};
use roead::Endian;

use crate::{file_format::BinTextFile::OpenedFile, Open_and_Save::SendData, Settings::Pathlib, Zstd::{is_xlink, TotkFileType, TotkZstd}};

type XlinkBinaryToYaml = unsafe extern "C" fn(data: *const i8, size: usize) -> *const i8;
type XlinkYamlToBinary =
    unsafe extern "C" fn(data: *const i8, size: usize, out_size: *mut usize) -> *mut i8;
type FreeXlinkBinary = unsafe extern "C" fn(data: *mut i8);
type FreeXlinkString = unsafe extern "C" fn(str_: *mut i8);

pub struct Xlink_rs<'a> {
    pub zstd: Arc<TotkZstd<'a>>
}

impl<'a> Xlink_rs<'_> {
    pub fn new(zstd: Arc<TotkZstd<'a>>) -> io::Result<Xlink_rs<'a>> {

        let res = Xlink_rs {
            zstd: zstd.clone()
        };
        Ok(res)
    }

    pub fn binary_to_yaml(&self, data: &[u8]) -> io::Result<String> {
        let rawdata = if !is_xlink(data) {
            self.zstd.decompress_zs(&data.to_vec())?
        } else {
            data.to_vec()
        };
        if !is_xlink(&rawdata) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Xlink_rs: Not a valid xlink binary",
            ));
        }
        let xlink_binary_to_yaml: Symbol<XlinkBinaryToYaml> = self
            .zstd
            .dll_manager
            .xlink_dll
            .get_function("xlink_binary_to_yaml")?;
        let free_xlink_string: Symbol<FreeXlinkString> = self
            .zstd
            .dll_manager
            .xlink_dll
            .get_function("free_xlink_string")?;
        let c_binary = rawdata.as_ptr() as *const i8;
        unsafe {
            let yaml_ptr = (xlink_binary_to_yaml)(c_binary, rawdata.len());
            if !yaml_ptr.is_null() {
                let yaml_cstr = CStr::from_ptr(yaml_ptr);
                let yaml_str = yaml_cstr.to_string_lossy().into_owned();
                // println!("YAML output: {}", yaml_cstr.to_string_lossy());
                (free_xlink_string)(yaml_ptr as *mut i8);
                return Ok(yaml_str);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::Other,
            "Xlink_rs: Failed to convert binary to YAML",
        ))
    }

    pub fn yaml_to_binary(&self, data: &str) -> io::Result<Vec<u8>> {
        let xlink_yaml_to_binary: Symbol<XlinkYamlToBinary> = self
            .zstd
            .dll_manager
            .xlink_dll
            .get_function("xlink_yaml_to_binary")?;
        let free_xlink_binary: Symbol<FreeXlinkBinary> = self
            .zstd
            .dll_manager
            .xlink_dll
            .get_function("free_xlink_binary")?;
        let rawdata = data.as_bytes();
        // let c_binary = rawdata.as_ptr() as *const i8;
        let mut out_size: usize = 0;
        unsafe {
            let binary_ptr = (xlink_yaml_to_binary)(rawdata.as_ptr() as *const i8, rawdata.len(), &mut out_size);
            if !binary_ptr.is_null() {
                let binary_slice = std::slice::from_raw_parts(binary_ptr as *const u8, out_size);
                let binary_vec = binary_slice.to_vec();
                (free_xlink_binary)(binary_ptr as *mut i8);
                return Ok(binary_vec);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::Other,
            "Xlink_rs: Failed to convert YAML to binary",
        ))
    }

    pub fn binary_file_to_yaml(zstd: Arc<TotkZstd<'a>>, path: &Path) -> io::Result<String> {
        let xlink = Xlink_rs::new(zstd)?;
        let data = std::fs::read(path)?;
        xlink.binary_to_yaml(&data)
    }

    pub fn open_xlink<P:AsRef<Path>>(path: P, zstd: Arc<TotkZstd>)  -> Option<(OpenedFile<'static>, SendData)> {
        let path = path.as_ref();
        let pathlib_var = Pathlib::new(&path);
        let rawdata = std::fs::read(path).ok()?;
        let xlink = Xlink_rs::new(zstd).ok()?;
        let mut opened_file = OpenedFile::default();
        let mut data = SendData::default();
        print!("Is {} a xlink file? ", path.display());
        match xlink.binary_to_yaml(&rawdata) {
            Ok(text) => {
                println!("Yes\n");
                opened_file.path = pathlib_var.clone();
                opened_file.endian = Some(Endian::Little);
                opened_file.file_type = TotkFileType::Xlink;
                data.status_text = format!("Opened {}", &pathlib_var.full_path);
                data.path = pathlib_var;
                data.text = text;
                data.get_file_label(opened_file.file_type, Some(Endian::Little));
                return Some((opened_file, data));
            }
            Err(e) => {
                println!("No\n{:?}", e);
                return None;
            }
        }
    }
}
