//! Collision-aware grid layout engine.
//!
//! Every widget occupies an explicit, non-overlapping grid rectangle. Moving
//! or resizing a widget produces a complete plan: the moved widget, every
//! displaced widget and their new positions. Plans are applied atomically, so
//! an invalid layout can never be committed.

use ratatui::layout::Rect;

use crate::config::{LayoutConfig, WidgetPlacement};

pub type RectGrid = (u16, u16, u16, u16); // col, row, colspan, rowspan

#[derive(Clone, Debug, PartialEq)]
pub struct MovePlan {
    /// New rectangles for every widget, indexed by widget position.
    pub positions: Vec<RectGrid>,
    pub displaced: Vec<usize>,
    pub blocked: bool,
    pub target: RectGrid,
}

impl MovePlan {
    fn unchanged(layout: &LayoutConfig, target: RectGrid, blocked: bool) -> Self {
        Self {
            positions: layout.widgets.iter().map(rect_of).collect(),
            displaced: Vec::new(),
            blocked,
            target,
        }
    }
}

pub fn rect_of(widget: &WidgetPlacement) -> RectGrid {
    (widget.col, widget.row, widget.colspan, widget.rowspan)
}

pub fn overlaps(a: RectGrid, b: RectGrid) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// Keep a widget inside the horizontal grid but allow its row to extend past
/// the configured height — displaced widgets may need "virtual" rows.
fn fit_rect(rect: RectGrid, cols: u16) -> RectGrid {
    let w = rect.2.clamp(1, cols.max(1));
    (rect.0.min(cols.saturating_sub(w)), rect.1, w, rect.3.max(1))
}

fn clamp_rect(rect: RectGrid, cols: u16, rows: u16) -> RectGrid {
    let w = rect.2.clamp(1, cols.max(1));
    let h = rect.3.clamp(1, rows.max(1));
    (
        rect.0.min(cols.saturating_sub(w)),
        rect.1.min(rows.saturating_sub(h)),
        w,
        h,
    )
}

fn occupancy(positions: &[RectGrid], cols: u16, rows: u16, exclude: Option<usize>) -> Vec<Vec<bool>> {
    let mut grid = vec![vec![false; cols as usize]; rows as usize];
    for (i, &(c, r, w, h)) in positions.iter().enumerate() {
        if Some(i) == exclude {
            continue;
        }
        for row in r..(r + h).min(rows) {
            for col in c..(c + w).min(cols) {
                grid[row as usize][col as usize] = true;
            }
        }
    }
    grid
}

fn fits(occ: &[Vec<bool>], rect: RectGrid) -> bool {
    let (c, r, w, h) = rect;
    if c + w > occ.first().map_or(0, Vec::len) as u16 {
        return false;
    }
    (r..r + h).all(|row| {
        occ.get(row as usize).is_some_and(|line| {
            (c..c + w).all(|col| !line[col as usize])
        })
    })
}

/// Find the closest free rectangle for `size` near `origin`.
///
/// Distance is Manhattan distance from the top-left corner, so displaced
/// widgets land next to where they were rather than jumping across the screen.
/// Returns the position and the number of extra virtual rows required.
pub fn nearest_free(
    positions: &[RectGrid],
    exclude: Option<usize>,
    size: (u16, u16),
    origin: (u16, u16),
    cols: u16,
    min_rows: u16,
) -> ((u16, u16), u16) {
    let (w, h) = (size.0.clamp(1, cols.max(1)), size.1.max(1));
    let max_bottom = positions
        .iter()
        .map(|&(_, r, _, hh)| r + hh)
        .max()
        .unwrap_or(0);
    let search_rows = min_rows.max(max_bottom).saturating_add(h + 2).max(1);

    let occ = occupancy(positions, cols, search_rows, exclude);
    let mut best: Option<(i64, u16, u16)> = None;
    for row in 0..search_rows {
        for col in 0..=cols.saturating_sub(w) {
            if !fits(&occ, (col, row, w, h)) {
                continue;
            }
            let distance = (col as i64 - origin.0 as i64).abs() + (row as i64 - origin.1 as i64).abs();
            if best.is_none_or(|(d, _, _)| distance < d) {
                best = Some((distance, col, row));
            }
        }
    }
    match best {
        Some((_, c, r)) => ((c, r), 0),
        None => ((0, max_bottom), max_bottom.saturating_sub(min_rows)),
    }
}

/// Repair a layout so no two widgets overlap. Pinned widgets are kept in
/// place; overlapping unpinned widgets are moved to their nearest free slots.
pub fn normalize(layout: &LayoutConfig) -> LayoutConfig {
    let mut result = layout.clone();
    let cols = result.columns.max(1);
    let rows = result.rows.max(1);
    let mut positions: Vec<RectGrid> = result.widgets.iter().map(rect_of).collect();

    // Pin conflicts are resolved by keeping the earlier pinned widget fixed
    // and unpinning later conflicting ones, which can then be relocated.
    for i in 0..positions.len() {
        if !result.widgets[i].pinned {
            continue;
        }
        let mut conflict = false;
        for j in 0..i {
            if positions[j].2 == 0 {
                continue;
            }
            if overlaps(positions[i], positions[j]) {
                conflict = true;
                break;
            }
        }
        if conflict {
            result.widgets[i].pinned = false;
        }
    }

    for i in 0..positions.len() {
        positions[i] = fit_rect(positions[i], cols);
        if result.widgets[i].pinned {
            continue;
        }
        let collision = (0..i).any(|j| overlaps(positions[i], positions[j]));
        if !collision {
            continue;
        }
        let ((c, r), _extra) = nearest_free(
            &positions,
            Some(i),
            (positions[i].2, positions[i].3),
            (positions[i].0, positions[i].1),
            cols,
            rows,
        );
        positions[i] = (c, r, positions[i].2, positions[i].3);
    }

    for (widget, rect) in result.widgets.iter_mut().zip(positions) {
        widget.col = rect.0;
        widget.row = rect.1;
        widget.colspan = rect.2;
        widget.rowspan = rect.3;
    }
    result
}

/// Plan a move by `(dx, dy)` grid cells.
pub fn plan_move(layout: &LayoutConfig, index: usize, dx: i16, dy: i16) -> MovePlan {
    let cols = layout.columns.max(1);
    let rows = layout.rows.max(1);
    let Some(current) = layout.widgets.get(index).map(rect_of) else {
        return MovePlan::unchanged(layout, (0, 0, 1, 1), true);
    };
    let target = clamp_rect(
        (
            (current.0 as i16 + dx).max(0) as u16,
            (current.1 as i16 + dy).max(0) as u16,
            current.2,
            current.3,
        ),
        cols,
        rows,
    );
    if target == current {
        return MovePlan::unchanged(layout, target, false);
    }
    plan_replace(layout, index, target)
}

/// Plan a resize by `(dw, dh)` grid cells.
pub fn plan_resize(layout: &LayoutConfig, index: usize, dw: i16, dh: i16) -> MovePlan {
    let cols = layout.columns.max(1);
    let rows = layout.rows.max(1);
    let Some(current) = layout.widgets.get(index).map(rect_of) else {
        return MovePlan::unchanged(layout, (0, 0, 1, 1), true);
    };
    let target = clamp_rect(
        (
            current.0,
            current.1,
            (current.2 as i16 + dw).max(1) as u16,
            (current.3 as i16 + dh).max(1) as u16,
        ),
        cols,
        rows,
    );
    if target == current {
        return MovePlan::unchanged(layout, target, false);
    }
    plan_replace(layout, index, target)
}

fn plan_replace(layout: &LayoutConfig, index: usize, target: RectGrid) -> MovePlan {
    let cols = layout.columns.max(1);
    let rows = layout.rows.max(1);
    let mut positions: Vec<RectGrid> = layout.widgets.iter().map(rect_of).collect();

    // A pinned widget is an immovable obstacle.
    for (i, widget) in layout.widgets.iter().enumerate() {
        if i != index && widget.pinned && overlaps(target, positions[i]) {
            return MovePlan::unchanged(layout, target, true);
        }
    }

    positions[index] = target;
    let displaced: Vec<usize> = (0..positions.len())
        .filter(|i| *i != index && overlaps(target, positions[*i]))
        .collect();
    let mut moved = Vec::new();
    for &i in &displaced {
        let origin = (positions[i].0, positions[i].1);
        let ((c, r), _) = nearest_free(
            &positions,
            Some(i),
            (positions[i].2, positions[i].3),
            origin,
            cols,
            rows,
        );
        positions[i] = fit_rect((c, r, positions[i].2, positions[i].3), cols);
        moved.push(i);
    }

    MovePlan {
        positions,
        displaced: moved,
        blocked: false,
        target,
    }
}

/// Apply an accepted plan to a layout.
pub fn apply_plan(layout: &LayoutConfig, plan: &MovePlan) -> LayoutConfig {
    let mut result = layout.clone();
    for (widget, rect) in result.widgets.iter_mut().zip(plan.positions.iter()) {
        widget.col = rect.0;
        widget.row = rect.1;
        widget.colspan = rect.2;
        widget.rowspan = rect.3;
    }
    result
}

/// Verify a layout has no overlapping widgets.
pub fn is_collision_free(layout: &LayoutConfig) -> bool {
    let positions: Vec<RectGrid> = layout.widgets.iter().map(rect_of).collect();
    for i in 0..positions.len() {
        for j in (i + 1)..positions.len() {
            if overlaps(positions[i], positions[j]) {
                return false;
            }
        }
    }
    true
}

#[derive(Clone, Debug)]
pub struct PlacedWidget {
    pub index: usize,
    pub rect: Rect,
}

/// Map grid rectangles to terminal pixels, adapting to any terminal size.
pub fn resolve_pixels(layout: &LayoutConfig, area: Rect) -> Vec<PlacedWidget> {
    let normalized = normalize(layout);
    let cols = normalized.columns.max(1);
    let mut max_row = normalized.rows.max(1);
    for widget in &normalized.widgets {
        max_row = max_row.max(widget.bottom());
    }
    let cell_w = area.width / cols;
    let cell_h = area.height / max_row;
    if cell_w == 0 || cell_h == 0 {
        return Vec::new();
    }
    normalized
        .widgets
        .iter()
        .enumerate()
        .map(|(index, widget)| {
            let x = area.x + widget.col * cell_w;
            let y = area.y + widget.row * cell_h;
            let mut width = widget.colspan * cell_w;
            let mut height = widget.rowspan * cell_h;
            if widget.right() < cols {
                width = width.saturating_sub(1);
            } else {
                width = area.right().saturating_sub(x);
            }
            if widget.bottom() < max_row {
                height = height.saturating_sub(1);
            } else {
                height = area.bottom().saturating_sub(y);
            }
            PlacedWidget {
                index,
                rect: Rect::new(x, y, width, height),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WidgetKind;

    fn layout() -> LayoutConfig {
        let mut l = LayoutConfig::new("test", 4, 4);
        l.widgets = vec![
            WidgetPlacement::new(WidgetKind::Cpu, 0, 0, 2, 2),
            WidgetPlacement::new(WidgetKind::Gpu, 2, 0, 2, 2),
            WidgetPlacement::new(WidgetKind::Ram, 0, 2, 2, 2),
        ];
        l
    }

    #[test]
    fn moving_onto_occupied_cell_displaces_nearest_widget() {
        let l = layout();
        let plan = plan_move(&l, 0, 1, 0);
        assert!(!plan.blocked);
        assert!(plan.displaced.contains(&1));
        let applied = apply_plan(&l, &plan);
        assert!(is_collision_free(&applied));
        assert_eq!(rect_of(&applied.widgets[0]), (1, 0, 2, 2));
        // The displaced GPU keeps its size.
        assert_eq!((applied.widgets[1].colspan, applied.widgets[1].rowspan), (2, 2));
    }

    #[test]
    fn pinned_widget_blocks_move() {
        let mut l = layout();
        l.widgets[1].pinned = true;
        let plan = plan_move(&l, 0, 2, 0);
        assert!(plan.blocked);
        assert_eq!(plan.positions, l.widgets.iter().map(rect_of).collect::<Vec<_>>());
    }

    #[test]
    fn resize_never_creates_overlap() {
        let l = layout();
        let plan = plan_resize(&l, 0, 1, 0);
        let applied = apply_plan(&l, &plan);
        assert!(is_collision_free(&applied));
    }

    #[test]
    fn normalization_repairs_overlapping_config() {
        let mut l = layout();
        l.widgets[2].col = 0;
        l.widgets[2].row = 0; // overlaps CPU
        assert!(!is_collision_free(&l));
        let fixed = normalize(&l);
        assert!(is_collision_free(&fixed));
    }

    #[test]
    fn nearest_free_prefers_close_positions() {
        let positions = vec![(0, 0, 2, 2), (2, 0, 2, 2)];
        let ((c, r), _) = nearest_free(&positions, None, (2, 2), (0, 2), 4, 4);
        assert_eq!((c, r), (0, 2));
    }

    #[test]
    fn pixel_rects_are_collision_free_at_any_size() {
        let l = layout();
        for area in [Rect::new(0, 0, 80, 40), Rect::new(0, 0, 40, 20), Rect::new(0, 0, 20, 12)] {
            let placed = resolve_pixels(&l, area);
            for i in 0..placed.len() {
                for j in (i + 1)..placed.len() {
                    assert!(!placed[i].rect.intersects(placed[j].rect));
                }
            }
        }
    }
}
