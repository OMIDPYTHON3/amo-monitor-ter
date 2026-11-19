//! Small formatting and parsing helpers shared across the application.

use ratatui::style::Color;

/// Format a `f64` value with a fixed number of decimals, or `N/A` when it is missing.
pub fn fmt_opt(value: Option<f64>, decimals: usize, unit: &str) -> String {
    match value {
        Some(v) if v.is_finite() => format!("{:.*}{}", decimals, v, unit),
        _ => "N/A".to_string(),
    }
}

/// Format a byte rate (bytes per second) using binary prefixes.
pub fn fmt_rate(bytes_per_sec: Option<f64>) -> String {
    match bytes_per_sec {
        Some(v) if v.is_finite() && v >= 0.0 => format!("{}/s", fmt_bytes(v)),
        _ => "N/A".to_string(),
    }
}

/// Format a byte count using binary prefixes (KiB, MiB, GiB, ...).
pub fn fmt_bytes(bytes: f64) -> String {
    if !bytes.is_finite() || bytes < 0.0 {
        return "N/A".to_string();
    }
    const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut value = bytes;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{:.0} {}", value, UNITS[unit])
    } else if value >= 100.0 {
        format!("{:.0} {}", value, UNITS[unit])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

/// Format a duration in seconds as `Xd HHh MMm SSs`.
pub fn fmt_uptime(secs: Option<u64>) -> String {
    match secs {
        None => "N/A".to_string(),
        Some(s) => {
            let days = s / 86_400;
            let hours = (s % 86_400) / 3_600;
            let minutes = (s % 3_600) / 60;
            let seconds = s % 60;
            if days > 0 {
                format!("{}d {:02}h {:02}m", days, hours, minutes)
            } else if hours > 0 {
                format!("{:02}h {:02}m {:02}s", hours, minutes, seconds)
            } else {
                format!("{:02}m {:02}s", minutes, seconds)
            }
        }
    }
}

/// Parse a `#RRGGBB` colour string. Returns `None` for malformed input.
pub fn parse_hex_color(input: &str) -> Option<Color> {
    let s = input.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(s.get(0..2)?, 16).ok()?;
    let g = u8::from_str_radix(s.get(2..4)?, 16).ok()?;
    let b = u8::from_str_radix(s.get(4..6)?, 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// Clamp a value to a closed range without panicking on inverted ranges.
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if max < min {
        min
    } else {
        value.clamp(min, max)
    }
}

/// One-minute load average from `/proc/loadavg`, when available.
pub fn read_loadavg() -> Option<f64> {
    read_sysfs(std::path::Path::new("/proc/loadavg"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Read a sysfs file as a trimmed string. Missing/unreadable files yield `None`.
#[cfg(target_os = "linux")]
pub fn read_sysfs(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_formatting() {
        assert_eq!(fmt_bytes(0.0), "0 B");
        assert_eq!(fmt_bytes(1023.0), "1023 B");
        assert_eq!(fmt_bytes(1024.0), "1.0 KiB");
        assert_eq!(fmt_bytes(1536.0), "1.5 KiB");
        assert_eq!(fmt_bytes(5.0 * 1024.0 * 1024.0), "5.0 MiB");
        assert_eq!(fmt_rate(Some(2.0 * 1024.0 * 1024.0)), "2.0 MiB/s");
        assert_eq!(fmt_rate(None), "N/A");
    }

    #[test]
    fn uptime_formatting() {
        assert_eq!(fmt_uptime(Some(0)), "00m 00s");
        assert_eq!(fmt_uptime(Some(65)), "01m 05s");
        assert_eq!(fmt_uptime(Some(3_723)), "01h 02m 03s");
        assert_eq!(fmt_uptime(Some(90_123)), "1d 01h 02m");
        assert_eq!(fmt_uptime(None), "N/A");
    }

    #[test]
    fn colours() {
        assert_eq!(parse_hex_color("#1A2b3C"), Some(Color::Rgb(0x1A, 0x2b, 0x3C)));
        assert_eq!(parse_hex_color("zzzzzz"), None);
        assert_eq!(parse_hex_color("#fff"), None);
    }
}
