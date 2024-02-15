
//use std::fs::File;
use std::io::{self};

//mod TestCases;
mod ButtonOperations;
mod TotkPath;
mod Pack;
mod Settings;
mod misc;
mod BinTextFile;
mod Tree;
mod GuiScroll;
mod Zstd;
mod BymlEntries;
mod Gui;
mod GuiMenuBar;
mod SarcFileLabel;


//use TestCases::test_case1;
/*
TODO:
- lines numbers for code editor
- byml file name in left rifght corner
- endiannes below 
*/

fn main() -> io::Result<()> {
 

    //println!("{}", code_content);
    //GuiUpdated::run();
    //Tree::test_tree();
    //Tree::test_paths_tree();
    Gui::run();
    Ok(())
}

