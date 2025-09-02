#[cfg(unix)]
use std::sync::atomic::AtomicU8;
#[cfg(unix)]
use std::sync::atomic::AtomicU16;
use std::{
    io::{Result, Stdout, stdout},
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crossterm::{
    Command, SynchronizedUpdate, cursor,
    cursor::MoveTo,
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event, KeyCode, KeyEvent,
        KeyEventKind, KeyModifiers, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, ScrollUp},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    crossterm::{
        execute,
        terminal::{disable_raw_mode, enable_raw_mode},
    },
    layout::Offset,
    text::Line,
};
use tokio::select;
use tokio_stream::Stream;

use crate::{custom_terminal, custom_terminal::Terminal as CustomTerminal};

/// A type alias for the terminal type used in this application
pub type Terminal = CustomTerminal<CrosstermBackend<Stdout>>;

pub fn set_modes() -> Result<()> {
    execute!(stdout(), EnterAlternateScreen)?;
    execute!(stdout(), EnableMouseCapture)?;
    execute!(stdout(), EnableBracketedPaste)?;

    enable_raw_mode()?;
    // Enable keyboard enhancement flags so modifiers for keys like Enter are disambiguated.
    // chat_composer.rs is using a keyboard event listener to enter for any modified keys
    // to create a new line that require this.
    // Some terminals (notably legacy Windows consoles) do not support
    // keyboard enhancement flags. Attempt to enable them, but continue
    // gracefully if unsupported.
    let _ = execute!(
        stdout(),
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
        )
    );
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EnableAlternateScroll;

impl Command for EnableAlternateScroll {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "\x1b[?1007h")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Err(std::io::Error::other("tried to execute EnableAlternateScroll using WinAPI; use ANSI instead"))
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisableAlternateScroll;

impl Command for DisableAlternateScroll {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "\x1b[?1007l")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Err(std::io::Error::other("tried to execute DisableAlternateScroll using WinAPI; use ANSI instead"))
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        true
    }
}

/// Restore the terminal to its original state.
/// Inverse of `set_modes`.
pub fn restore() -> Result<()> {
    execute!(stdout(), LeaveAlternateScreen);
    execute!(stdout(), DisableMouseCapture)?;
    execute!(stdout(), DisableBracketedPaste)?;
    disable_raw_mode()?;

    // Pop may fail on platforms that didn't support the push; ignore errors.
    let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
    Ok(())
}

fn set_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = restore(); // ignore any errors as we are already failing
        hook(panic_info);
    }));
}

/// Initialize the terminal (inline viewport; history stays in normal scrollback)
pub fn init() -> Result<Terminal> {
    set_modes()?;

    set_panic_hook();

    // Instead of clearing the screen (which can drop scrollback in some terminals),
    // scroll existing lines up until the cursor reaches the top, then start at (0, 0).
    if let Ok((_x, y)) = cursor::position()
        && y > 0
    {
        execute!(stdout(), ScrollUp(y))?;
    }
    execute!(stdout(), MoveTo(0, 0))?;

    let backend = CrosstermBackend::new(stdout());
    let tui = CustomTerminal::with_options(backend)?;
    Ok(tui)
}

#[derive(Debug)]
pub enum TuiEvent {
    Key(KeyEvent),
    Paste(String),
    Draw,
    AttachImage {
        path: PathBuf,
        width: u32,
        height: u32,
        format_label: &'static str,
    },
}

pub struct Tui {
    pub(crate) terminal: Terminal,
    task: tokio::task::JoinHandle<()>,
}

#[cfg(unix)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
enum ResumeAction {
    None = 0,
    RealignInline = 1,
    RestoreAlt = 2,
}

#[cfg(unix)]
enum PreparedResumeAction {
    RestoreAltScreen,
    RealignViewport(ratatui::layout::Rect),
}

#[cfg(unix)]
fn take_resume_action(pending: &AtomicU8) -> ResumeAction {
    match pending.swap(ResumeAction::None as u8, Ordering::Relaxed) {
        1 => ResumeAction::RealignInline,
        2 => ResumeAction::RestoreAlt,
        _ => ResumeAction::None,
    }
}

#[derive(Clone, Debug)]
pub struct FrameRequester {
    frame_schedule_tx: tokio::sync::mpsc::UnboundedSender<Instant>,
}
impl FrameRequester {
    pub fn schedule_frame(&self) {
        let _ = self.frame_schedule_tx.send(Instant::now());
    }

    pub fn schedule_frame_in(&self, dur: Duration) {
        let _ = self.frame_schedule_tx.send(Instant::now() + dur);
    }
}

impl Tui {
    pub fn new(terminal: Terminal) -> Self {
        let task = tokio::spawn(async {
            event_loop.await;
        });

        Self {
            terminal,
            task,
        }
    }

    // pub fn frame_requester(&self) -> FrameRequester {
    //     FrameRequester {
    //         frame_schedule_tx: self.frame_schedule_tx.clone(),
    //     }
    // }

    pub fn event_stream(&self) -> Pin<Box<dyn Stream<Item = TuiEvent> + Send + 'static>> {
        use tokio_stream::StreamExt;
        let mut crossterm_events = crossterm::event::EventStream::new();
        let mut draw_rx = self.draw_tx.subscribe();
        #[cfg(unix)]
        let resume_pending = self.resume_pending.clone();
        #[cfg(unix)]
        let alt_screen_active = self.alt_screen_active.clone();
        #[cfg(unix)]
        let suspend_cursor_y = self.suspend_cursor_y.clone();
        let event_stream = async_stream::stream! {
            loop {
                select! {
                    Some(Ok(event)) = crossterm_events.next() => {
                        match event {
                            // Detect Ctrl+V to attach an image from the clipboard.
                            Event::Key(key_event @ KeyEvent {
                                code: KeyCode::Char('v'),
                                modifiers: KeyModifiers::CONTROL,
                                kind: KeyEventKind::Press,
                                ..
                            }) => {
                                // match paste_image_to_temp_png() {
                                //     Ok((path, info)) => {
                                //         yield TuiEvent::AttachImage {
                                //             path,
                                //             width: info.width,
                                //             height: info.height,
                                //             format_label: info.encoded_format.label(),
                                //         };
                                //     }
                                //     Err(_) => {
                                //         // Fall back to normal key handling if no image is available.
                                //         yield TuiEvent::Key(key_event);
                                //     }
                                // }
                            }

                            crossterm::event::Event::Key(key_event) => {
                                #[cfg(unix)]
                                if matches!(
                                    key_event,
                                    crossterm::event::KeyEvent {
                                        code: crossterm::event::KeyCode::Char('z'),
                                        modifiers: crossterm::event::KeyModifiers::CONTROL,
                                        kind: crossterm::event::KeyEventKind::Press,
                                        ..
                                    }
                                )
                                {
                                    if alt_screen_active.load(Ordering::Relaxed) {
                                        // Disable alternate scroll when suspending from alt-screen
                                        let _ = execute!(stdout(), DisableAlternateScroll);
                                        let _ = execute!(stdout(), LeaveAlternateScreen);
                                        resume_pending.store(ResumeAction::RestoreAlt as u8, Ordering::Relaxed);
                                    } else {
                                        resume_pending.store(ResumeAction::RealignInline as u8, Ordering::Relaxed);
                                    }
                                    #[cfg(unix)]
                                    {
                                        let y = suspend_cursor_y.load(Ordering::Relaxed);
                                        let _ = execute!(stdout(), MoveTo(0, y));
                                    }
                                    let _ = execute!(stdout(), crossterm::cursor::Show);
                                    let _ = Tui::suspend();
                                    yield TuiEvent::Draw;
                                    continue;
                                }
                                yield TuiEvent::Key(key_event);
                            }
                            Event::Resize(_, _) => {
                                yield TuiEvent::Draw;
                            }
                            Event::Paste(pasted) => {
                                yield TuiEvent::Paste(pasted);
                            }
                            _ => {}
                        }
                    }
                    result = draw_rx.recv() => {
                        match result {
                            Ok(_) => {
                                yield TuiEvent::Draw;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                // We dropped one or more draw notifications; coalesce to a single draw.
                                yield TuiEvent::Draw;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                // Sender dropped; stop emitting draws from this source.
                            }
                        }
                    }
                }
            }
        };
        Box::pin(event_stream)
    }

    #[cfg(unix)]
    fn suspend() -> Result<()> {
        restore()?;
        unsafe { libc::kill(0, libc::SIGTSTP) };
        set_modes()?;
        Ok(())
    }

    #[cfg(unix)]
    fn prepare_resume_action(&mut self, action: ResumeAction) -> Result<Option<PreparedResumeAction>> {
        match action {
            ResumeAction::RealignInline => {
                let cursor_pos = self.terminal.get_cursor_position()?;
                Ok(Some(PreparedResumeAction::RealignViewport(ratatui::layout::Rect::new(0, cursor_pos.y, 0, 0))))
            }
            ResumeAction::RestoreAlt => {
                if let Ok(ratatui::layout::Position {
                    y,
                    ..
                }) = self.terminal.get_cursor_position()
                    && let Some(saved) = self.alt_saved_viewport.as_mut()
                {
                    saved.y = y;
                }
                Ok(Some(PreparedResumeAction::RestoreAltScreen))
            }
            ResumeAction::None => Ok(None),
        }
    }

    #[cfg(unix)]
    fn apply_prepared_resume_action(&mut self, prepared: PreparedResumeAction) -> Result<()> {
        match prepared {
            PreparedResumeAction::RealignViewport(area) => {
                self.terminal.set_viewport_area(area);
            }
            PreparedResumeAction::RestoreAltScreen => {
                execute!(self.terminal.backend_mut(), EnterAlternateScreen)?;
                // Enable "alternate scroll" so terminals may translate wheel to arrows
                execute!(self.terminal.backend_mut(), EnableAlternateScroll)?;
                if let Ok(size) = self.terminal.size() {
                    self.terminal.set_viewport_area(ratatui::layout::Rect::new(0, 0, size.width, size.height));
                    self.terminal.clear()?;
                }
            }
        }
        Ok(())
    }

    /// Enter alternate screen and expand the viewport to full terminal size, saving the current
    /// inline viewport for restoration when leaving.
    pub fn enter_alt_screen(&mut self) -> Result<()> {
        let _ = execute!(self.terminal.backend_mut(), EnterAlternateScreen);
        // Enable "alternate scroll" so terminals may translate wheel to arrows
        let _ = execute!(self.terminal.backend_mut(), EnableAlternateScroll);
        if let Ok(size) = self.terminal.size() {
            self.alt_saved_viewport = Some(self.terminal.viewport_area);
            self.terminal.set_viewport_area(ratatui::layout::Rect::new(0, 0, size.width, size.height));
            let _ = self.terminal.clear();
        }
        self.alt_screen_active.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Leave alternate screen and restore the previously saved inline viewport, if any.
    pub fn leave_alt_screen(&mut self) -> Result<()> {
        // Disable alternate scroll when leaving alt-screen
        let _ = execute!(self.terminal.backend_mut(), DisableAlternateScroll);
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        if let Some(saved) = self.alt_saved_viewport.take() {
            self.terminal.set_viewport_area(saved);
        }
        self.alt_screen_active.store(false, Ordering::Relaxed);
        Ok(())
    }

    pub fn insert_history_lines(&mut self, lines: Vec<Line<'static>>) {
        self.pending_history_lines.extend(lines);
        self.frame_requester().schedule_frame();
    }

    pub fn draw(&mut self, height: u16, draw_fn: impl FnOnce(&mut custom_terminal::Frame)) -> Result<()> {
        // Precompute any viewport updates that need a cursor-position query before entering
        // the synchronized update, to avoid racing with the event reader.
        let mut pending_viewport_area: Option<ratatui::layout::Rect> = None;
        #[cfg(unix)]
        let mut prepared_resume = self.prepare_resume_action(take_resume_action(&self.resume_pending))?;
        {
            let terminal = &mut self.terminal;
            let screen_size = terminal.size()?;
            let last_known_screen_size = terminal.last_known_screen_size;
            if screen_size != last_known_screen_size {
                let cursor_pos = terminal.get_cursor_position()?;
                let last_known_cursor_pos = terminal.last_known_cursor_pos;
                if cursor_pos.y != last_known_cursor_pos.y {
                    let cursor_delta = cursor_pos.y as i32 - last_known_cursor_pos.y as i32;
                    let new_viewport_area = terminal.viewport_area.offset(Offset {
                        x: 0,
                        y: cursor_delta,
                    });
                    pending_viewport_area = Some(new_viewport_area);
                }
            }
        }

        std::io::stdout().sync_update(|_| {
            #[cfg(unix)]
            {
                if let Some(prepared) = prepared_resume.take() {
                    self.apply_prepared_resume_action(prepared)?;
                }
            }
            let terminal = &mut self.terminal;
            if let Some(new_area) = pending_viewport_area.take() {
                terminal.set_viewport_area(new_area);
                terminal.clear()?;
            }

            let size = terminal.size()?;

            let mut area = terminal.viewport_area;
            area.height = height.min(size.height);
            area.width = size.width;
            if area.bottom() > size.height {
                terminal.backend_mut().scroll_region_up(0..area.top(), area.bottom() - size.height)?;
                area.y = size.height - area.height;
            }
            if area != terminal.viewport_area {
                terminal.clear()?;
                terminal.set_viewport_area(area);
            }
            // if !self.pending_history_lines.is_empty() {
            //     crate::insert_history::insert_history_lines(terminal, self.pending_history_lines.clone());
            //     self.pending_history_lines.clear();
            // }
            // Update the y position for suspending so Ctrl-Z can place the cursor correctly.
            #[cfg(unix)]
            {
                let inline_area_bottom = if self.alt_screen_active.load(Ordering::Relaxed) {
                    self.alt_saved_viewport
                        .map(|r| r.bottom().saturating_sub(1))
                        .unwrap_or_else(|| area.bottom().saturating_sub(1))
                } else {
                    area.bottom().saturating_sub(1)
                };
                self.suspend_cursor_y.store(inline_area_bottom, Ordering::Relaxed);
            }
            terminal.draw(|frame| {
                draw_fn(frame);
            })?;
            Ok(())
        })?
    }
}

// #[derive(Debug)]
// pub enum AppEvent {
//     UiEvent(Event),
// }

// pub fn start(tick_rate: Duration) -> Result<(), Box<dyn Error>> {
//     // setup terminal
//     enable_raw_mode()?;
//     let mut stdout: io::Stdout = io::stdout();
//     execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
//     let backend: CrosstermBackend<io::Stdout> = CrosstermBackend::new(stdout);
//     let mut terminal: Terminal<CrosstermBackend<io::Stdout>> = Terminal::new(backend)?;

//     let (tx, rx) = mpsc::channel();
//     let event_tx = tx;

//     thread::spawn(move || input_thread(&event_tx));

//     // create app and run it
//     let mut app: App<'_> = App::new();
//     app.setup_adb();

//     let app_result: Result<(), Box<dyn Error>> = run_app(&mut terminal, &rx, &mut app, tick_rate);

//     // restore terminal
//     disable_raw_mode()?;
//     execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
//     terminal.show_cursor()?;

//     if let Err(err) = app_result {
//         println!("{err:?}");
//     }

//     Ok(())
// }

// fn run_app<B: Backend>(
//     terminal: &mut Terminal<B>,
//     rx: &mpsc::Receiver<AppEvent>,
//     app: &mut App,
//     tick_rate: Duration,
// ) -> Result<(), Box<dyn Error>>
// where
//     B::Error: 'static,
// {
//     let mut last_tick = Instant::now();

//     loop {
//         // Wait for either input or the next tick.
//         let timeout = tick_rate.saturating_sub(last_tick.elapsed());
//         let mut should_draw = false;

//         match rx.recv_timeout(timeout) {
//             Ok(AppEvent::UiEvent(event)) => {
//                 handle_ui_event(app, &event);
//                 should_draw = true;

//                 // Drain any queued events to avoid redundant redraws.
//                 while let Ok(AppEvent::UiEvent(event)) = rx.try_recv() {
//                     handle_ui_event(app, &event);
//                 }
//             }
//             Err(mpsc::RecvTimeoutError::Timeout) => {
//                 app.on_tick();
//                 last_tick = Instant::now();
//                 should_draw = true;
//             }
//             Err(mpsc::RecvTimeoutError::Disconnected) => break,
//         }

//         if app.should_quit {
//             break;
//         }

//         if should_draw {
//             terminal.draw(|frame| ui::render(frame, app))?;
//         }
//     }
//     Ok(())
// }

// pub fn input_thread(tx_event: &mpsc::Sender<AppEvent>) -> anyhow::Result<()> {
//     while let Ok(event) = event::read() {
//         tx_event.send(AppEvent::UiEvent(event))?;
//     }
//     Ok(())
// }

// fn handle_ui_event(app: &mut App, event: &Event) {
//     match event {
//         Event::Key(key) => match key.code {
//             KeyCode::Char('j') | KeyCode::Down => app.scroll_down(),
//             KeyCode::Char('k') | KeyCode::Up => app.scroll_up(),
//             KeyCode::Tab => app.on_right(),
//             KeyCode::Char(c) => app.on_key(c),
//             _ => {}
//         },
//         Event::Mouse(mouse) => match mouse.kind {
//             MouseEventKind::ScrollUp => {
//                 app.scroll_up();
//             }
//             MouseEventKind::ScrollDown => {
//                 app.scroll_down();
//             }
//             _ => (),
//         },
//         _ => (),
//     }
// }
