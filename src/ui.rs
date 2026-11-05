//! Rendering with ratatui.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, Borders, Chart, Clear, Dataset, Gauge, GraphType, Paragraph, Sparkline, Tabs, Wrap,
};
use ratatui::Frame;

use crate::alerts::{Alert, AlertKind, Level};
use crate::app::{App, LayoutMode};
use crate::format::{fmt_kb, fmt_rate, fmt_secs, fmt_uptime};
use crate::stats::{Ring, Source};

fn level_color(level: Level) -> Color {
    match level {
        Level::Ok => Color::Green,
        Level::Warn => Color::Yellow,
        Level::Crit => Color::Red,
    }
}

fn level_style(level: Level) -> Style {
    match level {
        Level::Crit => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        Level::Warn => Style::default().fg(Color::Yellow),
        Level::Ok => Style::default(),
    }
}

fn split(area: Rect, dir: Direction, constraints: &[Constraint]) -> Vec<Rect> {
    Layout::default()
        .direction(dir)
        .constraints(constraints.to_vec())
        .split(area)
        .to_vec()
}

fn titled(title: String) -> Block<'static> {
    Block::default().borders(Borders::ALL).title(title)
}

/// Draw the whole interface.
pub fn draw(f: &mut Frame, app: &App) {
    let rows = split(
        f.size(),
        Direction::Vertical,
        &[
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ],
    );
    draw_header(f, rows[0], app);
    draw_body(f, rows[1], app);
    draw_footer(f, rows[2], app);
    if app.show_help {
        let full = f.size();
        draw_help(f, full);
    }
}

fn badge(text: &str, bg: Color) -> Span<'static> {
    Span::styled(
        format!(" {text} "),
        Style::default()
            .bg(bg)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    )
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.totals;
    let mut spans = vec![
        Span::styled(
            "telemetry-tui-dashboard ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "| {} | sources {} | packets {} | ",
            app.source_label,
            app.sources.len(),
            t.received
        )),
        Span::styled(
            format!("crc errors {}", t.crc_errors),
            level_style(if t.crc_errors > 0 {
                Level::Warn
            } else {
                Level::Ok
            }),
        ),
        Span::raw(format!(" | other errors {} ", t.other_errors)),
    ];
    if app.paused() {
        spans.push(badge("PAUSED", Color::Yellow));
        spans.push(Span::raw(" "));
    }
    if app.recording {
        let name = app.record_path.as_deref().unwrap_or("?");
        spans.push(badge(&format!("REC {name}"), Color::Red));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let hints = "q quit | Tab/1-9 source | p pause | r record | v layout | c clear | ? help";
    let line = if app.status.is_empty() {
        Line::from(Span::styled(hints, Style::default().fg(Color::DarkGray)))
    } else {
        Line::from(vec![
            Span::styled(hints, Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" | {}", app.status),
                Style::default().fg(Color::Yellow),
            ),
        ])
    };
    f.render_widget(Paragraph::new(line), area);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn draw_help(f: &mut Frame, area: Rect) {
    let rect = centered(area, 56, 15);
    let lines = vec![
        Line::from("Tab / Right / l   next source"),
        Line::from("BackTab / Left / h  previous source"),
        Line::from("1-9               jump to source"),
        Line::from("p / Space         pause or resume the display"),
        Line::from("r                 pause or resume recording"),
        Line::from("v                 tabs / side-by-side layout"),
        Line::from("c                 clear sources and counters"),
        Line::from("? / F1            toggle this help"),
        Line::from("q / Esc / Ctrl-C  quit"),
        Line::from(""),
        Line::from("Red values exceed an --alert-* threshold."),
        Line::from("Pausing freezes the view; counting continues."),
    ];
    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new(lines).block(titled(" Help ".to_string())),
        rect,
    );
}

fn draw_body(f: &mut Frame, area: Rect, app: &App) {
    let sources = app.view_sources();
    if sources.is_empty() {
        let text = vec![
            Line::from(""),
            Line::from(format!("Waiting for telemetry ({})...", app.source_label)),
            Line::from(""),
            Line::from("Start the daemon, or run with --demo to generate packets."),
        ];
        f.render_widget(
            Paragraph::new(text)
                .alignment(ratatui::layout::Alignment::Center)
                .block(Block::default().borders(Borders::ALL)),
            area,
        );
        return;
    }
    let selected = app.selected_index();
    match app.layout {
        LayoutMode::Tabs => {
            let parts = split(
                area,
                Direction::Vertical,
                &[Constraint::Length(3), Constraint::Min(5)],
            );
            draw_tabs(f, parts[0], app, sources, selected);
            draw_source(f, parts[1], app, &sources[selected], false, true);
        }
        LayoutMode::SideBySide => {
            let n = u32::try_from(sources.len()).unwrap_or(1).max(1);
            let constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Ratio(1, n)).collect();
            let cols = split(area, Direction::Horizontal, &constraints);
            for (i, (src, rect)) in sources.iter().zip(cols).enumerate() {
                draw_source(f, rect, app, src, true, i == selected);
            }
        }
    }
}

fn draw_tabs(f: &mut Frame, area: Rect, app: &App, sources: &[Source], selected: usize) {
    let now = app.now();
    let titles: Vec<Line> = sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let alerting = !app.thresholds.evaluate(s, now).is_empty();
            let label = format!("{} {}", i + 1, s.addr);
            if alerting {
                Line::from(Span::styled(
                    format!("! {label}"),
                    Style::default().fg(Color::Red),
                ))
            } else {
                Line::from(label)
            }
        })
        .collect();
    let tabs = Tabs::new(titles)
        .block(titled(" Sources ".to_string()))
        .select(selected)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD));
    f.render_widget(tabs, area);
}

fn alert_style(alerts: &[Alert], kind: AlertKind) -> Style {
    if alerts.iter().any(|a| a.kind == kind) {
        level_style(Level::Crit)
    } else {
        Style::default()
    }
}

fn pct_gauge(title: &str, pct: f64, level: Level, boxed: bool) -> Gauge<'static> {
    let pct = if pct.is_finite() { pct } else { 0.0 };
    let mut gauge = Gauge::default()
        .gauge_style(Style::default().fg(level_color(level)))
        .ratio((pct / 100.0).clamp(0.0, 1.0))
        .label(format!("{title} {pct:.1}%"));
    if boxed {
        gauge = gauge.block(Block::default().borders(Borders::ALL));
    }
    gauge
}

/// Newest `width` values of a ring, as sparkline input.
fn tail(ring: &Ring<f64>, width: u16) -> Vec<u64> {
    let all: Vec<u64> = ring.iter().map(|v| v.max(0.0).round() as u64).collect();
    let keep = usize::from(width).max(1);
    let skip = all.len().saturating_sub(keep);
    all[skip..].to_vec()
}

fn spark_block(name: &str, ring: &Ring<f64>, unit: &str) -> String {
    match ring.summary() {
        Some(s) => format!(
            " {name} min {:.0}{unit} avg {:.0}{unit} max {:.0}{unit} ",
            s.min, s.avg, s.max
        ),
        None => format!(" {name} "),
    }
}

fn draw_source(f: &mut Frame, area: Rect, app: &App, src: &Source, compact: bool, focused: bool) {
    let now = app.now();
    let alerts = app.thresholds.evaluate(src, now);
    let th = &app.thresholds;

    let mut border = Style::default();
    if !alerts.is_empty() {
        border = level_style(Level::Crit);
    } else if focused && compact {
        border = Style::default().fg(Color::Cyan);
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(format!(" {} ", src.addr));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(t) = src.latest.as_ref() else {
        f.render_widget(Paragraph::new("no data yet"), inner);
        return;
    };
    let mem_pct = t.mem_used_pct();
    let swap_pct = t.swap_used_pct();
    let cpu = f64::from(t.cpu_percent);
    let age = (now - src.last_seen).max(0.0);

    if compact {
        let parts = split(
            inner,
            Direction::Vertical,
            &[
                Constraint::Length(6),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Min(3),
            ],
        );
        let lines = vec![
            Line::from(format!("up {}", fmt_uptime(t.uptime_s))),
            Line::from(vec![
                Span::raw("load "),
                Span::styled(
                    format!("{:.2}", t.load1),
                    level_style(th.load_level(f64::from(t.load1))),
                ),
                Span::raw(format!(" {:.2} {:.2}", t.load5, t.load15)),
            ]),
            Line::from(format!(
                "mem {} / {}",
                fmt_kb(t.mem_total_kb.saturating_sub(t.mem_avail_kb)),
                fmt_kb(t.mem_total_kb)
            )),
            Line::from(vec![
                Span::raw("rx "),
                Span::styled(
                    fmt_rate(src.rx_now()),
                    level_style(th.net_level(src.rx_now())),
                ),
                Span::raw("  tx "),
                Span::styled(
                    fmt_rate(src.tx_now()),
                    level_style(th.net_level(src.tx_now())),
                ),
            ]),
            Line::from(format!(
                "pkts {} lost {} reord {} crc {}",
                src.seq.received, src.seq.lost, src.seq.reordered, src.crc_errors
            )),
            alert_line(&alerts),
        ];
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), parts[0]);
        f.render_widget(pct_gauge("CPU", cpu, th.cpu_level(cpu), false), parts[1]);
        f.render_widget(
            pct_gauge("MEM", mem_pct, th.mem_level(mem_pct), false),
            parts[2],
        );
        f.render_widget(
            pct_gauge("SWAP", swap_pct, th.swap_level(swap_pct), false),
            parts[3],
        );
        draw_spark(f, parts[4], "CPU", &src.cpu, th.cpu_level(cpu));
        draw_spark(f, parts[5], "MEM", &src.mem, th.mem_level(mem_pct));
        return;
    }

    let parts = split(
        inner,
        Direction::Vertical,
        &[
            Constraint::Length(8),
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Min(8),
            Constraint::Length(3),
        ],
    );

    // Top row: system information and packet statistics.
    let top = split(
        parts[0],
        Direction::Horizontal,
        &[Constraint::Percentage(50), Constraint::Percentage(50)],
    );
    let load_style = level_style(th.load_level(f64::from(t.load1)));
    let info = vec![
        Line::from(format!("Uptime    {}", fmt_uptime(t.uptime_s))),
        Line::from(vec![
            Span::raw("Load      "),
            Span::styled(format!("{:.2}", t.load1), load_style),
            Span::raw(format!("  {:.2}  {:.2}", t.load5, t.load15)),
        ]),
        Line::from(format!(
            "Memory    {} used of {}",
            fmt_kb(t.mem_total_kb.saturating_sub(t.mem_avail_kb)),
            fmt_kb(t.mem_total_kb)
        )),
        Line::from(format!(
            "Swap      {} used of {}",
            fmt_kb(t.swap_total_kb.saturating_sub(t.swap_free_kb)),
            fmt_kb(t.swap_total_kb)
        )),
        Line::from(vec![
            Span::raw("Net rx    "),
            Span::styled(
                fmt_rate(src.rx_now()),
                level_style(th.net_level(src.rx_now())),
            ),
            Span::raw(format!("  ({} pkts)", t.rx_packets)),
        ]),
        Line::from(vec![
            Span::raw("Net tx    "),
            Span::styled(
                fmt_rate(src.tx_now()),
                level_style(th.net_level(src.tx_now())),
            ),
            Span::raw(format!("  ({} pkts)", t.tx_packets)),
        ]),
    ];
    f.render_widget(
        Paragraph::new(info).block(titled(" System ".into())),
        top[0],
    );

    let stale_style = alert_style(&alerts, AlertKind::Stale);
    let loss_level = if src.seq.lost > 0 {
        Level::Warn
    } else {
        Level::Ok
    };
    let seq_text = src
        .seq
        .last_seq()
        .map_or_else(|| "-".to_string(), |s| s.to_string());
    let pk = vec![
        Line::from(format!("Received   {}", src.seq.received)),
        Line::from(vec![
            Span::raw("Lost       "),
            Span::styled(
                format!("{} ({:.2}%)", src.seq.lost, src.seq.loss_pct()),
                level_style(loss_level),
            ),
        ]),
        Line::from(format!("Reordered  {}", src.seq.reordered)),
        Line::from(format!("Duplicates {}", src.seq.duplicates)),
        Line::from(vec![
            Span::raw("CRC errors "),
            Span::styled(
                src.crc_errors.to_string(),
                level_style(if src.crc_errors > 0 {
                    Level::Warn
                } else {
                    Level::Ok
                }),
            ),
        ]),
        Line::from(vec![
            Span::raw(format!("Last seq   {seq_text}   seen ")),
            Span::styled(format!("{} ago", fmt_secs(age)), stale_style),
        ]),
    ];
    f.render_widget(Paragraph::new(pk).block(titled(" Packets ".into())), top[1]);

    // Gauges.
    let g = split(
        parts[1],
        Direction::Horizontal,
        &[
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ],
    );
    f.render_widget(pct_gauge("CPU", cpu, th.cpu_level(cpu), true), g[0]);
    f.render_widget(pct_gauge("MEM", mem_pct, th.mem_level(mem_pct), true), g[1]);
    f.render_widget(
        pct_gauge("SWAP", swap_pct, th.swap_level(swap_pct), true),
        g[2],
    );

    // Sparklines.
    let sp = split(
        parts[2],
        Direction::Horizontal,
        &[Constraint::Percentage(50), Constraint::Percentage(50)],
    );
    draw_spark(f, sp[0], "CPU", &src.cpu, th.cpu_level(cpu));
    draw_spark(f, sp[1], "MEM", &src.mem, th.mem_level(mem_pct));

    // Network chart.
    draw_net_chart(f, parts[3], app, src);

    // Alerts.
    let alert_block = if alerts.is_empty() {
        titled(" Alerts ".into())
    } else {
        titled(" Alerts ".into()).border_style(level_style(Level::Crit))
    };
    f.render_widget(
        Paragraph::new(alert_line(&alerts)).block(alert_block),
        parts[4],
    );
}

fn alert_line(alerts: &[Alert]) -> Line<'static> {
    if alerts.is_empty() {
        Line::from(Span::styled(
            "alerts: none",
            Style::default().fg(Color::Green),
        ))
    } else {
        let text = alerts
            .iter()
            .map(Alert::describe)
            .collect::<Vec<_>>()
            .join("; ");
        Line::from(Span::styled(text, level_style(Level::Crit)))
    }
}

fn draw_spark(f: &mut Frame, area: Rect, name: &str, ring: &Ring<f64>, level: Level) {
    let block = titled(spark_block(name, ring, "%"));
    let inner = block.inner(area);
    let data = tail(ring, inner.width);
    let spark = Sparkline::default()
        .block(block)
        .style(Style::default().fg(level_color(level)))
        .data(&data)
        .max(100);
    f.render_widget(spark, area);
}

fn series(ring: &Ring<f64>) -> Vec<(f64, f64)> {
    ring.iter()
        .enumerate()
        .map(|(i, v)| (i as f64, *v))
        .collect()
}

fn draw_net_chart(f: &mut Frame, area: Rect, app: &App, src: &Source) {
    let rx = series(&src.rx_rate);
    let tx = series(&src.tx_rate);
    let peak = src
        .rx_rate
        .iter()
        .chain(src.tx_rate.iter())
        .fold(0.0_f64, |m, v| m.max(*v));
    let ymax = (peak * 1.2).max(1024.0);
    let xmax = (src.rx_rate.capacity().saturating_sub(1)).max(1) as f64;
    let level = app.thresholds.net_level(src.rx_now().max(src.tx_now()));

    let datasets = vec![
        Dataset::default()
            .name("rx")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&rx),
        Dataset::default()
            .name("tx")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Magenta))
            .data(&tx),
    ];
    let title = Line::from(vec![
        Span::raw(" Network rx "),
        Span::styled(fmt_rate(src.rx_now()), level_style(level)),
        Span::raw("  tx "),
        Span::styled(fmt_rate(src.tx_now()), level_style(level)),
        Span::raw(" "),
    ]);
    let chart = Chart::new(datasets)
        .block(Block::default().borders(Borders::ALL).title(title))
        .x_axis(Axis::default().bounds([0.0, xmax]))
        .y_axis(Axis::default().bounds([0.0, ymax]).labels(vec![
            Span::raw("0"),
            Span::raw(fmt_rate(ymax / 2.0)),
            Span::raw(fmt_rate(ymax)),
        ]));
    f.render_widget(chart, area);
}
