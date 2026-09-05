//! 512-byte Ghost file header (`FEEF` magic).
//!
//! Every `.gho` and `.ghs` file begins with this header. In a spanned image,
//! each physical file has its own header at offset 0; the parser recognises
//! and skips these wherever they appear in the concatenated logical stream.

use crate::error::{Error, Result};

use super::{COMPRESSION_FAST, COMPRESSION_NONE, GHO_MAGIC, HEADER_SIZE};

/// Parsed 512-byte file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileHeader {
    /// 1 = first/single file, 9 = span continuation file.
    pub file_type: u8,
    /// 0 = none, 2 = FastLZ, 3..10 = zlib.
    pub compression: u8,
    /// Shared identifier across all span files of one logical image.
    pub image_id: u32,
    /// True if the encryption flag bit is set.
    pub encrypted: bool,
}

impl FileHeader {
    /// Parse from the first 512 bytes of a `.gho`/`.ghs` file.
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(Error::truncated(0, HEADER_SIZE, data.len()));
        }
        let magic = u16::from_le_bytes([data[0], data[1]]);
        if magic != GHO_MAGIC {
            return Err(Error::format(
                0,
                format!("bad file magic {magic:#06x}, expected {GHO_MAGIC:#06x}"),
            ));
        }
        let encrypted = data.len() > 12 && (data[12] & 0x02) != 0;
        Ok(Self {
            file_type: data[2],
            compression: data[3],
            image_id: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            encrypted,
        })
    }

    /// True if the compression byte indicates FastLZ.
    pub fn is_fastlz(&self) -> bool {
        self.compression == COMPRESSION_FAST
    }

    /// True if the compression byte indicates no compression.
    pub fn is_uncompressed(&self) -> bool {
        self.compression == COMPRESSION_NONE
    }

    /// True if this header starts a fresh logical image (not a continuation).
    pub fn is_first(&self) -> bool {
        self.file_type == 1
    }

    /// True if this header belongs to a continuation span file.
    pub fn is_continuation(&self) -> bool {
        self.file_type == 9
    }
}

/// Check whether the bytes at `buf[off..]` look like an embedded 512-byte
/// file header. Used by the stream parser to skip span-continuation headers.
pub fn looks_like_at(buf: &[u8], off: usize) -> bool {
    if buf.len() < off + HEADER_SIZE {
        return false;
    }
    let magic = u16::from_le_bytes([buf[off], buf[off + 1]]);
    magic == GHO_MAGIC && (buf[off + 2] == 1 || buf[off + 2] == 9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_parse_roundtrip() {
        let mut buf = [0u8; 512];
        buf[0] = 0xFE;
        buf[1] = 0xEF;
        buf[2] = 1;
        buf[3] = 2;
        buf[4..8].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        let hdr = FileHeader::parse(&buf).unwrap();
        assert_eq!(hdr.file_type, 1);
        assert_eq!(hdr.compression, 2);
        assert_eq!(hdr.image_id, 0x1234_5678);
        assert!(!hdr.encrypted);
        assert!(hdr.is_first());
        assert!(!hdr.is_continuation());
        assert!(hdr.is_fastlz());
        assert!(!hdr.is_uncompressed());
    }

    #[test]
    fn header_detects_encryption() {
        let mut buf = [0u8; 512];
        buf[0] = 0xFE;
        buf[1] = 0xEF;
        buf[12] = 0x02;
        let hdr = FileHeader::parse(&buf).unwrap();
        assert!(hdr.encrypted);
    }

    #[test]
    fn header_rejects_bad_magic() {
        let buf = [0u8; 512];
        let err = FileHeader::parse(&buf).unwrap_err();
        assert!(matches!(err, Error::Format { .. }));
    }

    #[test]
    fn header_rejects_truncated() {
        let err = FileHeader::parse(&[0xFE, 0xEF]).unwrap_err();
        assert!(matches!(err, Error::Truncated { .. }));
    }

    #[test]
    fn looks_like_at_with_continuation() {
        let mut buf = vec![0u8; 1024];
        buf[0] = 0xFE;
        buf[1] = 0xEF;
        buf[2] = 9;
        assert!(looks_like_at(&buf, 0));
        assert!(!looks_like_at(&buf, 600));
    }

    #[test]
    fn looks_like_at_rejects_short_buffer() {
        let buf = [0u8; 100];
        assert!(!looks_like_at(&buf, 0));
    }
}
