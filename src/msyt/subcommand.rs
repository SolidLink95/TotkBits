use crate::Result;

use glob::glob;
use std::path::PathBuf;

pub mod create;
pub mod export;
pub mod import;

pub use self::{create::create, export::export, import::import};

pub fn find_files<'a>(paths: impl Iterator<Item = &'a str>, ext: &str) -> Result<Vec<PathBuf>> {
    paths
        .flat_map(|p| glob(format!("{}/**/*.{}", p, ext).as_str()).expect("Glob error?"))
        .map(|p| p.map_err(Into::into))
        .collect()
}
