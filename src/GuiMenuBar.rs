use crate::ButtonOperations::ButtonOperations;
use crate::Gui::TotkBitsApp;


//use crate::SarcFileLabel::ScrollAreaPub;
use eframe::egui::{self, Style, TopBottomPanel};
use rfd::MessageDialog;

use std::collections::HashMap;
use std::sync::Arc;
use std::{io, process};

pub struct MenuBar {
    //app: &'a TotkBitsApp<'a>,
    pub buttons: HashMap<String, String>,
    style: Arc<Style>,
    pub padding: usize,
}

impl MenuBar {
    pub fn new(style: Arc<Style>) -> io::Result<MenuBar> {
        let mut b: HashMap<String, String> = Default::default();
        b.insert("Open".to_string(), "Ctrl+O".to_string());
        b.insert("Save".to_string(), "Ctrl+S".to_string());
        b.insert("Save as".to_string(), "Ctrl+Shift+S".to_string());
        b.insert("Close all".to_string(), "Ctrl+W".to_string());
        b.insert("Exit".to_string(), "Ctrl+Q".to_string());
        b.insert("Edit".to_string(), "Ctrl+E".to_string());
        b.insert("Extract".to_string(), "Ctrl+R".to_string());
        b.insert("Find".to_string(), "Ctrl+F".to_string());
        b.insert("Zoom in".to_string(), "Ctrl+Shift++".to_string());
        b.insert("Zoom out".to_string(), "Ctrl+Shift+-".to_string());
        let padding = b
            .keys()
            .max_by_key(|k| k.len())
            .unwrap_or(&"a".repeat(10).to_string())
            .len()
            + 10;
        Ok(MenuBar {
            buttons: b,
            style: style,
            padding: padding,
        })
    }

    pub fn display(&self, app: &mut TotkBitsApp, ctx: &egui::Context) {
        let original_style = ctx.style().clone();
        ctx.set_style(self.style.clone());
        let _x = self.padding;
        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("File", |ui| {
                    
                    if ui.button("Open").clicked() {
                        let _ = ButtonOperations::open_file_button_click(app);
                        ui.close_menu();
                    }
                    if ui.button("Save").clicked() {
                        ButtonOperations::save_click(app);
                        ui.close_menu();
                    }
                    if ui.button("Save as").clicked() {
                        let _ = ButtonOperations::save_as_click(app);
                        ui.close_menu();
                    }
                    if ui.button("Close all").clicked() {
                        ButtonOperations::close_all_click(app);
                        ui.close_menu();
                    }
                    if ui.button("Exit").clicked() {
                        if MessageDialog::new()
                            .set_title("Warning")
                            .set_description("Exit?")
                            .set_buttons(rfd::MessageButtons::YesNo)
                            .show()
                            == rfd::MessageDialogResult::Yes
                        {
                            process::exit(0); // Replace 0 with the desired exit code
                        }
                    }
                });

                ui.menu_button("Tools", |ui| {
                    if ui.button("Edit").clicked() {
                        ButtonOperations::edit_click(app, ui);
                        ui.close_menu();
                    }
                    if ui.button("Extract").clicked() {
                        let _ = ButtonOperations::extract_click(app);
                        ui.close_menu();
                    }
                    if ui.button("Find").clicked() {
                        app.text_searcher.is_shown = true;
                        ui.close_menu();
                    }
                    if ui.button("Settings").clicked() {}
                    if ui.button("Zoom in").clicked() {
                        app.settings.styles.scale.add(0.1);
                        ui.close_menu();
                    }
                    if ui.button("Zoom out").clicked() {
                        app.settings.styles.scale.add(-0.1);
                        ui.close_menu();
                    }

                    /*if ui.button("Wrap/unwrap all").clicked() {
                        app.settings.is_sarclabel_wrapped = !app.settings.is_sarclabel_wrapped;
                        //app.settings.is_tree_loaded = false;
                        ui.close_menu();
                    }*/

                });

                //ui.label(format!("{:?}", ctx.pixels_per_point()));

                app.settings.fps_counter.display(ui);
            });
            ui.add_space(1.0);
        });

        let _ = ctx.set_style(original_style);
    }
}


pub struct FpsCounter {
    last_update: std::time::Instant,
    frame_count: usize,
    fps: f32,
    is_shown: bool,
}

impl FpsCounter {
    pub fn new() -> Self {
        FpsCounter {
            last_update: std::time::Instant::now(),
            frame_count: 0,
            fps: 0.0,
            is_shown: true,
        }
    }

    fn update(&mut self) {
        self.frame_count += 1;
        let now = std::time::Instant::now();
        if now.duration_since(self.last_update).as_secs_f32() > 0.2 {
            self.fps = self.frame_count as f32 / now.duration_since(self.last_update).as_secs_f32();
            self.frame_count = 0;
            self.last_update = now;
        }
    }

    fn display(&mut self, ui: &mut egui::Ui) {
        if self.is_shown {
            self.update();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.label(format!("FPS: {}", self.fps() as i32));
            });
        }
    }

    fn fps(&self) -> f32 {
        if self.fps > 60.0 {
            return 60.0;
        }
        self.fps
    }
}

pub struct GenericF32 {
    pub val: f32,
    pub max: f32,
    pub min: f32
}

impl GenericF32 {
    pub fn new(val: f32, min: f32, max: f32) -> Self {
        Self {
            val: val,
            min: min,
            max: max
        }
    }
    pub fn update(&mut self) {
        self.val = self.val.max(self.min).min(self.max);
    }

    pub fn add(&mut self, x: f32) {
        self.val += x;
        self.update();
    }
}