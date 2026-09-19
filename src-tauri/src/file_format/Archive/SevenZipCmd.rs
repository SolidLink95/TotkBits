use super::SevenZip::SevenZipFile;
use super::{detect_archive_magic, validate_entry_path, ArchiveCodec, ArchiveMagic, ArchiveResult};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const COMMAND_THRESHOLD: usize = 2 * 1024 * 1024;

#[derive(Default)]
pub struct SevenZipCmd {
    entries: BTreeMap<String, Vec<u8>>,
    prefer_command: bool,
}

impl SevenZipCmd {
    pub fn discover_executable() -> Option<PathBuf> {
        if let Some(paths) = env::var_os("PATH") {
            for directory in env::split_paths(&paths) {
                let candidate = directory.join("7z.exe");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }

        let mut candidates = Vec::new();
        for variable in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(directory) = env::var_os(variable) {
                candidates.push(PathBuf::from(directory).join("7-Zip").join("7z.exe"));
            }
        }
        if let Some(directory) = env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(directory).join("Programs/7-Zip/7z.exe"));
        }
        candidates.push(PathBuf::from(r"C:\ProgramData\chocolatey\bin\7z.exe"));
        candidates.into_iter().find(|path| path.is_file())
    }

    fn from_command(data: &[u8], executable: &Path) -> ArchiveResult<Self> {
        let temporary = TemporaryDirectory::new()?;
        let archive = temporary.path.join("input.7z");
        let output = temporary.path.join("output");
        fs::write(&archive, data).map_err(|e| format!("failed to stage 7z archive: {e}"))?;
        fs::create_dir(&output)
            .map_err(|e| format!("failed to create 7z output directory: {e}"))?;

        validate_command_listing(executable, &archive)?;

        let result = crate::Settings::hidden_command(executable)
            .arg("x")
            .arg(&archive)
            .arg(format!("-o{}", output.display()))
            .args(["-y", "-bd", "-bb0"])
            .output()
            .map_err(|e| format!("failed to start {}: {e}", executable.display()))?;
        if !result.status.success() {
            return Err(command_error("extract", &result));
        }

        let mut entries = BTreeMap::new();
        collect_entries(&output, &output, &mut entries)?;
        Ok(Self {
            entries,
            prefer_command: true,
        })
    }

    fn to_command(&self, executable: &Path) -> ArchiveResult<Vec<u8>> {
        if self.entries.is_empty() {
            return SevenZipFile::default().to_bytes();
        }
        let temporary = TemporaryDirectory::new()?;
        let input = temporary.path.join("input");
        let archive = temporary.path.join("output.7z");
        fs::create_dir(&input).map_err(|e| format!("failed to create 7z input directory: {e}"))?;
        for (name, bytes) in &self.entries {
            validate_entry_path(name)?;
            let destination = input.join(Path::new(name));
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to stage 7z entry {name}: {e}"))?;
            }
            fs::write(&destination, bytes)
                .map_err(|e| format!("failed to stage 7z entry {name}: {e}"))?;
        }

        let result = crate::Settings::hidden_command(executable)
            .current_dir(&input)
            .arg("a")
            .arg("-t7z")
            .arg(&archive)
            .arg(".")
            .args(["-y", "-bd", "-bb0"])
            .output()
            .map_err(|e| format!("failed to start {}: {e}", executable.display()))?;
        if !result.status.success() {
            return Err(command_error("create", &result));
        }
        fs::read(&archive).map_err(|e| format!("failed to read generated 7z archive: {e}"))
    }

    fn in_process(&self) -> ArchiveResult<Vec<u8>> {
        let mut archive = SevenZipFile::default();
        archive.entries_mut().clone_from(&self.entries);
        archive.to_bytes()
    }
}

impl ArchiveCodec for SevenZipCmd {
    fn from_bytes(data: &[u8]) -> ArchiveResult<Self> {
        if detect_archive_magic(data) != Some(ArchiveMagic::SevenZip) {
            return Err("7z magic bytes do not match".into());
        }
        if data.len() > COMMAND_THRESHOLD {
            if let Some(executable) = Self::discover_executable() {
                return Self::from_command(data, &executable);
            }
        }
        let archive = SevenZipFile::from_bytes(data)?;
        Ok(Self {
            entries: archive.entries().clone(),
            prefer_command: false,
        })
    }

    fn to_bytes(&self) -> ArchiveResult<Vec<u8>> {
        let unpacked_size: usize = self.entries.values().map(Vec::len).sum();
        if self.prefer_command || unpacked_size > COMMAND_THRESHOLD {
            if let Some(executable) = Self::discover_executable() {
                return self.to_command(&executable);
            }
        }
        self.in_process()
    }

    fn entries(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.entries
    }

    fn entries_mut(&mut self) -> &mut BTreeMap<String, Vec<u8>> {
        &mut self.entries
    }
}

fn collect_entries(
    root: &Path,
    directory: &Path,
    entries: &mut BTreeMap<String, Vec<u8>>,
) -> ArchiveResult<()> {
    for item in fs::read_dir(directory).map_err(|e| format!("failed to read 7z output: {e}"))? {
        let item = item.map_err(|e| format!("failed to read 7z output entry: {e}"))?;
        let path = item.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|e| format!("failed to inspect extracted 7z entry: {e}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "7z archive contains an unsupported link: {}",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_entries(root, &path, entries)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "7z extracted an entry outside its output directory".to_string())?;
            let name = relative.to_string_lossy().replace('\\', "/");
            validate_entry_path(&name)?;
            let bytes = fs::read(&path)
                .map_err(|e| format!("failed to read extracted 7z entry {name}: {e}"))?;
            entries.insert(name, bytes);
        }
    }
    Ok(())
}

fn command_error(action: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if message.is_empty() {
        format!(
            "7z.exe failed to {action} archive with status {}",
            output.status
        )
    } else {
        format!("7z.exe failed to {action} archive: {message}")
    }
}

fn validate_command_listing(executable: &Path, archive: &Path) -> ArchiveResult<()> {
    let result = crate::Settings::hidden_command(executable)
        .arg("l")
        .arg("-slt")
        .arg("-ba")
        .arg("-sccUTF-8")
        .arg(archive)
        .output()
        .map_err(|e| format!("failed to start {}: {e}", executable.display()))?;
    if !result.status.success() {
        return Err(command_error("list", &result));
    }
    let listing = String::from_utf8(result.stdout)
        .map_err(|_| "7z.exe returned a non-UTF-8 archive listing".to_string())?;
    for line in listing.lines() {
        if let Some(name) = line.strip_prefix("Path = ") {
            let name = name.trim_end_matches(['/', '\\']).replace('\\', "/");
            validate_entry_path(&name)?;
        }
    }
    Ok(())
}

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new() -> ArchiveResult<Self> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("system clock error while creating 7z workspace: {e}"))?
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "totkbits-7z-{}-{timestamp}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).map_err(|e| format!("failed to create 7z workspace: {e}"))?;
        Ok(Self { path })
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_archives_use_in_process_fallback() {
        let mut source = SevenZipFile::default();
        source
            .entries_mut()
            .insert("folder/a.txt".into(), b"value".to_vec());
        let archive = SevenZipCmd::from_bytes(&source.to_bytes().unwrap()).unwrap();
        assert!(!archive.prefer_command);
        assert_eq!(archive.get("folder/a.txt"), Some(b"value".as_slice()));
    }

    #[test]
    fn command_backend_roundtrips_when_installed() {
        let Some(executable) = SevenZipCmd::discover_executable() else {
            eprintln!("skipping 7z command test: 7z.exe not installed");
            return;
        };
        let source = SevenZipCmd {
            entries: BTreeMap::from([
                ("folder/a.txt".into(), b"alpha".to_vec()),
                ("folder/b.bin".into(), vec![42; 4096]),
            ]),
            prefer_command: true,
        };
        let bytes = source.to_command(&executable).unwrap();
        let reopened = SevenZipCmd::from_command(&bytes, &executable).unwrap();
        assert_eq!(reopened.entries, source.entries);
    }
}
