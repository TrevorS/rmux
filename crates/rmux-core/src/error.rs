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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_grid_oob() {
        let e = CoreError::GridOutOfBounds { x: 5, y: 10, sx: 80, sy: 24 };
        let msg = format!("{e}");
        assert!(msg.contains("5"));
        assert!(msg.contains("10"));
        assert!(msg.contains("80"));
        assert!(msg.contains("24"));
    }

    #[test]
    fn error_display_invalid_utf8() {
        let e = CoreError::InvalidUtf8;
        let msg = format!("{e}");
        assert!(msg.contains("UTF-8"));
    }

    #[test]
    fn error_display_layout_constraint() {
        let e = CoreError::LayoutConstraint("too small".into());
        let msg = format!("{e}");
        assert!(msg.contains("too small"));
    }

    #[test]
    fn error_display_unknown_option() {
        let e = CoreError::UnknownOption("foo-bar".into());
        let msg = format!("{e}");
        assert!(msg.contains("foo-bar"));
    }

    #[test]
    fn error_display_type_mismatch() {
        let e = CoreError::OptionTypeMismatch {
            key: "status".into(),
            expected: "bool",
            got: "string",
        };
        let msg = format!("{e}");
        assert!(msg.contains("status"));
        assert!(msg.contains("bool"));
        assert!(msg.contains("string"));
    }

    #[test]
    fn core_result_ok() {
        let r: CoreResult<i32> = Ok(42);
        assert_eq!(r.unwrap(), 42);
    }

    #[test]
    fn core_result_err() {
        let r: CoreResult<i32> = Err(CoreError::InvalidUtf8);
        assert!(r.is_err());
    }
}
