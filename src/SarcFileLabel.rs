use std::rc::Rc;

use roead::byml::Byml;
use egui::{epaint, lerp, pos2, remap, remap_clamp, vec2, Context, Id, NumExt, Pos2, Rangef, Rect, SelectableLabel, Sense, Vec2, Vec2b };
use egui::scroll_area::ScrollBarVisibility;
use egui::Ui;
use crate::Tree::tree_node;


struct Label {
    root_node: Rc<tree_node<String>>,
    node: Rc<tree_node<String>>,

}
