use std::{
    error::Error,
    io,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;

use crate::{app::App, ui};

#[derive(Debug)]
pub enum AppEvent {
    UiEvent(Event),
}

pub fn start(tick_rate: Duration, enhanced_graphics: bool) -> Result<(), Box<dyn Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout: io::Stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend: CrosstermBackend<io::Stdout> = CrosstermBackend::new(stdout);
    let mut terminal: Terminal<CrosstermBackend<io::Stdout>> = Terminal::new(backend)?;

    let (tx, rx) = mpsc::channel();
    let event_tx = tx;

    thread::spawn(move || input_thread(&event_tx));

    // create app and run it
    let mut app: App<'_> = App::new("Uma Automation Debug", enhanced_graphics);
    app.setup_adb();

    let app_result: Result<(), Box<dyn Error>> = run_app(&mut terminal, &rx, &mut app, tick_rate);

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(err) = app_result {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    rx: &mpsc::Receiver<AppEvent>,
    app: &mut App,
    tick_rate: Duration,
) -> Result<(), Box<dyn Error>>
where
    B::Error: 'static,
{
    let mut last_tick = Instant::now();

    loop {
        // Wait for either input or the next tick.
        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        let mut should_draw = false;

        match rx.recv_timeout(timeout) {
            Ok(AppEvent::UiEvent(event)) => {
                handle_ui_event(app, &event);
                should_draw = true;

                // Drain any queued events to avoid redundant redraws.
                while let Ok(AppEvent::UiEvent(event)) = rx.try_recv() {
                    handle_ui_event(app, &event);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                app.on_tick();
                last_tick = Instant::now();
                should_draw = true;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if app.should_quit {
            break;
        }

        if should_draw {
            terminal.draw(|frame| ui::render(frame, app))?;
        }
    }
    Ok(())
}

pub fn input_thread(tx_event: &mpsc::Sender<AppEvent>) -> anyhow::Result<()> {
    while let Ok(event) = event::read() {
        tx_event.send(AppEvent::UiEvent(event))?;
    }
    Ok(())
}

fn handle_ui_event(app: &mut App, event: &Event) {
    if let Event::Key(key) = event {
        match key.code {
            // KeyCode::Char('h') | KeyCode::Left => app.on_left(),
            // KeyCode::Char('j') | KeyCode::Down => app.on_down(),
            // KeyCode::Char('k') | KeyCode::Up => app.on_up(),
            KeyCode::Char('j') | KeyCode::Down => app.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => app.scroll_up(),
            KeyCode::Char('h') | KeyCode::Left => app.scroll_left(),
            KeyCode::Char('l') | KeyCode::Right => app.scroll_right(),
            KeyCode::Tab => app.on_right(),
            KeyCode::Char(c) => app.on_key(c),
            _ => {}
        }
    }
}
