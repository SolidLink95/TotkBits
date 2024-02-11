use roead::byml::Byml;
use roead::sarc::Sarc;
use std::path::{Path, PathBuf};
//use std::fs;
use std::io::{self, Error, ErrorKind, Read, Write};
use std::collections;
use crate::Zstd::is_byml;

pub struct ActorParam<'a> {
    path: &'a str,
    pio: roead::byml::Byml,
    entries: collections::HashMap<String, String>
}

impl<'a> ActorParam<'_> {
    pub fn new(sarc: &Sarc, actor_path: &'a str) -> io::Result<ActorParam<'a>> {
        let mut pio: Byml = get_byml_pio(sarc, actor_path)
                .expect("Cannot get pio for ActorParam");
        let entries: collections::HashMap<String, String> = get_entries(pio.clone(), actor_path.to_string()).expect("msg");
        
        Ok(ActorParam {
            path: actor_path,
            pio: pio,
            entries: entries
        })
    }

}


pub fn get_byml_pio(sarc: &Sarc, file: &str) -> Option<Byml> {
    let data = sarc.get_data(file);
    let bytes = match data {
        Some(bytes) => {
            if !is_byml(&bytes){
                return None
            }
            let pio: Byml =  Byml::from_binary(bytes).unwrap();
            return Some(pio);
        },
        None => {
            return None;
        }
    };
    

}

pub fn get_entries(pio: Byml, actor_param_path: String) -> Result<collections::HashMap<String, String>, roead::Error> {
    let mut res: collections::HashMap<String, String> = Default::default();
    let pio_map = pio.as_map()?;
    if !pio_map.contains_key("Components") {
        return Ok(res);
    }
    loop {
        for elem in pio["Components"].as_map() {
            for param in elem.keys() {
                if !res.contains_key(&param.to_string()) {
                    res.insert(param.to_string(), elem[param].to_text());
                }
            }
        }
        if !pio_map.contains_key("$parent") {
            return Ok(res);
        }
        //let parent_path = pio["$parent"].to_text(); no recursive yet
        return Ok(res);
    }
}