# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [1.0.0] - 2026-11-05

### Added

- Terminal dashboard with CPU and memory sparklines, usage gauges and network
  rx/tx rate charts.
- Binary packet decoder (magic, CRC-32 IEEE, truncation checks) and one-line
  JSON packet support.
- Multiple sources keyed by sender address, shown as tabs or side by side.
- Packet statistics: received, lost, reordered, duplicated and CRC errors.
- Alert thresholds for CPU, memory, swap, load, network rate and silent sources.
- Pause, JSONL recording and replay with adjustable speed.
- `--demo` packet generator with simulated loss, corruption and reordering.
- Terminal restoration on exit, `Ctrl-C` and panic.
- CI workflow, tagged release workflow with checksums, and Dependabot updates.

[1.0.0]: https://github.com/bekirhankarakurt/telemetry-tui-dashboard/releases/tag/v1.0.0
