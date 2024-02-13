use crate::misc::{self, open_file_dialog};
use crate::BymlFile::byml_file;
use crate::Gui::TotkBitsApp;
use crate::GuiMenuBar::MenuBar;
use crate::SarcFileLabel::SarcLabel;
use crate::Settings::{Icons, Settings};
use crate::Tree::{self, tree_node};
//use crate::SarcFileLabel::ScrollAreaPub;
use eframe::egui::{self, ScrollArea, SelectableLabel, TopBottomPanel};
use egui::text::LayoutJob;
use egui::{Align, Label, Layout, Pos2, Rect, Shape, Vec2};
use egui_extras::install_image_loaders;
use rfd::FileDialog;
use std::io::Read;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::{fs, io};

pub struct EfficientScroll {
    //pub scroll: ScrollArea,
    chunk_sm: f32, // 1/3 of the read chunk
    chunk: f32,
    chunk_n: usize, //the text if finite - this means it has limited number of chunks; we are currently in chunk_n chunk
    pub top_space: f32,
    pub bottom_space: f32,
    alr_scrolled: f32,         //already scrolled pixels
    editor_height: f32,        //height of text
    window_height: f32,        //height of the window
    scrolled_perc: f32,        //percentage of scrolled text
    lines_count: f32,        //number of lines in text
    lines_range: Vec<usize>,   //number of lines in text
    line_per_pixel_ratio: f32, //ratio between letter height in pixels and font size (0.95833333333 for 12)
    change_per_perc: f32, // what percentage of app.text I need to read before reading next/previous chunk?
    text_len: f32, // what percentage of app.text I need to read before reading next/previous chunk?
}

impl EfficientScroll {
    pub fn new() -> Self {
        let chunk_sm = 3000.0;
        Self {
            //scroll: ScrollArea::vertical(),
            chunk_sm: chunk_sm,
            chunk: chunk_sm * 3.0,
            chunk_n: 0 as usize,
            top_space: 0.0,
            bottom_space: 0.0,
            alr_scrolled: 0.0,
            editor_height: 0.0,
            window_height: 0.0,
            scrolled_perc: 0.0,
            lines_count: 0.0,
            lines_range: vec![0 as usize, 0 as usize],
            line_per_pixel_ratio: 0.95833333333,
            change_per_perc: 0.0,
            text_len: 0.0,
        }
    }

    fn to_status_bar(app: &mut TotkBitsApp, start_ind: usize, end_ind: usize) {
        let sc = &mut app.scroll_updater;
        app.status_text = format!(
            "{} space: {}-{}, text ind.: {}-{}, perc: {}%, editor h.: {}, chunk_n:{}",
            sc.alr_scrolled as i32,
            sc.top_space,
            sc.bottom_space,
            start_ind,
            end_ind,
            sc.scrolled_perc * 100.0,
            sc.editor_height,
            sc.chunk_n
        );
    }

    pub fn update(app: &mut TotkBitsApp) {
        let sc = &mut app.scroll_updater;
        //if sc.chunk_n % 3 == 1 || sc.chunk_n == 0 {
        //    EfficientScroll::to_status_bar(app, 0, 0);
        //     return;
        //}
        if let Some(r) = &app.scroll_resp {
            sc.text_len = app.text.len() as f32;
            sc.alr_scrolled = r.state.offset.y;
            if r.content_size.y > 0.0 {
                sc.editor_height = r.content_size.y;
            }
            sc.scrolled_perc = sc.alr_scrolled / sc.editor_height;
            sc.chunk_n = (sc.scrolled_perc / sc.change_per_perc) as usize;
            app.status_text = format!(
                "{} space: {}-{}, perc: {:.2}%, editor h.: {}, chunk_n:{}, lines: {}",
                sc.alr_scrolled as i32,
                sc.top_space,
                sc.bottom_space,
                sc.scrolled_perc * 100.0,
                sc.editor_height,
                sc.chunk_n,
                sc.lines_count,
            );
            if sc.chunk_n % 3 == 1 || sc.chunk_n == 0 {return;}
            sc.lines_count = app.text.chars().filter(|&c| c == '\n').count() as f32 + 1.0;
            sc.change_per_perc = sc.chunk_sm / sc.editor_height;

            let chunk_perc = sc.change_per_perc * 3.0;
            let cur_perc = sc.scrolled_perc % chunk_perc;

            let upper_border = sc.change_per_perc * 2.0;
            let start =
                sc.scrolled_perc - (sc.scrolled_perc % sc.change_per_perc) - sc.change_per_perc;
            let end = start + chunk_perc;
            let start_ind = (start * sc.lines_count) as usize;
            let end_ind = ((end * sc.text_len) as usize).min((sc.lines_count-1.0) as usize);

            if cur_perc > upper_border || (cur_perc < sc.change_per_perc && sc.chunk_n != 0) {
                let mut new_text: String = String::new();//"\n".repeat(start_ind).to_string();
                let middle_text: Vec<&str> = app.text.split("\n").collect();
                for i in start_ind..end_ind {   
                    new_text.push_str(&format!("{}\n", middle_text[i]))

                }
                //new_text.push_str(&"\n".repeat(end_ind));
                app.displayed_text = new_text;
            }

            app.status_text = format!(
                "{} space: {}-{}, text ind.: {}-{}, perc: {:.2}%, editor h.: {}, chunk_n:{}, lines: {}",
                sc.alr_scrolled as i32,
                sc.top_space,
                sc.bottom_space,
                start_ind,
                end_ind,
                sc.scrolled_perc * 100.0,
                sc.editor_height,
                sc.chunk_n,
                sc.lines_count,
            );
        }
    }

    pub fn update1(app: &mut TotkBitsApp) {
        let mut sc = &mut app.scroll_updater;
        if app.scroll_resp.is_none() {
            return;
        } //nothing to update
        let text_len = app.text.len() as f32;
        //sc.resp = app.scroll_resp;
        let r = app.scroll_resp.as_ref().unwrap();
        sc.alr_scrolled = r.state.offset.y;
        sc.editor_height = r.content_size.y;
        sc.scrolled_perc = sc.alr_scrolled / sc.editor_height;
        //sc.lines_count = app.text.chars().filter(|&c| c == '\n').count() + 1; // app.text.chars().filter(|&c| c == '\n').count() + 1; //lines count of text
        sc.change_per_perc = sc.chunk_sm / sc.editor_height; //how many % is chunk_sm compared to editor height?

        let chunk_perc = sc.change_per_perc * 3.0; //sc.chunk is this % of whole text height
        let cur_perc = sc.scrolled_perc % chunk_perc; //determine in which part of chunk we are right now (first, middle or last)
        sc.chunk_n = (sc.scrolled_perc / sc.change_per_perc) as usize; //we are in chunk_n-chunk
        let upper_border = sc.change_per_perc * 2.0; //if i cross this border, i need to read next sc.change_per_perc % of app.text
        let start = sc.scrolled_perc - (sc.scrolled_perc % sc.change_per_perc) - sc.change_per_perc; //starting offset of the fresh string
        let end = start + chunk_perc;
        let start_ind = (start * text_len) as usize;
        let end_ind = (end * text_len) as usize;
        if cur_perc > upper_border {
            //moving closer to the end of chunk

            let new_text: String = app
                .text
                .chars()
                .skip(start_ind)
                .take(end_ind - start_ind)
                .collect();
            app.text = new_text;
            //app.text = new_text;
            sc.top_space = (sc.chunk_n as f32 - 1.0) * sc.chunk_sm; //sc.chunk_n always > 1
            sc.bottom_space = sc.editor_height - sc.top_space - sc.chunk;
            app.status_text = format!(
                "{} space: {}-{}, text ind.: {}-{}, perc: {}%, editor h.: {}, chunk_n:{}",
                sc.alr_scrolled as i32,
                sc.top_space,
                sc.bottom_space,
                start_ind,
                end_ind,
                sc.scrolled_perc * 100.0,
                sc.editor_height,
                sc.chunk_n
            );
        } else if cur_perc < sc.change_per_perc && sc.chunk_n != 0 {
            //we are in upper chunk_sm, ignore if we are in first chunk_sm

            let new_text: String = app
                .text
                .chars()
                .skip(start_ind)
                .take(end_ind - start_ind)
                .collect();
            app.text = new_text;
            sc.top_space = (sc.chunk_n as f32 - 1.0) * sc.chunk_sm;
            sc.bottom_space = sc.editor_height - sc.top_space - sc.chunk;
            app.status_text = format!(
                "{} space: {}-{}, text ind.: {}-{}, perc: {}%, editor h.: {}, chunk_n:{}",
                sc.alr_scrolled as i32,
                sc.top_space,
                sc.bottom_space,
                start_ind,
                end_ind,
                sc.scrolled_perc * 100.0,
                sc.editor_height,
                sc.chunk_n
            );
        } else {
            app.status_text = format!(
                "{} space: {}-{}, text ind.: {}-{}, perc: {}%, editor h.: {}, chunk_n:{}",
                sc.alr_scrolled as i32,
                sc.top_space,
                sc.bottom_space,
                start_ind,
                end_ind,
                sc.scrolled_perc * 100.0,
                sc.editor_height,
                sc.chunk_n
            );
        }
        //else if sc.scrolled_perc < 33.3 { //moving closer to the begin. of the chunk

        //}
        //else { //reading current chunk

        //    sc.lines_range = vec![]
        //}
    }

    pub fn scroll_move(app: &mut TotkBitsApp) {}

    pub fn get_lines_count(app: &mut TotkBitsApp) -> usize {
        app.text.chars().filter(|&c| c == '\n').count() + 1
    }
}
