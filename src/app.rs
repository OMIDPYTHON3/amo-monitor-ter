//! Application state and keyboard-driven interaction model.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::theme::{ResolvedTheme, ThemeSet};
use crate::config::{
    ChartType, Config, DisplayMode, HISTORY_WINDOWS, LayoutConfig, LayoutKind, LINE_THICKNESSES,
    LinearMetric, LinearMetricKind, ViewMode, WidgetConfig, WidgetKind, WidgetPlacement, YScale,
};
use crate::colors::{SCHEMES, Metric as ColorMetric};
use crate::layout::{self, MovePlan, RectGrid};
use crate::monitor::Monitor;

const REFRESH_CHOICES: [u64; 5] = [250, 500, 1000, 2000, 5000];

#[derive(Clone, Debug)]
pub enum Mode {
    Normal,
    Move,
    Resize,
    Help,
    Settings { tab: usize, index: usize },
    WidgetConfig { widget: usize, option: usize },
    LayoutManager { index: usize },
    ThemeMenu { index: usize },
    AddWidget { index: usize },
    LinearMetrics { index: usize },
    TextInput { purpose: InputPurpose, value: String },
}

#[derive(Clone, Debug)]
pub enum InputPurpose {
    NewLayout,
    RenameLayout,
    Accent,
    AccentAlt,
    Threshold(usize),
    ColorValue(usize),
}

/// A live, uncommitted layout change shown while moving or resizing.
#[derive(Clone, Debug)]
pub struct LayoutPreview {
    pub positions: Vec<RectGrid>,
    pub displaced: Vec<usize>,
    pub blocked: bool,
    pub target: RectGrid,
}

pub struct App {
    pub monitor: Monitor,
    pub config: Config,
    pub themes: ThemeSet,
    pub resolved: ResolvedTheme,
    pub mode: Mode,
    pub selected: usize,
    pub running: bool,
    pub status: Option<(String, Instant)>,
    pub preview: Option<LayoutPreview>,
    pub pending_plan: Option<MovePlan>,
    pub undo_stack: Vec<LayoutConfig>,
    pub settings_metric: usize,
}

impl App {
    pub fn new() -> Self {
        let config = Config::load();
        let themes = ThemeSet::load(&Config::config_dir());
        // Generate the editable theme file on first run.
        let _ = themes.save(&Config::config_dir());
        let resolved = themes.resolve(&config.theme, &config.color_overrides);
        Self {
            monitor: Monitor::new(),
            config,
            themes,
            resolved,
            mode: Mode::Normal,
            selected: 0,
            running: true,
            status: None,
            preview: None,
            pending_plan: None,
            undo_stack: Vec::new(),
            settings_metric: 0,
        }
    }

    pub fn active_layout(&self) -> &LayoutConfig {
        self.config.active_layout()
    }

    pub fn active_layout_mut(&mut self) -> &mut LayoutConfig {
        self.config.active_layout_mut()
    }

    pub fn tick(&mut self) {
        self.monitor.sample();
    }

    pub fn refresh_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.config.refresh_ms)
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now()));
    }

    pub fn save(&mut self) {
        match self.config.save() {
            Ok(()) => self.set_status("Configuration saved"),
            Err(e) => self.set_status(format!("Save failed: {e}")),
        }
    }

    fn set_theme(&mut self, name: &str) {
        self.config.theme = name.to_string();
        self.resolved = self.themes.resolve(name, &self.config.color_overrides);
        self.save();
    }

    /// Navigate widget selection in visual (row-major) order.
    fn navigate(&mut self, delta: isize) {
        let layout = self.active_layout();
        if layout.widgets.is_empty() {
            return;
        }
        let mut order: Vec<usize> = (0..layout.widgets.len()).collect();
        order.sort_by_key(|i| {
            let w = &layout.widgets[*i];
            (w.row, w.col)
        });
        let current = order.iter().position(|i| *i == self.selected).unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(order.len() as isize) as usize;
        self.selected = order[next];
    }

    /// When a layout changes presentation style, give it the default content
    /// for that style if it has none, without discarding any saved content.
    pub fn ensure_layout_content(&mut self) {
        let defaults = Config::default();
        let name = self.active_layout().name.clone();
        let kind = self.active_layout().kind;
        if let Some(layout) = self.config.layouts.iter_mut().find(|l| l.name == name) {
            if matches!(kind, LayoutKind::Linear | LayoutKind::Mixed) && layout.linear_metrics.is_empty() {
                if let Some(source) = defaults.layouts.iter().find(|l| l.kind == LayoutKind::Linear) {
                    layout.linear_metrics = source.linear_metrics.clone();
                }
            }
            if matches!(kind, LayoutKind::Grid | LayoutKind::Mixed) && layout.widgets.is_empty() {
                if let Some(source) = defaults.layouts.iter().find(|l| l.kind == LayoutKind::Grid) {
                    layout.widgets = source.widgets.clone();
                }
            }
        }
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(self.active_layout().clone());
        if self.undo_stack.len() > 32 {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self) {
        let Some(previous) = self.undo_stack.pop() else {
            self.set_status("Nothing to undo");
            return;
        };
        let name = previous.name.clone();
        if let Some(slot) = self.config.layouts.iter_mut().find(|l| l.name == name) {
            *slot = previous;
        } else {
            self.config.layouts.push(previous);
            self.config.active_layout = name.clone();
        }
        self.selected = self.selected.min(self.active_layout().widgets.len().saturating_sub(1));
        self.preview = None;
        self.pending_plan = None;
        self.set_status(format!("Undo — {name} layout restored"));
        self.save();
    }

    fn show_plan(&mut self, plan: MovePlan) {
        self.preview = Some(LayoutPreview {
            positions: plan.positions.clone(),
            displaced: plan.displaced.clone(),
            blocked: plan.blocked,
            target: plan.target,
        });
        let current: Vec<RectGrid> = self.active_layout().widgets.iter().map(layout::rect_of).collect();
        self.pending_plan = if plan.blocked || plan.positions == current {
            None
        } else {
            Some(plan)
        };
        if self.preview.as_ref().is_some_and(|p| p.blocked) {
            self.set_status("Blocked — a pinned widget occupies that space");
        }
    }

    fn preview_move(&mut self, dx: i16, dy: i16) {
        if self.selected >= self.active_layout().widgets.len() {
            return;
        }
        let plan = layout::plan_move(self.active_layout(), self.selected, dx, dy);
        self.show_plan(plan);
    }

    fn preview_resize(&mut self, dw: i16, dh: i16) {
        let selected = self.selected;
        if self.active_layout().widgets.get(selected).is_none() {
            return;
        }
        let plan = layout::plan_resize(self.active_layout(), selected, dw, dh);
        self.show_plan(plan);
    }

    fn commit_preview(&mut self) {
        if let Some(plan) = self.pending_plan.take() {
            self.push_undo();
            let applied = layout::apply_plan(self.active_layout(), &plan);
            *self.active_layout_mut() = applied;
            // A manually positioned widget is pinned so it keeps its place.
            let selected = self.selected;
            if let Some(widget) = self.active_layout_mut().widgets.get_mut(selected) {
                widget.pinned = true;
            }
            self.set_status(if plan.displaced.is_empty() {
                "Layout updated".to_string()
            } else {
                format!("Layout updated — {} widget(s) displaced", plan.displaced.len())
            });
            self.save();
        }
        self.preview = None;
    }

    fn cancel_preview(&mut self) {
        self.preview = None;
        self.pending_plan = None;
        self.set_status("Move cancelled");
    }

    fn toggle_pin(&mut self) {
        self.push_undo();
        let selected = self.selected;
        let pinned = {
            let Some(widget) = self.active_layout_mut().widgets.get_mut(selected) else {
                return;
            };
            widget.pinned = !widget.pinned;
            widget.pinned
        };
        self.set_status(if pinned { "Widget pinned" } else { "Widget unpinned" });
        self.save();
    }

    fn remove_selected(&mut self) {
        self.push_undo();
        let selected = self.selected;
        if selected < self.active_layout().widgets.len() {
            let kind = self.active_layout().widgets[selected].kind.title();
            self.active_layout_mut().widgets.remove(selected);
            self.selected = selected.saturating_sub(1);
            self.set_status(format!("{kind} widget removed"));
            self.save();
        }
    }

    fn add_widget(&mut self, kind: WidgetKind) {
        self.push_undo();
        let (cols, rows) = {
            let l = self.active_layout();
            (l.columns, l.rows)
        };
        let colspan = if cols >= 6 { 4 } else { cols };
        let rowspan = if rows >= 4 { 3 } else { rows };
        let (col, row) = self.active_layout().find_free_slot(colspan, rowspan);
        let mut placement = WidgetPlacement::new(kind, col, row, colspan, rowspan);
        // Compact widgets look better in a small default footprint.
        placement.config = WidgetConfig {
            display_mode: if matches!(kind, WidgetKind::Uptime) {
                DisplayMode::Compact
            } else {
                DisplayMode::Medium
            },
            ..WidgetConfig::default()
        };
        self.selected = self.active_layout().widgets.len();
        self.active_layout_mut().widgets.push(placement);
        self.set_status(format!("{} widget added", kind.title()));
        self.save();
        self.mode = Mode::Normal;
    }

    fn cycle_theme(&mut self, delta: isize) {
        let names = self.themes.names();
        if names.is_empty() {
            return;
        }
        let current = names
            .iter()
            .position(|n| *n == self.config.theme)
            .unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(names.len() as isize) as usize;
        self.set_theme(&names[next]);
    }

    fn cycle_refresh(&mut self, delta: isize) {
        let current = REFRESH_CHOICES
            .iter()
            .position(|v| *v == self.config.refresh_ms)
            .unwrap_or(2) as isize;
        let next = (current + delta).rem_euclid(REFRESH_CHOICES.len() as isize) as usize;
        self.config.refresh_ms = REFRESH_CHOICES[next];
        self.save();
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C')) {
            self.running = false;
            return;
        }
        if ctrl && matches!(key.code, KeyCode::Char('z') | KeyCode::Char('Z')) {
            self.undo();
            return;
        }

        match &self.mode.clone() {
            Mode::Normal => self.handle_normal(key),
            Mode::Move => match key.code {
                KeyCode::Left | KeyCode::Char('h') => self.preview_move(-1, 0),
                KeyCode::Right | KeyCode::Char('l') => self.preview_move(1, 0),
                KeyCode::Up | KeyCode::Char('k') => self.preview_move(0, -1),
                KeyCode::Down | KeyCode::Char('j') => self.preview_move(0, 1),
                KeyCode::Enter | KeyCode::Char('m') => {
                    self.commit_preview();
                    self.mode = Mode::Normal;
                }
                KeyCode::Esc => {
                    self.cancel_preview();
                    self.mode = Mode::Normal;
                }
                _ => {}
            },
            Mode::Resize => match key.code {
                KeyCode::Left => self.preview_resize(-1, 0),
                KeyCode::Right => self.preview_resize(1, 0),
                KeyCode::Up => self.preview_resize(0, -1),
                KeyCode::Down => self.preview_resize(0, 1),
                KeyCode::Enter | KeyCode::Char('r') => {
                    self.commit_preview();
                    self.mode = Mode::Normal;
                }
                KeyCode::Esc => {
                    self.cancel_preview();
                    self.mode = Mode::Normal;
                }
                _ => {}
            },
            Mode::Help | Mode::Settings { .. } | Mode::WidgetConfig { .. }
            | Mode::LayoutManager { .. } | Mode::ThemeMenu { .. } | Mode::AddWidget { .. }
            | Mode::LinearMetrics { .. } | Mode::TextInput { .. } => self.handle_menu(key),
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char('s') => self.mode = Mode::Settings { tab: 0, index: 0 },
            KeyCode::Char('t') => {
                let names = self.themes.names();
                let index = names
                    .iter()
                    .position(|n| *n == self.config.theme)
                    .unwrap_or(0);
                self.mode = Mode::ThemeMenu { index };
            }
            KeyCode::Char('l') => {
                let index = self.config.layout_index();
                self.mode = Mode::LayoutManager { index };
            }
            KeyCode::Char('a') => self.mode = Mode::AddWidget { index: 0 },
            KeyCode::Char('c') => {
                if !self.active_layout().widgets.is_empty() {
                    self.mode = Mode::WidgetConfig {
                        widget: self.selected,
                        option: 0,
                    };
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => self.remove_selected(),
            KeyCode::Char(' ') => self.toggle_pin(),
            KeyCode::Char('m') => self.mode = Mode::Move,
            KeyCode::Char('r') => self.mode = Mode::Resize,
            KeyCode::Char('v') => {
                self.push_undo();
                let current = self.active_layout().kind;
                let index = LayoutKind::ALL.iter().position(|k| *k == current).unwrap_or(0);
                let next = LayoutKind::ALL[(index + 1) % LayoutKind::ALL.len()];
                self.active_layout_mut().kind = next;
                self.ensure_layout_content();
                self.set_status(format!("Dashboard: {}", next.label()));
                self.save();
            }
            KeyCode::Char('y') => self.mode = Mode::LinearMetrics { index: 0 },
            KeyCode::Tab | KeyCode::Right | KeyCode::Down => self.navigate(1),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Up => self.navigate(-1),
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let index = (c.to_digit(10).unwrap() as usize).checked_sub(1);
                if let Some(layout) = index.and_then(|i| self.config.layouts.get(i)) {
                    let name = layout.name.clone();
                    self.config.active_layout = name;
                    self.selected = 0;
                    self.set_status(format!("Layout: {}", self.config.active_layout));
                    self.save();
                }
            }
            KeyCode::F(5) => self.save(),
            _ => {}
        }
    }

    fn handle_menu(&mut self, key: KeyEvent) {
        match self.mode.clone() {
            Mode::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?')) {
                    self.mode = Mode::Normal;
                }
            }
            Mode::Settings { mut tab, mut index } => {
                match key.code {
                    KeyCode::Esc => self.mode = Mode::Normal,
                    KeyCode::Tab => {
                        tab = (tab + 1) % 4;
                        index = 0;
                        self.mode = Mode::Settings { tab, index };
                    }
                    KeyCode::BackTab => {
                        tab = (tab + 3) % 4;
                        index = 0;
                        self.mode = Mode::Settings { tab, index };
                    }
                    KeyCode::Up => {
                        index = index.saturating_sub(1);
                        self.mode = Mode::Settings { tab, index };
                    }
                    KeyCode::Down => {
                        index = (index + 1).min(self.settings_item_count(tab) - 1);
                        self.mode = Mode::Settings { tab, index };
                    }
                    KeyCode::Left | KeyCode::Right => {
                        let delta = if matches!(key.code, KeyCode::Right) { 1 } else { -1 };
                        self.settings_adjust(tab, index, delta);
                    }
                    KeyCode::Char(' ') => self.settings_adjust(tab, index, 1),
                    KeyCode::Char('e') if tab == 0 && (index == 2 || index == 3) => {
                        self.mode = Mode::TextInput {
                            purpose: if index == 2 {
                                InputPurpose::Accent
                            } else {
                                InputPurpose::AccentAlt
                            },
                            value: String::new(),
                        };
                    }
                    KeyCode::Char('c') if tab == 1 && !self.active_layout().widgets.is_empty() => {
                        self.mode = Mode::WidgetConfig {
                            widget: index.min(self.active_layout().widgets.len() - 1),
                            option: 0,
                        };
                    }
                    KeyCode::Char('a') if tab == 1 => self.mode = Mode::AddWidget { index: 0 },
                    KeyCode::Char('d') | KeyCode::Delete if tab == 1 => self.remove_selected(),
                    KeyCode::Enter if tab == 2 => {
                        let name = self.config.layouts.get(index).map(|l| l.name.clone());
                        if let Some(name) = name {
                            self.config.active_layout = name;
                            self.selected = 0;
                            self.save();
                        }
                    }
                    KeyCode::Char('n') if tab == 2 => {
                        self.mode = Mode::TextInput {
                            purpose: InputPurpose::NewLayout,
                            value: String::new(),
                        };
                    }
                    KeyCode::Char('r') if tab == 2 => {
                        self.mode = Mode::TextInput {
                            purpose: InputPurpose::RenameLayout,
                            value: self.config.active_layout.clone(),
                        };
                    }
                    KeyCode::Char('e') if tab == 3 && (4..=6).contains(&index) => {
                        self.mode = Mode::TextInput {
                            purpose: InputPurpose::Threshold(index - 4),
                            value: String::new(),
                        };
                    }
                    KeyCode::Char('e') if tab == 3 && (7..=10).contains(&index) => {
                        self.mode = Mode::TextInput {
                            purpose: InputPurpose::ColorValue(index - 7),
                            value: String::new(),
                        };
                    }
                    KeyCode::Enter if tab == 3 && index == 11 => {
                        self.config.dynamic_colors.reset();
                        self.set_status("Dynamic colours reset to defaults");
                        self.save();
                    }
                    _ => {}
                }
            }
            Mode::WidgetConfig { widget, option } => {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => self.mode = Mode::Normal,
                    KeyCode::Up => self.mode = Mode::WidgetConfig {
                        widget,
                        option: option.saturating_sub(1),
                    },
                    KeyCode::Down => self.mode = Mode::WidgetConfig {
                        widget,
                        option: (option + 1).min(14),
                    },
                    KeyCode::Left | KeyCode::Right => {
                        let delta = if matches!(key.code, KeyCode::Right) { 1 } else { -1 };
                        self.configure_widget(widget, option, delta);
                    }
                    KeyCode::Char(' ') => self.configure_widget(widget, option, 1),
                    _ => {}
                }
            }
            Mode::LayoutManager { index } => {
                match key.code {
                    KeyCode::Esc => self.mode = Mode::Normal,
                    KeyCode::Up => self.mode = Mode::LayoutManager { index: index.saturating_sub(1) },
                    KeyCode::Down => {
                        self.mode = Mode::LayoutManager {
                            index: (index + 1).min(self.config.layouts.len() - 1),
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(name) = self.config.layouts.get(index).map(|l| l.name.clone()) {
                            self.config.active_layout = name;
                            self.selected = 0;
                            self.save();
                        }
                    }
                    KeyCode::Char('n') => {
                        self.mode = Mode::TextInput {
                            purpose: InputPurpose::NewLayout,
                            value: String::new(),
                        }
                    }
                    KeyCode::Char('r') => {
                        self.mode = Mode::TextInput {
                            purpose: InputPurpose::RenameLayout,
                            value: self.config.active_layout.clone(),
                        }
                    }
                    KeyCode::Char('d') | KeyCode::Delete if self.config.layouts.len() > 1 => {
                        let name = self.config.layouts[index].name.clone();
                        self.config.layouts.remove(index);
                        if self.config.active_layout == name {
                            let first = self.config.layouts[0].name.clone();
                            self.config.active_layout = first;
                        }
                        self.set_status(format!("Layout {name} deleted"));
                        self.save();
                        self.mode = Mode::LayoutManager { index: index.saturating_sub(1) };
                    }
                    _ => {}
                }
            }
            Mode::ThemeMenu { index } => {
                let names = self.themes.names();
                match key.code {
                    KeyCode::Esc => self.mode = Mode::Normal,
                    KeyCode::Up => self.mode = Mode::ThemeMenu { index: index.saturating_sub(1) },
                    KeyCode::Down => self.mode = Mode::ThemeMenu { index: (index + 1).min(names.len() - 1) },
                    KeyCode::Enter => {
                        if let Some(name) = names.get(index) {
                            self.set_theme(name);
                        }
                        self.mode = Mode::Normal;
                    }
                    _ => {}
                }
            }
            Mode::AddWidget { index } => {
                match key.code {
                    KeyCode::Esc => self.mode = Mode::Normal,
                    KeyCode::Up => self.mode = Mode::AddWidget { index: index.saturating_sub(1) },
                    KeyCode::Down => self.mode = Mode::AddWidget { index: (index + 1).min(WidgetKind::ALL.len() - 1) },
                    KeyCode::Enter => {
                        if let Some(kind) = WidgetKind::ALL.get(index) {
                            self.add_widget(*kind);
                        }
                    }
                    _ => {}
                }
            }
            Mode::LinearMetrics { index } => match key.code {
                KeyCode::Esc | KeyCode::Enter => self.mode = Mode::Normal,
                KeyCode::Up => self.mode = Mode::LinearMetrics { index: index.saturating_sub(1) },
                KeyCode::Down => {
                    let len = self.active_layout().linear_metrics.len();
                    self.mode = Mode::LinearMetrics {
                        index: if len == 0 { 0 } else { (index + 1).min(len - 1) },
                    };
                }
                KeyCode::Left | KeyCode::Right => {
                    let delta = if matches!(key.code, KeyCode::Right) { 1 } else { -1 };
                    self.reorder_linear_metric(index, delta);
                }
                KeyCode::Char(' ') => self.toggle_linear_pin(index),
                KeyCode::Char('c') => self.toggle_linear_chart(index),
                KeyCode::Char('g') => self.cycle_linear_group(index),
                KeyCode::Char('a') => self.add_linear_metric(),
                KeyCode::Char('d') | KeyCode::Delete => self.remove_linear_metric(index),
                KeyCode::Char('v') => {
                    let current = self.active_layout().linear_display;
                    let pos = DisplayMode::ALL.iter().position(|m| *m == current).unwrap_or(0);
                    let next = DisplayMode::ALL[(pos + 1) % DisplayMode::ALL.len()];
                    self.active_layout_mut().linear_display = next;
                    self.save();
                }
                _ => {}
            },
            Mode::TextInput { purpose, value } => match key.code {
                KeyCode::Esc => self.mode = Mode::Normal,
                KeyCode::Enter => {
                    self.commit_text_input(&purpose, &value);
                    self.mode = Mode::Normal;
                }
                KeyCode::Backspace => {
                    let mut v = value;
                    v.pop();
                    self.mode = Mode::TextInput { purpose, value: v };
                }
                KeyCode::Char(c) if !c.is_control() => {
                    let mut v = value;
                    v.push(c);
                    self.mode = Mode::TextInput { purpose, value: v };
                }
                _ => {}
            },
            Mode::Normal | Mode::Move | Mode::Resize => {}
        }
    }

    fn settings_item_count(&self, tab: usize) -> usize {
        match tab {
            0 => 4,
            1 => self.active_layout().widgets.len().max(1),
            2 => self.config.layouts.len(),
            _ => 12,
        }
    }

    fn reorder_linear_metric(&mut self, index: usize, delta: isize) {
        self.push_undo();
        let metrics = &mut self.active_layout_mut().linear_metrics;
        if index >= metrics.len() {
            return;
        }
        let target = (index as isize + delta).clamp(0, metrics.len() as isize - 1) as usize;
        if target != index {
            metrics.swap(index, target);
            let a = metrics[index].order;
            metrics[index].order = metrics[target].order;
            metrics[target].order = a;
            let ga = metrics[index].group;
            metrics[index].group = metrics[target].group;
            metrics[target].group = ga;
            self.save();
        }
    }

    fn toggle_linear_pin(&mut self, index: usize) {
        self.push_undo();
        if let Some(metric) = self.active_layout_mut().linear_metrics.get_mut(index) {
            metric.pinned = !metric.pinned;
            self.save();
        }
    }

    fn toggle_linear_chart(&mut self, index: usize) {
        if let Some(metric) = self.active_layout_mut().linear_metrics.get_mut(index) {
            metric.show_chart = !metric.show_chart;
            self.save();
        }
    }

    fn cycle_linear_group(&mut self, index: usize) {
        if let Some(metric) = self.active_layout_mut().linear_metrics.get_mut(index) {
            metric.group = (metric.group + 1) % 6;
            self.save();
        }
    }

    fn add_linear_metric(&mut self) {
        self.push_undo();
        let present: Vec<LinearMetricKind> = self
            .active_layout()
            .linear_metrics
            .iter()
            .map(|m| m.kind)
            .collect();
        let Some(kind) = LinearMetricKind::ALL.iter().find(|k| !present.contains(k)).copied() else {
            self.set_status("All metrics are already displayed");
            return;
        };
        let order = self.active_layout().linear_metrics.len() as u16;
        let group = kind.group();
        self.active_layout_mut()
            .linear_metrics
            .push(LinearMetric::new(kind, group, order));
        self.set_status(format!("{} metric added", kind.label()));
        self.save();
    }

    fn remove_linear_metric(&mut self, index: usize) {
        self.push_undo();
        if index < self.active_layout().linear_metrics.len() {
            let label = self.active_layout().linear_metrics[index].kind.label();
            self.active_layout_mut().linear_metrics.remove(index);
            self.set_status(format!("{label} metric removed"));
            self.save();
        }
    }

    fn settings_adjust(&mut self, tab: usize, index: usize, delta: isize) {
        match tab {
            0 => match index {
                0 => self.cycle_theme(delta),
                1 => self.cycle_refresh(delta),
                2 => {
                    // Pressing left/right here toggles between the two accent fields.
                    let next = if delta > 0 { "accent_alt" } else { "accent" };
                    self.config.color_overrides.entry(next.to_string()).or_default();
                    self.resolved = self.themes.resolve(&self.config.theme, &self.config.color_overrides);
                }
                _ => {}
            },
            1 => {
                if let Some(widget) = self.active_layout_mut().widgets.get_mut(index) {
                    if delta > 0 {
                        widget.pinned = !widget.pinned;
                    }
                }
                self.save();
            }
            2 => {
                let target = (index as isize + delta).rem_euclid(self.config.layouts.len() as isize) as usize;
                if let Some(name) = self.config.layouts.get(target).map(|l| l.name.clone()) {
                    self.config.active_layout = name;
                    self.selected = 0;
                    self.save();
                }
            }
            3 => {
                let colors = &mut self.config.dynamic_colors;
                match index {
                    0 => colors.enabled = !colors.enabled,
                    1 => {
                        let current = SCHEMES.iter().position(|(n, _)| *n == colors.scheme).unwrap_or(0) as isize;
                        let next = (current + delta).rem_euclid(SCHEMES.len() as isize) as usize;
                        colors.apply_scheme(SCHEMES[next].0);
                    }
                    2 | 3 => {
                        let next = (self.settings_metric as isize + delta)
                            .rem_euclid(ColorMetric::ALL.len() as isize) as usize;
                        let metric = ColorMetric::ALL[next];
                        if index == 3 {
                            let mut cfg = colors.metric(metric);
                            cfg.enabled = !cfg.enabled;
                            colors.set_metric(metric, cfg);
                        }
                        self.settings_metric = next;
                    }
                    4..=6 => {
                        let metric = ColorMetric::ALL[self.settings_metric];
                        let mut cfg = colors.metric(metric);
                        let step = if metric.is_percentage() { 5.0 } else { 10.0 };
                        cfg.thresholds[index - 4] =
                            (cfg.thresholds[index - 4] + delta as f64 * step).max(0.0);
                        colors.set_metric(metric, cfg.normalized());
                    }
                    7..=10 => {
                        let metric = ColorMetric::ALL[self.settings_metric];
                        let mut cfg = colors.metric(metric);
                        let choices = ["foreground", "warning", "orange", "danger", "success", "accent"];
                        let current = choices
                            .iter()
                            .position(|c| *c == cfg.colors[index - 7])
                            .unwrap_or(0) as isize;
                        let next = (current + delta).rem_euclid(choices.len() as isize) as usize;
                        cfg.colors[index - 7] = choices[next].to_string();
                        colors.set_metric(metric, cfg);
                    }
                    _ => {}
                }
                self.save();
            }
            _ => {}
        }
    }

    fn current_settings_metric(&self) -> ColorMetric {
        ColorMetric::ALL[self.settings_metric.min(ColorMetric::ALL.len() - 1)]
    }

    fn configure_widget(&mut self, index: usize, option: usize, delta: isize) {
        let Some(widget) = self.active_layout_mut().widgets.get_mut(index) else {
            return;
        };
        let cfg = &mut widget.config;
        match option {
            0 => cfg.show_primary = !cfg.show_primary,
            1 => cfg.show_frequency = !cfg.show_frequency,
            2 => cfg.show_power = !cfg.show_power,
            3 => cfg.show_temperature = !cfg.show_temperature,
            4 => cfg.show_extra = !cfg.show_extra,
            5 => cfg.show_chart = !cfg.show_chart,
            6 => {
                let current = DisplayMode::ALL.iter().position(|m| *m == cfg.display_mode).unwrap_or(1) as isize;
                let next = (current + delta).rem_euclid(DisplayMode::ALL.len() as isize) as usize;
                cfg.display_mode = DisplayMode::ALL[next];
            }
            7 => {
                let current = ViewMode::ALL.iter().position(|m| *m == cfg.view_mode).unwrap_or(1) as isize;
                let next = (current + delta).rem_euclid(ViewMode::ALL.len() as isize) as usize;
                cfg.view_mode = ViewMode::ALL[next];
            }
            8 => {
                let current = ChartType::ALL.iter().position(|m| *m == cfg.chart_type).unwrap_or(1) as isize;
                let next = (current + delta).rem_euclid(ChartType::ALL.len() as isize) as usize;
                cfg.chart_type = ChartType::ALL[next];
            }
            9 => {
                let current = HISTORY_WINDOWS.iter().position(|w| *w == cfg.history_window_secs).unwrap_or(0) as isize;
                let next = (current + delta).rem_euclid(HISTORY_WINDOWS.len() as isize) as usize;
                cfg.history_window_secs = HISTORY_WINDOWS[next];
            }
            10 => cfg.show_grid = !cfg.show_grid,
            11 => cfg.show_axes = !cfg.show_axes,
            12 => {
                let current = YScale::ALL.iter().position(|s| *s == cfg.y_scale).unwrap_or(0) as isize;
                let next = (current + delta).rem_euclid(YScale::ALL.len() as isize) as usize;
                cfg.y_scale = YScale::ALL[next];
            }
            13 => {
                const MAXES: [f64; 9] = [10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 10000.0];
                let current = MAXES.iter().position(|m| *m == cfg.manual_max).unwrap_or(3) as isize;
                let next = (current + delta).rem_euclid(MAXES.len() as isize) as usize;
                cfg.manual_max = MAXES[next];
            }
            14 => {
                let current = LINE_THICKNESSES.iter().position(|t| *t == cfg.line_thickness).unwrap_or(0) as isize;
                let next = (current + delta).rem_euclid(LINE_THICKNESSES.len() as isize) as usize;
                cfg.line_thickness = LINE_THICKNESSES[next];
            }
            _ => {}
        }
        self.save();
    }

    fn commit_text_input(&mut self, purpose: &InputPurpose, value: &str) {
        let value = value.trim();
        match purpose {
            InputPurpose::NewLayout if !value.is_empty() => {
                if self.config.layouts.iter().any(|l| l.name == value) {
                    self.set_status("A layout with that name already exists");
                    return;
                }
                let mut layout = LayoutConfig::new(value, self.active_layout().columns, self.active_layout().rows);
                layout.widgets = self.active_layout().widgets.clone();
                self.config.layouts.push(layout);
                self.config.active_layout = value.to_string();
                self.selected = 0;
                self.set_status(format!("Layout {value} created"));
                self.save();
            }
            InputPurpose::RenameLayout if !value.is_empty() => {
                let old = self.config.active_layout.clone();
                if self.config.layouts.iter().any(|l| l.name == value && l.name != old) {
                    self.set_status("A layout with that name already exists");
                    return;
                }
                if let Some(layout) = self.config.layouts.iter_mut().find(|l| l.name == old) {
                    layout.name = value.to_string();
                }
                self.config.active_layout = value.to_string();
                self.set_status("Layout renamed");
                self.save();
            }
            InputPurpose::Accent | InputPurpose::AccentAlt => {
                if crate::util::parse_hex_color(value).is_some() {
                    let key = if matches!(purpose, InputPurpose::Accent) {
                        "accent"
                    } else {
                        "accent_alt"
                    };
                    self.config.color_overrides.insert(key.to_string(), format!("#{}", value.trim_start_matches('#')));
                    self.resolved = self.themes.resolve(&self.config.theme, &self.config.color_overrides);
                    self.set_status("Accent colour updated");
                    self.save();
                } else {
                    self.set_status("Invalid colour: use #RRGGBB");
                }
            }
            InputPurpose::Threshold(level) => {
                if let Ok(value) = value.parse::<f64>() {
                    if value.is_finite() && value >= 0.0 {
                        let metric = self.current_settings_metric();
                        let cfg = &mut self.config.dynamic_colors;
                        let mut one = cfg.metric(metric);
                        one.thresholds[(*level).min(2)] = value;
                        cfg.set_metric(metric, one.normalized());
                        self.set_status("Threshold updated");
                        self.save();
                    } else {
                        self.set_status("Invalid threshold");
                    }
                } else {
                    self.set_status("Invalid threshold");
                }
            }
            InputPurpose::ColorValue(level) => {
                let token = value.to_lowercase();
                let valid_token = [
                    "foreground", "text", "muted", "success", "warning", "danger", "accent",
                    "accent_alt", "memory", "cpu", "gpu", "orange",
                ]
                .contains(&token.as_str());
                let hex = if value.starts_with('#') {
                    Some(value.to_string())
                } else if value.len() == 6 && value.chars().all(|c| c.is_ascii_hexdigit()) {
                    Some(format!("#{value}"))
                } else {
                    None
                };
                if let Some(spec) = hex.or_else(|| valid_token.then(|| token.clone())) {
                    let metric = self.current_settings_metric();
                    let cfg = &mut self.config.dynamic_colors;
                    let mut one = cfg.metric(metric);
                    one.colors[(*level).min(3)] = spec;
                    cfg.set_metric(metric, one);
                    self.set_status("Threshold colour updated");
                    self.save();
                } else {
                    self.set_status("Invalid colour: use #RRGGBB or a theme token");
                }
            }
            _ => {}
        }
    }
}
