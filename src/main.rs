//#![windows_subsystem = "windows"]
//use std::fs::File;


//mod TestCases;
mod file_format;
mod ButtonOperations;
mod Gui;
mod GuiMenuBar;
mod GuiScroll;
mod SarcFileLabel;
mod Settings;
mod FileReader;
mod TotkConfig;
mod ui_elements;
mod widgets;
mod Tree;
mod Zstd;
mod Open_Save;
mod misc;


//use msyt;


fn main() {

    Gui::run();
    
}
