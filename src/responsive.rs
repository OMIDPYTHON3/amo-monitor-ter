//! Practical, stable responsive dashboard behavior.
//!
//! Only three presentation states exist. Layout structure changes only when a
//! breakpoint is crossed; within a state the grid stays put. All decisions are
//! derived per frame and never modify saved user settings.

use ratatui::layout::Rect;

use crate::config::{DisplayMode, LayoutConfig, LayoutKind, LinearMetricKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeClass {
    Normal,
    Compact,
    Minimal,
}

impl SizeClass {
    pub fn label(self) -> &'static str {
        match self {
            SizeClass::Normal => "Normal",
            SizeClass::Compact => "Compact",
            SizeClass::Minimal => "Minimal",
        }
    }
}

/// Two breakpoints only: 80x20 and 120x30. Both dimensions matter, so a wide
/// but very short terminal is still compact/minimal.
pub fn classify(width: u16, height: u16) -> SizeClass {
    if width < 80 || height < 20 {
        SizeClass::Minimal
    } else if width < 120 || height < 30 {
        SizeClass::Compact
    } else {
        SizeClass::Normal
    }
}

pub fn classify_area(area: Rect) -> SizeClass {
    classify(area.width, area.height)
}

/// Minimal terminals use compact text rows regardless of the saved layout.
pub fn effective_kind(configured: LayoutKind, class: SizeClass) -> LayoutKind {
    if class == SizeClass::Minimal {
        LayoutKind::Linear
    } else {
        configured
    }
}

/// Grid structure is intentionally stable: columns and rows do not change
/// between Normal and Compact. Only Minimal changes the presentation style.
pub fn target_columns(configured: u16, _class: SizeClass) -> u16 {
    configured.max(1)
}

pub fn target_rows(configured: u16, _class: SizeClass) -> u16 {
    configured.max(1)
}

fn mode_rank(mode: DisplayMode) -> u8 {
    match mode {
        DisplayMode::Compact => 0,
        DisplayMode::Medium => 1,
        DisplayMode::Detailed => 2,
    }
}

/// Drop optional detail rather than shrinking everything.
pub fn adapt_detail(preferred: DisplayMode, class: SizeClass, widget_area: Rect) -> DisplayMode {
    let class_cap = match class {
        SizeClass::Normal => 2,
        SizeClass::Compact => 1,
        SizeClass::Minimal => 0,
    };
    let area_cap = if widget_area.height < 6 {
        0
    } else if widget_area.height < 10 {
        1
    } else {
        2
    };
    match mode_rank(preferred).min(class_cap).min(area_cap) {
        0 => DisplayMode::Compact,
        1 => DisplayMode::Medium,
        _ => DisplayMode::Detailed,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartPresentation {
    Full,
    Hidden,
}

/// A chart is drawn only when its widget can show it properly. Compact mode
/// asks for more room than Normal; small widgets simply show their numbers.
/// There is no automatic Sparkline substitution.
pub fn adapt_chart(class: SizeClass, widget_area: Rect) -> ChartPresentation {
    if class == SizeClass::Minimal {
        return ChartPresentation::Hidden;
    }
    let (min_width, min_height) = if class == SizeClass::Normal {
        (28, 5)
    } else {
        (44, 8)
    };
    if widget_area.width >= min_width && widget_area.height >= min_height {
        ChartPresentation::Full
    } else {
        ChartPresentation::Hidden
    }
}

/// Axis labels are decoration: only shown in Normal mode when the user enabled
/// them and there is genuine room for the chart.
pub fn show_chart_axes(class: SizeClass, presentation: ChartPresentation) -> bool {
    class == SizeClass::Normal && presentation == ChartPresentation::Full
}

pub fn show_chart_grid(class: SizeClass, presentation: ChartPresentation) -> bool {
    class == SizeClass::Normal && presentation == ChartPresentation::Full
}

/// Only these metrics remain in Minimal mode.
pub fn is_essential(metric: LinearMetricKind) -> bool {
    matches!(
        metric,
        LinearMetricKind::CpuUsage
            | LinearMetricKind::GpuUsage
            | LinearMetricKind::Ram
            | LinearMetricKind::Swap
            | LinearMetricKind::NetRx
            | LinearMetricKind::NetTx
            | LinearMetricKind::Uptime
    )
}

/// Keep the user's grid exactly as saved, but repair any pre-existing overlap.
pub fn adapt_layout(layout: &LayoutConfig, _class: SizeClass) -> LayoutConfig {
    crate::layout::normalize(layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_three_states_with_two_breakpoints() {
        assert_eq!(classify(60, 20), SizeClass::Minimal);
        assert_eq!(classify(79, 40), SizeClass::Minimal);
        assert_eq!(classify(80, 24), SizeClass::Compact);
        assert_eq!(classify(100, 30), SizeClass::Compact);
        assert_eq!(classify(119, 40), SizeClass::Compact);
        assert_eq!(classify(120, 40), SizeClass::Normal);
        assert_eq!(classify(160, 50), SizeClass::Normal);
        assert_eq!(classify(200, 60), SizeClass::Normal);
    }

    #[test]
    fn only_minimal_changes_layout_kind() {
        assert_eq!(effective_kind(LayoutKind::Grid, SizeClass::Minimal), LayoutKind::Linear);
        assert_eq!(effective_kind(LayoutKind::Grid, SizeClass::Compact), LayoutKind::Grid);
        assert_eq!(effective_kind(LayoutKind::Mixed, SizeClass::Normal), LayoutKind::Mixed);
    }

    #[test]
    fn grid_structure_is_stable_between_normal_and_compact() {
        assert_eq!(target_columns(12, SizeClass::Normal), 12);
        assert_eq!(target_columns(12, SizeClass::Compact), 12);
        assert_eq!(target_rows(8, SizeClass::Normal), 8);
        assert_eq!(target_rows(8, SizeClass::Compact), 8);
    }

    #[test]
    fn detail_is_only_dropped_when_warranted() {
        let big = Rect::new(0, 0, 60, 14);
        assert_eq!(adapt_detail(DisplayMode::Detailed, SizeClass::Normal, big), DisplayMode::Detailed);
        assert_eq!(adapt_detail(DisplayMode::Detailed, SizeClass::Compact, big), DisplayMode::Medium);
        assert_eq!(adapt_detail(DisplayMode::Detailed, SizeClass::Minimal, big), DisplayMode::Compact);
        let small = Rect::new(0, 0, 30, 4);
        assert_eq!(adapt_detail(DisplayMode::Detailed, SizeClass::Normal, small), DisplayMode::Compact);
    }

    #[test]
    fn charts_hide_when_widget_is_too_small_and_return_when_it_grows() {
        let tiny = Rect::new(0, 0, 20, 4);
        assert_eq!(adapt_chart(SizeClass::Normal, tiny), ChartPresentation::Hidden);
        assert_eq!(adapt_chart(SizeClass::Compact, Rect::new(0, 0, 50, 9)), ChartPresentation::Full);
        assert_eq!(adapt_chart(SizeClass::Minimal, Rect::new(0, 0, 80, 20)), ChartPresentation::Hidden);
        assert_eq!(adapt_chart(SizeClass::Normal, Rect::new(0, 0, 60, 14)), ChartPresentation::Full);
    }

    #[test]
    fn axes_only_in_normal_mode() {
        assert!(show_chart_axes(SizeClass::Normal, ChartPresentation::Full));
        assert!(!show_chart_axes(SizeClass::Compact, ChartPresentation::Full));
        assert!(!show_chart_axes(SizeClass::Normal, ChartPresentation::Hidden));
    }

    #[test]
    fn minimal_keeps_only_essential_metrics() {
        assert!(is_essential(LinearMetricKind::CpuUsage));
        assert!(is_essential(LinearMetricKind::Uptime));
        assert!(!is_essential(LinearMetricKind::CpuFreq));
        assert!(!is_essential(LinearMetricKind::DiskRead));
    }

    #[test]
    fn adaptation_preserves_layout_structure() {
        let mut layout = LayoutConfig::new("test", 12, 8);
        layout.widgets.push(crate::config::WidgetPlacement::new(
            crate::config::WidgetKind::Cpu,
            0,
            0,
            6,
            4,
        ));
        for class in [SizeClass::Normal, SizeClass::Compact, SizeClass::Minimal] {
            let adapted = adapt_layout(&layout, class);
            assert_eq!(adapted.columns, layout.columns);
            assert_eq!(adapted.rows, layout.rows);
            assert_eq!(adapted.widgets, layout.widgets);
            assert!(crate::layout::is_collision_free(&adapted));
        }
    }
}
