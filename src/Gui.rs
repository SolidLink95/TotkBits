use crate::file_format::BinTextFile::OpenedFile;
use crate::file_format::Pack::PackComparer;
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
use egui::{Align, Button, Label, Layout};
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
                if app.settings.is_loading {
                    ui.add(egui::Spinner::new());
                }
                ui.add_space(20.0);

                if app.active_tab == ActiveTab::TextBox {
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        // Right-aligned part
                        //ui.add(egui::Slider::new(&mut app.settings.styles.font_size, app.settings.styles.min_font_size..=app.settings.styles.max_font_size).suffix(""));
                        ui.add(egui::DragValue::new(&mut app.settings.styles.font.val).speed(0.5));
                        app.settings.styles.font.update();
                        ui.label("Font size:");
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
                                        Tree::update_from_sarc_paths(&app.root_node, opened);
                                        app.settings.is_tree_loaded = true;
                                        println!("Reloading tree");
                                        //Tree::TreeNode::print(&app.root_node, 1);
                                    }
                                }
                                let tmp = app.text_searcher.show(ctx, ui); //TODO cleanup
                                if !&app.text_searcher.text.is_empty() {
                                    TreeNode::clean_up_tree(
                                        &app.root_node,
                                        &app.text_searcher.text,
                                    );
                                }
                                if tmp {
                                    app.settings.is_tree_loaded = false;
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
                Gui::display_filename_endian(app, ui);
            })
        });
        ui.add_space(10.0);
    }

    fn display_filename_endian(app: &mut TotkBitsApp, ui: &mut egui::Ui) {
        match app.active_tab {
            ActiveTab::DiretoryTree => {
                if let Some(pack) = &app.pack {
                    if let Some(opened) = &pack.opened {
                        let label_endian = match opened.endian {
                            roead::Endian::Big => "BE",
                            roead::Endian::Little => "LE",
                        };
                        display_infolabels(ui, label_endian, Some(&opened.path.name));
                    }
                }
            }
            ActiveTab::TextBox => {
                let mut label_path: Option<&str> = None;
                let label_endian = &app.opened_file.get_endian_label();
                if let Some(internal_file) = &app.internal_sarc_file {
                    label_path = Some(&internal_file.path.name);
                } else {
                    label_path = Some(&app.opened_file.path.name);
                }

                display_infolabels(ui, label_endian, label_path);
            }
            ActiveTab::Settings => {
                let label_path: Option<&str> = Some(&app.opened_file.path.name);
                let label_endian = if label_path.is_some() { "LE" } else { "" };
                display_infolabels(ui, label_endian, label_path);
            }
        }
    }
}

fn calc_labels_width(label: &str) -> f32 {
    (label.len() + 3) as f32 * 6.0 //very rough calculation based on default Style
}

fn are_infolables_shown(ui: &mut egui::Ui, label: &str) -> bool {
    let perc = calc_labels_width(label) / ui.available_width();
    return perc < 0.79;
}
pub fn display_infolabels(ui: &mut egui::Ui, endian: &str, path: Option<&str>) {
    if let Some(path) = &path {
        if are_infolables_shown(ui, path) {
            //ui.add(Label::new(endian));
            ui.add(Label::new(format!("{} {}", path, endian)));
            //ui.add(Label::new(path.to_string()));
        }
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
