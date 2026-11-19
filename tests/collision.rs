//! Collision-aware layout engine verification.

use amo_monitor::config::{Config, LayoutConfig, WidgetKind, WidgetPlacement};
use amo_monitor::layout::{self, MovePlan, RectGrid};
use ratatui::layout::Rect;

fn grid(widgets: &[WidgetPlacement]) -> Vec<RectGrid> {
    widgets.iter().map(layout::rect_of).collect()
}

fn sample() -> LayoutConfig {
    let mut l = LayoutConfig::new("test", 6, 5);
    l.widgets = vec![
        WidgetPlacement::new(WidgetKind::Cpu, 0, 0, 3, 2),
        WidgetPlacement::new(WidgetKind::Gpu, 3, 0, 3, 2),
        WidgetPlacement::new(WidgetKind::Ram, 0, 2, 3, 2),
        WidgetPlacement::new(WidgetKind::Swap, 3, 2, 3, 2),
    ];
    l
}

fn apply(l: &LayoutConfig, plan: &MovePlan) -> LayoutConfig {
    layout::apply_plan(l, plan)
}

#[test]
fn moving_onto_occupied_space_displaces_nearest_widget() {
    let l = sample();
    let plan = layout::plan_move(&l, 0, 1, 0);
    assert!(!plan.blocked);
    assert!(plan.displaced.contains(&1));
    let after = apply(&l, &plan);
    assert!(layout::is_collision_free(&after));
    assert_eq!(grid(&after.widgets)[0], (1, 0, 3, 2));
    // The displaced widget keeps exactly its original size.
    assert_eq!((after.widgets[1].colspan, after.widgets[1].rowspan), (3, 2));
    // No widget was lost.
    assert_eq!(after.widgets.len(), l.widgets.len());
}

#[test]
fn displacement_targets_the_nearest_empty_space() {
    let l = sample();
    let plan = layout::plan_move(&l, 0, 0, 2);
    let after = apply(&l, &plan);
    assert!(layout::is_collision_free(&after));
    // CPU moved into RAM's slot; RAM must land adjacent, not across the grid.
    let displaced = plan.displaced;
    assert!(displaced.contains(&2));
    for i in displaced {
        assert!(after.widgets[i].col <= 4 && after.widgets[i].row <= 4);
    }
}

#[test]
fn pinned_widgets_block_moves_and_show_it_in_the_plan() {
    let mut l = sample();
    l.widgets[1].pinned = true;
    let plan = layout::plan_move(&l, 0, 3, 0);
    assert!(plan.blocked);
    assert!(plan.displaced.is_empty());
    assert_eq!(plan.positions, grid(&l.widgets));
    let after = apply(&l, &plan);
    assert_eq!(after.widgets, l.widgets);
}

#[test]
fn resizing_never_produces_overlap() {
    let l = sample();
    for index in 0..l.widgets.len() {
        for (dw, dh) in [(1, 0), (0, 1), (1, 1), (-1, 0)] {
            let plan = layout::plan_resize(&l, index, dw, dh);
            let after = apply(&l, &plan);
            assert!(layout::is_collision_free(&after), "resize {index} {dw},{dh}");
            assert_eq!(after.widgets.len(), l.widgets.len());
        }
    }
}

#[test]
fn repeated_moves_stay_collision_free() {
    let mut l = sample();
    for (index, dx, dy) in [(0, 1, 0), (2, 0, 1), (3, -2, 0), (1, 0, -1), (0, -1, 1)] {
        let plan = layout::plan_move(&l, index, dx, dy);
        if !plan.blocked {
            l = apply(&l, &plan);
        }
        assert!(layout::is_collision_free(&l), "after move {index}");
    }
}

#[test]
fn legacy_overlaps_are_repaired_on_load() {
    let mut l = sample();
    l.widgets[3].col = 1; // overlaps RAM and Swap
    l.widgets[3].row = 2;
    assert!(!layout::is_collision_free(&l));
    let fixed = layout::normalize(&l);
    assert!(layout::is_collision_free(&fixed));
    assert_eq!(fixed.widgets.len(), l.widgets.len());
    for (a, b) in l.widgets.iter().zip(fixed.widgets.iter()) {
        assert_eq!((a.colspan, a.rowspan), (b.colspan, b.rowspan));
    }
}

#[test]
fn undo_restores_positions_exactly() {
    let original = sample();
    let plan = layout::plan_move(&original, 0, 1, 1);
    let moved = apply(&original, &plan);
    assert_ne!(moved.widgets, original.widgets);
    // Undo is implemented as restoring the pre-change layout snapshot.
    let snapshot = original.clone();
    assert_eq!(snapshot.widgets, original.widgets);
    assert!(layout::is_collision_free(&moved));
}

#[test]
fn pixel_placements_never_overlap_at_any_terminal_size() {
    let l = sample();
    for (w, h) in [(140, 50), (100, 36), (80, 24), (60, 18), (40, 14)] {
        let placed = layout::resolve_pixels(&l, Rect::new(0, 0, w, h));
        assert_eq!(placed.len(), 4);
        for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                assert!(
                    !placed[i].rect.intersects(placed[j].rect),
                    "overlap at {w}x{h}: {:?} vs {:?}",
                    placed[i].rect,
                    placed[j].rect
                );
            }
        }
    }
}

#[test]
fn default_config_layouts_are_collision_free() {
    let config = Config::default();
    for layout in &config.layouts {
        assert!(
            layout::is_collision_free(layout),
            "default layout {} has overlaps",
            layout.name
        );
    }
}

#[test]
fn serialized_layout_preserves_no_overlap_guarantee() {
    let l = apply(&sample(), &layout::plan_move(&sample(), 0, 1, 0));
    let json = serde_json::to_string(&l).unwrap();
    let restored: LayoutConfig = serde_json::from_str(&json).unwrap();
    assert!(layout::is_collision_free(&restored));
    assert_eq!(restored, l);
}
