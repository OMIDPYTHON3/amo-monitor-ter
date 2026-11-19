//! Compact linear dashboard: flowing metric chips with automatic wrapping.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::chart::{render_chart, ChartOptions, Series};
use crate::config::{DisplayMode, LayoutConfig, LinearMetric, LinearMetricKind};
use crate::monitor::SeriesKey;
use crate::responsive::SizeClass;

/// Sort metrics by group and order, keeping pinned metrics ahead within their
/// group so pinning has a visible, predictable effect.
fn ordered_metrics(layout: &LayoutConfig) -> Vec<(usize, &LinearMetric)> {
    let mut metrics: Vec<(usize, &LinearMetric)> = layout.linear_metrics.iter().enumerate().collect();
    metrics.sort_by_key(|(_, m)| {
        let group = m.kind.group().max(m.group);
        (group, !m.pinned, m.order)
    });
    metrics
}

fn series_points(app: &App, key: SeriesKey, window: f64) -> Vec<(f64, f64)> {
    let Some(history) = app.monitor.history.get(key) else {
        return Vec::new();
    };
    let latest = history.samples().last().map(|p| p.0).unwrap_or(0.0);
    history
        .samples()
        .iter()
        .filter(|(t, _)| latest - *t <= window)
        .map(|(t, v)| (*t, *v))
        .collect()
}

/// Render a single chip and return its consumed width.
fn compact_value(kind: crate::config::LinearMetricKind, app: &App) -> String {
    use crate::config::LinearMetricKind as K;
    let snapshot = &app.monitor.snapshot;
    match kind {
        K::Uptime => crate::util::fmt_uptime(snapshot.system.uptime_secs).replace(' ', ""),
        K::NetRx | K::NetTx | K::DiskRead | K::DiskWrite => {
            let Some(v) = kind.value(snapshot) else {
                return "N/A".to_string();
            };
            if v >= 1_000_000.0 {
                format!("{:.0}M", v / 1_000_000.0)
            } else if v >= 1_000.0 {
                format!("{:.0}K", v / 1_000.0)
            } else {
                format!("{:.0}", v)
            }
        }
        _ => kind.display(snapshot),
    }
}

fn draw_chip(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    max_width: u16,
    metric: &LinearMetric,
    app: &App,
    class: SizeClass,
) -> u16 {
    let theme = &app.resolved;
    let cfg = &app.config.dynamic_colors;
    let value = metric.kind.value(&app.monitor.snapshot);
    let color = metric
        .kind
        .color_metric()
        .map(|m| cfg.color_for(m, value, theme))
        .unwrap_or(theme.accent);
    let mut text = if class == SizeClass::Minimal {
        compact_value(metric.kind, app)
    } else {
        metric.kind.display(&app.monitor.snapshot)
    };
    let mut label = metric.kind.label();
    let compact = class != SizeClass::Normal;
    if compact {
        label = metric.kind.short_label();
    }

    let effective_mode = crate::responsive::adapt_detail(
        app.active_layout().linear_display,
        class,
        Rect::new(0, 0, max_width, 1),
    );
    let detailed = effective_mode == DisplayMode::Detailed;
    let medium = effective_mode == DisplayMode::Medium;
    let chart_width = if metric.show_chart && !compact && max_width >= 24 && detailed { 8 } else { 0 };
    let bar_width = if !compact && (medium || detailed) { 5 } else { 0 };

    let mut label_width = label.chars().count() as u16;
    let mut text_width = text.chars().count() as u16;
    let overhead = 2 + bar_width + chart_width;
    // Never clip: shorten the value first, then give up if even a short chip
    // cannot fit in the available width.
    if label_width + 1 + text_width + overhead > max_width {
        let keep = max_width.saturating_sub(label_width + overhead + 2) as usize;
        if keep >= 2 {
            let shortened: String = text.chars().take(keep).collect();
            text = format!("{shortened}…");
            text_width = text.chars().count() as u16;
        } else {
            label = metric.kind.short_label();
            label_width = label.chars().count() as u16;
        }
    }
    if label_width + 1 + text_width + overhead > max_width {
        return 0;
    }
    let total = label_width + 1 + text_width + overhead;

    let style = Style::default().bg(theme.panel_alt);
    for cell_x in x..x + total {
        if let Some(cell) = buf.cell_mut((cell_x, y)) {
            cell.set_char(' ').set_style(style);
        }
    }
    buf.set_string(
        x + 1,
        y,
        label,
        Style::default().fg(if metric.pinned { theme.warning } else { theme.muted }).bg(theme.panel_alt),
    );
    buf.set_string(
        x + 1 + label_width + 1,
        y,
        text,
        Style::default().fg(color).bg(theme.panel_alt).add_modifier(Modifier::BOLD),
    );

    let mut cursor = x + 1 + label_width + 1 + text_width + 1;
    if bar_width > 0 {
        let ratio = value.map(|v| (v / 100.0).clamp(0.0, 1.0)).unwrap_or(0.0);
        let filled = (ratio * bar_width as f64).round() as u16;
        for i in 0..bar_width {
            let (ch, c) = if i < filled {
                ('█', color)
            } else {
                ('░', theme.chart_grid)
            };
            if let Some(cell) = buf.cell_mut((cursor + i, y)) {
                cell.set_char(ch).set_style(Style::default().fg(c).bg(theme.panel_alt));
            }
        }
        cursor += bar_width;
    }
    if chart_width > 0 {
        if let Some(key) = metric.kind.series_key() {
            let points = series_points(app, key, 30.0);
            let area = Rect::new(cursor, y, chart_width, 1);
            let options = ChartOptions {
                style: crate::config::ChartType::Sparkline,
                show_grid: false,
                show_axes: false,
                ..ChartOptions::default()
            };
            render_chart(buf, area, &[Series::new(&points, color)], &options, theme);
        }
    }
    total
}

/// Flow compact metric chips across the available width.
pub fn render_linear(frame: &mut Frame, area: Rect, layout: &LayoutConfig, app: &App, class: SizeClass) {
    if area.width < 8 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(app.resolved.background)),
        area,
    );
    let mut metrics = ordered_metrics(layout);
    if class == SizeClass::Minimal {
        metrics.retain(|(_, metric)| crate::responsive::is_essential(metric.kind));
        // Fall back to the essential defaults if the user's layout has none.
        if metrics.is_empty() {
            let defaults = crate::config::Config::default();
            let source = defaults
                .layouts
                .iter()
                .find(|l| l.kind == crate::config::LayoutKind::Linear)
                .expect("default linear layout exists");
            return render_linear(frame, area, source, app, class);
        }
    }
    if metrics.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from("No metrics configured — press 'y' to add metrics."))
                .style(Style::default().fg(app.resolved.muted)),
            area,
        );
        return;
    }

    let buf = frame.buffer_mut();
    let mut x = area.x;
    let mut y = area.y;
    for (_, metric) in metrics {
        if y >= area.bottom() {
            break;
        }
        let available = area.right().saturating_sub(x);
        let width = draw_chip(buf, x, y, available, metric, app, class);
        if width == 0 {
            // Does not fit: wrap, or skip when the terminal is too narrow.
            if x > area.x {
                x = area.x;
                y += 1;
                let width = draw_chip(buf, x, y, area.width, metric, app, class);
                if width > 0 {
                    x += width + 1;
                }
            }
            continue;
        }
        x += width + 1;
        if x >= area.right() - 1 {
            x = area.x;
            y += 1;
        }
    }
}

/// Render the Mixed dashboard: a compact strip above a grid of panels.
pub fn render_mixed(frame: &mut Frame, area: Rect, layout: &LayoutConfig, app: &App, class: SizeClass) {
    if area.height < 6 {
        crate::ui::dashboard::render_grid(frame, area, app, class);
        return;
    }
    let strip_height = match class {
        SizeClass::Normal => 6,
        SizeClass::Compact => 4,
        SizeClass::Minimal => 3,
    }
    .min(area.height / 3);
    let strip = Rect { height: strip_height, ..area };
    let grid_area = Rect {
        y: area.y + strip_height,
        height: area.height - strip_height,
        ..area
    };
    render_linear(frame, strip, layout, app, class);
    if let Some(cell) = frame.buffer_mut().cell_mut((area.x, area.y + strip_height - 1)) {
        cell.set_char('─')
            .set_style(Style::default().fg(app.resolved.border));
    }
    crate::ui::dashboard::render_grid(frame, grid_area, app, class);
}

/// Small helper used by the settings preview.
pub fn chip_width(metric: &LinearMetric, app: &App) -> u16 {
    let text = metric.kind.display(&app.monitor.snapshot);
    metric.kind.label().chars().count() as u16 + text.chars().count() as u16 + 3
}

/// Draw a single metric chip at a fixed position (used by previews).
pub fn render_preview(frame: &mut Frame, area: Rect, kind: LinearMetricKind, app: &App) {
    let metric = LinearMetric::new(kind, kind.group(), 0);
    let _ = draw_chip(
        frame.buffer_mut(),
        area.x,
        area.y,
        area.width,
        &metric,
        app,
        SizeClass::Normal,
    );
}
