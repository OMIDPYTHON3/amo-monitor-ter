# AMO Monitor

A modern, fully customizable terminal system monitor built with Rust, Ratatui and Crossterm.
It uses real system sensors only — unavailable values are shown as `N/A`, never invented.

## Features

### Monitoring
- **CPU** — usage (total + per-core), frequency, package power (Intel/AMD RAPL where available) and temperature.
- **GPU** — usage, frequency, temperature and power. Sources: `nvidia-smi`, AMD/Intel DRM `gpu_busy_percent` + hwmon, and the Broadcom V3D `gpu_stats` busy-time counter on Raspberry Pi.
- **RAM / Swap** — used bytes, total bytes and percentage.
- **Network** — upload/download speed in bytes per second (loopback and virtual interfaces excluded).
- **Disk I/O** — read/write speed from `/proc/diskstats` (whole disks only, no partition double counting).
- **System** — uptime, hostname, kernel and CPU model.

### Charts
Eight real chart styles, selectable **independently for every widget** in `c` (Configure):

1. **Line** — connected half-block line chart
2. **Braille Line** — high-resolution braille line chart
3. **Area** — filled area below the line
4. **Gradient Area** — shaded gradient fill using terminal colours
5. **Multi-Line** — several real series on one graph (CPU/GPU: usage, frequency, power, temperature; Network: down/up; Disk: read/write)
6. **Step** — staircase rendering of historical samples
7. **Sparkline** — compact inline trends
8. **Histogram** — distribution of the values in the selected window

- Per-widget history windows: **30 s / 1 min / 5 min** (a 5-minute buffer is always retained).
- Per-widget **grid lines**, **axes & labels**, **auto/manual Y scaling** and **line thickness** (1–3).
- Consecutive samples are joined with rasterised segments; X is positioned by timestamp and spans the full chart width while a window fills.
- Network and Disk I/O plot two overlaid series with distinct theme colours; missing sensor data breaks the line instead of inventing values.
- Every widget can enable or disable its chart independently. All chart preferences persist in `config/config.json`.

### Compact responsive dashboard
Three stable states, defined by two breakpoints (`src/responsive.rs`):
- **Normal** (≥120×30) — full panels, detailed values and large charts with axes/labels.
- **Compact** (≥80×20) — same grid structure and widget order, reduced spacing and simplified details; charts are kept only in widgets with enough room.
- **Minimal** (<80 wide or <20 rows) — essential metrics only in compact text rows; all charts hidden.

Charts are never auto-converted to sparklines: they are shown only when the widget is large enough and hidden otherwise. Grid columns/rows never change on small resize steps, so the layout stays predictable. Charts, axes, detail levels and saved settings return unchanged when space is restored — responsive decisions are display-only.

### Customizable dashboard
Three dashboard styles, switched instantly with `v`:
- **Grid Dashboard** — independent monitoring panels.
- **Linear Dashboard** — compact metric rows that wrap with the terminal width (`y` edits metrics: pin, reorder, group, add/remove, optional inline charts).
- **Mixed Dashboard** — a compact metric strip above grid panels.

Collision-aware positioning (`layout.rs`):
- Widgets can never overlap. Moving onto an occupied area displaces nearby widgets into the closest free space, preserving their sizes.
- Pinned widgets stay put; a blocked move shows a `BLOCKED` outline.
- Move/resize arrows show a live `DROP` / `MOVED` preview; `Enter` confirms and `Esc` cancels.
- `Ctrl-Z` undoes the last layout change. Layouts are normalised on load and stay collision-free at every terminal size.
- Layouts, widget settings, theme and refresh rate are saved to `config/config.json` and restored on restart.

### Dynamic metric colours
Every metric's **value text** changes colour from its current reading:
- Defaults: **0–59 % white, 60–74 % yellow, 75–89 % orange, 90–100 % red**.
- Unit-aware thresholds for CPU/GPU power (W), temperature (°C), frequency (MHz), network and disk rates (B/s) — percentage rules are never applied to those metrics.
- Global on/off switch plus an independent on/off switch for each of the 14 metrics.
- Configurable thresholds and colours per metric, five predefined schemes, reset-to-defaults and a live preview.
- Settings → **Colors** (`s`, then `Tab` three times); all preferences persist. Works identically in Grid, Linear and Mixed layouts.

### Widget settings (per widget)
- Show/hide primary value, frequency, power, temperature and extra details.
- Enable/disable the chart and choose any of the eight chart styles.
- Display mode: **Compact / Medium / Detailed**.
- View mode: **Numbers / Numbers + Charts / Charts**.
- Chart style, history window, grid, axes, Y scaling and line width.

### Themes
Twelve complete themes with instant switching and live previews in the theme picker (`t`):
Dark, Light, Dracula, Nord, Tokyo Night, Catppuccin, Monokai, **Gruvbox Dark, Solarized Dark, Ayu Dark, Everforest and Rose Pine**.
Every theme lives in its own editable TOML file under `config/themes/`.
Chart colours follow the selected theme automatically. Legacy `config/themes.json`
overrides and two custom accent colours remain supported via **Settings → General**.

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| `Tab` / arrows | Select widget |
| `Space` | Pin / unpin selected widget |
| `m` / `r` then arrows | Move / resize with live preview (`Enter` confirms, `Esc` cancels) |
| `Ctrl-Z` | Undo last layout change |
| `v` | Cycle Grid / Linear / Mixed dashboard |
| `y` | Edit linear metrics |
| `c` | Configure selected widget |
| `a` | Add widget |
| `d` / `Delete` | Remove widget |
| `s` | Settings (General / Widgets / Layouts / Colors) |
| `t` | Theme menu |
| `l` | Layout manager (create / rename / delete / activate) |
| `1`–`9` | Quick-switch layout |
| `F5` | Save configuration |
| `?` | Help |
| `q` / `Ctrl-C` | Quit |

## Build & test

```bash
./build.sh          # optimized release binary -> dist/amo-monitor
cargo test          # unit + integration tests (monitoring, charts, config, layouts)
```

The executable can also be run directly from `target/release/amo-monitor`. `--version` prints the version.

## Project structure

```
src/
  main.rs           terminal lifecycle and event loop
  lib.rs            library crate (used by integration tests)
  app.rs            state machine, input handling, undo and previews
  colors.rs         dynamic metric text-colour system
  layout.rs         collision-aware grid layout engine
  responsive.rs     terminal-size breakpoints and adaptive presentation
  monitor/
    mod.rs          snapshots, sampling, rolling history
    linux.rs        /proc and sysfs parsers
    gpu.rs          NVIDIA / DRM / V3D GPU detection
  config/
    mod.rs          layouts, widget settings, persistence
    theme.rs        theme palettes and colour resolution
  chart/mod.rs      braille line-chart renderer
  ui/
    mod.rs          header, footer, render root
    dashboard.rs    grid rendering and move/resize previews
    linear.rs       linear and mixed dashboard rendering
    widgets.rs      per-widget rendering
    overlays.rs     menus, settings and help
tests/              integration tests
config/             runtime configuration and themes (created in this directory)
dist/               final executable
```

## Platform notes

- Designed for Linux on `x86_64` and `ARM64` (including Raspberry Pi). All monitoring is done through
  `/proc`, sysfs and optional `nvidia-smi`; no native bindings are required.
- CPU package power requires kernel RAPL support. Most ARM boards do not expose it, so it reads `N/A`.
- GPU telemetry depends on the driver exposing usage counters; unsupported GPUs show `N/A` for those fields.
- The application never fabricates sensor data: missing or unreadable sensors are represented as `None`
  and rendered as `N/A`.
