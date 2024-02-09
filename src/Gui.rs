use crate::misc::{self, open_file_dialog, save_file_dialog};
use crate::BymlFile::byml_file;
use crate::Pack::PackFile;
use crate::TotkPath::TotkPath;
use crate::Tree::{self, test_tree, tree_node};
use crate::Zstd::{totk_zstd, ZsDic};
use eframe::egui::{
    self, Color32, ScrollArea, SelectableLabel, TextStyle, TextureId, TopBottomPanel,
};
use egui::emath::Numeric;
use egui::text::Fonts;
use egui::{CollapsingHeader, Context};
use image::io::Reader as ImageReader;
use native_dialog::{MessageDialog, MessageType};
use roead::byml::Byml;
use std::borrow::BorrowMut;
use std::io::Read;
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
    byml: Option<Byml>,
    root_node: Rc<tree_node<String>>,
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
            root_node: tree_node::new("ROOT".to_string()),
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
        let mut theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx());
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
                });
            }
            ActiveTab::DiretoryTree => {
                ob.scroll.clone().show(ui, |ui| {
                    Gui::open_byml_or_sarc(ob, ui);
                    if !ob.pack.is_none() {
                        Tree::update_from_sarc_paths(&ob.root_node, &ob.pack.as_mut().expect("Error passing pack file"));
                        for child in ob.root_node.children.borrow().iter() {
                            display_tree_in_egui(&child, ui);
                        }
                    }
                });
            }
        }
    }

    fn open_byml_or_sarc(ob: &mut TotkBitsApp, ui: &mut egui::Ui) {
        if ob.is_file_loaded {
            return;
        }
        let p = PathBuf::from(ob.opened_file.clone());
        println!("Is {} a sarc?", ob.opened_file.clone());
        match PackFile::new(ob.opened_file.clone(), ob.zstd.clone()) {
            Ok(pack) => {
                ob.pack = Some(pack);
                ob.is_file_loaded = true;
                println!("Sarc  opened!");
                return;
            }
            Err(_) => {}
        }
        println!("Is {} a byml?", ob.opened_file.clone());
        match byml_file::new(ob.opened_file.clone(), ob.zstd.clone()) {
            Ok(b) => {
                //ob.byml = Some(b.pio);
                match &Some(b.pio) {
                    Some(x) => {
                        ob.text = Byml::to_text(&x);
                        ob.byml = Some(x.clone());
                        ob.active_tab = ActiveTab::TextBox;
                        ob.is_file_loaded = true;
                        println!("Byml  opened!");
                    }
                    None => {}
                };
            }
            Err(_) => {}
        };
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

    fn get_label_height(ctx: &egui::Context) -> f32 {
        //ctx.fonts() unused
        //    .layout_no_wrap("Example".to_string(), TextStyle::Heading, Color32::BLACK)
        //    .size()
        //    .y
        0.0
    }
}

pub fn display_tree_in_egui(root_node: &Rc<tree_node<String>>, ui: &mut egui::Ui) {
    CollapsingHeader::new(root_node.value.clone())
        .default_open(false)
        .show(ui, |ui| {
            for child in root_node.children.borrow().iter() {
                if !tree_node::is_leaf(&child) {
                    display_tree_in_egui(child, ui);
                } else {
                    ui.horizontal(|ui| {
                        if ui.button(child.value.clone()).clicked() {
                            println!("Clicked {}", child.value);
                        }
                    });
                }
            }
        });
}

pub fn run() {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Totkbits",
        options,
        Box::new(|_cc| Box::<TotkBitsApp>::default()),
    )
    .unwrap();
}
