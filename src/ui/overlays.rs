//! Modal overlays: help, settings, menus and text input.

use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, InputPurpose, Mode};
use crate::config::theme::ResolvedTheme;
use crate::config::ViewMode;

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width.saturating_sub(2)).max(10);
    let h = height.min(area.height.saturating_sub(2)).max(5);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

fn popup(frame: &mut Frame, app: &App, title: &str, mut body: Vec<Line<'static>>, hint: &str) {
    let theme = &app.resolved;
    let area = centered(frame.area(), 78, (body.len() + 4).min(30) as u16);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "))
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default().bg(theme.panel).fg(theme.text));
    frame.render_widget(&block, area);
    let inner = area.inner(Margin::new(1, 1));
    if inner.height == 0 {
        return;
    }
    body.push(Line::from(Span::styled(
        hint.to_string(),
        Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
    )));
    body.truncate(inner.height as usize);
    frame.render_widget(Paragraph::new(body), inner);
}

fn item(text: String, selected: bool, theme: &ResolvedTheme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            if selected { "▶ " } else { "  " }.to_string(),
            Style::default().fg(theme.accent),
        ),
        Span::styled(
            text,
            Style::default()
                .fg(if selected { theme.accent } else { theme.text })
                .add_modifier(if selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
    ])
}

fn text_line(label: &str, value: String, theme: &ResolvedTheme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<18}"), Style::default().fg(theme.muted)),
        Span::styled(value, Style::default().fg(theme.accent_alt)),
    ])
}

pub fn render_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let _ = area;
    match &app.mode {
        Mode::Normal | Mode::Move | Mode::Resize => {}
        Mode::Help => render_help(frame, app),
        Mode::Settings { tab, index } => render_settings(frame, app, *tab, *index),
        Mode::WidgetConfig { widget, option } => render_widget_config(frame, app, *widget, *option),
        Mode::LayoutManager { index } => render_layouts(frame, app, *index),
        Mode::ThemeMenu { index } => render_themes(frame, app, *index),
        Mode::AddWidget { index } => render_add_widget(frame, app, *index),
        Mode::LinearMetrics { index } => render_linear_metrics(frame, app, *index),
        Mode::TextInput { purpose, value } => render_text_input(frame, app, purpose, value),
    }
}

fn render_help(frame: &mut Frame, app: &App) {
    let body = vec![
        Line::from(Span::styled(
            "AMO MONITOR — keyboard reference",
            Style::default().fg(app.resolved.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  Tab / ← ↑ ↓ →   Select widget (visual order)"),
        Line::from("  Space            Pin / unpin selected widget"),
        Line::from("  m / r            Move / resize mode (arrows adjust, Esc done)"),
        Line::from("  Enter            Confirm move/resize preview (Esc cancels)"),
        Line::from("  Ctrl-Z           Undo last layout change"),
        Line::from("  v                Cycle Grid / Linear / Mixed dashboard"),
        Line::from("  y                Edit linear metrics (pin, reorder, group)"),
        Line::from("  c                Configure selected widget"),
        Line::from("  a                Add a widget"),
        Line::from("  d / Delete       Remove selected widget"),
        Line::from("  s                Settings (general, widgets, layouts)"),
        Line::from("  t                Switch theme"),
        Line::from("  l                Layout manager (create/rename/delete)"),
        Line::from("  1..9             Quick-switch layout"),
        Line::from("  F5               Save configuration"),
        Line::from("  q / Ctrl-C       Quit (layouts and settings are saved)"),
        Line::from(""),
        Line::from(Span::styled(
            "Pinned widgets keep their grid cell. Unpinned widgets flow into the free space.",
            Style::default().fg(app.resolved.muted),
        )),
    ];
    popup(frame, app, "HELP", body, "Esc / ? to close");
}

fn render_settings(frame: &mut Frame, app: &App, tab: usize, index: usize) {
    let theme = &app.resolved;
    let mut body = vec![Line::from(vec![
        tab_item("General", tab == 0, theme),
        Span::raw("  "),
        tab_item("Widgets", tab == 1, theme),
        Span::raw("  "),
        tab_item("Layouts", tab == 2, theme),
        Span::raw("  "),
        tab_item("Colors", tab == 3, theme),
    ])];
    body.push(Line::from(""));
    match tab {
        0 => {
            body.push(item(format!("Theme                    {}", app.config.theme), index == 0, theme));
            body.push(item(format!("Refresh interval         {} ms", app.config.refresh_ms), index == 1, theme));
            let accent = app.config.color_overrides.get("accent").cloned().unwrap_or_else(|| "<theme default>".into());
            body.push(item(format!("Custom accent (e)        {accent}"), index == 2, theme));
            let accent_alt = app.config.color_overrides.get("accent_alt").cloned().unwrap_or_else(|| "<theme default>".into());
            body.push(item(format!("Secondary accent (e)     {accent_alt}"), index == 3, theme));
            body.push(Line::from(""));
            body.push(text_line("Config file", crate::config::Config::config_path().display().to_string(), theme));
        }
        1 => {
            if app.active_layout().widgets.is_empty() {
                body.push(Line::from("No widgets in this layout. Press 'a' to add one."));
            }
            for (i, w) in app.active_layout().widgets.iter().enumerate() {
                let pin = if w.pinned { "[P]" } else { "   " };
                body.push(item(
                    format!("{pin} {:<9} {:>9}", w.kind.title(), w.config.display_mode.label()),
                    i == index,
                    theme,
                ));
            }
        }
        2 => {
            for (i, layout) in app.config.layouts.iter().enumerate() {
                let active = if layout.name == app.config.active_layout { "*" } else { " " };
                body.push(item(
                    format!("{active} {:<14} {:<7} {} widgets", layout.name, layout.kind.short(), layout.widgets.len()),
                    i == index,
                    theme,
                ));
            }
        }
        _ => {
            use crate::colors::{resolve_color, Metric as ColorMetric};
            let colors = &app.config.dynamic_colors;
            let check = |on: bool| if on { "[x]" } else { "[ ]" };
            body.push(item(format!("{} Global dynamic colours", check(colors.enabled)), index == 0, theme));
            body.push(item(format!("<> Scheme: {}", colors.scheme), index == 1, theme));
            let metric = ColorMetric::ALL[app.settings_metric.min(ColorMetric::ALL.len() - 1)];
            let cfg = colors.metric(metric);
            body.push(item(format!("<> Metric: {} ({})", metric.label(), metric.group()), index == 2, theme));
            body.push(item(format!("{} Metric colours", check(cfg.enabled)), index == 3, theme));
            for level in 0..3 {
                body.push(item(
                    format!("<> Tier {} threshold (e)   {}", level + 1, cfg.thresholds[level]),
                    index == 4 + level,
                    theme,
                ));
            }
            let preview = crate::util::fmt_opt(metric.value(&app.monitor.snapshot), 1, metric.unit());
            for level in 0..4 {
                let swatch = resolve_color(&cfg.colors[level], theme);
                let mut line = item(format!("<> Tier {} colour (e)     {}", level + 1, cfg.colors[level]), index == 7 + level, theme);
                line.spans.push(Span::styled(
                    "  ██",
                    Style::default().fg(swatch).add_modifier(Modifier::BOLD),
                ));
                body.push(line);
            }
            body.push(item("⏎ Reset all colours to defaults".to_string(), index == 11, theme));
            body.push(Line::from(Span::styled(
                format!("Live preview: {preview}"),
                Style::default().fg(colors.color_for(metric, metric.value(&app.monitor.snapshot), theme)),
            )));
        }
    }
    let hint = match tab {
        0 => "←→ change · e edit accent · Tab section · Esc close",
        1 => "Space pin · c configure · a add · d delete · Tab section · Esc close",
        2 => "Enter activate · v dashboard type · n new · r rename · d delete · Esc close",
        _ => "←→ change · e edit value · Enter reset · Tab section · Esc close",
    };
    popup(frame, app, "SETTINGS", body, hint);
}

fn tab_item(label: &str, active: bool, theme: &ResolvedTheme) -> Span<'static> {
    Span::styled(
        format!(" [ {label} ] "),
        Style::default()
            .fg(if active { theme.background } else { theme.muted })
            .bg(if active { theme.accent } else { theme.panel_alt }),
    )
}

fn render_widget_config(frame: &mut Frame, app: &App, widget: usize, option: usize) {
    let theme = &app.resolved;
    let Some(placement) = app.active_layout().widgets.get(widget) else {
        return;
    };
    let cfg = &placement.config;
    let check = |on: bool| if on { "[x]" } else { "[ ]" };
    let rows: Vec<String> = vec![
        format!("{} Show primary value / usage", check(cfg.show_primary)),
        format!("{} Show frequency", check(cfg.show_frequency)),
        format!("{} Show power (W)", check(cfg.show_power)),
        format!("{} Show temperature", check(cfg.show_temperature)),
        format!("{} Show extra details", check(cfg.show_extra)),
        format!("{} Show line chart", check(cfg.show_chart)),
        format!("<> Display mode: {}", cfg.display_mode.label()),
        format!("<> View mode: {}", cfg.view_mode.label()),
        format!("<> Chart type:  {}", cfg.chart_type.label()),
        format!("<> History:     {} s", cfg.history_window_secs),
        format!("{} Grid lines", check(cfg.show_grid)),
        format!("{} Axes & labels", check(cfg.show_axes)),
        format!("<> Y scaling:   {}", cfg.y_scale.label()),
        format!("<> Manual max:  {}", cfg.manual_max),
        format!("<> Line width:  {}", cfg.line_thickness),
    ];
    let mut body = vec![Line::from(Span::styled(
        format!("CONFIGURE — {}", placement.kind.title()),
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    ))];
    body.push(Line::from(""));
    for (i, row) in rows.iter().enumerate() {
        body.push(item(row.clone(), i == option, theme));
    }
    body.push(Line::from(""));
    body.push(Line::from(Span::styled(
        if cfg.view_mode == ViewMode::ChartsOnly {
            "Charts only: numeric fields are hidden."
        } else if cfg.view_mode == ViewMode::NumbersOnly {
            "Numbers only: the chart is hidden."
        } else {
            "Numbers and charts are shown together."
        },
        Style::default().fg(theme.muted),
    )));
    popup(frame, app, "WIDGET", body, "↑↓ select · ←→/Space change · Esc done");
}

fn render_layouts(frame: &mut Frame, app: &App, index: usize) {
    let theme = &app.resolved;
    let mut body = vec![Line::from(Span::styled(
        "DASHBOARD LAYOUTS",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    ))];
    body.push(Line::from(""));
    for (i, layout) in app.config.layouts.iter().enumerate() {
        let active = if layout.name == app.config.active_layout { "●" } else { " " };
        body.push(item(
            format!("{active} {:<18} {}x{} grid", layout.name, layout.columns, layout.rows),
            i == index,
            theme,
        ));
    }
    popup(frame, app, "LAYOUTS", body, "Enter activate · n new · r rename · d delete · Esc close");
}

fn render_themes(frame: &mut Frame, app: &App, index: usize) {
    let theme = &app.resolved;
    let names = app.themes.names();
    let mut body = vec![Line::from(Span::styled(
        "COLOR THEMES",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    ))];
    body.push(Line::from(""));
    for (i, name) in names.iter().enumerate() {
        // Preview each theme using its own palette, not the active one.
        let preview = app.themes.resolve(name, &Default::default());
        let mut line = if i == index {
            item(String::new(), true, theme)
        } else {
            item(String::new(), false, theme)
        };
        for (color, glyph) in [
            (preview.accent, "▓▓"),
            (preview.cpu, "▓▓"),
            (preview.gpu, "▓▓"),
            (preview.memory, "▓▓"),
            (preview.success, "▓▓"),
            (preview.danger, "▓▓"),
        ] {
            line.spans.push(Span::styled(
                glyph,
                Style::default().fg(color).bg(preview.panel),
            ));
        }
        line.spans.push(Span::raw(" "));
        line.spans.push(Span::styled(
            format!("{name}"),
            Style::default().fg(if i == index { theme.accent } else { theme.text }),
        ));
        body.push(line);
    }
    body.push(Line::from(""));
    body.push(Line::from(Span::styled(
        "Colours: edit config/themes/<name>.toml, themes.json overrides, or Settings → accent (e).",
        Style::default().fg(theme.muted),
    )));
    popup(frame, app, "THEMES", body, "↑↓ select · Enter apply · Esc close");
}

fn render_add_widget(frame: &mut Frame, app: &App, index: usize) {
    let theme = &app.resolved;
    let mut body = vec![Line::from(Span::styled(
        "ADD WIDGET",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    ))];
    body.push(Line::from(""));
    for (i, kind) in crate::config::WidgetKind::ALL.iter().enumerate() {
        body.push(item(kind.title().to_string(), i == index, theme));
    }
    popup(frame, app, "ADD", body, "↑↓ select · Enter add · Esc cancel");
}

fn render_linear_metrics(frame: &mut Frame, app: &App, index: usize) {
    let theme = &app.resolved;
    let layout = app.active_layout();
    let mut body = vec![
        Line::from(Span::styled(
            format!("LINEAR METRICS — {}", layout.kind.label()),
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("Display: {}   · pinned metrics stay ahead in their group", layout.linear_display.label()),
            Style::default().fg(theme.muted),
        )),
        Line::from(""),
    ];
    if layout.linear_metrics.is_empty() {
        body.push(Line::from("No metrics — press 'a' to add one."));
    }
    for (i, metric) in layout.linear_metrics.iter().enumerate() {
        let pin = if metric.pinned { "[P]" } else { "   " };
        let chart = if metric.show_chart { "▤" } else { " " };
        let value = metric.kind.display(&app.monitor.snapshot);
        body.push(item(
            format!(
                "{pin} {chart} {:<11} grp {}  {:<14}",
                metric.kind.label(), metric.group, value
            ),
            i == index,
            theme,
        ));
    }
    popup(
        frame,
        app,
        "METRICS",
        body,
        "←→ reorder · Space pin · c chart · g group · a add · d delete · v view · Esc done",
    );
}

fn render_text_input(frame: &mut Frame, app: &App, purpose: &InputPurpose, value: &str) {
    let theme = &app.resolved;
    let prompt = match purpose {
        InputPurpose::NewLayout => "Layout name",
        InputPurpose::RenameLayout => "Rename layout to",
        InputPurpose::Accent => "Accent colour (#RRGGBB)",
        InputPurpose::AccentAlt => "Secondary accent (#RRGGBB)",
        InputPurpose::Threshold(_) => "Threshold value",
        InputPurpose::ColorValue(_) => "Colour (#RRGGBB or theme token)",
    };
    let body = vec![
        Line::from(Span::styled(prompt, Style::default().fg(theme.accent))),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{value}█"),
                Style::default().fg(theme.text),
            ),
        ]),
    ];
    popup(frame, app, "INPUT", body, "Enter confirm · Esc cancel");
}
