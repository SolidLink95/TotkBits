use std::collections::HashMap;
//use std::{env, path};
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{self, Read};
use std::str::FromStr;
mod TotkPath;
mod Pack;
mod misc;
mod Zstd;
mod BymlEntries;
use misc::{print_as_hex};
use Pack::PackFile;


fn asdf(totkPath: &TotkPath::TotkPath) -> io::Result<()>{
    let p = PathBuf::from(r"W:\coding\learning\res\Armor_006_Upper.pack.zs");
    let decompressor = Zstd::ZstdDecompressor::new(&totkPath)?;
    let compressor = Zstd::ZstdCompressor::new(&totkPath, 16)?;
    let mut x: PackFile<'_> = PackFile::new(&p, &totkPath, &decompressor, &compressor)?;
    for file in x.sarc.files(){
        let name  = file.name().unwrap();
        if name.starts_with("Actor/") {
            println!("{}", file.name().unwrap());
            let data = file.data();
            let mut pio = roead::byml::Byml::from_binary(&data.clone()).unwrap();//.expect("msg");
            println!("  {:?}", pio["Components"].as_mut_map().unwrap().contains_key("ModelInfoRef"));
            for e in pio["Components"].as_map() {
                println!("  {:?}", e);
            }
            println!("  {:?}", pio["Components"]["ModelInfoRef"].as_string());
            pio["Components"]["ModelInfoRef"] = roead::byml::Byml::String("DUPA".to_string().into());
            let  t = pio["Components"].as_mut_map().expect("Dupa huj");
            t.remove("ModelInfoRef");
            t.remove("ASRef");
            //pio["Components"].as_mut_map().unwrap().remove("ModelInfoRef");
            println!("  {:?}", t.contains_key("ModelInfoRef"));
            println!("  {:?}", pio["Components"].as_mut_map().unwrap().contains_key("ModelInfoRef"));
            //let pio1 = roead::byml::Byml::from_binary(t.).expect("msg");
            for e in pio["Components"].as_map() {
                for key in e.keys() {
                    println!("  XXXXXXXXXXXX{:?} {:?}",key, e[key].as_string().unwrap());
            }}
        }
    }
    //x.save("res/asdf/zxcv.pack")?;
    //x.save("res/asdf/zxcv.pack.zs")?;
    Ok(())
}

fn main() -> io::Result<()> {
    //let totkPath = TotkPath::TotkPath::new(PathBuf::from(""), PathBuf::from(""));
    let totkPath = TotkPath::TotkPath::new(
        //r"W:\TOTK_modding\0100F2C0115B6000\romfs", 
        //r"W:\TOTK_modding\0100F2C0115B6000\Bfres_1.1.2"
    );
    let _ = totkPath.print();
    let zstd = Zstd::ZstdDecompressor::new(&totkPath)?;
    println!("Hello, world!");
    asdf(&totkPath);
    let _ = totkPath.print();


    println!("{:?}", totkPath.get_pack_path("Player").unwrap());
    Ok(())
}

