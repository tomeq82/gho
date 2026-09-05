//! `gho info` — display header, partition table, and detected format.

use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub struct InfoArgs {
    /// Input image file(s). For spanned images, list in order.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,
}

pub type Args = InfoArgs;

pub fn run(_args: Args) -> anyhow::Result<()> {
    anyhow::bail!("`info` not yet implemented")
}
