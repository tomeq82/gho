//! `gho extract` — extract partitions (11.x/12.x) or files (pre-11.x).

use clap::Args as ClapArgs;
use std::path::PathBuf;

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
}

pub type Args = ExtractArgs;

pub fn run(_args: Args) -> anyhow::Result<()> {
    anyhow::bail!("`extract` not yet implemented")
}
