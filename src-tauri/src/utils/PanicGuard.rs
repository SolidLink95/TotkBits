//! Converts panics from third-party decoders and parsers into ordinary
//! errors so one malformed game file cannot take down the whole app.

/// Runs `operation`, turning a panic into an `Err` carrying the panic
/// message. Use it at command boundaries and around libraries known to
/// panic on malformed input instead of returning errors.
pub fn catch_panic<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    catch_panic_with(operation, Err)
}

/// Runs `operation`; if it panics, `on_panic` builds the value to return
/// from the panic message instead of letting the panic unwind further.
/// Tauri invokes synchronous commands on the main thread, so a panic that
/// escapes a command aborts the whole process; every command boundary
/// funnels through here.
pub fn catch_panic_with<T>(operation: impl FnOnce() -> T, on_panic: impl FnOnce(String) -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)) {
        Ok(value) => value,
        Err(panic) => on_panic(panic_message(panic.as_ref())),
    }
}

pub fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("the parser panicked");
    format!("panicked: {message}")
}
