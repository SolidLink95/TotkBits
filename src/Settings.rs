use std::sync::Arc;

use egui::{epaint::Shadow, include_image, Color32, Margin, Style, TextStyle, Vec2};




pub struct Settings {
    pub lines_count: usize,
    pub comp_level: i32,
    pub editor_font: TextStyle,
    pub window_color: Color32,
    pub tree_bg_color: Color32,
    pub button_size: Vec2,
    pub icon_size: Vec2,
    pub is_file_loaded: bool, //flag for loading file, prevents the program from loading file from disk in every frame
    pub is_tree_loaded: bool,
    pub styles: Styles
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
            icon_size: Vec2::new(32.0,32.0),
            is_file_loaded: true,
            is_tree_loaded: true,
            styles: Styles::new(def_style)
        }
    }
}




pub struct Icons<'a> {
    add: egui::Image<'a>,
    dir_closed: egui::Image<'a>,
    dir_opened: egui::Image<'a>,
    extract: egui::Image<'a>,
    new: egui::Image<'a>,
    save_as: egui::Image<'a>,
    save: egui::Image<'a>,
    update_from_folder: egui::Image<'a>,
}

impl<'a> Icons<'_> {
    pub fn new(size: &Vec2) -> Self {
        Self {
            add: egui::Image::new(include_image!("../icon/add.png")).fit_to_exact_size(*size),
            dir_closed: egui::Image::new(include_image!("../icon/dir_closed.png")).fit_to_exact_size(*size),
            dir_opened: egui::Image::new(include_image!("../icon/dir_opened.png")).fit_to_exact_size(*size),
            extract: egui::Image::new(include_image!("../icon/extract.png")).fit_to_exact_size(*size),
            new: egui::Image::new(include_image!("../icon/extract.png")).fit_to_exact_size(*size),
            save_as: egui::Image::new(include_image!("../icon/new.png")).fit_to_exact_size(*size),
            save: egui::Image::new(include_image!("../icon/save.png")).fit_to_exact_size(*size),
            update_from_folder: egui::Image::new(include_image!("../icon/update_from_folder.png")).fit_to_exact_size(*size)
        }
    }
}

pub struct Styles {
    pub def_style: Style, 
    pub tree: Arc<Style>,
    pub text_editor: Arc<Style>,
    pub toolbar: Arc<Style>,
    pub context_menu: Arc<Style>,
    pub menubar: Arc<Style>,
}

impl Styles {
    pub fn new(def_style: Style) -> Self {
        Self {
            def_style: def_style.clone(), 
            tree: Arc::new(def_style.clone()),
            text_editor: Arc::new(def_style.clone()),
            toolbar: Arc::new(def_style.clone()),
            context_menu: Styles::get_context_menu_style(def_style.clone()),
            menubar: Styles::get_menubar_style(def_style)
        }
    }

    

    pub fn get_context_menu_style(def_style: Style) -> Arc<Style> {
        Styles::get_menubar_style(def_style)
    }
    pub fn get_menubar_style(def_style: Style) -> Arc<Style> {
            let mut style: Style = def_style.clone();
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
    
            return Arc::new(style);
    
    }
}