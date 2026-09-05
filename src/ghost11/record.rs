//! 10-byte record header for Ghost 11.x/12.x streams.
//!
//! Each record has the layout:
//! - bytes 0..2: type code (little-endian u16)
//! - bytes 2..4: unknown padding (typically zero)
//! - bytes 4..8: magic `0x012F18D8` (little-endian u32)
//! - bytes 8..10: body length (little-endian u16)
//!
//! Followed by `body_len` bytes of payload.

use crate::error::{Error, Result};

/// Magic number embedded in every record header (little-endian).
pub const RECORD_MAGIC: u32 = 0x012F_18D8;

/// Size of a record header in bytes.
pub const RECORD_HEADER_SIZE: usize = 10;

/// Track0 record — appears once at the start; body starts with a 6-byte mini
/// header and (optionally) a 512-byte MBR.
pub const RECORD_TYPE_TRACK0: u16 = 0x0006;
/// Partition record — marks the start of a new partition payload.
pub const RECORD_TYPE_PARTITION: u16 = 0x0603;
/// Continuation record — links a spanned image to the next physical file.
pub const RECORD_TYPE_CONTINUATION: u16 = 0x0703;
/// End record — terminates the image stream.
pub const RECORD_TYPE_END: u16 = 0x0023;

/// All known record type codes.
pub const KNOWN_RECORD_TYPES: &[u16] = &[
    RECORD_TYPE_TRACK0,
    RECORD_TYPE_PARTITION,
    RECORD_TYPE_CONTINUATION,
    RECORD_TYPE_END,
];

/// One decoded record header (the body is owned by the caller).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub kind: RecordType,
    pub body_len: u16,
}

/// The type code of a parsed record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordType {
    Track0,
    Partition,
    Continuation,
    End,
}

impl RecordType {
    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
            RECORD_TYPE_TRACK0 => Some(Self::Track0),
            RECORD_TYPE_PARTITION => Some(Self::Partition),
            RECORD_TYPE_CONTINUATION => Some(Self::Continuation),
            RECORD_TYPE_END => Some(Self::End),
            _ => None,
        }
    }
}

impl Record {
    /// Try to parse a record header at `buf[off..]`. Returns `None` if the
    /// bytes do not match a known record header (e.g. they are mid-block data).
    pub fn parse_at(buf: &[u8], off: usize) -> Option<Self> {
        if buf.len() < off + RECORD_HEADER_SIZE {
            return None;
        }
        let type_code = u16::from_le_bytes([buf[off], buf[off + 1]]);
        let kind = RecordType::from_u16(type_code)?;
        let magic = u32::from_le_bytes([buf[off + 4], buf[off + 5], buf[off + 6], buf[off + 7]]);
        if magic != RECORD_MAGIC {
            return None;
        }
        let body_len = u16::from_le_bytes([buf[off + 8], buf[off + 9]]);
        Some(Self { kind, body_len })
    }

    /// Strict parse: returns an error if the bytes at `off` are not a record.
    pub fn parse_strict(buf: &[u8], off: usize) -> Result<Self> {
        Self::parse_at(buf, off).ok_or_else(|| {
            Error::format(off as u64, "expected record header, found non-matching bytes")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_record(type_code: u16, body_len: u16) -> [u8; RECORD_HEADER_SIZE] {
        let mut r = [0u8; RECORD_HEADER_SIZE];
        r[0..2].copy_from_slice(&type_code.to_le_bytes());
        // bytes 2..4 left as zero
        r[4..8].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
        r[8..10].copy_from_slice(&body_len.to_le_bytes());
        r
    }

    #[test]
    fn parse_track0() {
        let r = build_record(RECORD_TYPE_TRACK0, 6 + 512);
        let rec = Record::parse_at(&r, 0).unwrap();
        assert_eq!(rec.kind, RecordType::Track0);
        assert_eq!(rec.body_len, 518);
    }

    #[test]
    fn parse_partition() {
        let r = build_record(RECORD_TYPE_PARTITION, 20);
        let rec = Record::parse_at(&r, 0).unwrap();
        assert_eq!(rec.kind, RecordType::Partition);
        assert_eq!(rec.body_len, 20);
    }

    #[test]
    fn parse_end() {
        let r = build_record(RECORD_TYPE_END, 24);
        let rec = Record::parse_at(&r, 0).unwrap();
        assert_eq!(rec.kind, RecordType::End);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut r = build_record(RECORD_TYPE_TRACK0, 6);
        r[4] = 0xFF;
        r[5] = 0xFF;
        assert!(Record::parse_at(&r, 0).is_none());
    }

    #[test]
    fn rejects_unknown_type() {
        let r = build_record(0x9999, 0);
        assert!(Record::parse_at(&r, 0).is_none());
    }

    #[test]
    fn rejects_truncated() {
        let r = [0u8; 5];
        assert!(Record::parse_at(&r, 0).is_none());
    }
}
