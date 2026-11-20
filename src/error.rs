use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BxdlError {
    pub code: String,
    pub message: String,
    /// Local operator guidance; never serialize paths into shared error reports.
    pub(crate) local_hint: Option<String>,
}
impl BxdlError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            local_hint: None,
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
