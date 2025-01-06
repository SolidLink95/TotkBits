use std::fs::OpenOptions;
use std::thread::sleep;
use std::time::Duration;
use std::{env, fs};
use std::io::{self, Write};
// use std::fs::OpenOptions;
use std::error::Error;
use std::path::PathBuf;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut msgs: Vec<&str> = vec![];
    //parse argv
    let args: Vec<String> = env::args().collect();
    let installed_ver = args.get(1).cloned().unwrap_or_else(|| String::from(""));
    let totkbits_exe_pid_str = args.get(2).cloned().unwrap_or_else(|| String::from(""));
    let is_update_needed = args.get(3).cloned().unwrap_or_else(|| String::from("no")) == "yes";
    if installed_ver.is_empty() {
        println!("[-] Installed version not provided");
        pause();
        return Ok(());
    }
    let mut totkbits_exe_pid: u32 = 0;
    match totkbits_exe_pid_str.parse::<u32>() {
        Ok(pid) => totkbits_exe_pid = pid,
        Err(_) => {
            println!("[-] Invalid Totkbits.exe pid: {:?}, exiting", &totkbits_exe_pid_str);
            pause();
            return Ok(());
        }
    }

    println!("[+] Installed version: {}, Totkbits.exe pid: {}", &installed_ver, &totkbits_exe_pid);
    //Updater
    let mut updater = Updater::Updater::default();
    let root_path = PathBuf::from("C:/Users/Luiza/Documents/coding/TotkBits/tmp");
    updater.temp_dir = root_path.clone().join("updater_temp");
    updater.cwd_dir = root_path.clone().join("updater_cwd_dir");
    updater.get_asset_and_version().await?;
    //calculate installed and newest version
    let installed_ver = TotkbitsVersion::TotkbitsVersion::from_str(&installed_ver);
    if !installed_ver.is_valid() {
        println!("[-] Invalid installed version: {}", installed_ver.as_str());
        pause();
        return Ok(());
    }
    if installed_ver >= updater.latest_ver {
        println!("[+] Latest version installed, no need to update: {} > {}", installed_ver.as_str(), updater.latest_ver.as_str());
        msgs.push("Latest version installed, no need to update");
        // return Ok(());
    } else {
        println!("[+] Update needed: {} < {}", installed_ver.as_str(), updater.latest_ver.as_str());
        msgs.push("Update needed");
    }
    // msgs.push("KILL"); //test kill
    
    // sleep(Duration::from_secs(10));
    // terminate_process_by_pid(totkbits_exe_pid as DWORD)?;
    println!("[+] Totkbits.exe process terminated pid: {}", totkbits_exe_pid);
    pipe_test(msgs);
    println!("[+] Nothing more to do, just debugging");

    // if updater.temp_dir.exists() {
    //     fs::remove_dir_all(&updater.temp_dir)?;
    // }
    // let asset_path = updater.download_7z().await?;
    // updater.backup_current_version()?;
    // updater.decompress_asset(&asset_path)?;
    pause();
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