//! Command-line interface.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Parser;

use crate::alerts::Thresholds;
use crate::app::LayoutMode;

/// Live terminal dashboard for the embedded-telemetry-daemon UDP downlink.
#[derive(Debug, Parser)]
#[command(name = "telemetry-tui-dashboard", version, about)]
pub struct Cli {
    /// UDP address to listen on.
    #[arg(short, long, default_value = "0.0.0.0:9000", value_name = "ADDR")]
    pub bind: SocketAddr,

    /// Generate simulated telemetry on loopback instead of waiting for a daemon.
    #[arg(long, conflicts_with = "replay")]
    pub demo: bool,

    /// Number of simulated senders in demo mode.
    #[arg(long, default_value_t = 2, value_name = "N")]
    pub demo_sources: usize,

    /// Packets per second per simulated sender in demo mode.
    #[arg(long, default_value_t = 4.0, value_name = "HZ")]
    pub demo_rate: f64,

    /// Append every received sample to a JSONL file.
    #[arg(long, value_name = "FILE", conflicts_with = "replay")]
    pub record: Option<PathBuf>,

    /// Replay a JSONL recording instead of listening on the network.
    #[arg(long, value_name = "FILE")]
    pub replay: Option<PathBuf>,

    /// Replay speed multiplier.
    #[arg(long, default_value_t = 1.0, value_name = "X")]
    pub replay_speed: f64,

    /// Samples kept per chart.
    #[arg(long, default_value_t = 120, value_name = "N")]
    pub history: usize,

    /// Screen refresh interval in milliseconds.
    #[arg(long, default_value_t = 100, value_name = "MS")]
    pub refresh_ms: u64,

    /// Initial layout for multiple sources.
    #[arg(long, value_enum, default_value_t = LayoutMode::Tabs)]
    pub layout: LayoutMode,

    /// Highlight CPU usage at or above this percentage (0 disables).
    #[arg(long, default_value_t = 90.0, value_name = "PCT")]
    pub alert_cpu: f64,

    /// Highlight memory usage at or above this percentage (0 disables).
    #[arg(long, default_value_t = 90.0, value_name = "PCT")]
    pub alert_mem: f64,

    /// Highlight swap usage at or above this percentage (0 disables).
    #[arg(long, default_value_t = 80.0, value_name = "PCT")]
    pub alert_swap: f64,

    /// Highlight 1-minute load average at or above this value (0 disables).
    #[arg(long, default_value_t = 0.0, value_name = "LOAD")]
    pub alert_load: f64,

    /// Highlight rx or tx rate at or above this many MB/s (0 disables).
    #[arg(long, default_value_t = 0.0, value_name = "MBPS")]
    pub alert_net: f64,

    /// Flag a source that sent nothing for this many seconds (0 disables).
    #[arg(long, default_value_t = 5.0, value_name = "SECS")]
    pub alert_stale: f64,
}

fn opt(v: f64) -> Option<f64> {
    if v > 0.0 {
        Some(v)
    } else {
        None
    }
}

impl Cli {
    /// Check value ranges that clap cannot express.
    pub fn validate(&self) -> Result<()> {
        if !(10..=100_000).contains(&self.history) {
            bail!("--history must be between 10 and 100000");
        }
        if !(self.replay_speed.is_finite() && self.replay_speed > 0.0) {
            bail!("--replay-speed must be greater than zero");
        }
        if !(self.demo_rate.is_finite() && self.demo_rate > 0.0 && self.demo_rate <= 1000.0) {
            bail!("--demo-rate must be between 0 and 1000");
        }
        if !(1..=16).contains(&self.demo_sources) {
            bail!("--demo-sources must be between 1 and 16");
        }
        if self.refresh_ms < 10 {
            bail!("--refresh-ms must be at least 10");
        }
        let limits = [
            self.alert_cpu,
            self.alert_mem,
            self.alert_swap,
            self.alert_load,
            self.alert_net,
            self.alert_stale,
        ];
        if limits.iter().any(|v| !v.is_finite() || *v < 0.0) {
            bail!("alert thresholds must be finite and not negative");
        }
        Ok(())
    }

    pub fn thresholds(&self) -> Thresholds {
        Thresholds {
            cpu_pct: opt(self.alert_cpu),
            mem_pct: opt(self.alert_mem),
            swap_pct: opt(self.alert_swap),
            load1: opt(self.alert_load),
            net_bytes_per_s: opt(self.alert_net * 1_000_000.0),
            stale_secs: opt(self.alert_stale),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["telemetry-tui-dashboard"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full)
    }

    #[test]
    fn defaults_are_valid() {
        let cli = parse(&[]).unwrap();
        cli.validate().unwrap();
        assert_eq!(cli.bind.port(), 9000);
        let th = cli.thresholds();
        assert_eq!(th.cpu_pct, Some(90.0));
        assert_eq!(th.load1, None);
    }

    #[test]
    fn alert_flags_and_disable() {
        let cli = parse(&["--alert-cpu", "75", "--alert-mem", "0", "--alert-net", "2"]).unwrap();
        let th = cli.thresholds();
        assert_eq!(th.cpu_pct, Some(75.0));
        assert_eq!(th.mem_pct, None);
        assert_eq!(th.net_bytes_per_s, Some(2_000_000.0));
    }

    #[test]
    fn rejects_bad_values() {
        assert!(parse(&["--history", "2"]).unwrap().validate().is_err());
        assert!(parse(&["--replay-speed", "0"]).unwrap().validate().is_err());
        assert!(parse(&["--alert-cpu=-5"]).unwrap().validate().is_err());
        assert!(parse(&["--bind", "nonsense"]).is_err());
    }

    #[test]
    fn demo_conflicts_with_replay() {
        assert!(parse(&["--demo", "--replay", "x.jsonl"]).is_err());
    }
}
