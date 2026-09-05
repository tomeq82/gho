//! Norton Ghost 11.x / 12.x image parser.
//!
//! Format layout (see `docs/FORMAT.md`):
//! - 512-byte file header (`FEEF` magic).
//! - Stream of 10-byte record headers followed by `body_len` body bytes.
//! - Record types: Track0 (`0x0006`), Partition (`0x0603`), Continuation
//!   (`0x0703`), End (`0x0023`).
//! - Between records, embedded compressed blocks (2-byte `stored_len` +
//!   payload).
//!
//! Block payload is either stored uncompressed (`compression == 0`), FastLZ
//! (`compression == 2`), or zlib (`compression ∈ [3, 10)`).

pub mod header;
pub mod record;
pub mod stream;

pub use header::FileHeader;
pub use record::{Record, RecordType, KNOWN_RECORD_TYPES, RECORD_HEADER_SIZE, RECORD_MAGIC};
pub use stream::{extract, ExtractResult, PartitionSummary};

use crate::error::Result;

/// `FEEF` magic at the start of every `.gho` / `.ghs` file.
pub const GHO_MAGIC: u16 = 0xEFFE;

/// Size of a single Ghost file header in bytes.
pub const HEADER_SIZE: usize = 512;

/// No compression (blocks are stored verbatim).
pub const COMPRESSION_NONE: u8 = 0;
/// FastLZ (Z1) compression.
pub const COMPRESSION_FAST: u8 = 2;

/// Decoded block-level compression type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    FastLz,
    Zlib,
}

impl Compression {
    pub fn from_byte(b: u8) -> Result<Self> {
        match b {
            COMPRESSION_NONE => Ok(Self::None),
            COMPRESSION_FAST => Ok(Self::FastLz),
            3..=9 => Ok(Self::Zlib),
            other => Err(crate::error::Error::UnsupportedCompression(other)),
        }
    }
}

/// Look at `buf[off..]` and decide whether a known record header starts here.
///
/// Returns the decoded `Record` (with body length) if so, or `None` if the
/// bytes at `off` do not match a record header.
pub fn looks_like_record(buf: &[u8], off: usize) -> Option<Record> {
    Record::parse_at(buf, off)
}

/// Look at `buf[off..]` and decide whether a 512-byte embedded file header
/// starts here.
pub fn looks_like_embedded_file_header(buf: &[u8], off: usize) -> bool {
    header::looks_like_at(buf, off)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_from_byte() {
        assert_eq!(Compression::from_byte(0).unwrap(), Compression::None);
        assert_eq!(Compression::from_byte(2).unwrap(), Compression::FastLz);
        assert_eq!(Compression::from_byte(3).unwrap(), Compression::Zlib);
        assert_eq!(Compression::from_byte(9).unwrap(), Compression::Zlib);
        assert!(Compression::from_byte(11).is_err());
        assert!(Compression::from_byte(255).is_err());
    }
}
