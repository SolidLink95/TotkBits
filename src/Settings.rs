use std::{path::Path, rc::Rc, sync::Arc};

use egui::{
    epaint::Shadow, include_image, style::HandleShape, Color32, Margin, Pos2, Rect, Response, Style, TextStyle, Vec2
};
use egui_code_editor::Syntax;

use crate::{Gui::TotkBitsApp, GuiMenuBar::FpsCounter, Tree::TreeNode};

pub struct Settings {
    pub lines_count: usize,
    pub comp_level: i32,
    pub editor_font: TextStyle,
    pub window_color: Color32,
    pub tree_bg_color: Color32, // color of the backgroun
    pub button_size: Vec2,      //size of the buttons
    pub icon_size: Vec2,        //size of the icons for buttons
    pub is_file_loaded: bool, //flag for loading file, prevents the program from loading file from disk in every frame
    pub is_tree_loaded: bool, //flag to reload gui (collapsingheaders) from tree, prevents from traversing tree in every frame
    pub styles: Styles,
    pub syntax: Arc<Syntax>, //syntax for code editor
    pub modded_color: Color32,
    pub is_dir_context_menu: bool, //is context menu for dir opened
    pub dir_context_pos: Option<egui::Pos2>, //
    pub dir_context_size: Option<Response>,
    pub fps_counter: FpsCounter
}

impl Default for Settings {
    fn default() -> Self {
        let def_style = Style::default();
        Self {
            lines_count: 0 as usize,
            comp_level: 16,
            editor_font: TextStyle::Monospace,
            window_color: Color32::from_gray(27),
            tree_bg_color: Color32::from_gray(15),
            button_size: Vec2::default(),
            icon_size: Vec2::new(32.0, 32.0),
            is_file_loaded: true,
            is_tree_loaded: true,
            styles: Styles::new(def_style),
            syntax: Arc::new(Syntax::rust()),
            modded_color: Color32::from_rgb(204, 153, 16),
            is_dir_context_menu: false,
            dir_context_pos: None, //
            dir_context_size: None,
            fps_counter: FpsCounter::new(),
        }
    }
}

pub struct Icons<'a> {
    pub open: egui::Image<'a>,
    pub add: egui::Image<'a>,
    pub add_sarc: egui::Image<'a>,
    pub dir_closed: egui::Image<'a>,
    pub dir_opened: egui::Image<'a>,
    pub extract: egui::Image<'a>,
    pub new: egui::Image<'a>,
    pub save_as: egui::Image<'a>,
    pub save: egui::Image<'a>,
    pub update_from_folder: egui::Image<'a>,
    pub edit: egui::Image<'a>,
}

impl<'a> Icons<'_> {
    pub fn new(size: &Vec2) -> Self {
        Self {
            open: egui::Image::new(include_image!("../icon/open.png")).fit_to_exact_size(*size),
            add: egui::Image::new(include_image!("../icon/add.png")).fit_to_exact_size(*size),
            add_sarc: egui::Image::new(include_image!("../icon/add_sarc.png"))
                .fit_to_exact_size(*size),
            dir_closed: egui::Image::new(include_image!("../icon/dir_closed.png"))
                .fit_to_exact_size(*size),
            dir_opened: egui::Image::new(include_image!("../icon/dir_opened.png"))
                .fit_to_exact_size(*size),
            extract: egui::Image::new(include_image!("../icon/extract.png"))
                .fit_to_exact_size(*size),
            new: egui::Image::new(include_image!("../icon/add_sarc.png")).fit_to_exact_size(*size),
            save_as: egui::Image::new(include_image!("../icon/save_as.png"))
                .fit_to_exact_size(*size),
            save: egui::Image::new(include_image!("../icon/save.png")).fit_to_exact_size(*size),
            update_from_folder: egui::Image::new(include_image!("../icon/update_from_folder.png"))
                .fit_to_exact_size(*size),
            edit: egui::Image::new(include_image!("../icon/edit.png")).fit_to_exact_size(*size),
        }
    }
}

pub struct Styles {
    pub def_style: Arc<Style>,    //default style
    pub tree: Arc<Style>,         //sarc directory tree
    pub text_editor: Arc<Style>,  //text editor textedit
    pub toolbar: Arc<Style>,      // image buttons (save, open)
    pub context_menu: Arc<Style>, //context menu
    pub menubar: Arc<Style>,      //the menu bar at the top
    pub modded_file: Arc<Style>,  //the menu bar at the top
    pub added_file: Arc<Style>,   //the menu bar at the top
    pub vanila_file: Arc<Style>,   //the menu bar at the top
}

impl Styles {
    pub fn new(def_style: Style) -> Self {
        Self {
            def_style: Arc::new(def_style.clone()),
            tree: Arc::new(def_style.clone()),
            text_editor: Arc::new(Styles::get_text_editor_style(def_style.clone())),
            toolbar: Arc::new(Styles::get_toolbar_style(def_style.clone())),
            context_menu: Arc::new(Styles::get_context_menu_style(def_style.clone())),
            menubar: Arc::new(Styles::get_menubar_style(def_style.clone())),
            modded_file: Arc::new(Styles::get_modded_file_style(def_style.clone())),
            added_file: Arc::new(Styles::get_added_file_style(def_style.clone())),
            vanila_file: Arc::new(Styles::get_vanila_file_style(def_style)),
        }
    }

    pub fn get_style_from_comparer(app: &mut TotkBitsApp, ui: &mut egui::Ui, child: &Rc<TreeNode<String>>,) -> Arc<Style> {
        if let Some(pack) = &mut app.pack {
            let path = &child.path.full_path;
            if pack.modded.contains_key(path) {
                //println!("modded {}", path);
                return app.settings.styles.modded_file.clone();
            }
            else if pack.added.contains_key(path) {
                return app.settings.styles.added_file.clone();
            }
        }

        app.settings.styles.vanila_file.clone()
    }

    pub fn get_vanila_file_style(def_style: Style) -> Style {
        let mut style: Style = def_style;
        /*let dark_yellow = Color32::from_rgb(84, 62, 6);
        let yellow = Color32::from_rgb(145, 111, 0);
        //let font_color = Color32::from_gray(27);
        style.visuals.widgets.noninteractive.weak_bg_fill = dark_yellow;
        style.visuals.widgets.inactive.weak_bg_fill = dark_yellow;
        style.visuals.widgets.active.weak_bg_fill = dark_yellow;
        style.visuals.widgets.hovered.weak_bg_fill = dark_yellow;
        style.visuals.widgets.open.weak_bg_fill = dark_yellow;

        style.visuals.widgets.noninteractive.bg_fill = yellow;
        style.visuals.widgets.inactive.bg_fill = yellow;
        style.visuals.widgets.active.bg_fill = yellow;
        style.visuals.widgets.hovered.bg_fill = yellow;
        style.visuals.widgets.open.bg_fill = yellow;

        /*style.visuals.widgets.noninteractive.fg_stroke.color = font_color; // White text
        style.visuals.widgets.inactive.fg_stroke.color = font_color; // White text
        style.visuals.widgets.active.fg_stroke.color = font_color; // White text
        style.visuals.widgets.hovered.fg_stroke.color = font_color; // White text
        style.visuals.widgets.open.fg_stroke.color = font_color; // White text
        */
        style.visuals.selection.bg_fill = yellow;
        //style.visuals.selection.stroke.color = yellow; font
        */
        return style;
    }

    pub fn get_modded_file_style(def_style: Style) -> Style {
        let mut style: Style = def_style;
        let dark_yellow = Color32::from_rgb(84, 62, 6);
        let yellow = Color32::from_rgb(145, 111, 0);
        //let font_color = Color32::from_gray(27);
        style.visuals.widgets.noninteractive.weak_bg_fill = dark_yellow;
        style.visuals.widgets.inactive.weak_bg_fill = dark_yellow;
        style.visuals.widgets.active.weak_bg_fill = dark_yellow;
        style.visuals.widgets.hovered.weak_bg_fill = dark_yellow;
        style.visuals.widgets.open.weak_bg_fill = dark_yellow;

        style.visuals.widgets.noninteractive.bg_fill = yellow;
        style.visuals.widgets.inactive.bg_fill = yellow;
        style.visuals.widgets.active.bg_fill = yellow;
        style.visuals.widgets.hovered.bg_fill = yellow;
        style.visuals.widgets.open.bg_fill = yellow;

        /*style.visuals.widgets.noninteractive.fg_stroke.color = font_color; // White text
        style.visuals.widgets.inactive.fg_stroke.color = font_color; // White text
        style.visuals.widgets.active.fg_stroke.color = font_color; // White text
        style.visuals.widgets.hovered.fg_stroke.color = font_color; // White text
        style.visuals.widgets.open.fg_stroke.color = font_color; // White text
        */
        style.visuals.selection.bg_fill = yellow;
        //style.visuals.selection.stroke.color = yellow; font

        return style;
    }

    pub fn get_added_file_style(def_style: Style) -> Style {
        let mut style: Style = def_style;
        let purple = Color32::from_rgb(109, 0, 158);
        let dark_purple = Color32::from_rgb(87, 0, 127);
        style.visuals.widgets.noninteractive.weak_bg_fill = dark_purple;
        style.visuals.widgets.inactive.weak_bg_fill = dark_purple;
        style.visuals.widgets.active.weak_bg_fill = dark_purple;
        style.visuals.widgets.hovered.weak_bg_fill = dark_purple;
        style.visuals.widgets.open.weak_bg_fill = dark_purple;

        style.visuals.widgets.noninteractive.bg_fill = purple;
        style.visuals.widgets.inactive.bg_fill = purple;
        style.visuals.widgets.active.bg_fill = purple;
        style.visuals.widgets.hovered.bg_fill = purple;
        style.visuals.widgets.open.bg_fill = purple;

        style.visuals.selection.bg_fill = purple;

        return style;
    }

    pub fn get_text_editor_style(def_style: Style) -> Style {
        let mut style: Style = def_style;
        let square_rounding = egui::Rounding::same(0.0);
        let transparent = Color32::TRANSPARENT;
        //No outline
        style.visuals.widgets.noninteractive.bg_stroke.color = transparent;
        style.visuals.widgets.inactive.bg_stroke.color = transparent;
        style.visuals.widgets.active.bg_stroke.color = transparent;
        style.visuals.widgets.hovered.bg_stroke.color = transparent;
        style.visuals.widgets.open.bg_stroke.color = transparent;
        //Square rounding/edges
        style.visuals.menu_rounding = square_rounding;
        style.visuals.widgets.noninteractive.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.inactive.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.hovered.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.active.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.open.rounding = square_rounding; // No rounding on buttons
        style.visuals.window_rounding = square_rounding;

        return style;
    }
    pub fn get_context_menu_style(def_style: Style) -> Style {
        let style: Style = Styles::get_menubar_style(def_style);

        return style;
    }

    pub fn get_toolbar_style(def_style: Style) -> Style {
        let mut style: Style = def_style;
        let square_rounding = egui::Rounding::same(0.0);
        let white = Color32::WHITE;
        let _inactive_color = Color32::from_gray(27);
        let _active_color = Color32::from_gray(60);
        let transparent = Color32::TRANSPARENT;
        style.spacing.item_spacing.x = 0.0;
        //Buttons have the same colors as background
        style.visuals.widgets.noninteractive.weak_bg_fill = transparent;
        style.visuals.widgets.inactive.weak_bg_fill = transparent;
        //No outline
        style.visuals.widgets.noninteractive.bg_stroke.color = transparent;
        style.visuals.widgets.inactive.bg_stroke.color = transparent;
        style.visuals.widgets.active.bg_stroke.color = white;
        style.visuals.widgets.hovered.bg_stroke.color = white;
        style.visuals.widgets.open.bg_stroke.color = white;
        //misc
        style.visuals.widgets.noninteractive.fg_stroke.color = Color32::WHITE; // White text
        style.spacing.item_spacing = Vec2::new(1.25, 0.0);
        style.spacing.menu_margin = Margin::ZERO;
        style.spacing.window_margin = Margin::ZERO;
        //style.visuals.window_shadow = Shadow::NONE;
        //style.visuals.popup_shadow = Shadow::NONE;

        //Square rounding/edges
        style.visuals.menu_rounding = square_rounding;
        style.visuals.widgets.noninteractive.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.inactive.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.hovered.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.active.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.open.rounding = square_rounding; // No rounding on buttons
        style.visuals.window_rounding = square_rounding;
        style.visuals.widgets.noninteractive.fg_stroke.width = 0.0; // Width of the border line
        style.spacing.button_padding = Vec2::new(0.0, 0.0); // Padding inside the buttons
                                                            //style.spacing.window_margin = Margin::symmetric(4.0, 4.0); // Margin around the window
        return style;
    }

    pub fn get_menubar_style(def_style: Style) -> Style {
        let mut style: Style = def_style;
        let square_rounding = egui::Rounding::same(0.0);
        let active_color = Color32::from_gray(60);
        let inactive_color = Color32::from_gray(27);
        let transparent = Color32::TRANSPARENT;
        style.spacing.item_spacing.x = 1.0;

        //Buttons have the same colors as background
        style.visuals.widgets.noninteractive.bg_fill = active_color;
        style.visuals.widgets.inactive.bg_fill = active_color;
        style.visuals.widgets.active.bg_fill = active_color;
        style.visuals.widgets.open.bg_fill = active_color;
        style.visuals.widgets.hovered.bg_fill = active_color;
        //Buttons have the same colors as background
        style.visuals.widgets.noninteractive.weak_bg_fill = inactive_color;
        style.visuals.widgets.inactive.weak_bg_fill = inactive_color;
        style.visuals.widgets.active.weak_bg_fill = active_color;
        style.visuals.widgets.open.weak_bg_fill = active_color; //when clicked
        style.visuals.widgets.hovered.weak_bg_fill = active_color;
        //No outline
        style.visuals.widgets.noninteractive.bg_stroke.color = transparent;
        style.visuals.widgets.inactive.bg_stroke.color = transparent;
        style.visuals.widgets.active.bg_stroke.color = transparent;
        style.visuals.widgets.hovered.bg_stroke.color = transparent;
        style.visuals.widgets.open.bg_stroke.color = transparent;
        //misc
        style.visuals.widgets.noninteractive.fg_stroke.color = Color32::WHITE; // White text
        style.visuals.window_shadow = Shadow::NONE;
        style.visuals.popup_shadow = Shadow::NONE;

        //Square rounding/edges
        style.visuals.menu_rounding = square_rounding;
        style.visuals.widgets.noninteractive.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.inactive.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.hovered.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.active.rounding = square_rounding; // No rounding on buttons
        style.visuals.widgets.open.rounding = square_rounding; // No rounding on buttons
        style.visuals.window_rounding = square_rounding;
        style.visuals.widgets.noninteractive.fg_stroke.width = 1.0; // Width of the border line
        style.spacing.button_padding = Vec2::new(10.0, 4.0); // Padding inside the buttons
        style.spacing.window_margin = Margin::symmetric(4.0, 4.0); // Margin around the window

        return style;
    }
}

#[derive(Debug)]
pub struct Pathlib {
    pub parent: String,
    pub name: String,
    pub stem: String,
    pub extension: String,
    pub full_path: String,
}

impl Pathlib {
    pub fn new(path: String) -> Self {
        let p = Path::new(&path);
        Self {
            parent: Pathlib::get_parent(&path),
            name: Pathlib::get_name(&path),
            stem: Pathlib::get_stem(&path),
            extension: Pathlib::get_extension(&path),
            full_path: path,
        }
    }
    pub fn get_parent(path: &str) -> String {
        //parent dir
        Path::new(path)
            .parent()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or("".to_string())
    }
    pub fn get_name(path: &str) -> String {
        //file name + extension
        Path::new(path)
            .file_name()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or("".to_string())
    }
    pub fn get_stem(path: &str) -> String {
        //just file name
        let mut res = Path::new(path)
            .file_stem()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or("".to_string());
        if res.contains(".") {
           return  res.split(".").next().unwrap_or("").to_string();
        }
        res
    }
    pub fn get_extension(path: &str) -> String {
        let dots = path.chars().filter(|&x| x == '.').count();
        if dots == 0 {return String::new();}
        if dots > 1 {return path.split('.').skip(1).collect::<Vec<&str>>().join(".");}
        Path::new(path)
            .extension()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
            .unwrap_or("".to_string())
    }
    pub fn path_to_string(path: &Path) -> String {
        path.to_str()
            .map(|s| s.to_string())
            .unwrap_or("".to_string())
    }
}
