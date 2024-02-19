pub mod option_serde_base64;
pub mod serde_base64;

pub fn strip_nul(s: &str) -> &str {
    if let Some(s) = s.strip_suffix('\u{0000}') {
        s
    } else {
        s
    }
    // match s.strip_suffix('\u{0000}') {
    //     Some(ss) => ss,
    //     None => s,
    // }
}

pub fn append_nul(mut s: String) -> String {
    s.push('\u{0000}');
    s
}
