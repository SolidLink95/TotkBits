use std::path::{Path, PathBuf};
//use std::fs::File;
use std::io::{self, Read};
use std::str::FromStr;
//mod TestCases;
mod TotkPath;
mod Pack;
mod misc;
mod BymlFile;
mod Tree;
mod Zstd;
mod BymlEntries;
mod Gui;
use Tree::tree_node;
use Pack::PackFile;
//use TestCases::test_case1;
use std::thread;

fn main() -> io::Result<()> {
 

    //println!("{}", code_content);
    //GuiUpdated::run();
    //Tree::test_tree();
    //Tree::test_paths_tree();
    Gui::run();
    Ok(())
}

