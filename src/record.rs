//! JSONL recording and replay.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::net::Msg;
use crate::packet::{Packet, Telemetry};

/// One line of a recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Seconds since the recording session started.
    pub t: f64,
    pub src: String,
    pub seq: Option<u32>,
    pub ts_ns: u64,
    pub data: Telemetry,
}

impl Record {
    pub fn new(t: f64, src: &str, packet: &Packet) -> Self {
        Self {
            t,
            src: src.to_string(),
            seq: packet.seq,
            ts_ns: packet.timestamp_ns,
            data: packet.telemetry,
        }
    }

    pub fn packet(&self) -> Packet {
        Packet {
            seq: self.seq,
            timestamp_ns: self.ts_ns,
            telemetry: self.data,
        }
    }
}

/// Parse one JSONL line.
pub fn parse_line(line: &str) -> Result<Record, serde_json::Error> {
    serde_json::from_str(line)
}

/// Appends records to a JSONL file.
pub struct Recorder {
    out: BufWriter<File>,
    pub count: u64,
}

impl Recorder {
    /// Open `path` for appending, creating it when needed.
    pub fn create(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("cannot open record file {}", path.display()))?;
        Ok(Self {
            out: BufWriter::new(file),
            count: 0,
        })
    }

    pub fn write(&mut self, record: &Record) -> Result<()> {
        serde_json::to_writer(&mut self.out, record)?;
        self.out.write_all(b"\n")?;
        self.out.flush()?;
        self.count += 1;
        Ok(())
    }
}

/// Replay a recording on a background thread, honouring recorded timing.
/// Gaps are divided by `speed` and capped at two seconds.
pub fn spawn_replay(path: &Path, speed: f64, tx: Sender<Msg>) -> Result<JoinHandle<()>> {
    let file =
        File::open(path).with_context(|| format!("cannot open replay file {}", path.display()))?;
    Ok(thread::spawn(move || {
        let mut prev_t: Option<f64> = None;
        let mut sent = 0u64;
        let mut bad = 0u64;
        for line in BufReader::new(file).lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let Ok(rec) = parse_line(&line) else {
                bad += 1;
                continue;
            };
            if let Some(prev) = prev_t {
                let gap = (rec.t - prev) / speed;
                if gap.is_finite() && gap > 0.0 {
                    thread::sleep(Duration::from_secs_f64(gap.min(2.0)));
                }
            }
            prev_t = Some(rec.t);
            let msg = Msg::Packet {
                src: rec.src.clone(),
                packet: rec.packet(),
            };
            if tx.send(msg).is_err() {
                return;
            }
            sent += 1;
        }
        let _ = tx.send(Msg::Status(format!(
            "replay finished: {sent} packets, {bad} unreadable lines"
        )));
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet() -> Packet {
        Packet {
            seq: Some(4),
            timestamp_ns: 77,
            telemetry: Telemetry {
                cpu_percent: 33.5,
                mem_total_kb: 1024,
                rx_bytes: 99,
                ..Telemetry::default()
            },
        }
    }

    #[test]
    fn record_round_trip() {
        let rec = Record::new(1.5, "10.0.0.1:9", &packet());
        let line = serde_json::to_string(&rec).unwrap();
        assert!(!line.contains('\n'));
        let back = parse_line(&line).unwrap();
        assert_eq!(back, rec);
        assert_eq!(back.packet(), packet());
    }

    #[test]
    fn rejects_garbage_lines() {
        assert!(parse_line("not json").is_err());
        assert!(parse_line("{\"t\": 1.0}").is_err());
    }

    #[test]
    fn recorder_writes_one_line_per_record() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("tld-test-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let mut r = Recorder::create(&path).unwrap();
            r.write(&Record::new(0.0, "a", &packet())).unwrap();
            r.write(&Record::new(1.0, "a", &packet())).unwrap();
            assert_eq!(r.count, 2);
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let records: Vec<Record> = text.lines().map(|l| parse_line(l).unwrap()).collect();
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].t, 1.0);
    }

    #[test]
    fn replay_sends_packets_then_status() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("tld-replay-{}.jsonl", std::process::id()));
        let lines = [
            serde_json::to_string(&Record::new(0.0, "a", &packet())).unwrap(),
            "garbage".to_string(),
            serde_json::to_string(&Record::new(0.0, "b", &packet())).unwrap(),
        ];
        std::fs::write(&path, lines.join("\n")).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = spawn_replay(&path, 1.0, tx).unwrap();
        handle.join().unwrap();
        let _ = std::fs::remove_file(&path);
        let msgs: Vec<Msg> = rx.try_iter().collect();
        assert_eq!(msgs.len(), 3);
        assert!(matches!(&msgs[0], Msg::Packet { src, .. } if src == "a"));
        assert!(matches!(&msgs[1], Msg::Packet { src, .. } if src == "b"));
        assert!(matches!(&msgs[2], Msg::Status(s) if s.contains("1 unreadable")));
    }
}
