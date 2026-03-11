pub mod commands;
pub mod output;

use anyhow::Result;
use clap::Parser;

use crate::AppContext;

pub use commands::Cli;

impl Cli {
    pub fn run() -> Result<()> {
        let cli = Self::parse();
        let context = AppContext::bootstrap()?;
        commands::execute(cli.command, &context)
    }
}
