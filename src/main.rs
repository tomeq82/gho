use clap::{Parser, Subcommand};

mod cli;

use cli::{bench, browse, diff, extract, info, verify};

/// `gho` — extract partitions and files from Norton Ghost .GHO/.GHS disk images.
#[derive(Debug, Parser)]
#[command(name = "gho", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Display header, partition table, and detected format of an image.
    Info(info::Args),
    /// Extract partitions (11.x/12.x) or files (pre-11.x) to a directory.
    Extract(extract::Args),
    /// Verify image integrity by walking every record and decompressing every block.
    Verify(verify::Args),
    /// Browse a Norton Ghost image interactively (TUI).
    ///
    /// Launches an interactive two-panel viewer with mouse + theme
    /// switching. Press `?` inside the TUI for keybindings.
    Browse(browse::Args),
    /// Compare two Norton Ghost images interactively (TUI snapshot diff).
    ///
    /// Renders a side-by-side diff of the directory tree and contents,
    /// with line-level diffs for matching files.
    Diff(diff::Args),
    /// Run benchmarks against an image (FastLZ + parser throughput).
    Bench(bench::Args),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Info(args) => info::run(args),
        Command::Extract(args) => extract::run(args),
        Command::Verify(args) => verify::run(args),
        Command::Browse(args) => browse::run(args),
        Command::Diff(args) => diff::run(args),
        Command::Bench(args) => bench::run(args),
    }
}
