use std::io::Write;
use std::fs::OpenOptions;
use std::thread::sleep;
use std::time::Duration;
use miow::pipe::NamedPipeBuilder;

fn main() {
    let pipe_name = r"\\.\pipe\tauri_pipe";
    
    // Connect to the named pipe created by the Tauri app
    let mut pipe = loop {
        match OpenOptions::new().write(true).open(pipe_name) {
            Ok(pipe) => break pipe,
            Err(_) => {
                println!("Waiting for pipe...");
                sleep(Duration::from_secs(3));
            }
        }
    };

    // Write message to the pipe
    writeln!(pipe, "Hello from Rust process!").unwrap();
    writeln!(pipe, "Hello from Rust process!").unwrap();
    writeln!(pipe, "Hello from Rust process!").unwrap();
    writeln!(pipe, "END").unwrap();
    println!("Message sent to the Tauri app.");
}
