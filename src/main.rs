mod adb;
mod app;
mod crossterm;
mod ui;

use std::{collections::HashMap, error::Error, io::Write, time::Duration};

use adb::AdbOptions;
use adb_client::{ADBServer, ADBServerDevice};
use clap::Parser;

#[derive(Debug, Parser)]
struct Cli {
    /// time in ms between two ticks.
    #[arg(short, long, default_value_t = 250)]
    tick_rate: u64,

    /// whether unicode symbols are used to improve the overall look of the app
    #[arg(short, long, default_value_t = true)]
    unicode: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    // color_eyre::install()?;
    let cli = Cli::parse();
    let tick_rate = Duration::from_millis(cli.tick_rate);

    // let adb_options: AdbOptions = AdbOptions::default();

    // let server_address_ip = adb_options.address.ip();
    // if server_address_ip.is_loopback() || server_address_ip.is_unspecified() {
    //     ADBServer::start(&HashMap::default(), &None);
    // }

    // let mut device = ADBServerDevice::autodetect(Some(adb_options.address));

    // let writer: Box<dyn Write> = Box::new(std::io::stdout());
    // device.get_logs(writer)?;

    crossterm::start(tick_rate, cli.unicode)?;
    Ok(())
}
