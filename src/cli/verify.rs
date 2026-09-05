//! `gho verify` — walk every record and decompress every block.

use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use std::path::PathBuf;

use gho::ghost11::FileHeader;
use gho::ghostold::HEADER_SIZE as GHOSTOLD_HEADER_SIZE;

#[derive(Debug, ClapArgs)]
pub struct VerifyArgs {
    /// Input image file(s). For spanned images, list in order.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,

    /// Stop after the first error.
    #[arg(long)]
    pub fail_fast: bool,
}

pub type Args = VerifyArgs;

pub fn run(args: Args) -> Result<()> {
    if args.inputs.is_empty() {
        anyhow::bail!("at least one input file is required");
    }
    let header = read_header(&args.inputs[0])?;
    if FileHeader::parse(&header).is_err() {
        anyhow::bail!("not a Norton Ghost image (bad magic)");
    }
    let format = super::info::detect_format_family(&args.inputs[0]);
    match format {
        super::info::FormatFamily::Ghost11 => verify_ghost11(&args),
        super::info::FormatFamily::GhostOld => verify_ghostold(&args),
        super::info::FormatFamily::Unknown => {
            anyhow::bail!("could not detect format version")
        }
    }
}

fn verify_ghost11(args: &Args) -> Result<()> {
    let tmp = tempfile::tempdir().context("create tempdir")?;
    let result = gho::ghost11::stream::extract(&args.inputs[0], tmp.path());
    match result {
        Ok(r) => {
            println!(
                "OK: parsed {} records; extracted {} partitions ({} decompressed bytes)",
                r.partitions.len(),
                r.partitions.len(),
                r.partitions
                    .iter()
                    .map(|p| p.decompressed_bytes)
                    .sum::<u64>()
            );
            for p in &r.partitions {
                println!(
                    "  partition_{}: {} compressed → {} bytes",
                    p.index, p.compressed_bytes, p.decompressed_bytes
                );
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("verify failed: {e}");
            Err(e).context("verify")
        }
    }
}

fn verify_ghostold(args: &Args) -> Result<()> {
    let tmp = tempfile::tempdir().context("create tempdir")?;
    let combined = tmp.path().join("combined.gho");
    let paths: Vec<&std::path::Path> = args.inputs.iter().map(|p| p.as_path()).collect();
    gho::span::concatenate_spans(paths.iter().copied(), &combined).context("concatenate spans")?;
    let entries = gho::ghostold::stream::walk_dirents(&combined).context("walk dirents")?;

    let mut checked = 0usize;
    let mut failed = 0usize;
    for entry in &entries {
        if entry.is_empty || entry.data_start_offset.is_none() {
            continue;
        }
        let out = tmp
            .path()
            .join(format!("verify_{}.bin", entry.dirent_offset));
        match gho::ghostold::stream::extract_file(&combined, entry, &out) {
            Ok(_) => checked += 1,
            Err(e) => {
                failed += 1;
                if args.fail_fast {
                    return Err(e)
                        .context(format!("verify failed on {}", entry.dirent.display_name()));
                }
            }
        }
    }
    println!(
        "OK: walked {} dirents, verified {} files (decompressed end-to-end), {} failures",
        entries.len(),
        checked,
        failed
    );
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn read_header(path: &std::path::Path) -> Result<[u8; 512]> {
    let mut f = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut buf = [0u8; 512];
    use std::io::Read;
    f.read_exact(&mut buf)
        .with_context(|| format!("read header from {}", path.display()))?;
    let _ = GHOSTOLD_HEADER_SIZE;
    Ok(buf)
}
