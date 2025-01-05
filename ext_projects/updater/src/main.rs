use std::fs;
use std::io::{self, Write};
// use std::fs::OpenOptions;
use std::error::Error;
use std::path::PathBuf;
// use std::thread::sleep;
// use std::time::Duration;
// use miow::pipe::NamedPipeBuilder;
mod Updater;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut updater = Updater::Updater::default();
    let root_path = PathBuf::from("C:/Users/Luiza/Documents/coding/TotkBits/tmp");
    updater.temp_dir = root_path.clone().join("updater_temp");
    updater.cwd_dir = root_path.clone().join("updater_cwd_dir");
    if updater.temp_dir.exists() {
        fs::remove_dir_all(&updater.temp_dir)?;
    }
    let asset_path = updater.download_7z().await?;
    updater.backup_current_version()?;
    updater.decompress_asset(&asset_path)?;

    Ok(())
}


// fn pipe_test() {
//     let pipe_name = r"//./pipe/tauri_pipe";
    
//     // Connect to the named pipe created by the Tauri app
//     let mut pipe = loop {
//         match OpenOptions::new().write(true).open(pipe_name) {
//             Ok(pipe) => break pipe,
//             Err(_) => {
//                 println!("Waiting for pipe...");
//                 sleep(Duration::from_secs(3));
//             }
//         }
//     };

//     // Write message to the pipe
//     writeln!(pipe, "Hello from Rust process!").unwrap();
//     writeln!(pipe, "Hello from Rust process!").unwrap();
//     writeln!(pipe, "Hello from Rust process!").unwrap();
//     writeln!(pipe, "END").unwrap();
//     println!("Message sent to the Tauri app.");
// }