//! Built-in packet generator that sends realistic telemetry over loopback UDP.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::packet::{encode, encode_json, Telemetry};

/// Small xorshift generator; the demo needs no cryptographic quality.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform value in `[0, 1)`.
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

struct Sim {
    socket: UdpSocket,
    seq: u32,
    phase: f64,
    mem_total_kb: u64,
    swap_total_kb: u64,
    uptime_base: u64,
    rx_bytes: u64,
    tx_bytes: u64,
    rx_packets: u64,
    tx_packets: u64,
    held: Option<Vec<u8>>,
}

impl Sim {
    fn sample(&mut self, t: f64, dt: f64, rng: &mut Rng) -> Telemetry {
        let wave = (t * 0.35 + self.phase).sin();
        let mut cpu = 45.0 + 30.0 * wave + (rng.unit() - 0.5) * 8.0;
        if rng.unit() < 0.02 {
            cpu = 92.0 + rng.unit() * 7.0;
        }
        let cpu = cpu.clamp(1.0, 100.0);

        let mem_used = 0.55 + 0.12 * (t * 0.05 + self.phase).sin() + (rng.unit() - 0.5) * 0.02;
        let mem_avail = (self.mem_total_kb as f64 * (1.0 - mem_used)) as u64;
        let swap_used = (0.08 + 0.05 * (t * 0.02 + self.phase).sin()).clamp(0.0, 1.0);
        let swap_free = (self.swap_total_kb as f64 * (1.0 - swap_used)) as u64;

        let rx_rate = 400_000.0 * (1.0 + (t * 0.5 + self.phase).sin()) + rng.unit() * 80_000.0;
        let tx_rate = 120_000.0 * (1.0 + (t * 0.8 + self.phase).cos()) + rng.unit() * 30_000.0;
        self.rx_bytes += (rx_rate * dt) as u64;
        self.tx_bytes += (tx_rate * dt) as u64;
        self.rx_packets += (rx_rate * dt / 900.0) as u64;
        self.tx_packets += (tx_rate * dt / 700.0) as u64;

        let load = 0.2 + cpu / 100.0 * 3.0;
        Telemetry {
            cpu_percent: cpu as f32,
            mem_total_kb: self.mem_total_kb,
            mem_avail_kb: mem_avail,
            swap_total_kb: self.swap_total_kb,
            swap_free_kb: swap_free,
            uptime_s: self.uptime_base + t as u64,
            load1: load as f32,
            load5: (load * 0.9) as f32,
            load15: (load * 0.8) as f32,
            rx_bytes: self.rx_bytes,
            tx_bytes: self.tx_bytes,
            rx_packets: self.rx_packets,
            tx_packets: self.tx_packets,
        }
    }
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_nanos()).ok())
        .unwrap_or(0)
}

/// Where the demo should send packets, given the address the dashboard listens on.
pub fn loopback_target(listen: SocketAddr) -> SocketAddr {
    let ip = match listen.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        other => other,
    };
    SocketAddr::new(ip, listen.port())
}

/// Start `sources` simulated daemons sending `hz` packets per second each.
/// The stream includes occasional loss, reordering, corruption and JSON packets.
pub fn spawn(listen: SocketAddr, sources: usize, hz: f64) -> Result<JoinHandle<()>> {
    let target = loopback_target(listen);
    let local: SocketAddr = SocketAddr::new(target.ip(), 0);
    let mut sims = Vec::with_capacity(sources);
    for i in 0..sources {
        let socket = UdpSocket::bind(local).context("cannot open demo sender socket")?;
        sims.push(Sim {
            socket,
            seq: 1000 * (i as u32 + 1),
            phase: i as f64 * 1.7,
            mem_total_kb: (4 + 4 * (i as u64 % 4)) * 1024 * 1024,
            swap_total_kb: 2 * 1024 * 1024,
            uptime_base: 3_600 * (i as u64 + 5),
            rx_bytes: 10_000_000,
            tx_bytes: 4_000_000,
            rx_packets: 20_000,
            tx_packets: 15_000,
            held: None,
        });
    }
    let period = Duration::from_secs_f64(1.0 / hz.max(0.1));
    Ok(thread::spawn(move || {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let start = Instant::now();
        let mut last = 0.0;
        loop {
            thread::sleep(period);
            let t = start.elapsed().as_secs_f64();
            let dt = t - last;
            last = t;
            for sim in &mut sims {
                let sample = sim.sample(t, dt, &mut rng);
                let seq = sim.seq;
                sim.seq = sim.seq.wrapping_add(1);

                let roll = rng.unit();
                if roll < 0.03 {
                    continue; // simulated loss
                }
                let mut bytes = if rng.unit() < 0.1 {
                    encode_json(seq, now_ns(), &sample).into_bytes()
                } else {
                    encode(seq, now_ns(), &sample)
                };
                if roll < 0.045 && bytes.first() == Some(&b'T') {
                    let idx = 24 + (rng.next_u64() % 60) as usize;
                    bytes[idx] ^= 0x5A; // simulated corruption
                }
                if roll > 0.97 && sim.held.is_none() {
                    sim.held = Some(bytes); // simulated reordering
                    continue;
                }
                if sim.socket.send_to(&bytes, target).is_err() {
                    return;
                }
                if let Some(late) = sim.held.take() {
                    if sim.socket.send_to(&late, target).is_err() {
                        return;
                    }
                }
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_replaces_unspecified() {
        let t = loopback_target("0.0.0.0:9000".parse().unwrap());
        assert_eq!(t, "127.0.0.1:9000".parse().unwrap());
        let t = loopback_target("[::]:9100".parse().unwrap());
        assert_eq!(t, "[::1]:9100".parse().unwrap());
        let t = loopback_target("192.168.1.5:9000".parse().unwrap());
        assert_eq!(t, "192.168.1.5:9000".parse().unwrap());
    }

    #[test]
    fn rng_is_in_unit_range() {
        let mut r = Rng(12345);
        for _ in 0..1000 {
            let v = r.unit();
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn samples_are_plausible() {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut sim = Sim {
            socket,
            seq: 0,
            phase: 0.0,
            mem_total_kb: 8 * 1024 * 1024,
            swap_total_kb: 1024 * 1024,
            uptime_base: 10,
            rx_bytes: 0,
            tx_bytes: 0,
            rx_packets: 0,
            tx_packets: 0,
            held: None,
        };
        let mut rng = Rng(99);
        let a = sim.sample(1.0, 0.25, &mut rng);
        let b = sim.sample(1.25, 0.25, &mut rng);
        assert!((0.0..=100.0).contains(&f64::from(a.cpu_percent)));
        assert!(a.mem_avail_kb < a.mem_total_kb);
        assert!(b.rx_bytes >= a.rx_bytes);
    }
}
