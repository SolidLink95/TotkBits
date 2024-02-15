use crate::misc;
use crate::BinTextFile::BymlFile;
use crate::ButtonOperations::{
    edit_click, extract_click, open_byml_or_sarc, open_file_button_click, save_as_click,
    save_click, save_file_dialog,
};
use crate::GuiMenuBar::MenuBar;
use crate::Pack::PackFile;
use crate::SarcFileLabel::SarcLabel;
use crate::Settings::{Icons, Pathlib, Settings};
use crate::TotkPath::TotkPath;
use crate::Tree::{self, tree_node};
use crate::Zstd::{FileType, TotkZstd};
//use crate::SarcFileLabel::ScrollAreaPub;
use crate::GuiScroll::EfficientScroll;
use eframe::egui::{self, ScrollArea, SelectableLabel, TopBottomPanel};
use egui::text::LayoutJob;
use egui::{Align, Button, Label, Layout, Pos2, Rect, Shape};
use egui_extras::install_image_loaders;
use roead::byml::Byml;
use std::cell::RefCell;
use std::io::Read;
use std::rc::Rc;
use std::sync::Arc;
use std::{fs, io};

#[derive(PartialEq)]
pub enum ActiveTab {
    DiretoryTree,
    TextBox,
}

pub struct TotkBitsApp<'a> {
    pub opened_file: Pathlib,             //path to opened file in string
    pub opened_file_type: FileType,       //path to opened file in string
    pub text: String,                     //content of the text editor
    pub displayed_text: String,           //content of the text editor
    pub status_text: String,              //bottom bar text
    pub scroll: ScrollArea,               //scroll area
    pub scroll_updater: EfficientScroll,  //scroll area
    pub active_tab: ActiveTab,            //active tab, either sarc file or text editor
    language: String, //language for highlighting, no option for yaml yet, toml is closest
    pub zstd: Arc<TotkZstd<'a>>, //zstd compressors and decompressors
    pub pack: Option<PackFile<'a>>, //opened sarc file object, none if none opened
    pub byml: Option<BymlFile<'a>>, //opened byml file, none if none opened
    pub root_node: Rc<tree_node<String>>, //root_node pf the sarc directory tree
    pub internal_sarc_file: Option<Rc<tree_node<String>>>, // node of sarc internal file opened in text editor
    pub scroll_resp: Option<egui::scroll_area::ScrollAreaOutput<()>>, //response from self.scroll, for controlling scrollbar position
    pub menu_bar: Arc<MenuBar>,                                       //menu bar at the top
    pub icons: Icons<'a>,                                             //cached icons for buttons
    pub settings: Settings,                                           //various settings
}
impl Default for TotkBitsApp<'_> {
    fn default() -> Self {
        let totk_path = Arc::new(TotkPath::new());
        let settings = Settings::default();
        Self {
            opened_file: Pathlib::new("".to_string()),
            opened_file_type: FileType::None, //used only for TextBox active tab
            text: misc::get_example_yaml(),
            displayed_text: misc::get_example_yaml(),
            status_text: "Ready".to_owned(),
            scroll: ScrollArea::vertical(),
            scroll_updater: EfficientScroll::new(),
            active_tab: ActiveTab::TextBox,
            language: "toml".into(),
            zstd: Arc::new(TotkZstd::new(totk_path, settings.comp_level).unwrap()),
            pack: None,
            byml: None,
            root_node: tree_node::new("ROOT".to_string(), "/".to_string()),
            internal_sarc_file: None,
            scroll_resp: None,
            menu_bar: Arc::new(MenuBar::new(settings.styles.menubar.clone()).unwrap()),
            icons: Icons::new(&settings.icon_size.clone()),
            settings: settings,
        }
    }
}

impl eframe::App for TotkBitsApp<'_> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

            Gui::display_main(self, ui);
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
                    let _ = open_file_button_click(app);
                }
                if ui
                    .add(Button::image(app.icons.save.clone()))
                    .on_hover_text("Save")
                    .clicked()
                {
                    save_click(app);
                }
                if ui
                    .add(Button::image(app.icons.save_as.clone()))
                    .on_hover_text("Save as")
                    .clicked()
                {
                    let _ = save_as_click(app);
                }

                if ui
                    .add(Button::image(app.icons.edit.clone()))
                    .on_hover_text("Edit")
                    .clicked()
                {
                    edit_click(app, ui);
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
                    extract_click(app);
                }
            });
            ui.add_space(2.0);
            ui.set_style(egui::Style::default());
        });
    }

    fn scroll_the_boy(ui: &mut egui::Ui, val: f32) {
        let target_rect = Rect {
            min: Pos2 { x: 0.0, y: val },
            max: Pos2 { x: 0.0, y: val },
        };
        ui.scroll_to_rect(target_rect, Some(Align::Max));
    }

    pub fn display_main(app: &mut TotkBitsApp, ui: &mut egui::Ui) {
        let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx());
        let language = app.language.clone();
        let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
            let mut layout_job: LayoutJob =
                egui_extras::syntax_highlighting::highlight(ui.ctx(), &theme, string, &language);
            layout_job.wrap.max_width = wrap_width;
            ui.fonts(|f| f.layout_job(layout_job))
        };
        let _font_id = egui::FontId::monospace(12.0);
        app.settings.lines_count = app.text.chars().filter(|&c| c == '\n').count() + 1;

        match app.active_tab {
            ActiveTab::TextBox => {
                //scrollbar
                app.scroll_resp = Some(app.scroll.clone().show(ui, |ui| {
                    ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::multiline(&mut app.text)
                            .font(app.settings.editor_font.clone()) // Use monospace font for proper alignment
                            .code_editor()
                            .desired_rows(10)
                            .lock_focus(true)
                            .desired_width(f32::INFINITY)
                            .layouter(&mut layouter),
                    );
                    open_byml_or_sarc(app, ui);
                    //TODO: get scrollbar position and render only that part of text
                    //println!("{:?}", app.scroll.clone().show_viewport(ui, add_contents))
                }));
                let r = app.scroll_resp.as_ref().unwrap();
                let _p = (r.state.offset.y * 100.0) / r.content_size.y;
                /*app.status_text = format!(
                    "Scroll: {:?} [{:?}%] size {:?}, cur. height: {:?}, {:?} lines",
                    r.state.offset.y as i32,
                    p,
                    r.content_size,
                    r.inner_rect.height(),
                    app.settings.lines_count //app.text.chars().filter(|&c| c == '\n').count()
                );*/
                //println!("{:?} \n\n\n", r.state);
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
                            if !app.pack.is_none() {
                                if !app.settings.is_tree_loaded {
                                    Tree::update_from_sarc_paths(
                                        &app.root_node,
                                        &app.pack.as_mut().expect("Error passing pack file"),
                                    );
                                    app.settings.is_tree_loaded = true;
                                }
                                let children: Vec<_> =
                                    app.root_node.children.borrow().iter().cloned().collect();
                                for child in children {
                                    SarcLabel::display_tree_in_egui(app, &child, ui);
                                }
                            }
                        }),
                );
            }
        }
    }

    pub fn display_lines_numbers(app: &mut TotkBitsApp, ui: &mut egui::Ui) {
        app.settings.lines_count = app.text.chars().filter(|&c| c == '\n').count() + 1;
        let max_width = format!("{}", app.settings.lines_count).len();
        let mut lines_numbers = String::new();
        for i in 1..app.settings.lines_count {
            let l = format!("{}", i);
            let spacing = " ".repeat(max_width - l.len());
            lines_numbers.push_str(&format!("{}{}\n", spacing, l));
        }
        let lines_count = app.text.chars().filter(|&c| c == '\n').count() + 1;
        let lines_numbers: String = (1..=lines_count).map(|i| format!("{}\n", i)).collect();
        let label = Label::new(lines_numbers);
        ui.vertical(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.add(label);
            });
        });
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
                    let endian_label = match pack.endian {
                        roead::Endian::Big => "BE",
                        roead::Endian::Little => "LE",
                    };
                    ui.label(endian_label);
                    ui.label(&pack.path.name);
                }
            }
            ActiveTab::TextBox => {
                let mut label_path: Option<String> = None;
                let mut label_endian = String::new();
                if let Some(internal_file) = &app.internal_sarc_file {
                    label_path = Some(internal_file.path.name.clone());
                }
                match app.opened_file_type {
                    FileType::Msbt => {
                        label_endian = "LE".to_string();
                    }
                    FileType::Byml => {
                        if let Some(byml) = &app.byml {
                            match byml.endian {
                                Some(endian) => {
                                    label_endian = match endian {
                                        roead::Endian::Big => "BE".to_string(),
                                        roead::Endian::Little => "LE".to_string(),
                                    }
                                },
                                None => {
                                    label_endian = "LE".to_string();
                                }
                            }
                        }
                    }
                    FileType::None => {
                        label_endian = String::new();
                    },
                    FileType::Other => {
                        label_endian = String::new();
                    },
                    _ => {
                        label_endian = "LE".to_string();
                    }
                }
                if label_endian.len() == 0 {
                    label_endian = "LE".to_string();
                }
                if let Some(l_path) = &label_path {
                    //ui.label(format!("{:?}", app.opened_file_type));
                    ui.label(label_endian);
                    ui.label(l_path);
                }
            }
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
