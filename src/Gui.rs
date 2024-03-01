use crate::file_format::BinTextFile::OpenedFile;
use crate::file_format::Pack::PackComparer;
use crate::ui_elements::InfoLabel::InfoLabel;
use crate::ButtonOperations::ButtonOperations;
use crate::FileReader::FileReader;
use crate::GuiMenuBar::MenuBar;
use crate::Open_Save::FileOpener;
use crate::SarcFileLabel::SarcLabel;
use crate::Settings::{FileRenamer, Icons, Settings, TextSearcher};
use crate::TotkConfig::TotkConfig;
use crate::Tree::{self, TreeNode};
use crate::Zstd::{TotkFileType, TotkZstd};
use eframe::egui::{self, ScrollArea, SelectableLabel, TopBottomPanel};
use egui::scroll_area::ScrollAreaOutput;
use egui::{pos2, Align, Button, Label, Layout, Rect, Vec2};
use egui_code_editor::{CodeEditor, Syntax};
use egui_extras::install_image_loaders;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

#[derive(PartialEq)]
pub enum ActiveTab {
    DiretoryTree,
    TextBox,
    Settings,
}

pub struct TotkBitsApp<'a> {
    pub opened_file: OpenedFile<'a>,     //path to opened file in string
    pub status_text: String,             //bottom bar text
    pub scroll: ScrollArea,              //scroll area
    pub active_tab: ActiveTab,           //active tab, either sarc file or text editor
    language: String, //language for highlighting, no option for yaml yet, toml is closest
    pub zstd: Arc<TotkZstd<'a>>, //zstd compressors and decompressors
    pub pack: Option<PackComparer<'a>>, //opened sarc file object, none if none opened
    pub root_node: Rc<TreeNode<String>>, //root_node pf the sarc directory tree
    pub internal_sarc_file: Option<Rc<TreeNode<String>>>, // node of sarc internal file opened in text editor
    pub scroll_resp: Option<egui::scroll_area::ScrollAreaOutput<()>>, //response from self.scroll, for controlling scrollbar position
    pub menu_bar: Arc<MenuBar>,                                       //menu bar at the top
    pub icons: Icons<'a>,                                             //cached icons for buttons
    pub settings: Settings,                                           //various settings
    pub code_editor: CodeEditor,
    pub file_reader: FileReader,
    pub file_renamer: FileRenamer,
    pub text_searcher: TextSearcher,
}
impl Default for TotkBitsApp<'_> {
    fn default() -> Self {
        let totk_config = Arc::new(TotkConfig::new());
        let settings = Settings::default();
        let mut file_reader = FileReader::default();
        file_reader.buf_size = 8192;
        file_reader.set_pos(0, file_reader.buf_size as i32);
        file_reader.reload = true;
        Self {
            opened_file: OpenedFile::default(),
            status_text: "Ready".to_owned(),
            scroll: ScrollArea::vertical(),
            active_tab: ActiveTab::TextBox,
            language: "toml".into(),
            zstd: Arc::new(TotkZstd::new(totk_config, settings.comp_level).unwrap()),
            pack: None,
            root_node: TreeNode::new("ROOT".to_string(), "/".to_string()),
            internal_sarc_file: None,
            scroll_resp: None,
            menu_bar: Arc::new(MenuBar::new(settings.styles.menubar.clone()).unwrap()),
            icons: Icons::new(&settings.icon_size.clone()),
            settings: settings,
            code_editor: CodeEditor::default(),
            file_reader: file_reader,
            file_renamer: FileRenamer::default(),
            text_searcher: TextSearcher::default(),
        }
    }
}

impl<'a> TotkBitsApp<'_> {
    pub fn new(path: Option<String>) -> Self {
        let totk_config = Arc::new(TotkConfig::new());
        let mut settings = Settings::default();
        let mut opened_file = OpenedFile::default();
        if let Some(p) = path {
            opened_file = OpenedFile::from_path(p, TotkFileType::Other);
            settings.is_file_loaded = false;
        }
        let mut file_reader = FileReader::default();
        file_reader.buf_size = 8192;
        file_reader.set_pos(0, file_reader.buf_size as i32);
        file_reader.reload = true;
        Self {
            opened_file: opened_file,
            status_text: "Ready".to_owned(),
            scroll: ScrollArea::vertical(),
            active_tab: ActiveTab::TextBox,
            language: "toml".into(),
            zstd: Arc::new(TotkZstd::new(totk_config, settings.comp_level).unwrap()),
            pack: None,
            root_node: TreeNode::new("ROOT".to_string(), "/".to_string()),
            internal_sarc_file: None,
            scroll_resp: None,
            menu_bar: Arc::new(MenuBar::new(settings.styles.menubar.clone()).unwrap()),
            icons: Icons::new(&settings.icon_size.clone()),
            settings: settings,
            code_editor: CodeEditor::default(),
            file_reader: file_reader,
            file_renamer: FileRenamer::default(),
            text_searcher: TextSearcher::default(),
        }
    }
}
impl eframe::App for TotkBitsApp<'_> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.settings.def_scale.is_none() {
            self.settings.def_scale = Some(ctx.pixels_per_point());
            self.settings.styles.scale.val = ctx.pixels_per_point();
        }
        ctx.set_pixels_per_point(self.settings.styles.scale.val);
        //let x = ctx.input(|i| i.raw_scroll_delta.y);
        //self.status_text = format!("{}", x);
        install_image_loaders(ctx);
        // Top panel (menu bar)
        self.menu_bar.clone().display(self, ctx);
        //GuiMenuBar::MenuBar::display(self, ctx);
        Gui::display_main_buttons(self, ctx);

        // Bottom panel (status bar)
        Gui::display_status_bar(self, ctx);

        if !&self.text_searcher.text.is_empty() {
            TreeNode::clean_up_tree(&self.root_node, &self.text_searcher.text);
        }
        if self
            .text_searcher
            .show(ctx, self.settings.styles.toolbar.clone())
        {
            self.settings.is_tree_loaded = false;
        }

        // Central panel (text area)
        egui::CentralPanel::default().show(ctx, |ui| {
            Gui::display_labels(self, ui);

            Gui::display_main(self, ui, ctx);
        });
    }
}

pub struct Gui {}

impl Gui {
    pub fn display_status_bar(app: &mut TotkBitsApp, ctx: &egui::Context) {
        TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(if app.settings.is_loading {
                    ""
                } else {
                    &app.status_text
                });
                //ui.label(if app.settings.is_dir_context_menu {""} else {&app.status_text});
            });
        });
    }

    pub fn display_main_buttons(app: &mut TotkBitsApp, ctx: &egui::Context) {
        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.set_style(app.settings.styles.toolbar.clone());
            ui.horizontal(|ui| {
                //if ui.add(Button::image(app.icons.new.clone())).on_hover_text("New").clicked(){}

                if ui
                    .add(Button::image(app.icons.open.clone()))
                    .on_hover_text("Open")
                    .clicked()
                {
                    let _ = ButtonOperations::open_file_button_click(app);
                }
                if ui
                    .add(Button::image(app.icons.save.clone()))
                    .on_hover_text("Save")
                    .clicked()
                {
                    ButtonOperations::save_click(app);
                }
                if ui
                    .add(Button::image(app.icons.save_as.clone()))
                    .on_hover_text("Save as")
                    .clicked()
                {
                    let _ = ButtonOperations::save_as_click(app);
                }

                if ui
                    .add(Button::image(app.icons.edit.clone()))
                    .on_hover_text("Edit")
                    .clicked()
                {
                    ButtonOperations::edit_click(app, ui);
                }
                if ui
                    .add(Button::image(app.icons.add_sarc.clone()))
                    .on_hover_text("Add file")
                    .clicked()
                {
                    //app.code_editor.syntax.keywords = BTreeSet::from(["Spiny"]);
                    app.code_editor.syntax = Syntax::yaml_find(BTreeSet::from(["Spiny"]));
                }
                if ui
                    .add(Button::image(app.icons.add_sarc.clone()))
                    .on_hover_text("Add file")
                    .clicked()
                {
                    // app.code_editor.syntax = Syntax::yaml(BTreeSet::from(["Bone"]));
                    app.code_editor.syntax = Syntax::yaml();
                }
                if ui
                    .add(Button::image(app.icons.extract.clone()))
                    .on_hover_text("Extract")
                    .clicked()
                {
                    let _ = ButtonOperations::extract_click(app);
                }
                ui.add_space(20.0);

                if ui
                    .add(Button::image(app.icons.zoomin.clone()))
                    .on_hover_text("Zoom in")
                    .clicked()
                {
                    app.settings.styles.scale.add(0.1);
                }
                if ui
                    .add(Button::image(app.icons.zoomout.clone()))
                    .on_hover_text("Zoom out")
                    .clicked()
                {
                    app.settings.styles.scale.add(-0.1);
                } //TODO: make them NOT overlap blue buttons

                ui.add_space(20.0);
                if app.settings.is_loading {
                    ui.add(egui::Spinner::new());
                }
                ui.add_space(20.0);

                if app.active_tab == ActiveTab::TextBox {
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let available_width = ui.available_width();
                        let button_size = 40.0; // Approximate width of each button, adjust based on your actual UI
                        let total_required_width = button_size * 5.0 + 20.0; // Adjust the multiplier based on number of buttons and spaces

                        // Right-aligned part
                        //ui.add(egui::Slider::new(&mut app.settings.styles.font_size, app.settings.styles.min_font_size..=app.settings.styles.max_font_size).suffix(""));
                        if available_width > total_required_width {
                            ui.add(
                                egui::DragValue::new(&mut app.settings.styles.font.val).speed(0.5),
                            );
                            app.settings.styles.font.update();
                            ui.label("Font size:");
                            ui.add_space(20.0);
                            if ui
                                .add(Button::image(app.icons.replace.clone()))
                                .on_hover_text("Replace")
                                .clicked()
                            {}
                            if ui
                                .add(Button::image(app.icons.lupa.clone()))
                                .on_hover_text("Find")
                                .clicked()
                            {}
                            if ui
                                .add(Button::image(app.icons.forward.clone()))
                                .on_hover_text("Find next")
                                .clicked()
                            {}
                            if ui
                                .add(Button::image(app.icons.back.clone()))
                                .on_hover_text("Find previous")
                                .clicked()
                            {}
                            ui.add_space(20.0); //TODO: add  zoom in out buttons
                        }
                    });
                }
            });
            ui.add_space(2.0);
            ui.set_style(egui::Style::default());
        });
    }

    pub fn display_main(app: &mut TotkBitsApp, ui: &mut egui::Ui, ctx: &egui::Context) {
        match app.active_tab {
            ActiveTab::TextBox => {
                app.settings.scroll_val =
                    app.file_reader
                        .check_for_changes(ctx, &ui, &app.scroll_resp);
                //app.file_reader.update_scroll_pos(&app.scroll_resp);
                let _ = app.file_reader.update();
                app.code_editor.line_offset = app.file_reader.lines_offset;
                ui.set_style(app.settings.styles.text_editor.clone());

                app.scroll_resp = app
                    .code_editor
                    .clone()
                    .id_source("code editor")
                    .with_rows(12)
                    .with_fontsize(app.settings.styles.font.val)
                    .vscroll(true)
                    //.with_theme(ColorTheme::GRUVBOX)
                    .with_syntax(app.code_editor.syntax.clone())
                    .with_numlines(false)
                    .show(ui, &mut app.file_reader.displayed_text, ctx.clone());
                FileOpener::open_byml_or_sarc(app, false);
                //});
                let r = app.scroll_resp.as_ref().unwrap();
                let _p = (r.state.offset.y * 100.0) / r.content_size.y;
                /*app.status_text = format!(
                    "  {:.1}-{:.1} {:.1}  {:.1}% {:?}",
                    r.state.offset.x,r.state.offset.y, r.content_size.y, _p, r.inner_rect
                );*/
                app.status_text = app.file_reader.get_status(format!(
                    "  {:.1} {:.1}  {:.1}% {:?}",
                    r.state.offset.y, r.content_size.y, _p, r.inner_rect
                ));
                //ctx.set_style(app.settings.styles.def_style.clone());
            }
            ActiveTab::DiretoryTree => {
                app.scroll_resp = Some(
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(ui.available_height())
                        .max_width(ui.available_width())
                        .show(ui, |ui| {
                            Gui::display_tree_background(app, ui);

                            FileOpener::open_byml_or_sarc(app, false);
                            if let Some(pack) = &mut app.pack {
                                //Comparer opened
                                if app.file_renamer.is_renamed
                                    || app.settings.do_i_compare_and_reload
                                {
                                    pack.compare_and_reload();
                                    app.file_renamer.is_renamed = false;
                                    app.settings.do_i_compare_and_reload = false;
                                }
                                if let Some(opened) = &mut pack.opened {
                                    let internal_file = &app.internal_sarc_file;

                                    app.file_renamer
                                        .show(opened, internal_file.as_ref(), ctx, ui);
                                    //Sarc is opened
                                    if !app.settings.is_tree_loaded || app.file_renamer.is_renamed {
                                        app.root_node =
                                            TreeNode::new("ROOT".to_string(), "/".to_string());
                                        Tree::update_from_sarc_paths(&app.root_node, opened);
                                        app.settings.is_tree_loaded = true;
                                        println!("Reloading tree");
                                        //Tree::TreeNode::print(&app.root_node, 1);
                                    }
                                }

                                let children: Vec<_> =
                                    app.root_node.children.borrow().iter().cloned().collect();
                                for child in children {
                                    SarcLabel::display_tree_in_egui(app, &child, ui, &ctx);
                                }

                                //ctx.set_style(app.settings.styles.text_editor.clone());
                            }
                        }),
                );
            }
            ActiveTab::Settings => {
                app.scroll_resp = Some(
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(ui.available_height())
                        .max_width(ui.available_width())
                        .show(ui, |ui| {
                            Gui::display_tree_background(app, ui);
                            FileOpener::open_byml_or_sarc(app, false);
                            /*if let Some(tag) = &mut app.opened_file.tag {
                                for (key, item) in tag.actor_tag_data.iter() {
                                    CollapsingHeader::new(key)
                                        .default_open(false)
                                        .show(ui, |ui| {
                                            ui.text_edit_multiline(&mut format!("{:?}", item));
                                        });
                                }
                            }*/
                        }),
                );
            }
        }
    }

    pub fn display_tree_background(app: &mut TotkBitsApp, ui: &mut egui::Ui) {
        let mut height = 0.0;
        if let Some(resp) = &app.scroll_resp {
            height = resp.inner_rect.height().max(resp.content_size.y);
        }
        let painter = ui.painter();
        let tree_bg =
            egui::Rect::from_min_size(ui.min_rect().min, egui::vec2(ui.available_width(), height));
        let shape = egui::Shape::rect_filled(tree_bg, 0.0, app.settings.tree_bg_color);
        painter.add(shape);
    }

    //pub fn scroll_test(app: &mut TotkBitsApp,ui: &egui::Ui, scroll_offset: f32) {
    pub fn scroll_top_test(resp: &Option<ScrollAreaOutput<()>>, ui: &egui::Ui) {
        Gui::scroll_test(resp, ui, 99999999.0)
    }
    pub fn scroll_test(resp: &Option<ScrollAreaOutput<()>>, ui: &egui::Ui, scroll_offset: f32) {
        if let Some(r) = resp {
            //let r = app.scroll_resp.as_ref().unwrap();
            let visible_rect = r.inner_rect.clone();
            // Create a new rectangle adjusted by the offset
            let target_rect = egui::Rect::from_min_size(
                visible_rect.min + egui::vec2(0.0, scroll_offset + 3.0),
                visible_rect.size(),
            );
            // Scroll to the new rectangle
            ui.scroll_to_rect(target_rect, Some(egui::Align::Center));
            println!("Scrolled {}", scroll_offset);
        }
    }

    pub fn display_labels(app: &mut TotkBitsApp, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .add(SelectableLabel::new(
                    app.active_tab == ActiveTab::DiretoryTree,
                    "SARC",
                ))
                .clicked()
            {
                app.active_tab = ActiveTab::DiretoryTree;
            }
            if ui
                .add(SelectableLabel::new(
                    app.active_tab == ActiveTab::TextBox,
                    "YAML",
                ))
                .clicked()
            {
                app.active_tab = ActiveTab::TextBox;
            }
            if ui
                .add(SelectableLabel::new(
                    app.active_tab == ActiveTab::Settings,
                    "Settings",
                ))
                .clicked()
            {
                app.active_tab = ActiveTab::Settings;
            }
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                InfoLabel::display_filename_endian(app, ui);
            })
        });
        ui.add_space(10.0);
    }

}
//TODO: saving byml file,

pub fn run() {
    let mut options = eframe::NativeOptions::default();
    let argv1 = Settings::get_arg1();
    //options::viewport::initial_window_size(Some(egui::vec2(1000.0, 1000.0)));
    options.viewport.inner_size = Some(egui::vec2(700.0, 700.0));
    eframe::run_native(
        "Totkbits",
        options,
        Box::new(|_cc| Box::new(TotkBitsApp::new(argv1))),
    )
    .unwrap();
}

struct DraggableRect {
    // Initial position of the rectangle
    position: Vec2,
    // Offset from the initial position (how far it has been dragged)
    drag_offset: f32,
    // Whether the rectangle is currently being dragged
    is_dragging: bool,
}

impl DraggableRect {
    fn new(x: f32, y: f32) -> Self {
        Self {
            position: Vec2::new(x, y),
            drag_offset: 0.0,
            is_dragging: false,
        }
    }

    fn show(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // The area where the rectangle will be drawn and interacted with
            let (response, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::drag());

            // Current mouse position
            let mouse_pos = response.hover_pos().unwrap_or_default();
            // Check if the rectangle is being dragged
            if response.dragged() && self.is_dragging {
                // Update the rectangle's vertical position based on the drag
                let delta = mouse_pos.y - self.position.y;
                self.drag_offset += delta;
                self.position.y = mouse_pos.y;
            }

            // Check for mouse down to start dragging
            if response.clicked() {
                self.is_dragging = true;
            }

            // Check for mouse release to stop dragging
            if response.drag_released() {
                self.is_dragging = false;
            }

            // Draw the rectangle
            let rect = Rect::from_min_size(
                pos2(self.position.x, self.position.y + self.drag_offset),
                Vec2::new(100.0, 50.0), // Width and height of the rectangle
            );
            painter.rect_filled(rect, 0.0, egui::Color32::GREEN);

            // Display drag offset
            ui.label(format!("Drag offset: {:.2}", self.drag_offset));
        });
    }
}
