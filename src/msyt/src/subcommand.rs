use walkdir::{DirEntry, WalkDir};

use crate::Result;

use std::path::PathBuf;

pub mod create;
pub mod export;
pub mod import;

pub use self::{
  create::create,
  export::export,
  import::import,
};

pub fn find_files<'a>(paths: impl Iterator<Item = &'a str>, ext: &str) -> Result<Vec<PathBuf>> {
  paths
    .flat_map(|p| WalkDir::new(p)
      .into_iter()
      .map(|e| e.map(DirEntry::into_path))
      .filter(|p| p.as_ref().map(|p| p.is_file() && p.extension().and_then(std::ffi::OsStr::to_str) == Some(ext)).unwrap_or(false)))
      .map(|p| p.map_err(Into::into))
    .collect()
}
