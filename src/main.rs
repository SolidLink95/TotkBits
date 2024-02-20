//use std::fs::File;
use std::{fs::{self, File}, io::{self, BufReader, BufWriter, Cursor, Read, Write}, sync::Arc};

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
    let mut buf = String::new();
    f.read_to_string(&mut buf);
    buf
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
impl Shared {
    fn update(&mut self) {
        if self.pos[0] < 0 {
            self.pos[0] = 0;
        } else if self.pos[0] > self.text.len() - self.chunk {
            self.pos[0] = self.text.len() - self.chunk;
        }
        if self.pos[1] < self.chunk {
            self.pos[1] = self.chunk.clone();
        } else if self.pos[1] > self.text.len() {
            self.pos[1] = self.text.len() -1;
        }
    }
}


fn main() {
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
    let key_listener_handle = start_key_listener(tx);

    loop {
        if let Ok(data) = rx.try_recv() {
            println!("Data received in main thread: {:?}", data);
            //println!("\n\n{}", data);





            
        //if key_code == KeyCode::Esc {
        //    break;
        //}
        }

        // Main thread can perform other tasks here
        // ...

        // Sleep to prevent the loop from running too fast
        
        thread::sleep(Duration::from_millis(10));
    }

    key_listener_handle.join().unwrap();
}

fn start_key_listener(tx: Sender<String>) -> thread::JoinHandle<()> {
    let mut payload = Shared {
        text: get_string(),
        key: KeyCode::Null,
        chunk: 20,
        pos: vec![0,20]
    };
    let mut l = payload.text.len();
    println!("{:?}", l);
    thread::spawn(move || {
        enable_raw_mode().unwrap();
        loop {
            if let Ok(Event::Key(key_event)) = event::read() {
                
                let key_code = key_event.code;
                payload.key = key_code;
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
                payload.update();

                println!("{:?}", payload.pos);
                let data = &payload.text[payload.pos[0]..payload.pos[1]];
                tx.send(data.to_string()).unwrap();
                if key_code == KeyCode::Esc {
                    break;
                }
            }
        }
        disable_raw_mode().unwrap();
    })
}

//Stop-Process -Name "Totkbits" -Force