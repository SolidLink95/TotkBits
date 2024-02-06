use std::path::{Path, PathBuf};
//use std::fs::File;
use std::io::{self, Read};
use std::str::FromStr;
mod TestCases;
mod TotkPath;
mod Pack;
mod misc;
mod Zstd;
mod BymlEntries;
mod Gui;
use Pack::PackFile;
use TestCases::test_case1;
use std::thread;

fn main() -> io::Result<()> {
    //let totk_path = TotkPath::TotkPath::new(PathBuf::from(""), PathBuf::from(""));
    let totk_path = TotkPath::TotkPath::new(
        //r"W:\TOTK_modding\0100F2C0115B6000\romfs", 
        //r"W:\TOTK_modding\0100F2C0115B6000\Bfres_1.1.2"
    );
    let _ = totk_path.print();
    //let zstd = Zstd::ZstdDecompressor::new(&totk_path)?;
    println!("Hello, world!");
    test_case1(&totk_path);
    let _ = totk_path.print();


    println!("{:?}", totk_path.get_pack_path("Player").unwrap());
    Gui::run();
    Ok(())
}

