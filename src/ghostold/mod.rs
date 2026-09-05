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

use crate::error::Result;

/// `FEEF` magic at the start of every `.gho` / `.ghs` file.
pub const GHO_MAGIC: u16 = 0xEFFE;

/// Size of a single Ghost file header in bytes.
pub const HEADER_SIZE: usize = 512;

/// Size of a record header in bytes.
pub const RECORD_HEADER_SIZE: usize = 10;

/// Magic number embedded in every record header (little-endian).
pub const RECORD_MAGIC: u32 = 0x012F_18D8;

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

/// Size of a dirent in bytes.
pub const DIRENT_SIZE: usize = 56;

/// One parsed directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirent {
    /// 8-character name (space-padded).
    pub name: [u8; 8],
    /// 3-character extension (space-padded).
    pub ext: [u8; 3],
    /// FAT-style attributes.
    pub attrs: u8,
    /// File size in bytes (0 for directories).
    pub size: u32,
    /// First cluster (combined from cluster_hi / cluster_lo halves).
    pub cluster: u32,
    /// 16-bit FAT creation time.
    pub ctime: u16,
    /// 16-bit FAT creation date.
    pub cdate: u16,
    /// 16-bit FAT modification time.
    pub mtime: u16,
    /// 16-bit FAT modification date.
    pub mdate: u16,
    /// Last-access date.
    pub adate: u16,
}

impl Dirent {
    pub fn parse(buf: &[u8]) -> Result<Self> {
        if buf.len() < DIRENT_SIZE {
            return Err(crate::error::Error::truncated(0, DIRENT_SIZE, buf.len()));
        }
        let mut name = [0u8; 8];
        let mut ext = [0u8; 3];
        name.copy_from_slice(&buf[0..8]);
        ext.copy_from_slice(&buf[8..11]);
        let attrs = buf[11];
        let ctime = u16::from_le_bytes([buf[14], buf[15]]);
        let cdate = u16::from_le_bytes([buf[16], buf[17]]);
        let cluster_hi = u16::from_le_bytes([buf[20], buf[21]]);
        let mtime = u16::from_le_bytes([buf[22], buf[23]]);
        let mdate = u16::from_le_bytes([buf[24], buf[25]]);
        let cluster_lo = u16::from_le_bytes([buf[26], buf[27]]);
        let size = u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]);
        let cluster = (u32::from(cluster_hi) << 16) | u32::from(cluster_lo);
        Ok(Self {
            name,
            ext,
            attrs,
            size,
            cluster,
            ctime,
            cdate,
            mtime,
            mdate,
            adate: u16::from_le_bytes([buf[18], buf[19]]),
        })
    }

    /// Render the 8.3 name as a `NAME.EXT` string (trimming spaces).
    pub fn display_name(&self) -> String {
        let name = std::str::from_utf8(&self.name)
            .unwrap_or("????????")
            .trim_end();
        let ext = std::str::from_utf8(&self.ext)
            .unwrap_or("???")
            .trim_end();
        if ext.is_empty() {
            name.to_string()
        } else {
            format!("{name}.{ext}")
        }
    }

    /// FAT attribute: directory.
    pub fn is_directory(&self) -> bool {
        self.attrs & 0x10 != 0
    }

    /// FAT attribute: regular file (archive bit).
    pub fn is_file(&self) -> bool {
        self.attrs & 0x20 != 0
    }

    /// FAT attribute: VFAT long-name fragment.
    pub fn is_vfat_long(&self) -> bool {
        self.attrs & 0x0F == 0x0F
    }
}

/// Result of parsing a pre-11.x image.
#[derive(Debug, Default)]
pub struct ParsedImage {
    pub dirents: Vec<Dirent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirent_parse_ntfs_archive() {
        let mut buf = [0u8; 56];
        buf[0..8].copy_from_slice(b"SETUPEXE");
        buf[8..11].copy_from_slice(b"   ");
        buf[11] = 0x20; // archive
        buf[28..32].copy_from_slice(&180_224u32.to_le_bytes());
        let d = Dirent::parse(&buf).unwrap();
        assert_eq!(d.name, *b"SETUPEXE");
        assert_eq!(d.ext, *b"   ");
        assert_eq!(d.size, 180_224);
        assert!(d.is_file());
        assert!(!d.is_directory());
        assert_eq!(d.display_name(), "SETUPEXE");
    }

    #[test]
    fn dirent_parse_with_extension() {
        let mut buf = [0u8; 56];
        buf[0..8].copy_from_slice(b"GG      ");
        buf[8..11].copy_from_slice(b"EXE");
        buf[11] = 0x20;
        buf[28..32].copy_from_slice(&622_592u32.to_le_bytes());
        let d = Dirent::parse(&buf).unwrap();
        assert_eq!(d.display_name(), "GG.EXE");
        assert_eq!(d.size, 622_592);
    }

    #[test]
    fn dirent_directory_attr() {
        let mut buf = [0u8; 56];
        buf[0..8].copy_from_slice(b"WINDOWS ");
        buf[11] = 0x10;
        let d = Dirent::parse(&buf).unwrap();
        assert!(d.is_directory());
        assert!(!d.is_file());
    }

    #[test]
    fn dirent_vfat_long_attr() {
        let mut buf = [0u8; 56];
        buf[11] = 0x0F;
        let d = Dirent::parse(&buf).unwrap();
        assert!(d.is_vfat_long());
    }
}
