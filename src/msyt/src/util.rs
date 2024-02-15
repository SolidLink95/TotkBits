pub mod option_serde_base64;
pub mod serde_base64;

pub fn strip_nul(s: &str) -> &str {
  if s.ends_with('\u{0000}') {
    &s[..s.len() - 1]
  } else {
    s
  }
}

pub fn append_nul(mut s: String) -> String {
  s.push('\u{0000}');
  s
}
