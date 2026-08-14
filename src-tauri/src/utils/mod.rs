mod AppPaths;
mod FileUtilities;
pub mod LookupData;
mod Startup;
mod ValueUtilities;
mod magic;

pub const NO_WINDOW_FLAG: u32 = 0x08000000;

pub use magic::Magic;
pub use AppPaths::{exe_relative_path, running_exe_dir, Pathlib};
pub use FileUtilities::{
    list_files_recursively, makedirs, read_string_from_file, write_string_to_file,
};
pub use Startup::{get_startup_data, launch_weapon_icon_cache, StartupData};
pub use ValueUtilities::{process_inline_content, update_json};
