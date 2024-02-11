use crate::Gui::TotkBitsApp;
use crate::Tree::{self, tree_node};
use crate::Zstd::{is_byml, totk_zstd, ZsDic};
//use crate::SarcFileLabel::ScrollAreaPub;
use eframe::egui::{
    self, Color32, ScrollArea, SelectableLabel, Style, TextStyle, TextureId, TopBottomPanel
};
use egui::text::LayoutJob;
use egui::{CollapsingHeader, Context};
use std::path::{Path, PathBuf};
use std::{any, fs, io};

pub struct MenuBar {
    //app: &'a TotkBitsApp<'a>,
}

impl MenuBar {
    pub fn new() -> io::Result<MenuBar> {
        Ok(MenuBar {
                //app: app
            })
    }




    pub fn display(app: &mut TotkBitsApp, ctx: &egui::Context) {
        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open").clicked() {}
                    if ui.button("Save").clicked() {}
                    if ui.button("Save as").clicked() {}
                    if ui.button("Close all").clicked() {}
                    if ui.button("Exit").clicked() {}
                });

                ui.menu_button("Tools", |ui| {
                    if ui.button("Find").clicked() {}
                    if ui.button("Settings").clicked() {}
                });
            });
        });
    }

    fn get_style(ctx: &Context) -> Style {
        let mut style: Style = (*ctx.style()).clone();
        style.visuals.widgets.noninteractive.bg_fill = Color32::from_gray(60);

        return style;
    }
}
