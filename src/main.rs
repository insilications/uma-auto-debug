mod adb;
mod app;
mod crossterm;
mod ui;

use std::{error::Error, time::Duration};

use clap::Parser;

#[derive(Debug, Parser)]
struct Cli {
    /// time in ms between two ticks.
    #[arg(short, long, default_value_t = 250)]
    tick_rate: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    // color_eyre::install()?;
    let cli = Cli::parse();
    let tick_rate = Duration::from_millis(cli.tick_rate);

    crossterm::start(tick_rate)?;
    Ok(())
}
