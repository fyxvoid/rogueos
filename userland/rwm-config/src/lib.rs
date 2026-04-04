//! # rwm-config
//!
//! TOML-based configuration for **roguewm**.
//!
//! Replaces dwm's compile-time `config.h` with a runtime config file at
//! `~/.config/roguewm/config.toml`.  Defaults are the **rogue-website** theme
//! (`#050505` void, `#ff0000` rogue-red, `#111111` panel, JetBrainsMono NF).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ══════════════════════════════════════════════════════════════════════
//  Top-level Config
// ══════════════════════════════════════════════════════════════════════

/// User profile — selects WM color theme (Pentester=red, Developer=blue, PowerUser=white).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    #[default]
    Pentester,
    Developer,
    PowerUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub profile: Profile,
    pub appearance: Appearance,
    pub bar: BarConfig,
    pub tags: TagsConfig,
    pub keybinds: Vec<Keybind>,
    pub rules: Vec<Rule>,
    pub plugins: PluginConfig,
    pub scratchpad: ScratchpadConfig,
    pub compositor: CompositorConfig,
}

impl Default for Config {
    fn default() -> Self {
        let profile = Profile::default();
        Self {
            profile,
            appearance: Appearance::default_with_profile(profile),
            bar: BarConfig::default(),
            tags: TagsConfig::default(),
            keybinds: default_keybinds(),
            rules: default_rules(),
            plugins: PluginConfig::default(),
            scratchpad: ScratchpadConfig::default(),
            compositor: CompositorConfig::default(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Appearance — colors, borders, gaps, font
// ══════════════════════════════════════════════════════════════════════

/// Visual appearance settings — mapped from rogue-website css/core.css variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// Font for bar and dmenu.  Default: JetBrainsMono Nerd Font.
    pub font: String,
    /// Window border width in pixels.  Default: 2 (website: `border: 2px`).
    pub border_width: u32,
    /// Snap distance for mouse operations.
    pub snap: u32,
    /// Gap configuration.
    pub gaps: GapConfig,
    /// Color scheme (rogue-website palette).
    pub colors: ColorConfig,
}

impl Default for Appearance {
    fn default() -> Self {
        Self::default_with_profile(Profile::default())
    }
}

impl Appearance {
    /// Default appearance for a given profile (theme colors).
    pub fn default_with_profile(profile: Profile) -> Self {
        Self {
            font: "JetBrainsMono Nerd Font:size=11".into(),
            border_width: 2,
            snap: 16,
            gaps: GapConfig::default(),
            colors: ColorConfig::for_profile(profile),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GapConfig {
    pub inner_h: i32,
    pub inner_v: i32,
    pub outer_h: i32,
    pub outer_v: i32,
    /// Disable outer gaps when only one window is visible.
    pub smart_gaps: bool,
}

impl Default for GapConfig {
    fn default() -> Self {
        Self {
            inner_h: 4,
            inner_v: 4,
            outer_h: 4,
            outer_v: 4,
            smart_gaps: true,
        }
    }
}

/// Color scheme — direct map of rogue-website CSS custom properties.
///
/// ```css
/// :root {
///   --color-void:       #050505;
///   --color-void-deep:  #000000;
///   --color-panel:      #111111;
///   --color-rogue-red:  #ff0000;
///   --color-linux-white:#ffffff;
/// }
/// .dwm-tag         { color: #888; background: #111; }
/// .dwm-tag.active  { color: #fff; background: var(--color-rogue-red); }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorConfig {
    /// Background for void areas.  `--color-void` = `#050505`.
    pub void: String,
    /// Deepest black (bar bg).  `--color-void-deep` = `#000000`.
    pub void_deep: String,
    /// Panel / elevated surface.  `--color-panel` = `#111111`.
    pub panel: String,
    /// Primary accent.  `--color-rogue-red` = `#ff0000`.
    pub rogue_red: String,
    /// Bright text.  `--color-linux-white` = `#ffffff`.
    pub white: String,
    /// Secondary text.  `.dwm-tag` color = `#888888`.
    pub gray: String,
    /// Border / divider.  `#333333`.
    pub border: String,
    /// Warning accent.  `--color-warning-amber` = `#ffb703`.
    pub warning: String,

    // ── Derived (scheme norm / sel) ─────────────────────────────────
    /// Normal foreground (inactive tags, status text).
    pub norm_fg: String,
    /// Normal background (bar bg).
    pub norm_bg: String,
    /// Normal border (unfocused windows).
    pub norm_border: String,
    /// Selected foreground (active tag text, focused title).
    pub sel_fg: String,
    /// Selected background (active tag bg, focused title bg).
    pub sel_bg: String,
    /// Selected border (focused windows).
    pub sel_border: String,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self::for_profile(Profile::default())
    }
}

impl ColorConfig {
    /// Color preset for the given profile: Pentester=red, Developer=blue, PowerUser=white.
    pub fn for_profile(profile: Profile) -> Self {
        let (accent, sel_fg) = match profile {
            Profile::Pentester => ("#ff0000", "#ffffff"),
            Profile::Developer => ("#0a84ff", "#ffffff"),
            Profile::PowerUser => ("#ffffff", "#000000"),
        };
        Self {
            void: "#050505".into(),
            void_deep: "#000000".into(),
            panel: "#111111".into(),
            rogue_red: accent.to_string(),
            white: "#ffffff".into(),
            gray: "#888888".into(),
            border: "#333333".into(),
            warning: "#ffb703".into(),
            norm_fg: "#888888".into(),
            norm_bg: "#000000".into(),
            norm_border: "#111111".into(),
            sel_fg: sel_fg.to_string(),
            sel_bg: accent.to_string(),
            sel_border: accent.to_string(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Bar
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BarConfig {
    pub enabled: bool,
    /// `"top"` or `"bottom"`.
    pub position: String,
    /// Bar height in pixels (0 = auto from font).
    pub height: u32,
    /// Widgets on the left side of the bar.
    pub left: Vec<String>,
    /// Widgets on the right side of the bar.
    pub right: Vec<String>,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position: "top".into(),
            height: 0, // auto
            left: vec!["tags".into(), "layout".into(), "title".into()],
            right: vec![
                "target".into(), "network".into(), "volume".into(),
                "battery".into(), "cpu".into(), "memory".into(), "clock".into(),
            ],
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Tags
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TagsConfig {
    /// Tag display names (max 9).
    pub names: Vec<String>,
}

impl Default for TagsConfig {
    fn default() -> Self {
        Self {
            // Nerd Font icons for pentesting/power-user workflows
            names: vec![
                "\u{eb01}".into(),  // nf-cod-terminal  — Terminal/shell
                "\u{eb10}".into(),  // nf-cod-code      — Code editors
                "\u{f0239}".into(), // nf-md-firefox     — Browser/Burp
                "\u{f0483}".into(), // nf-md-shield_lock — Security tools
                "\u{f0379}".into(), // nf-md-monitor     — Monitoring
                "\u{ea7b}".into(),  // nf-cod-file_text  — Documentation
                "\u{f066f}".into(), // nf-md-message     — Communication
                "\u{f057c}".into(), // nf-md-volume_high — Media/audio
                "\u{eb51}".into(),  // nf-cod-gear       — Config/settings
            ],
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Keybinds
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybind {
    /// Modifier keys: `"Super"`, `"Shift"`, `"Control"`, `"Alt"`.
    #[serde(rename = "mod")]
    pub modifiers: Vec<String>,
    /// Key name (X11 keysym name, e.g. `"Return"`, `"p"`, `"1"`).
    pub key: String,
    /// Action to perform.
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Spawn an external command.
    Spawn { cmd: Vec<String> },
    /// Switch to a layout by name.
    SetLayout { name: String },
    /// View specific tags (bitmask).
    ViewTag { tag: u32 },
    /// Move client to tag.
    MoveToTag { tag: u32 },
    /// Toggle view of a tag.
    ToggleViewTag { tag: u32 },
    /// Toggle client on a tag.
    ToggleClientTag { tag: u32 },
    /// Focus next/prev in stack.
    FocusStack { direction: i32 },
    /// Adjust master factor.
    SetMfact { delta: f32 },
    /// Adjust cfact on focused client.
    SetCfact { delta: f32 },
    /// Inc/dec master count.
    IncNmaster { delta: i32 },
    /// Promote focused to master.
    Zoom,
    /// Kill focused client.
    KillClient,
    /// Toggle bar.
    ToggleBar,
    /// Toggle scratchpad.
    ToggleScratchpad,
    /// Toggle focused client floating/tiled.
    ToggleFloating,
    /// Toggle focused client fullscreen.
    ToggleFullscreen,
    /// Quit WM.
    Quit,
    /// View previous tags.
    ViewPrevious,
    /// Inc/dec gaps.
    IncGaps { delta: i32 },
    /// Toggle gaps on/off.
    ToggleGaps,
    /// Reset gaps to default.
    DefaultGaps,
    /// Focus next/prev monitor.
    FocusMonitor { direction: i32 },
    /// Reload config.
    ReloadConfig,
    /// Run a Lua plugin command.
    LuaCommand { name: String },
    /// Screenshot full screen to clipboard (maim | xclip).
    Screenshot,
    /// Screenshot selected region to clipboard (maim -s | xclip).
    ScreenshotSelect,
    /// Screenshot full screen saved to ~/screenshots/.
    ScreenshotSave,
    /// Pick a color from screen (xcolor | xclip).
    ColorPicker,
    /// Start/stop screen recording (ffmpeg x11grab).
    ToggleRecording,
}

// ══════════════════════════════════════════════════════════════════════
//  Rules
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub instance: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// Tag number (1-based) or 0 for inherit.
    #[serde(default)]
    pub tag: u32,
    #[serde(default)]
    pub floating: Option<bool>,
    #[serde(default)]
    pub monitor: Option<i32>,
}

// ══════════════════════════════════════════════════════════════════════
//  Plugins
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginConfig {
    /// Directories to search for Lua plugins.
    pub lua_paths: Vec<String>,
    /// Specific Lua scripts to load.
    pub lua_files: Vec<String>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            lua_paths: vec!["~/.config/roguewm/plugins".into()],
            lua_files: Vec::new(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Scratchpad
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScratchpadConfig {
    /// Name to match window title.
    pub name: String,
    /// Command to spawn the scratchpad.
    pub cmd: Vec<String>,
}

impl Default for ScratchpadConfig {
    fn default() -> Self {
        Self {
            name: "scratchpad".into(),
            cmd: vec!["st".into(), "-t".into(), "scratchpad".into()],
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Compositor
// ══════════════════════════════════════════════════════════════════════

/// Compositor configuration — launches picom for transparency and tear-free rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CompositorConfig {
    /// Whether to launch the compositor on WM startup.
    pub enabled: bool,
    /// Compositor binary name.
    pub command: String,
    /// Arguments to pass to the compositor.
    pub args: Vec<String>,
}

impl Default for CompositorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            command: "picom".into(),
            args: vec![
                "--config".into(),
                "/etc/xdg/picom/picom.conf".into(),
            ],
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Default Keybinds (port of dwm config.h keys[])
// ══════════════════════════════════════════════════════════════════════

fn default_keybinds() -> Vec<Keybind> {
    let s = |s: &str| s.to_string();
    let sup = || vec![s("Super")];
    let sup_shift = || vec![s("Super"), s("Shift")];
    let sup_ctrl = || vec![s("Super"), s("Control")];

    let mut binds = vec![
        Keybind { modifiers: sup_shift(), key: s("Return"), action: Action::Spawn { cmd: vec![s("st")] } },
        Keybind { modifiers: sup(), key: s("p"), action: Action::Spawn { cmd: vec![s("rogue-run")] } },
        Keybind { modifiers: sup(), key: s("b"), action: Action::ToggleBar },
        Keybind { modifiers: sup(), key: s("j"), action: Action::FocusStack { direction: 1 } },
        Keybind { modifiers: sup(), key: s("k"), action: Action::FocusStack { direction: -1 } },
        Keybind { modifiers: sup(), key: s("i"), action: Action::IncNmaster { delta: 1 } },
        Keybind { modifiers: sup(), key: s("d"), action: Action::IncNmaster { delta: -1 } },
        Keybind { modifiers: sup(), key: s("h"), action: Action::SetMfact { delta: -0.05 } },
        Keybind { modifiers: sup(), key: s("l"), action: Action::SetMfact { delta: 0.05 } },
        Keybind { modifiers: sup_shift(), key: s("h"), action: Action::SetCfact { delta: 0.25 } },
        Keybind { modifiers: sup_shift(), key: s("l"), action: Action::SetCfact { delta: -0.25 } },
        Keybind { modifiers: sup_shift(), key: s("o"), action: Action::SetCfact { delta: 0.0 } },
        Keybind { modifiers: sup(), key: s("Return"), action: Action::Zoom },
        Keybind { modifiers: sup(), key: s("Tab"), action: Action::ViewPrevious },
        Keybind { modifiers: sup_shift(), key: s("c"), action: Action::KillClient },
        Keybind { modifiers: sup(), key: s("t"), action: Action::SetLayout { name: s("tile") } },
        Keybind { modifiers: sup(), key: s("m"), action: Action::SetLayout { name: s("monocle") } },
        Keybind { modifiers: sup(), key: s("f"), action: Action::SetLayout { name: s("spiral") } },
        Keybind { modifiers: sup_shift(), key: s("space"), action: Action::ToggleFloating },
        Keybind { modifiers: sup_shift(), key: s("f"), action: Action::ToggleFullscreen },
        Keybind { modifiers: sup(), key: s("grave"), action: Action::ToggleScratchpad },
        Keybind { modifiers: sup_ctrl(), key: s("u"), action: Action::IncGaps { delta: 1 } },
        Keybind { modifiers: vec![s("Super"), s("Control"), s("Shift")], key: s("u"), action: Action::IncGaps { delta: -1 } },
        Keybind { modifiers: sup_ctrl(), key: s("0"), action: Action::ToggleGaps },
        Keybind { modifiers: sup_shift(), key: s("q"), action: Action::Quit },
        Keybind { modifiers: sup_shift(), key: s("r"), action: Action::ReloadConfig },
        Keybind { modifiers: sup(), key: s("comma"), action: Action::FocusMonitor { direction: -1 } },
        Keybind { modifiers: sup(), key: s("period"), action: Action::FocusMonitor { direction: 1 } },
        // ── Power-user tools ────────────────────────────────────────
        Keybind { modifiers: vec![], key: s("Print"), action: Action::Screenshot },
        Keybind { modifiers: sup(), key: s("Print"), action: Action::ScreenshotSelect },
        Keybind { modifiers: sup_shift(), key: s("Print"), action: Action::ScreenshotSave },
        Keybind { modifiers: sup_shift(), key: s("p"), action: Action::ColorPicker },
        Keybind { modifiers: sup_shift(), key: s("v"), action: Action::ToggleRecording },
    ];

    // Tag binds: Super+1..9 → view, Super+Shift+1..9 → move
    for i in 1..=9u32 {
        let key = i.to_string();
        binds.push(Keybind {
            modifiers: sup(),
            key: key.clone(),
            action: Action::ViewTag { tag: i },
        });
        binds.push(Keybind {
            modifiers: sup_shift(),
            key: key.clone(),
            action: Action::MoveToTag { tag: i },
        });
        binds.push(Keybind {
            modifiers: sup_ctrl(),
            key: key.clone(),
            action: Action::ToggleViewTag { tag: i },
        });
        binds.push(Keybind {
            modifiers: vec![s("Super"), s("Control"), s("Shift")],
            key,
            action: Action::ToggleClientTag { tag: i },
        });
    }

    binds
}

fn default_rules() -> Vec<Rule> {
    vec![
        // Browsers → tag 3 (www/browser)
        Rule { class: Some("Firefox".into()), instance: None, title: None, tag: 3, floating: Some(false), monitor: None },
        Rule { class: Some("Chromium".into()), instance: None, title: None, tag: 3, floating: Some(false), monitor: None },
        // Security tools → tag 4 (security)
        Rule { class: Some("burp-StartBurp".into()), instance: None, title: None, tag: 4, floating: Some(false), monitor: None },
        Rule { class: Some("Metasploit".into()), instance: None, title: None, tag: 4, floating: Some(false), monitor: None },
        // Monitoring → tag 5 (monitoring)
        Rule { class: Some("Wireshark".into()), instance: None, title: None, tag: 5, floating: Some(false), monitor: None },
        // Communication → tag 7 (comms)
        Rule { class: Some("discord".into()), instance: None, title: None, tag: 7, floating: Some(false), monitor: None },
        Rule { class: Some("Signal".into()), instance: None, title: None, tag: 7, floating: Some(false), monitor: None },
        // Media → tag 8 (media)
        Rule { class: Some("mpv".into()), instance: None, title: None, tag: 8, floating: Some(false), monitor: None },
        // Floating by default
        Rule { class: Some("Gimp".into()), instance: None, title: None, tag: 0, floating: Some(true), monitor: None },
        Rule { class: None, instance: None, title: Some("Event Tester".into()), tag: 0, floating: Some(true), monitor: None },
    ]
}

// ══════════════════════════════════════════════════════════════════════
//  Loading and validation
// ══════════════════════════════════════════════════════════════════════

/// Maximum number of tags (must match rwm_core::TAG_COUNT).
const MAX_TAGS: usize = 9;

impl Config {
    /// Load config from the default path (`~/.config/roguewm/config.toml`),
    /// falling back to built-in defaults. Writes default config if path does not exist (first-run).
    pub fn load() -> Self {
        let path = Self::default_path();
        if !path.exists() {
            if let Err(e) = Self::write_default(&path) {
                tracing::warn!("Could not write default config to {}: {}", path.display(), e);
            } else {
                tracing::info!("Wrote default config to {} (first-run)", path.display());
            }
        }
        Self::load_from_path(&path)
    }

    /// Default config file path.
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/etc"))
            .join("roguewm")
            .join("config.toml")
    }

    /// Validate and sanitize config. Clamps tag count to MAX_TAGS; invalid entries are logged.
    pub fn validate(&mut self) {
        if self.tags.names.len() > MAX_TAGS {
            tracing::warn!(
                "Config has {} tag names; clamping to {}",
                self.tags.names.len(),
                MAX_TAGS
            );
            self.tags.names.truncate(MAX_TAGS);
        }
        for keybind in &self.keybinds {
            for mod_ in &keybind.modifiers {
                let m = mod_.to_lowercase();
                if !["super", "shift", "control", "alt"].contains(&m.as_str()) {
                    tracing::debug!("Unknown modifier in keybind: '{}' (key: {})", mod_, keybind.key);
                }
            }
        }
    }

    /// Load from a specific path, falling back to defaults on any error.
    /// Colors are applied from the configured profile (red/blue/white).
    /// Config is validated after load.
    pub fn load_from_path(path: &Path) -> Self {
        let mut cfg = match std::fs::read_to_string(path) {
            Ok(content) => match toml::from_str::<Config>(&content) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Config parse error ({}), using defaults: {}", path.display(), e);
                    return Self::default();
                }
            },
            Err(_) => {
                tracing::info!("No config at {}, using defaults (rogue-website theme)", path.display());
                return Self::default();
            }
        };
        cfg.appearance.colors = ColorConfig::for_profile(cfg.profile);
        cfg.validate();
        tracing::info!("Config loaded from {} (profile: {:?})", path.display(), cfg.profile);
        cfg
    }

    /// Write default config to the config path (for first-run).
    pub fn write_default(path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(&Config::default())
            .map_err(std::io::Error::other)?;
        std::fs::write(path, content)
    }
}
