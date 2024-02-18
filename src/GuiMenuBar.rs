use crate::ButtonOperations::ButtonOperations;
use crate::Gui::{OpenedFile, TotkBitsApp};
use crate::Tree::{TreeNode};

//use crate::SarcFileLabel::ScrollAreaPub;
use eframe::egui::{
    self, Style, TopBottomPanel
};




use std::sync::Arc;
use std::{io};

pub struct MenuBar {
    //app: &'a TotkBitsApp<'a>,
    style: Arc<Style>
}

impl MenuBar {
    pub fn new(style: Arc<Style>) -> io::Result<MenuBar> {
        Ok(MenuBar {
                style: style
            })
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
                    /*let egui_icon = include_image!("../res/open_icon.png"); 
                    if ui.add(Button::image(egui::Image::new(egui_icon.clone()).fit_to_exact_size(Vec2::new(32.0, 32.0)))).clicked() {
                            println!("Test");
                        }; image button example*/
                });
            });
            ui.add_space(1.0);
        //let _ = ui.set_style(original_style);
        });
        let _ = ctx.set_style(original_style);
    }

}

struct DirContextMenu {
pub style: Arc<Style>
}



