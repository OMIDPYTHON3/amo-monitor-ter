//! Real-time chart engine with eight professional terminal visualisation styles.
//!
//! Every style renders the actual samples stored in the rolling history. Pixel
//! grids are rasterised with Bresenham segments and mapped onto braille or
//! half-block glyphs, so lines stay continuous at any widget size.

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::config::theme::ResolvedTheme;
use crate::config::{ChartType, YScale};

/// Braille dot bits indexed by `row * 2 + column` inside a 2x4 cell.
const BRAILLE_BITS: [u16; 8] = [0x01, 0x08, 0x02, 0x10, 0x04, 0x20, 0x40, 0x80];

/// Default rolling window (seconds) used by callers that do not specify one.
pub const CHART_WINDOW_SECS: f64 = 30.0;

#[derive(Clone, Copy)]
pub struct Series<'a> {
    /// Chronological `(seconds, value)` samples.
    pub points: &'a [(f64, f64)],
    pub color: Color,
    /// Optional per-series maximum, used when mixing units in a Multi-Line chart.
    pub scale: Option<f64>,
    pub label: &'a str,
}

impl<'a> Series<'a> {
    pub fn new(points: &'a [(f64, f64)], color: Color) -> Self {
        Self {
            points,
            color,
            scale: None,
            label: "",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChartOptions {
    pub style: ChartType,
    pub window_secs: f64,
    pub show_grid: bool,
    pub show_axes: bool,
    pub y_scale: YScale,
    pub manual_max: f64,
    pub thickness: u8,
}

impl ChartOptions {
    pub fn thickness(&self) -> u8 {
        if (1..=3).contains(&self.thickness) {
            self.thickness
        } else {
            1
        }
    }
}

impl Default for ChartOptions {
    fn default() -> Self {
        Self {
            style: ChartType::BrailleLine,
            window_secs: CHART_WINDOW_SECS,
            show_grid: true,
            show_axes: false,
            y_scale: YScale::Auto,
            manual_max: 100.0,
            thickness: 1,
        }
    }
}

/// Blend two RGB colours; `t = 0` returns `a`, `t = 1` returns `b`.
fn blend(a: Color, b: Color, t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => Color::Rgb(
            (ar as f64 * (1.0 - t) + br as f64 * t).round() as u8,
            (ag as f64 * (1.0 - t) + bg as f64 * t).round() as u8,
            (ab as f64 * (1.0 - t) + bb as f64 * t).round() as u8,
        ),
        _ => a,
    }
}

fn clear_cell(cell: &mut Cell, theme: &ResolvedTheme) {
    cell.set_char(' ')
        .set_style(Style::default().fg(theme.chart_grid).bg(theme.panel));
}

/// A tiny raster grid that packs sub-pixels into braille (2x4) or half-block
/// (1x2) glyphs. All line styles share this primitive.
struct PixelGrid {
    width: usize,
    height: usize,
    x_factor: usize,
    y_factor: usize,
    braille: bool,
    bits: Vec<u16>,
    colors: Vec<Color>,
}

impl PixelGrid {
    fn new(width: usize, height: usize, x_factor: usize, y_factor: usize, braille: bool) -> Self {
        Self {
            width,
            height,
            x_factor,
            y_factor,
            braille,
            bits: vec![0; width * height],
            colors: vec![Color::Reset; width * height],
        }
    }

    fn pixels_x(&self) -> usize {
        self.width * self.x_factor
    }

    fn pixels_y(&self) -> usize {
        self.height * self.y_factor
    }

    fn plot(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.pixels_x() || y >= self.pixels_y() {
            return;
        }
        let index = (y % self.y_factor) * self.x_factor + (x % self.x_factor);
        let bit = if self.braille {
            BRAILLE_BITS[index.min(7)]
        } else {
            1 << index.min(15)
        };
        let cell = (y / self.y_factor) * self.width + (x / self.x_factor);
        self.bits[cell] |= bit;
        self.colors[cell] = color;
    }

    /// Rasterise a connected segment; `thickness` widens it vertically.
    fn line(
        &mut self,
        (x0, y0): (usize, usize),
        (x1, y1): (usize, usize),
        color: Color,
        thickness: u8,
    ) {
        let mut x = x0 as isize;
        let mut y = y0 as isize;
        let tx = x1 as isize;
        let ty = y1 as isize;
        let dx = (tx - x).abs();
        let dy = (ty - y).abs();
        let sx = if x < tx { 1 } else { -1 };
        let sy = if y < ty { 1 } else { -1 };
        let mut error = dx - dy;
        let half = (thickness / 2) as isize;
        loop {
            for offset in -half..=half {
                self.plot(x as usize, (y + offset).max(0) as usize, color);
            }
            if x == tx && y == ty {
                break;
            }
            let doubled = 2 * error;
            if doubled > -dy {
                error -= dy;
                x += sx;
            }
            if doubled < dx {
                error += dx;
                y += sy;
            }
        }
    }

    fn glyph(&self, bits: u16) -> char {
        if self.braille {
            char::from_u32(0x2800 + bits as u32).unwrap_or('⠿')
        } else {
            match bits & 0b11 {
                0b01 => '▀',
                0b10 => '▄',
                0b11 => '█',
                _ => ' ',
            }
        }
    }

    fn paint(&self, buf: &mut Buffer, area: Rect, theme: &ResolvedTheme) {
        for row in 0..self.height {
            for col in 0..self.width {
                let idx = row * self.width + col;
                let Some(cell) = buf.cell_mut((area.x + col as u16, area.y + row as u16)) else {
                    continue;
                };
                if self.bits[idx] == 0 {
                    // Leave any grid/background already painted in place.
                    continue;
                }
                let glyph = self.glyph(self.bits[idx]);
                if glyph == ' ' {
                    clear_cell(cell, theme);
                } else {
                    cell.set_char(glyph).set_style(
                        Style::default().fg(self.colors[idx]).bg(theme.panel),
                    );
                }
            }
        }
    }
}

fn draw_grid(buf: &mut Buffer, area: Rect, theme: &ResolvedTheme) {
    if area.width < 4 || area.height < 3 {
        return;
    }
    // Whisper-quiet dotted guides. Only two of each so charts stay legible.
    let style = Style::default().fg(theme.chart_grid);
    for fraction in [1, 2] {
        let y = area.y + (area.height * fraction / 3).min(area.height - 1);
        for x in (area.x..area.right()).step_by(2) {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char('·').set_style(style);
            }
        }
    }
    for fraction in [1, 2] {
        let x = area.x + (area.width * fraction / 3).min(area.width - 1);
        for y in area.y..area.bottom() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char('┊').set_style(style);
            }
        }
    }
}

fn draw_axes(
    buf: &mut Buffer,
    area: Rect,
    plot: Rect,
    ceiling: f64,
    window: f64,
    theme: &ResolvedTheme,
) {
    let style = Style::default().fg(theme.muted);
    let top = if ceiling >= 10.0 {
        format!("{:>4}", ceiling.round() as i64)
    } else {
        format!("{:>4}", ceiling)
    };
    buf.set_string(area.x, plot.y, top, style);
    buf.set_string(area.x, plot.bottom().saturating_sub(1), "   0", style);
    let label = format!("-{}s", window.round() as i64);
    buf.set_string(plot.x, area.bottom() - 1, label, style);
    buf.set_string(
        plot.right().saturating_sub(3),
        area.bottom() - 1,
        "now",
        style,
    );
}

/// Linear interpolation of a series value at time `t`.
fn interpolate(points: &[(f64, f64)], t: f64) -> Option<f64> {
    let n = points.len();
    if n == 0 {
        return None;
    }
    if t <= points[0].0 {
        return Some(points[0].1);
    }
    if t >= points[n - 1].0 {
        return Some(points[n - 1].1);
    }
    for pair in points.windows(2) {
        if t >= pair[0].0 && t <= pair[1].0 {
            let span = pair[1].0 - pair[0].0;
            if span <= 0.0 {
                return Some(pair[1].1);
            }
            let ratio = (t - pair[0].0) / span;
            return Some(pair[0].1 + (pair[1].1 - pair[0].1) * ratio);
        }
    }
    None
}

/// Map a series' samples to plot pixels for the requested window.
fn map_points(
    points: &[(f64, f64)],
    t_start: f64,
    window: f64,
    ceiling: f64,
    series_scale: Option<f64>,
    pixels_x: usize,
    pixels_y: usize,
) -> Vec<Option<(usize, usize)>> {
    let n = points.len();
    let mut mapped = Vec::with_capacity(n);
    for (i, &(t, v)) in points.iter().enumerate() {
        if !t.is_finite() || !v.is_finite() || v < 0.0 || t < t_start {
            mapped.push(None);
            continue;
        }
        let x = (ratio_x(t, t_start, window, n, i) * (pixels_x - 1) as f64).round() as usize;
        let scale = series_scale.unwrap_or(ceiling).max(1e-9);
        let ratio = (v / scale).clamp(0.0, 1.0);
        let y = ((1.0 - ratio) * (pixels_y - 1) as f64).round() as usize;
        mapped.push(Some((x.min(pixels_x - 1), y.min(pixels_y - 1))));
    }
    mapped
}

fn ratio_x(t: f64, t_start: f64, window: f64, n: usize, i: usize) -> f64 {
    if window >= 0.5 {
        ((t - t_start) / window).clamp(0.0, 1.0)
    } else if n <= 1 {
        0.5
    } else {
        i as f64 / (n - 1) as f64
    }
}

/// Draw connected segments, breaking the line when a sensor gap is detected.
fn connect(
    grid: &mut PixelGrid,
    mapped: &[Option<(usize, usize)>],
    points: &[(f64, f64)],
    color: Color,
    thickness: u8,
    stepped: bool,
) {
    for i in 1..mapped.len() {
        let (Some(a), Some(b)) = (mapped[i - 1], mapped[i]) else {
            continue;
        };
        let gap = points[i].0 - points[i - 1].0;
        if gap > 3.0 {
            // No data was reported during this period; keep the points but do
            // not fabricate a line through the gap.
            grid.plot(a.0, a.1, color);
            grid.plot(b.0, b.1, color);
            continue;
        }
        if stepped {
            grid.line(a, (b.0, a.1), color, thickness);
            grid.line((b.0, a.1), b, color, thickness);
        } else {
            grid.line(a, b, color, thickness);
        }
    }
    if mapped.len() == 1 {
        if let Some((x, y)) = mapped[0] {
            grid.plot(x, y, color);
            grid.plot(x.saturating_sub(1), y, color);
            grid.plot((x + 1).min(grid.pixels_x() - 1), y, color);
        }
    }
}

/// Fill the cells below a line with a shade pattern (Area / Gradient Area).
fn fill_below(
    buf: &mut Buffer,
    plot: Rect,
    mapped: &[Option<(usize, usize)>],
    color: Color,
    gradient: bool,
    theme: &ResolvedTheme,
) {
    let height = plot.height as usize;
    let width = plot.width as usize;
    let mut top_per_col = vec![height; width];
    for point in mapped.iter().flatten() {
        let col = (point.0 / 2).min(width - 1);
        let row = point.1 / 4;
        top_per_col[col] = top_per_col[col].min(row);
    }
    for col in 0..width {
        let top = top_per_col[col];
        if top >= height {
            continue;
        }
        for row in (top + 1)..height {
            let depth = (row - top) as f64 / (height - top).max(1) as f64;
            let (ch, shade) = if gradient {
                let index = ((1.0 - depth) * 4.0).round() as usize;
                (['░', '▒', '▓', '█'][index.min(3)], blend(color, theme.background, depth * 0.75))
            } else {
                ('░', color)
            };
            if let Some(cell) = buf.cell_mut((plot.x + col as u16, plot.y + row as u16)) {
                cell.set_char(ch)
                    .set_style(Style::default().fg(shade).bg(theme.panel));
            }
        }
    }
}

fn render_sparkline(
    buf: &mut Buffer,
    plot: Rect,
    series: &[Series<'_>],
    t_start: f64,
    window: f64,
    ceiling: f64,
    theme: &ResolvedTheme,
) {
    const STEPS: [char; 9] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█', '█'];
    for (index, line) in series.iter().enumerate() {
        let y = plot.y + index as u16;
        if y >= plot.bottom() {
            break;
        }
        for col in 0..plot.width {
            let t = t_start + (col as f64 + 0.5) / plot.width as f64 * window;
            let Some(value) = interpolate(line.points, t) else {
                continue;
            };
            let scale = line.scale.unwrap_or(ceiling).max(1e-9);
            let step = ((value / scale).clamp(0.0, 1.0) * 8.0).round() as usize;
            if let Some(cell) = buf.cell_mut((plot.x + col as u16, y)) {
                cell.set_char(STEPS[step])
                    .set_style(Style::default().fg(line.color).bg(theme.panel));
            }
        }
    }
}

fn render_histogram(
    buf: &mut Buffer,
    plot: Rect,
    series: &[Series<'_>],
    t_start: f64,
    ceiling: f64,
    theme: &ResolvedTheme,
) {
    if plot.height == 0 || plot.width == 0 || series.is_empty() {
        return;
    }
    let bins = (plot.width as usize / 2).clamp(1, 16);
    let group_width = plot.width as usize / bins;
    if group_width == 0 {
        return;
    }
    let mut counts = vec![0u32; bins * series.len()];
    for (s, line) in series.iter().enumerate() {
        for &(t, v) in line.points {
            if t < t_start || !v.is_finite() || v < 0.0 {
                continue;
            }
            let scale = line.scale.unwrap_or(ceiling).max(1e-9);
            let ratio = (v / scale).clamp(0.0, 1.0);
            let bin = (ratio * bins as f64) as usize;
            counts[s * bins + bin.min(bins - 1)] += 1;
        }
    }
    let peak = counts.iter().copied().max().unwrap_or(1).max(1);
    for bin in 0..bins {
        for (s, line) in series.iter().enumerate() {
            let count = counts[s * bins + bin];
            let height = (count as f64 / peak as f64 * plot.height as f64).round() as u16;
            let x = plot.x + (bin * group_width + s) as u16;
            if x >= plot.right() {
                continue;
            }
            for row in 0..height {
                let y = plot.bottom() - 1 - row;
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char('█')
                        .set_style(Style::default().fg(line.color).bg(theme.panel));
                }
            }
        }
    }
}

/// Render a chart of real monitoring data in the requested style.
pub fn render_chart(
    buf: &mut Buffer,
    area: Rect,
    series: &[Series<'_>],
    options: &ChartOptions,
    theme: &ResolvedTheme,
) {
    if area.width == 0 || area.height == 0 || series.is_empty() {
        return;
    }

    // Reserve space for axis labels when requested.
    let mut plot = area;
    if options.show_axes && area.width > 10 && area.height > 2 {
        plot.x = area.x + 5;
        plot.width = area.width.saturating_sub(5);
        plot.height = area.height.saturating_sub(1);
    }
    if plot.width == 0 || plot.height == 0 {
        return;
    }

    // Clear the plot and paint an optional grid underneath the data.
    for y in plot.y..plot.bottom() {
        for x in plot.x..plot.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                clear_cell(cell, theme);
            }
        }
    }
    if options.show_grid {
        draw_grid(buf, plot, theme);
    }

    let configured_window = options.window_secs.max(0.5);
    let mut t_earliest = f64::INFINITY;
    let mut t_latest = f64::NEG_INFINITY;
    for t in series
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p.0))
        .filter(|t| t.is_finite())
    {
        t_earliest = t_earliest.min(t);
        t_latest = t_latest.max(t);
    }
    if !t_latest.is_finite() {
        return; // no sensor data yet — leave an empty grid
    }
    // Before the full window has elapsed, stretch the available samples across
    // the chart so it always uses the space it is given. Once the configured
    // window is filled, the view scrolls with real timestamps.
    let data_span = t_latest - t_earliest;
    let window = if data_span > 0.5 && data_span < configured_window {
        data_span
    } else {
        configured_window
    };
    let t_start = t_latest - window;

    // Dynamic or manual vertical scale, shared by every series in the chart.
    let data_max = series
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p.1))
        .filter(|v| v.is_finite() && *v > 0.0)
        .fold(0.0_f64, f64::max);
    let ceiling = match options.y_scale {
        YScale::Manual => options.manual_max.max(1e-9),
        YScale::Auto => nice_ceiling(data_max).max(1.0),
    };

    if options.show_axes && area.width > 10 && area.height > 2 {
        draw_axes(buf, area, plot, ceiling, window, theme);
    }

    match options.style {
        ChartType::Sparkline => {
            render_sparkline(buf, plot, series, t_start, window, ceiling, theme);
            return;
        }
        ChartType::Histogram => {
            render_histogram(buf, plot, series, t_start, ceiling, theme);
            return;
        }
        _ => {}
    }

    let braille = options.style != ChartType::Line;
    let (xf, yf) = if braille { (2, 4) } else { (1, 2) };
    let mut grid = PixelGrid::new(plot.width as usize, plot.height as usize, xf, yf, braille);
    let thickness = if braille { options.thickness() } else { 1 };

    for line in series {
        let mapped = map_points(
            line.points,
            t_start,
            window,
            ceiling,
            line.scale,
            grid.pixels_x(),
            grid.pixels_y(),
        );

        match options.style {
            ChartType::Area | ChartType::GradientArea => {
                fill_below(
                    buf,
                    plot,
                    &mapped,
                    line.color,
                    options.style == ChartType::GradientArea,
                    theme,
                );
                // Re-map at braille resolution for the outline.
                let outline = map_points(
                    line.points,
                    t_start,
                    window,
                    ceiling,
                    line.scale,
                    grid.pixels_x(),
                    grid.pixels_y(),
                );
                connect(&mut grid, &outline, line.points, line.color, thickness, false);
            }
            ChartType::Step => connect(&mut grid, &mapped, line.points, line.color, thickness, true),
            _ => connect(&mut grid, &mapped, line.points, line.color, thickness, false),
        }
    }

    grid.paint(buf, plot, theme);
}

/// Backwards-compatible helper: a high-resolution braille line chart.
pub fn render_line_chart(
    buf: &mut Buffer,
    area: Rect,
    series: &[Series<'_>],
    y_floor: Option<f64>,
    theme: &ResolvedTheme,
) {
    let mut options = ChartOptions::default();
    options.style = ChartType::BrailleLine;
    if let Some(floor) = y_floor {
        options.y_scale = YScale::Manual;
        options.manual_max = floor;
    }
    render_chart(buf, area, series, &options, theme);
}

/// Round a value up to a "nice" chart ceiling (1, 2, 5 x 10^n).
pub fn nice_ceiling(value: f64) -> f64 {
    if !value.is_finite() || value <= 0.0 {
        return 1.0;
    }
    let exponent = value.log10().floor();
    let magnitude = 10f64.powf(exponent);
    let normalized = value / magnitude;
    let step = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    step * magnitude
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_ceiling_rounds_up() {
        assert_eq!(nice_ceiling(0.0), 1.0);
        assert_eq!(nice_ceiling(1.5), 2.0);
        assert_eq!(nice_ceiling(250.0), 500.0);
    }

    #[test]
    fn braille_bits_cover_all_dots() {
        assert_eq!(BRAILLE_BITS.iter().fold(0, |a, b| a | b), 0xFF);
    }

    #[test]
    fn pixel_grid_connects_every_segment() {
        let mut grid = PixelGrid::new(20, 4, 2, 4, true);
        grid.line((0, 0), (39, 15), Color::Cyan, 1);
        let dots: u32 = grid.bits.iter().map(|b| b.count_ones()).sum();
        assert!(dots >= 40, "expected a continuous line, got {dots} dots");
    }

    #[test]
    fn zero_values_are_drawn_at_the_bottom() {
        let theme = crate::config::theme::ThemeSet::builtin().resolve("Dark", &Default::default());
        let points = vec![(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)];
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 4));
        render_chart(
            &mut buffer,
            Rect::new(0, 0, 20, 4),
            &[Series::new(&points, Color::Cyan)],
            &ChartOptions::default(),
            &theme,
        );
        let bottom = (0..20).any(|x| {
            buffer[(x, 3)]
                .symbol()
                .chars()
                .next()
                .is_some_and(|c| (0x2800..=0x28ff).contains(&(c as u32)))
        });
        assert!(bottom);
    }
}
