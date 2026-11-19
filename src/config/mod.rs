//! Persisted configuration: layouts, widget settings, themes and timing.

pub mod theme;

use std::collections::BTreeMap;

use crate::colors::{DynamicColorConfig, Metric as ColorMetric};
use crate::monitor::Snapshot;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

pub use theme::ThemeSet;

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_id() -> String {
    let n = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("w{}-{}", std::process::id(), n)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WidgetKind {
    Cpu,
    Gpu,
    Ram,
    Swap,
    Network,
    DiskIo,
    Uptime,
}

impl WidgetKind {
    pub const ALL: [WidgetKind; 7] = [
        WidgetKind::Cpu,
        WidgetKind::Gpu,
        WidgetKind::Ram,
        WidgetKind::Swap,
        WidgetKind::Network,
        WidgetKind::DiskIo,
        WidgetKind::Uptime,
    ];

    pub fn title(self) -> &'static str {
        match self {
            WidgetKind::Cpu => "CPU",
            WidgetKind::Gpu => "GPU",
            WidgetKind::Ram => "RAM",
            WidgetKind::Swap => "SWAP",
            WidgetKind::Network => "NETWORK",
            WidgetKind::DiskIo => "DISK I/O",
            WidgetKind::Uptime => "SYSTEM",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayMode {
    Compact,
    Medium,
    Detailed,
}

impl DisplayMode {
    pub const ALL: [DisplayMode; 3] = [DisplayMode::Compact, DisplayMode::Medium, DisplayMode::Detailed];
    pub fn label(self) -> &'static str {
        match self {
            DisplayMode::Compact => "Compact",
            DisplayMode::Medium => "Medium",
            DisplayMode::Detailed => "Detailed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    NumbersOnly,
    NumbersAndCharts,
    ChartsOnly,
}

impl ViewMode {
    pub const ALL: [ViewMode; 3] = [
        ViewMode::NumbersOnly,
        ViewMode::NumbersAndCharts,
        ViewMode::ChartsOnly,
    ];
    pub fn label(self) -> &'static str {
        match self {
            ViewMode::NumbersOnly => "Numbers",
            ViewMode::NumbersAndCharts => "Numbers + Charts",
            ViewMode::ChartsOnly => "Charts",
        }
    }
}

/// Available real-time chart visualisation styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChartType {
    Line,
    BrailleLine,
    Area,
    GradientArea,
    MultiLine,
    Step,
    Sparkline,
    Histogram,
}

impl ChartType {
    pub const ALL: [ChartType; 8] = [
        ChartType::Line,
        ChartType::BrailleLine,
        ChartType::Area,
        ChartType::GradientArea,
        ChartType::MultiLine,
        ChartType::Step,
        ChartType::Sparkline,
        ChartType::Histogram,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ChartType::Line => "Line",
            ChartType::BrailleLine => "Braille Line",
            ChartType::Area => "Area",
            ChartType::GradientArea => "Gradient Area",
            ChartType::MultiLine => "Multi-Line",
            ChartType::Step => "Step",
            ChartType::Sparkline => "Sparkline",
            ChartType::Histogram => "Histogram",
        }
    }
}

/// How the vertical extent of a chart is determined.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum YScale {
    Auto,
    Manual,
}

impl YScale {
    pub const ALL: [YScale; 2] = [YScale::Auto, YScale::Manual];
    pub fn label(self) -> &'static str {
        match self {
            YScale::Auto => "Auto",
            YScale::Manual => "Manual",
        }
    }
}

/// Selectable rolling history windows, in seconds.
pub const HISTORY_WINDOWS: [u32; 3] = [30, 60, 300];
/// Selectable chart line thicknesses (supported by pixel-based styles).
pub const LINE_THICKNESSES: [u8; 3] = [1, 2, 3];

/// Settings shared by every widget type.
///
/// `#[serde(default)]` keeps older configuration files loadable: any newly
/// added chart preference simply falls back to its default value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetConfig {
    pub show_primary: bool,
    pub show_frequency: bool,
    pub show_power: bool,
    pub show_temperature: bool,
    pub show_extra: bool,
    pub show_chart: bool,
    pub display_mode: DisplayMode,
    pub view_mode: ViewMode,
    pub chart_type: ChartType,
    pub history_window_secs: u32,
    pub show_grid: bool,
    pub show_axes: bool,
    pub y_scale: YScale,
    pub manual_max: f64,
    pub line_thickness: u8,
}

impl Default for WidgetConfig {
    fn default() -> Self {
        Self {
            show_primary: true,
            show_frequency: true,
            show_power: true,
            show_temperature: true,
            show_extra: true,
            show_chart: true,
            display_mode: DisplayMode::Medium,
            view_mode: ViewMode::NumbersAndCharts,
            chart_type: ChartType::BrailleLine,
            history_window_secs: HISTORY_WINDOWS[0],
            show_grid: false,
            show_axes: false,
            y_scale: YScale::Auto,
            manual_max: 100.0,
            line_thickness: 1,
        }
    }
}

impl WidgetConfig {
    pub fn window(&self) -> f64 {
        if HISTORY_WINDOWS.contains(&self.history_window_secs) {
            self.history_window_secs as f64
        } else {
            HISTORY_WINDOWS[0] as f64
        }
    }

    pub fn thickness(&self) -> u8 {
        if LINE_THICKNESSES.contains(&self.line_thickness) {
            self.line_thickness
        } else {
            1
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WidgetPlacement {
    pub id: String,
    pub kind: WidgetKind,
    pub col: u16,
    pub row: u16,
    pub colspan: u16,
    pub rowspan: u16,
    pub pinned: bool,
    pub config: WidgetConfig,
}

impl WidgetPlacement {
    pub fn new(kind: WidgetKind, col: u16, row: u16, colspan: u16, rowspan: u16) -> Self {
        Self {
            id: new_id(),
            kind,
            col,
            row,
            colspan,
            rowspan,
            pinned: false,
            config: WidgetConfig::default(),
        }
    }

    pub fn right(&self) -> u16 {
        self.col.saturating_add(self.colspan)
    }

    pub fn bottom(&self) -> u16 {
        self.row.saturating_add(self.rowspan)
    }

    pub fn overlaps(&self, other: &WidgetPlacement) -> bool {
        self.col < other.right()
            && other.col < self.right()
            && self.row < other.bottom()
            && other.row < self.bottom()
    }
}

/// Dashboard presentation style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutKind {
    Grid,
    Linear,
    Mixed,
}

impl LayoutKind {
    pub const ALL: [LayoutKind; 3] = [LayoutKind::Grid, LayoutKind::Linear, LayoutKind::Mixed];
    pub fn label(self) -> &'static str {
        match self {
            LayoutKind::Grid => "Grid Dashboard",
            LayoutKind::Linear => "Linear Dashboard",
            LayoutKind::Mixed => "Mixed Dashboard",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            LayoutKind::Grid => "Grid",
            LayoutKind::Linear => "Linear",
            LayoutKind::Mixed => "Mixed",
        }
    }
}

fn default_layout_kind() -> LayoutKind {
    LayoutKind::Grid
}

fn default_linear_display() -> DisplayMode {
    DisplayMode::Compact
}

/// A single compact metric shown in the Linear / Mixed dashboards.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinearMetricKind {
    CpuUsage,
    CpuFreq,
    CpuPower,
    CpuTemp,
    GpuUsage,
    GpuFreq,
    GpuPower,
    GpuTemp,
    Ram,
    Swap,
    NetRx,
    NetTx,
    DiskRead,
    DiskWrite,
    Uptime,
}

impl LinearMetricKind {
    pub const ALL: [LinearMetricKind; 15] = [
        LinearMetricKind::CpuUsage,
        LinearMetricKind::CpuFreq,
        LinearMetricKind::CpuPower,
        LinearMetricKind::CpuTemp,
        LinearMetricKind::GpuUsage,
        LinearMetricKind::GpuFreq,
        LinearMetricKind::GpuPower,
        LinearMetricKind::GpuTemp,
        LinearMetricKind::Ram,
        LinearMetricKind::Swap,
        LinearMetricKind::NetRx,
        LinearMetricKind::NetTx,
        LinearMetricKind::DiskRead,
        LinearMetricKind::DiskWrite,
        LinearMetricKind::Uptime,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LinearMetricKind::CpuUsage => "CPU",
            LinearMetricKind::CpuFreq => "CPU FREQ",
            LinearMetricKind::CpuPower => "CPU PWR",
            LinearMetricKind::CpuTemp => "CPU TEMP",
            LinearMetricKind::GpuUsage => "GPU",
            LinearMetricKind::GpuFreq => "GPU FREQ",
            LinearMetricKind::GpuPower => "GPU PWR",
            LinearMetricKind::GpuTemp => "GPU TEMP",
            LinearMetricKind::Ram => "RAM",
            LinearMetricKind::Swap => "SWAP",
            LinearMetricKind::NetRx => "NET ↓",
            LinearMetricKind::NetTx => "NET ↑",
            LinearMetricKind::DiskRead => "DISK R",
            LinearMetricKind::DiskWrite => "DISK W",
            LinearMetricKind::Uptime => "UPTIME",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            LinearMetricKind::CpuUsage => "CPU",
            LinearMetricKind::CpuFreq => "FRQ",
            LinearMetricKind::CpuPower => "PWR",
            LinearMetricKind::CpuTemp => "TMP",
            LinearMetricKind::GpuUsage => "GPU",
            LinearMetricKind::GpuFreq => "GFrq",
            LinearMetricKind::GpuPower => "GPwr",
            LinearMetricKind::GpuTemp => "GTmp",
            LinearMetricKind::Ram => "RAM",
            LinearMetricKind::Swap => "SWP",
            LinearMetricKind::NetRx => "DN",
            LinearMetricKind::NetTx => "UP",
            LinearMetricKind::DiskRead => "R",
            LinearMetricKind::DiskWrite => "W",
            LinearMetricKind::Uptime => "UP",
        }
    }

    pub fn group(self) -> u8 {
        match self {
            LinearMetricKind::CpuUsage
            | LinearMetricKind::CpuFreq
            | LinearMetricKind::CpuPower
            | LinearMetricKind::CpuTemp => 0,
            LinearMetricKind::GpuUsage
            | LinearMetricKind::GpuFreq
            | LinearMetricKind::GpuPower
            | LinearMetricKind::GpuTemp => 1,
            LinearMetricKind::Ram | LinearMetricKind::Swap => 2,
            LinearMetricKind::NetRx | LinearMetricKind::NetTx => 3,
            LinearMetricKind::DiskRead | LinearMetricKind::DiskWrite => 4,
            LinearMetricKind::Uptime => 5,
        }
    }

    pub fn color_metric(self) -> Option<ColorMetric> {
        Some(match self {
            LinearMetricKind::CpuUsage => ColorMetric::CpuUsage,
            LinearMetricKind::CpuFreq => ColorMetric::CpuFreq,
            LinearMetricKind::CpuPower => ColorMetric::CpuPower,
            LinearMetricKind::CpuTemp => ColorMetric::CpuTemp,
            LinearMetricKind::GpuUsage => ColorMetric::GpuUsage,
            LinearMetricKind::GpuFreq => ColorMetric::GpuFreq,
            LinearMetricKind::GpuPower => ColorMetric::GpuPower,
            LinearMetricKind::GpuTemp => ColorMetric::GpuTemp,
            LinearMetricKind::Ram => ColorMetric::Ram,
            LinearMetricKind::Swap => ColorMetric::Swap,
            LinearMetricKind::NetRx => ColorMetric::NetRx,
            LinearMetricKind::NetTx => ColorMetric::NetTx,
            LinearMetricKind::DiskRead => ColorMetric::DiskRead,
            LinearMetricKind::DiskWrite => ColorMetric::DiskWrite,
            LinearMetricKind::Uptime => return None,
        })
    }

    pub fn series_key(self) -> Option<crate::monitor::SeriesKey> {
        use crate::monitor::SeriesKey;
        Some(match self {
            LinearMetricKind::CpuUsage => SeriesKey::CpuUsage,
            LinearMetricKind::CpuFreq => SeriesKey::CpuFrequency,
            LinearMetricKind::CpuPower => SeriesKey::CpuPower,
            LinearMetricKind::CpuTemp => SeriesKey::CpuTemperature,
            LinearMetricKind::GpuUsage => SeriesKey::GpuUsage,
            LinearMetricKind::GpuFreq => SeriesKey::GpuFrequency,
            LinearMetricKind::GpuPower => SeriesKey::GpuPower,
            LinearMetricKind::GpuTemp => SeriesKey::GpuTemperature,
            LinearMetricKind::Ram => SeriesKey::MemoryUsed,
            LinearMetricKind::Swap => SeriesKey::SwapUsed,
            LinearMetricKind::NetRx => SeriesKey::NetworkRx,
            LinearMetricKind::NetTx => SeriesKey::NetworkTx,
            LinearMetricKind::DiskRead => SeriesKey::DiskRead,
            LinearMetricKind::DiskWrite => SeriesKey::DiskWrite,
            LinearMetricKind::Uptime => return None,
        })
    }

    pub fn value(self, snapshot: &Snapshot) -> Option<f64> {
        if let Some(metric) = self.color_metric() {
            return metric.value(snapshot);
        }
        snapshot.system.uptime_secs.map(|v| v as f64)
    }

    pub fn display(self, snapshot: &Snapshot) -> String {
        match self {
            LinearMetricKind::Uptime => crate::util::fmt_uptime(snapshot.system.uptime_secs),
            LinearMetricKind::CpuFreq | LinearMetricKind::GpuFreq => {
                crate::util::fmt_opt(self.value(snapshot), 0, " MHz")
            }
            LinearMetricKind::CpuPower | LinearMetricKind::GpuPower => {
                crate::util::fmt_opt(self.value(snapshot), 1, " W")
            }
            LinearMetricKind::CpuTemp | LinearMetricKind::GpuTemp => {
                crate::util::fmt_opt(self.value(snapshot), 1, " °C")
            }
            LinearMetricKind::NetRx | LinearMetricKind::NetTx
            | LinearMetricKind::DiskRead | LinearMetricKind::DiskWrite => {
                crate::util::fmt_rate(self.value(snapshot))
            }
            _ => crate::util::fmt_opt(self.value(snapshot), 1, "%"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LinearMetric {
    pub id: String,
    pub kind: LinearMetricKind,
    pub pinned: bool,
    pub group: u8,
    pub order: u16,
    pub show_chart: bool,
}

impl Default for LinearMetric {
    fn default() -> Self {
        Self {
            id: new_id(),
            kind: LinearMetricKind::CpuUsage,
            pinned: false,
            group: 0,
            order: 0,
            show_chart: false,
        }
    }
}

impl LinearMetric {
    pub fn new(kind: LinearMetricKind, group: u8, order: u16) -> Self {
        Self {
            id: new_id(),
            kind,
            pinned: false,
            group,
            order,
            show_chart: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutConfig {
    pub name: String,
    pub columns: u16,
    pub rows: u16,
    pub widgets: Vec<WidgetPlacement>,
    #[serde(default = "default_layout_kind")]
    pub kind: LayoutKind,
    #[serde(default)]
    pub linear_metrics: Vec<LinearMetric>,
    #[serde(default = "default_linear_display")]
    pub linear_display: DisplayMode,
}

impl LayoutConfig {
    pub fn new(name: &str, columns: u16, rows: u16) -> Self {
        Self {
            name: name.to_string(),
            columns,
            rows,
            widgets: Vec::new(),
            kind: LayoutKind::Grid,
            linear_metrics: Vec::new(),
            linear_display: DisplayMode::Compact,
        }
    }

    /// Find the first free top-left slot able to hold a widget of the given size.
    pub fn find_free_slot(&self, colspan: u16, rowspan: u16) -> (u16, u16) {
        let cols = self.columns.max(1);
        let w = colspan.clamp(1, cols);
        let h = rowspan.max(1);
        let occupancy = self.occupancy(self.rows.saturating_add(h));
        for row in 0..self.rows.saturating_add(h) {
            for col in 0..cols.saturating_sub(w - 1) {
                if self.fits(&occupancy, col, row, w, h) {
                    return (col, row);
                }
            }
        }
        (0, self.rows)
    }

    fn fits(&self, occupancy: &[Vec<bool>], col: u16, row: u16, w: u16, h: u16) -> bool {
        let cols = occupancy.first().map_or(0, Vec::len) as u16;
        if col.saturating_add(w) > cols {
            return false;
        }
        for r in row..row.saturating_add(h) {
            let Some(line) = occupancy.get(r as usize) else {
                return false;
            };
            for c in col..col.saturating_add(w) {
                if line.get(c as usize).copied().unwrap_or(true) {
                    return false;
                }
            }
        }
        true
    }

    pub fn occupancy(&self, rows: u16) -> Vec<Vec<bool>> {
        let mut grid = vec![vec![false; self.columns.max(1) as usize]; rows.max(1) as usize];
        for widget in &self.widgets {
            if !widget.pinned {
                continue;
            }
            for r in widget.row..widget.bottom() {
                for c in widget.col..widget.right() {
                    if let Some(cell) = grid.get_mut(r as usize).and_then(|line| line.get_mut(c as usize)) {
                        *cell = true;
                    }
                }
            }
        }
        grid
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub theme: String,
    pub color_overrides: BTreeMap<String, String>,
    pub refresh_ms: u64,
    pub chart_window_secs: u64,
    pub layouts: Vec<LayoutConfig>,
    pub active_layout: String,
    #[serde(default)]
    pub dynamic_colors: DynamicColorConfig,
}

impl Default for Config {
    fn default() -> Self {
        // Balanced composition: three cards, then two, then two — all the
        // same height so no single panel dominates the screen.
        let mut overview = LayoutConfig::new("Overview", 12, 9);
        overview.widgets = vec![
            pinned(WidgetKind::Cpu, 0, 0, 4, 3),
            pinned(WidgetKind::Gpu, 4, 0, 4, 3),
            widget(WidgetKind::Ram, 8, 0, 4, 3),
            widget(WidgetKind::Swap, 0, 3, 4, 3),
            widget(WidgetKind::Network, 4, 3, 8, 3),
            widget(WidgetKind::DiskIo, 0, 6, 6, 3),
            widget(WidgetKind::Uptime, 6, 6, 6, 3),
        ];

        let mut focus = LayoutConfig::new("Focus", 12, 9);
        focus.widgets = vec![
            pinned(WidgetKind::Cpu, 0, 0, 6, 3),
            pinned(WidgetKind::Gpu, 6, 0, 6, 3),
            widget(WidgetKind::Ram, 0, 3, 4, 3),
            widget(WidgetKind::Network, 4, 3, 8, 3),
            widget(WidgetKind::DiskIo, 0, 6, 6, 3),
            widget(WidgetKind::Uptime, 6, 6, 6, 3),
        ];

        let mut compact = LayoutConfig::new("Compact", 12, 6);
        compact.widgets = vec![
            widget(WidgetKind::Cpu, 0, 0, 4, 2),
            widget(WidgetKind::Gpu, 4, 0, 4, 2),
            widget(WidgetKind::Ram, 8, 0, 4, 2),
            widget(WidgetKind::Network, 0, 2, 6, 2),
            widget(WidgetKind::Swap, 6, 2, 6, 2),
            widget(WidgetKind::DiskIo, 0, 4, 6, 2),
            widget(WidgetKind::Uptime, 6, 4, 6, 2),
        ];
        for widget in &mut compact.widgets {
            widget.config.display_mode = DisplayMode::Compact;
        }

        let mut linear = LayoutConfig::new("Linear", 12, 4);
        linear.kind = LayoutKind::Linear;
        linear.linear_metrics = LinearMetricKind::ALL
            .iter()
            .enumerate()
            .map(|(i, kind)| LinearMetric::new(*kind, kind.group(), i as u16))
            .collect();

        let mut mixed = LayoutConfig::new("Mixed", 12, 8);
        mixed.kind = LayoutKind::Mixed;
        mixed.widgets = vec![
            pinned(WidgetKind::Cpu, 0, 0, 6, 4),
            pinned(WidgetKind::Gpu, 6, 0, 6, 4),
            widget(WidgetKind::Network, 0, 4, 6, 4),
            widget(WidgetKind::DiskIo, 6, 4, 6, 4),
        ];
        mixed.linear_metrics = [
            LinearMetricKind::CpuUsage,
            LinearMetricKind::GpuUsage,
            LinearMetricKind::Ram,
            LinearMetricKind::Swap,
            LinearMetricKind::NetRx,
            LinearMetricKind::NetTx,
            LinearMetricKind::Uptime,
        ]
        .iter()
        .enumerate()
        .map(|(i, kind)| LinearMetric::new(*kind, kind.group(), i as u16))
        .collect();

        Self {
            version: 1,
            theme: "Tokyo Night".to_string(),
            color_overrides: BTreeMap::new(),
            refresh_ms: 1000,
            chart_window_secs: 30,
            layouts: vec![overview, focus, compact, linear, mixed],
            active_layout: "Overview".to_string(),
            dynamic_colors: DynamicColorConfig::default(),
        }
    }
}

fn widget(kind: WidgetKind, col: u16, row: u16, w: u16, h: u16) -> WidgetPlacement {
    WidgetPlacement::new(kind, col, row, w, h)
}

fn pinned(kind: WidgetKind, col: u16, row: u16, w: u16, h: u16) -> WidgetPlacement {
    let mut wgt = WidgetPlacement::new(kind, col, row, w, h);
    wgt.pinned = true;
    wgt
}

impl Config {
    /// Directory where all persistent files live (inside the project root).
    pub fn config_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("AMO_CONFIG_DIR") {
            return PathBuf::from(dir);
        }
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Self>(&text) {
                Ok(config) if !config.layouts.is_empty() => config,
                _ => Self::default(),
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let temp = dir.join("config.json.tmp");
        std::fs::write(&temp, json)?;
        std::fs::rename(temp, Self::config_path())
    }

    pub fn active_layout(&self) -> &LayoutConfig {
        self.layouts
            .iter()
            .find(|l| l.name == self.active_layout)
            .unwrap_or(&self.layouts[0])
    }

    pub fn active_layout_mut(&mut self) -> &mut LayoutConfig {
        let index = self.layout_index();
        &mut self.layouts[index]
    }

    pub fn layout_index(&self) -> usize {
        self.layouts
            .iter()
            .position(|l| l.name == self.active_layout)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_layouts() {
        let config = Config::default();
        assert_eq!(config.layouts.len(), 5);
        assert_eq!(config.active_layout().name, "Overview");
        assert_eq!(config.active_layout().widgets.len(), 7);
        assert!(config.layouts.iter().any(|l| l.kind == LayoutKind::Linear));
        assert!(config.layouts.iter().any(|l| l.kind == LayoutKind::Mixed));
    }

    #[test]
    fn placement_overlaps() {
        let a = WidgetPlacement::new(WidgetKind::Cpu, 0, 0, 4, 4);
        let b = WidgetPlacement::new(WidgetKind::Gpu, 3, 3, 4, 4);
        let c = WidgetPlacement::new(WidgetKind::Ram, 4, 4, 2, 2);
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
    }

    #[test]
    fn find_free_slot_respects_pinned() {
        let mut layout = LayoutConfig::new("test", 4, 4);
        layout.widgets.push(pinned(WidgetKind::Cpu, 0, 0, 2, 2));
        assert_eq!(layout.find_free_slot(2, 2), (2, 0));
        assert_eq!(layout.find_free_slot(4, 1), (0, 2));
    }
}
