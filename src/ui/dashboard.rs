//! Dashboard rendering: grid panels, collision-free placement and the Linear
//! and Mixed presentation modes.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::Frame;

use crate::app::{App, LayoutPreview};
use crate::config::{Config, LayoutKind};
use crate::layout::{self, RectGrid};
use crate::responsive::{self, SizeClass};
use crate::ui::{linear, widgets};

pub use crate::layout::PlacedWidget;

/// Backwards-compatible placement helper.
pub fn compute_placements(layout: &crate::config::LayoutConfig, area: Rect) -> Vec<PlacedWidget> {
    layout::resolve_pixels(layout, area)
}

fn grid_to_pixel(rect: RectGrid, cols: u16, rows: u16, area: Rect) -> Rect {
    let cell_w = area.width / cols.max(1);
    let cell_h = area.height / rows.max(1);
    if cell_w == 0 || cell_h == 0 {
        return Rect::new(0, 0, 0, 0);
    }
    let (c, r, w, h) = rect;
    let x = area.x + c * cell_w;
    let y = area.y + r * cell_h;
    let width = if c + w < cols {
        (w * cell_w).saturating_sub(1)
    } else {
        area.right().saturating_sub(x)
    };
    let height = if r + h < rows {
        (h * cell_h).saturating_sub(1)
    } else {
        area.bottom().saturating_sub(y)
    };
    Rect::new(x, y, width, height)
}

fn outline(frame: &mut Frame, rect: Rect, color: ratatui::style::Color, label: &str) {
    if rect.width < 2 || rect.height < 2 {
        return;
    }
    let buf = frame.buffer_mut();
    let last_x = rect.right() - 1;
    let last_y = rect.bottom() - 1;
    for x in rect.x..rect.right() {
        let ch = if x == rect.x {
            '╔'
        } else if x == last_x {
            '╗'
        } else {
            '═'
        };
        if let Some(cell) = buf.cell_mut((x, rect.y)) {
            cell.set_char(ch).set_style(Style::default().fg(color));
        }
        if let Some(cell) = buf.cell_mut((x, last_y)) {
            let ch = if x == rect.x { '╚' } else if x == last_x { '╝' } else { '═' };
            cell.set_char(ch).set_style(Style::default().fg(color));
        }
    }
    for y in (rect.y + 1)..last_y {
        if let Some(cell) = buf.cell_mut((rect.x, y)) {
            cell.set_char('║').set_style(Style::default().fg(color));
        }
        if let Some(cell) = buf.cell_mut((last_x, y)) {
            cell.set_char('║').set_style(Style::default().fg(color));
        }
    }
    if rect.width > label.len() as u16 + 2 {
        buf.set_string(
            rect.x + 1,
            rect.y,
            label,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        );
    }
}

fn draw_preview(frame: &mut Frame, area: Rect, app: &App, preview: &LayoutPreview) {
    let layout = app.active_layout();
    let cols = layout.columns.max(1);
    let rows = preview
        .positions
        .iter()
        .map(|&(_, r, _, h)| r + h)
        .max()
        .unwrap_or(layout.rows)
        .max(layout.rows);
    let color = if preview.blocked {
        app.resolved.danger
    } else {
        app.resolved.accent
    };
    let target = grid_to_pixel(preview.target, cols, rows, area);
    let label = if preview.blocked { " BLOCKED " } else { " DROP " };
    outline(frame, target, color, label);

    if !preview.blocked {
        for &index in &preview.displaced {
            let Some(rect) = preview.positions.get(index) else { continue };
            outline(frame, grid_to_pixel(*rect, cols, rows, area), app.resolved.warning, " MOVED ");
        }
    }
}

pub fn render_grid(frame: &mut Frame, area: Rect, app: &App, class: SizeClass) {
    // While a move/resize is being previewed the original grid is shown so the
    // drop outlines stay aligned; otherwise adapt to the current space.
    let adapted;
    let layout = if app.preview.is_some() {
        app.active_layout()
    } else {
        adapted = responsive::adapt_layout(app.active_layout(), class);
        &adapted
    };
    let (placements, preview_layout) = match &app.preview {
        Some(preview) => {
            let mut temp = layout.clone();
            for (widget, rect) in temp.widgets.iter_mut().zip(preview.positions.iter()) {
                widget.col = rect.0;
                widget.row = rect.1;
                widget.colspan = rect.2;
                widget.rowspan = rect.3;
            }
            (layout::resolve_pixels(&temp, area), Some(temp))
        }
        None => (layout::resolve_pixels(layout, area), None),
    };
    for placement in &placements {
        let Some(widget) = preview_layout
            .as_ref()
            .unwrap_or(layout)
            .widgets
            .get(placement.index)
        else {
            continue;
        };
        widgets::render_widget(
            frame,
            placement.rect,
            widget,
            app,
            placement.index == app.selected,
            class,
        );
    }
    if let Some(preview) = &app.preview {
        draw_preview(frame, area, app, preview);
    }
}

pub fn render_dashboard(frame: &mut Frame, area: Rect, app: &App, class: SizeClass) {
    let kind = responsive::effective_kind(app.active_layout().kind, class);
    if kind == LayoutKind::Grid {
        render_grid(frame, area, app, class);
        return;
    }
    // A small terminal may force Linear presentation onto a grid layout that
    // has no metrics of its own. Use the default metric set for this frame
    // only — saved user configuration is never modified.
    let layout = app.active_layout();
    if !layout.linear_metrics.is_empty() {
        match kind {
            LayoutKind::Linear => linear::render_linear(frame, area, layout, app, class),
            _ => linear::render_mixed(frame, area, layout, app, class),
        }
        return;
    }
    let defaults = Config::default();
    let mut fallback = layout.clone();
    if let Some(source) = defaults.layouts.iter().find(|l| l.kind == LayoutKind::Linear) {
        fallback.linear_metrics = source.linear_metrics.clone();
    }
    match kind {
        LayoutKind::Linear => linear::render_linear(frame, area, &fallback, app, class),
        _ => linear::render_mixed(frame, area, &fallback, app, class),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LayoutConfig, WidgetKind, WidgetPlacement};

    #[test]
    fn pixel_rects_never_overlap() {
        let mut layout = LayoutConfig::new("test", 4, 4);
        for i in 0..4 {
            layout.widgets.push(WidgetPlacement::new(WidgetKind::Cpu, i, 0, 1, 1));
        }
        let placed = compute_placements(&layout, Rect::new(0, 0, 40, 20));
        for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                assert!(!placed[i].rect.intersects(placed[j].rect));
            }
        }
    }

    #[test]
    fn tiny_area_yields_no_placements() {
        let layout = LayoutConfig::new("test", 12, 8);
        assert!(compute_placements(&layout, Rect::new(0, 0, 2, 2)).is_empty());
    }
}
