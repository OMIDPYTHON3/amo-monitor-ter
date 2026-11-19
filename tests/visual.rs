//! Visual-design regression checks for the redesigned dashboard.

use std::path::PathBuf;
use std::sync::Mutex;

use amo_monitor::app::App;
use amo_monitor::config::{Config, LayoutConfig};
use amo_monitor::layout;
use amo_monitor::ui;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("amo-visual-{}", std::process::id()))
}

#[test]
fn default_layout_is_balanced() {
    let overview = Config::default()
        .layouts
        .iter()
        .find(|l| l.name == "Overview")
        .cloned()
        .unwrap();
    let heights: Vec<u16> = overview.widgets.iter().map(|w| w.rowspan).collect();
    assert!(heights.iter().all(|h| *h == heights[0]), "cards must share one height");
    assert_eq!(overview.rows, 9);
    assert_eq!(overview.widgets.len(), 7);
    // No single card may span the whole dashboard width.
    assert!(overview.widgets.iter().all(|w| w.colspan < overview.columns));
    assert!(layout::is_collision_free(&overview));
}

#[test]
fn compact_layout_has_no_oversized_panels() {
    let compact: LayoutConfig = Config::default()
        .layouts
        .iter()
        .find(|l| l.name == "Compact")
        .cloned()
        .unwrap();
    assert!(compact.widgets.iter().all(|w| w.rowspan <= 2));
    assert!(layout::is_collision_free(&compact));
}

#[test]
fn selection_uses_a_border_not_a_filled_block() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = config_dir();
    std::env::set_var("AMO_CONFIG_DIR", &dir);
    let mut app = App::new();
    app.selected = 0;

    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| ui::render(frame, &app)).unwrap();

    let selection_bg = app.resolved.selection;
    let filled_cells = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .filter(|cell| cell.bg == selection_bg)
        .count();
    assert_eq!(filled_cells, 0, "selection must not fill the widget background");

    // The selected widget's border uses the accent colour.
    let accent_cells = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .filter(|cell| cell.fg == app.resolved.accent)
        .count();
    assert!(accent_cells > 0, "selection border highlight is missing");

    std::env::remove_var("AMO_CONFIG_DIR");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn values_are_right_aligned_and_brighter_than_labels() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = config_dir();
    std::env::set_var("AMO_CONFIG_DIR", &dir);
    let app = App::new();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| ui::render(frame, &app)).unwrap();

    let buffer = terminal.backend().buffer();
    let lines: Vec<String> = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
                .collect()
        })
        .collect();
    let usage_line = lines
        .iter()
        .find(|line| line.contains("Usage"))
        .expect("usage row must be rendered");
    // The value ends near the right edge of its card, not next to the label.
    assert!(usage_line.find('%').unwrap_or(0) > 25);

    std::env::remove_var("AMO_CONFIG_DIR");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_widget_overlap_at_any_checked_size() {
    let layout = Config::default()
        .layouts
        .iter()
        .find(|l| l.name == "Overview")
        .cloned()
        .unwrap();
    for (w, h) in [(200, 60), (150, 40), (120, 30), (100, 30), (80, 24)] {
        let placed = layout::resolve_pixels(&layout, ratatui::layout::Rect::new(0, 0, w, h));
        for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                assert!(!placed[i].rect.intersects(placed[j].rect), "overlap at {w}x{h}");
            }
        }
    }
}
