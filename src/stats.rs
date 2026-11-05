//! Ring buffers, sequence tracking and per-source state.

use std::collections::VecDeque;

use crate::packet::{Packet, Telemetry};

/// Fixed-capacity buffer that discards the oldest item when full.
#[derive(Debug, Clone)]
pub struct Ring<T> {
    items: VecDeque<T>,
    cap: usize,
}

impl<T> Ring<T> {
    /// Create a ring holding at most `cap` items (minimum 1).
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(1);
        Self {
            items: VecDeque::with_capacity(cap),
            cap,
        }
    }

    pub fn push(&mut self, item: T) {
        if self.items.len() == self.cap {
            self.items.pop_front();
        }
        self.items.push_back(item);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    pub fn last(&self) -> Option<&T> {
        self.items.back()
    }

    /// Iterate from oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// Minimum, mean and maximum of a series.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    pub min: f64,
    pub avg: f64,
    pub max: f64,
}

impl Ring<f64> {
    /// Summary of the buffered values, or `None` when empty.
    pub fn summary(&self) -> Option<Summary> {
        if self.items.is_empty() {
            return None;
        }
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        let mut sum = 0.0;
        for &v in &self.items {
            min = min.min(v);
            max = max.max(v);
            sum += v;
        }
        Some(Summary {
            min,
            avg: sum / self.items.len() as f64,
            max,
        })
    }
}

/// Outcome of observing one sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeqEvent {
    /// First packet, or a packet without a sequence number.
    Fresh,
    /// The expected next packet.
    InOrder,
    /// A gap: this many packets are missing before this one.
    Gap(u32),
    /// Same sequence number as the newest packet.
    Duplicate,
    /// Older than the newest packet.
    Reordered,
}

/// Tracks received, lost, reordered and duplicate packets by sequence number.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeqTracker {
    pub received: u64,
    pub lost: u64,
    pub reordered: u64,
    pub duplicates: u64,
    newest: Option<u32>,
}

impl SeqTracker {
    /// Record one packet. Sequence numbers use wrapping 32-bit arithmetic.
    /// A reordered packet that fills an earlier gap reduces the lost count.
    pub fn observe(&mut self, seq: Option<u32>) -> SeqEvent {
        self.received += 1;
        let Some(seq) = seq else {
            return SeqEvent::Fresh;
        };
        let Some(newest) = self.newest else {
            self.newest = Some(seq);
            return SeqEvent::Fresh;
        };
        let diff = seq.wrapping_sub(newest) as i32;
        if diff > 0 {
            self.newest = Some(seq);
            if diff == 1 {
                SeqEvent::InOrder
            } else {
                let missing = (diff - 1) as u32;
                self.lost += u64::from(missing);
                SeqEvent::Gap(missing)
            }
        } else if diff == 0 {
            self.duplicates += 1;
            SeqEvent::Duplicate
        } else {
            self.reordered += 1;
            self.lost = self.lost.saturating_sub(1);
            SeqEvent::Reordered
        }
    }

    /// Percentage of expected packets that were lost.
    pub fn loss_pct(&self) -> f64 {
        let expected = self.received + self.lost;
        if expected == 0 {
            0.0
        } else {
            self.lost as f64 / expected as f64 * 100.0
        }
    }

    pub fn last_seq(&self) -> Option<u32> {
        self.newest
    }
}

/// Everything known about one telemetry sender.
#[derive(Debug, Clone)]
pub struct Source {
    pub addr: String,
    pub latest: Option<Telemetry>,
    pub cpu: Ring<f64>,
    pub mem: Ring<f64>,
    pub rx_rate: Ring<f64>,
    pub tx_rate: Ring<f64>,
    pub seq: SeqTracker,
    pub crc_errors: u64,
    pub last_seen: f64,
    prev: Option<(f64, u64, u64)>,
}

impl Source {
    pub fn new(addr: impl Into<String>, history: usize) -> Self {
        Self {
            addr: addr.into(),
            latest: None,
            cpu: Ring::new(history),
            mem: Ring::new(history),
            rx_rate: Ring::new(history),
            tx_rate: Ring::new(history),
            seq: SeqTracker::default(),
            crc_errors: 0,
            last_seen: 0.0,
            prev: None,
        }
    }

    /// Ingest a packet received `now` seconds after application start.
    /// Duplicate and reordered packets only update the counters.
    pub fn ingest(&mut self, now: f64, packet: &Packet) {
        let event = self.seq.observe(packet.seq);
        if matches!(event, SeqEvent::Duplicate | SeqEvent::Reordered) {
            return;
        }
        let t = &packet.telemetry;
        if let Some((prev_t, prev_rx, prev_tx)) = self.prev {
            let dt = now - prev_t;
            if dt > 1e-3 {
                self.rx_rate.push(counter_rate(prev_rx, t.rx_bytes, dt));
                self.tx_rate.push(counter_rate(prev_tx, t.tx_bytes, dt));
            }
        }
        self.prev = Some((now, t.rx_bytes, t.tx_bytes));
        self.cpu.push(f64::from(t.cpu_percent).clamp(0.0, 100.0));
        self.mem.push(t.mem_used_pct());
        self.latest = Some(*t);
        self.last_seen = now;
    }

    pub fn rx_now(&self) -> f64 {
        self.rx_rate.last().copied().unwrap_or(0.0)
    }

    pub fn tx_now(&self) -> f64 {
        self.tx_rate.last().copied().unwrap_or(0.0)
    }
}

/// Bytes per second between two counter readings; a counter reset yields 0.
pub fn counter_rate(prev: u64, now: u64, dt: f64) -> f64 {
    if now < prev || dt <= 0.0 {
        0.0
    } else {
        (now - prev) as f64 / dt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_drops_oldest() {
        let mut r = Ring::new(3);
        assert!(r.is_empty());
        for i in 1..=5 {
            r.push(i);
        }
        assert_eq!(r.len(), 3);
        assert_eq!(r.capacity(), 3);
        assert_eq!(r.iter().copied().collect::<Vec<_>>(), vec![3, 4, 5]);
        assert_eq!(r.last(), Some(&5));
        r.clear();
        assert!(r.is_empty());
        assert_eq!(r.last(), None);
    }

    #[test]
    fn ring_minimum_capacity_is_one() {
        let mut r = Ring::new(0);
        r.push(1);
        r.push(2);
        assert_eq!(r.len(), 1);
        assert_eq!(r.last(), Some(&2));
    }

    #[test]
    fn ring_summary() {
        let mut r: Ring<f64> = Ring::new(4);
        assert_eq!(r.summary(), None);
        for v in [10.0, 20.0, 30.0] {
            r.push(v);
        }
        let s = r.summary().unwrap();
        assert_eq!(s.min, 10.0);
        assert_eq!(s.max, 30.0);
        assert!((s.avg - 20.0).abs() < 1e-9);
    }

    #[test]
    fn sequence_in_order() {
        let mut t = SeqTracker::default();
        assert_eq!(t.observe(Some(10)), SeqEvent::Fresh);
        assert_eq!(t.observe(Some(11)), SeqEvent::InOrder);
        assert_eq!(t.observe(Some(12)), SeqEvent::InOrder);
        assert_eq!(
            (t.received, t.lost, t.reordered, t.duplicates),
            (3, 0, 0, 0)
        );
        assert_eq!(t.last_seq(), Some(12));
    }

    #[test]
    fn sequence_gap_counts_loss() {
        let mut t = SeqTracker::default();
        t.observe(Some(1));
        assert_eq!(t.observe(Some(5)), SeqEvent::Gap(3));
        assert_eq!(t.lost, 3);
        assert_eq!(t.received, 2);
        assert!((t.loss_pct() - 60.0).abs() < 1e-9);
    }

    #[test]
    fn sequence_reorder_fills_gap() {
        let mut t = SeqTracker::default();
        t.observe(Some(1));
        t.observe(Some(3));
        assert_eq!(t.lost, 1);
        assert_eq!(t.observe(Some(2)), SeqEvent::Reordered);
        assert_eq!(t.lost, 0);
        assert_eq!(t.reordered, 1);
        assert_eq!(t.last_seq(), Some(3));
    }

    #[test]
    fn sequence_duplicate() {
        let mut t = SeqTracker::default();
        t.observe(Some(7));
        assert_eq!(t.observe(Some(7)), SeqEvent::Duplicate);
        assert_eq!(t.duplicates, 1);
        assert_eq!(t.lost, 0);
    }

    #[test]
    fn sequence_wraps_around() {
        let mut t = SeqTracker::default();
        t.observe(Some(u32::MAX - 1));
        assert_eq!(t.observe(Some(u32::MAX)), SeqEvent::InOrder);
        assert_eq!(t.observe(Some(0)), SeqEvent::InOrder);
        assert_eq!(t.observe(Some(2)), SeqEvent::Gap(1));
        assert_eq!(t.lost, 1);
    }

    #[test]
    fn missing_sequence_is_counted_but_not_tracked() {
        let mut t = SeqTracker::default();
        assert_eq!(t.observe(None), SeqEvent::Fresh);
        assert_eq!(t.observe(None), SeqEvent::Fresh);
        assert_eq!((t.received, t.lost, t.reordered), (2, 0, 0));
        assert_eq!(t.loss_pct(), 0.0);
    }

    #[test]
    fn counter_rate_handles_reset() {
        assert_eq!(counter_rate(100, 300, 2.0), 100.0);
        assert_eq!(counter_rate(300, 100, 2.0), 0.0);
        assert_eq!(counter_rate(1, 2, 0.0), 0.0);
    }

    fn pkt(seq: u32, rx: u64, cpu: f32) -> Packet {
        Packet {
            seq: Some(seq),
            timestamp_ns: 0,
            telemetry: Telemetry {
                cpu_percent: cpu,
                mem_total_kb: 100,
                mem_avail_kb: 40,
                rx_bytes: rx,
                tx_bytes: rx / 2,
                ..Telemetry::default()
            },
        }
    }

    #[test]
    fn source_computes_rates_and_history() {
        let mut s = Source::new("1.2.3.4:5", 10);
        s.ingest(0.0, &pkt(1, 1000, 10.0));
        assert!(s.rx_rate.is_empty());
        s.ingest(2.0, &pkt(2, 5000, 150.0));
        assert_eq!(s.rx_now(), 2000.0);
        assert_eq!(s.tx_now(), 1000.0);
        assert_eq!(s.cpu.iter().copied().collect::<Vec<_>>(), vec![10.0, 100.0]);
        assert_eq!(s.mem.last().copied(), Some(60.0));
        assert_eq!(s.last_seen, 2.0);
    }

    #[test]
    fn source_ignores_reordered_telemetry() {
        let mut s = Source::new("a", 10);
        s.ingest(0.0, &pkt(1, 0, 10.0));
        s.ingest(1.0, &pkt(3, 100, 20.0));
        s.ingest(1.5, &pkt(2, 50, 99.0));
        assert_eq!(s.seq.reordered, 1);
        assert_eq!(s.seq.lost, 0);
        assert_eq!(s.cpu.len(), 2);
        assert_eq!(s.latest.unwrap().cpu_percent, 20.0);
    }
}
