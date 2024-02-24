use crate::misc;
use crate::BinTextFile::{BymlFile, OpenedFile, TagProduct};
use crate::ButtonOperations::{open_byml_or_sarc, save_file_dialog, ButtonOperations};
use crate::GuiMenuBar::MenuBar;
use crate::GuiScroll::EfficientScroll;
use crate::Pack::{PackComparer, PackFile};
use crate::SarcFileLabel::SarcLabel;
use crate::Settings::{FileReader, Icons, Pathlib, Settings};
use crate::TotkConfig::TotkConfig;
use crate::Tree::{self, TreeNode};
use crate::Zstd::{FileType, TotkZstd};
use eframe::egui::{self, ScrollArea, SelectableLabel, TopBottomPanel};
use egui::mutex::Mutex;
use egui::text::LayoutJob;
use egui::{
    Align, Button, CollapsingHeader, Context, FontId, InputState, Key, Label, Layout, Pos2, Rect, Response, Shape, TextEdit
};
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use egui_extras::install_image_loaders;
use roead::sarc::File;
use std::rc::Rc;
use std::sync::Arc;
use std::{fs, io};

#[derive(PartialEq)]
pub enum ActiveTab {
    DiretoryTree,
    TextBox,
    Advanced,
}

pub struct TotkBitsApp<'a> {
    pub opened_file: OpenedFile<'a>,     //path to opened file in string
    pub text: String,                    //content of the text editor
    pub status_text: String,             //bottom bar text
    pub scroll: ScrollArea,              //scroll area
   // pub scroll_updater: EfficientScroll, //scroll area
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
            text: misc::get_example_yaml(),
            status_text: "Ready".to_owned(),
            scroll: ScrollArea::vertical(),
            //scroll_updater: EfficientScroll::new(),
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
            file_reader: file_reader
        }
    }
}

impl eframe::App for TotkBitsApp<'_> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                ui.label(&app.status_text);
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
                {}
                if ui
                    .add(Button::image(app.icons.extract.clone()))
                    .on_hover_text("Extract")
                    .clicked()
                {
                    ButtonOperations::extract_click(app);
                }
            });
            ui.add_space(2.0);
            ui.set_style(egui::Style::default());
        });
    }

    pub fn display_main(app: &mut TotkBitsApp, ui: &mut egui::Ui, ctx: &egui::Context) {
        /*let theme: egui_extras::syntax_highlighting::CodeTheme =
            egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx());
        let language = app.language.clone();
        let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
            let mut layout_job: LayoutJob =
                egui_extras::syntax_highlighting::highlight(ui.ctx(), &theme, string, &language);
            layout_job.wrap.max_width = wrap_width;
            ui.fonts(|f| f.layout_job(layout_job))
        };*/

        match app.active_tab {
            ActiveTab::TextBox => {
                //scrollbar
                //app.scroll.clone().show(ui, |ui| {
                    ui.set_style(app.settings.styles.text_editor.clone());
                    //app.text_loader.ui(ui,ctx);
                    //ui.add(TextEdit::multiline(&mut app.text.clone()).desired_width(f32::INFINITY).code_editor());
                    app.file_reader.check_for_changes(ctx, &ui, &app.scroll_resp);
                    //app.file_reader.update_scroll_pos(&app.scroll_resp);
                    if let Err(err) = &app.file_reader.update() {
                        println!("Error parsing {}", &app.opened_file.path.full_path);
                    }
                    app.scroll_resp = app.code_editor
                        .clone()
                        .id_source("code editor")
                        .with_rows(12)
                        .with_fontsize(12.0)
                        .vscroll(true)
                        //.with_theme(ColorTheme::GRUVBOX)
                        //.with_syntax(app.settings.syntax.clone())
                        .with_numlines(false)
                        //.show(ui, &mut app.text, ctx.clone());
                        .show(ui, &mut app.file_reader.displayed_text, ctx.clone());
                    open_byml_or_sarc(app, ui);
                //});
                let r = app.scroll_resp.as_ref().unwrap();
                let _p = (r.state.offset.y * 100.0) / r.content_size.y;
                //app.status_text = format!("  {:.1} {:.1}  {:.1}%", r.state.offset.y, r.content_size.y, _p);
                app.status_text = app.file_reader.get_status(format!("  {:.1} {:.1}  {:.1}% {:?}", r.state.offset.y, r.content_size.y, _p, r.inner_rect));
            }
            ActiveTab::DiretoryTree => {
                app.scroll_resp = Some(
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(ui.available_height())
                        .max_width(ui.available_width())
                        .show(ui, |ui| {
                            Gui::display_tree_background(app, ui);
                            open_byml_or_sarc(app, ui);
                            if let Some(pack) = &app.pack {
                                //Comparer opened
                                if let Some(opened) = &pack.opened {
                                    //Sarc is opened
                                    if !app.settings.is_tree_loaded {
                                        Tree::update_from_sarc_paths(&app.root_node, opened);
                                        app.settings.is_tree_loaded = true;
                                        //Tree::TreeNode::print(&app.root_node, 1);
                                    }
                                }
                                let children: Vec<_> =
                                    app.root_node.children.borrow().iter().cloned().collect();
                                for child in children {
                                    SarcLabel::display_tree_in_egui(app, &child, ui, &ctx);
                                }
                            }
                            if !app.pack.is_none() {}
                        }),
                );
            }
            ActiveTab::Advanced => {
                app.scroll_resp = Some(
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(ui.available_height())
                        .max_width(ui.available_width())
                        .show(ui, |ui| {
                            Gui::display_tree_background(app, ui);
                            open_byml_or_sarc(app, ui);
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
                    app.active_tab == ActiveTab::Advanced,
                    "Advanced",
                ))
                .clicked()
            {
                app.active_tab = ActiveTab::Advanced;
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
                        display_infolabels(
                            ui,
                            label_endian.to_string(),
                            Some(opened.path.name.clone()),
                        );
                    }
                }
            }
            ActiveTab::TextBox => {
                let mut label_path: Option<String> = None;
                let label_endian = app.opened_file.get_endian_label();
                if let Some(internal_file) = &app.internal_sarc_file {
                    label_path = Some(internal_file.path.name.clone());
                } else {
                    label_path = Some(app.opened_file.path.name.clone());
                }

                display_infolabels(ui, label_endian, label_path);
            }
            ActiveTab::Advanced => {
                let label_path: Option<String> = Some(app.opened_file.path.name.clone());
                let label_endian = if label_path.is_some() { "LE" } else { "" };
                display_infolabels(ui, label_endian.to_string(), label_path);
            }
        }
    }
}

fn calc_labels_width(label: &str) -> f32 {
    (label.len() + 3) as f32 * 6.0 //very rough calculation based on default Style
}

fn are_infolables_shown(ui: &mut egui::Ui, label: &str) -> bool {
    let perc = calc_labels_width(label) / ui.available_width();
    if perc < 0.79 {
        return true;
    }
    return false;
}
pub fn display_infolabels(ui: &mut egui::Ui, endian: String, path: Option<String>) {
    if let Some(path) = &path {
        if are_infolables_shown(ui, path) {
            ui.add(Label::new(endian));
            ui.add(Label::new(path));
        }
    }
}

//TODO: saving byml file,

pub fn run() {
    let mut options = eframe::NativeOptions::default();
    //options::viewport::initial_window_size(Some(egui::vec2(1000.0, 1000.0)));
    options.viewport.inner_size = Some(egui::vec2(700.0, 700.0));
    eframe::run_native(
        "Totkbits",
        options,
        Box::new(|_cc| Box::<TotkBitsApp>::default()),
    )
    .unwrap();
}

pub struct LargeTextLoader {
    pub text: Arc<Mutex<String>>,
    pub code_editor: CodeEditor,
    // Other fields to manage loaded text, cache, etc.
}

impl LargeTextLoader {
    fn new(file_path: &str) -> Self {
        let text = Arc::new(Mutex::new(String::new()));
        let text_clone = text.clone();
        let ff = file_path.clone().to_string();
        // Simulate background loading
        std::thread::spawn(move || {
            if let Ok(mut file_text) = std::fs::read_to_string(ff.clone()) {
                // Perform chunked loading here...
                // For now, we'll just load the entire file as an example.
                *text_clone.lock() = file_text;
            }
        });

        LargeTextLoader {
            text: text,
            code_editor: CodeEditor::default(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ScrollArea::vertical().show(ui, |ui| {
            // Here we would only render the visible portion of the text.
            // For simplicity, we'll show a TextEdit with all the text.
            let text_lock = self.text.lock();
            if ui.add(
                TextEdit::multiline(&mut text_lock.clone())
                    .desired_width(f32::INFINITY)
                    //.code_editor(),
            ).changed() {
                
                //self.text = Arc::new(Mutex::new(text_lock.clone().to_string())) ;
            };
        });
    }
}
