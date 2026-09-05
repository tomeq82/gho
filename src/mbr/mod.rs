//! Master Boot Record partition table parser.
//!
//! The partition table sits in bytes 446..510 of the 512-byte MBR. Each of the
//! four entries is a 16-byte CHS/LBA descriptor. A valid MBR ends with the boot
//! signature `0x55 0xAA` at offsets 510 and 511.

use crate::error::Result;

/// Size of one MBR partition entry.
pub const MBR_ENTRY_SIZE: usize = 16;
/// Number of entries in a standard MBR partition table.
pub const MBR_ENTRY_COUNT: usize = 4;
/// Boot signature byte 0.
pub const MBR_BOOT_SIG_0: u8 = 0x55;
/// Boot signature byte 1.
pub const MBR_BOOT_SIG_1: u8 = 0xAA;

/// One parsed MBR partition entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MbrEntry {
    /// Partition type (0x07 = NTFS, 0x83 = Linux, 0x82 = Linux swap, ...).
    pub part_type: u8,
    /// First LBA sector.
    pub lba_start: u32,
    /// Number of LBA sectors.
    pub lba_size: u32,
}

/// Parse an MBR partition table from a 512-byte (or longer) buffer.
///
/// Returns an empty vector if the boot signature is missing or the buffer is
/// too short.
pub fn parse(mbr: &[u8]) -> Result<Vec<MbrEntry>> {
    if mbr.len() < 512 || mbr[510] != MBR_BOOT_SIG_0 || mbr[511] != MBR_BOOT_SIG_1 {
        return Ok(Vec::new());
    }
    let mut entries = Vec::with_capacity(MBR_ENTRY_COUNT);
    for i in 0..MBR_ENTRY_COUNT {
        let off = 446 + i * MBR_ENTRY_SIZE;
        let part_type = mbr[off + 4];
        if part_type == 0 {
            continue;
        }
        let lba_start =
            u32::from_le_bytes([mbr[off + 8], mbr[off + 9], mbr[off + 10], mbr[off + 11]]);
        let lba_size =
            u32::from_le_bytes([mbr[off + 12], mbr[off + 13], mbr[off + 14], mbr[off + 15]]);
        entries.push(MbrEntry {
            part_type,
            lba_start,
            lba_size,
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_missing_boot_signature() {
        let mbr = vec![0u8; 512];
        let entries = parse(&mbr).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_reads_single_ntfs_entry() {
        let mut mbr = vec![0u8; 512];
        mbr[446 + 4] = 0x07;
        mbr[446 + 8..446 + 12].copy_from_slice(&63u32.to_le_bytes());
        mbr[446 + 12..446 + 16].copy_from_slice(&1000u32.to_le_bytes());
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        let entries = parse(&mbr).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0],
            MbrEntry {
                part_type: 0x07,
                lba_start: 63,
                lba_size: 1000
            }
        );
    }

    #[test]
    fn parse_skips_zero_type_entries() {
        let mut mbr = vec![0u8; 512];
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        let entries = parse(&mbr).unwrap();
        assert!(entries.is_empty());
    }
}
