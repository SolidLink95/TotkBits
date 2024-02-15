//use std::fs::File;
use std::{fs::File, io::{self, BufReader, BufWriter}};

//mod TestCases;
mod BinTextFile;
mod ButtonOperations;
mod BymlEntries;
mod Gui;
mod GuiMenuBar;
mod GuiScroll;
mod Pack;
mod SarcFileLabel;
mod Settings;
mod TotkPath;
mod Tree;
mod Zstd;
mod misc;

//use msyt;
use egui::output;
use msbt::{section::Atr1, Msbt};
use BinTextFile::MsytFile;

//use TestCases::test_case1;
/*
TODO:
- lines numbers for code editor
- byml file name in left rifght corner
- endiannes below*/


fn msbt_to_text(path:String) {
    let f = BufReader::new(File::open(&path).unwrap());

    let msbt = Msbt::from_reader(f).unwrap();
    }
    


fn main() -> io::Result<()> {
    
    //MsytFile::file_to_text("res/Attachment.msbt".to_string());
    Gui::run();
    Ok(())
}
