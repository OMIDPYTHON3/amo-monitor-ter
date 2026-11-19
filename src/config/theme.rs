//! Theme definitions and colour resolution.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::util::parse_hex_color;

/// Every named colour used by the UI.
pub const COLOR_KEYS: [&str; 23] = [
    "background",
    "foreground",
    "panel",
    "panel_alt",
    "border",
    "border_focused",
    "accent",
    "accent_alt",
    "text",
    "muted",
    "success",
    "warning",
    "danger",
    "cpu",
    "gpu",
    "memory",
    "swap",
    "net_down",
    "net_up",
    "disk_read",
    "disk_write",
    "chart_grid",
    "selection",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub colors: BTreeMap<String, String>,
}

impl Theme {
    fn new(name: &str, values: [&str; COLOR_KEYS.len()]) -> Self {
        let colors = COLOR_KEYS
            .iter()
            .zip(values.iter())
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        Self {
            name: name.to_string(),
            colors,
        }
    }
}

/// Concrete colours ready for rendering.
#[derive(Clone, Debug)]
pub struct ResolvedTheme {
    pub name: String,
    pub background: Color,
    pub foreground: Color,
    pub panel: Color,
    pub panel_alt: Color,
    pub border: Color,
    pub border_focused: Color,
    pub accent: Color,
    pub accent_alt: Color,
    pub text: Color,
    pub muted: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub cpu: Color,
    pub gpu: Color,
    pub memory: Color,
    pub swap: Color,
    pub net_down: Color,
    pub net_up: Color,
    pub disk_read: Color,
    pub disk_write: Color,
    pub chart_grid: Color,
    pub selection: Color,
}

impl ResolvedTheme {
    pub fn metric_color(&self, key: crate::monitor::SeriesKey) -> Color {
        use crate::monitor::SeriesKey::*;
        match key {
            CpuUsage | CpuFrequency | CpuTemperature | CpuPower => self.cpu,
            GpuUsage | GpuFrequency | GpuTemperature | GpuPower => self.gpu,
            MemoryUsed => self.memory,
            SwapUsed => self.swap,
            NetworkRx => self.net_down,
            NetworkTx => self.net_up,
            DiskRead => self.disk_read,
            DiskWrite => self.disk_write,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ThemesFile {
    themes: Vec<Theme>,
}

/// All available themes, including user-defined ones loaded from disk.
#[derive(Clone, Debug)]
pub struct ThemeSet {
    themes: Vec<Theme>,
}

impl ThemeSet {
    pub fn builtin() -> Self {
        Self {
            themes: vec![
                // name, background, foreground, panel, panel_alt, border,
                // border_focused, accent, accent_alt, text, muted, success,
                // warning, danger, cpu, gpu, memory, swap, net_down, net_up,
                // disk_read, disk_write, chart_grid, selection
                Theme::new("Dark", [
                    "#0B0E14", "#D7DAE0", "#121722", "#19202E", "#2A3446", "#5B8DEF",
                    "#5B8DEF", "#A06BFF", "#E6E9EF", "#7A869A", "#3DDC97", "#F7CA88",
                    "#FF6B6B", "#5B8DEF", "#A06BFF", "#45C2E3", "#F7CA88", "#3DDC97",
                    "#F78FB3", "#45C2E3", "#F7CA88", "#1C2534", "#233354",
                ]),
                Theme::new("Light", [
                    "#F4F6FB", "#1B2431", "#FFFFFF", "#EDF1F8", "#CBD5E1", "#2563EB",
                    "#2563EB", "#7C3AED", "#162033", "#5B6B82", "#059669", "#B45309",
                    "#DC2626", "#2563EB", "#7C3AED", "#0891B2", "#B45309", "#059669",
                    "#DB2777", "#0891B2", "#B45309", "#DCE3EE", "#DCE7FF",
                ]),
                Theme::new("Dracula", [
                    "#282A36", "#F8F8F2", "#21222C", "#343746", "#44475A", "#BD93F9",
                    "#BD93F9", "#FF79C6", "#F8F8F2", "#6272A4", "#50FA7B", "#F1FA8C",
                    "#FF5555", "#BD93F9", "#FF79C6", "#8BE9FD", "#F1FA8C", "#50FA7B",
                    "#FF79C6", "#8BE9FD", "#F1FA8C", "#3A3D4D", "#44475A",
                ]),
                Theme::new("Nord", [
                    "#2E3440", "#D8DEE9", "#3B4252", "#434C5E", "#4C566A", "#88C0D0",
                    "#88C0D0", "#B48EAD", "#ECEFF4", "#7B88A1", "#A3BE8C", "#EBCB8B",
                    "#BF616A", "#88C0D0", "#B48EAD", "#81A1C1", "#EBCB8B", "#A3BE8C",
                    "#BF616A", "#81A1C1", "#EBCB8B", "#4C566A", "#434C5E",
                ]),
                Theme::new("Tokyo Night", [
                    "#1A1B26", "#C0CAF5", "#1F2335", "#24283B", "#3B4261", "#7AA2F7",
                    "#7AA2F7", "#BB9AF7", "#C0CAF5", "#565F89", "#9ECE6A", "#E0AF68",
                    "#F7768E", "#7AA2F7", "#BB9AF7", "#7DCFFF", "#E0AF68", "#9ECE6A",
                    "#F7768E", "#7DCFFF", "#E0AF68", "#292E42", "#354A7C",
                ]),
                Theme::new("Catppuccin", [
                    "#1E1E2E", "#CDD6F4", "#181825", "#313244", "#45475A", "#89B4FA",
                    "#89B4FA", "#CBA6F7", "#CDD6F4", "#6C7086", "#A6E3A1", "#F9E2AF",
                    "#F38BA8", "#89B4FA", "#CBA6F7", "#94E2D5", "#F9E2AF", "#A6E3A1",
                    "#F38BA8", "#94E2D5", "#F9E2AF", "#313244", "#45475A",
                ]),
                Theme::new("Monokai", [
                    "#272822", "#F8F8F2", "#1E1F1C", "#34352F", "#49483E", "#A6E22E",
                    "#A6E22E", "#AE81FF", "#F8F8F2", "#75715E", "#A6E22E", "#E6DB74",
                    "#F92672", "#A6E22E", "#AE81FF", "#66D9EF", "#E6DB74", "#A6E22E",
                    "#F92672", "#66D9EF", "#E6DB74", "#3B3C34", "#49483E",
                ]),
                Theme::new("Gruvbox Dark", [
                    "#282828", "#EBDBB2", "#32302F", "#3C3836", "#504945", "#FABD2F",
                    "#FABD2F", "#FE8019", "#EBDBB2", "#928374", "#B8BB26", "#FABD2F",
                    "#FB4934", "#83A598", "#D3869B", "#8EC07C", "#FABD2F", "#B8BB26",
                    "#FB4934", "#83A598", "#FE8019", "#504945", "#3C3836",
                ]),
                Theme::new("Solarized Dark", [
                    "#002B36", "#839496", "#073642", "#073642", "#586E75", "#268BD2",
                    "#268BD2", "#D33682", "#EEE8D5", "#657B83", "#859900", "#B58900",
                    "#DC322F", "#268BD2", "#D33682", "#2AA198", "#B58900", "#859900",
                    "#DC322F", "#2AA198", "#B58900", "#073642", "#073642",
                ]),
                Theme::new("Ayu Dark", [
                    "#0A0E14", "#B3B1AD", "#0E141C", "#131A23", "#252D3A", "#FFB454",
                    "#FFB454", "#F07178", "#E3E1DE", "#6B7A85", "#C2D94C", "#FFB454",
                    "#F07178", "#59C2FF", "#F07178", "#95E6CB", "#FFB454", "#C2D94C",
                    "#F07178", "#59C2FF", "#FFB454", "#252D3A", "#1D2833",
                ]),
                Theme::new("Everforest", [
                    "#2D353B", "#D3C6AA", "#343F44", "#3D484D", "#4F585E", "#A7C080",
                    "#A7C080", "#DBBC7F", "#D3C6AA", "#7A8478", "#A7C080", "#DBBC7F",
                    "#E67E80", "#7FBBB3", "#DBBC7F", "#83C092", "#E69875", "#A7C080",
                    "#E67E80", "#7FBBB3", "#E69875", "#4F585E", "#475258",
                ]),
                Theme::new("Rose Pine", [
                    "#191724", "#E0DEF4", "#1F1D2E", "#26233A", "#403D52", "#EBBCBA",
                    "#EBBCBA", "#C4A7E7", "#E0DEF4", "#6E6A86", "#9ECE6A", "#F6C177",
                    "#EB6F92", "#C4A7E7", "#EBBCBA", "#9CCFD8", "#F6C177", "#9CCFD8",
                    "#EB6F92", "#EBBCBA", "#F6C177", "#403D52", "#403D52",
                ]),
            ],
        }
    }

    /// Merge a theme into the set, preserving any previously loaded colours
    /// for keys the incoming theme does not define.
    fn merge_theme(set: &mut Self, theme: Theme) {
        if let Some(existing) = set.themes.iter_mut().find(|t| t.name == theme.name) {
            for (key, value) in theme.colors {
                existing.colors.insert(key, value);
            }
        } else {
            set.themes.push(theme);
        }
    }

    /// Load themes from disk.
    ///
    /// Precedence (low to high): compiled-in defaults, one TOML file per theme
    /// in `config/themes/`, then legacy `config/themes.json` overrides. This
    /// keeps existing themes and user customisations intact while allowing
    /// every shipped theme to be edited in its own file.
    pub fn load(dir: &Path) -> Self {
        let mut set = Self::builtin();

        let theme_dir = dir.join("themes");
        if let Ok(entries) = std::fs::read_dir(&theme_dir) {
            let mut files: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
                .collect();
            files.sort();
            for file in files {
                let Ok(text) = std::fs::read_to_string(&file) else {
                    continue;
                };
                let Ok(theme) = toml::from_str::<Theme>(&text) else {
                    continue;
                };
                Self::merge_theme(&mut set, theme);
            }
        }

        let path = dir.join("themes.json");
        if let Ok(file) = std::fs::read_to_string(&path) {
            if let Ok(file) = serde_json::from_str::<ThemesFile>(&file) {
                let defaults = Self::builtin();
                for theme in file.themes {
                    if let Some(existing) = set.themes.iter_mut().find(|t| t.name == theme.name) {
                        let default = defaults.get(&theme.name);
                        for (key, value) in theme.colors {
                            // Only real user customisations override TOML.
                            // Default-valued entries in an old auto-generated
                            // themes.json must not mask edits to a TOML file.
                            let matches_default = default
                                .and_then(|d| d.colors.get(&key))
                                .is_some_and(|v| *v == value);
                            if !matches_default {
                                existing.colors.insert(key, value);
                            }
                        }
                    } else {
                        Self::merge_theme(&mut set, theme);
                    }
                }
            }
        }
        set
    }

    /// Ensure every theme has its own editable TOML file and that the legacy
    /// override file exists. Existing files are never overwritten.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let theme_dir = dir.join("themes");
        std::fs::create_dir_all(&theme_dir)?;
        for theme in &self.themes {
            let safe: String = theme
                .name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let path = theme_dir.join(format!("{safe}.toml"));
            if path.exists() {
                continue;
            }
            let text = toml::to_string_pretty(theme)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            std::fs::write(path, text)?;
        }

        let json_path = dir.join("themes.json");
        if !json_path.exists() {
            let file = ThemesFile { themes: Vec::new() };
            let json = serde_json::to_string_pretty(&file)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            std::fs::write(json_path, json)?;
        }
        Ok(())
    }

    pub fn names(&self) -> Vec<String> {
        self.themes.iter().map(|t| t.name.clone()).collect()
    }

    pub fn get(&self, name: &str) -> Option<&Theme> {
        self.themes.iter().find(|t| t.name == name)
    }

    /// Resolve a theme to Ratatui colours, applying per-install overrides.
    pub fn resolve(&self, name: &str, overrides: &BTreeMap<String, String>) -> ResolvedTheme {
        let fallback = self.themes.first().expect("built-in themes exist");
        let theme = self.get(name).unwrap_or(fallback);
        let color = |key: &str, default: &str| -> Color {
            overrides
                .get(key)
                .or_else(|| theme.colors.get(key))
                .and_then(|s| parse_hex_color(s))
                .or_else(|| parse_hex_color(default))
                .unwrap_or(Color::Reset)
        };
        let d = |key: &str| theme.colors.get(key).map(String::as_str).unwrap_or("#888888");
        ResolvedTheme {
            name: theme.name.clone(),
            background: color("background", d("background")),
            foreground: color("foreground", d("foreground")),
            panel: color("panel", d("panel")),
            panel_alt: color("panel_alt", d("panel_alt")),
            border: color("border", d("border")),
            border_focused: color("border_focused", d("border_focused")),
            accent: color("accent", d("accent")),
            accent_alt: color("accent_alt", d("accent_alt")),
            text: color("text", d("text")),
            muted: color("muted", d("muted")),
            success: color("success", d("success")),
            warning: color("warning", d("warning")),
            danger: color("danger", d("danger")),
            cpu: color("cpu", d("cpu")),
            gpu: color("gpu", d("gpu")),
            memory: color("memory", d("memory")),
            swap: color("swap", d("swap")),
            net_down: color("net_down", d("net_down")),
            net_up: color("net_up", d("net_up")),
            disk_read: color("disk_read", d("disk_read")),
            disk_write: color("disk_write", d("disk_write")),
            chart_grid: color("chart_grid", d("chart_grid")),
            selection: color("selection", d("selection")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_themes_complete() {
        let set = ThemeSet::builtin();
        assert_eq!(set.names().len(), 12);
        for expected in [
            "Dark", "Light", "Dracula", "Nord", "Tokyo Night", "Catppuccin", "Monokai",
            "Gruvbox Dark", "Solarized Dark", "Ayu Dark", "Everforest", "Rose Pine",
        ] {
            assert!(set.names().iter().any(|n| n == expected), "missing theme {expected}");
        }
        for theme in &set.themes {
            for key in COLOR_KEYS {
                assert!(theme.colors.contains_key(key), "{} missing {}", theme.name, key);
                assert!(parse_hex_color(&theme.colors[key]).is_some());
            }
        }
    }

    #[test]
    fn overrides_apply() {
        let set = ThemeSet::builtin();
        let mut overrides = BTreeMap::new();
        overrides.insert("accent".to_string(), "#010203".to_string());
        let resolved = set.resolve("Dark", &overrides);
        assert_eq!(resolved.accent, Color::Rgb(1, 2, 3));
    }
}
