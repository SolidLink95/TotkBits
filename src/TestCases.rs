use std::collections::HashMap;
//use std::{env, path};
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{self, Read};
use std::str::FromStr;
use std::sync::Arc;
use crate::TotkConfig;
use crate::Pack;
use crate::misc;
use crate::Zstd::{self, ZsDic};
use crate::BymlEntries;
use misc::{print_as_hex};
use Pack::PackFile;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind},
    execute,
    terminal::{enable_raw_mode, disable_raw_mode},
};


pub fn test_case1(totk_config: &TotkConfig::TotkConfig) -> io::Result<String>{
    //let mut zsdic = Arc::new(ZsDic::new(&totk_config)?);
    let TotkZstd: Zstd::TotkZstd<'_> = Zstd::TotkZstd::new(totk_config, 16)?;
    let p = PathBuf::from(r"res\Armor_006_Upper.pack.zs");
    //let compressor = Zstd::ZstdCompressor::new(&totk_config, zsdic, 16)?;
    let mut ret_res: String = Default::default();
    let mut x: PackFile<'_> = PackFile::new(&p, &totk_config, &TotkZstd)?;
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

use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::env;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <file_path>", args[0]);
        return Ok(());
    }

    let file_path = &args[1];
    let file = File::open(file_path)?;
    let mut reader = BufReader::new(file);

    let chunk_size = 1024; // Size of each chunk in bytes
    let mut buffer = vec![0; chunk_size];

    loop {
        println!("Press Page Up/Page Down to scroll, or any other key to exit.");
        
        // Here, you would have code to detect key presses.
        // This is a placeholder for the actual key press handling logic.
        let key_pressed = get_key_press(); // Implement this function based on your environment

        match key_pressed {
            Key::PageUp => {
                // Move the cursor back by twice the chunk size, then read one chunk
                let current_pos = reader.stream_position()?;
                let new_pos = current_pos.saturating_sub(2 * chunk_size as u64);
                reader.seek(SeekFrom::Start(new_pos))?;
            }
            Key::PageDown => {
                // Continue reading the next chunk
            }
            _ => break,
        }

        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break; // End of file reached
        }

        // Display the text chunk
        let text_chunk = String::from_utf8_lossy(&buffer[..bytes_read]);
        println!("{}", text_chunk);
    }

    Ok(())
}

// Dummy function to represent key press handling
fn get_key_press() -> Key {
    // Implement your key press detection logic here
    // For the sake of example, let's just return PageDown
    Key::PageDown
}

// Enum to represent different key presses
enum Key {
    PageUp,
    PageDown,
    // Include other keys as needed
}


pub fn test_key_listener() {
    enable_raw_mode()?;

    // Enable mouse event capturing
    execute!(io::stdout(), EnableMouseCapture)?;

    loop {
        match event::read()? {
            Event::Key(KeyEvent { code, modifiers, .. }) => {
                match code {
                    KeyCode::Esc => break,
                    _ => {
                        print!("Key pressed: {:?}", code);
                        if modifiers.contains(KeyModifiers::SHIFT) {
                            print!(" + Shift");
                        }
                        if modifiers.contains(KeyModifiers::CONTROL) {
                            print!(" + Ctrl");
                        }
                        if modifiers.contains(KeyModifiers::ALT) {
                            print!(" + Alt");
                        }
                        println!();
                        io::stdout().flush()?;
                    }
                }
            },
            Event::Mouse(MouseEvent { kind, .. }) => {
                match kind {
                    MouseEventKind::ScrollUp => {
                        println!("Mouse scrolled up");
                        io::stdout().flush()?;
                    },
                    MouseEventKind::ScrollDown => {
                        println!("Mouse scrolled down");
                        io::stdout().flush()?;
                    },
                    _ => {}
                }
            },
            _ => {}
        }
    }

    // Disable mouse event capturing before exiting
    execute!(io::stdout(), DisableMouseCapture)?;

    disable_raw_mode()?;
}



use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn main1() {
    // Create two channels for two-way communication
    let (tx, rx) = mpsc::channel();
    let (processed_tx, processed_rx) = mpsc::channel();

    // Spawn a new thread
    thread::spawn(move || {
        let mut last_string = String::new();
        loop {
            let new_string = rx.recv().unwrap();
            if new_string != last_string {
                let modified_string = format!("{}{}", new_string, new_string.len());
                processed_tx.send(modified_string).unwrap();
                last_string = new_string;
            }
        }
    });

    // Simulate sending strings to the child thread
    tx.send("Hello".to_string()).unwrap();
    thread::sleep(Duration::from_secs(1));
    // Receive processed string
    println!("Processed: {}", processed_rx.recv().unwrap());

    tx.send("World".to_string()).unwrap();
    thread::sleep(Duration::from_secs(1));
    // Receive processed string
    println!("Processed: {}", processed_rx.recv().unwrap());

    tx.send("World".to_string()).unwrap();
    thread::sleep(Duration::from_secs(1));
    // Attempt to receive processed string
    match processed_rx.try_recv() {
        Ok(s) => println!("Processed: {}", s),
        Err(_) => println!("No new processed string received"),
    }

    // Close the channel
    drop(tx);
}
