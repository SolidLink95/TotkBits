
use eframe::egui::{self, CtxRef, Layout, TopBottomPanel, ScrollArea,TextStyle, Color32};
use eframe::epi::App;
use eframe::epi::Frame;
use egui::text::Fonts;
use std::fs;
use std::io::Read;
use crate::misc::open_file_dialog;

struct NotepadApp {
    text: String,
    status_text: String,
    scroll: egui::ScrollArea,
}

impl Default for NotepadApp {
    fn default() -> Self {
        Self {
            text: "Initial content of the text box".to_owned(),
            status_text: "Ready".to_owned(),
            scroll: egui::ScrollArea::vertical(), 
        }
    }
}

impl App for NotepadApp {
    fn name(&self) -> &str {
        "Totkbits"
    }

    fn update(&mut self, ctx: &CtxRef, _frame: &mut Frame) {
        // Top panel (menu bar)
        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    open_file_button_click(self);
                }
                if ui.button("Save").clicked() {
                    // Logic for saving the current text
                    self.status_text = "Save clicked".to_owned();
                }
                // Add more menu items here
            });
        });

        // Central panel (text area)
        egui::CentralPanel::default().show(ctx, |ui| {
            //no scrollbar
            //ui.add_sized(ui.available_size(), egui::TextEdit::multiline(&mut self.text).desired_rows(10));
            //scrollbar
            self.scroll.clone().show(ui, |ui| {
                let mut max_size = ui.available_size();
                max_size[1] -= get_label_height(ctx);
                ui.add_sized(max_size, egui::TextEdit::multiline(&mut self.text).desired_rows(10));
            })
            //ui.scroll_area("text_area_scroll").show(ui.available_size(), |ui| {
            //    ui.add(egui::TextEdit::multiline(&mut self.text).desired_rows(10));
            //});
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


fn open_file_button_click(ob: &mut NotepadApp) {
    // Logic for opening a file
    let file_name = open_file_dialog();
    if file_name.len() > 0 {
        ob.status_text = format!("Opened file: {}", file_name).to_owned();
        let mut f_handle = fs::File::open(file_name.clone()).unwrap();
        let mut buffer: Vec<u8> = Vec::new();//String::new();
        match f_handle.read_to_end(&mut buffer) {
            Ok(_) => ob.text = String::from_utf8_lossy(&buffer).to_string(),
            Err(err) => ob.status_text = format!("Error reading file: {}", file_name)
        }
        //self.text = buffer;
    }
    else {
        ob.status_text = "No file selected".to_owned();
    }
    //self.status_text = open_file_dialog();
}

fn get_label_height(ctx: &CtxRef) -> f32 {
    ctx.fonts().layout_no_wrap(
        "Example".to_string(), 
        TextStyle::Heading, 
        Color32::BLACK
    ).size().y
}

pub fn run() {
    
    let options = eframe::NativeOptions::default();
    eframe::run_native(Box::new(NotepadApp::default()), options);
}

