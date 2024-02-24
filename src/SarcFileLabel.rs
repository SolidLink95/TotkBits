use std::io;
use std::rc::Rc;

use crate::BinTextFile::{BymlFile, FileData, OpenedFile};
use crate::ButtonOperations::ButtonOperations;
use crate::Settings::Styles;
use msyt::converter::MsytFile;

use crate::Gui::{ActiveTab, TotkBitsApp};
use crate::Tree::TreeNode;
use crate::Zstd::{is_aamp, is_byml, is_msyt, FileType};

use egui::{
    CollapsingHeader, Id, Response, Sense, TextStyle, Widget, WidgetInfo, WidgetText, WidgetType,
};
use egui::{Color32, Pos2, Rect, SelectableLabel, Vec2};
use roead::byml::Byml;

pub struct SarcLabel {
    //root_node: Rc<TreeNode<String>>,
    //node: &'a Rc<TreeNode<String>>,
    //app: &'a mut TotkBitsApp<'a>,
    //ui: &'a egui::Ui,
}

impl SarcLabel {
    pub fn new() -> Self {
        Self {} // { node, app }
    }

    fn display_leaf_node(
        app: &mut TotkBitsApp,
        child: &Rc<TreeNode<String>>,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
    ) {
        let style = Styles::get_style_from_comparer(app, ui, &child);
        //ctx.set_style(Styles::get_style_from_comparer(app,ui,&child));
        ui.horizontal(|ui: &mut egui::Ui| {
            ui.set_style(style);
            let is_selected = SarcLabel::is_internal_file_selected(app, child);

            //if is_selected {
            let file_label = ui.add(SelectableLabelSarc::new(is_selected, &child.value));
            //file_label.rect.size().y

            if file_label.double_clicked() {
                //println!("Clicked {}", child.full_path.clone());
                app.internal_sarc_file = Some(child.clone());
                SarcLabel::safe_open_file_from_opened_sarc(app, ui, &child)
            }
            if file_label.clicked() || file_label.secondary_clicked() {
                //println!("Double Clicked {}", child.full_path.clone());
                app.internal_sarc_file = Some(child.clone());
            }
            ctx.set_style(app.settings.styles.context_menu.clone());
            file_label.context_menu(|ui| {
                if ui.button("Open").clicked() {
                    SarcLabel::safe_open_file_from_opened_sarc(app, ui, &child);
                    ui.close_menu();
                }
                if ui.button("Extract").clicked() {
                    let _ = ButtonOperations::extract_click(app);
                    ui.close_menu();
                }
                if ui.button("Add").clicked() {
                    println!("Add");
                    let _ = ButtonOperations::add_click(app, &child);
                    ui.close_menu();
                }
                if ui.button("Remove").clicked() {
                    println!("Remove"); //fix this ugly code
                    let _ = ButtonOperations::remove_click(app, &child);
                    ui.close_menu();
                }
                if ui.button("Replace").clicked() {
                    println!("Replace");
                    ui.close_menu();
                }
                if ui.button("Rename").clicked() {
                    println!("Rename");
                    ui.close_menu();
                }
            });
            ctx.set_style(app.settings.styles.def_style.clone());
            //ctx.set_style(app.settings.styles.context_menu.clone());
        });
    }

    pub fn display_tree_in_egui(
        app: &mut TotkBitsApp,
        root_node: &Rc<TreeNode<String>>,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
    ) {
        if TreeNode::is_leaf(&root_node) && root_node.is_file() {
            //rarely files in sarc are in root directory
            SarcLabel::display_leaf_node(app, root_node, ui, ctx);
            return;
        }
        let response = CollapsingHeader::new(&root_node.value)
            .default_open(false)
            .show(ui, |ui| {
                let children: Vec<_> = root_node.children.borrow().iter().cloned().collect(); //prevents borrowing issues
                for child in children {
                    if TreeNode::is_leaf(&child) && child.is_file() {
                        SarcLabel::display_leaf_node(app, &child, ui, ctx);
                    } else {
                        SarcLabel::display_tree_in_egui(app, &child, ui, ctx);
                    }
                }
            }).header_response;
            SarcLabel::display_dir_context_menu(app, root_node, ui, ctx, response);
    }

    pub fn display_dir_context_menu( //TODO: make as separate widget
        app: &mut TotkBitsApp,
        child: &Rc<TreeNode<String>>,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        response: Response,
    ) {
        if response.secondary_clicked() {
            app.settings.is_dir_context_menu = true;
            app.internal_sarc_file = Some(child.clone());
        }
        if response.clicked() {
            app.internal_sarc_file = Some(child.clone());
            if let Some(internal_file) = &app.internal_sarc_file {
                if internal_file.path.full_path == child.path.full_path {
                    println!("CLosed context menu - another node selected");
                    app.settings.is_dir_context_menu = false;
                    app.settings.dir_context_pos = None;
                }
            }
        }

        if !app.settings.is_dir_context_menu {
            return;
        }

        if let Some(internal_file) = &app.internal_sarc_file {
            if internal_file.path.full_path != child.path.full_path {
                return;
            }
            //let last_click = ui.input(|i| i.pointer.interact_pos());
            if app.settings.dir_context_pos.is_none() {
                app.settings.dir_context_pos = ui.input(|i| i.pointer.interact_pos());
            }
            if let Some(pos) = &app.settings.dir_context_pos {
                let margin = Vec2::new(5.0, 5.0); // Additional margin for width and height
                let little_margin = Vec2::new(1.0, 1.0); // Additional margin for width and height
                    
                let mut rect_pos = pos.clone();
                let mut dir_pos = pos.clone() + margin;
                


                egui::Area::new(ui.id()).fixed_pos(dir_pos).show(ctx, |ui| {
                    ui.set_style(app.settings.styles.context_menu.clone());
                    if app.settings.dir_context_size.is_none() {
                        app.settings.dir_context_size = Some(
                            ui.allocate_ui(Vec2::new(ui.available_width(), 0.0), |ui| {
                                ui.vertical(|ui| {
                                    ui.button("Button 1");
                                    ui.button("Button 2");
                                    ui.button("Button 3");
                                    ui.button("Button 4");
                                });
                            })
                            .response,
                        );
                    }

                    let margin = Vec2::new(10.0, 10.0); // Additional margin for width and height
                    let mut rect_size = app.settings.dir_context_size.as_ref().unwrap().rect.size() +  margin;

                    // Calculate the position for the rectangle
                    //let mut rect_pos = response.rect.min - margin;

                    // Draw the rectangle
                    let rect = Rect::from_min_size(rect_pos, rect_size);

                    let outer_rect = Rect::from_min_size(rect_pos-little_margin, rect_size+(2.0*little_margin));
                    let button_width = egui::vec2(rect.width() - margin.x, ui.spacing().interact_size.y);

                    ui.painter().rect_filled(outer_rect, 0.0, Color32::from_gray(60));
                    ui.painter().rect_filled(rect, 0.0, app.settings.window_color);
                    
                    //SarcLabel::display_dir_context_menu_bg(app, ui, rect_pos);

                    //if ui.button("Add   ").clicked() {
                    if ui.add(egui::Button::new("Add").min_size(button_width)).clicked() {
                        println!("Add   ");
                        let _ = ButtonOperations::add_click(app, &child);
                        app.settings.is_dir_context_menu = false;
                        app.settings.dir_context_pos = None;
                    }
                    //if ui.button("Remove").clicked() {
                    if ui.add(egui::Button::new("Remove").min_size(button_width)).clicked() {
                        println!("Remove"); //fix this ugly code
                        let _ = ButtonOperations::remove_click(app, &child);
                        app.settings.is_dir_context_menu = false;
                        app.settings.dir_context_pos = None;
                    }
                    if ui.add(egui::Button::new("Rename").min_size(button_width)).clicked() {
                        println!("Rename");
                        app.settings.is_dir_context_menu = false;
                        app.settings.dir_context_pos = None;
                    }
                    if ui.add(egui::Button::new("Close").min_size(button_width)).clicked() {
                        println!("Close ");
                        app.settings.is_dir_context_menu = false;
                        app.settings.dir_context_pos = None;
                    }
                });
                //ctx.set_style(app.settings.styles.def_style.clone());
            }
        }

        if response.clicked_elsewhere() {
            if let Some(internal_file) = &app.internal_sarc_file {
                if internal_file.path.full_path == child.path.full_path {
                    println!("CLosed context menu");
                    app.settings.is_dir_context_menu = false;
                    app.settings.dir_context_pos = None;
                }
            }
        }
    }

    pub fn display_dir_context_menu_bg(app: &mut TotkBitsApp, ui: &mut egui::Ui, pos: Pos2) {
        let mut rect_pos = pos.clone();
        let margin = Vec2::new(10.0, 10.0); // Additional margin for width and height
                                            //let rect_size = response_rect.response.rect.size() + 2.0 * margin;
        let mut rect_size = Vec2::new(68.8, 102.0);
        rect_pos -= margin;
        // Draw the rectangle
        rect_size += margin * 2.0;
        let rect = Rect::from_min_size(rect_pos, rect_size);
        //println!("{:?} {:?}", rect, rect_size);
        ui.painter()
            .rect_filled(rect, 0.0, app.settings.window_color);
    }

    pub fn is_internal_file_selected(app: &mut TotkBitsApp, child: &Rc<TreeNode<String>>) -> bool {
        if let Some(internal_file) = &app.internal_sarc_file {
            if internal_file.path.full_path == child.path.full_path {
                return true;
            }
        }
        return false;
    }

    pub fn safe_open_file_from_opened_sarc(
        app: &mut TotkBitsApp,
        ui: &mut egui::Ui,
        child: &Rc<TreeNode<String>>,
    ) {
        match SarcLabel::open_file_from_opened_sarc(app, ui, child) {
            Ok(_) => {}
            Err(err) => {
                eprintln!(
                    "Failed to open {}, \nError: {:?}",
                    &child.path.full_path.clone(),
                    err
                );
                app.status_text = format!("Failed to open {}", &child.path.full_path.clone());
            }
        }
    }

    fn open_file_from_opened_sarc(
        app: &mut TotkBitsApp,
        _ui: &mut egui::Ui,
        child: &Rc<TreeNode<String>>,
    ) -> io::Result<()> {
        if app.pack.is_none() {
            return Err(io::Error::new(io::ErrorKind::Other, "No sarc opened"));
        }
        let path = &child.path.full_path.clone();
        let prob_sarc = &app.pack.as_ref().unwrap().opened;
        if let Some(op_sarc) = &prob_sarc {
            let data = op_sarc.sarc.get_data(&path);
            if data.is_none() {
                return Err(io::Error::new(io::ErrorKind::Other, "File absent in sarc"));
            }
            //For now assume only byml and msyt files will be opened
            let raw_data = data.unwrap().to_vec();
            if is_msyt(&raw_data) {
                app.text = MsytFile::binary_to_text(raw_data).expect("Error getting file msyt");
                app.opened_file = OpenedFile::new(
                    path.to_string(),
                    FileType::Msbt,
                    Some(roead::Endian::Little),
                    None,
                );
                app.settings.is_file_loaded = true; //precaution
            } else if is_byml(&raw_data) {
                let file_data = FileData::from(raw_data, FileType::Byml);
                let the_byml = BymlFile::from_binary(
                    file_data,
                    app.zstd.clone(),
                    child.path.full_path.clone(),
                )?;
                app.opened_file = OpenedFile::new(
                    path.to_string(),
                    FileType::Byml,
                    Some(roead::Endian::Little),
                    None,
                );
                app.text = Byml::to_text(&the_byml.pio);
                app.opened_file.byml = Some(the_byml);
                app.settings.is_file_loaded = true; //precaution
            } else if is_aamp(&raw_data) {
                //placeholder for aamp
            }
            if app.settings.is_file_loaded {
                //something got opened
                app.internal_sarc_file = Some(child.clone());
                app.active_tab = ActiveTab::TextBox;
                app.status_text = format!("Opened: {}", path);
            }
        }

        Ok(())
    }
}

#[must_use = "You should put this widget in an ui with `ui.add(widget);`"]
pub struct SelectableLabelSarc {
    selected: bool,
    text: WidgetText,
}

impl SelectableLabelSarc {
    pub fn new(selected: bool, text: impl Into<WidgetText>) -> Self {
        Self {
            selected,
            text: text.into(),
        }
    }
}

impl Widget for SelectableLabelSarc {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let Self { selected, text } = self;

        let button_padding = ui.spacing().button_padding;
        let total_extra = button_padding + button_padding;

        let wrap_width = ui.available_width() - total_extra.x;
        let galley = text.into_galley(ui, None, wrap_width, TextStyle::Button);

        let mut desired_size = total_extra + galley.size();
        desired_size.y = desired_size.y.max(ui.spacing().interact_size.y);
        let (rect, response) = ui.allocate_at_least(desired_size, Sense::click());
        response.widget_info(|| {
            WidgetInfo::selected(WidgetType::SelectableLabel, selected, galley.text())
        });

        if ui.is_rect_visible(response.rect) {
            let text_pos = ui
                .layout()
                .align_size_within_rect(galley.size(), rect.shrink2(button_padding))
                .min;

            let visuals = ui.style().interact_selectable(&response, selected);

            //if selected || response.hovered() || response.highlighted() || response.has_focus() {
            let rect = rect.expand(visuals.expansion);

            ui.painter().rect(
                rect,
                visuals.rounding,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
            );
            //}

            ui.painter().galley(text_pos, galley, visuals.text_color());
        }

        response
    }
}
