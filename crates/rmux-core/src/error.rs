//! Error types for rmux-core.

/// Errors that can occur in core grid/screen operations.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Grid position is out of bounds.
    #[error("grid position out of bounds: ({x}, {y}) in grid of size ({sx}, {sy})")]
    GridOutOfBounds { x: u32, y: u32, sx: u32, sy: u32 },

    /// Invalid UTF-8 sequence encountered.
    #[error("invalid UTF-8 sequence")]
    InvalidUtf8,

    /// Layout constraint violation during resize.
    #[error("layout constraint violation: {0}")]
    LayoutConstraint(String),

    /// Option key not found.
    #[error("unknown option: {0}")]
    UnknownOption(String),

    /// Option type mismatch.
    #[error("option type mismatch for '{key}': expected {expected}, got {got}")]
    OptionTypeMismatch { key: String, expected: &'static str, got: &'static str },
}

/// Convenience type alias for core operations.
pub type CoreResult<T> = Result<T, CoreError>;
