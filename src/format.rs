//! Human-readable formatting helpers.

const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

/// Format a byte count using binary prefixes.
pub fn fmt_bytes(bytes: f64) -> String {
    if !bytes.is_finite() || bytes < 0.0 {
        return "0 B".to_string();
    }
    let mut value = bytes;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Format a bytes-per-second rate.
pub fn fmt_rate(bytes_per_s: f64) -> String {
    format!("{}/s", fmt_bytes(bytes_per_s))
}

/// Format a size given in kibibytes.
pub fn fmt_kb(kb: u64) -> String {
    fmt_bytes(kb as f64 * 1024.0)
}

/// Format an uptime in seconds as `Nd HH:MM:SS` or `HH:MM:SS`.
pub fn fmt_uptime(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let mins = (secs % 3_600) / 60;
    let s = secs % 60;
    if days > 0 {
        format!("{days}d {hours:02}:{mins:02}:{s:02}")
    } else {
        format!("{hours:02}:{mins:02}:{s:02}")
    }
}

/// Format a short duration in seconds.
pub fn fmt_secs(secs: f64) -> String {
    if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        format!("{:.0}m", secs / 60.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes() {
        assert_eq!(fmt_bytes(0.0), "0 B");
        assert_eq!(fmt_bytes(1023.0), "1023 B");
        assert_eq!(fmt_bytes(1536.0), "1.5 KiB");
        assert_eq!(fmt_bytes(5.0 * 1024.0 * 1024.0), "5.0 MiB");
        assert_eq!(fmt_bytes(-1.0), "0 B");
        assert_eq!(fmt_bytes(f64::NAN), "0 B");
    }

    #[test]
    fn rates_and_kb() {
        assert_eq!(fmt_rate(2048.0), "2.0 KiB/s");
        assert_eq!(fmt_kb(1024), "1.0 MiB");
    }

    #[test]
    fn uptime() {
        assert_eq!(fmt_uptime(59), "00:00:59");
        assert_eq!(fmt_uptime(3_725), "01:02:05");
        assert_eq!(fmt_uptime(90_061), "1d 01:01:01");
    }

    #[test]
    fn secs() {
        assert_eq!(fmt_secs(1.5), "1.5s");
        assert_eq!(fmt_secs(120.0), "2m");
    }
}
