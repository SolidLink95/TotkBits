//use std::fs::File;
use std::{fs::{self}, io::{Read}};

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
mod widgets;
mod Tree;
mod Zstd;
mod Open_Save;
mod misc;


//use msyt;




//use TestCases::test_case1;
/*
TODO:
- lines numbers for code editor
- byml file name in left rifght corner
- endiannes below*/

fn get_string() -> String{
    let mut f = fs::File::open(r"res\Tag.Product.120.rstbl.byml.zs.json").unwrap();
    let mut buf = String::new();
    f.read_to_string(&mut buf);
    buf
}



fn main() {

    Gui::run();
    
}
