//! Terminal user interface: quiet header, dashboard and compact status bar.

pub mod dashboard;
pub mod linear;
pub mod overlays;
pub mod widgets;

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::app::{App, Mode};
use crate::config::theme::ResolvedTheme;
use crate::responsive::{self, SizeClass};
use crate::util::{fmt_uptime, fmt_opt};

pub fn render(frame: &mut Frame, app: &App) {
    let theme = &app.resolved;
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background).fg(theme.text)),
        frame.area(),
    );

    let class = responsive::classify_area(frame.area());
    let (header_h, footer_h) = match class {
        SizeClass::Normal => (2, 2),
        SizeClass::Compact | SizeClass::Minimal => (1, 1),
    };
    let chunks = Layout::vertical([
        Constraint::Length(header_h),
        Constraint::Min(1),
        Constraint::Length(footer_h),
    ])
    .split(frame.area());

    render_header(frame, chunks[0], app, class);
    dashboard::render_dashboard(frame, chunks[1], app, class);
    render_footer(frame, chunks[2], app, class);
    overlays::render_overlay(frame, frame.area(), app);
}

fn render_header(frame: &mut Frame, area: ratatui::layout::Rect, app: &App, class: SizeClass) {
    let theme = &app.resolved;
    let snap = &app.monitor.snapshot;

    if class == SizeClass::Minimal {
        let line = Line::from(vec![
            Span::styled("amo", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled("  ", Style::default()),
            Span::styled(
                fmt_opt(snap.cpu.usage_percent, 0, "%"),
                Style::default().fg(theme.cpu).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ·  ", Style::default().fg(theme.border)),
            Span::styled(fmt_uptime(snap.system.uptime_secs), Style::default().fg(theme.muted)),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    let left = format!(
        "{} · {}",
        snap.system.hostname,
        snap.system.kernel.split('-').next().unwrap_or("Linux")
    );
    let right = format!(
        "{}  ·  {}",
        fmt_uptime(snap.system.uptime_secs),
        theme.name
    );
    let used = 17 + left.chars().count() + right.chars().count();
    let gap = " ".repeat((area.width as usize).saturating_sub(used).max(1));

    let line = Line::from(vec![
        Span::styled("amo ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled("monitor", Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::styled("   ", Style::default()),
        Span::styled(left, Style::default().fg(theme.muted)),
        Span::raw(gap),
        Span::styled("up ", Style::default().fg(theme.muted)),
        Span::styled(right, Style::default().fg(theme.accent_alt)),
    ]);

    let mut lines = vec![line];
    if class == SizeClass::Normal && area.height >= 2 {
        lines.push(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(theme.border),
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_footer(frame: &mut Frame, area: ratatui::layout::Rect, app: &App, class: SizeClass) {
    let theme: &ResolvedTheme = &app.resolved;
    let hints: Vec<(&str, &str)> = match app.mode {
        Mode::Move | Mode::Resize => {
            vec![("←→↑↓", "adjust"), ("Enter", "apply"), ("Esc", "cancel")]
        }
        Mode::WidgetConfig { .. } => vec![("↑↓", "option"), ("←→", "change"), ("Esc", "done")],
        Mode::Settings { .. } => vec![("Tab", "section"), ("↑↓", "select"), ("Esc", "done")],
        _ if class != SizeClass::Normal => {
            vec![("Tab", "select"), ("Space", "pin"), ("v", "view"), ("s", "set"), ("q", "quit")]
        }
        _ => vec![
            ("Tab", "select"),
            ("Space", "pin"),
            ("m", "move"),
            ("r", "resize"),
            ("v", "view"),
            ("y", "metrics"),
            ("c", "config"),
            ("s", "settings"),
            ("t", "theme"),
            ("?", "help"),
            ("q", "quit"),
        ],
    };

    let mut spans: Vec<Span> = Vec::new();
    for (i, (key, action)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(theme.border)));
        }
        spans.push(Span::styled(
            *key,
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!(" {action}"), Style::default().fg(theme.muted)));
    }

    let mut lines = vec![Line::from(spans)];
    if class == SizeClass::Normal && area.height >= 2 {
        if let Some((message, at)) = &app.status {
            if at.elapsed().as_secs() < 4 {
                lines.push(Line::from(Span::styled(
                    format!("  • {message}"),
                    Style::default().fg(theme.success),
                )));
            }
        }
    }
    let paragraph = Paragraph::new(lines).style(Style::default().bg(theme.background));
    frame.render_widget(paragraph, area);
}
