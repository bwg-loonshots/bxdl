use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BxdlError {
    pub code: String,
    pub message: String,
}
impl BxdlError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl fmt::Display for BxdlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for BxdlError {}
pub type Result<T> = std::result::Result<T, BxdlError>;
