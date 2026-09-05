//! Norton Ghost pre-11.x image parser.
//!
//! Format layout (see `docs/FORMAT_OLD.md`):
//! - 512-byte file header (`FEEF` magic), same shape as 11.x.
//! - Stream of records with the same 10-byte header layout, but different
//!   type codes (e.g. `0x2c17`, `0x2c04`, `0x0104`, `0x0002`, `0x0102`,
//!   `0x0103`, `0x0118`, `0x0117`).
//! - Data is a flat directory of FAT-style 56-byte dirents with 8.3 names
//!   and cluster/size fields.
//!
//! Span boundaries in this format can land **inside** a compressed block.
//! Pre-strip the embedded 512-byte file headers at known offsets before
//! handing the result to the parser.

pub mod dirent;
pub mod record;
pub mod stream;

pub use dirent::Dirent;
pub use record::{
    KNOWN_RECORD_TYPES, RECORD_DATA_FULL, RECORD_DATA_LAST, RECORD_DATA_TRAILER,
    RECORD_DIRENT, RECORD_FIRST_DIRENT, RECORD_HEADER_SIZE, RECORD_MAGIC,
    RECORD_PART2_BOOT, RECORD_PART2_TABLE, RECORD_TYPE_BOOT_HMR, Record, RecordType,
};
pub use stream::{extract_file, walk_dirents, WalkedEntry};

/// `FEEF` magic at the start of every `.gho` / `.ghs` file.
pub const GHO_MAGIC: u16 = 0xEFFE;

/// Size of a single Ghost file header in bytes.
pub const HEADER_SIZE: usize = 512;

/// The fixed 32 KiB size of a full FastLZ data block in pre-11.x format.
pub const DATA_FULL_BLOCK_SIZE: usize = 32 * 1024;
