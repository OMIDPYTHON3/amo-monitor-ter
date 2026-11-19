//! Fully responsive dashboard verification across required terminal sizes.

use std::path::PathBuf;
use std::sync::Mutex;

use amo_monitor::app::App;
use amo_monitor::config::{LayoutConfig, LayoutKind, WidgetKind, WidgetPlacement};
use amo_monitor::responsive::{self, ChartPresentation, SizeClass};
use amo_monitor::ui;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

static ENV_LOCK: Mutex<()> = Mutex::new(());

const SIZES: [(u16, u16); 6] = [
    (60, 20),
    (80, 24),
    (100, 30),
    (120, 40),
    (160, 50),
    (200, 60),
];

fn config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("amo-responsive-{}", std::process::id()))
}

fn render_at(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| ui::render(frame, app))
        .unwrap_or_else(|e| panic!("render failed at {width}x{height}: {e}"));
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol().chars().next().unwrap_or(' '))
        .collect()
}

#[test]
fn all_required_sizes_render_every_dashboard_kind() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = config_dir();
    std::env::set_var("AMO_CONFIG_DIR", &dir);
    let mut app = App::new();

    for kind in LayoutKind::ALL {
        let name = app
            .config
            .layouts
            .iter()
            .find(|l| l.kind == kind)
            .map(|l| l.name.clone())
            .unwrap();
        app.config.active_layout = name;

        for (width, height) in SIZES {
            let screen = render_at(&app, width, height);
            assert!(screen.contains('A'), "{kind:?} blank at {width}x{height}");
            // No terminal cell may be left as an uninitialised placeholder.
            assert!(!screen.contains('\u{0}'), "{kind:?} NUL at {width}x{height}");
        }
    }
    std::env::remove_var("AMO_CONFIG_DIR");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn grid_structure_stays_stable_until_minimal() {
    let mut layout = LayoutConfig::new("test", 12, 8);
    for i in 0..6 {
        layout.widgets.push(WidgetPlacement::new(WidgetKind::Cpu, i * 2, 0, 2, 4));
    }
    for class in [SizeClass::Normal, SizeClass::Compact, SizeClass::Minimal] {
        let adapted = responsive::adapt_layout(&layout, class);
        assert_eq!(adapted.columns, 12, "columns must not churn on minor resizes");
        assert_eq!(adapted.rows, 8);
        assert!(amo_monitor::layout::is_collision_free(&adapted));
        assert_eq!(adapted.widgets, layout.widgets);
    }
}

#[test]
fn minimal_terminals_use_essential_metric_rows() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = config_dir();
    std::env::set_var("AMO_CONFIG_DIR", &dir);
    let mut app = App::new();
    app.config.active_layout = "Overview".to_string(); // Grid

    let screen = render_at(&app, 60, 20);
    // Very small terminals render compact metric chips, not panel borders.
    assert!(screen.contains("CPU"), "metrics must remain visible when very small");
    assert!(!screen.contains("┌ CPU"), "large panels must be replaced when very small");
    assert_eq!(responsive::effective_kind(LayoutKind::Grid, SizeClass::Minimal), LayoutKind::Linear);
    assert_eq!(responsive::effective_kind(LayoutKind::Grid, SizeClass::Compact), LayoutKind::Grid);

    std::env::remove_var("AMO_CONFIG_DIR");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn charts_are_hidden_when_small_and_restored_when_space_returns() {
    let full = Rect::new(0, 0, 60, 14);
    assert_eq!(responsive::adapt_chart(SizeClass::Normal, full), ChartPresentation::Full);
    // Small widgets hide the chart instead of switching to a sparkline.
    assert_eq!(
        responsive::adapt_chart(SizeClass::Normal, Rect::new(0, 0, 20, 4)),
        ChartPresentation::Hidden
    );
    // Compact mode asks for more space before showing a chart.
    assert_eq!(
        responsive::adapt_chart(SizeClass::Compact, Rect::new(0, 0, 30, 6)),
        ChartPresentation::Hidden
    );
    assert_eq!(
        responsive::adapt_chart(SizeClass::Compact, Rect::new(0, 0, 50, 9)),
        ChartPresentation::Full
    );
    // Minimal mode never shows charts.
    assert_eq!(
        responsive::adapt_chart(SizeClass::Minimal, Rect::new(0, 0, 80, 20)),
        ChartPresentation::Hidden
    );
    // Restored space restores the chart without touching settings.
    assert_eq!(responsive::adapt_chart(SizeClass::Normal, full), ChartPresentation::Full);
}

#[test]
fn responsive_rendering_never_mutates_saved_settings() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = config_dir();
    std::env::set_var("AMO_CONFIG_DIR", &dir);
    let app = App::new();
    let before = app.config.clone();

    for (width, height) in SIZES {
        let _ = render_at(&app, width, height);
    }
    assert_eq!(app.config, before, "responsive presentation must not change config");

    std::env::remove_var("AMO_CONFIG_DIR");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn continuous_resizing_does_not_break_the_dashboard() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = config_dir();
    std::env::set_var("AMO_CONFIG_DIR", &dir);
    let mut app = App::new();
    app.config.active_layout = "Mixed".to_string();

    // Simulate dragging a terminal window through many intermediate sizes.
    for step in 0..24 {
        let width = 60 + step * 6;
        let height = 18 + step * 2;
        let screen = render_at(&app, width, height);
        assert!(screen.contains('A'), "blank frame during resize step {step}");
    }
    for step in (0..24).rev() {
        let width = 60 + step * 6;
        let height = 18 + step * 2;
        let _ = render_at(&app, width, height);
    }
    std::env::remove_var("AMO_CONFIG_DIR");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn adaptation_preserves_pins_ordering_and_all_widgets() {
    let mut layout = LayoutConfig::new("test", 12, 8);
    let mut pinned = WidgetPlacement::new(WidgetKind::Cpu, 0, 0, 6, 4);
    pinned.pinned = true;
    layout.widgets.push(pinned);
    layout.widgets.push(WidgetPlacement::new(WidgetKind::Gpu, 6, 0, 6, 4));
    layout.widgets.push(WidgetPlacement::new(WidgetKind::Network, 0, 4, 6, 4));

    for class in [SizeClass::Normal, SizeClass::Compact, SizeClass::Minimal] {
        let adapted = responsive::adapt_layout(&layout, class);
        assert_eq!(adapted.widgets.len(), 3);
        assert!(adapted.widgets[0].pinned);
        assert!(amo_monitor::layout::is_collision_free(&adapted));
    }
}
