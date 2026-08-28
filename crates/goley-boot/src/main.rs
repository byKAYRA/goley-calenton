

use anyhow::Result;
use clap::Parser;
use goley_boot::{Cli, execute};

fn main() -> Result<()> {
    let cli = Cli::parse();
    execute(cli)
}
