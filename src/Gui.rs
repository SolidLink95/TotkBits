use crate::misc::{self, open_file_dialog, save_file_dialog};
use crate::BymlFile::byml_file;
use crate::CodeEditorFormatter::Editor;
use crate::GuiMenuBar::{self, MenuBar};
use crate::Pack::PackFile;
use crate::SarcFileLabel::SarcLabel;
use crate::TotkPath::TotkPath;
use crate::Tree::{self, tree_node};
use crate::Zstd::{is_byml, totk_zstd, ZsDic};
//use crate::SarcFileLabel::ScrollAreaPub;
use eframe::egui::{
    self, Color32, ScrollArea, SelectableLabel, TextStyle, TextureId, TopBottomPanel,
};
use egui::text::LayoutJob;
use egui::{vec2, Align, CollapsingHeader, Context, Label, Pos2, Rect, Style, Vec2};
use egui_extras::install_image_loaders;
use native_dialog::{MessageDialog, MessageType};
use rfd::FileDialog;
use roead::byml::Byml;
use roead::sarc::File;
use std::io::Read;
use std::os::raw;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::{any, fs, io};

pub struct Flags{
    pub is_file_loaded: bool, //flag for loading file, prevents the program from loading file from disk in every frame
    pub is_tree_loaded: bool
}

impl Flags {
    pub fn new(is_file_loaded: bool, is_tree_loaded: bool) -> Self {
        Self {is_file_loaded, is_tree_loaded}
    }
}

#[derive(PartialEq)]
pub enum ActiveTab {
    DiretoryTree,
    TextBox,
}

pub struct TotkBitsApp<'a> {
    opened_file: String, //path to opened file in string
    pub text: String, //content of the text editor
    pub status_text: String, //bottom bar text
    scroll: ScrollArea, //scroll area
    pub active_tab: ActiveTab, //active tab, either sarc file or text editor
    language: String, //language for highlighting, no option for yaml yet, toml is closest
    pub zstd: Arc<totk_zstd<'a>>, //zstd compressors and decompressors
    pub pack: Option<PackFile<'a>>, //opened sarc file object, none if none opened
    pub byml: Option<byml_file<'a>>, //opened byml file, none if none opened
    pub root_node: Rc<tree_node<String>>, //root_node pf the sarc directory tree
    pub internal_sarc_file: Option<Rc<tree_node<String>>>, // node of sarc internal file opened in text editor
    pub scroll_resp: Option<egui::scroll_area::ScrollAreaOutput<()>>, //response from self.scroll, for controlling scrollbar position
    pub menu_bar: Arc<MenuBar>, //menu bar at the top
    pub flags: Flags
}
impl Default for TotkBitsApp<'_> {
    fn default() -> Self {
        let totk_path = Arc::new(TotkPath::new());
        let def_style = Style::default();
        Self {
            opened_file: String::new(),
            text: misc::get_example_yaml(),
            status_text: "Ready".to_owned(),
            scroll: ScrollArea::vertical(),
            active_tab: ActiveTab::TextBox,
            language: "toml".into(),
            zstd: Arc::new(totk_zstd::new(totk_path, 16).unwrap()),
            pack: None,
            byml: None,
            root_node: tree_node::new("ROOT".to_string(), "/".to_string()),
            internal_sarc_file: None,
            scroll_resp: None,
            menu_bar: Arc::new(MenuBar::new(&def_style).unwrap()),
            flags: Flags::new(true, true)
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
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    let _ = Gui::open_file_button_click(app);
                }
                if ui.button("Save").clicked() {
                    if app.opened_file.len() > 0 {
                        println!("Saving file to {}", app.opened_file);
                    }
                }
                if ui.button("TEST").clicked() {
                    //let mut c = Editor::new(app, &app.text, 3500);
                    let x = app.scroll_resp.as_ref().unwrap().content_size.y * 0.1;
                    //Gui::scroll_the_boy(ui, x);
                    app.text = misc::get_other_yaml();
                    
                }
            });
        });
    }

    fn scroll_the_boy(ui: &mut egui::Ui, val: f32)  {
        let target_rect = Rect {
            min: Pos2 {
                x: 0.0,
                y: val,
            },
            max: Pos2 {
                x: 0.0,
                y: val,
            },
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

        match app.active_tab {
            ActiveTab::TextBox => {
                
                //scrollbar
                app.scroll_resp = Some(app.scroll.clone().show(ui, |ui| {
                    //let r =scroll_area.show(ui, |ui| {
                    ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::multiline(&mut app.text)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_rows(10)
                            .lock_focus(true)
                            .desired_width(f32::INFINITY)
                            .layouter(&mut layouter),
                    );
                    Gui::open_byml_or_sarc(app, ui);
                    //TODO: get scrollbar position and render only that part of text
                    //println!("{:?}", app.scroll.clone().show_viewport(ui, add_contents))
                }));
                let r = app.scroll_resp.as_ref().unwrap();
                let p = ((r.state.offset.y * 100.0) / r.content_size.y);
                app.status_text = format!(
                    "Scroll: {:?} [{:?}%] size {:?}, cur. height: {:?}, size {:?} bytes",
                    r.state.offset.y as i32, p , r.content_size, r.inner_rect.height(), app.text.chars().filter(|&c| c == '\n').count()
                );
                //println!("{:?} \n\n\n", r.state);
            }
            ActiveTab::DiretoryTree => {
                //println!("{:?}", egui::ScrollArea::vertical().off);
                //app.scroll.scroll_offset(offset)
                let response = app
                    .scroll
                    .clone()
                    .auto_shrink([false, false])
                    .max_height(ui.available_height())
                    .max_width(ui.available_width())
                    .show(ui, |ui| {
                        Gui::open_byml_or_sarc(app, ui);
                        if !app.pack.is_none() {
                            if !app.flags.is_tree_loaded {
                                Tree::update_from_sarc_paths(
                                    &app.root_node,
                                    &app.pack.as_mut().expect("Error passing pack file"),
                                );
                                app.flags.is_tree_loaded = true;
                            }
                            let children: Vec<_> =
                                app.root_node.children.borrow().iter().cloned().collect();
                            for child in children {
                                SarcLabel::display_tree_in_egui(app, &child, ui);
                            }
                        }
                    });
            }
        }
    }

    fn open_byml_or_sarc(app: &mut TotkBitsApp, ui: &mut egui::Ui) {
        if app.flags.is_file_loaded {
            return; //stops the app from infinite file loading from disk
        }
        println!("Is {} a sarc?", app.opened_file.clone());
        match PackFile::new(app.opened_file.clone(), app.zstd.clone()) {
            Ok(pack) => {
                app.pack = Some(pack);
                app.flags.is_file_loaded = true;
                println!("Sarc  opened!");
                app.active_tab = ActiveTab::DiretoryTree;
                app.flags.is_tree_loaded = false;
                return;
            }
            Err(_) => {}
        }
        println!("Is {} a byml?", app.opened_file.clone());
        let mut res_byml: Result<byml_file<'_>, io::Error> =
            byml_file::new(app.opened_file.clone(), app.zstd.clone());
        match res_byml {
            Ok(ref b) => {
                app.text = Byml::to_text(&b.pio);
                app.byml = Some(res_byml.unwrap());
                app.active_tab = ActiveTab::TextBox;
                println!("Byml  opened!");
                app.flags.is_file_loaded = true;
                return;
            }

            Err(_) => {}
        };
        app.flags.is_file_loaded = true;
        app.flags.is_tree_loaded = true;
        app.status_text = format!("Failed to open: {}", app.opened_file.clone());
    }

    pub fn display_labels(app: &mut TotkBitsApp, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .add(SelectableLabel::new(
                    app.active_tab == ActiveTab::DiretoryTree,
                    "Sarc files",
                ))
                .clicked()
            {
                app.active_tab = ActiveTab::DiretoryTree;
            }
            if ui
                .add(SelectableLabel::new(
                    app.active_tab == ActiveTab::TextBox,
                    "Byml editor",
                ))
                .clicked()
            {
                app.active_tab = ActiveTab::TextBox;
            }
        });
    }

    fn open_file_button_click(app: &mut TotkBitsApp) -> io::Result<()> {
        // Logic for opening a file
        let file_name = open_file_dialog();
        if !file_name.is_empty() {
            println!("Attempting to read {} file", &file_name);
            app.opened_file = file_name.clone();
            let mut f_handle = fs::File::open(&file_name)?;
            let mut buffer: Vec<u8> = Vec::new(); //String::new();
            match f_handle.read_to_end(&mut buffer) {
                Ok(_) => {
                    app.status_text =
                        format!("Opened file: {}", &app.opened_file);
                    app.flags.is_file_loaded = false;
                    return Ok(());
                }
                Err(err) => {
                    app.status_text = format!("Error reading file: {}", file_name);
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        app.status_text.clone(),
                    ));
                }
            }
            //self.text = buffer;
        } else {
            app.status_text = "No file selected".to_owned();
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "No file selected",
            ));
        }
    }


}

//TODO: saving byml file,
fn save_as(app: &mut TotkBitsApp) {
    match app.active_tab {
        ActiveTab::DiretoryTree => {
            if app.pack.is_none() {
                return; //no sarc opened, aborting
            }
            let file: Option<PathBuf> = FileDialog::new()
                .set_title("Save sarc file as")
                .set_file_name(app.opened_file.clone())
                .save_file();
            if file.is_none() {
                return; //saving aborted
            }
            let dest_file: String = file
                .unwrap()
                .into_os_string()
                .into_string()
                .map_err(|os_str| format!("Path contains invalid UTF-8: {:?}", os_str))
                .unwrap();
            let res = app.pack.as_mut().unwrap().save(dest_file.clone());
            match res {
                Ok(_) => {
                    app.status_text = format!("Saved: {}", dest_file);
                }
                Err(err) => {
                    app.status_text = format!("Error in save: {}", dest_file);
                }
            }
        }

        ActiveTab::TextBox => {
            if app.pack.is_none() { //just byml
            }
        }
    }
}

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
