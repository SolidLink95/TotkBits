//! Converts panics from third-party decoders and parsers into ordinary
//! errors so one malformed game file cannot take down the whole app.

/// Runs `operation`, turning a panic into an `Err` carrying the panic
/// message. Use it at command boundaries and around libraries known to
/// panic on malformed input instead of returning errors.
pub fn catch_panic<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
        .unwrap_or_else(|panic| Err(panic_message(panic.as_ref())))
}

pub fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("the parser panicked");
    format!("panicked: {message}")
}
