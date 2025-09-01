use std::{net::Ipv4Addr, sync::mpsc};

use adb_client::{ADBServer, ADBServerDevice};
use ratatui::{text::Line, widgets::ScrollbarState};

use crate::adb::AdbOptions;

// This struct will implement `std::io::Write`. It will take log data as bytes,
// buffer it into lines, and send each complete line over an MPSC channel.
struct ChannelWriter {
    sender: mpsc::Sender<String>,
    buffer: Vec<u8>,
}

impl ChannelWriter {
    fn new(sender: mpsc::Sender<String>) -> Self {
        Self {
            sender,
            buffer: Vec::new(),
        }
    }
}

impl std::io::Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        // Process all complete lines found in the buffer.
        while let Some(i) = self.buffer.iter().position(|&b| b == b'\n') {
            let line_bytes = self.buffer.drain(..=i).collect::<Vec<u8>>();
            // Attempt to convert to string and send.
            if let Ok(line) = String::from_utf8(line_bytes) {
                // If the receiver is dropped, the app is closing. Signal this by
                // returning a BrokenPipe error, which will stop the `get_logs` call.
                if self.sender.send(line.trim_end().to_string()).is_err() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "Channel receiver has been dropped.",
                    ));
                }
            }
            // Non-UTF8 lines are silently ignored.
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // No-op, as we send data as soon as a full line is received.
        Ok(())
    }
}

pub struct TabsState<'a> {
    pub titles: Vec<&'a str>,
    pub index: usize,
}

impl<'a> TabsState<'a> {
    pub const fn new(titles: Vec<&'a str>) -> Self {
        Self {
            titles,
            index: 0,
        }
    }

    pub fn next(&mut self) {
        self.index = (self.index + 1) % self.titles.len();
    }
}

pub struct App<'a> {
    pub should_quit: bool,
    pub adb_options: AdbOptions,
    pub follow_tail: bool,
    pub tabs: TabsState<'a>,
    pub vertical_scroll_state: ScrollbarState,
    pub vertical_scroll: usize,
    // Buffer owns its data with a 'static lifetime.
    pub logs_buffer: Vec<Line<'static>>,
    log_receiver: Option<mpsc::Receiver<String>>,
}

impl App<'_> {
    // --- Cap the buffer to prevent unbounded memory growth ---
    const MAX_LOG_LINES: usize = 65536;

    pub fn new() -> Self {
        App {
            should_quit: false,
            adb_options: AdbOptions::default(),
            follow_tail: true,
            tabs: TabsState::new(vec!["TRAINING", "LOGS"]),
            vertical_scroll_state: ScrollbarState::default(),
            vertical_scroll: 0,
            logs_buffer: Vec::new(),
            log_receiver: None,
        }
    }

    pub fn setup_adb(&mut self) {
        let server_address_ip: &Ipv4Addr = self.adb_options.address.ip();
        if server_address_ip.is_loopback() || server_address_ip.is_unspecified() {
            ADBServer::start(&std::collections::HashMap::default(), &None);
        }

        // --- Spawn the dedicated log-fetching thread ---
        let (tx, rx) = mpsc::channel();
        self.log_receiver = Some(rx);

        let adb_addr = self.adb_options.address;

        std::thread::spawn(move || {
            // Create a new device instance for this thread to avoid sharing state.
            let mut log_device = ADBServerDevice::autodetect(Some(adb_addr));
            let writer = ChannelWriter::new(tx);

            // This call blocks until the device disconnects or the writer returns an error.
            if let Err(e) = log_device.get_logs(writer) {
                // You can log this error to a file for debugging.
                eprintln!("ADB logcat thread exited with error: {e:?}");
            }
        });
    }

    pub fn on_tick(&mut self) {
        // --- Process incoming logs from the channel ---
        // let mut number_received_lines: usize = 0;
        // let mut logs_buffer_len: usize = 0;
        // let mut logs_buffer_len: usize = self.logs_buffer.len();
        if let Some(rx) = &self.log_receiver {
            // Drain the channel of all pending messages without blocking.
            while let Ok(log_line) = rx.try_recv() {
                self.logs_buffer.push(Line::from(log_line));
                // number_received_lines = number_received_lines.saturating_add(1);
                // self.vertical_scroll = self.vertical_scroll.saturating_add(1);
            }

            // logs_buffer_len = self.logs_buffer.len();
            // logs_buffer_len = logs_buffer_len.saturating_add(number_received_lines);

            // if self.logs_buffer.len() > Self::MAX_LOG_LINES {
            //     let overflow_to_remove = self.logs_buffer.len() - Self::MAX_LOG_LINES;
            //     self.logs_buffer.drain(0..overflow_to_remove);

            //     self.vertical_scroll = self.vertical_scroll.saturating_sub(overflow_to_remove) +
            // number_received_lines; }
        }

        // self.vertical_scroll = self.vertical_scroll.saturating_add(self.logs_buffer.len());
        // self.vertical_scroll_state = self.vertical_scroll_state.content_length(self.logs_buffer.len());
        // self.vertical_scroll_state = self
        // .vertical_scroll_state
        // .content_length(self.logs_buffer.len())
        // .viewport_content_length(6)
        // .position(self.vertical_scroll);
    }

    pub fn on_right(&mut self) {
        self.tabs.next();
    }

    pub fn scroll_down(&mut self) {
        self.vertical_scroll = self.vertical_scroll.saturating_add(1);
        self.vertical_scroll_state = self.vertical_scroll_state.position(self.vertical_scroll);
        self.follow_tail = false;
    }

    pub fn scroll_up(&mut self) {
        self.vertical_scroll = self.vertical_scroll.saturating_sub(1);
        self.vertical_scroll_state = self.vertical_scroll_state.position(self.vertical_scroll);
        self.follow_tail = false;
    }

    pub fn on_key(&mut self, c: char) {
        if c == 'q' {
            self.should_quit = true;
        }
    }
}
