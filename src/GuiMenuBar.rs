use crate::ButtonOperations::ButtonOperations;
use crate::Gui::TotkBitsApp;
use crate::Tree::TreeNode;

//use crate::SarcFileLabel::ScrollAreaPub;
use eframe::egui::{self, Style, TopBottomPanel};

use std::io;
use std::sync::Arc;

pub struct MenuBar {
    //app: &'a TotkBitsApp<'a>,
    style: Arc<Style>,
}

impl MenuBar {
    pub fn new(style: Arc<Style>) -> io::Result<MenuBar> {
        Ok(MenuBar { style: style })
    }

    pub fn display(&self, app: &mut TotkBitsApp, ctx: &egui::Context) {
        let original_style = ctx.style().clone();
        ctx.set_style(self.style.clone());
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
                    if ui.button("Exit").clicked() {}
                });

                ui.menu_button("Tools", |ui| {
                    if ui.button("Edit").clicked() {
                        ButtonOperations::edit_click(app, ui);
                        ui.close_menu();
                    }
                    if ui.button("Extract").clicked() {
                        ButtonOperations::extract_click(app);
                        ui.close_menu();
                    }
                    if ui.button("Find").clicked() {}
                    if ui.button("Settings").clicked() {}
                    if ui.button("Zoom in").clicked() {}
                    if ui.button("Zoom out").clicked() {}
                });

                app.settings.fps_counter.display(ui);
            });
            ui.add_space(1.0);
        });

        let _ = ctx.set_style(original_style);
    }
}

struct DirContextMenu {
    pub style: Arc<Style>,
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
            return 60.0
        }
        self.fps
    }
}
