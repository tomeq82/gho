//! `gho extract` — extract partitions (11.x/12.x) or files (pre-11.x).

use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use std::path::PathBuf;

use gho::ghost11::FileHeader;
use gho::ghostold::HEADER_SIZE as GHOSTOLD_HEADER_SIZE;
use gho::safety::sanitize_8_3;

#[derive(Debug, ClapArgs)]
pub struct ExtractArgs {
    /// Input image file(s). For spanned images, list in order.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,

    /// Output directory.
    #[arg(short, long)]
    pub out: PathBuf,

    /// Only extract a specific partition (11.x/12.x).
    #[arg(long)]
    pub partition: Option<usize>,

    /// Only extract files matching this 8.3 substring (pre-11.x).
    #[arg(long)]
    pub pattern: Option<String>,

    /// Output JSON progress events to stdout.
    #[arg(long)]
    pub json: bool,

    /// Refuse to overwrite existing files.
    #[arg(long)]
    pub no_clobber: bool,
}

pub type Args = ExtractArgs;

pub fn run(args: Args) -> Result<()> {
    if args.inputs.is_empty() {
        anyhow::bail!("at least one input file is required");
    }

    // Detect format.
    let header = read_header(&args.inputs[0])?;
    let magic_ok = FileHeader::parse(&header).is_ok();
    if !magic_ok {
        anyhow::bail!("not a Norton Ghost image (bad magic)");
    }

    let format = super::info::detect_format_family(&args.inputs[0]);

    std::fs::create_dir_all(&args.out).context("create output directory")?;

    match format {
        super::info::FormatFamily::Ghost11 => {
            extract_ghost11(&args)?;
        }
        super::info::FormatFamily::GhostOld => {
            extract_ghostold(&args)?;
        }
        super::info::FormatFamily::Unknown => {
            anyhow::bail!("could not detect format version");
        }
    }

    Ok(())
}

fn extract_ghost11(args: &Args) -> Result<()> {
    let result = gho::ghost11::stream::extract(&args.inputs[0], &args.out)
        .context("extract partitions from 11.x image")?;
    if args.json {
        let partitions: Vec<_> = result
            .partitions
            .iter()
            .filter(|p| args.partition.map(|n| n == p.index).unwrap_or(true))
            .map(|p| {
                serde_json::json!({
                    "index": p.index,
                    "mbr_type": p.mbr_type,
                    "compressed_bytes": p.compressed_bytes,
                    "decompressed_bytes": p.decompressed_bytes,
                    "output_path": p.output_path.display().to_string(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "format": "ghost-11.x",
                "partitions": partitions,
            }))?
        );
    } else {
        println!(
            "Extracted {} partition(s) to {}",
            result.partitions.len(),
            args.out.display()
        );
        for p in &result.partitions {
            println!(
                "  partition_{}: {} decompressed bytes -> {}",
                p.index,
                p.decompressed_bytes,
                p.output_path.display()
            );
        }
    }
    Ok(())
}

fn extract_ghostold(args: &Args) -> Result<()> {
    // Concatenate spans first.
    let tmp = tempfile::tempdir().context("create tempdir")?;
    let combined = tmp.path().join("combined.gho");
    let paths: Vec<&std::path::Path> = args.inputs.iter().map(|p| p.as_path()).collect();
    gho::span::concatenate_spans(paths.iter().copied(), &combined).context("concatenate spans")?;

    let entries = gho::ghostold::stream::walk_dirents(&combined).context("walk dirents")?;

    let mut extracted = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for entry in &entries {
        // Apply --pattern filter (case-insensitive substring match on display_name).
        if let Some(ref pat) = args.pattern {
            let name = entry.dirent.display_name();
            if !name.to_uppercase().contains(&pat.to_uppercase()) {
                skipped += 1;
                continue;
            }
        }

        let name = match sanitize_8_3(&entry.dirent.name, &entry.dirent.ext) {
            Some(n) => n,
            None => {
                // Fallback to a synthetic name based on offset.
                gho::safety::fallback_name(entry.dirent_offset)
            }
        };

        let out_path = args.out.join(&name);

        // Defence in depth: refuse any output path that escapes --out.
        if gho::safety::contains_parent_traversal(&out_path) {
            anyhow::bail!(
                "refusing to write to path that traverses parent dirs: {}",
                out_path.display()
            );
        }

        if args.no_clobber && out_path.exists() {
            skipped += 1;
            continue;
        }

        match gho::ghostold::stream::extract_file(&combined, entry, &out_path) {
            Ok(written) => {
                extracted += 1;
                if !args.json {
                    println!(
                        "  {} ({} bytes) -> {}",
                        entry.dirent.display_name(),
                        written,
                        out_path.display()
                    );
                }
            }
            Err(e) => {
                failed += 1;
                if !args.json {
                    eprintln!(
                        "  {} extraction failed: {}",
                        entry.dirent.display_name(),
                        e
                    );
                }
            }
        }
    }

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "format": "ghost-pre-11.x",
                "total_dirents": entries.len(),
                "extracted": extracted,
                "skipped": skipped,
                "failed": failed,
            }))?
        );
    } else {
        println!(
            "\n{extracted} extracted, {skipped} skipped, {failed} failed (of {} total dirents)",
            entries.len()
        );
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
