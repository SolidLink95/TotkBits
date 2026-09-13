use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BntxError {
    pub offset: usize,
    pub message: String,
}

impl BntxError {
    pub(super) fn new(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }
}

impl fmt::Display for BntxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BNTX error at 0x{:X}: {}", self.offset, self.message)
    }
}

impl std::error::Error for BntxError {}

impl From<std::io::Error> for BntxError {
    fn from(error: std::io::Error) -> Self {
        Self::new(0, error.to_string())
    }
}

impl From<BntxError> for std::io::Error {
    fn from(error: BntxError) -> Self {
        std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
    }
}
