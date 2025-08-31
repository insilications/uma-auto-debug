use std::net::Ipv4Addr;

use adb_client::{ADBServer, ADBServerDevice};
use ratatui::widgets::{ListState, ScrollbarState};

use crate::adb::AdbOptions;

const LOGS: [(&str, &str); 26] = [
    ("Event1", "INFO"),
    ("Event2", "INFO"),
    ("Event3", "CRITICAL"),
    ("Event4", "ERROR"),
    ("Event5", "INFO"),
    ("Event6", "INFO"),
    ("Event7", "WARNING"),
    ("Event8", "INFO"),
    ("Event9", "INFO"),
    ("Event10", "INFO"),
    ("Event11", "CRITICAL"),
    ("Event12", "INFO"),
    ("Event13", "INFO"),
    ("Event14", "INFO"),
    ("Event15", "INFO"),
    ("Event16", "INFO"),
    ("Event17", "ERROR"),
    ("Event18", "ERROR"),
    ("Event19", "INFO"),
    ("Event20", "INFO"),
    ("Event21", "WARNING"),
    ("Event22", "INFO"),
    ("Event23", "INFO"),
    ("Event24", "WARNING"),
    ("Event25", "INFO"),
    ("Event26", "INFO"),
];

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

    pub fn previous(&mut self) {
        if self.index > 0 {
            self.index -= 1;
        } else {
            self.index = self.titles.len() - 1;
        }
    }
}

pub struct StatefulList<T> {
    pub state: ListState,
    pub items: Vec<T>,
}

impl<T> StatefulList<T> {
    pub fn with_items(items: Vec<T>) -> Self {
        Self {
            state: ListState::default(),
            items,
        }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }
}

pub struct App<'a> {
    pub title: &'a str,
    pub should_quit: bool,
    pub adb_options: AdbOptions,
    pub adb_device: Option<ADBServerDevice>,
    pub tabs: TabsState<'a>,
    pub logs: StatefulList<(&'a str, &'a str)>,
    pub vertical_scroll_state: ScrollbarState,
    pub horizontal_scroll_state: ScrollbarState,
    pub vertical_scroll: usize,
    pub horizontal_scroll: usize,
    pub enhanced_graphics: bool,
}

impl<'a> App<'a> {
    pub fn new(title: &'a str, enhanced_graphics: bool) -> Self {
        App {
            title,
            should_quit: false,
            adb_options: AdbOptions::default(),
            adb_device: None,
            tabs: TabsState::new(vec!["TRAINING", "LOGS"]),
            logs: StatefulList::with_items(LOGS.to_vec()),
            vertical_scroll_state: ScrollbarState::default(),
            horizontal_scroll_state: ScrollbarState::default(),
            vertical_scroll: 0,
            horizontal_scroll: 0,
            enhanced_graphics,
        }
    }

    pub fn setup_adb(&mut self) {
        let server_address_ip: &Ipv4Addr = self.adb_options.address.ip();
        if server_address_ip.is_loopback() || server_address_ip.is_unspecified() {
            ADBServer::start(&std::collections::HashMap::default(), &None);
        }

        self.adb_device = Some(ADBServerDevice::autodetect(Some(self.adb_options.address)));
    }

    // pub fn on_up(&mut self) {
    //     self.tasks.previous();
    // }

    // pub fn on_down(&mut self) {
    //     self.tasks.next();
    // }

    pub fn on_right(&mut self) {
        self.tabs.next();
    }

    // pub fn on_left(&mut self) {
    //     self.tabs.previous();
    // }

    pub fn scroll_down(&mut self) {
        self.vertical_scroll = self.vertical_scroll.saturating_add(1);
        self.vertical_scroll_state = self.vertical_scroll_state.position(self.vertical_scroll);
    }

    pub fn scroll_up(&mut self) {
        self.vertical_scroll = self.vertical_scroll.saturating_sub(1);
        self.vertical_scroll_state = self.vertical_scroll_state.position(self.vertical_scroll);
    }

    pub fn scroll_left(&mut self) {
        self.horizontal_scroll = self.horizontal_scroll.saturating_sub(1);
        self.horizontal_scroll_state = self.horizontal_scroll_state.position(self.horizontal_scroll);
    }

    pub fn scroll_right(&mut self) {
        self.horizontal_scroll = self.horizontal_scroll.saturating_add(1);
        self.horizontal_scroll_state = self.horizontal_scroll_state.position(self.horizontal_scroll);
    }

    pub fn on_key(&mut self, c: char) {
        if c == 'q' {
            self.should_quit = true;
        }
        // match c {
        //     'q' => {
        //         self.should_quit = true;
        //     }
        //     _ => {}
        // }
    }

    pub fn on_tick(&mut self) {
        let log = self.logs.items.pop().unwrap();
        self.logs.items.insert(0, log);
    }
}
