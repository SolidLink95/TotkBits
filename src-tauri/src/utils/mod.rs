mod AppPaths;
mod FileUtilities;
pub mod LookupData;
mod PanicGuard;
mod Startup;
mod ValueUtilities;
mod magic;

pub const NO_WINDOW_FLAG: u32 = 0x08000000;

/// A `Command` for `program` that never opens a console window on Windows.
/// Every external process the app spawns goes through here.
pub fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut command = std::process::Command::new(program);
    command.creation_flags(NO_WINDOW_FLAG);
    command
}

pub use magic::Magic;
pub use AppPaths::{exe_relative_path, running_exe_dir, Pathlib};
pub use FileUtilities::{
    list_files_recursively, makedirs, read_string_from_file, write_string_to_file,
};
pub use PanicGuard::{catch_panic, catch_panic_with, panic_message};
pub(crate) use Startup::cache_directory;
pub use Startup::{get_startup_data, launch_weapon_icon_cache, StartupData};
pub use ValueUtilities::{process_inline_content, update_json};
