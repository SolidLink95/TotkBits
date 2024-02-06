use crate::misc::open_file_dialog;
use eframe::egui::{self, Color32, CtxRef, ScrollArea, SelectableLabel, TextStyle, TopBottomPanel};
use eframe::epi::App;
use eframe::epi::Frame;
use egui::text::Fonts;
use native_dialog::{MessageDialog, MessageType};
use std::fs;
use std::io::Read;

#[derive(PartialEq)]
enum ActiveTab {
    DiretoryTree,
    TextBox,
}

struct TotkBitsApp {
    text: String,
    status_text: String,
    scroll: ScrollArea,
    active_tab: ActiveTab,
}
impl Default for TotkBitsApp {
    fn default() -> Self {
        Self {
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
        display_menu_bar(self, ctx);

        // Central panel (text area)
        egui::CentralPanel::default().show(ctx, |ui| {
            display_labels(self, ctx, ui);

            //scrollbar
            display_text_editor(self, ctx, ui);
        });

        // Bottom panel (status bar)
        TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status_text);
                // You can add more status information here
            });
        });
    }
}

fn display_menu_bar(ob: &mut TotkBitsApp, ctx: &CtxRef) {
    TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Open").clicked() {
                open_file_button_click(ob);
            }
            if ui.button("Save").clicked() {
                // Logic for saving the current text
                ob.status_text = "Save clicked".to_owned();
                MessageDialog::new()
                    .set_type(MessageType::Info)
                    .set_title("Save dialog")
                    .set_text("You just clicked save button")
                    .show_alert()
                    .unwrap();
            }
            // Add more menu items here
        });
    });
}

fn display_text_editor(ob: &mut TotkBitsApp, ctx: &CtxRef, ui: &mut egui::Ui) {
    //scrollbar
    ob.scroll.clone().show(ui, |ui| {
        let mut max_size = ui.available_size();
        max_size[1] -= get_label_height(ctx);
        ui.add_sized(
            max_size,
            egui::TextEdit::multiline(&mut ob.text).desired_rows(10),
        );
    })
}
fn display_labels(ob: &mut TotkBitsApp, ctx: &CtxRef, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        //let mut text_tab = ActiveTab::TextBox;
        if ui
            .add(SelectableLabel::new(
                ob.active_tab == ActiveTab::TextBox,
                "Byml editor",
            ))
            .clicked()
        {
            ob.active_tab = ActiveTab::TextBox;
        }
        if ui
            .add(SelectableLabel::new(
                ob.active_tab == ActiveTab::DiretoryTree,
                "Sarc files",
            ))
            .clicked()
        {
            ob.active_tab = ActiveTab::DiretoryTree;
        }
    });
}

fn open_file_button_click(ob: &mut TotkBitsApp) {
    // Logic for opening a file
    let file_name = open_file_dialog();
    if file_name.len() > 0 {
        ob.status_text = format!("Opened file: {}", file_name).to_owned();
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

pub fn run() {
    let options = eframe::NativeOptions::default();
    eframe::run_native(Box::new(TotkBitsApp::default()), options);
}
