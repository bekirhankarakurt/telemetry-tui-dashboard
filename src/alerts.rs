//! Alert thresholds and evaluation.

use crate::format::{fmt_rate, fmt_secs};
use crate::stats::Source;

/// Severity of a value relative to its threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Crit,
}

/// Which metric an alert refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    Cpu,
    Mem,
    Swap,
    Load,
    Net,
    Stale,
}

/// A threshold that is currently exceeded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alert {
    pub kind: AlertKind,
    pub value: f64,
    pub limit: f64,
}

impl Alert {
    pub fn describe(&self) -> String {
        match self.kind {
            AlertKind::Cpu => format!("CPU {:.1}% >= {:.0}%", self.value, self.limit),
            AlertKind::Mem => format!("MEM {:.1}% >= {:.0}%", self.value, self.limit),
            AlertKind::Swap => format!("SWAP {:.1}% >= {:.0}%", self.value, self.limit),
            AlertKind::Load => format!("LOAD1 {:.2} >= {:.2}", self.value, self.limit),
            AlertKind::Net => format!("NET {} >= {}", fmt_rate(self.value), fmt_rate(self.limit)),
            AlertKind::Stale => format!(
                "NO DATA for {} (limit {})",
                fmt_secs(self.value),
                fmt_secs(self.limit)
            ),
        }
    }
}

/// User-configurable alert limits. `None` disables a check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub cpu_pct: Option<f64>,
    pub mem_pct: Option<f64>,
    pub swap_pct: Option<f64>,
    pub load1: Option<f64>,
    /// Combined rx or tx rate in bytes per second.
    pub net_bytes_per_s: Option<f64>,
    pub stale_secs: Option<f64>,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            cpu_pct: Some(90.0),
            mem_pct: Some(90.0),
            swap_pct: Some(80.0),
            load1: None,
            net_bytes_per_s: None,
            stale_secs: Some(5.0),
        }
    }
}

fn level(limit: Option<f64>, value: f64) -> Level {
    match limit {
        Some(l) if value >= l => Level::Crit,
        Some(l) if value >= l * 0.8 => Level::Warn,
        _ => Level::Ok,
    }
}

impl Thresholds {
    pub fn cpu_level(&self, v: f64) -> Level {
        level(self.cpu_pct, v)
    }

    pub fn mem_level(&self, v: f64) -> Level {
        level(self.mem_pct, v)
    }

    pub fn swap_level(&self, v: f64) -> Level {
        level(self.swap_pct, v)
    }

    pub fn load_level(&self, v: f64) -> Level {
        level(self.load1, v)
    }

    pub fn net_level(&self, v: f64) -> Level {
        level(self.net_bytes_per_s, v)
    }

    /// All thresholds currently exceeded by `src` at time `now`.
    pub fn evaluate(&self, src: &Source, now: f64) -> Vec<Alert> {
        let mut out = Vec::new();
        let Some(t) = src.latest.as_ref() else {
            return out;
        };
        let mut check = |kind, limit: Option<f64>, value: f64| {
            if let Some(limit) = limit {
                if value >= limit {
                    out.push(Alert { kind, value, limit });
                }
            }
        };
        check(AlertKind::Cpu, self.cpu_pct, f64::from(t.cpu_percent));
        check(AlertKind::Mem, self.mem_pct, t.mem_used_pct());
        check(AlertKind::Swap, self.swap_pct, t.swap_used_pct());
        check(AlertKind::Load, self.load1, f64::from(t.load1));
        check(
            AlertKind::Net,
            self.net_bytes_per_s,
            src.rx_now().max(src.tx_now()),
        );
        if let Some(limit) = self.stale_secs {
            let age = now - src.last_seen;
            if age > limit {
                out.push(Alert {
                    kind: AlertKind::Stale,
                    value: age,
                    limit,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::{Packet, Telemetry};

    fn source_with(t: Telemetry) -> Source {
        let mut s = Source::new("x", 8);
        s.ingest(
            1.0,
            &Packet {
                seq: Some(1),
                timestamp_ns: 0,
                telemetry: t,
            },
        );
        s
    }

    #[test]
    fn levels() {
        let th = Thresholds::default();
        assert_eq!(th.cpu_level(50.0), Level::Ok);
        assert_eq!(th.cpu_level(75.0), Level::Warn);
        assert_eq!(th.cpu_level(90.0), Level::Crit);
        assert_eq!(th.load_level(100.0), Level::Ok);
    }

    #[test]
    fn evaluates_exceeded_limits_only() {
        let t = Telemetry {
            cpu_percent: 95.0,
            mem_total_kb: 100,
            mem_avail_kb: 5,
            swap_total_kb: 100,
            swap_free_kb: 90,
            load1: 3.0,
            ..Telemetry::default()
        };
        let src = source_with(t);
        let th = Thresholds {
            load1: Some(2.0),
            ..Thresholds::default()
        };
        let kinds: Vec<_> = th.evaluate(&src, 1.0).iter().map(|a| a.kind).collect();
        assert_eq!(kinds, vec![AlertKind::Cpu, AlertKind::Mem, AlertKind::Load]);
    }

    #[test]
    fn disabled_checks_never_fire() {
        let t = Telemetry {
            cpu_percent: 100.0,
            ..Telemetry::default()
        };
        let src = source_with(t);
        let th = Thresholds {
            cpu_pct: None,
            mem_pct: None,
            swap_pct: None,
            load1: None,
            net_bytes_per_s: None,
            stale_secs: None,
        };
        assert!(th.evaluate(&src, 1000.0).is_empty());
    }

    #[test]
    fn stale_source_alerts() {
        let src = source_with(Telemetry::default());
        let th = Thresholds::default();
        assert!(th.evaluate(&src, 3.0).is_empty());
        let alerts = th.evaluate(&src, 20.0);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::Stale);
        assert!(alerts[0].describe().starts_with("NO DATA"));
    }

    #[test]
    fn no_data_means_no_alerts() {
        let src = Source::new("x", 4);
        assert!(Thresholds::default().evaluate(&src, 100.0).is_empty());
    }
}
