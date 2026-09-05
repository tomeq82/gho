//! Error types for the `gho` crate.

use std::io;
use thiserror::Error;

/// Result alias for `gho` operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by `gho` parsing and extraction.
#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("format error at offset {offset:#x}: {message}")]
    Format { offset: u64, message: String },

    #[error("encrypted GHO image is not supported")]
    Encrypted,

    #[error("unsupported compression type {0}")]
    UnsupportedCompression(u8),

    #[error("truncated input: expected {expected} bytes, got {actual} at offset {offset:#x}")]
    Truncated {
        offset: u64,
        expected: usize,
        actual: usize,
    },

    #[error("invalid FastLZ block at offset {offset:#x}: {message}")]
    FastLz { offset: u64, message: String },
}

impl Error {
    pub fn format(offset: u64, message: impl Into<String>) -> Self {
        Self::Format {
            offset,
            message: message.into(),
        }
    }

    pub fn truncated(offset: u64, expected: usize, actual: usize) -> Self {
        Self::Truncated {
            offset,
            expected,
            actual,
        }
    }

    pub fn fastlz(offset: u64, message: impl Into<String>) -> Self {
        Self::FastLz {
            offset,
            message: message.into(),
        }
    }
}
