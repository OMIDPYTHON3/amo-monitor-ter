//! Verification that all eight chart styles render real monitoring data.

use std::collections::HashSet;
use std::time::Duration;

use amo_monitor::chart::{render_chart, ChartOptions, Series};
use amo_monitor::config::theme::ThemeSet;
use amo_monitor::config::{ChartType, YScale};
use amo_monitor::monitor::{Monitor, SeriesKey};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

fn points(monitor: &Monitor, key: SeriesKey) -> Vec<(f64, f64)> {
    monitor
        .history
        .get(key)
        .map(|h| h.samples().to_vec())
        .unwrap_or_default()
}

fn signature(buffer: &Buffer) -> Vec<char> {
    buffer.content().iter().map(|c| c.symbol().chars().next().unwrap_or(' ')).collect()
}

fn painted(buffer: &Buffer) -> usize {
    buffer.content().iter().filter(|c| c.symbol() != " ").count()
}

#[test]
fn every_style_renders_real_cpu_history() {
    let mut monitor = Monitor::new();
    for _ in 0..3 {
        std::thread::sleep(Duration::from_millis(260));
        monitor.sample();
    }
    let cpu = points(&monitor, SeriesKey::CpuUsage);
    assert!(cpu.len() >= 2, "monitor should have produced real CPU samples");
    assert!(cpu.iter().all(|(_, v)| *v >= 0.0 && *v <= 100.0));

    let theme = ThemeSet::builtin().resolve("Tokyo Night", &Default::default());
    let area = Rect::new(0, 0, 46, 7);
    let styles: HashSet<ChartType> = ChartType::ALL.iter().copied().collect();
    assert_eq!(styles.len(), 8);

    for style in ChartType::ALL {
        let options = ChartOptions {
            style,
            window_secs: 30.0,
            ..ChartOptions::default()
        };
        let mut buffer = Buffer::empty(area);
        render_chart(&mut buffer, area, &[Series::new(&cpu, theme.cpu)], &options, &theme);
        assert!(
            painted(&buffer) > 8,
            "{style:?} did not render the real CPU history"
        );
    }
}

#[test]
fn styles_are_visually_distinct() {
    let theme = ThemeSet::builtin().resolve("Dracula", &Default::default());
    // A realistic varying trace with a clear shape.
    let data: Vec<(f64, f64)> = (0..20)
        .map(|i| (i as f64, 20.0 + (i as f64 * 1.7).sin() * 15.0 + i as f64))
        .collect();
    let area = Rect::new(0, 0, 48, 8);
    let mut signatures: Vec<(ChartType, Vec<char>)> = Vec::new();
    for style in ChartType::ALL {
        let mut buffer = Buffer::empty(area);
        render_chart(
            &mut buffer,
            area,
            &[Series::new(&data, Color::Cyan)],
            &ChartOptions { style, ..ChartOptions::default() },
            &theme,
        );
        signatures.push((style, signature(&buffer)));
    }
    let unique: HashSet<Vec<char>> = signatures.iter().map(|(_, s)| s.clone()).collect();
    assert!(
        unique.len() >= 6,
        "chart styles should be visually distinct, got {} unique renders",
        unique.len()
    );
}

#[test]
fn grid_axes_scaling_and_thickness_change_output() {
    let theme = ThemeSet::builtin().resolve("Nord", &Default::default());
    let data: Vec<(f64, f64)> = (0..15).map(|i| (i as f64, 10.0 + i as f64 * 2.0)).collect();
    let area = Rect::new(0, 0, 40, 7);
    let base = ChartOptions { style: ChartType::BrailleLine, ..ChartOptions::default() };

    let render = |options: &ChartOptions| {
        let mut buffer = Buffer::empty(area);
        render_chart(&mut buffer, area, &[Series::new(&data, Color::Cyan)], options, &theme);
        signature(&buffer)
    };

    assert_ne!(render(&base), render(&ChartOptions { show_grid: false, ..base.clone() }));
    assert_ne!(render(&base), render(&ChartOptions { show_axes: true, ..base.clone() }));
    assert_ne!(render(&base), render(&ChartOptions { thickness: 3, ..base.clone() }));
    assert_ne!(
        render(&base),
        render(&ChartOptions {
            y_scale: YScale::Manual,
            manual_max: 500.0,
            ..base.clone()
        })
    );
}

#[test]
fn window_setting_controls_visible_history() {
    let theme = ThemeSet::builtin().resolve("Light", &Default::default());
    // Five minutes of synthetic samples at one-second intervals.
    let data: Vec<(f64, f64)> = (0..300)
        .map(|i| (i as f64, 25.0 + (i as f64 / 12.0).sin() * 10.0))
        .collect();
    let area = Rect::new(0, 0, 60, 6);

    let render = |window: f64| {
        let mut buffer = Buffer::empty(area);
        render_chart(
            &mut buffer,
            area,
            &[Series::new(&data, Color::Blue)],
            &ChartOptions { window_secs: window, ..ChartOptions::default() },
            &theme,
        );
        painted(&buffer)
    };

    let short = render(30.0);
    let minute = render(60.0);
    let five_minutes = render(300.0);
    assert!(short > 0 && minute > 0 && five_minutes > 0);
    // All windows fill the width with a connected line, but longer windows
    // retain more of the waveform's shape.
    assert!(minute >= short);
    assert!(five_minutes >= short);
}

#[test]
fn multiple_series_use_distinct_colours() {
    let theme = ThemeSet::builtin().resolve("Catppuccin", &Default::default());
    let down: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, 80.0)).collect();
    let up: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, 20.0)).collect();
    let area = Rect::new(0, 0, 30, 5);
    let mut buffer = Buffer::empty(area);
    render_chart(
        &mut buffer,
        area,
        &[
            Series::new(&down, theme.net_down),
            Series::new(&up, theme.net_up),
        ],
        &ChartOptions { style: ChartType::MultiLine, ..ChartOptions::default() },
        &theme,
    );
    let colours: HashSet<Color> = buffer
        .content()
        .iter()
        .filter(|c| c.symbol() != " ")
        .map(|c| c.fg)
        .collect();
    assert!(colours.contains(&theme.net_down));
    assert!(colours.contains(&theme.net_up));
}
