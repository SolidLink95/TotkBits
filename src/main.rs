use std::path::{Path, PathBuf};
//use std::fs::File;
use std::io::{self, Read};
use std::str::FromStr;
//mod TestCases;
mod TotkPath;
mod Pack;
mod Settings;
mod misc;
mod BymlFile;
mod Tree;
mod Zstd;
mod BymlEntries;
mod Gui;
mod GuiMenuBar;
mod SarcFileLabel;
mod CodeEditorFormatter;
use Tree::tree_node;
use Pack::PackFile;
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

