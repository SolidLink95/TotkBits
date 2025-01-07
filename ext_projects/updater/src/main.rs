use std::fs::OpenOptions;
use std::thread::sleep;
use std::time::Duration;
use std::{env, fs};
use std::io::{self, Write};
// use std::fs::OpenOptions;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use winapi::um::processthreadsapi::OpenProcess;
use winapi::um::processthreadsapi::TerminateProcess;
use winapi::um::handleapi::CloseHandle;
use winapi::um::winnt::PROCESS_TERMINATE;
use winapi::shared::minwindef::DWORD;
use sysinfo::{Pid, System};

// use std::thread::sleep;
// use std::time::Duration;
// use miow::pipe::NamedPipeBuilder;
mod Updater;
mod TotkbitsVersion;

struct ArgvStruct {
    installed_ver: String,
    download_url: String,
    asset_name: String,
    latest_ver: String,
}
impl ArgvStruct {
    fn is_valid(&self) -> bool {
        if self.installed_ver.is_empty() {
            println!("[-] Installed version not provided");
            return false;
        }
        if self.download_url.is_empty() {
            println!("[-] Download  url not provided");
            return false;
        }
        if self.asset_name.is_empty() {
            println!("[-] Asset name not provided");
            return false;
        }
        if self.latest_ver.is_empty() {
            println!("[-] Latest version not provided");
            return false;
        }
        return true;
    }
}


async fn main_process() -> Result<(), Box<dyn Error>>  {
    let mut msgs: Vec<&str> = vec![];
    //parse argv
    let args: Vec<String> = env::args().collect();
    let installed_ver_str = args.get(1).cloned().unwrap_or_else(|| String::from(""));
    let download_url = args.get(2).cloned().unwrap_or_else(|| String::from(""));
    let asset_name = args.get(3).cloned().unwrap_or_else(|| String::from(""));
    let latest_ver_str = args.get(4).cloned().unwrap_or_else(|| String::from(""));
    let argv = ArgvStruct {
        installed_ver: installed_ver_str.clone(),
        download_url: download_url.clone(),
        asset_name: asset_name.clone(),
        latest_ver: latest_ver_str.clone(),
    };
    if !argv.is_valid() {
        pause();
        return Ok(());
    }
    //Updater
    let installed_ver = TotkbitsVersion::TotkbitsVersion::from_str(&installed_ver_str);
    let latest_ver = TotkbitsVersion::TotkbitsVersion::from_str(&latest_ver_str);
    if !installed_ver.is_valid() {
        println!("[-] Invalid installed version: {}", installed_ver_str);
        pause();
        return Ok(());
    }
    if !latest_ver.is_valid() {
        println!("[-] Invalid latest version: {}", latest_ver_str);
        pause();
        return Ok(());
    }
    if installed_ver >= latest_ver {
        println!("[+] Latest version installed, no need to update: {} > {}", installed_ver.as_str(), latest_ver.as_str());
        pause();
        return Ok(());
    } 
    let mut updater = Updater::Updater::default();
    updater.latest_ver = latest_ver;
    updater.installed_ver = installed_ver;
    updater.asset.name = asset_name;
    updater.asset.browser_download_url = download_url;
    updater.get_cwd_dir()?;
    updater.get_temp_dir()?;
    if let Err(e) = updater.backup_current_version() {
        println!("[-] Error backing up current version: {:?}", e);
        fs::remove_dir_all(&updater.temp_dir)?;
        pause();
        return Ok(());
    }
    if let Err(e) = updater.download_asset().await {
        println!("[-] Error downloading asset: {:?}", e);
        pause();
        return Ok(());
    }
    let file_path = Path::new(&updater.temp_dir).join(&updater.asset.name);
    if let Err(e) = updater.decompress_asset(&file_path) {
        println!("[-] Error decompressing asset {} with subprocess: {:?}", &file_path.display(), e);
        pause();
        return Ok(());
    }
    if let Err(e) = fs::remove_file(&file_path) {
        println!("[-] Error removing asset file: {:?}", e);
        pause();
        return Ok(());
    }
    if let Err(e) = updater.clean_up() {
        println!("[-] Error cleaning up: {:?}", e);
        pause();
        return Ok(());
    }
    println!("[+] Update successful: {} -> {}", &updater.installed_ver.as_str(), updater.latest_ver.as_str());


    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    main_process().await?;

    Ok(())
}


fn pipe_test(msgs: Vec<&str>) {
    let pipe_name = r"//./pipe/tauri_pipe";
    
    // Connect to the named pipe created by the Tauri app
    let mut pipe = loop {
        match OpenOptions::new().write(true).open(pipe_name) {
            Ok(pipe) => break pipe,
            Err(_) => {
                println!("Waiting for pipe...");
                sleep(Duration::from_secs(1));
            }
        }
    };

    // Write message to the pipe
    for msg in msgs {
        writeln!(pipe, "{}", msg).unwrap();
    }
    writeln!(pipe, "END").unwrap();
    println!("[+] Message sent to the Tauri app.");
}

fn pause() {
    println!("[+] Usage: updater <installed_ver> <download_url> <asset_name> <latest_ver>");
    println!("[+] Press Enter to exit");
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input); // Wait for user to press Enter
}

fn terminate_process_by_pid(pid: DWORD) -> Result<(), String> {
    unsafe {
        // Open the process with PROCESS_TERMINATE access rights
        let process_handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if process_handle.is_null() {
            return Err(format!("Failed to open process with PID {}.", pid));
        }

        // Attempt to terminate the process
        let result = TerminateProcess(process_handle, 1); // Exit code 1
        CloseHandle(process_handle); // Always close the handle

        if result == 0 {
            return Err(format!("Failed to terminate process with PID {}.", pid));
        }
    }

    Ok(())
}