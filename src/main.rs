mod cli;

use anyhow::Result;
use clap::Parser;

use crate::cli::Cli;

fn main() -> Result<()> {
    let _cli = Cli::parse();
    eprintln!("vademecum {}: scaffold only, rendering lands in milestone 1", env!("CARGO_PKG_VERSION"));
    Ok(())
}
