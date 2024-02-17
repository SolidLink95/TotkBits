pub mod botw;
pub mod subcommand;
pub mod model;
pub mod util;
pub mod converter;

pub type Result<T> = std::result::Result<T, failure::Error>;