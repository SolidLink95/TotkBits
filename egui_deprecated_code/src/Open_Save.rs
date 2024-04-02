use std::io::{BufWriter, Read, Write};
use std::{env, fs, io};

use msyt::converter::MsytFile;
use roead::byml::Byml;

use crate::file_format::BinTextFile::{BymlFile, OpenedFile};
use crate::file_format::Pack::{PackComparer, PackFile};
use crate::file_format::TagProduct::TagProduct;
use crate::{
    Gui::{ActiveTab, TotkBitsApp},
    Settings::Pathlib,
    Tree::TreeNode,
    Zstd::TotkFileType,
};

pub struct FileOpener {}

impl FileOpener {
    pub fn open_byml_or_sarc_alt(
        app: &mut TotkBitsApp,
        path: String,
        old_path: String,
    ) -> Option<io::Result<()>> {
        let endian = app.opened_file.endian;
        app.opened_file = OpenedFile::from_path(path, TotkFileType::Other);
        app.opened_file.endian = endian;
        if FileOpener::open_byml_or_sarc(app, true).is_none() {
            app.opened_file.path = Pathlib::new(old_path);
        }
        None
    }
    pub fn open_byml_or_sarc(app: &mut TotkBitsApp, overwrite: bool) -> Option<io::Result<()>> {
        if app.settings.is_file_loaded && !overwrite {
            return None; //stops the app from infinite file loading from disk
        }
        //app.file_reader.f.unlock();
        app.settings.is_file_loaded = true;
        let path = app.opened_file.path.full_path.clone();
        println!("Guessing file type: {}", &path);
        if let Some(_res) = &FileOpener::open_tag(app, &path) {
            return Some(Ok(()));
        }
        if let Some(_res) = &FileOpener::open_msbt(app, &path) {
            return Some(Ok(()));
        }
        if let Some(_res) = &FileOpener::open_sarc(app, &path) {
            return Some(Ok(()));
        }
        if let Some(_res) = &FileOpener::open_byml(app, &path) {
            return Some(Ok(()));
        }
        if let Ok(_res) = &FileOpener::open_text(app, &path) {
            return Some(Ok(()));
        }
        app.settings.is_tree_loaded = true;
        app.status_text = format!("Failed to open: {}", &path);
        println!("Failed to open: {}", &path);
        None
    }

    pub fn try_open_argv1(app: &mut TotkBitsApp) -> io::Result<()> {
        let argv: Vec<String> = env::args().collect();
        if argv.len() < 2 {
            return Err(io::Error::new(io::ErrorKind::NotFound, ""));
        }
        let file_name = &argv[1];
        let old_path = app.opened_file.path.full_path.clone();
        let path = file_name.clone();
        let mut f_handle = fs::File::open(&file_name)?;
        let mut buffer: Vec<u8> = Vec::new(); //String::new();
        match f_handle.read_to_end(&mut buffer) {
            Ok(_) => {
                app.status_text = format!("Opened file: {}", &file_name);

                FileOpener::open_byml_or_sarc_alt(app, path, old_path);
                //app.settings.is_file_loaded = false; //this flag lets FileOpener to determine the file type
            }

            Err(_err) => {
                app.status_text = format!("Error reading file: {}", file_name);
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    app.status_text.clone(),
                ));
            }
        }
        Ok(())
    }

    pub fn open_text(app: &mut TotkBitsApp, path: &str) -> io::Result<()> {
        let mut file = fs::File::open(path)?;
        let mut buffer = Vec::new();

        file.read_to_end(&mut buffer)?;

        match String::from_utf8(buffer) {
            Ok(contents) => {
                // Check if most of the characters are printable or whitespace
                let _ = app.file_reader.from_string(&contents);
                app.active_tab = ActiveTab::TextBox;
                app.internal_sarc_file = None;
                app.status_text = format!("Opened: {}", &path);
                app.opened_file = OpenedFile::new(path.to_string(), TotkFileType::Text, None, None);
                app.opened_file.byml = None;
            }
            Err(_) => {
                return Err(io::Error::new(io::ErrorKind::InvalidData, ""));
            } // Not valid UTF-8
        }
        Ok(())
    }

    pub fn open_byml(app: &mut TotkBitsApp, path: &str) -> Option<()> {
        println!("Is {} a byml?", path);
        if let Ok(b) = BymlFile::new(path.to_string(), app.zstd.clone()) {
            let text = Byml::to_text(&b.pio);
            let _ = app.file_reader.from_string(&text);
            /*println!(
                "{}, {} {}",
                &app.text.len(),
                &b.pio.to_binary(roead::Endian::Little).len(),
                app.text.chars().filter(|&c| c == '\n').count()
            );*/
            app.active_tab = ActiveTab::TextBox;
            println!("Byml  opened!");
            app.internal_sarc_file = None;
            app.opened_file = OpenedFile::new(
                path.to_string(),
                TotkFileType::Byml,
                BymlFile::get_endiannes(&b.file_data.data),
                None,
            );
            app.opened_file.byml = Some(b);
            app.status_text = format!("Opened: {}", app.opened_file.path.full_path);
            return Some(());
        }
        None
    }

    pub fn open_sarc(app: &mut TotkBitsApp, path: &str) -> Option<()> {
        println!("Is {} a sarc?", path);
        match PackFile::new(path.to_string(), app.zstd.clone()) {
            Ok(inner_pack) => {
                let mut pack = PackComparer::from_pack(inner_pack, app.zstd.clone());
                pack.compare();
                app.pack = Some(pack);
                app.settings.is_file_loaded = true;
                println!("Sarc  opened!");
                app.active_tab = ActiveTab::DiretoryTree;
                app.settings.is_tree_loaded = false;
                app.root_node = TreeNode::new("ROOT".to_string(), "/".to_string());
                app.status_text = format!("Opened: {}", app.opened_file.path.full_path);
                app.opened_file = OpenedFile::new(
                    path.to_string(),
                    TotkFileType::Sarc,
                    Some(roead::Endian::Little),
                    None,
                );
                return Some(());
            }
            Err(_err) => {}
        }
        None
    }

    pub fn open_msbt(app: &mut TotkBitsApp, path: &str) -> Option<()> {
        println!("Is {} a msyt?", &path);
        if let Ok(text) = MsytFile::file_to_text(path.to_string()) {
            //app.text = text;
            let _ = app.file_reader.from_string(&text);
            app.internal_sarc_file = None;
            app.active_tab = ActiveTab::TextBox;
            app.status_text = format!("Opened: {}", app.opened_file.path.full_path);
            app.opened_file = OpenedFile::from_path(path.to_string(), TotkFileType::Msbt);
            app.opened_file.endian = Some(roead::Endian::Little);
            return Some(());
        }
        None
    }

    pub fn open_tag(app: &mut TotkBitsApp, path: &str) -> Option<()> {
        if app
            .opened_file
            .path
            .name
            .to_lowercase()
            .starts_with("tag.product")
        {
            println!("Is {} a Tag product?", path);
            let tag = TagProduct::new(app.opened_file.path.full_path.clone(), app.zstd.clone());
            match tag {
                Ok(mut tag) => {
                    match tag.parse() {
                        Ok(_) => {
                            println!("Tag parsed!");
                        }
                        Err(err) => {
                            eprintln!("Error parsing tag! {:?}", err);
                            return None;
                        }
                    }
                    //tag.print();
                    app.opened_file =
                        OpenedFile::from_path(path.to_string(), TotkFileType::TagProduct);
                    let _ = app.file_reader.from_string(&tag.text);
                    app.opened_file.tag = Some(tag);
                    app.active_tab = ActiveTab::TextBox;
                    app.opened_file.file_type = TotkFileType::TagProduct;
                    app.opened_file.endian = Some(roead::Endian::Little);
                    app.internal_sarc_file = None;
                    return Some(());
                }
                Err(err) => {
                    println!("{:?}", err);
                    return None;
                }
            }
        }
        None
    }
}

pub fn write_string_to_file(path: &str, content: &str) -> io::Result<()> {
    let file = fs::File::create(path)?;
    let mut writer = BufWriter::new(file);

    writer.write_all(content.as_bytes())?;

    // The buffer is automatically flushed when writer goes out of scope,
    // but you can manually flush it if needed.
    writer.flush()?;

    Ok(())
}

pub fn read_string_from_file(path: &str) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    Ok(contents)
}
