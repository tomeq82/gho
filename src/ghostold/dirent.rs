//! 56-byte FAT-style directory entry used by pre-11.x Ghost images.

use crate::error::Result;

/// Size of a dirent in bytes.
pub const DIRENT_SIZE: usize = 56;

/// FAT attributes.
pub const ATTR_READ_ONLY: u8 = 0x01;
/// FAT attribute: hidden.
pub const ATTR_HIDDEN: u8 = 0x02;
/// FAT attribute: system.
pub const ATTR_SYSTEM: u8 = 0x04;
/// FAT attribute: volume label.
pub const ATTR_VOLUME_ID: u8 = 0x08;
/// FAT attribute: directory.
pub const ATTR_DIRECTORY: u8 = 0x10;
/// FAT attribute: archive (regular file).
pub const ATTR_ARCHIVE: u8 = 0x20;
/// FAT attribute: VFAT long-name fragment (all four low bits set).
pub const ATTR_LFN: u8 = 0x0F;

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
        let adate = u16::from_le_bytes([buf[18], buf[19]]);
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
            adate,
        })
    }

    /// Render the 8.3 name as a `NAME.EXT` string (trimming spaces and
    /// NUL padding). Real FAT dirents pad with `0x20`, but some images
    /// in the wild use `0x00`, so we trim both. For bytes that are not
    /// valid UTF-8 (a malformed dirent), we use a short `?` fallback so
    /// the result stays within the normal 8.3 length budget.
    pub fn display_name(&self) -> String {
        let trim = |b: &[u8]| -> String {
            std::str::from_utf8(b)
                .unwrap_or("?")
                .trim_end_matches([' ', '\0'])
                .to_string()
        };
        let name = trim(&self.name);
        let ext = trim(&self.ext);
        if ext.is_empty() {
            name
        } else {
            format!("{name}.{ext}")
        }
    }

    /// FAT attribute: directory.
    pub fn is_directory(&self) -> bool {
        self.attrs & ATTR_DIRECTORY != 0
    }

    /// FAT attribute: regular file (archive bit).
    pub fn is_file(&self) -> bool {
        self.attrs & ATTR_ARCHIVE != 0
    }

    /// FAT attribute: VFAT long-name fragment.
    pub fn is_vfat_long(&self) -> bool {
        self.attrs & ATTR_LFN == ATTR_LFN
    }
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

    #[test]
    fn dirent_cluster_combines_hi_lo() {
        let mut buf = [0u8; 56];
        buf[20..22].copy_from_slice(&0x1234u16.to_le_bytes());
        buf[26..28].copy_from_slice(&0x5678u16.to_le_bytes());
        let d = Dirent::parse(&buf).unwrap();
        assert_eq!(d.cluster, 0x1234_5678);
    }
}
