//! Dynamic text-colour verification for every metric and theme.

use std::collections::BTreeMap;

use amo_monitor::colors::{
    resolve_color, DynamicColorConfig, Metric, MetricColorConfig, SCHEMES,
};
use amo_monitor::config::theme::ThemeSet;
use amo_monitor::config::Config;
use ratatui::style::Color;

fn theme(name: &str) -> amo_monitor::config::theme::ResolvedTheme {
    ThemeSet::builtin().resolve(name, &BTreeMap::new())
}

#[test]
fn default_percentage_thresholds_are_exact() {
    let cfg = DynamicColorConfig::default();
    let t = theme("Dark");
    for metric in [Metric::CpuUsage, Metric::GpuUsage, Metric::Ram, Metric::Swap] {
        assert_eq!(metric.default_thresholds(), [60.0, 75.0, 90.0]);
        let at = |v: f64| cfg.color_for(metric, Some(v), &t);
        assert_eq!(at(0.0), resolve_color("foreground", &t));
        assert_eq!(at(59.0), resolve_color("foreground", &t));
        assert_eq!(at(60.0), resolve_color("warning", &t));
        assert_eq!(at(74.0), resolve_color("warning", &t));
        assert_eq!(at(75.0), resolve_color("orange", &t));
        assert_eq!(at(89.0), resolve_color("orange", &t));
        assert_eq!(at(90.0), resolve_color("danger", &t));
        assert_eq!(at(100.0), resolve_color("danger", &t));
    }
}

#[test]
fn every_metric_has_its_own_unit_appropriate_thresholds() {
    assert_eq!(Metric::ALL.len(), 14);
    for metric in Metric::ALL {
        let thresholds = metric.default_thresholds();
        assert!(thresholds[0] < thresholds[1] && thresholds[1] < thresholds[2]);
        assert!(!metric.unit().is_empty());
        assert!(!metric.label().is_empty());
        assert!(Config::default().dynamic_colors.metric(metric).enabled);
    }
    // Non-percentage metrics must not use the percentage scale.
    for metric in [
        Metric::CpuPower,
        Metric::GpuPower,
        Metric::CpuTemp,
        Metric::GpuTemp,
        Metric::CpuFreq,
        Metric::GpuFreq,
        Metric::NetRx,
        Metric::NetTx,
        Metric::DiskRead,
        Metric::DiskWrite,
    ] {
        assert_ne!(metric.default_thresholds(), [60.0, 75.0, 90.0], "{}", metric.label());
    }
}

#[test]
fn global_and_per_metric_toggles_are_independent() {
    let t = theme("Nord");
    let mut cfg = DynamicColorConfig::default();
    let hot = cfg.color_for(Metric::CpuUsage, Some(99.0), &t);
    assert_ne!(hot, t.text);

    let mut cpu = cfg.metric(Metric::CpuUsage);
    cpu.enabled = false;
    cfg.set_metric(Metric::CpuUsage, cpu);
    assert_eq!(cfg.color_for(Metric::CpuUsage, Some(99.0), &t), t.text);
    assert_ne!(cfg.color_for(Metric::Ram, Some(99.0), &t), t.text);

    cfg.enabled = false;
    for metric in Metric::ALL {
        assert_eq!(cfg.color_for(metric, Some(9999.0), &t), t.text);
    }
}

#[test]
fn custom_thresholds_and_colours_are_honoured() {
    let t = theme("Dracula");
    let mut cfg = DynamicColorConfig::default();
    let mut net = cfg.metric(Metric::NetRx);
    net.thresholds = [100.0, 200.0, 300.0];
    net.colors = ["#010203".into(), "#040506".into(), "#070809".into(), "#0A0B0C".into()];
    cfg.set_metric(Metric::NetRx, net);
    assert_eq!(cfg.color_for(Metric::NetRx, Some(50.0), &t), Color::Rgb(1, 2, 3));
    assert_eq!(cfg.color_for(Metric::NetRx, Some(150.0), &t), Color::Rgb(4, 5, 6));
    assert_eq!(cfg.color_for(Metric::NetRx, Some(250.0), &t), Color::Rgb(7, 8, 9));
    assert_eq!(cfg.color_for(Metric::NetRx, Some(500.0), &t), Color::Rgb(10, 11, 12));
}

#[test]
fn thresholds_are_normalized_when_misordered() {
    let mut cfg = MetricColorConfig::for_metric(Metric::CpuTemp);
    cfg.thresholds = [80.0, 10.0, 30.0];
    let normalized = cfg.normalized();
    assert!(normalized.thresholds[0] < normalized.thresholds[1]);
    assert!(normalized.thresholds[1] < normalized.thresholds[2]);
}

#[test]
fn predefined_schemes_and_reset_work() {
    assert!(SCHEMES.len() >= 4);
    let mut cfg = DynamicColorConfig::default();
    cfg.apply_scheme("sunset");
    assert_eq!(cfg.scheme, "sunset");
    assert_eq!(cfg.metric(Metric::GpuUsage).colors[1], "#FFD166");
    cfg.reset();
    assert_eq!(cfg, DynamicColorConfig::default());
    assert_eq!(cfg.scheme, "classic");
}

#[test]
fn theme_tokens_follow_the_selected_theme() {
    let dark = theme("Dark");
    let light = theme("Light");
    let cfg = DynamicColorConfig::default();
    let dark_hot = cfg.color_for(Metric::CpuUsage, Some(95.0), &dark);
    let light_hot = cfg.color_for(Metric::CpuUsage, Some(95.0), &light);
    assert_ne!(dark_hot, light_hot);
    assert_eq!(dark_hot, dark.danger);
    assert_eq!(light_hot, light.danger);
}

#[test]
fn missing_values_never_get_a_fabricated_colour() {
    let t = theme("Everforest");
    let cfg = DynamicColorConfig::default();
    for metric in Metric::ALL {
        assert_eq!(cfg.color_for(metric, None, &t), t.text);
        assert_eq!(cfg.color_for(metric, Some(f64::NAN), &t), t.text);
    }
}

#[test]
fn colour_preferences_persist_in_config_json() {
    let mut config = Config::default();
    let mut cfg = config.dynamic_colors.clone();
    cfg.enabled = false;
    let mut one = cfg.metric(Metric::DiskWrite);
    one.thresholds = [11.0, 22.0, 33.0];
    one.colors[3] = "#ABCDEF".into();
    cfg.set_metric(Metric::DiskWrite, one);
    config.dynamic_colors = cfg.clone();

    let json = serde_json::to_string(&config).unwrap();
    let restored: Config = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.dynamic_colors, cfg);
    assert!(!restored.dynamic_colors.enabled);
    assert_eq!(restored.dynamic_colors.metric(Metric::DiskWrite).thresholds, [11.0, 22.0, 33.0]);
    assert_eq!(restored.dynamic_colors.metric(Metric::DiskWrite).colors[3], "#ABCDEF");
}

#[test]
fn legacy_config_without_dynamic_colours_loads_defaults() {
    let legacy = r##"{
        "version": 1,
        "theme": "Dark",
        "color_overrides": {},
        "refresh_ms": 1000,
        "chart_window_secs": 30,
        "layouts": [],
        "active_layout": "x"
    }"##;
    let config: Config = serde_json::from_str(legacy).unwrap();
    assert!(config.dynamic_colors.enabled);
    assert_eq!(config.dynamic_colors.metric(Metric::CpuUsage).thresholds, [60.0, 75.0, 90.0]);
}
