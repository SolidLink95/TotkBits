use crate::misc::{self, open_file_dialog};
use crate::BymlFile::byml_file;
use crate::GuiMenuBar::{MenuBar};
use crate::Pack::PackFile;
use crate::SarcFileLabel::SarcLabel;
use crate::Settings::{Icons, Settings};
use crate::TotkPath::TotkPath;
use crate::Tree::{self, tree_node};
use crate::Zstd::{totk_zstd};
//use crate::SarcFileLabel::ScrollAreaPub;
use eframe::egui::{
    self, ScrollArea, SelectableLabel, TopBottomPanel,
};
use egui::text::LayoutJob;
use egui::{
    Align, Label, Layout, Pos2, Rect, Shape
};
use egui_extras::install_image_loaders;

use rfd::FileDialog;
use roead::byml::Byml;

use std::io::Read;

use std::path::{PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::{fs, io};

struct EfficientScroll {
    alr_scrolled: f32,
    editor_height: f32,
    window_height: f32,
    scrolled_perc: f32
}

impl EfficientScroll {
    pub fn new() -> Self {
        Self {
            alr_scrolled: 0.0,
            editor_height: 0.0,
            window_height: 0.0,
            scrolled_perc: 0.0
        }
    }
}