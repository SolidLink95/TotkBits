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
use egui::output;
use msbt::{section::Atr1, Msbt};

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
    /*let path = "".to_string();
    let msbt_file = File::open(&path).unwrap();
    let msbt = Msbt::from_reader(BufReader::new(msbt_file)).unwrap();
    let mut buf: Vec<u8> = Vec::new();
    let new_f = BufWriter::new(buf);
    msbt.write_to(new_f).unwrap();*/
    //println!("{}", code_content);
    //GuiUpdated::run();
    //Tree::test_tree();
    //Tree::test_paths_tree();
    Gui::run();
    Ok(())
}
