mod actions;
mod args;
mod defs;
mod picker;

use clap::Parser;

use defs::Cli;
use fcs::errors::AppError;

pub fn run() -> Result<(), AppError> {
    let cli = Cli::parse();
    let config = fcs::config::Config::load_or_create()?;
    fcs::config::init_global(config.clone());
    actions::execute(cli.command, config)
}
