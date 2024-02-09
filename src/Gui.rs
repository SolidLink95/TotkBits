use crate::misc::{self, open_file_dialog, save_file_dialog};
use crate::Pack::PackFile;
use crate::TotkPath::TotkPath;
use crate::Tree::{self, test_tree, tree_node};
use crate::Zstd::{totk_zstd, ZsDic};
use eframe::egui::{
    self, Color32, ScrollArea, SelectableLabel, TextStyle, TextureId, TopBottomPanel,
};
use egui::emath::Numeric;
use egui::{CollapsingHeader, Context};
//use eframe::epi::App;
//use eframe::epi::Frame;
use egui::text::Fonts;
use image::io::Reader as ImageReader;
use native_dialog::{MessageDialog, MessageType};
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
    //totk_path: Arc<TotkPath>,
    zstd: totk_zstd<'a>,
    reload_tree: bool
}
impl Default for TotkBitsApp<'_> {
    fn default() -> Self {
        let totk_path = Arc::new(TotkPath::new());
        Self {
            opened_file: String::new(),
            text: misc::get_example_yaml(),
            status_text: "Ready".to_owned(),
            scroll: ScrollArea::vertical(),
            active_tab: ActiveTab::DiretoryTree,
            language: "toml".into(),
            //totk_path:  totk_path.clone(),
            zstd: totk_zstd::new(totk_path, 16).unwrap(),
            reload_tree: false
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
                // You can add more status information here
            });
        });
    }

    pub fn display_menu_bar(ob: &mut TotkBitsApp, ctx: &egui::Context) {
        //ob.open_icon_id = Gui::load_icon("res/open_icon.png");
        //ob.save_icon_id = Gui::load_icon("res/save_icon.png");
        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    Gui::open_file_button_click(ob);
                }
                if ui.button("Save").clicked() {
                    // Logic for saving the current text
                    if ob.opened_file.len() > 0 {
                        println!("Saving file to {}", ob.opened_file);
                    }
                }
                if ui.button("Save as").clicked() {
                    // Logic for saving the current text
                    let file_path = save_file_dialog();
                    if file_path.len() > 0 {
                        println!("Saving file to {}", file_path);
                    }
                }

                // Add more menu items here
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

        let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
            let mut layout_job =
                egui_extras::syntax_highlighting::highlight(ui.ctx(), &theme, string, &ob.language);
            layout_job.wrap.max_width = wrap_width;
            ui.fonts(|f| f.layout_job(layout_job))
        };

        match ob.active_tab {
            ActiveTab::TextBox => {
                //scrollbar
                ob.scroll.clone().show(ui, |ui| {
                    ui.add_sized(
                        ui.available_size(),
                        //egui::TextEdit::multiline(&mut ob.text).code_editor().desired_rows(10),
                        egui::TextEdit::multiline(&mut ob.text)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_rows(10)
                            .lock_focus(true)
                            .desired_width(f32::INFINITY)
                            .layouter(&mut layouter),
                    );
                });
            }
            ActiveTab::DiretoryTree => {
                //println!("{:?} {:?}",)
                if ob.reload_tree{
                    if ob.opened_file.len() > 0 {
                        let p = PathBuf::from(ob.opened_file.clone());
                        let x: PackFile<'_> = PackFile::new(&p, &ob.zstd).unwrap();
                        let root_node: Rc<tree_node<String>> = tree_node::new("ROOT".to_string());
                        
                        Tree::update_from_sarc_paths(&root_node, x);
                        for child in root_node.children.borrow().iter() {
                            display_tree_in_egui(&child, ui);
                        }
                        //display_tree_in_egui(&root_node, ui);
                        //ob.reload_tree = !ob.reload_tree;
                }}
            }
        }
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

    fn open_file_button_click(ob: &mut TotkBitsApp) {
        // Logic for opening a file
        let file_name = open_file_dialog();
        if file_name.len() > 0 {
            ob.opened_file = file_name.clone();
            ob.reload_tree = true;
            let mut f_handle = fs::File::open(file_name.clone()).unwrap();
            let mut buffer: Vec<u8> = Vec::new(); //String::new();
            match f_handle.read_to_end(&mut buffer) {
                Ok(_) =>  ob.status_text = format!("Opened file: {}", file_name).to_owned(),
                Err(err) => ob.status_text = format!("Error reading file: {}", file_name),
            }
            //self.text = buffer;
        } else {
            ob.status_text = "No file selected".to_owned();
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
                }
                else {
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
