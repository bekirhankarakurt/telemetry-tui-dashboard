//! Application state and input handling.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::alerts::Thresholds;
use crate::net::Msg;
use crate::packet::DecodeError;
use crate::record::{Record, Recorder};
use crate::stats::Source;

/// Upper bound on tracked senders; further senders are counted as errors.
pub const MAX_SOURCES: usize = 32;

/// How multiple sources are displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LayoutMode {
    /// One source at a time, selected with tabs.
    Tabs,
    /// All sources next to each other.
    SideBySide,
}

/// Counters that are not tied to one source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Totals {
    pub received: u64,
    pub crc_errors: u64,
    pub other_errors: u64,
}

pub struct App {
    pub sources: Vec<Source>,
    pub totals: Totals,
    pub selected: usize,
    pub layout: LayoutMode,
    pub thresholds: Thresholds,
    pub history: usize,
    pub source_label: String,
    pub status: String,
    pub show_help: bool,
    pub quit: bool,
    pub recording: bool,
    pub record_path: Option<String>,
    frozen: Option<Vec<Source>>,
    recorder: Option<Recorder>,
    started: Instant,
}

impl App {
    pub fn new(
        source_label: String,
        history: usize,
        layout: LayoutMode,
        thresholds: Thresholds,
    ) -> Self {
        Self {
            sources: Vec::new(),
            totals: Totals::default(),
            selected: 0,
            layout,
            thresholds,
            history,
            source_label,
            status: String::new(),
            show_help: false,
            quit: false,
            recording: false,
            record_path: None,
            frozen: None,
            recorder: None,
            started: Instant::now(),
        }
    }

    /// Start recording to `recorder`, remembering the file name for display.
    pub fn attach_recorder(&mut self, recorder: Recorder, path: String) {
        self.recorder = Some(recorder);
        self.record_path = Some(path);
        self.recording = true;
    }

    /// Seconds since the application started.
    pub fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    pub fn paused(&self) -> bool {
        self.frozen.is_some()
    }

    /// Sources as shown on screen: a frozen snapshot while paused.
    pub fn view_sources(&self) -> &[Source] {
        self.frozen.as_deref().unwrap_or(&self.sources)
    }

    /// Selected index clamped to the visible sources.
    pub fn selected_index(&self) -> usize {
        self.selected
            .min(self.view_sources().len().saturating_sub(1))
    }

    fn source_index(&mut self, addr: &str) -> Option<usize> {
        if let Some(i) = self.sources.iter().position(|s| s.addr == addr) {
            return Some(i);
        }
        if self.sources.len() >= MAX_SOURCES {
            return None;
        }
        self.sources.push(Source::new(addr, self.history));
        Some(self.sources.len() - 1)
    }

    pub fn handle_msg(&mut self, msg: Msg) {
        match msg {
            Msg::Packet { src, packet } => {
                let now = self.now();
                let Some(i) = self.source_index(&src) else {
                    self.totals.other_errors += 1;
                    return;
                };
                self.totals.received += 1;
                self.sources[i].ingest(now, &packet);
                if self.recording {
                    if let Some(rec) = self.recorder.as_mut() {
                        if let Err(e) = rec.write(&Record::new(now, &src, &packet)) {
                            self.status = format!("recording stopped: {e}");
                            self.recording = false;
                        }
                    }
                }
            }
            Msg::Invalid { src, error } => {
                if matches!(error, DecodeError::BadCrc { .. }) {
                    self.totals.crc_errors += 1;
                    if let Some(i) = self.source_index(&src) {
                        self.sources[i].crc_errors += 1;
                    }
                } else {
                    self.totals.other_errors += 1;
                }
            }
            Msg::Status(text) => self.status = text,
        }
    }

    pub fn toggle_pause(&mut self) {
        self.frozen = match self.frozen {
            Some(_) => None,
            None => Some(self.sources.clone()),
        };
    }

    pub fn toggle_record(&mut self) {
        if self.recorder.is_none() {
            self.status = "recording is off: start with --record FILE".to_string();
            return;
        }
        self.recording = !self.recording;
        self.status = if self.recording {
            "recording resumed".to_string()
        } else {
            "recording stopped".to_string()
        };
    }

    pub fn toggle_layout(&mut self) {
        self.layout = match self.layout {
            LayoutMode::Tabs => LayoutMode::SideBySide,
            LayoutMode::SideBySide => LayoutMode::Tabs,
        };
    }

    /// Forget all sources and counters.
    pub fn reset(&mut self) {
        self.sources.clear();
        self.totals = Totals::default();
        self.selected = 0;
        if self.frozen.is_some() {
            self.frozen = Some(Vec::new());
        }
        self.status = "statistics cleared".to_string();
    }

    fn select_next(&mut self) {
        let n = self.view_sources().len();
        if n > 0 {
            self.selected = (self.selected_index() + 1) % n;
        }
    }

    fn select_prev(&mut self) {
        let n = self.view_sources().len();
        if n > 0 {
            self.selected = (self.selected_index() + n - 1) % n;
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                if self.show_help {
                    self.show_help = false;
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => self.select_next(),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => self.select_prev(),
            KeyCode::Char(d @ '1'..='9') => {
                let idx = d as usize - '1' as usize;
                if idx < self.view_sources().len() {
                    self.selected = idx;
                }
            }
            KeyCode::Char('p') | KeyCode::Char(' ') => self.toggle_pause(),
            KeyCode::Char('r') => self.toggle_record(),
            KeyCode::Char('v') => self.toggle_layout(),
            KeyCode::Char('c') => self.reset(),
            KeyCode::Char('?') | KeyCode::F(1) => self.show_help = !self.show_help,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::{Packet, Telemetry};
    use crossterm::event::KeyEventKind;

    fn app() -> App {
        App::new("test".into(), 16, LayoutMode::Tabs, Thresholds::default())
    }

    fn packet_msg(src: &str, seq: u32) -> Msg {
        Msg::Packet {
            src: src.to_string(),
            packet: Packet {
                seq: Some(seq),
                timestamp_ns: 0,
                telemetry: Telemetry::default(),
            },
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        let mut k = KeyEvent::new(code, KeyModifiers::NONE);
        k.kind = KeyEventKind::Press;
        k
    }

    #[test]
    fn groups_packets_by_sender() {
        let mut a = app();
        a.handle_msg(packet_msg("1.1.1.1:1", 1));
        a.handle_msg(packet_msg("2.2.2.2:2", 1));
        a.handle_msg(packet_msg("1.1.1.1:1", 2));
        assert_eq!(a.sources.len(), 2);
        assert_eq!(a.sources[0].seq.received, 2);
        assert_eq!(a.totals.received, 3);
    }

    #[test]
    fn counts_errors() {
        let mut a = app();
        a.handle_msg(Msg::Invalid {
            src: "x:1".into(),
            error: DecodeError::BadCrc {
                expected: 1,
                actual: 2,
            },
        });
        a.handle_msg(Msg::Invalid {
            src: "x:1".into(),
            error: DecodeError::BadMagic(0),
        });
        assert_eq!(a.totals.crc_errors, 1);
        assert_eq!(a.totals.other_errors, 1);
        assert_eq!(a.sources[0].crc_errors, 1);
    }

    #[test]
    fn source_count_is_bounded() {
        let mut a = app();
        for i in 0..(MAX_SOURCES + 5) {
            a.handle_msg(packet_msg(&format!("h:{i}"), 1));
        }
        assert_eq!(a.sources.len(), MAX_SOURCES);
        assert_eq!(a.totals.other_errors, 5);
    }

    #[test]
    fn pause_freezes_view_but_not_ingestion() {
        let mut a = app();
        a.handle_msg(packet_msg("s:1", 1));
        a.toggle_pause();
        assert!(a.paused());
        a.handle_msg(packet_msg("s:1", 2));
        assert_eq!(a.view_sources()[0].seq.received, 1);
        assert_eq!(a.sources[0].seq.received, 2);
        a.toggle_pause();
        assert_eq!(a.view_sources()[0].seq.received, 2);
    }

    #[test]
    fn keys_navigate_and_quit() {
        let mut a = app();
        a.handle_msg(packet_msg("a:1", 1));
        a.handle_msg(packet_msg("b:1", 1));
        a.handle_msg(packet_msg("c:1", 1));
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.selected_index(), 1);
        a.on_key(key(KeyCode::BackTab));
        a.on_key(key(KeyCode::BackTab));
        assert_eq!(a.selected_index(), 2);
        a.on_key(key(KeyCode::Char('2')));
        assert_eq!(a.selected_index(), 1);
        a.on_key(key(KeyCode::Char('9')));
        assert_eq!(a.selected_index(), 1);
        a.on_key(key(KeyCode::Char('v')));
        assert_eq!(a.layout, LayoutMode::SideBySide);
        assert!(!a.quit);
        a.on_key(key(KeyCode::Char('q')));
        assert!(a.quit);
    }

    #[test]
    fn ctrl_c_quits_and_esc_closes_help_first() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('?')));
        assert!(a.show_help);
        a.on_key(key(KeyCode::Esc));
        assert!(!a.show_help && !a.quit);
        a.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(a.quit);
    }

    #[test]
    fn record_toggle_requires_recorder() {
        let mut a = app();
        a.toggle_record();
        assert!(!a.recording);
        assert!(a.status.contains("--record"));
    }

    #[test]
    fn reset_clears_state() {
        let mut a = app();
        a.handle_msg(packet_msg("a:1", 1));
        a.on_key(key(KeyCode::Char('c')));
        assert!(a.sources.is_empty());
        assert_eq!(a.totals, Totals::default());
    }
}
