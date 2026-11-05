use std::net::UdpSocket;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use telemetry_tui_dashboard::app::App;
use telemetry_tui_dashboard::cli::Cli;
use telemetry_tui_dashboard::net::spawn_receiver;
use telemetry_tui_dashboard::record::{spawn_replay, Recorder};
use telemetry_tui_dashboard::{demo, tui};

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.validate()?;

    let (tx, rx) = mpsc::channel();

    let label = if let Some(path) = &cli.replay {
        spawn_replay(path, cli.replay_speed, tx)?;
        format!("replay {}", path.display())
    } else {
        let socket = UdpSocket::bind(cli.bind)
            .with_context(|| format!("cannot bind UDP socket on {}", cli.bind))?;
        let local = socket.local_addr().unwrap_or(cli.bind);
        spawn_receiver(socket, tx);
        if cli.demo {
            demo::spawn(local, cli.demo_sources, cli.demo_rate)?;
            format!("demo on {local}")
        } else {
            format!("listening on {local}")
        }
    };

    let mut app = App::new(label, cli.history, cli.layout, cli.thresholds());
    if let Some(path) = &cli.record {
        let recorder = Recorder::create(path)?;
        app.attach_recorder(recorder, path.display().to_string());
    }

    tui::run(&mut app, &rx, Duration::from_millis(cli.refresh_ms))
}
