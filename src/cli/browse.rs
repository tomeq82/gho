//! `gho browse` — interactive TUI for inspecting a single `.gho` / `.ghs` image.

use anyhow::Result;
use clap::Args as ClapArgs;
use std::path::PathBuf;

use gho::tui;

/// Browse a Norton Ghost image in an interactive terminal UI.
#[derive(Debug, ClapArgs)]
pub struct BrowseArgs {
    /// Image file(s). For spanned images, list in order.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,
}

pub type Args = BrowseArgs;

pub fn run(args: Args) -> Result<()> {
    if args.inputs.is_empty() {
        anyhow::bail!("at least one input file is required");
    }
    tui::run_browse(args.inputs)
}
