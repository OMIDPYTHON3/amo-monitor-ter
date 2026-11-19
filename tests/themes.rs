//! Verification for the expanded theme collection and per-theme TOML files.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use amo_monitor::config::theme::{ThemeSet, COLOR_KEYS};

fn config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config")
}

#[test]
fn all_twelve_themes_are_complete() {
    let set = ThemeSet::builtin();
    assert_eq!(set.names().len(), 12);
    for name in set.names() {
        let theme = set.get(&name).unwrap();
        for key in COLOR_KEYS {
            let value = theme.colors.get(key).unwrap_or_else(|| panic!("{name} missing {key}"));
            assert!(crate::hex_ok(value), "{name}.{key} is not a hex colour: {value}");
        }
    }
}

fn hex_ok(value: &str) -> bool {
    let s = value.trim_start_matches('#');
    s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[test]
fn five_new_themes_are_present() {
    let names = ThemeSet::builtin().names();
    for expected in [
        "Gruvbox Dark",
        "Solarized Dark",
        "Ayu Dark",
        "Everforest",
        "Rose Pine",
    ] {
        assert!(names.iter().any(|n| n == expected), "missing {expected}");
    }
    // Existing themes must remain.
    for existing in ["Dark", "Light", "Dracula", "Nord", "Tokyo Night", "Catppuccin", "Monokai"] {
        assert!(names.iter().any(|n| n == existing), "lost existing theme {existing}");
    }
}

#[test]
fn each_theme_has_its_own_toml_file() {
    let dir = config_dir().join("themes");
    assert!(dir.is_dir(), "config/themes directory missing");
    let files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    assert!(files.len() >= 12, "expected at least 12 theme files, found {}", files.len());
    for name in ThemeSet::builtin().names() {
        let safe: String = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let path = dir.join(format!("{safe}.toml"));
        assert!(path.exists(), "missing TOML file for {name}");
        let text = fs::read_to_string(&path).unwrap();
        let parsed: amo_monitor::config::theme::Theme = toml::from_str(&text)
            .unwrap_or_else(|e| panic!("invalid TOML for {name}: {e}"));
        assert_eq!(parsed.name, name);
        assert_eq!(parsed.colors.len(), COLOR_KEYS.len());
    }
}

#[test]
fn themes_load_from_toml_and_preserve_existing_palettes() {
    let set = ThemeSet::load(&config_dir());
    assert_eq!(set.names().len(), 12);
    let resolved = set.resolve("Gruvbox Dark", &BTreeMap::new());
    assert_eq!(resolved.background.to_string(), "#282828");
    assert_eq!(resolved.accent.to_string(), "#FABD2F");
    // Existing themes survive unchanged.
    let dark = set.resolve("Dark", &BTreeMap::new());
    assert_eq!(dark.background.to_string(), "#0B0E14");
    assert_eq!(dark.cpu.to_string(), "#5B8DEF");
}

#[test]
fn toml_edits_win_over_legacy_default_json_entries() {
    let dir = std::env::temp_dir().join(format!("amo-theme-merge-{}", std::process::id()));
    let themes_dir = dir.join("themes");
    fs::create_dir_all(&themes_dir).unwrap();

    let mut dark: amo_monitor::config::theme::Theme = toml::from_str(
        &fs::read_to_string(config_dir().join("themes").join("Dark.toml")).unwrap(),
    )
    .unwrap();
    dark.colors.insert("accent".into(), "#123456".into());
    fs::write(themes_dir.join("Dark.toml"), toml::to_string_pretty(&dark).unwrap()).unwrap();

    // An old generated themes.json still contains the original default accent.
    fs::write(
        dir.join("themes.json"),
        r##"{"themes":[{"name":"Dark","colors":{"accent":"#5B8DEF"}}]}"##,
    )
    .unwrap();

    let set = ThemeSet::load(&dir);
    assert_eq!(
        set.resolve("Dark", &BTreeMap::new()).accent.to_string(),
        "#123456"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn chart_colours_adapt_to_selected_theme() {
    let set = ThemeSet::builtin();
    let tokyo = set.resolve("Tokyo Night", &BTreeMap::new());
    let gruvbox = set.resolve("Gruvbox Dark", &BTreeMap::new());
    let rose = set.resolve("Rose Pine", &BTreeMap::new());
    assert_ne!(tokyo.cpu, gruvbox.cpu);
    assert_ne!(gruvbox.gpu, rose.gpu);
    assert_ne!(tokyo.net_down, rose.net_down);
}

#[test]
fn custom_toml_theme_is_discovered() {
    let dir = std::env::temp_dir().join(format!("amo-theme-test-{}", std::process::id()));
    let themes_dir = dir.join("themes");
    fs::create_dir_all(&themes_dir).unwrap();
    let t: amo_monitor::config::theme::Theme = toml::from_str(
        &fs::read_to_string(config_dir().join("themes").join("Dark.toml")).unwrap(),
    )
    .unwrap();
    let mut custom = t;
    custom.name = "My Custom Theme".into();
    custom.colors.insert("accent".into(), "#123456".into());
    fs::write(
        themes_dir.join("custom.toml"),
        toml::to_string_pretty(&custom).unwrap(),
    )
    .unwrap();

    let set = ThemeSet::load(&dir);
    assert!(set.names().iter().any(|n| n == "My Custom Theme"));
    assert_eq!(
        set.resolve("My Custom Theme", &BTreeMap::new()).accent.to_string(),
        "#123456"
    );
    let _ = fs::remove_dir_all(&dir);
}
