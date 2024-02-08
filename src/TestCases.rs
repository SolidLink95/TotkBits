use std::collections::HashMap;
//use std::{env, path};
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{self, Read};
use std::str::FromStr;
use std::sync::Arc;
use crate::TotkPath;
use crate::Pack;
use crate::misc;
use crate::Zstd::{self, ZsDic};
use crate::BymlEntries;
use misc::{print_as_hex};
use Pack::PackFile;


pub fn test_case1(totk_path: &TotkPath::TotkPath) -> io::Result<String>{
    //let mut zsdic = Arc::new(ZsDic::new(&totk_path)?);
    let TotkZstd: Zstd::totk_zstd<'_> = Zstd::totk_zstd::new(totk_path, 16)?;
    let p = PathBuf::from(r"res\Armor_006_Upper.pack.zs");
    //let compressor = Zstd::ZstdCompressor::new(&totk_path, zsdic, 16)?;
    let mut ret_res: String = Default::default();
    let mut x: PackFile<'_> = PackFile::new(&p, &totk_path, &TotkZstd)?;
    for file in x.sarc.files(){
        let name  = file.name().unwrap();
        println!("{}",name);
        if name.starts_with("Actor/") {
            
            println!("{}", file.name().unwrap());
            let data = file.data();
            let mut pio = roead::byml::Byml::from_binary(&data.clone()).unwrap();//.expect("msg");
            ret_res = pio.to_text();
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
    x.save("res/asdf/zxcv.pack")?;
    x.save("res/asdf/zxcv.pack.zs")?;
    Ok(ret_res)
}