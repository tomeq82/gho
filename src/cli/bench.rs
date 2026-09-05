//! `gho bench` — run micro-benchmarks against an image.

use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub struct BenchArgs {
    /// Input image file(s). For spanned images, list in order.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,
}

pub type Args = BenchArgs;

pub fn run(_args: Args) -> anyhow::Result<()> {
    anyhow::bail!("`bench` not yet implemented")
}
