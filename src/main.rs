//use std::fs::File;
use std::{fs::File, io::{self, BufReader, BufWriter, Cursor}};

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
mod TotkConfig;
mod Tree;
mod Zstd;
mod misc;
use msyt::{model::{Content, Msyt}, Result as MsbtResult};

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


    


fn main() -> io::Result<()> {
    //MsytFile::file_to_text("res/Attachment.msbt".to_string());
    Gui::run();
    Ok(())
}
