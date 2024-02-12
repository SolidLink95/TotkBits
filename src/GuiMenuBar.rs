use crate::Gui::{Flags, TotkBitsApp};
use crate::Tree::{self, tree_node};
use crate::Zstd::{is_byml, totk_zstd, ZsDic};
//use crate::SarcFileLabel::ScrollAreaPub;
use eframe::egui::{
    self, Color32, ScrollArea, SelectableLabel, Style, TextStyle, TextureId, TopBottomPanel
};
use egui::text::LayoutJob;
use egui::{CollapsingHeader, Context, Margin, Vec2};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{any, fs, io};

pub struct MenuBar {
    //app: &'a TotkBitsApp<'a>,
    style: Arc<Style>
}

impl MenuBar {
    pub fn new(style: &Style) -> io::Result<MenuBar> {
        Ok(MenuBar {
                style: Self::get_style(style)
            })
    }




    pub fn display(&self, app: &mut TotkBitsApp, ctx: &egui::Context) {
        let original_style = ctx.style().clone();
        ctx.set_style(self.style.clone());
        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open").clicked() {}
                    if ui.button("Save").clicked() {}
                    if ui.button("Save as").clicked() {}
                    if ui.button("Close all").clicked() {
                        app.pack = None;
                        app.byml = None;
                        app.root_node = tree_node::new("ROOT".to_string(), "/".to_string());
                        app.text = String::new();
                        app.flags = Flags::new(true, true);
                        ui.close_menu();
                    }
                    if ui.button("Restart").clicked() {}
                    if ui.button("Exit").clicked() {}
                });

                ui.menu_button("Tools", |ui| {
                    if ui.button("Find").clicked() {}
                    if ui.button("Settings").clicked() {}
                    if ui.button("Zoom in").clicked() {}
                    if ui.button("Zoom out").clicked() {}
                });
            });
            ui.add_space(1.0);
        //let _ = ui.set_style(original_style);
        });
        let _ = ctx.set_style(original_style);
    }

    fn get_style(style: &Style) -> Arc<Style> {
        let mut style: Style = style.clone();
        let square_rounding = egui::Rounding::same(0.0);
        let inactive_color = Color32::from_gray(27);
        let transparent = Color32::TRANSPARENT;
        style.spacing.item_spacing.x = 1.0;
        //Buttons have the same colors as background
        style.visuals.widgets.noninteractive.weak_bg_fill  = inactive_color;
        style.visuals.widgets.inactive.weak_bg_fill  = inactive_color;
        //No outline
        style.visuals.widgets.noninteractive.bg_stroke.color = transparent;
        style.visuals.widgets.inactive.bg_stroke.color = transparent;
        style.visuals.widgets.active.bg_stroke.color = transparent;
        style.visuals.widgets.hovered.bg_stroke.color = transparent;
        style.visuals.widgets.open.bg_stroke.color = transparent;

        style.visuals.widgets.noninteractive.fg_stroke.color = Color32::WHITE; // White text

        //Square rounding/edges
        style.visuals.widgets.noninteractive.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.inactive.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.hovered.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.active.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.open.rounding = square_rounding; // No rounding on buttons
        style.visuals.window_rounding = square_rounding;
        style.visuals.widgets.noninteractive.fg_stroke.width = 1.0; // Width of the border line
        style.spacing.button_padding = Vec2::new(10.0, 4.0); // Padding inside the buttons
        style.spacing.window_margin = Margin::symmetric(4.0, 4.0); // Margin around the window

        return Arc::new(style);
    }
}
