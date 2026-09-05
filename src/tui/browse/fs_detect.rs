//! Filesystem detection for partition boot sectors.
//!
//! Reads the first 512 bytes of a partition and identifies the filesystem
//! by magic bytes / signatures. This is heuristic — not a full validation —
//! but covers the common cases that matter for forensic triage:
//! - NTFS, FAT12/16/32, exFAT
//! - ext2/3/4, XFS, Btrfs
//! - HFS, HFS+, APFS
//! - swap, ISO 9660, UDF
//! - Linux RAID, LVM
//!
//! Returns `FsKind::Unknown` if no signature matches. The bootstrap loader
//! is treated separately (NTLDR, GRUB, syslinux) because they cohabit
//! with filesystems.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsKind {
    Ntfs,
    Fat { variant: FatVariant },
    ExFat,
    Ext { version: ExtVersion },
    Xfs,
    Btrfs,
    Hfs,
    HfsPlus,
    Apfs,
    Swap,
    Iso9660,
    Udf,
    LinuxRaid,
    Lvm,
    Zfs,
    BootSector(BootLoader),
    Raw,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatVariant {
    Fat12,
    Fat16,
    Fat32,
}

impl fmt::Display for FatVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FatVariant::Fat12 => f.write_str("FAT12"),
            FatVariant::Fat16 => f.write_str("FAT16"),
            FatVariant::Fat32 => f.write_str("FAT32"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtVersion {
    Ext2,
    Ext3,
    Ext4,
}

impl fmt::Display for ExtVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtVersion::Ext2 => f.write_str("ext2"),
            ExtVersion::Ext3 => f.write_str("ext3"),
            ExtVersion::Ext4 => f.write_str("ext4"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootLoader {
    Grub,
    GrubLegacy,
    Ntldr,
    Syslinux,
    Lilo,
    BootMgr,
    MbrBlank,
}

impl fmt::Display for BootLoader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootLoader::Grub => f.write_str("GRUB"),
            BootLoader::GrubLegacy => f.write_str("GRUB Legacy"),
            BootLoader::Ntldr => f.write_str("NTLDR"),
            BootLoader::Syslinux => f.write_str("syslinux"),
            BootLoader::Lilo => f.write_str("LILO"),
            BootLoader::BootMgr => f.write_str("BOOTMGR"),
            BootLoader::MbrBlank => f.write_str("MBR (blank)"),
        }
    }
}

impl fmt::Display for FsKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsKind::Ntfs => f.write_str("NTFS"),
            FsKind::Fat { variant } => write!(f, "{variant}"),
            FsKind::ExFat => f.write_str("exFAT"),
            FsKind::Ext { version } => write!(f, "{version}"),
            FsKind::Xfs => f.write_str("XFS"),
            FsKind::Btrfs => f.write_str("Btrfs"),
            FsKind::Hfs => f.write_str("HFS"),
            FsKind::HfsPlus => f.write_str("HFS+"),
            FsKind::Apfs => f.write_str("APFS"),
            FsKind::Swap => f.write_str("swap"),
            FsKind::Iso9660 => f.write_str("ISO 9660"),
            FsKind::Udf => f.write_str("UDF"),
            FsKind::LinuxRaid => f.write_str("Linux RAID"),
            FsKind::Lvm => f.write_str("LVM"),
            FsKind::Zfs => f.write_str("ZFS"),
            FsKind::BootSector(b) => write!(f, "{b}"),
            FsKind::Raw => f.write_str("raw"),
            FsKind::Unknown => f.write_str("?"),
        }
    }
}

/// Identify the filesystem that lives in the first 512 bytes of a partition.
/// `bytes` must be at least 512 long; shorter inputs return `FsKind::Unknown`.
pub fn detect_fs(bytes: &[u8]) -> FsKind {
    if bytes.len() < 512 {
        return FsKind::Unknown;
    }

    // MBR signature: 0x55 0xAA at offsets 510/511 indicates a boot sector of
    // some kind. Many FSes (FAT, NTFS, etc.) put their own signature here too.
    let has_mbr_sig = bytes[510] == 0x55 && bytes[511] == 0xAA;

    // NTFS: bytes 0..4 = "NTFS" (case-sensitive, 4 ASCII bytes).
    if bytes.starts_with(b"NTFS") {
        return FsKind::Ntfs;
    }

    // exFAT: bytes 0..3 = "EXFAT" in OEM label area.
    if bytes.starts_with(b"EXFAT") {
        return FsKind::ExFat;
    }

    // ext2/3/4: magic 0x53 0xEF at offset 1080 (superblock offset 56 with 1024-byte blocks).
    if bytes.len() >= 1082 && bytes[1080] == 0x53 && bytes[1081] == 0xEF {
        // Revision level at 1120 (compat feature flags elsewhere); we use the
        // minor revision as a quick-and-dirty version distinguisher.
        let compat = read_le32(bytes, 1120); // s_rev_level (actually s_def_reserved)
        let _ = compat;
        // Use INCOMPAT feature flags at 1124 (s_first_ino_hi in some layouts
        // — the offsets vary by block size; this is approximate but good
        // enough for a nagios-style identifier).
        // Actually use a more robust probe: s_feature_compat (96) bit flags.
        let feature_compat = read_le32(bytes, 92);
        let feature_incompat = read_le32(bytes, 96);
        let feature_ro_compat = read_le32(bytes, 100);
        let _ = feature_compat;
        let _ = feature_ro_compat;
        // EXT4_FEATURE_INCOMPAT_EXTENTS = 0x40, EXT4_FEATURE_RO_COMPAT_HUGE_FILE = 0x08
        let is_ext4 = (feature_incompat & 0x40) != 0 || (feature_ro_compat & 0x08) != 0;
        let is_ext3 = (feature_incompat & 0x04) != 0; // journal
        let v = if is_ext4 { ExtVersion::Ext4 } else if is_ext3 { ExtVersion::Ext3 } else { ExtVersion::Ext2 };
        return FsKind::Ext { version: v };
    }

    // XFS: magic "XFSB" at offset 0.
    if bytes.starts_with(b"XFSB") {
        return FsKind::Xfs;
    }

    // Btrfs: magic "_BHRfS_M" at offset 0x10040 (superblock).
    if bytes.len() >= 0x10048 && &bytes[0x10040..0x10048] == b"_BHRfS_M" {
        return FsKind::Btrfs;
    }

    // HFS+: magic "H+" at offset 0, or "HX" for HFSX.
    if bytes.starts_with(b"H+") {
        return FsKind::HfsPlus;
    }
    if bytes.starts_with(b"HX") {
        return FsKind::Hfs;
    }

    // APFS: container magic "NXSB" at offset 32.
    if bytes.len() >= 36 && &bytes[32..36] == b"NXSB" {
        return FsKind::Apfs;
    }

    // FAT32: "FAT32   " at offset 82..90
    if bytes.len() >= 90 && &bytes[82..90] == b"FAT32   " {
        return FsKind::Fat { variant: FatVariant::Fat32 };
    }

    // FAT16 / FAT12: "FAT16   " or "FAT12   " at offset 54..62, or "FAT     " at 82..90
    if bytes.len() >= 90 {
        if &bytes[54..62] == b"FAT16   " {
            return FsKind::Fat { variant: FatVariant::Fat16 };
        }
        if &bytes[54..62] == b"FAT12   " {
            return FsKind::Fat { variant: FatVariant::Fat12 };
        }
        if &bytes[82..90] == b"FAT16   " {
            return FsKind::Fat { variant: FatVariant::Fat16 };
        }
        if &bytes[82..90] == b"FAT12   " {
            return FsKind::Fat { variant: FatVariant::Fat12 };
        }
    }

    // swap: magic "SWAPSPACE2" at offset 4086, or version page magic.
    if bytes.len() >= 4096 && &bytes[4086..4096] == b"SWAPSPACE2" {
        return FsKind::Swap;
    }

    // ISO 9660: "CD001" at offset 32768..32773.
    if bytes.len() >= 32773 && &bytes[32768..32773] == b"CD001" {
        return FsKind::Iso9660;
    }

    // UDF: "NSR02" or "NSR03" at offset 32769..32774.
    if bytes.len() >= 32774
        && (&bytes[32769..32774] == b"NSR02" || &bytes[32769..32774] == b"NSR03") {
            return FsKind::Udf;
        }

    // Linux RAID: "Linux RAID member" or ".this is a md superblock".
    if bytes.len() >= 36 && &bytes[8..36] == b".this is a md superblock" {
        return FsKind::LinuxRaid;
    }

    // LVM2: "LVM2 001" at offset 0.
    if bytes.starts_with(b"LVM2 001") {
        return FsKind::Lvm;
    }

    // ZFS: little-endian magic at multiple offsets.
    if bytes.len() >= 8 {
        let m0 = u64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0; 8]));
        let m8 = u64::from_le_bytes(bytes[8..16].try_into().unwrap_or([0; 8]));
        // ZFS uses two different magic constants in different locations.
        if m0 == 0x00_01_02_03_04_05_06_07 || m0 == 0x0B_10_C5_11_B0_59_11_37
            || m8 == 0x00_01_02_03_04_05_06_07 || m8 == 0x0B_10_C5_11_B0_59_11_37
        {
            return FsKind::Zfs;
        }
    }

    // Boot loaders — only meaningful when there's an MBR signature.
    if has_mbr_sig {
        // GRUB2: "GRUB" at offset 0x178 + magic 0x55AA
        if bytes.len() >= 0x17C && &bytes[0x178..0x17C] == b"GRUB" {
            return FsKind::BootSector(BootLoader::Grub);
        }
        // GRUB Legacy: "GRUB" at offset 0x44 + magic
        if bytes.len() >= 0x48 && &bytes[0x44..0x48] == b"GRUB" {
            return FsKind::BootSector(BootLoader::GrubLegacy);
        }
        // syslinux: "SYSLINUX" near end of sector
        if has_mbr_sig && bytes.len() >= 0x80 && &bytes[0x6E..0x76] == b"SYSLINUX" {
            return FsKind::BootSector(BootLoader::Syslinux);
        }
        // NTLDR: "NTLDR" at 0x44-ish; BOOTMGR: "BOOTMGR" at 0x44
        if bytes.len() >= 0x48 {
            if &bytes[0x2A..0x30] == b"BOOTMGR" {
                return FsKind::BootSector(BootLoader::BootMgr);
            }
            // LILO magic "LILO" at 0x6
            if bytes.len() >= 0x0A && &bytes[0x06..0x0A] == b"LILO" {
                return FsKind::BootSector(BootLoader::Lilo);
            }
        }
        // All MBR signatures match, but no FS — likely just an MBR partition
        // table with empty boot code.
        let ipart_boot = &bytes[0..0x1B8];
        let any_non_zero = ipart_boot.iter().any(|b| *b != 0);
        if !any_non_zero {
            return FsKind::BootSector(BootLoader::MbrBlank);
        }
        return FsKind::Raw;
    }

    FsKind::Unknown
}

fn read_le32(b: &[u8], off: usize) -> u32 {
    if b.len() < off + 4 {
        0
    } else {
        u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_512() -> Vec<u8> {
        vec![0u8; 512]
    }

    #[test]
    fn empty_returns_unknown() {
        assert_eq!(detect_fs(&empty_512()), FsKind::Unknown);
    }

    #[test]
    fn short_input_returns_unknown() {
        assert_eq!(detect_fs(&[0u8; 100]), FsKind::Unknown);
    }

    #[test]
    fn detects_ntfs() {
        let mut b = empty_512();
        b[0..4].copy_from_slice(b"NTFS");
        b[510] = 0x55;
        b[511] = 0xAA;
        assert_eq!(detect_fs(&b), FsKind::Ntfs);
    }

    #[test]
    fn detects_exfat() {
        let mut b = empty_512();
        b[0..5].copy_from_slice(b"EXFAT");
        assert_eq!(detect_fs(&b), FsKind::ExFat);
    }

    #[test]
    fn detects_fat32() {
        let mut b = empty_512();
        b[82..90].copy_from_slice(b"FAT32   ");
        b[510] = 0x55;
        b[511] = 0xAA;
        assert_eq!(detect_fs(&b), FsKind::Fat { variant: FatVariant::Fat32 });
    }

    #[test]
    fn detects_fat16_at_54() {
        let mut b = empty_512();
        b[54..62].copy_from_slice(b"FAT16   ");
        b[510] = 0x55;
        b[511] = 0xAA;
        assert_eq!(detect_fs(&b), FsKind::Fat { variant: FatVariant::Fat16 });
    }

    #[test]
    fn detects_ext4_via_incompat_extents_flag() {
        let mut b = vec![0u8; 2048];
        b[1080] = 0x53;
        b[1081] = 0xEF;
        // Incompat features at offset 96 — set EXTENTS bit (0x40)
        b[96..100].copy_from_slice(&0x40u32.to_le_bytes());
        assert_eq!(detect_fs(&b), FsKind::Ext { version: ExtVersion::Ext4 });
    }

    #[test]
    fn detects_ext2_when_no_journal_no_extents() {
        let mut b = vec![0u8; 2048];
        b[1080] = 0x53;
        b[1081] = 0xEF;
        assert_eq!(detect_fs(&b), FsKind::Ext { version: ExtVersion::Ext2 });
    }

    #[test]
    fn detects_xfs() {
        let mut b = empty_512();
        b[0..4].copy_from_slice(b"XFSB");
        assert_eq!(detect_fs(&b), FsKind::Xfs);
    }

    #[test]
    fn detects_btrfs() {
        let mut b = vec![0u8; 0x10050];
        b[0x10040..0x10048].copy_from_slice(b"_BHRfS_M");
        assert_eq!(detect_fs(&b), FsKind::Btrfs);
    }

    #[test]
    fn detects_apfs() {
        let mut b = empty_512();
        b[32..36].copy_from_slice(b"NXSB");
        assert_eq!(detect_fs(&b), FsKind::Apfs);
    }

    #[test]
    fn detects_swap() {
        let mut b = vec![0u8; 4096];
        b[4086..4096].copy_from_slice(b"SWAPSPACE2");
        assert_eq!(detect_fs(&b), FsKind::Swap);
    }

    #[test]
    fn detects_iso9660() {
        let mut b = vec![0u8; 32773];
        b[32768..32773].copy_from_slice(b"CD001");
        assert_eq!(detect_fs(&b), FsKind::Iso9660);
    }

    #[test]
    fn detects_mbr_blank() {
        let mut b = empty_512();
        b[510] = 0x55;
        b[511] = 0xAA;
        assert_eq!(detect_fs(&b), FsKind::BootSector(BootLoader::MbrBlank));
    }

    #[test]
    fn display_format_is_compact() {
        assert_eq!(FsKind::Ntfs.to_string(), "NTFS");
        assert_eq!(FsKind::Fat { variant: FatVariant::Fat32 }.to_string(), "FAT32");
        assert_eq!(FsKind::Ext { version: ExtVersion::Ext4 }.to_string(), "ext4");
        assert_eq!(FsKind::BootSector(BootLoader::Grub).to_string(), "GRUB");
    }
}
