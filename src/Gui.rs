use crate::misc::{self, open_file_dialog, save_file_dialog};
use crate::BymlFile::byml_file;
use crate::Pack::PackFile;
use crate::TotkPath::TotkPath;
use crate::Tree::{self, tree_node};
use crate::Zstd::{is_byml, totk_zstd, ZsDic};
use eframe::egui::{
    self, Color32, ScrollArea, SelectableLabel, TextStyle, TextureId, TopBottomPanel,
};
use egui::{CollapsingHeader, Context};
use native_dialog::{MessageDialog, MessageType};
use rfd::FileDialog;
use roead::byml::Byml;
use roead::sarc::File;
use std::io::Read;
use std::os::raw;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::{any, fs, io};

#[derive(PartialEq)]
enum ActiveTab {
    DiretoryTree,
    TextBox,
}

struct TotkBitsApp<'a> {
    opened_file: String,
    text: String,
    status_text: String,
    scroll: ScrollArea,
    active_tab: ActiveTab,
    language: String,
    zstd: Arc<totk_zstd<'a>>,
    pack: Option<PackFile<'a>>,
    byml: Option<byml_file<'a>>,
    root_node: Rc<tree_node<String>>,
    internal_sarc_file: Option<Rc<tree_node<String>>>,
    is_file_loaded: bool,
}
impl Default for TotkBitsApp<'_> {
    fn default() -> Self {
        let totk_path = Arc::new(TotkPath::new());
        Self {
            opened_file: String::new(),
            //text: misc::get_example_yaml(),
            text: String::new(),
            status_text: "Ready".to_owned(),
            scroll: ScrollArea::vertical(),
            active_tab: ActiveTab::DiretoryTree,
            language: "toml".into(),
            zstd: Arc::new(totk_zstd::new(totk_path, 16).unwrap()),
            pack: None,
            byml: None,
            root_node: tree_node::new("ROOT".to_string(), "/".to_string()),
            internal_sarc_file: None,
            is_file_loaded: true,
        }
    }
}

impl eframe::App for TotkBitsApp<'_> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Top panel (menu bar)
        Gui::display_menu_bar(self, ctx);

        // Bottom panel (status bar)
        Gui::display_status_bar(self, ctx);
        // Central panel (text area)
        egui::CentralPanel::default().show(ctx, |ui| {
            Gui::display_labels(self, ui);

            Gui::display_main(self, ui);
        });
    }
}

struct Gui {}

impl Gui {
    pub fn display_status_bar(ob: &mut TotkBitsApp, ctx: &egui::Context) {
        TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&ob.status_text);
            });
        });
    }

    pub fn display_menu_bar(ob: &mut TotkBitsApp, ctx: &egui::Context) {
        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    Gui::open_file_button_click(ob);
                }
                if ui.button("Save").clicked() {
                    if ob.opened_file.len() > 0 {
                        println!("Saving file to {}", ob.opened_file);
                    }
                }
                if ui.button("Save as").clicked() {
                    let file_path = save_file_dialog();
                    if file_path.len() > 0 {
                        println!("Saving file to {}", file_path);
                    }
                }
            });
        });
    }

    pub fn display_main(ob: &mut TotkBitsApp, ui: &mut egui::Ui) {
        let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx());
        //ui.collapsing("Theme", |ui| {
        //    ui.group(|ui| {
        //        theme.ui(ui);
        //        theme.clone().store_in_memory(ui.ctx());
        //   })
        //});
        let language = ob.language.clone();
        let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
            let mut layout_job =
                egui_extras::syntax_highlighting::highlight(ui.ctx(), &theme, string, &language);
            layout_job.wrap.max_width = wrap_width;
            ui.fonts(|f| f.layout_job(layout_job))
        };

        match ob.active_tab {
            ActiveTab::TextBox => {
                //scrollbar
                ob.scroll.clone().show(ui, |ui| {
                    ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::multiline(&mut ob.text)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_rows(10)
                            .lock_focus(true)
                            .desired_width(f32::INFINITY)
                            .layouter(&mut layouter),
                    );
                    Gui::open_byml_or_sarc(ob, ui);
                    //TODO: get scrollbar position and render only that part of text
                    //println!("{:?}", ob.scroll.clone().show_viewport(ui, add_contents))
                });
            }
            ActiveTab::DiretoryTree => {
                ob.scroll
                    .clone()
                    .auto_shrink([false, false])
                    .max_height(ui.available_height())
                    .max_width(ui.available_width())
                    .show(ui, |ui| {
                        Gui::open_byml_or_sarc(ob, ui);
                        if !ob.pack.is_none() {
                            Tree::update_from_sarc_paths(
                                &ob.root_node,
                                &ob.pack.as_mut().expect("Error passing pack file"),
                            );
                            let children: Vec<_> =
                                ob.root_node.children.borrow().iter().cloned().collect();
                            for child in children {
                                Gui::display_tree_in_egui(ob, &child, ui);
                            }
                        }
                    });
            }
        }
    }

    fn open_byml_or_sarc(ob: &mut TotkBitsApp, ui: &mut egui::Ui) {
        if ob.is_file_loaded {
            return; //stops the app from infinite file loading from disk
        }
        println!("Is {} a sarc?", ob.opened_file.clone());
        match PackFile::new(ob.opened_file.clone(), ob.zstd.clone()) {
            Ok(pack) => {
                ob.pack = Some(pack);
                ob.is_file_loaded = true;
                println!("Sarc  opened!");
                ob.active_tab = ActiveTab::DiretoryTree;
                return;
            }
            Err(_) => {}
        }
        println!("Is {} a byml?", ob.opened_file.clone());
        let mut res_byml: Result<byml_file<'_>, io::Error> =
            byml_file::new(ob.opened_file.clone(), ob.zstd.clone());
        match res_byml {
            Ok(ref b) => {
                ob.text = Byml::to_text(&b.pio);
                ob.byml = Some(res_byml.unwrap());
                ob.active_tab = ActiveTab::TextBox;
                println!("Byml  opened!");
                ob.is_file_loaded = true;
                return;
            }

            Err(_) => {}
        };
        ob.is_file_loaded = true;
        ob.status_text = format!("Failed to open: {}", ob.opened_file.clone());
    }

    pub fn display_labels(ob: &mut TotkBitsApp, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .add(SelectableLabel::new(
                    ob.active_tab == ActiveTab::DiretoryTree,
                    "Sarc files",
                ))
                .clicked()
            {
                ob.active_tab = ActiveTab::DiretoryTree;
            }
            if ui
                .add(SelectableLabel::new(
                    ob.active_tab == ActiveTab::TextBox,
                    "Byml editor",
                ))
                .clicked()
            {
                ob.active_tab = ActiveTab::TextBox;
            }
        });
    }

    fn open_file_button_click(ob: &mut TotkBitsApp) -> io::Result<()> {
        // Logic for opening a file
        let file_name = open_file_dialog();
        if file_name.len() > 0 {
            println!("Attempting to read {} file", file_name.clone());
            ob.opened_file = file_name.clone();
            let mut f_handle = fs::File::open(file_name.clone())?;
            let mut buffer: Vec<u8> = Vec::new(); //String::new();
            match f_handle.read_to_end(&mut buffer) {
                Ok(_) => {
                    ob.status_text = format!("Opened file: {}", ob.opened_file.clone()).to_owned();
                    ob.is_file_loaded = false;
                    return Ok(());
                }
                Err(err) => {
                    ob.status_text = format!("Error reading file: {}", file_name);
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        ob.status_text.clone(),
                    ));
                }
            }
            //self.text = buffer;
        } else {
            ob.status_text = "No file selected".to_owned();
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "No file selected",
            ));
        }
    }

    fn display_tree_in_egui(
        ob: &mut TotkBitsApp,
        root_node: &Rc<tree_node<String>>,
        ui: &mut egui::Ui,
    ) {
        CollapsingHeader::new(root_node.value.clone())
            .default_open(false)
            .show(ui, |ui| {
                for child in root_node.children.borrow().iter() {
                    if !tree_node::is_leaf(&child) {
                        Gui::display_tree_in_egui(ob, child, ui);
                    } else {
                        ui.horizontal(|ui| {
                            let file_label = ui.add(SelectableLabel::new(Gui::is_internal_file_selected(ob, child), child.value.clone()));
                            if file_label.double_clicked()
                            {
                                //println!("Clicked {}", child.full_path.clone());
                                ob.internal_sarc_file = Some(child.clone());
                                Gui::safe_open_file_from_opened_sarc(ob, ui, child.full_path.clone())
                            }
                            if file_label.clicked() {
                                //println!("Double Clicked {}", child.full_path.clone());
                                ob.internal_sarc_file = Some(child.clone());

                            }
                            if file_label.secondary_clicked() {
                                println!("Mocking future context menu for ");
                            }
                        });
                    }
                }
            });
    }

    fn is_internal_file_selected(ob: &mut TotkBitsApp,  child: &Rc<tree_node<String>>) -> bool {
        match &ob.internal_sarc_file {
            Some(x) => {
                if x.full_path == child.full_path {return true;}
                return false;
            },
            None => {return false;}
        }
    }

    fn safe_open_file_from_opened_sarc(ob: &mut TotkBitsApp, ui: &mut egui::Ui, full_path: String) {
        match Gui::open_file_from_opened_sarc(ob, ui, full_path.clone()) {
            Ok(_) => {}
            Err(err) => {
                eprintln!(
                    "Failed to open {}, \nError: {:?}",
                    full_path.clone(),
                    err
                );
                ob.status_text = format!("Failed to open {}", full_path.clone());
            }
        }
    }

    fn open_file_from_opened_sarc(
        ob: &mut TotkBitsApp,
        ui: &mut egui::Ui,
        full_path: String,
    ) -> io::Result<()> {
        if ob.pack.is_none() {
            return Err(io::Error::new(io::ErrorKind::Other, "No sarc opened"));
        }
        let op_sarc = ob.pack.as_ref().unwrap();
        let data = op_sarc.sarc.get_data(&full_path.clone());
        if data.is_none() {
            return Err(io::Error::new(io::ErrorKind::Other, "File absent in sarc"));
        }
        //For now assume only byml files will be opened
        let raw_data = data.unwrap().to_vec();
        if !is_byml(&raw_data) {
            return Err(io::Error::new(io::ErrorKind::Other, "File is not a byml"));
        }
        let the_byml = byml_file::from_binary(&raw_data, ob.zstd.clone(), full_path)?;
        let text = Byml::to_text(&the_byml.pio);
        ob.text = text;
        ob.is_file_loaded = true; //precaution
        ob.active_tab = ActiveTab::TextBox;
        Ok(())
    }
}

//TODO: saving byml file,
fn save_as(ob: &mut TotkBitsApp) {
    match ob.active_tab {
        ActiveTab::DiretoryTree => {
            if ob.pack.is_none() {
                return; //no sarc opened, aborting
            }
            let file: Option<PathBuf> = FileDialog::new()
                .set_title("Save sarc file as")
                .set_file_name(ob.opened_file.clone())
                .save_file();
            if file.is_none() {
                return; //saving aborted
            }
            let dest_file: String = file
                .unwrap()
                .into_os_string()
                .into_string()
                .map_err(|os_str| format!("Path contains invalid UTF-8: {:?}", os_str))
                .unwrap();
            let res = ob.pack.as_mut().unwrap().save(dest_file.clone());
            match res {
                Ok(_) => {
                    ob.status_text = format!("Saved: {}", dest_file);
                }
                Err(err) => {
                    ob.status_text = format!("Error in save: {}", dest_file);
                }
            }
        }

        ActiveTab::TextBox => {
            if ob.pack.is_none() { //just byml
            }
        }
    }
}

pub fn run() {
    let mut options = eframe::NativeOptions::default();
    //options::viewport::initial_window_size(Some(egui::vec2(1000.0, 1000.0)));
    options.viewport.inner_size = Some(egui::vec2(700.0, 700.0));
    eframe::run_native(
        "Totkbits",
        options,
        Box::new(|_cc| Box::<TotkBitsApp>::default()),
    )
    .unwrap();
}
