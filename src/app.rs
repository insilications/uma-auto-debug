use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use codex_ansi_escape::ansi_escape_line;
use codex_core::{ConversationManager, config::Config, protocol::TokenUsage};
use codex_login::AuthManager;
use color_eyre::eyre::Result;
use crossterm::{
    event::{KeyCode, KeyEvent, KeyEventKind},
    terminal::supports_keyboard_enhancement,
};
use ratatui::{style::Stylize, text::Line};
use tokio::{
    select,
    sync::mpsc::{channel, unbounded_channel},
};
use tokio_stream::Stream;

use crate::{app_backtrack::BacktrackState, app_event::AppEvent, app_event_sender::AppEventSender, tui, tui::TuiEvent};

// use crate::{
//     app_backtrack::BacktrackState, app_event::AppEvent, app_event_sender::AppEventSender, chatwidget::ChatWidget,
//     file_search::FileSearchManager, pager_overlay::Overlay, tui, tui::TuiEvent,
// };
// use uuid::Uuid;

pub(crate) struct App {
    tick_rate: f64,
    // pub(crate) server: Arc<ConversationManager>,
    // pub(crate) app_event_tx: AppEventSender,
    // pub(crate) chat_widget: ChatWidget,

    // /// Config is stored here so we can recreate ChatWidgets as needed.
    // pub(crate) config: Config,

    // pub(crate) file_search: FileSearchManager,

    // pub(crate) transcript_lines: Vec<Line<'static>>,

    // // Pager overlay state (Transcript or Static like Diff)
    // pub(crate) overlay: Option<Overlay>,
    // pub(crate) deferred_history_lines: Vec<Line<'static>>,

    // pub(crate) enhanced_keys_supported: bool,

    // /// Controls the animation thread that sends CommitTick events.
    // pub(crate) commit_anim_running: Arc<AtomicBool>,

    // // Esc-backtracking state grouped
    // pub(crate) backtrack: crate::app_backtrack::BacktrackState,
}

impl App {
    pub async fn run(tui: &mut tui::Tui, tick_rate: f64) -> Result<()> {
        use tokio_stream::StreamExt;
        let (app_event_tx, mut app_event_rx) = unbounded_channel();
        let app_event_tx = AppEventSender::new(app_event_tx);

        // let chat_widget = ChatWidget::new(
        //     config.clone(),
        //     conversation_manager.clone(),
        //     tui.frame_requester(),
        //     app_event_tx.clone(),
        //     initial_prompt,
        //     initial_images,
        //     enhanced_keys_supported,
        // );

        let mut app: App = Self {
            tick_rate,
        };

        let tui_events: std::pin::Pin<Box<dyn Stream<Item = TuiEvent> + Send + 'static>> = tui.event_stream();
        tokio::pin!(tui_events);

        while select! {
            Some(event) = app_event_rx.recv() => {
                app.handle_event(tui, event).await?
            }
            Some(event) = tui_events.next() => {
                app.handle_tui_event(tui, event).await?
            }
        } {}
        tui.terminal.clear()?;
        Ok(())
    }

    pub(crate) async fn handle_tui_event(&mut self, tui: &mut tui::Tui, event: TuiEvent) -> Result<bool> {
        if self.overlay.is_some() {
            let _ = self.handle_backtrack_overlay_event(tui, event).await?;
        } else {
            match event {
                TuiEvent::Key(key_event) => {
                    self.handle_key_event(tui, key_event).await;
                }
                TuiEvent::Paste(pasted) => {
                    // Many terminals convert newlines to \r when pasting (e.g., iTerm2),
                    // but tui-textarea expects \n. Normalize CR to LF.
                    // [tui-textarea]: https://github.com/rhysd/tui-textarea/blob/4d18622eeac13b309e0ff6a55a46ac6706da68cf/src/textarea.rs#L782-L783
                    // [iTerm2]: https://github.com/gnachman/iTerm2/blob/5d0c0d9f68523cbd0494dad5422998964a2ecd8d/sources/iTermPasteHelper.m#L206-L216
                    let pasted = pasted.replace("\r", "\n");
                    self.chat_widget.handle_paste(pasted);
                }
                TuiEvent::Draw => {
                    if self.chat_widget.handle_paste_burst_tick(tui.frame_requester()) {
                        return Ok(true);
                    }
                    tui.draw(self.chat_widget.desired_height(tui.terminal.size()?.width), |frame| {
                        frame.render_widget_ref(&self.chat_widget, frame.area());
                        if let Some((x, y)) = self.chat_widget.cursor_pos(frame.area()) {
                            frame.set_cursor_position((x, y));
                        }
                    })?;
                }
                TuiEvent::AttachImage {
                    path,
                    width,
                    height,
                    format_label,
                } => {
                    self.chat_widget.attach_image(path, width, height, format_label);
                }
            }
        }
        Ok(true)
    }

    async fn handle_event(&mut self, tui: &mut tui::Tui, event: AppEvent) -> Result<bool> {
        match event {
            AppEvent::NewSession => {
                self.chat_widget = ChatWidget::new(
                    self.config.clone(),
                    self.server.clone(),
                    tui.frame_requester(),
                    self.app_event_tx.clone(),
                    None,
                    Vec::new(),
                    self.enhanced_keys_supported,
                );
                tui.frame_requester().schedule_frame();
            }
            AppEvent::InsertHistoryLines(lines) => {
                if let Some(Overlay::Transcript(t)) = &mut self.overlay {
                    t.insert_lines(lines.clone());
                    tui.frame_requester().schedule_frame();
                }
                self.transcript_lines.extend(lines.clone());
                if self.overlay.is_some() {
                    self.deferred_history_lines.extend(lines);
                } else {
                    tui.insert_history_lines(lines);
                }
            }
            AppEvent::InsertHistoryCell(cell) => {
                let cell_transcript = cell.transcript_lines();
                if let Some(Overlay::Transcript(t)) = &mut self.overlay {
                    t.insert_lines(cell_transcript.clone());
                    tui.frame_requester().schedule_frame();
                }
                self.transcript_lines.extend(cell_transcript.clone());
                let display = cell.display_lines();
                if !display.is_empty() {
                    if self.overlay.is_some() {
                        self.deferred_history_lines.extend(display);
                    } else {
                        tui.insert_history_lines(display);
                    }
                }
            }
            AppEvent::StartCommitAnimation => {
                if self.commit_anim_running.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok()
                {
                    let tx = self.app_event_tx.clone();
                    let running = self.commit_anim_running.clone();
                    thread::spawn(move || {
                        while running.load(Ordering::Relaxed) {
                            thread::sleep(Duration::from_millis(50));
                            tx.send(AppEvent::CommitTick);
                        }
                    });
                }
            }
            AppEvent::StopCommitAnimation => {
                self.commit_anim_running.store(false, Ordering::Release);
            }
            AppEvent::CommitTick => {
                self.chat_widget.on_commit_tick();
            }
            AppEvent::CodexEvent(event) => {
                self.chat_widget.handle_codex_event(event);
            }
            AppEvent::ConversationHistory(ev) => {
                self.on_conversation_history_for_backtrack(tui, ev).await?;
            }
            AppEvent::ExitRequest => {
                return Ok(false);
            }
            AppEvent::CodexOp(op) => self.chat_widget.submit_op(op),
            AppEvent::DiffResult(text) => {
                // Clear the in-progress state in the bottom pane
                self.chat_widget.on_diff_complete();
                // Enter alternate screen using TUI helper and build pager lines
                let _ = tui.enter_alt_screen();
                let pager_lines: Vec<ratatui::text::Line<'static>> = if text.trim().is_empty() {
                    vec!["No changes detected.".italic().into()]
                } else {
                    text.lines().map(ansi_escape_line).collect()
                };
                self.overlay = Some(Overlay::new_static_with_title(pager_lines, "D I F F".to_string()));
                tui.frame_requester().schedule_frame();
            }
            AppEvent::StartFileSearch(query) => {
                if !query.is_empty() {
                    self.file_search.on_user_query(query);
                }
            }
            AppEvent::FileSearchResult {
                query,
                matches,
            } => {
                self.chat_widget.apply_file_search_result(query, matches);
            }
            AppEvent::UpdateReasoningEffort(effort) => {
                self.chat_widget.set_reasoning_effort(effort);
            }
            AppEvent::UpdateModel(model) => {
                self.chat_widget.set_model(model);
            }
            AppEvent::UpdateAskForApprovalPolicy(policy) => {
                self.chat_widget.set_approval_policy(policy);
            }
            AppEvent::UpdateSandboxPolicy(policy) => {
                self.chat_widget.set_sandbox_policy(policy);
            }
        }
        Ok(true)
    }

    async fn handle_key_event(&mut self, tui: &mut tui::Tui, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Char('t'),
                modifiers: crossterm::event::KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } => {
                // Enter alternate screen and set viewport to full size.
                let _ = tui.enter_alt_screen();
                self.overlay = Some(Overlay::new_transcript(self.transcript_lines.clone()));
                tui.frame_requester().schedule_frame();
            }
            // Esc primes/advances backtracking only in normal (not working) mode
            // with an empty composer. In any other state, forward Esc so the
            // active UI (e.g. status indicator, modals, popups) handles it.
            KeyEvent {
                code: KeyCode::Esc,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                if self.chat_widget.is_normal_backtrack_mode() && self.chat_widget.composer_is_empty() {
                    self.handle_backtrack_esc_key(tui);
                } else {
                    self.chat_widget.handle_key_event(key_event);
                }
            }
            // Enter confirms backtrack when primed + count > 0. Otherwise pass to widget.
            KeyEvent {
                code: KeyCode::Enter,
                kind: KeyEventKind::Press,
                ..
            } if self.backtrack.primed && self.backtrack.count > 0 && self.chat_widget.composer_is_empty() => {
                // Delegate to helper for clarity; preserves behavior.
                self.confirm_backtrack_from_main();
            }
            KeyEvent {
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                // Any non-Esc key press should cancel a primed backtrack.
                // This avoids stale "Esc-primed" state after the user starts typing
                // (even if they later backspace to empty).
                if key_event.code != KeyCode::Esc && self.backtrack.primed {
                    self.reset_backtrack_state();
                }
                self.chat_widget.handle_key_event(key_event);
            }
            _ => {
                // Ignore Release key events.
            }
        };
    }
}

// use std::{net::Ipv4Addr, sync::mpsc};

// use adb_client::{ADBServer, ADBServerDevice};
// use ratatui::{text::Line, widgets::ScrollbarState};

// use crate::adb::AdbOptions;

// // This struct will implement `std::io::Write`. It will take log data as bytes,
// // buffer it into lines, and send each complete line over an MPSC channel.
// struct ChannelWriter {
//     sender: mpsc::Sender<String>,
//     buffer: Vec<u8>,
// }

// impl ChannelWriter {
//     fn new(sender: mpsc::Sender<String>) -> Self {
//         Self {
//             sender,
//             buffer: Vec::new(),
//         }
//     }
// }

// impl std::io::Write for ChannelWriter {
//     fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
//         self.buffer.extend_from_slice(buf);
//         // Process all complete lines found in the buffer.
//         while let Some(i) = self.buffer.iter().position(|&b| b == b'\n') {
//             let line_bytes = self.buffer.drain(..=i).collect::<Vec<u8>>();
//             // Attempt to convert to string and send.
//             if let Ok(line) = String::from_utf8(line_bytes) {
//                 // If the receiver is dropped, the app is closing. Signal this by
//                 // returning a BrokenPipe error, which will stop the `get_logs` call.
//                 if self.sender.send(line.trim_end().to_string()).is_err() {
//                     return Err(std::io::Error::new(
//                         std::io::ErrorKind::BrokenPipe,
//                         "Channel receiver has been dropped.",
//                     ));
//                 }
//             }
//             // Non-UTF8 lines are silently ignored.
//         }
//         Ok(buf.len())
//     }

//     fn flush(&mut self) -> std::io::Result<()> {
//         // No-op, as we send data as soon as a full line is received.
//         Ok(())
//     }
// }

// pub struct TabsState<'a> {
//     pub titles: Vec<&'a str>,
//     pub index: usize,
// }

// impl<'a> TabsState<'a> {
//     pub const fn new(titles: Vec<&'a str>) -> Self {
//         Self {
//             titles,
//             index: 0,
//         }
//     }

//     pub fn next(&mut self) {
//         self.index = (self.index + 1) % self.titles.len();
//     }
// }

// pub struct App<'a> {
//     pub should_quit: bool,
//     pub adb_options: AdbOptions,
//     pub follow_tail: bool,
//     pub tabs: TabsState<'a>,
//     pub vertical_scroll_state: ScrollbarState,
//     pub vertical_scroll: usize,
//     // Buffer owns its data with a 'static lifetime.
//     pub logs_buffer: Vec<Line<'static>>,
//     log_receiver: Option<mpsc::Receiver<String>>,
// }

// impl App<'_> {
//     // --- Cap the buffer to prevent unbounded memory growth ---
//     const MAX_LOG_LINES: usize = 65536;

//     pub fn new() -> Self {
//         App {
//             should_quit: false,
//             adb_options: AdbOptions::default(),
//             follow_tail: true,
//             tabs: TabsState::new(vec!["TRAINING", "LOGS"]),
//             vertical_scroll_state: ScrollbarState::default(),
//             vertical_scroll: 0,
//             logs_buffer: Vec::new(),
//             log_receiver: None,
//         }
//     }

//     pub fn setup_adb(&mut self) {
//         let server_address_ip: &Ipv4Addr = self.adb_options.address.ip();
//         if server_address_ip.is_loopback() || server_address_ip.is_unspecified() {
//             ADBServer::start(&std::collections::HashMap::default(), &None);
//         }

//         // --- Spawn the dedicated log-fetching thread ---
//         let (tx, rx) = mpsc::channel();
//         self.log_receiver = Some(rx);

//         let adb_addr = self.adb_options.address;

//         std::thread::spawn(move || {
//             // Create a new device instance for this thread to avoid sharing state.
//             let mut log_device = ADBServerDevice::autodetect(Some(adb_addr));
//             let writer = ChannelWriter::new(tx);

//             // This call blocks until the device disconnects or the writer returns an error.
//             if let Err(e) = log_device.get_logs(writer) {
//                 // You can log this error to a file for debugging.
//                 eprintln!("ADB logcat thread exited with error: {e:?}");
//             }
//         });
//     }

//     pub fn on_tick(&mut self) {
//         // --- Process incoming logs from the channel ---
//         // let mut number_received_lines: usize = 0;
//         // let mut logs_buffer_len: usize = 0;
//         // let mut logs_buffer_len: usize = self.logs_buffer.len();
//         if let Some(rx) = &self.log_receiver {
//             // Drain the channel of all pending messages without blocking.
//             while let Ok(log_line) = rx.try_recv() {
//                 self.logs_buffer.push(Line::from(log_line));
//                 // number_received_lines = number_received_lines.saturating_add(1);
//                 // self.vertical_scroll = self.vertical_scroll.saturating_add(1);
//             }

//             // logs_buffer_len = self.logs_buffer.len();
//             // logs_buffer_len = logs_buffer_len.saturating_add(number_received_lines);

//             // if self.logs_buffer.len() > Self::MAX_LOG_LINES {
//             //     let overflow_to_remove = self.logs_buffer.len() - Self::MAX_LOG_LINES;
//             //     self.logs_buffer.drain(0..overflow_to_remove);

//             //     self.vertical_scroll = self.vertical_scroll.saturating_sub(overflow_to_remove) +
//             // number_received_lines; }
//         }

//         // self.vertical_scroll = self.vertical_scroll.saturating_add(self.logs_buffer.len());
//         // self.vertical_scroll_state = self.vertical_scroll_state.content_length(self.logs_buffer.len());
//         // self.vertical_scroll_state = self
//         // .vertical_scroll_state
//         // .content_length(self.logs_buffer.len())
//         // .viewport_content_length(6)
//         // .position(self.vertical_scroll);
//     }

//     pub fn on_right(&mut self) {
//         self.tabs.next();
//     }

//     pub fn scroll_down(&mut self) {
//         self.vertical_scroll = self.vertical_scroll.saturating_add(1);
//         self.vertical_scroll_state = self.vertical_scroll_state.position(self.vertical_scroll);
//         self.follow_tail = false;
//     }

//     pub fn scroll_up(&mut self) {
//         self.vertical_scroll = self.vertical_scroll.saturating_sub(1);
//         self.vertical_scroll_state = self.vertical_scroll_state.position(self.vertical_scroll);
//         self.follow_tail = false;
//     }

//     pub fn on_key(&mut self, c: char) {
//         if c == 'q' {
//             self.should_quit = true;
//         }
//     }
// }
