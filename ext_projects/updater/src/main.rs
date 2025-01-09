use std::fs::OpenOptions;
use std::process::{self, Command};
use std::thread::sleep;
use std::time::Duration;
use std::{env, fs};
use std::io::{self, Write};
// use std::fs::OpenOptions;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use indicatif::ProgressBar;
use reqwest::blocking::Client;
use serde::Deserialize;
use updater::Updater::{backup_current_version, copy_dir_recursive, get_cwd_dir, pause, unpack_zip_file};
use winapi::um::processthreadsapi::OpenProcess;
use winapi::um::processthreadsapi::TerminateProcess;
use winapi::um::handleapi::CloseHandle;
use winapi::um::winnt::PROCESS_TERMINATE;
use winapi::shared::minwindef::DWORD;
use sysinfo::{Pid, System};
use serde_json;
use sevenz_rust::decompress_file as decompress_7z_file;

// use std::thread::sleep;
// use std::time::Duration;
// use miow::pipe::NamedPipeBuilder;
// mod Updater;
mod TotkbitsVersion;

fn download_and_extract(name: &str, url: &str, cwd_dir: &str) -> io::Result<()> {
    let client = Client::new();
    let temp_path = env::temp_dir();
    let temp_file = temp_path.join(name);
    if !temp_path.exists() {
        fs::create_dir_all(&temp_path)?;
    }
    println!("[+] Downloading {} to {}...", name, temp_file.display());
    let response = client.get(url).send().map_err(|e| io::Error::new(io::ErrorKind::Other, "[-] Failed to get reponse"))?;
    let bytes = response.bytes().map_err(|e| io::Error::new(io::ErrorKind::Other, "[-] Failed to get bytes from response"))?;
    let mut file = OpenOptions::new().write(true).create(true).open(&temp_file).map_err(|e| io::Error::new(io::ErrorKind::Other, "[-] Failed to open file"))?;
    file.write_all(&bytes)?;
    println!("[+] Downloaded {} to {} ({}MB)", name, temp_file.display(), bytes.len() / 1024 / 1024);
    //extract
    let mut is_done = false;
    if name.to_ascii_lowercase().ends_with(".7z"){
        println!("[+] Extracting 7z file to {}...", cwd_dir);
        if let Err(e) = decompress_7z_file(&temp_file, cwd_dir) {
            println!("[-] Failed to extract 7z file: {}", e);
            pause();
            return Ok(());
        } else {
            println!("[+] 7z file extracted to {}", cwd_dir);
            is_done = true;
        }
    }
    if name.to_ascii_lowercase().ends_with(".zip"){
        println!("[+] Extracting zip file to {}...", cwd_dir);
        if let Err(e) = unpack_zip_file(&temp_file, &PathBuf::from(cwd_dir)) {
            println!("[-] Failed to extract zip file: {}", e);
            pause();
            return Ok(());
        } else {
            println!("[+] Zip extracted to {}", cwd_dir);
            is_done = true;
        }
    }
    fs::remove_file(&temp_file).unwrap_or_default();
    if is_done {
        let backup_dir = PathBuf::from(cwd_dir).join("backup");
        if backup_dir.exists() {
            if let Ok(_) = fs::remove_dir_all(&backup_dir) {
                println!("[+] Removed backup directory.");
            } else {
                println!("[-] Failed to remove backup directory. Please remove it manually.");
            }
        }
        return Ok(());
    }
    Err(io::Error::new(io::ErrorKind::Other, "[-] Failed to extract file."))
}

fn process_main() -> io::Result<()> {
    //Init
    let binding = "".to_string();
    let repo_owner = "SolidLink95".to_string();
    let repo_name = "TotkBits".to_string();
    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        &repo_owner, &repo_name
    );
    let args: Vec<String> = env::args().collect();
    let cur_ver_str = args.get(1).cloned().unwrap_or_else(|| String::from(""));
    let latest_ver_str = args.get(2).cloned().unwrap_or_else(|| String::from(""));
    if cur_ver_str.is_empty() {
        println!("[-] No current version provided.");
        pause();
        return Ok(());
    }
    if latest_ver_str.is_empty() {
        println!("[-] No latest version provided.");
        pause();
        return Ok(());
    }

    println!("[+] Checking for updates...");
    let client = Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "MyAppName")
        .send();

    if let Ok(response) = response {

        if let Ok(json_value) = response.json::<serde_json::Value>() {
            // println!("\n\nJson value: {:?}", json_value);
            if let Some(git_ver_str) = json_value["tag_name"].as_str() {
                let latest_ver = TotkbitsVersion::TotkbitsVersion::from_str(&latest_ver_str);
                let git_ver = TotkbitsVersion::TotkbitsVersion::from_str(git_ver_str);
                if git_ver < latest_ver {
                    println!("[-] Latest version is behind currently installed version: {} vs {}", git_ver_str, latest_ver_str);
                    pause();
                    return Ok(());
                }
                let empty_vec = vec![];
                let assets = json_value["assets"].as_array().unwrap_or(&empty_vec);
                if assets.is_empty() {
                    println!("[-] No assets found for download in the latest release.");
                    pause();
                    return Ok(());
                }

                let filtered_assets_7z: Vec<(String, String)> = assets
                .iter()
                .filter_map(|asset| {
                    let name = asset["name"].as_str()?;
                    let url = asset["browser_download_url"].as_str()?;
                    if name.ends_with(".7z") {
                        Some((name.to_string(), url.to_string()))
                    } else {
                        None
                    }
                })
                .collect();
                let filtered_assets_zip: Vec<(String, String)> = assets
                .iter()
                .filter_map(|asset| {
                    let name = asset["name"].as_str()?;
                    let url = asset["browser_download_url"].as_str()?;
                    if name.ends_with(".7z") {
                        Some((name.to_string(), url.to_string()))
                    } else {
                        None
                    }
                })
                .collect();

                if filtered_assets_7z.is_empty() && filtered_assets_zip.is_empty() {
                    println!("[-] No 7z or zip assets found for download in the latest release.");
                    pause();
                    return Ok(());
                }
                let mut cwd_dir:PathBuf = Default::default();
                if let Ok(cwd) = get_cwd_dir() {
                    cwd_dir = cwd;
                } else {
                    println!("[-] Failed to get current working directory.");
                    pause();
                    return Ok(());
                }
                let cwd_dir_str = cwd_dir.to_string_lossy().to_string();
                println!("[+] Backing up current version...");
                if let Err(e) = backup_current_version(&cwd_dir) {
                    println!("[-] Failed to backup current version: {}", e);
                    pause();
                    return Ok(());
                }
                let mut is_done = false;
                if !filtered_assets_7z.is_empty() {
                    let (name, url) = &filtered_assets_7z[0];
                    println!("[+] Downloading {}...", name);
                    if let Err(e) = download_and_extract(name, url, &cwd_dir_str) {
                        println!("[-] Failed to download and extract 7z file: \n    {:?}", e);
                    } else {
                        is_done = true;
                    }
                }
                if !is_done && !filtered_assets_zip.is_empty() {
                    let (name, url) = &filtered_assets_zip[0];
                    println!("[+] Downloading {}...", name);
                    if let Err(e) = download_and_extract(name, url, &cwd_dir_str) {
                        println!("[-] Failed to download and extract 7z file: \n    {:?}", e);
                    } else {
                        is_done = true;
                    }
                }
                if !is_done {
                    println!("[-] Failed to download and extract any file. Reverting backup...");
                    let mut bar = ProgressBar::new(100);
                    if let Ok(_) = copy_dir_recursive(&mut bar,&cwd_dir.join("backup"), &cwd_dir) {
                        println!("[+] Reverted backup successfully.");
                    } else {
                        println!("[-] Failed to revert backup.");
                    }
                    pause();
                    process::exit(1);
                    return Ok(());
                }
            }
        }
    }
    
    Ok(())
}



fn main() -> io::Result<()> {
    // decompress_7z_file(r"W:\coding\TotkBits\res\Totkbits_portable_v008(1).7z", r"W:\coding\TotkBits\res");
    process_main()?;
    if let Ok(_) =  Command::new("cmd")
        .arg("/c")
        .arg("start")
        .arg("TotkBits.exe")
        .spawn() {
        println!("[+] Started TotkBits.exe");
        } else {
            println!("[-] Failed to start TotkBits.exe");
        }
    println!("\n\n[+] Update applied successfully. You can safely close this window.");
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