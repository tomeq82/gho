//! `gho info` — display header, partition table, and detected format.

use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};

use gho::ghost11::{FileHeader, HEADER_SIZE as GHOST11_HEADER_SIZE};
use gho::ghostold::{record::RECORD_MAGIC, HEADER_SIZE as GHOSTOLD_HEADER_SIZE};

#[derive(Debug, ClapArgs)]
pub struct InfoArgs {
    /// Input image file(s). For spanned images, list in order.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,

    /// Output JSON instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

pub type Args = InfoArgs;

#[derive(Debug, Serialize)]
struct InfoOutput {
    format: String,
    files: Vec<FileInfo>,
    image_id: Option<u32>,
    compression: Option<String>,
    encrypted: bool,
    partition_count: Option<usize>,
    dirent_count: Option<usize>,
}

#[derive(Debug, Serialize)]
struct FileInfo {
    path: String,
    size_bytes: u64,
    file_type: u8,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FormatFamily {
    Ghost11,
    GhostOld,
    Unknown,
}

/// Disambiguate between 11.x and pre-11.x by reading the first 16 bytes
/// after the 512-byte file header and looking at the type code / magic.
/// Scans forward up to 200 KB to skip zero padding (common in pre-11.x
/// images) before giving up.
pub fn detect_format_family(path: &Path) -> FormatFamily {
    let Ok(mut f) = std::fs::File::open(path) else {
        return FormatFamily::Unknown;
    };
    use std::io::Seek;
    let _ = f.seek(std::io::SeekFrom::Start(GHOST11_HEADER_SIZE as u64));
    let mut buf = [0u8; 16];
    let scan_limit: u64 = 200_000;
    let mut scanned: u64 = 0;
    while scanned < scan_limit {
        let pos = (GHOST11_HEADER_SIZE as u64) + scanned;
        let _ = f.seek(std::io::SeekFrom::Start(pos));
        let n = f.read(&mut buf).unwrap_or(0);
        if n < 10 {
            return FormatFamily::Unknown;
        }
        let type_code = u16::from_le_bytes([buf[0], buf[1]]);
        let magic = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        if magic == RECORD_MAGIC {
            return match type_code {
                0x0006 | 0x0603 | 0x0703 | 0x0023 => FormatFamily::Ghost11,
                0x2C17 | 0x2C04 | 0x0104 | 0x0002 | 0x0102 | 0x0103 | 0x0118 | 0x0117 => {
                    FormatFamily::GhostOld
                }
                _ => FormatFamily::Unknown,
            };
        }
        scanned += 1;
    }
    FormatFamily::Unknown
}

pub fn run(args: Args) -> Result<()> {
    if args.inputs.is_empty() {
        anyhow::bail!("at least one input file is required");
    }

    // Read the first 512 bytes of the first file to detect the format.
    let first = &args.inputs[0];
    let header_bytes = read_header(first)?;
    let mut info = InfoOutput {
        format: "unknown".to_string(),
        files: Vec::new(),
        image_id: None,
        compression: None,
        encrypted: false,
        partition_count: None,
        dirent_count: None,
    };

    for path in &args.inputs {
        let meta = std::fs::metadata(path)
            .with_context(|| format!("stat {}", path.display()))?;
        let hdr = read_header(path).unwrap_or([0u8; GHOST11_HEADER_SIZE]);
        let file_type = hdr.get(2).copied().unwrap_or(255);
        info.files.push(FileInfo {
            path: path.display().to_string(),
            size_bytes: meta.len(),
            file_type,
        });
    }

    // Try Ghost 11.x header.
    if let Ok(hdr) = FileHeader::parse(&header_bytes) {
        info.image_id = Some(hdr.image_id);
        info.compression = Some(match hdr.compression {
            0 => "none".to_string(),
            2 => "fastlz".to_string(),
            3..=9 => format!("zlib({})", hdr.compression),
            other => format!("unknown({other})"),
        });
        info.encrypted = hdr.encrypted;

        // Disambiguate between 11.x and pre-11.x by peeking at the first
        // record after the header. Pre-11.x uses type codes from a different
        // set (BootHmr=0x2C17, FirstDirent=0x2C04, Dirent=0x0104, etc.).
        let format = detect_format_family(&args.inputs[0]);

        match format {
            FormatFamily::Ghost11 => {
                info.format = "ghost-11.x".to_string();
                let tmp = tempfile::tempdir().context("create tempdir")?;
                if let Ok(r) = gho::ghost11::stream::extract(&args.inputs[0], tmp.path()) {
                    info.partition_count = Some(r.partitions.len());
                }
            }
            FormatFamily::GhostOld => {
                info.format = "ghost-pre-11.x".to_string();
                let tmp = tempfile::tempdir().context("create tempdir")?;
                let combined = tmp.path().join("combined.gho");
                let paths: Vec<&std::path::Path> =
                    args.inputs.iter().map(|p| p.as_path()).collect();
                if gho::span::concatenate_spans(paths.iter().copied(), &combined).is_ok() {
                    if let Ok(entries) = gho::ghostold::stream::walk_dirents(&combined) {
                        info.dirent_count = Some(entries.len());
                    }
                }
            }
            FormatFamily::Unknown => {
                info.format = "unknown (valid FEEF header)".to_string();
            }
        }
    } else {
        info.format = "not a Ghost image (bad magic)".to_string();
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        print_human(&info);
    }
    Ok(())
}

fn print_human(info: &InfoOutput) {
    println!("Format:       {}", info.format);
    if let Some(id) = info.image_id {
        println!("Image ID:     0x{:08x}", id);
    }
    if let Some(ref c) = info.compression {
        println!("Compression:  {}", c);
    }
    if info.encrypted {
        println!("Encrypted:    yes (not supported)");
    }
    if let Some(n) = info.partition_count {
        println!("Partitions:   {}", n);
    }
    if let Some(n) = info.dirent_count {
        println!("Dirents:      {}", n);
    }
    println!("\nFiles:");
    for f in &info.files {
        let type_str = match f.file_type {
            1 => "first",
            9 => "continuation",
            _ => "?",
        };
        println!(
            "  {} ({} B, type {})",
            f.path,
            f.size_bytes,
            type_str
        );
    }
}

fn read_header(path: &std::path::Path) -> Result<[u8; GHOST11_HEADER_SIZE]> {
    let mut f = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut buf = [0u8; GHOST11_HEADER_SIZE];
    f.read_exact(&mut buf)
        .with_context(|| format!("read header from {}", path.display()))?;
    let _ = GHOSTOLD_HEADER_SIZE;
    Ok(buf)
}
