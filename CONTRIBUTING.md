# Contributing

Thanks for your interest in improving telemetry-tui-dashboard.

## Getting started

```sh
git clone https://github.com/bekirhankarakurt/telemetry-tui-dashboard
cd telemetry-tui-dashboard
cargo run -- --demo
```

The crate declares a minimum supported Rust version of 1.74. CI builds with
the current stable toolchain.

## Before opening a pull request

Run the same checks as CI:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

## Guidelines

- Keep changes focused; one logical change per pull request.
- Add or update unit tests for decoder, statistics and alert logic.
- Do not add `unsafe` code.
- Update the README and CHANGELOG when behaviour or the wire format changes.
- Write commit messages in the imperative mood, for example
  "Fix sequence wrap handling".

## Reporting bugs

Open an issue with the version, your terminal, the command line used and, for
decoding problems, a hex dump or JSONL recording of the offending packets.
Security issues should follow [SECURITY.md](SECURITY.md).
