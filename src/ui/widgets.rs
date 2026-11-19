//! Rendering of individual monitoring widgets — compact modern cards.

use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, Mode};
use crate::chart::{render_chart, ChartOptions, Series};
use crate::colors::Metric as ColorMetric;
use crate::config::theme::ResolvedTheme;
use crate::config::{ChartType, DisplayMode, ViewMode, WidgetKind, WidgetPlacement};
use crate::monitor::SeriesKey;
use crate::responsive::{self, ChartPresentation, SizeClass};
use crate::util::{fmt_bytes, fmt_opt, fmt_rate, fmt_uptime, read_loadavg};

/// Colour for a value: dynamic when enabled, otherwise the theme's static
/// metric colour.
fn value_color(app: &App, metric: ColorMetric, fallback: Color) -> Color {
    let cfg = &app.config.dynamic_colors;
    if !cfg.enabled || !cfg.metric(metric).enabled {
        return fallback;
    }
    cfg.color_for(metric, metric.value(&app.monitor.snapshot), &app.resolved)
}

fn metric_color(kind: WidgetKind, theme: &ResolvedTheme) -> Color {
    match kind {
        WidgetKind::Cpu => theme.cpu,
        WidgetKind::Gpu => theme.gpu,
        WidgetKind::Ram => theme.memory,
        WidgetKind::Swap => theme.swap,
        WidgetKind::Network => theme.net_down,
        WidgetKind::DiskIo => theme.disk_read,
        WidgetKind::Uptime => theme.accent,
    }
}

/// A metric row: muted label on the left, bright value right-aligned so
/// numbers form a clean vertical column.
fn field(
    label: &str,
    value: String,
    color: Color,
    theme: &ResolvedTheme,
    width: u16,
    emphasis: bool,
) -> Line<'static> {
    let label: String = label.chars().take(9).collect();
    let value_width = width.saturating_sub(11);
    let value: String = value.chars().take(value_width as usize).collect();
    let padding = value_width.saturating_sub(value.chars().count() as u16) as usize;
    let mut style = Style::default().fg(color);
    if emphasis {
        style = style.add_modifier(Modifier::BOLD);
    }
    Line::from(vec![
        Span::styled(format!("{label:<9}"), Style::default().fg(theme.muted)),
        Span::raw(" "),
        Span::styled(format!("{}{}", " ".repeat(padding), value), style),
    ])
}

/// A slim progress line, quieter than a solid block bar.
fn progress(ratio: Option<f64>, color: Color, width: usize) -> Line<'static> {
    let ratio = ratio.unwrap_or(0.0).clamp(0.0, 1.0);
    let filled = (ratio * width as f64).round() as usize;
    let mut spans = Vec::with_capacity(2);
    if filled > 0 {
        spans.push(Span::styled("━".repeat(filled), Style::default().fg(color)));
    }
    let rest = width.saturating_sub(filled);
    if rest > 0 {
        spans.push(Span::styled("─".repeat(rest), Style::default().fg(Color::DarkGray)));
    }
    Line::from(spans)
}

fn core_line(values: &[f64], theme: &ResolvedTheme) -> Line<'static> {
    const STEPS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let spans: Vec<Span> = values
        .iter()
        .take(24)
        .map(|v| {
            let index = ((v / 100.0).clamp(0.0, 1.0) * 7.0).round() as usize;
            let color = if *v >= 80.0 {
                theme.danger
            } else if *v >= 50.0 {
                theme.warning
            } else {
                theme.cpu
            };
            Span::styled(STEPS[index].to_string(), Style::default().fg(color))
        })
        .collect();
    let mut all = vec![Span::styled("Cores     ", Style::default().fg(theme.muted))];
    all.extend(spans);
    Line::from(all)
}

fn history_points(app: &App, key: SeriesKey) -> Vec<(f64, f64)> {
    app.monitor
        .history
        .get(key)
        .map(|h| h.samples().to_vec())
        .unwrap_or_default()
}

/// Additional real series shown by the Multi-Line style for widgets that
/// normally have only one trace.
fn extra_series(kind: WidgetKind, theme: &ResolvedTheme) -> Vec<(SeriesKey, Color, f64)> {
    match kind {
        WidgetKind::Cpu => vec![
            (SeriesKey::CpuFrequency, theme.accent_alt, 5000.0),
            (SeriesKey::CpuPower, theme.warning, 150.0),
            (SeriesKey::CpuTemperature, theme.danger, 100.0),
        ],
        WidgetKind::Gpu => vec![
            (SeriesKey::GpuFrequency, theme.accent_alt, 3000.0),
            (SeriesKey::GpuPower, theme.warning, 450.0),
            (SeriesKey::GpuTemperature, theme.danger, 100.0),
        ],
        WidgetKind::Ram => vec![(SeriesKey::SwapUsed, theme.swap, 100.0)],
        WidgetKind::Swap => vec![(SeriesKey::MemoryUsed, theme.memory, 100.0)],
        _ => Vec::new(),
    }
}

pub fn render_widget(
    frame: &mut Frame,
    area: Rect,
    placement: &WidgetPlacement,
    app: &App,
    selected: bool,
    class: SizeClass,
) {
    if area.width < 4 || area.height < 3 {
        return;
    }
    let theme = &app.resolved;
    let kind = placement.kind;
    let color = metric_color(kind, theme);

    // Selection is a refined border + accent marker, never a filled block.
    let border_color = if selected {
        match app.mode {
            Mode::Move => theme.warning,
            Mode::Resize => theme.accent_alt,
            _ => theme.accent,
        }
    } else {
        theme.border
    };
    let mut title_spans = vec![
        Span::styled("▪ ", Style::default().fg(color)),
        Span::styled(
            kind.title(),
            Style::default()
                .fg(if selected { theme.accent } else { theme.text })
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if placement.pinned {
        title_spans.push(Span::styled(
            "  •",
            Style::default().fg(if selected { theme.accent } else { theme.warning }),
        ));
    }
    if selected {
        match app.mode {
            Mode::Move => title_spans.push(Span::styled("  MOVE", Style::default().fg(theme.warning))),
            Mode::Resize => title_spans.push(Span::styled("  RESIZE", Style::default().fg(theme.accent_alt))),
            _ => {}
        }
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(symbols::border::ROUNDED)
        .border_style(Style::default().fg(border_color))
        .title(Line::from(title_spans))
        .style(Style::default().bg(theme.panel).fg(theme.text));
    frame.render_widget(&block, area);
    let inner = area.inner(Margin::new(1, 1));
    if inner.width < 6 || inner.height < 2 {
        return;
    }

    let cfg = &placement.config;
    let snap = &app.monitor.snapshot;
    let effective_mode = responsive::adapt_detail(cfg.display_mode, class, area);
    let compact = effective_mode == DisplayMode::Compact;
    let detailed = effective_mode == DisplayMode::Detailed;
    let label_width = inner.width;

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut chart_keys: Vec<SeriesKey> = Vec::new();

    match kind {
        WidgetKind::Cpu => {
            let cpu = &snap.cpu;
            if cfg.show_primary {
                lines.push(field(
                    "Usage",
                    fmt_opt(cpu.usage_percent, 1, "%"),
                    value_color(app, ColorMetric::CpuUsage, color),
                    theme,
                    label_width,
                    true,
                ));
                lines.push(progress(
                    cpu.usage_percent.map(|v| v / 100.0),
                    value_color(app, ColorMetric::CpuUsage, color),
                    label_width as usize,
                ));
            }
            if !compact {
                if cfg.show_frequency {
                    lines.push(field(
                        "Freq",
                        fmt_opt(cpu.frequency_mhz, 0, " MHz"),
                        value_color(app, ColorMetric::CpuFreq, theme.text),
                        theme,
                        label_width,
                        false,
                    ));
                }
                if cfg.show_power {
                    lines.push(field(
                        "Power",
                        fmt_opt(cpu.power_w, 2, " W"),
                        value_color(app, ColorMetric::CpuPower, theme.text),
                        theme,
                        label_width,
                        false,
                    ));
                }
                if cfg.show_temperature {
                    lines.push(field(
                        "Temp",
                        fmt_opt(cpu.temperature_c, 1, " °C"),
                        value_color(app, ColorMetric::CpuTemp, theme.text),
                        theme,
                        label_width,
                        false,
                    ));
                }
            }
            if detailed && cfg.show_extra {
                if !cpu.per_core.is_empty() {
                    lines.push(core_line(&cpu.per_core, theme));
                }
                lines.push(field(
                    "Cores",
                    format!("{} logical", cpu.cores),
                    theme.text,
                    theme,
                    label_width,
                    false,
                ));
            }
            chart_keys.push(SeriesKey::CpuUsage);
        }
        WidgetKind::Gpu => {
            let gpu = &snap.gpu;
            if detailed {
                lines.push(field(
                    "Device",
                    if gpu.name.is_empty() { "N/A" } else { &gpu.name }.to_string(),
                    if gpu.available { theme.text } else { theme.muted },
                    theme,
                    label_width,
                    false,
                ));
            }
            if cfg.show_primary {
                lines.push(field(
                    "Usage",
                    fmt_opt(gpu.usage_percent, 1, "%"),
                    value_color(app, ColorMetric::GpuUsage, color),
                    theme,
                    label_width,
                    true,
                ));
                lines.push(progress(
                    gpu.usage_percent.map(|v| v / 100.0),
                    value_color(app, ColorMetric::GpuUsage, color),
                    label_width as usize,
                ));
            }
            if !compact {
                if cfg.show_frequency {
                    lines.push(field(
                        "Freq",
                        fmt_opt(gpu.frequency_mhz, 0, " MHz"),
                        value_color(app, ColorMetric::GpuFreq, theme.text),
                        theme,
                        label_width,
                        false,
                    ));
                }
                if cfg.show_power {
                    lines.push(field(
                        "Power",
                        fmt_opt(gpu.power_w, 2, " W"),
                        value_color(app, ColorMetric::GpuPower, theme.text),
                        theme,
                        label_width,
                        false,
                    ));
                }
                if cfg.show_temperature {
                    lines.push(field(
                        "Temp",
                        fmt_opt(gpu.temperature_c, 1, " °C"),
                        value_color(app, ColorMetric::GpuTemp, theme.text),
                        theme,
                        label_width,
                        false,
                    ));
                }
            }
            chart_keys.push(SeriesKey::GpuUsage);
        }
        WidgetKind::Ram | WidgetKind::Swap => {
            let mem = if kind == WidgetKind::Ram {
                &snap.memory
            } else {
                &snap.swap
            };
            let metric = if kind == WidgetKind::Ram {
                ColorMetric::Ram
            } else {
                ColorMetric::Swap
            };
            if cfg.show_primary {
                lines.push(field(
                    "Used",
                    fmt_opt(mem.percent, 1, "%"),
                    value_color(app, metric, color),
                    theme,
                    label_width,
                    true,
                ));
                lines.push(progress(
                    mem.percent.map(|v| v / 100.0),
                    value_color(app, metric, color),
                    label_width as usize,
                ));
            }
            if detailed && cfg.show_extra {
                let used = mem.used_bytes.map(|v| fmt_bytes(v as f64)).unwrap_or_default();
                let total = mem.total_bytes.map(|v| fmt_bytes(v as f64)).unwrap_or_default();
                let value = if mem.total_bytes == Some(0) {
                    "none".to_string()
                } else {
                    format!("{used} / {total}")
                };
                lines.push(field("Memory", value, theme.text, theme, label_width, false));
            }
            chart_keys.push(if kind == WidgetKind::Ram {
                SeriesKey::MemoryUsed
            } else {
                SeriesKey::SwapUsed
            });
        }
        WidgetKind::Network => {
            let net = &snap.network;
            if cfg.show_primary {
                lines.push(field(
                    "Down",
                    fmt_rate(net.rx_bytes_per_sec),
                    value_color(app, ColorMetric::NetRx, theme.net_down),
                    theme,
                    label_width,
                    true,
                ));
                lines.push(field(
                    "Up",
                    fmt_rate(net.tx_bytes_per_sec),
                    value_color(app, ColorMetric::NetTx, theme.net_up),
                    theme,
                    label_width,
                    true,
                ));
            }
            if detailed && cfg.show_extra {
                lines.push(field("Iface", net.interface.clone(), theme.text, theme, label_width, false));
            }
            chart_keys.push(SeriesKey::NetworkRx);
            chart_keys.push(SeriesKey::NetworkTx);
        }
        WidgetKind::DiskIo => {
            let disk = &snap.disk;
            if cfg.show_primary {
                lines.push(field(
                    "Read",
                    fmt_rate(disk.read_bytes_per_sec),
                    value_color(app, ColorMetric::DiskRead, theme.disk_read),
                    theme,
                    label_width,
                    true,
                ));
                lines.push(field(
                    "Write",
                    fmt_rate(disk.write_bytes_per_sec),
                    value_color(app, ColorMetric::DiskWrite, theme.disk_write),
                    theme,
                    label_width,
                    true,
                ));
            }
            if detailed && cfg.show_extra {
                lines.push(field(
                    "Disks",
                    disk.device_count.to_string(),
                    theme.text,
                    theme,
                    label_width,
                    false,
                ));
            }
            chart_keys.push(SeriesKey::DiskRead);
            chart_keys.push(SeriesKey::DiskWrite);
        }
        WidgetKind::Uptime => {
            if cfg.show_primary {
                lines.push(field(
                    "Uptime",
                    fmt_uptime(snap.system.uptime_secs),
                    color,
                    theme,
                    label_width,
                    true,
                ));
            }
            if !compact || detailed {
                lines.push(field("Host", snap.system.hostname.clone(), theme.text, theme, label_width, false));
                lines.push(field("Kernel", snap.system.kernel.clone(), theme.text, theme, label_width, false));
                lines.push(field(
                    "Load",
                    fmt_opt(read_loadavg(), 2, ""),
                    theme.text,
                    theme,
                    label_width,
                    false,
                ));
                lines.push(field(
                    "Arch",
                    std::env::consts::ARCH.to_string(),
                    theme.text,
                    theme,
                    label_width,
                    false,
                ));
            }
            if cfg.show_extra && (!compact || detailed) && !snap.cpu.model.is_empty() {
                lines.push(field(
                    "Model",
                    snap.cpu.model.clone(),
                    theme.text,
                    theme,
                    label_width,
                    false,
                ));
            }
        }
    }

    let show_numbers = matches!(cfg.view_mode, ViewMode::NumbersOnly | ViewMode::NumbersAndCharts);
    let presentation = responsive::adapt_chart(class, inner);
    let show_chart = cfg.show_chart
        && matches!(cfg.view_mode, ViewMode::NumbersAndCharts | ViewMode::ChartsOnly)
        && !chart_keys.is_empty()
        && presentation != ChartPresentation::Hidden;

    let mut lines = if show_numbers { lines } else { Vec::new() };
    // Reserve room for a modest chart; never let details crowd it out.
    let max_lines = if show_chart {
        inner.height.saturating_sub(4).max(1) as usize
    } else {
        inner.height as usize
    };
    if lines.len() > max_lines {
        lines.truncate(max_lines);
    }
    let numbers_height = lines.len() as u16;
    if !lines.is_empty() {
        frame.render_widget(
            Paragraph::new(lines),
            Rect {
                height: numbers_height,
                ..inner
            },
        );
    }

    if show_chart {
        let remaining = inner.height.saturating_sub(numbers_height);
        // Charts are proportional to their space but never stretched to fill
        // a tall card; a small gap keeps them visually centred.
        let chart_height = remaining.min(5);
        if chart_height >= 2 {
            let gap = (remaining - chart_height) / 2;
            let chart_area = Rect {
                y: inner.y + numbers_height + gap,
                height: chart_height,
                ..inner
            };
            let mut specs: Vec<(SeriesKey, Color, Option<f64>)> = chart_keys
                .iter()
                .map(|key| (*key, theme.metric_color(*key), None))
                .collect();
            if cfg.chart_type == ChartType::MultiLine {
                specs.extend(
                    extra_series(kind, theme)
                        .into_iter()
                        .map(|(key, color, scale)| (key, color, Some(scale))),
                );
            }
            let histories: Vec<Vec<(f64, f64)>> = specs
                .iter()
                .map(|(key, _, _)| history_points(app, *key))
                .collect();
            let series: Vec<Series> = specs
                .iter()
                .zip(histories.iter())
                .map(|((_, color, scale), points)| Series {
                    points,
                    color: *color,
                    scale: *scale,
                    label: "",
                })
                .collect();
            let options = ChartOptions {
                style: cfg.chart_type,
                window_secs: cfg.window(),
                show_grid: cfg.show_grid && responsive::show_chart_grid(class, presentation),
                show_axes: cfg.show_axes && responsive::show_chart_axes(class, presentation),
                y_scale: cfg.y_scale,
                manual_max: cfg.manual_max,
                thickness: cfg.thickness(),
            };
            render_chart(frame.buffer_mut(), chart_area, &series, &options, theme);
        }
    }
}
