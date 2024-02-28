use std::{
    fs,
    io::{self, Read},
    rc::Rc,
};

use rfd::{FileDialog, MessageDialog};

use crate::{
    file_format::BinTextFile::{bytes_to_file, OpenedFile},
    FileReader::FileReader,
    Gui::{ActiveTab, TotkBitsApp},
    Open_Save::{write_string_to_file, FileOpener, FileSaver},
    SarcFileLabel::SarcLabel,
    Settings::{FileRenamer, Pathlib, TextSearcher},
    Tree::TreeNode,
};

pub struct ButtonOperations {}

impl ButtonOperations {
    pub fn add_click(app: &mut TotkBitsApp, child: &Rc<TreeNode<String>>) -> io::Result<()> {
        if let Some(pack) = &mut app.pack {
            if let Some(opened) = &mut pack.opened {
                let file = FileDialog::new().set_title("Add file").pick_file();
                if let Some(file) = &file {
                    let file_path = Pathlib::new(file.to_string_lossy().to_string());
                    if !file_path.full_path.is_empty() && file.exists() {
                        let mut f = fs::File::open(file_path.full_path)?;
                        let mut buf: Vec<u8> = Vec::new();
                        f.read_to_end(&mut buf)?;
                        let mut parent_dir = &child.path.full_path; //selected node is a directory
                        if child.is_file() {
                            parent_dir = &child.path.parent; //selected node is a parent
                        }
                        let internal_path = format!("{}/{}", parent_dir, file_path.name);
                        opened.writer.add_file(&internal_path, buf); //file added to writer
                                                                     //opened.reload(); //reload .sarc from .writer
                        app.settings.do_i_compare_and_reload = true;
                        app.settings.is_tree_loaded = false; //reload tree to watch effects
                    }
                }
            }
        }

        Ok(())
    }

    pub fn remove_click(app: &mut TotkBitsApp, child: &Rc<TreeNode<String>>) -> io::Result<()> {
        if let Some(pack) = &mut app.pack {
            if let Some(opened) = &mut pack.opened {
                let p = &child.path.full_path;
                let mut message = format!(
                    "All files from this directory will be deleted:\n{}\nProceed?",
                    p
                );
                if TreeNode::is_leaf(&child) && child.is_file() {
                    message = format!("The following file will be deleted:\n{}\nProceed?", p);
                }
                if MessageDialog::new()
                    .set_title("Warning")
                    .set_description(message)
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    == rfd::MessageDialogResult::Yes
                {
                    if TreeNode::is_leaf(&child) {
                        opened.writer.remove_file(p);
                    } else {
                        for file in opened.sarc.files() {
                            let file_name = file.name.unwrap_or("");
                            if file_name.starts_with(p) {
                                opened.writer.remove_file(file_name);
                            }
                        }
                    }
                    app.settings.is_tree_loaded = false; //reload tree
                    opened.reload();
                    child.remove_itself();
                }
            }
        }
        Ok(())
    }

    pub fn extract_click(app: &mut TotkBitsApp) -> io::Result<()> {
        match app.active_tab {
            ActiveTab::DiretoryTree => {
                if let Some(internal_file) = &mut app.internal_sarc_file {
                    if internal_file.is_file() && TreeNode::is_leaf(internal_file) {
                        if let Some(pack) = &mut app.pack {
                            if let Some(opened) = &mut pack.opened {
                                let ext = if internal_file.path.ext_last.is_empty() {&internal_file.path.extension} else {&internal_file.path.ext_last};
                                let path = FileDialog::new()
                                    .set_file_name(&internal_file.path.name)
                                    .add_filter(ext, &vec![ext])
                                    .add_filter("yaml", &vec!["yml", "yaml"])
                                    .add_filter("All", &vec![""])
                                    .set_title("Extract")
                                    .save_file();
                                //println!("{}", &path.clone().unwrap().to_string_lossy().into_owned());
                                if let Some(dest_file) = &path {
                                    let dst = dest_file.to_string_lossy().into_owned();
                                    if let Some(data) =
                                        opened.sarc.get_data(&internal_file.path.full_path)
                                    {
                                        match bytes_to_file(data.to_vec(), &dst) {
                                            Ok(_) => {
                                                app.status_text = format!("Saved: {}", &dst);
                                            }
                                            Err(_err) => {
                                                app.status_text =
                                                    format!("Error extracting: {}", &dst);
                                            }
                                        }
                                    } else {
                                        app.status_text = format!(
                                            "Error extracting: {} to {}",
                                            &internal_file.path.name, &dst
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub fn edit_click(app: &mut TotkBitsApp, ui: &mut egui::Ui) {
        if let Some(child) = &mut app.internal_sarc_file.clone() {
            SarcLabel::safe_open_file_from_opened_sarc(app, ui, child)
        }
        //app.internal_sarc_file = Some(child.clone());
    }

    pub fn save_click(app: &mut TotkBitsApp) {
        match app.active_tab {
            ActiveTab::DiretoryTree => {
                FileSaver::save_tab_tree(app);
            }
            ActiveTab::TextBox => {
                let _ = FileSaver::save_tab_text(app);
            }
            ActiveTab::Settings => {}
        }
    }

    pub fn save_as_click(app: &mut TotkBitsApp) -> Result<(), roead::Error> {
        let mut prob_file_name = String::new();
        if app.opened_file.path.full_path.len() > 0 {
            prob_file_name = Pathlib::new(app.opened_file.path.full_path.clone()).name;
        }
        let dest_file = save_file_dialog(Some(prob_file_name));
        if !dest_file.is_empty() {
            //check if file is saved in romfs
            if dest_file.starts_with(&app.zstd.totk_config.romfs.to_string_lossy().to_string()) {
                let m = format!(
                    "About to save file:\n{}\nin romfs dump. Continue?",
                    &dest_file
                );
                if MessageDialog::new()
                    .set_title("Warning")
                    .set_description(m)
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    == rfd::MessageDialogResult::No
                {
                    return Ok(());
                }
            }

            match app.active_tab {
                ActiveTab::DiretoryTree => {
                    if let Some(pack) = &mut app.pack {
                        if let Some(opened) = &mut pack.opened {
                            opened.save(dest_file)?;
                            return Ok(());
                        }
                    }
                }
                ActiveTab::TextBox => {
                    for ext in vec![".yml", ".yaml", ".json"] {
                        if dest_file.to_lowercase().ends_with(ext) {
                            write_string_to_file(
                                &dest_file,
                                &String::from_utf8(app.file_reader.buffer.clone())
                                    .unwrap_or("".to_string()),
                            );
                            return Ok(());
                        }
                    }
                    FileSaver::save_text_file_by_file_type(app, &dest_file);
                }
                ActiveTab::Settings => {}
            }
        }

        Ok(())
    }

    pub fn open_file_button_click(app: &mut TotkBitsApp) -> io::Result<()> {
        // Logic for opening a file
        if let Some(file) = FileDialog::new().pick_file() {
            let file_name = file.to_string_lossy().to_string();
            if !file.exists() {
                //open dialog forbids opening nonexistent files
                app.status_text = format!("File does not exist: {}", file_name);
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    app.status_text.clone(),
                ));
            }
            if !file_name.is_empty() {
                let old_path = app.opened_file.path.full_path.clone();
                let path = file_name.clone();
                /*app.opened_file = OpenedFile::from_path(file_name.clone(), TotkFileType::Other);
                app.opened_file.endian = None;
                app.opened_file.msyt = None;*/
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
            }
        } else {
            app.status_text = "No file selected".to_owned();
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "No file selected",
            ));
        }
        return Ok(());
    }

    pub fn close_all_click(app: &mut TotkBitsApp) {
        if MessageDialog::new()
            .set_title("Warning")
            .set_description("All opened files will close. Proceed?")
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            == rfd::MessageDialogResult::No
        {
            return;
        }
        app.opened_file = OpenedFile::default();
        //app.pack = None;
        app.root_node = TreeNode::new("ROOT".to_string(), "/".to_string());
        //app.text = String::new();
        app.settings.is_file_loaded = true;
        app.settings.is_tree_loaded = true;
        app.settings.is_dir_context_menu = false;
        app.settings.dir_context_pos = None;
        app.file_reader = FileReader::default();
        app.file_renamer = FileRenamer::default();
        app.text_searcher = TextSearcher::default();
        app.status_text = "All files closed".to_string();
    }
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
