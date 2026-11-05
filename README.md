# telemetry-tui-dashboard

An htop-style terminal dashboard that turns the embedded-telemetry-daemon UDP downlink into live CPU, memory and network charts.

[![CI](https://github.com/bekirhankarakurt/telemetry-tui-dashboard/actions/workflows/ci.yml/badge.svg)](https://github.com/bekirhankarakurt/telemetry-tui-dashboard/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/bekirhankarakurt/telemetry-tui-dashboard)](https://github.com/bekirhankarakurt/telemetry-tui-dashboard/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.74%2B-orange.svg)](https://www.rust-lang.org)

## What and why

- Listens on a UDP port and decodes the daemon's binary telemetry packets, plus one-line JSON packets with the same field names.
- Shows several senders at once, either as tabs or side by side, keyed by the sender address.
- Draws CPU and memory sparklines, usage gauges, and network rx/tx rate charts.
- Keeps honest packet statistics: received, lost (from sequence gaps), reordered, duplicated and CRC errors.
- Highlights values that cross thresholds you set on the command line.
- Pauses the view, records samples to JSONL and replays recordings later.
- Runs standalone with `--demo`, which generates realistic traffic including loss, corruption and reordering.
- Restores the terminal on exit, on `Ctrl-C`, and after a panic.
- Written in safe Rust, with no `unsafe` code.

## Screenshot

```
┌────────────────────────────────────────────────────────────────────────────┐
│telemetry-tui-dashboard | demo on 0.0.0.0:9000 | sources 2 | packets 1840 | │
│crc errors 21 | other errors 0                                              │
└────────────────────────────────────────────────────────────────────────────┘
┌ Sources ───────────────────────────────────────────────────────────────────┐
│ 1 127.0.0.1:51734 │ ! 2 127.0.0.1:51735                                    │
└────────────────────────────────────────────────────────────────────────────┘
┌ 127.0.0.1:51734 ───────────────────────────────────────────────────────────┐
│┌ System ─────────────────────────┐┌ Packets ───────────────────────────────┐│
││Uptime    05:12:44               ││Received   920                          ││
││Load      1.43  1.29  1.14       ││Lost       27 (2.85%)                   ││
││Memory    4.4 GiB used of 8.0 GiB││Reordered  2                            ││
││Swap      172.4 MiB used of 2 GiB││Duplicates 0                            ││
││Net rx    612.3 KiB/s            ││CRC errors 11                           ││
││Net tx    198.0 KiB/s            ││Last seq   1946   seen 0.2s ago         ││
│└─────────────────────────────────┘└────────────────────────────────────────┘│
│┌ CPU 62.4% ───────────┐┌ MEM 55.0% ──────────┐┌ SWAP 8.2% ─────────────────┐│
││██████████████        ││████████████         ││██                          ││
│└──────────────────────┘└─────────────────────┘└────────────────────────────┘│
│┌ CPU min 18% avg 47% max 96% ────┐┌ MEM min 49% avg 56% max 66% ───────────┐│
││   ▂▃▅▆▇█▇▅▃▂▁▁▂▃▅▆▇▆▄▃▂▁▂▃▅▆▇   ││ ▄▄▅▅▅▆▆▆▆▅▅▅▄▄▄▄▅▅▅▆▆▆▆▅▅▅▄▄▄▄▅▅▅▆▆▆   ││
│└─────────────────────────────────┘└────────────────────────────────────────┘│
│┌ Network rx 612.3 KiB/s  tx 198.0 KiB/s ───────────────────────────────────┐│
││ 1.1 MiB│  ⣀⡠⠔⠊⠉⠑⠢⣀        ⣀⠤⠒⠉⠑⠢⢄                                       ││
││        │⠔⠊      ⠉⠒⠤⣀⣀⠔⠊⠁       ⠉⠒⠤⡀   rx                                ││
││      0 └──────────────────────────────────────────────────────────────────┘│
│└────────────────────────────────────────────────────────────────────────────┘│
│┌ Alerts ────────────────────────────────────────────────────────────────────┐│
││alerts: none                                                                ││
│└────────────────────────────────────────────────────────────────────────────┘│
└────────────────────────────────────────────────────────────────────────────┘
 q quit | Tab/1-9 source | p pause | r record | v layout | c clear | ? help
```

A source whose values cross a threshold gets a red border and a `!` in its tab, and the offending value turns red, for example `CPU 96.2% >= 90%`.

## Quick start

```sh
# Build and run with simulated traffic
cargo run --release -- --demo

# Listen for a real daemon on UDP port 9000
cargo run --release -- --bind 0.0.0.0:9000

# Record a session, then replay it at double speed
cargo run --release -- --demo --record session.jsonl
cargo run --release -- --replay session.jsonl --replay-speed 2

# Alert when CPU reaches 80%, memory 85%, or any link exceeds 5 MB/s
cargo run --release -- --alert-cpu 80 --alert-mem 85 --alert-net 5
```

Prebuilt Linux binaries are attached to each [release](https://github.com/bekirhankarakurt/telemetry-tui-dashboard/releases) together with a `SHA256SUMS` file.

## CLI options

| Option | Default | Description |
| --- | --- | --- |
| `-b, --bind <ADDR>` | `0.0.0.0:9000` | UDP address to listen on |
| `--demo` | off | Generate simulated telemetry on loopback |
| `--demo-sources <N>` | `2` | Number of simulated senders (1 to 16) |
| `--demo-rate <HZ>` | `4` | Packets per second per simulated sender |
| `--record <FILE>` | none | Append every received sample to a JSONL file |
| `--replay <FILE>` | none | Replay a JSONL recording instead of listening |
| `--replay-speed <X>` | `1` | Replay speed multiplier |
| `--history <N>` | `120` | Samples kept per chart (10 to 100000) |
| `--refresh-ms <MS>` | `100` | Screen refresh interval |
| `--layout <tabs\|side-by-side>` | `tabs` | Initial layout for multiple sources |
| `--alert-cpu <PCT>` | `90` | CPU alert threshold, `0` disables |
| `--alert-mem <PCT>` | `90` | Memory alert threshold, `0` disables |
| `--alert-swap <PCT>` | `80` | Swap alert threshold, `0` disables |
| `--alert-load <LOAD>` | off | 1-minute load average alert threshold |
| `--alert-net <MBPS>` | off | Alert when rx or tx reaches this many MB/s |
| `--alert-stale <SECS>` | `5` | Alert when a source sends nothing for this long |

`--demo` and `--record` cannot be combined with `--replay`.

## Keyboard controls

| Key | Action |
| --- | --- |
| `Tab`, `Right`, `l` | Next source |
| `BackTab`, `Left`, `h` | Previous source |
| `1` to `9` | Jump to a source |
| `p`, `Space` | Pause or resume the display (counting and recording continue) |
| `r` | Pause or resume recording (needs `--record`) |
| `v` | Switch between tabs and side-by-side layout |
| `c` | Clear all sources and counters |
| `?`, `F1` | Toggle help |
| `q`, `Esc`, `Ctrl-C` | Quit |

## Wire format

Binary packets use network byte order (big-endian) throughout. A packet is a 20-byte header, an 88-byte payload and a 4-byte CRC, 112 bytes in total.

Header:

| Offset | Size | Field | Notes |
| --- | --- | --- | --- |
| 0 | 4 | `magic` | `0x544C4D59` ("TLMY") |
| 4 | 1 | `version` | Currently `1`; not enforced |
| 5 | 1 | `msg_type` | `1` = telemetry |
| 6 | 2 | `payload_len` | Payload size in bytes |
| 8 | 4 | `seq` | Increments by one per packet, wraps at 2^32 |
| 12 | 8 | `timestamp_ns` | Sender timestamp in nanoseconds |

Payload (offsets are from the start of the payload):

| Offset | Type | Field |
| --- | --- | --- |
| 0 | `f32` | `cpu_percent` |
| 4 | `u64` | `mem_total_kb` |
| 12 | `u64` | `mem_avail_kb` |
| 20 | `u64` | `swap_total_kb` |
| 28 | `u64` | `swap_free_kb` |
| 36 | `u64` | `uptime_s` |
| 44 | `f32` | `load1` |
| 48 | `f32` | `load5` |
| 52 | `f32` | `load15` |
| 56 | `u64` | `rx_bytes` |
| 64 | `u64` | `tx_bytes` |
| 72 | `u64` | `rx_packets` |
| 80 | `u64` | `tx_packets` |

Trailer: a `u32` CRC-32 (IEEE) computed over the header and payload.

The decoder is tolerant: it rejects bad magic, truncated input and CRC mismatches (counted, never fatal), accepts payloads longer than 88 bytes, and ignores bytes after the CRC. Network counters are cumulative; the dashboard derives rates from consecutive samples, and a counter that goes backwards is treated as a reset.

JSON packets are one object per datagram. A datagram whose first non-blank byte is `{` is parsed as JSON. Fields have the same names as above, plus `seq` and `ts_ns`. Missing fields default to zero and unknown fields are ignored. A JSON packet without `seq` is counted but excluded from loss tracking.

```json
{"seq": 42, "ts_ns": 1700000000000000000, "cpu_percent": 12.5, "mem_total_kb": 8000000, "mem_avail_kb": 5000000, "swap_total_kb": 1000000, "swap_free_kb": 900000, "uptime_s": 3600, "load1": 0.5, "load5": 0.4, "load15": 0.3, "rx_bytes": 100000, "tx_bytes": 50000, "rx_packets": 700, "tx_packets": 500}
```

Recordings are JSONL, one sample per line: `{"t": 1.25, "src": "10.0.0.7:41000", "seq": 42, "ts_ns": 0, "data": {...telemetry fields...}}`, where `t` is seconds since the session started.

Packet statistics per source:

- Lost: sequence gaps; a late packet that fills a gap reduces the count again.
- Reordered: a packet older than the newest one seen.
- Duplicates: the same sequence number as the newest packet.
- CRC errors: counted globally and per sender; other malformed packets are counted globally.

## Architecture

```
 daemon(s) ── UDP ──────> +-----------------+
                          | receiver thread |      Msg (mpsc)      +------------------+
 --demo generator ─ UDP ─>| packet::decode  | ───────────────────> |    App state     |
 (loopback sockets)       +-----------------+                      |  per-source ring |
                                                                   |  buffers, stats, |
 --replay FILE ──────────> replay thread (JSONL) ────────────────> |  alerts, pause   |
                                                                   +---+----------+---+
 keyboard (crossterm) ── on_key ─────────────────────────────────────> |          |
                                                                       v          v
                                                            ui::draw (ratatui)   JSONL recorder
                                                                       |
                                                                       v
                                                        terminal (restored on exit)
```

| Module | Role |
| --- | --- |
| `packet` | Binary and JSON decoder and encoder, CRC check |
| `net` | UDP receiver thread and the `Msg` type |
| `stats` | Ring buffers, sequence tracking, per-source rates |
| `alerts` | Threshold configuration and evaluation |
| `app` | Application state, key handling, pause snapshot |
| `ui` | ratatui rendering |
| `tui` | Terminal setup, teardown and the event loop |
| `record` | JSONL recorder and replay thread |
| `demo` | Simulated sender threads |
| `cli` | clap argument definitions and validation |

## Troubleshooting

- **No sources appear.** Check the daemon's destination address and port against `--bind`, and that no firewall blocks UDP on that port. `--demo` confirms the dashboard itself works.
- **`cannot bind UDP socket`.** Another process is using the port. Choose a different one with `--bind 0.0.0.0:9001`.
- **CRC errors keep growing.** The sender and receiver disagree on the format, or datagrams are corrupted. Verify the field order and byte order against the wire format above.
- **Lost packets while the network is fine.** The sender may skip sequence numbers, or UDP buffers overflow under bursts; lower the sender rate.
- **`cannot enable raw mode`.** Standard output is not a terminal. Run the dashboard in an interactive terminal.
- **Garbled display after a crash.** Run `reset`. The dashboard restores the terminal on normal exit, `Ctrl-C` and panics, but a `kill -9` cannot be intercepted.
- **Wide layouts look cramped.** Press `v` to switch to tabs, or enlarge the terminal.

## Roadmap

- Per-core CPU and per-interface network breakdowns
- Optional filtering of senders by address
- Export of statistics snapshots
- Prebuilt binaries for macOS and Windows
- Configuration file for thresholds

## License

MIT, see [LICENSE](LICENSE).

Bekirhan Karakurt - github.com/bekirhankarakurt
