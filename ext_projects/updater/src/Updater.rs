use indicatif::ProgressBar;
use zip::ZipArchive;
use std::fs::{self, create_dir_all, read_dir, File};
use std::io::{self, copy};
use std::path::{Path, PathBuf};
use std::process::{self};


fn copy_file(src: &Path, dst: &Path) -> io::Result<u64> {
    let mut src_file = File::open(src)?;
    let mut dst_file = File::create(dst)?;
    copy(&mut src_file, &mut dst_file)
}

pub fn list_files_recursively<T: AsRef<Path>>(path: &T) -> Vec<String> {
    let mut files = Vec::new();

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries {
            if let Ok(entry) = entry {
                let entry_path = entry.path();
                if entry_path.is_file() && entry_path.exists() {
                    if let Some(path_str) = entry_path.to_str() {
                        files.push(path_str.to_string().replace("//", "/"));
                    }
                } else if entry_path.is_dir() {
                    // Recurse into subdirectories
                    files.extend(list_files_recursively(&entry_path));
                }
            }
        }
    }

    files
}

// async fn download_with_progress(url: &str, file_path: &Path) -> Result<(), Box<dyn Error>> {
//     let client = Client::new();
//     let mut response = client.get(url).send().await?;

//     // Get the total size of the file (if available)
//     let total_size = response.content_length().unwrap_or(0);

//     // Create a progress bar
//     let pb = ProgressBar::new(total_size);
//     pb.set_style(
//         ProgressStyle::default_bar()
//             .template("{wide_bar} {bytes}/{total_bytes} ({eta})")?
//             .progress_chars("=>-"),
//     );

//     let mut file = File::create(file_path)?;
//     let mut downloaded: u64 = 0;

//     while let Some(chunk) = response.chunk().await? {
//         file.write_all(&chunk)?;
//         downloaded += chunk.len() as u64;
//         pb.set_position(downloaded);
//     }

//     pb.finish_with_message("Download complete/n");
//     Ok(())
// }



pub fn unpack_zip_file<P:AsRef<Path>>(zip_path_p: P, output_dir_p: P) -> Result<(), Box<dyn std::error::Error>> {
    // Open the ZIP file
    let zip_path = zip_path_p.as_ref();
    let output_dir = output_dir_p.as_ref();
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    // Iterate through the files in the archive
    for i in 0..archive.len() {
        let mut file_in_zip = archive.by_index(i)?;
        let outpath = Path::new(output_dir).join(file_in_zip.name());

        if file_in_zip.is_dir() {
            // Create the directory if it doesn't exist
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Create and write to the output file
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file_in_zip, &mut outfile)?;
        }
    }

    Ok(())
}


















pub fn pause() {
    // println!("[+] Usage: updater <latest_ver>");
    println!("[+] Press Enter to exit");
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input); // Wait for user to press Enter
}

pub fn pause_and_exit() {
    pause();
    process::exit(1);
}



pub fn copy_dir_recursive<P: AsRef<Path>>(bar: &mut ProgressBar, src_path: P, dst_path: P) -> io::Result<()> {
    let src = src_path.as_ref();
    let dst = dst_path.as_ref();
    if !src.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source is not a directory",
        ));
    }

    // Create the destination directory if it doesn't exist
    create_dir_all(dst).unwrap_or_default();

    // Iterate over entries in the source directory
    for entry in read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            // Recursively copy subdirectories
            copy_dir_recursive(bar, &src_path, &dst_path)?;
        } else if src_path.is_file() {
            // Copy files
            copy_file(&src_path, &dst_path)?;
            bar.inc(1);
        }
    }

    Ok(())
}

pub fn backup_current_version<P: AsRef<Path>>(cwd_dir_path: P) -> io::Result<()> {
    let cwd_dir =cwd_dir_path.as_ref();
    let backup_dir = cwd_dir.join("backup");
    if backup_dir.exists() {
        std::fs::remove_dir_all(&backup_dir)?;
    }
    std::fs::create_dir_all(&backup_dir)?;
    let entries = vec![
        "bin",
        "misc",
        "src",
        "pip.txt",
        "TotkBits.exe",
        "uninstall.exe",
        "updater.exe",
    ];
    let all_files = list_files_recursively(&cwd_dir);
    let files_count = all_files.len() as u64;
    // let mut skipped_files: i32 = 0;
    // let mut not_copied_files: i32 = 0;
    let mut bar = ProgressBar::new(files_count); // Create a progress bar with 100 steps
    println!("[+] Backing up current version from {}: approximately {} files to copy", &cwd_dir.display(), files_count);
    for entry in entries {
        let entry_path = cwd_dir.join(entry);
        let dest_path = backup_dir.join(entry);
        if entry_path.exists() {
            if entry_path.is_dir() {
                if let Err(e) = copy_dir_recursive(&mut bar, &entry_path, &dest_path) {
                    println!("[-] Error while backing up directory: {:?}", e);
                    pause();
                    process::exit(1);
                }
            } else if entry_path.is_file() {
                if let Err(e) = fs::copy(&entry_path, &dest_path) {
                    println!("[-] Error while backing up file: {:?}", e);
                    pause();
                    process::exit(1);
                }
                bar.inc(1);
            }
        } 
        if dest_path.exists() { //success
            if entry_path.is_dir() {
                if let Err(e) = fs::remove_dir_all(&entry_path) {
                    println!("[-] Error while removing directory: {:?}", e);
                    pause();
                    process::exit(1);
                }
            } else if entry_path.is_file() {
                if let Err(e) = fs::remove_file(&entry_path) {
                    println!("[-] Error while removing file: {:?}", e);
                    pause();
                    process::exit(1);
                }
            }
        } 
    }
    bar.finish_with_message("[+] Finished backup\n");
    println!("[+] Backup to {} complete", &backup_dir.display());
    // let current_dir = self.cwd_dir.join("current");
    // std::fs::rename(&current_dir, &backup_dir)?;
    Ok(())
}

pub fn get_cwd_dir() -> io::Result<PathBuf> {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(cwd_dir) = exe_path.parent() {
            return Ok(cwd_dir.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        return Ok(cwd);
    }
    return Err(io::Error::new(
        io::ErrorKind::Other,
        "Failed to get current working directory",
    ));
}