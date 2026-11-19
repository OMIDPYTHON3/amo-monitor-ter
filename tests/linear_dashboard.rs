//! Linear / Mixed dashboard verification at different terminal sizes.

use std::path::PathBuf;
use std::sync::Mutex;

use amo_monitor::app::App;
use amo_monitor::config::{
    Config, LayoutKind, LayoutConfig, LinearMetric, LinearMetricKind,
};
use amo_monitor::ui;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn config_dir(tag: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("amo-linear-{tag}-{}", std::process::id()))
}

#[test]
fn default_config_provides_all_three_dashboard_types() {
    let config = Config::default();
    let kinds: Vec<LayoutKind> = config.layouts.iter().map(|l| l.kind).collect();
    assert!(kinds.contains(&LayoutKind::Grid));
    assert!(kinds.contains(&LayoutKind::Linear));
    assert!(kinds.contains(&LayoutKind::Mixed));
    let linear = config.layouts.iter().find(|l| l.kind == LayoutKind::Linear).unwrap();
    assert_eq!(linear.linear_metrics.len(), LinearMetricKind::ALL.len());
    let mixed = config.layouts.iter().find(|l| l.kind == LayoutKind::Mixed).unwrap();
    assert!(!mixed.widgets.is_empty());
    assert!(!mixed.linear_metrics.is_empty());
}

#[test]
fn linear_metric_settings_are_independent_and_persist() {
    let mut layout = LayoutConfig::new("custom", 12, 4);
    layout.kind = LayoutKind::Linear;
    layout.linear_display = amo_monitor::config::DisplayMode::Detailed;
    layout.linear_metrics.push(LinearMetric::new(LinearMetricKind::CpuUsage, 0, 0));
    let mut second = LinearMetric::new(LinearMetricKind::NetRx, 3, 1);
    second.pinned = true;
    second.show_chart = true;
    layout.linear_metrics.push(second);

    let json = serde_json::to_string(&layout).unwrap();
    let restored: LayoutConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.kind, LayoutKind::Linear);
    assert_eq!(restored.linear_display, amo_monitor::config::DisplayMode::Detailed);
    assert!(!restored.linear_metrics[0].pinned);
    assert!(restored.linear_metrics[1].pinned);
    assert!(restored.linear_metrics[1].show_chart);
    assert_eq!(restored.linear_metrics[1].group, 3);
}

#[test]
fn legacy_layouts_default_to_grid() {
    let legacy = r#"{"name":"old","columns":12,"rows":8,"widgets":[]}"#;
    let layout: LayoutConfig = serde_json::from_str(legacy).unwrap();
    assert_eq!(layout.kind, LayoutKind::Grid);
    assert!(layout.linear_metrics.is_empty());
    assert_eq!(layout.linear_display, amo_monitor::config::DisplayMode::Compact);
}

#[test]
fn all_dashboard_types_render_at_many_terminal_sizes() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = config_dir("render");
    std::env::set_var("AMO_CONFIG_DIR", &dir);
    let mut app = App::new();
    for kind in [LayoutKind::Grid, LayoutKind::Linear, LayoutKind::Mixed] {
        let name = app
            .config
            .layouts
            .iter()
            .find(|l| l.kind == kind)
            .map(|l| l.name.clone())
            .expect("layout kind exists");
        app.config.active_layout = name;
        for (w, h) in [(150, 48), (110, 32), (80, 24), (60, 16)] {
            let backend = TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| ui::render(frame, &app))
                .unwrap_or_else(|e| panic!("{kind:?} failed at {w}x{h}: {e}"));
            let buffer = terminal.backend().buffer();
            let symbols: String = buffer.content().iter().map(|c| c.symbol().chars().next().unwrap_or(' ')).collect();
            assert!(symbols.contains('A'), "{kind:?} did not render at {w}x{h}");
        }
    }
    std::env::remove_var("AMO_CONFIG_DIR");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn switching_dashboard_kind_populates_missing_content() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = config_dir("switch");
    std::env::set_var("AMO_CONFIG_DIR", &dir);
    let mut app = App::new();
    let layout = app.active_layout_mut();
    layout.kind = LayoutKind::Linear;
    assert!(layout.linear_metrics.is_empty());
    app.ensure_layout_content();
    assert!(!app.active_layout().linear_metrics.is_empty());

    let layout = app.active_layout_mut();
    layout.kind = LayoutKind::Mixed;
    layout.widgets.clear();
    app.ensure_layout_content();
    assert!(!app.active_layout().widgets.is_empty());
    assert!(!app.active_layout().linear_metrics.is_empty());
    std::env::remove_var("AMO_CONFIG_DIR");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn layout_selection_survives_restart() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = config_dir("persist");
    std::env::set_var("AMO_CONFIG_DIR", &dir);
    let mut config = Config::default();
    config.active_layout = "Linear".to_string();
    config.save().unwrap();
    let loaded = Config::load();
    assert_eq!(loaded.active_layout, "Linear");
    assert_eq!(loaded.active_layout().kind, LayoutKind::Linear);
    std::env::remove_var("AMO_CONFIG_DIR");
    let _ = std::fs::remove_dir_all(&dir);
}
