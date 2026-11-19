//! Fully configurable dynamic text colours for every monitoring metric.

use std::collections::BTreeMap;

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::config::theme::ResolvedTheme;
use crate::monitor::Snapshot;
use crate::util::parse_hex_color;

/// Every metric that can be colour-coded independently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Metric {
    CpuUsage,
    GpuUsage,
    Ram,
    Swap,
    CpuPower,
    GpuPower,
    CpuTemp,
    GpuTemp,
    CpuFreq,
    GpuFreq,
    NetRx,
    NetTx,
    DiskRead,
    DiskWrite,
}

impl Metric {
    pub const ALL: [Metric; 14] = [
        Metric::CpuUsage,
        Metric::GpuUsage,
        Metric::Ram,
        Metric::Swap,
        Metric::CpuPower,
        Metric::GpuPower,
        Metric::CpuTemp,
        Metric::GpuTemp,
        Metric::CpuFreq,
        Metric::GpuFreq,
        Metric::NetRx,
        Metric::NetTx,
        Metric::DiskRead,
        Metric::DiskWrite,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Metric::CpuUsage => "CPU usage",
            Metric::GpuUsage => "GPU usage",
            Metric::Ram => "RAM usage",
            Metric::Swap => "Swap usage",
            Metric::CpuPower => "CPU power",
            Metric::GpuPower => "GPU power",
            Metric::CpuTemp => "CPU temp",
            Metric::GpuTemp => "GPU temp",
            Metric::CpuFreq => "CPU freq",
            Metric::GpuFreq => "GPU freq",
            Metric::NetRx => "Download",
            Metric::NetTx => "Upload",
            Metric::DiskRead => "Disk read",
            Metric::DiskWrite => "Disk write",
        }
    }

    pub fn group(self) -> &'static str {
        match self {
            Metric::CpuUsage | Metric::CpuPower | Metric::CpuTemp | Metric::CpuFreq => "CPU",
            Metric::GpuUsage | Metric::GpuPower | Metric::GpuTemp | Metric::GpuFreq => "GPU",
            Metric::Ram | Metric::Swap => "MEM",
            Metric::NetRx | Metric::NetTx => "NET",
            Metric::DiskRead | Metric::DiskWrite => "DISK",
        }
    }

    pub fn is_percentage(self) -> bool {
        matches!(self, Metric::CpuUsage | Metric::GpuUsage | Metric::Ram | Metric::Swap)
    }

    pub fn unit(self) -> &'static str {
        match self {
            Metric::CpuUsage | Metric::GpuUsage | Metric::Ram | Metric::Swap => "%",
            Metric::CpuPower | Metric::GpuPower => " W",
            Metric::CpuTemp | Metric::GpuTemp => " °C",
            Metric::CpuFreq | Metric::GpuFreq => " MHz",
            Metric::NetRx | Metric::NetTx | Metric::DiskRead | Metric::DiskWrite => " B/s",
        }
    }

    /// Thresholds appropriate for this metric's unit (never percentages for
    /// watts, MHz, degrees or byte rates).
    pub fn default_thresholds(self) -> [f64; 3] {
        match self {
            Metric::CpuUsage | Metric::GpuUsage | Metric::Ram | Metric::Swap => [60.0, 75.0, 90.0],
            Metric::CpuPower => [30.0, 65.0, 100.0],
            Metric::GpuPower => [80.0, 200.0, 350.0],
            Metric::CpuTemp | Metric::GpuTemp => [50.0, 70.0, 85.0],
            Metric::CpuFreq => [1200.0, 2400.0, 3600.0],
            Metric::GpuFreq => [600.0, 1200.0, 1800.0],
            Metric::NetRx | Metric::NetTx => [1024.0, 1024.0 * 1024.0, 10.0 * 1024.0 * 1024.0],
            Metric::DiskRead | Metric::DiskWrite => {
                [1024.0 * 1024.0, 20.0 * 1024.0 * 1024.0, 100.0 * 1024.0 * 1024.0]
            }
        }
    }

    pub fn default_colors(self) -> [String; 4] {
        default_scheme_colors("classic")
    }

    /// Current real value of this metric (no fabricated fallbacks).
    pub fn value(self, snapshot: &Snapshot) -> Option<f64> {
        match self {
            Metric::CpuUsage => snapshot.cpu.usage_percent,
            Metric::GpuUsage => snapshot.gpu.usage_percent,
            Metric::Ram => snapshot.memory.percent,
            Metric::Swap => snapshot.swap.percent,
            Metric::CpuPower => snapshot.cpu.power_w,
            Metric::GpuPower => snapshot.gpu.power_w,
            Metric::CpuTemp => snapshot.cpu.temperature_c,
            Metric::GpuTemp => snapshot.gpu.temperature_c,
            Metric::CpuFreq => snapshot.cpu.frequency_mhz,
            Metric::GpuFreq => snapshot.gpu.frequency_mhz,
            Metric::NetRx => snapshot.network.rx_bytes_per_sec,
            Metric::NetTx => snapshot.network.tx_bytes_per_sec,
            Metric::DiskRead => snapshot.disk.read_bytes_per_sec,
            Metric::DiskWrite => snapshot.disk.write_bytes_per_sec,
        }
    }
}

/// Predefined colour schemes. Tokens such as `warning` resolve through the
/// active theme so palettes stay readable in every theme.
pub const SCHEMES: [(&str, [&str; 4]); 5] = [
    ("classic", ["foreground", "warning", "orange", "danger"]),
    ("traffic", ["success", "warning", "orange", "danger"]),
    ("ocean", ["foreground", "accent", "memory", "danger"]),
    ("mono", ["muted", "foreground", "accent", "danger"]),
    ("sunset", ["foreground", "#FFD166", "#F47047", "#D90429"]),
];

pub fn default_scheme_colors(scheme: &str) -> [String; 4] {
    let (_, colors) = SCHEMES
        .iter()
        .find(|(name, _)| *name == scheme)
        .unwrap_or(&SCHEMES[0]);
    colors.iter().map(|s| s.to_string()).collect::<Vec<_>>().try_into().unwrap()
}

fn blend(a: Color, b: Color, t: f64) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => Color::Rgb(
            (ar as f64 * (1.0 - t) + br as f64 * t).round() as u8,
            (ag as f64 * (1.0 - t) + bg as f64 * t).round() as u8,
            (ab as f64 * (1.0 - t) + bb as f64 * t).round() as u8,
        ),
        _ => a,
    }
}

/// Resolve a colour token or `#RRGGBB` value against the active theme.
pub fn resolve_color(spec: &str, theme: &ResolvedTheme) -> Color {
    match spec.trim().to_lowercase().as_str() {
        "foreground" => theme.foreground,
        "text" => theme.text,
        "muted" => theme.muted,
        "success" => theme.success,
        "warning" => theme.warning,
        "danger" => theme.danger,
        "accent" => theme.accent,
        "accent_alt" => theme.accent_alt,
        "memory" => theme.memory,
        "cpu" => theme.cpu,
        "gpu" => theme.gpu,
        "orange" => blend(theme.warning, theme.danger, 0.5),
        other => parse_hex_color(other).unwrap_or(theme.text),
    }
}

/// Colour settings for a single metric.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricColorConfig {
    pub enabled: bool,
    pub thresholds: [f64; 3],
    pub colors: [String; 4],
}

impl Default for MetricColorConfig {
    fn default() -> Self {
        Self::for_metric(Metric::CpuUsage)
    }
}

impl MetricColorConfig {
    pub fn for_metric(metric: Metric) -> Self {
        Self {
            enabled: true,
            thresholds: metric.default_thresholds(),
            colors: metric.default_colors(),
        }
    }

    /// Keep thresholds strictly increasing so tier lookup stays predictable.
    pub fn normalized(&self) -> Self {
        let mut t = self.thresholds;
        t[0] = t[0].max(0.0);
        t[1] = t[1].max(t[0] + 1e-6);
        t[2] = t[2].max(t[1] + 1e-6);
        Self { thresholds: t, ..self.clone() }
    }
}

/// Global dynamic-colour preferences, persisted in `config.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DynamicColorConfig {
    pub enabled: bool,
    pub scheme: String,
    pub metrics: BTreeMap<String, MetricColorConfig>,
}

impl Default for DynamicColorConfig {
    fn default() -> Self {
        Self::for_scheme("classic")
    }
}

impl DynamicColorConfig {
    pub fn for_scheme(scheme: &str) -> Self {
        let mut metrics = BTreeMap::new();
        for metric in Metric::ALL {
            let mut cfg = MetricColorConfig::for_metric(metric);
            cfg.colors = default_scheme_colors(scheme);
            metrics.insert(metric.label().to_string(), cfg);
        }
        Self {
            enabled: true,
            scheme: scheme.to_string(),
            metrics,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn metric(&self, metric: Metric) -> MetricColorConfig {
        self.metrics
            .get(metric.label())
            .cloned()
            .unwrap_or_else(|| MetricColorConfig::for_metric(metric))
    }

    pub fn set_metric(&mut self, metric: Metric, config: MetricColorConfig) {
        self.metrics.insert(metric.label().to_string(), config);
    }

    pub fn apply_scheme(&mut self, scheme: &str) {
        self.scheme = scheme.to_string();
        let colors = default_scheme_colors(scheme);
        for metric in Metric::ALL {
            let mut cfg = self.metric(metric);
            cfg.colors = colors.clone();
            self.set_metric(metric, cfg);
        }
    }

    /// Resolve the colour for a value, or the neutral text colour when dynamic
    /// colours are disabled or no sensor value is available.
    pub fn color_for(&self, metric: Metric, value: Option<f64>, theme: &ResolvedTheme) -> Color {
        let Some(v) = value.filter(|v| v.is_finite()) else {
            return theme.text;
        };
        if !self.enabled {
            return theme.text;
        }
        let config = self.metric(metric);
        if !config.enabled {
            return theme.text;
        }
        let config = config.normalized();
        let tier = if v < config.thresholds[0] {
            0
        } else if v < config.thresholds[1] {
            1
        } else if v < config.thresholds[2] {
            2
        } else {
            3
        };
        resolve_color(&config.colors[tier], theme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> ResolvedTheme {
        crate::config::theme::ThemeSet::builtin().resolve("Dark", &Default::default())
    }

    #[test]
    fn percentage_thresholds_match_specification() {
        assert_eq!(Metric::CpuUsage.default_thresholds(), [60.0, 75.0, 90.0]);
        let cfg = DynamicColorConfig::default();
        let t = theme();
        for (value, tier) in [(0.0, 0), (59.0, 0), (60.0, 1), (74.0, 1), (75.0, 2), (89.0, 2), (90.0, 3), (100.0, 3)] {
            let expected = resolve_color(&default_scheme_colors("classic")[tier], &t);
            assert_eq!(cfg.color_for(Metric::CpuUsage, Some(value), &t), expected, "value {value}");
        }
    }

    #[test]
    fn non_percentage_metrics_use_their_own_scales() {
        assert_ne!(Metric::CpuPower.default_thresholds(), [60.0, 75.0, 90.0]);
        assert_ne!(Metric::NetRx.default_thresholds(), [60.0, 75.0, 90.0]);
        assert_ne!(Metric::CpuTemp.default_thresholds(), [60.0, 75.0, 90.0]);
        for metric in Metric::ALL {
            let t = metric.default_thresholds();
            assert!(t[0] < t[1] && t[1] < t[2], "{} has inverted thresholds", metric.label());
        }
    }

    #[test]
    fn toggles_disable_dynamic_colours() {
        let t = theme();
        let mut cfg = DynamicColorConfig::default();
        let hot = cfg.color_for(Metric::CpuUsage, Some(95.0), &t);
        assert_ne!(hot, t.text);
        cfg.enabled = false;
        assert_eq!(cfg.color_for(Metric::CpuUsage, Some(95.0), &t), t.text);
        cfg.enabled = true;
        let mut one = cfg.metric(Metric::CpuUsage);
        one.enabled = false;
        cfg.set_metric(Metric::CpuUsage, one);
        assert_eq!(cfg.color_for(Metric::CpuUsage, Some(95.0), &t), t.text);
        assert_ne!(cfg.color_for(Metric::Ram, Some(95.0), &t), t.text);
    }

    #[test]
    fn custom_thresholds_and_colours_are_applied() {
        let t = theme();
        let mut cfg = DynamicColorConfig::default();
        let mut one = cfg.metric(Metric::GpuTemp);
        one.thresholds = [30.0, 40.0, 50.0];
        one.colors = ["#010203".into(), "#040506".into(), "#070809".into(), "#0A0B0C".into()];
        cfg.set_metric(Metric::GpuTemp, one);
        assert_eq!(cfg.color_for(Metric::GpuTemp, Some(55.0), &t), Color::Rgb(10, 11, 12));
    }
}
