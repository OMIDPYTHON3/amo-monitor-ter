//! Integration tests for dashboard layouts and persistence.

use std::path::PathBuf;
use std::sync::Mutex;

use amo_monitor::config::{Config, DisplayMode, LayoutConfig, WidgetKind};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn default_layouts_have_widgets_and_are_distinct() {
    let config = Config::default();
    assert!(config.layouts.len() >= 3);
    for layout in &config.layouts {
        assert!(layout.columns > 0 && layout.rows > 0);
        assert!(
            !layout.widgets.is_empty() || !layout.linear_metrics.is_empty(),
            "{} has no content",
            layout.name
        );
    }
    assert_eq!(config.active_layout(), &config.layouts[0]);
}

#[test]
fn config_json_round_trip_preserves_layout_edits() {
    let mut config = Config::default();
    {
        let layout = config.active_layout_mut();
        layout.widgets[0].col = 3;
        layout.widgets[0].row = 2;
        layout.widgets[0].colspan = 5;
        layout.widgets[0].rowspan = 3;
        layout.widgets[0].pinned = true;
        layout.widgets[0].config.display_mode = DisplayMode::Detailed;
        layout.widgets[0].config.show_chart = false;
        layout.widgets.remove(1);
    }
    config.active_layout = "Compact".to_string();
    config.theme = "Dracula".to_string();

    let json = serde_json::to_string_pretty(&config).unwrap();
    let restored: Config = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, config);
    assert_eq!(restored.active_layout, "Compact");
    assert_eq!(restored.theme, "Dracula");
    let layout = restored.active_layout();
    assert_eq!(layout.widgets.len(), 7);
    let overview = restored.layouts.iter().find(|l| l.name == "Overview").unwrap();
    let cpu = &overview.widgets[0];
    assert!(cpu.pinned);
    assert_eq!((cpu.col, cpu.row, cpu.colspan, cpu.rowspan), (3, 2, 5, 3));
    assert_eq!(cpu.config.display_mode, DisplayMode::Detailed);
    assert!(!cpu.config.show_chart);
}

#[test]
fn save_and_load_from_project_config_directory() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("test-config");
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("AMO_CONFIG_DIR", &dir);

    let mut config = Config::default();
    config.theme = "Monokai".into();
    config.refresh_ms = 500;
    config.active_layout_mut().widgets.push(
        amo_monitor::config::WidgetPlacement::new(WidgetKind::Network, 0, 0, 4, 2),
    );
    config.save().unwrap();
    assert!(Config::config_path().exists());

    let loaded = Config::load();
    assert_eq!(loaded.theme, "Monokai");
    assert_eq!(loaded.refresh_ms, 500);
    assert_eq!(loaded.active_layout().widgets.len(), 8);

    let _ = std::fs::remove_dir_all(&dir);
    std::env::remove_var("AMO_CONFIG_DIR");
}

#[test]
fn pinned_position_and_auto_flow_survive_reload() {
    let mut config = Config::default();
    let layout = config.active_layout_mut();
    assert!(layout.widgets[0].pinned, "CPU is pinned by default");
    assert!(layout.widgets[1].pinned, "GPU is pinned by default");
    assert!(!layout.widgets[2].pinned, "RAM flows freely by default");
    let (col, row) = layout.find_free_slot(layout.widgets[2].colspan, layout.widgets[2].rowspan);
    assert_eq!((col, row), (layout.widgets[2].col, layout.widgets[2].row));
}

#[test]
fn layouts_can_be_created_renamed_and_deleted() {
    let mut config = Config::default();
    let original_count = config.layouts.len();
    let mut new_layout = LayoutConfig::new("Test Layout", 6, 4);
    new_layout.widgets = config.active_layout().widgets.clone();
    config.layouts.push(new_layout);
    config.active_layout = "Test Layout".to_string();
    assert_eq!(config.layouts.len(), original_count + 1);
    assert_eq!(config.active_layout().name, "Test Layout");

    config.layouts.retain(|l| l.name != "Test Layout");
    config.active_layout = "Overview".to_string();
    assert_eq!(config.layouts.len(), original_count);
}
