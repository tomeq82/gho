//! `gho diff` — interactive TUI comparing two `.gho` / `.ghs` images.

use anyhow::Result;
use clap::Args as ClapArgs;
use std::path::PathBuf;

use gho::tui;

/// Show differences between two Norton Ghost images in an interactive TUI.
///
/// The two arguments are `OLD` and `NEW`. They may be raw `.gho` files or
/// pre-concatenated logical streams from `gho span concat`.
#[derive(Debug, ClapArgs)]
pub struct DiffArgs {
    /// Old image, then new image.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,
}

pub type Args = DiffArgs;

pub fn run(args: Args) -> Result<()> {
    if args.inputs.len() != 2 {
        anyhow::bail!("gho diff requires exactly two input files (OLD NEW)");
    }
    tui::run_diff(args.inputs)
}
