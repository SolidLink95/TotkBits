//use std::fs::File;
use std::{fs::{self, File}, io::{self, BufReader, BufWriter, Cursor, Write}, sync::Arc};

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
use roead::byml::Byml;
use BinTextFile::{BymlFile, MsytFile};
use Zstd::TotkZstd;

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
