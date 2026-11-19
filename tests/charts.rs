//! Integration tests for rolling history and continuous braille line charts.

use std::collections::HashSet;

use amo_monitor::chart::{nice_ceiling, render_line_chart, Series};
use amo_monitor::config::theme::ThemeSet;
use amo_monitor::monitor::{History, HISTORY_WINDOW_SECS};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

fn is_braille(symbol: &str) -> bool {
    symbol.chars().next().is_some_and(|c| ('\u{2800}'..='\u{28ff}').contains(&c))
}

/// Horizontal braille sub-pixel columns that contain at least one painted dot.
fn plotted_columns(buffer: &Buffer, area: Rect) -> HashSet<usize> {
    let mut columns = HashSet::new();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let symbol = buffer[(x, y)].symbol();
            let Some(ch) = symbol.chars().next() else {
                continue;
            };
            if !(0x2800..=0x28FF).contains(&(ch as u32)) {
                continue;
            }
            let bits = ch as u32 - 0x2800;
            if bits == 0 {
                continue;
            }
            // Ratatui braille bits: index = row*2 + column. The bit values are
            // not 1<<index (dots 0..2 are in the low byte, 3..5 in the high).
            const BRAILLE: [u32; 8] = [0x01, 0x08, 0x02, 0x10, 0x04, 0x20, 0x40, 0x80];
            for index in 0..8 {
                if bits & BRAILLE[index] != 0 {
                    columns.insert(((x - area.x) as usize) * 2 + (index % 2));
                }
            }
        }
    }
    columns
}

#[test]
fn history_keeps_only_the_rolling_window() {
    let mut h = History::new(HISTORY_WINDOW_SECS, 120);
    for t in 0..60 {
        h.push(t as f64, t as f64);
    }
    // Samples older than 30 seconds from the newest (t=59) must be dropped.
    assert_eq!(h.len(), 30);
    assert_eq!(h.latest(), Some(59.0));
    assert_eq!(h.samples().first().unwrap().0, 30.0);
    assert_eq!(h.max(), Some(59.0));
    assert_eq!(h.min(), Some(30.0));
}

#[test]
fn history_respects_capacity_and_ignores_nan() {
    let mut h = History::new(1000.0, 5);
    h.push(1.0, f64::NAN);
    h.push(2.0, f64::INFINITY);
    assert!(h.is_empty());
    for i in 0..10 {
        h.push(i as f64, i as f64);
    }
    assert_eq!(h.len(), 5);
    assert_eq!(h.latest(), Some(9.0));
    h.clear();
    assert!(h.is_empty());
}

#[test]
fn chart_writes_braille_cells() {
    let theme = ThemeSet::builtin().resolve("Dark", &Default::default());
    let points: Vec<(f64, f64)> = (0..30).map(|i| (i as f64, i as f64 * 3.0)).collect();
    let area = Rect::new(0, 0, 30, 6);
    let mut buffer = Buffer::empty(area);
    render_line_chart(
        &mut buffer,
        area,
        &[Series::new(&points, Color::Cyan)],
        None,
        &theme,
    );
    let plotted = buffer.content().iter().filter(|c| is_braille(c.symbol())).count();
    assert!(plotted > 10, "expected a drawn line, found {plotted} cells");
}

#[test]
fn line_is_continuous_across_the_whole_width() {
    let theme = ThemeSet::builtin().resolve("Dark", &Default::default());
    // Only five samples on a wide chart: a dot renderer lights five columns,
    // a connected line must fill essentially every sub-pixel column.
    let points = vec![(0.0, 10.0), (1.0, 20.0), (2.0, 15.0), (3.0, 40.0), (4.0, 30.0)];
    let area = Rect::new(0, 0, 40, 6);
    let mut buffer = Buffer::empty(area);
    render_line_chart(
        &mut buffer,
        area,
        &[Series::new(&points, Color::Cyan)],
        None,
        &theme,
    );
    let columns = plotted_columns(&buffer, area);
    for x in 0..(area.width as usize * 2) {
        assert!(columns.contains(&x), "line is missing at sub-pixel column {x}");
    }
}

#[test]
fn multi_series_chart_uses_each_series_colour() {
    let theme = ThemeSet::builtin().resolve("Nord", &Default::default());
    let a: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, 10.0)).collect();
    let b: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, 90.0)).collect();
    let area = Rect::new(0, 0, 10, 4);
    let mut buffer = Buffer::empty(area);
    render_line_chart(
        &mut buffer,
        area,
        &[
            Series::new(&a, Color::Red),
            Series::new(&b, Color::Blue),
        ],
        None,
        &theme,
    );
    let colours: HashSet<Color> = buffer
        .content()
        .iter()
        .filter(|c| is_braille(c.symbol()))
        .map(|c| c.fg)
        .collect();
    assert!(colours.contains(&Color::Red));
    assert!(colours.contains(&Color::Blue));
}

#[test]
fn y_axis_scales_dynamically_to_visible_values() {
    let theme = ThemeSet::builtin().resolve("Dark", &Default::default());
    let points: Vec<(f64, f64)> = (0..5).map(|i| (i as f64, 4.0 + i as f64)).collect();
    let area = Rect::new(0, 0, 20, 6);

    let mut auto = Buffer::empty(area);
    render_line_chart(
        &mut auto,
        area,
        &[Series::new(&points, Color::Cyan)],
        None,
        &theme,
    );
    let mut fixed = Buffer::empty(area);
    render_line_chart(
        &mut fixed,
        area,
        &[Series::new(&points, Color::Cyan)],
        Some(100.0),
        &theme,
    );

    // Without a floor the 8.0 peak reaches the top rows of the chart.
    let top_touched = (0..2).any(|y| {
        (0..20).any(|x| is_braille(auto[(x as u16, y as u16)].symbol()))
    });
    assert!(top_touched, "auto-scaled chart should use the full height");

    // With a 100 ceiling the same data stays near the bottom.
    let top_touched_fixed = (0..2).any(|y| {
        (0..20).any(|x| is_braille(fixed[(x as u16, y as u16)].symbol()))
    });
    assert!(!top_touched_fixed, "fixed ceiling should keep small values low");
}

#[test]
fn missing_samples_do_not_get_fabricated_lines() {
    let theme = ThemeSet::builtin().resolve("Dark", &Default::default());
    // Three regular samples, then a large hole where a sensor went missing.
    let points = vec![(0.0, 50.0), (1.0, 50.0), (2.0, 50.0), (10.0, 50.0)];
    let area = Rect::new(0, 0, 30, 4);
    let mut buffer = Buffer::empty(area);
    render_line_chart(
        &mut buffer,
        area,
        &[Series::new(&points, Color::Cyan)],
        None,
        &theme,
    );
    let columns = plotted_columns(&buffer, area);
    let pixels_x = area.width as usize * 2;
    // The regular samples end around 20% of the width; the last point is at
    // the far right. Columns in between must stay empty (no invented line).
    let middle_empty = (7..pixels_x - 3).any(|x| !columns.contains(&x));
    assert!(middle_empty, "a gap in sensor data must not be filled by a line");
}

#[test]
fn empty_and_zero_series_are_handled() {
    let theme = ThemeSet::builtin().resolve("Light", &Default::default());
    let area = Rect::new(0, 0, 12, 3);
    let mut empty = Buffer::empty(area);
    render_line_chart(&mut empty, area, &[], None, &theme);
    assert!(!empty.content().iter().any(|c| is_braille(c.symbol())));

    let zeros = vec![(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)];
    let mut zero_buffer = Buffer::empty(area);
    render_line_chart(
        &mut zero_buffer,
        area,
        &[Series::new(&zeros, Color::Cyan)],
        None,
        &theme,
    );
    // Zero is real data: the baseline row must be painted continuously.
    let columns = plotted_columns(&zero_buffer, area);
    assert!(columns.len() >= area.width as usize * 2 - 1);
}

#[test]
fn chart_ceiling_scales_to_data() {
    assert_eq!(nice_ceiling(0.0), 1.0);
    assert_eq!(nice_ceiling(50_000.0), 50_000.0);
    assert!(nice_ceiling(123.0) >= 123.0);
}
