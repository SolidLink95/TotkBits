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
use egui::{output, Pos2};
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

fn get_string() -> String{
    let mut f = fs::File::open(r"res\Tag.Product.120.rstbl.byml.zs.json").unwrap();
    let mut buf: Vec<u8> = Vec::new();
    f.write_all(&mut buf);
    String::from_utf8(buf).unwrap()
}


use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{enable_raw_mode, disable_raw_mode},
};
use std::sync::mpsc::{self, Sender, Receiver};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
struct Shared {
    text: String,
    key: KeyCode,
    chunk: usize,
    pos: Vec<usize>
}


fn main() {
    let (tx, rx): (Sender<KeyCode>, Receiver<KeyCode>) = mpsc::channel();
    let key_listener_handle = start_key_listener(tx);

    loop {
        if let Ok(key_code) = rx.try_recv() {
            println!("Key received in main thread: {:?}", key_code);
            //println!("\n\n{}", data);
        }

        // Main thread can perform other tasks here
        // ...

        // Sleep to prevent the loop from running too fast
        thread::sleep(Duration::from_millis(10));
    }

    key_listener_handle.join().unwrap();
}

fn start_key_listener(tx: Sender<KeyCode>) -> thread::JoinHandle<()> {
    let mut payload = Shared {
        text: get_string(),
        key: KeyCode::Null,
        chunk: 1024,
        pos: vec![0,0]
    };
    let mut l = payload.text.len();
    println!("{:?}", l);
    thread::spawn(move || {
        enable_raw_mode().unwrap();
        loop {
            if let Ok(Event::Key(key_event)) = event::read() {
                
                let key_code = key_event.code;
                match key_code {
                    KeyCode::PageDown => {
                        payload.pos[0] += 50;
                        payload.pos[1] += 50
                    },
                    KeyCode::PageUp => {
                        payload.pos[0] -= 50;
                        payload.pos[1] -= 50
                    },
                    KeyCode::Down => {
                        payload.pos[0] += 10;
                        payload.pos[1] += 10;
                    },
                    KeyCode::Up => {
                        payload.pos[0] -= 10;
                        payload.pos[1] -= 10;
                    },
                    _ => {},
                }
                /*if payload.pos.x<0.0 {payload.pos.x = 0.0;}
                else if payload.pos.x > (l - payload.chunk) as f32 {payload.pos.x = (l - payload.chunk) as f32;}
                if payload.pos.y<payload.chunk as f32 {payload.pos.y = payload.chunk as f32;}
                else if payload.pos.y < l as f32  {payload.pos.y = l as f32;}*/

                //println!("{:?}", payload);
                let data = &payload.text[payload.pos[0]..payload.pos[1]];
                tx.send(key_code).unwrap();
                if key_code == KeyCode::Esc {
                    break;
                }
            }
        }
        disable_raw_mode().unwrap();
    })
}
