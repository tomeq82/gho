//! `gho bench` — run micro-benchmarks against an image.

use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use std::path::PathBuf;
use std::time::Instant;

use gho::fastlz;
use gho::ghost11::FileHeader;
use gho::ghostold::HEADER_SIZE as GHOSTOLD_HEADER_SIZE;

#[derive(Debug, ClapArgs)]
pub struct BenchArgs {
    /// Input image file(s). For spanned images, list in order.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,

    /// Number of iterations for the FastLZ micro-benchmark.
    #[arg(long, default_value_t = 5)]
    pub iterations: usize,
}

pub type Args = BenchArgs;

pub fn run(args: Args) -> Result<()> {
    if args.inputs.is_empty() {
        anyhow::bail!("at least one input file is required");
    }

    // Header detection.
    let header = read_header(&args.inputs[0])?;
    let hdr = FileHeader::parse(&header).context("not a Norton Ghost image")?;

    // Read the whole file into memory for the FastLZ benchmark.
    let bytes = std::fs::read(&args.inputs[0]).context("read image")?;
    println!(
        "Loaded {} ({:.2} MiB) — compression: {}",
        args.inputs[0].display(),
        bytes.len() as f64 / 1_048_576.0,
        match hdr.compression {
            0 => "none",
            2 => "fastlz",
            3..=9 => "zlib",
            _ => "unknown",
        }
    );

    // FastLZ throughput: take blocks of MAX_BLOCK_DECOMPRESSED bytes
    // (the per-block cap, so this is a "happy path" micro-benchmark).
    let block_size = gho::fastlz::MAX_BLOCK_DECOMPRESSED.min(bytes.len());
    let chunk = &bytes[..block_size];
    // Prepend the uncompressed-escape prefix: 1 byte header + 3-byte LE length.
    let mut sample = Vec::with_capacity(block_size + 4);
    sample.push(1u8);
    let n = block_size as u32;
    sample.push((n & 0xFF) as u8);
    sample.push(((n >> 8) & 0xFF) as u8);
    sample.push(((n >> 16) & 0xFF) as u8);
    sample.extend_from_slice(chunk);

    let start = Instant::now();
    let mut total_bytes = 0usize;
    for _ in 0..args.iterations {
        let out = fastlz::decompress(&sample, sample.len())?;
        total_bytes += out.len();
    }
    let elapsed = start.elapsed();
    let throughput = total_bytes as f64 / elapsed.as_secs_f64() / 1_048_576.0;
    println!(
        "FastLZ decompress: {} iterations × {} bytes = {} total in {:.2?} ({:.2} MiB/s)",
        args.iterations, block_size, total_bytes, elapsed, throughput
    );

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
