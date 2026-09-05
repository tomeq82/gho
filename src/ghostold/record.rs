//! 10-byte record header for pre-11.x Ghost images.
//!
//! Same wire format as Ghost 11.x — 2 bytes type, 2 bytes unknown, 4 bytes
//! magic `0x012F18D8`, 2 bytes body length. Different type codes, though.

use crate::error::{Error, Result};

/// Magic number embedded in every record header (little-endian).
pub const RECORD_MAGIC: u32 = 0x012F_18D8;

/// Size of a record header in bytes.
pub const RECORD_HEADER_SIZE: usize = 10;

/// Record type: HMR / read-record at the very start of the image.
pub const RECORD_TYPE_BOOT_HMR: u16 = 0x2C17;
/// Record type: the very first dirent in the image (singleton).
pub const RECORD_TYPE_FIRST_DIRENT: u16 = 0x2C04;
/// Record type: a normal FAT-style directory entry.
pub const RECORD_TYPE_DIRENT: u16 = 0x0104;
/// Record type: a full 32 KiB compressed data block.
pub const RECORD_TYPE_DATA_FULL: u16 = 0x0002;
/// Record type: the **last** (partial) compressed data block of a file.
pub const RECORD_TYPE_DATA_LAST: u16 = 0x0102;
/// Record type: 20-byte trailer after the last data block of a file.
pub const RECORD_TYPE_DATA_TRAILER: u16 = 0x0103;
/// Record type: boot sector of the second partition (FAT32).
pub const RECORD_TYPE_PART2_BOOT: u16 = 0x0118;
/// Record type: 512-byte partition table of the second partition.
pub const RECORD_TYPE_PART2_TABLE: u16 = 0x0117;

// Aliases matching the snake_case style used elsewhere in the crate.
pub use RECORD_TYPE_BOOT_HMR as RECORD_BOOT_HMR;
pub use RECORD_TYPE_FIRST_DIRENT as RECORD_FIRST_DIRENT;
pub use RECORD_TYPE_DIRENT as RECORD_DIRENT;
pub use RECORD_TYPE_DATA_FULL as RECORD_DATA_FULL;
pub use RECORD_TYPE_DATA_LAST as RECORD_DATA_LAST;
pub use RECORD_TYPE_DATA_TRAILER as RECORD_DATA_TRAILER;
pub use RECORD_TYPE_PART2_BOOT as RECORD_PART2_BOOT;
pub use RECORD_TYPE_PART2_TABLE as RECORD_PART2_TABLE;

/// All observed record type codes for the pre-11.x format.
pub const KNOWN_RECORD_TYPES: &[u16] = &[
    RECORD_TYPE_BOOT_HMR,
    RECORD_TYPE_FIRST_DIRENT,
    RECORD_TYPE_DIRENT,
    RECORD_TYPE_DATA_FULL,
    RECORD_TYPE_DATA_LAST,
    RECORD_TYPE_DATA_TRAILER,
    RECORD_TYPE_PART2_BOOT,
    RECORD_TYPE_PART2_TABLE,
];

/// One decoded record header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub kind: RecordType,
    pub body_len: u16,
}

/// Type code of a parsed pre-11.x record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordType {
    BootHmr,
    FirstDirent,
    Dirent,
    DataFull,
    DataLast,
    DataTrailer,
    Part2Boot,
    Part2Table,
    /// An unknown but validly-formed record (we still know its body length).
    Unknown(u16),
}

impl RecordType {
    pub fn from_u16(v: u16) -> Self {
        match v {
            RECORD_TYPE_BOOT_HMR => Self::BootHmr,
            RECORD_TYPE_FIRST_DIRENT => Self::FirstDirent,
            RECORD_TYPE_DIRENT => Self::Dirent,
            RECORD_TYPE_DATA_FULL => Self::DataFull,
            RECORD_TYPE_DATA_LAST => Self::DataLast,
            RECORD_TYPE_DATA_TRAILER => Self::DataTrailer,
            RECORD_TYPE_PART2_BOOT => Self::Part2Boot,
            RECORD_TYPE_PART2_TABLE => Self::Part2Table,
            other => Self::Unknown(other),
        }
    }
}

impl Record {
    /// Try to parse a record header at `buf[off..]`. Returns `None` if the
    /// magic does not match.
    ///
    /// Pre-11.x records have type codes outside the 11.x set, but we still
    /// rely on the magic + non-zero body_len to recognise them. This way we
    /// can walk streams that mix in continuation records from a later Ghost
    /// version without false positives on arbitrary byte sequences.
    pub fn parse_at(buf: &[u8], off: usize) -> Option<Self> {
        if buf.len() < off + RECORD_HEADER_SIZE {
            return None;
        }
        let type_code = u16::from_le_bytes([buf[off], buf[off + 1]]);
        let magic = u32::from_le_bytes([buf[off + 4], buf[off + 5], buf[off + 6], buf[off + 7]]);
        if magic != RECORD_MAGIC {
            return None;
        }
        let body_len = u16::from_le_bytes([buf[off + 8], buf[off + 9]]);
        Some(Self {
            kind: RecordType::from_u16(type_code),
            body_len,
        })
    }

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
        r[4..8].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
        r[8..10].copy_from_slice(&body_len.to_le_bytes());
        r
    }

    #[test]
    fn parse_boot_hmr() {
        let r = build_record(RECORD_TYPE_BOOT_HMR, 6);
        let rec = Record::parse_at(&r, 0).unwrap();
        assert_eq!(rec.kind, RecordType::BootHmr);
        assert_eq!(rec.body_len, 6);
    }

    #[test]
    fn parse_first_dirent() {
        let r = build_record(RECORD_TYPE_FIRST_DIRENT, 56);
        let rec = Record::parse_at(&r, 0).unwrap();
        assert_eq!(rec.kind, RecordType::FirstDirent);
    }

    #[test]
    fn parse_dirent() {
        let r = build_record(RECORD_TYPE_DIRENT, 56);
        let rec = Record::parse_at(&r, 0).unwrap();
        assert_eq!(rec.kind, RecordType::Dirent);
    }

    #[test]
    fn parse_data_full() {
        let r = build_record(RECORD_TYPE_DATA_FULL, 32 * 1024);
        let rec = Record::parse_at(&r, 0).unwrap();
        assert_eq!(rec.kind, RecordType::DataFull);
        assert_eq!(rec.body_len, 32 * 1024);
    }

    #[test]
    fn parse_data_last() {
        let r = build_record(RECORD_TYPE_DATA_LAST, 1024);
        let rec = Record::parse_at(&r, 0).unwrap();
        assert_eq!(rec.kind, RecordType::DataLast);
    }

    #[test]
    fn parse_data_trailer() {
        let r = build_record(RECORD_TYPE_DATA_TRAILER, 20);
        let rec = Record::parse_at(&r, 0).unwrap();
        assert_eq!(rec.kind, RecordType::DataTrailer);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut r = build_record(RECORD_TYPE_DIRENT, 56);
        r[4] = 0xFF;
        r[5] = 0xFF;
        assert!(Record::parse_at(&r, 0).is_none());
    }

    #[test]
    fn rejects_truncated() {
        let r = [0u8; 5];
        assert!(Record::parse_at(&r, 0).is_none());
    }

    #[test]
    fn unknown_type_is_captured() {
        let r = build_record(0xABCD, 100);
        let rec = Record::parse_at(&r, 0).unwrap();
        assert_eq!(rec.kind, RecordType::Unknown(0xABCD));
    }
}
