//! Typed failures. No path leakage beyond the user-facing message.

#[derive(Debug, thiserror::Error)]
pub enum SyomError {
    #[error("media: {0}")]
    Media(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<syom_aac::AacError> for SyomError {
    fn from(e: syom_aac::AacError) -> Self {
        Self::Media(e.to_string())
    }
}

pub(crate) fn media(msg: impl Into<String>) -> SyomError {
    SyomError::Media(msg.into())
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod error_tests;
