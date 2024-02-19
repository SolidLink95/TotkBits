pub use byteordered::Endianness;
mod botw;
// mod cli;
pub mod model;
// mod subcommand;
mod util;

pub use crate::model::Msyt;
pub type Result<T> = std::result::Result<T, anyhow::Error>;
