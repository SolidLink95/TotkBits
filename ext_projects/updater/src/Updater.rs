use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use sevenz_rust::decompress_file as decompress_7z_file;
use zip::ZipArchive;
use std::error::Error;
use std::fs::{self, create_dir_all, read_dir, File};
use std::io::{self, copy, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::TotkbitsVersion::TotkbitsVersion;



#[derive(Serialize, Debug, Clone)]
pub struct Updater {
    pub repo_owner: String,
    pub repo_name: String,
    pub latest_ver: TotkbitsVersion,
    pub installed_ver: TotkbitsVersion,
    pub url: String,
    pub asset: Asset,
    #[serde(skip)]
    pub client: Client,
    pub temp_dir: PathBuf, // Temporary directory to store downloaded files
    pub cwd_dir: PathBuf, //where the download will be extracted
    #[serde(skip)]
    pub bar: ProgressBar,
}

impl Default for Updater {
    fn default() -> Self {
        let repo_owner = "SolidLink95".to_string();
        let repo_name = "TotkBits".to_string();
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            &repo_owner, &repo_name
        );
        Updater {
            repo_owner: repo_owner,
            repo_name: repo_name,
            latest_ver: Default::default(),
            installed_ver: Default::default(),
            url: url,
            asset: Default::default(),
            client: Client::new(),
            temp_dir: Default::default(),
            cwd_dir: Default::default(),
            bar: ProgressBar::new(0),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Release {
    pub tag_name: String,   // The release tag (e.g., "v1.0.0")
    pub assets: Vec<Asset>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

impl Default for Asset {
    fn default() -> Self {
        Asset {
            name: "".to_string(),
            browser_download_url: "".to_string(),
        }
    }
}

impl Updater {
    pub fn is_update_needed(&self) -> bool {
        self.installed_ver < self.latest_ver
    }

    pub async fn check_if_update_needed(&mut self, installed_ver: &str) -> Result<(), Box<dyn Error>> {
        if !self.installed_ver.is_valid() {
            self.installed_ver = TotkbitsVersion::from_str(&installed_ver);
        }
        if let Err(_) =    self.get_asset_and_version(".7z").await {
            self.get_asset_and_version(".zip").await?;
        }
        if !self.is_update_needed() {
            println!("[+] Latest version installed, no need to update: {} > {}", &self.installed_ver.as_str(), &self.latest_ver.as_str());
            return Err("Latest version installed".into());
        }
    
    
        Ok(())
    }


    pub async fn get_asset_and_version(&mut self, ext: &str) -> Result<(), Box<dyn Error>> {
        let response = self
            .client
            .get(&self.url)
            .header("User-Agent", "rust-client")
            .send()
            .await?
            .json::<Release>()
            .await?;
        println!("[+] Release tag: {}", response.tag_name);
        self.latest_ver = TotkbitsVersion::from_str(&response.tag_name);
        if !self.latest_ver.is_valid() {
            println!("[-] Invalid version number");
            return Err("[-] Invalid version number".into());
        }

        let asset = response
            .assets
            .iter()
            .find(|a| a.name.ends_with(ext))
            .ok_or("[-] No .7z file found in the latest release")?;
        println!("[+] Found {}  file: {}", ext,  &asset.name);
        self.asset = asset.clone();
        
        Ok(())
    }

    pub async fn download_asset(&mut self) -> Result<PathBuf, Box<dyn Error>> {
        if self.asset.name.is_empty() || self.asset.browser_download_url.is_empty() {
            println!("[-] Execute get_asset_and_version first!");
            return Err("[-] Execute get_asset_and_version first!".into());
        }
        if !self.temp_dir.exists() {
            fs::create_dir_all(&self.temp_dir)?;
        }
        // Step 3: Download the .7z file
        let file_path = Path::new(&self.temp_dir).join(&self.asset.name);
        println!("[+] Downloading {}...", &file_path.display());
        download_with_progress(&self.asset.browser_download_url, &file_path).await?;
        println!("[+] Downloaded {}...", &file_path.display());
        Ok(file_path)
    }
    
    pub fn decompress_asset<P: AsRef<Path>>(&mut self, asset: P) -> Result<(), Box<dyn Error>> {
        let path_str = asset.as_ref().to_string_lossy().to_string();
        println!("[+] Decompressing:/n      {:?}/n  to:/n       {} ...", &asset.as_ref().display(), &self.cwd_dir.display());
        if path_str.to_ascii_lowercase().ends_with(".zip") {
            return unpack_zip_file(&path_str, &self.cwd_dir.to_string_lossy());
        }
        
        if let Err(e) = decompress_7z_file(&asset, &self.cwd_dir) {
            println!("[-] Error decompressing {} with sevenz_rust, attempting subprocess: {:?}", &asset.as_ref().display(), e);
        } else {
            return Ok(());
        }
        self.decompress_subprocess_7z(asset.as_ref())
    }
    pub fn decompress_subprocess_7z<P: AsRef<Path>>(&mut self, asset: P) -> Result<(), Box<dyn Error>> {
        let output = Command::new("C:/Program Files/7-Zip/7z.exe")
        .arg("x")
        .arg(asset.as_ref())
        .arg(format!("-o{}", self.cwd_dir.display())) // Correct output directory argument
        .output()?;

    // Check if the command was successful
    if !output.status.success() {
        return Err(format!(
            "Failed to decompress {}: {}",
            asset.as_ref().display(),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    // Print standard output and error for debugging purposes
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("{}", String::from_utf8_lossy(&output.stderr));

    Ok(())
    }

    pub async fn new_default() -> Result<Self, Box<dyn Error>> {
        let mut upd = Updater::default();
        upd.get_temp_dir()?;
        upd.get_cwd_dir()?;
        upd.backup_current_version()?;
        let asset_path = upd.download_asset().await?;
        upd.decompress_asset(&asset_path)?;
        // let backup_dir = upd.cwd_dir.join("backup");

        // Step 4: Extract the .7z file using `7z` command-line tool

        Ok(upd)
    }

    pub fn clean_up(&mut self) -> io::Result<()> {
        if self.temp_dir.exists() {
            fs::remove_dir_all(&self.temp_dir)?;
        }
        let backup_path = self.cwd_dir.join("backup");
        if backup_path.exists() {
            fs::remove_dir_all(&backup_path)?;
        }
        Ok(())
    }

    pub fn get_temp_dir(&mut self) -> io::Result<()> {
        let temp_dir = std::env::temp_dir().join("TotkBitsTmp");
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir)?;
        }
        std::fs::create_dir_all(&temp_dir)?;
        self.temp_dir = temp_dir;
        Ok(())
    }
    pub fn get_cwd_dir(&mut self) -> io::Result<()> {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(cwd_dir) = exe_path.parent() {
                self.cwd_dir = cwd_dir.to_path_buf();
                return Ok(());
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            self.cwd_dir = cwd;
            return Ok(());
        }
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to get current working directory",
        ));
    }
    pub fn backup_current_version(&mut self) -> io::Result<()> {
        let backup_dir = self.cwd_dir.join("backup");
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
        let all_files = list_files_recursively(&self.cwd_dir);
        let files_count = all_files.len() as u64;
        // let mut skipped_files: i32 = 0;
        // let mut not_copied_files: i32 = 0;
        self.bar = ProgressBar::new(files_count); // Create a progress bar with 100 steps
        println!("[+] Backing up current version from {}: approximately {} files to copy", &self.cwd_dir.display(), files_count);
        for entry in entries {
            let entry_path = self.cwd_dir.join(entry);
            let dest_path = backup_dir.join(entry);
            if entry_path.exists() {
                if entry_path.is_dir() {
                    self.copy_dir_recursive(&entry_path, &dest_path)?;
                } else if entry_path.is_file() {
                    fs::copy(&entry_path, &dest_path)?;
                    self.bar.inc(1);
                }
            } 
            if dest_path.exists() { //success
                if entry_path.is_dir() {
                    fs::remove_dir_all(&entry_path)?;
                } else if entry_path.is_file() {
                    fs::remove_file(&entry_path)?;
                }
            } 
        }
        self.bar.finish_with_message("Finished backup");
        println!("[+] Backup to {} complete", &backup_dir.display());
        // let current_dir = self.cwd_dir.join("current");
        // std::fs::rename(&current_dir, &backup_dir)?;
        Ok(())
    }

    fn copy_dir_recursive<P: AsRef<Path>>(&mut self, src_path: P, dst_path: P) -> io::Result<()> {
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
                self.copy_dir_recursive(&src_path, &dst_path)?;
            } else if src_path.is_file() {
                // Copy files
                copy_file(&src_path, &dst_path)?;
                self.bar.inc(1);
            }
        }

        Ok(())
    }
}

fn extract_7z_file<P: AsRef<Path>>(archive_path: P, output_dir: P) -> Result<(), Box<dyn Error>> {
    // let archive = Path::new(archive_path);
    // let output = Path::new(output_dir);

    // Extract the .7z archive to the specified output directory
    decompress_7z_file(&archive_path, &output_dir)?;

    println!(
        "Successfully extracted {:?} to {:?}",
        archive_path.as_ref(),
        output_dir.as_ref()
    );
    Ok(())
}

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

async fn download_with_progress(url: &str, file_path: &Path) -> Result<(), Box<dyn Error>> {
    let client = Client::new();
    let mut response = client.get(url).send().await?;

    // Get the total size of the file (if available)
    let total_size = response.content_length().unwrap_or(0);

    // Create a progress bar
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{wide_bar} {bytes}/{total_bytes} ({eta})")?
            .progress_chars("=>-"),
    );

    let mut file = File::create(file_path)?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("Download complete/n");
    Ok(())
}



fn unpack_zip_file(zip_path: &str, output_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Open the ZIP file
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