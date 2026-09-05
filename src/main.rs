// The front half of milestone 1: loading, parsing and link classification are
// complete and tested here, and the renderer that consumes them lands in the
// next pull request of the same issue. The `dead_code` allows go with it.
mod cli;
#[allow(dead_code, reason = "consumed by theme/ and render/ in the next part of milestone 1")]
mod config;
#[allow(dead_code, reason = "consumed by theme/ and render/ in the next part of milestone 1")]
mod document;
#[allow(dead_code, reason = "consumed by theme/ and render/ in the next part of milestone 1")]
mod markdown;

use anyhow::Result;
use clap::Parser;

use crate::cli::Cli;

fn main() -> Result<()> {
    let _cli = Cli::parse();
    eprintln!("vademecum {}: scaffold only, rendering lands in milestone 1", env!("CARGO_PKG_VERSION"));
    Ok(())
}
