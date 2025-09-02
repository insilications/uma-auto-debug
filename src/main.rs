mod adb;
mod app;
mod cli;
pub mod custom_terminal;
pub mod insert_history;
mod pager_overlay;
mod tui;
mod ui;

use app::App;
use clap::Parser;
use cli::Cli;
use tui::Tui;

fn restore() {
    if let Err(err) = tui::restore() {
        eprintln!("failed to restore terminal. Run `reset` or restart your terminal to recover: {err}");
    }
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let cli_args: Cli = Cli::parse();
    run_tui(cli_args).await?;
    Ok(())
}

async fn run_tui(cli_args: Cli) -> color_eyre::Result<()> {
    color_eyre::install()?;

    // Forward panic reports through tracing so they appear in the UI status
    // line, but do not swallow the default/color-eyre panic handler.
    // Chain to the previous hook so users still get a rich panic report
    // (including backtraces) after we restore the terminal.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {info}");
        prev_hook(info);
    }));
    let mut terminal = tui::init()?;
    terminal.clear()?;

    let mut tui = Tui::new(terminal);

    let app_result = App::run(&mut tui, cli_args.tick_rate).await;
    restore();

    app_result
}
