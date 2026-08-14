//! `tzcraft` error model.
//!
//! One `Copy` error type: no heap, no external error crates. Parse failures
//! optionally carry the byte offset where parsing stopped, so callers can
//! point at the offending character without building a `String`.

use core::fmt;

/// Coarse classification of a [`Error`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A component (month, day, hour, ...) fell outside its allowed range.
    OutOfRange(&'static str),
    /// Text could not be parsed.
    Parse(&'static str),
    /// Signed arithmetic overflowed.
    Overflow,
    /// A timezone offset outside ±24 hours was requested.
    InvalidOffset,
    /// Generic invalid input.
    Invalid(&'static str),
}

/// A `tzcraft` error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    offset: Option<usize>,
}

impl Error {
    /// Create an error without position information.
    pub const fn new(kind: ErrorKind) -> Error {
        Error { kind, offset: None }
    }

    /// A component was out of range.
    pub const fn out_of_range(component: &'static str) -> Error {
        Error::new(ErrorKind::OutOfRange(component))
    }

    /// A parse failure at byte `offset`.
    pub const fn parse(what: &'static str, offset: usize) -> Error {
        Error {
            kind: ErrorKind::Parse(what),
            offset: Some(offset),
        }
    }

    /// Signed arithmetic overflow.
    pub const fn overflow() -> Error {
        Error::new(ErrorKind::Overflow)
    }

    /// An offset outside ±24 hours was requested.
    pub const fn invalid_offset() -> Error {
        Error::new(ErrorKind::InvalidOffset)
    }

    /// Generic invalid input.
    pub const fn invalid(what: &'static str) -> Error {
        Error::new(ErrorKind::Invalid(what))
    }

    /// The error kind.
    pub const fn kind(self) -> ErrorKind {
        self.kind
    }

    /// The byte offset for parse errors.
    pub const fn offset(self) -> Option<usize> {
        self.offset
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ErrorKind::OutOfRange(component) => write!(f, "{component} is out of range"),
            ErrorKind::Parse(what) => write!(f, "parse error: {what}"),
            ErrorKind::Overflow => write!(f, "arithmetic overflow"),
            ErrorKind::InvalidOffset => write!(f, "timezone offset must be within ±24 hours"),
            ErrorKind::Invalid(what) => write!(f, "invalid input: {what}"),
        }?;
        if let Some(offset) = self.offset {
            write!(f, " at byte {offset}")?;
        }
        Ok(())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

/// Convenience alias for `tzcraft` operations.
pub type Result<T> = core::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn display_carries_context() {
        assert_eq!(
            Error::out_of_range("month").to_string(),
            "month is out of range"
        );
        assert_eq!(
            Error::parse("expected digits", 7).to_string(),
            "parse error: expected digits at byte 7"
        );
        assert!(Error::overflow().to_string().contains("overflow"));
        assert_eq!(Error::invalid_offset().kind(), ErrorKind::InvalidOffset);
    }
}
