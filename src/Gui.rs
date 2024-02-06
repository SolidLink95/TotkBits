use crate::misc::{open_file_dialog, save_file_dialog};
use eframe::egui::{self, Color32, CtxRef, ScrollArea, SelectableLabel, TextStyle, TopBottomPanel, TextureId};
use egui::emath::Numeric;
use egui::{Context};
use eframe::epi::App;
use eframe::epi::Frame;
use egui::text::Fonts;
use native_dialog::{MessageDialog, MessageType};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::{fs, io};
use image::io::Reader as ImageReader;

#[derive(PartialEq)]
enum ActiveTab {
    DiretoryTree,
    TextBox,
}

struct TotkBitsApp {
    opened_file: String,
    text: String,
    status_text: String,
    scroll: ScrollArea,
    active_tab: ActiveTab,
}
impl Default for TotkBitsApp {
    fn default() -> Self {
        Self {
            opened_file: String::new(),
            text: "Initial content of the text box".to_owned(),
            status_text: "Ready".to_owned(),
            scroll: ScrollArea::vertical(),
            active_tab: ActiveTab::TextBox,
        }
    }
}

impl App for TotkBitsApp {
    fn name(&self) -> &str {
        "Totkbits"
    }

    fn update(&mut self, ctx: &CtxRef, _frame: &mut Frame) {
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
    pub fn display_status_bar(ob: &mut TotkBitsApp, ctx: &CtxRef) {
        TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&ob.status_text);
                // You can add more status information here
            });
        });
    }
    


    pub fn display_menu_bar(ob: &mut TotkBitsApp, ctx: &CtxRef) {
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
        match ob.active_tab {
            ActiveTab::TextBox => {
                //scrollbar
                ob.scroll.clone().show(ui, |ui| {
                    ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::multiline(&mut ob.text).desired_rows(10),
                    );
                })
            }
            ActiveTab::DiretoryTree => {
                ui.allocate_space(ui.available_size());
                //ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::BLACK);
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
            ob.status_text = format!("Opened file: {}", file_name).to_owned();
            ob.opened_file = file_name.clone();
            let mut f_handle = fs::File::open(file_name.clone()).unwrap();
            let mut buffer: Vec<u8> = Vec::new(); //String::new();
            match f_handle.read_to_end(&mut buffer) {
                Ok(_) => ob.text = String::from_utf8_lossy(&buffer).to_string(),
                Err(err) => ob.status_text = format!("Error reading file: {}", file_name),
            }
            //self.text = buffer;
        } else {
            ob.status_text = "No file selected".to_owned();
        }
    }

    fn get_label_height(ctx: &CtxRef) -> f32 {
        ctx.fonts()
            .layout_no_wrap("Example".to_string(), TextStyle::Heading, Color32::BLACK)
            .size()
            .y
    }
}
pub fn run() {
    let options = eframe::NativeOptions::default();
    eframe::run_native(Box::new(TotkBitsApp::default()), options);
}
