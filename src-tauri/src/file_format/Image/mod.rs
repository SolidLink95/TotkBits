#![allow(non_snake_case)]

pub mod block_compress;
pub mod dds;
mod document;
pub mod gdiplus_resample;
pub mod png;
pub mod raster;
pub(crate) mod switch_texture;

pub use document::{BntxReplacementReport, ImageDocument, RenderedImage};
