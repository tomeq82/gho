//! `gho verify` — walk every record and decompress every block.

use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub struct VerifyArgs {
    /// Input image file(s). For spanned images, list in order.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,
}

pub type Args = VerifyArgs;

pub fn run(_args: Args) -> anyhow::Result<()> {
    anyhow::bail!("`verify` not yet implemented")
}
