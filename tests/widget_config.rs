//! Integration tests for per-widget configuration, including chart styles.

use amo_monitor::config::{
    ChartType, DisplayMode, ViewMode, WidgetConfig, WidgetKind, WidgetPlacement, YScale,
    HISTORY_WINDOWS,
};

fn full_config() -> WidgetConfig {
    WidgetConfig {
        show_primary: true,
        show_frequency: true,
        show_power: true,
        show_temperature: true,
        show_extra: true,
        show_chart: true,
        display_mode: DisplayMode::Medium,
        view_mode: ViewMode::NumbersAndCharts,
        chart_type: ChartType::BrailleLine,
        history_window_secs: 30,
        show_grid: true,
        show_axes: false,
        y_scale: YScale::Auto,
        manual_max: 100.0,
        line_thickness: 1,
    }
}

#[test]
fn every_widget_kind_is_configurable() {
    assert_eq!(WidgetKind::ALL.len(), 7);
    let titles: Vec<&str> = WidgetKind::ALL.iter().map(|k| k.title()).collect();
    assert_eq!(
        titles,
        vec!["CPU", "GPU", "RAM", "SWAP", "NETWORK", "DISK I/O", "SYSTEM"]
    );
    for kind in WidgetKind::ALL {
        let placement = WidgetPlacement::new(kind, 0, 0, 4, 3);
        assert!(!placement.id.is_empty());
        assert!(!placement.pinned);
        assert_eq!(placement.config, WidgetConfig::default());
    }
}

#[test]
fn all_fields_can_be_toggled_independently() {
    let base = full_config();
    assert!(base.show_primary);
    assert!(base.show_chart);
    assert_eq!(base.display_mode, DisplayMode::Medium);
    assert_eq!(base.view_mode, ViewMode::NumbersAndCharts);

    let customized = WidgetConfig {
        show_primary: true,
        show_frequency: false,
        show_power: false,
        show_temperature: false,
        show_extra: false,
        show_chart: false,
        display_mode: DisplayMode::Compact,
        view_mode: ViewMode::ChartsOnly,
        chart_type: ChartType::Histogram,
        history_window_secs: 300,
        show_grid: false,
        show_axes: true,
        y_scale: YScale::Manual,
        manual_max: 250.0,
        line_thickness: 3,
    };
    assert_ne!(customized, base);
    // Toggling one field does not affect the others.
    let one_change = WidgetConfig {
        show_temperature: false,
        ..base.clone()
    };
    assert_eq!(one_change.show_chart, base.show_chart);
    assert_eq!(one_change.chart_type, base.chart_type);
    assert_eq!(one_change.history_window_secs, base.history_window_secs);
}

#[test]
fn display_view_and_chart_modes_cycle() {
    assert_eq!(DisplayMode::ALL.len(), 3);
    assert_eq!(ViewMode::ALL.len(), 3);
    assert_eq!(ChartType::ALL.len(), 8);
    assert_eq!(YScale::ALL.len(), 2);
    for i in 0..ChartType::ALL.len() {
        let style = ChartType::ALL[i];
        assert!(!style.label().is_empty());
        assert_ne!(style, ChartType::ALL[(i + 1) % ChartType::ALL.len()]);
    }
    assert_eq!(HISTORY_WINDOWS, [30, 60, 300]);
}

#[test]
fn chart_preferences_are_independent_per_widget() {
    let mut a = WidgetPlacement::new(WidgetKind::Cpu, 0, 0, 4, 3);
    let mut b = WidgetPlacement::new(WidgetKind::Network, 4, 0, 4, 3);
    a.config.chart_type = ChartType::Area;
    a.config.history_window_secs = 60;
    b.config.chart_type = ChartType::MultiLine;
    b.config.history_window_secs = 300;
    b.config.show_axes = true;
    assert_eq!(a.config.chart_type, ChartType::Area);
    assert_eq!(b.config.chart_type, ChartType::MultiLine);
    assert_ne!(a.config, b.config);
}

#[test]
fn widget_config_survives_json_round_trip() {
    let cfg = WidgetConfig {
        show_primary: false,
        show_frequency: true,
        show_power: false,
        show_temperature: true,
        show_extra: false,
        show_chart: true,
        display_mode: DisplayMode::Detailed,
        view_mode: ViewMode::NumbersOnly,
        chart_type: ChartType::GradientArea,
        history_window_secs: 60,
        show_grid: false,
        show_axes: true,
        y_scale: YScale::Manual,
        manual_max: 500.0,
        line_thickness: 2,
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let decoded: WidgetConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, cfg);
}

#[test]
fn older_config_files_gain_chart_defaults() {
    let legacy = r#"{
        "show_primary": true,
        "show_frequency": true,
        "show_power": true,
        "show_temperature": true,
        "show_extra": true,
        "show_chart": true,
        "display_mode": "Medium",
        "view_mode": "NumbersAndCharts"
    }"#;
    let cfg: WidgetConfig = serde_json::from_str(legacy).unwrap();
    assert_eq!(cfg.chart_type, ChartType::BrailleLine);
    assert_eq!(cfg.history_window_secs, 30);
    assert!(!cfg.show_grid);
    assert!(!cfg.show_axes);
    assert_eq!(cfg.y_scale, YScale::Auto);
    assert_eq!(cfg.line_thickness, 1);
}
