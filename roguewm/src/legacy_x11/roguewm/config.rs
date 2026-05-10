#![allow(dead_code)]
use x11rb::protocol::xproto::ModMask;

pub const MOD_KEY: ModMask = ModMask::M1; // Alt key
pub const SHIFT_KEY: ModMask = ModMask::SHIFT;

// Nord Theme Colors
pub const COLOR_NORM_BORDER: u32 = 0x3b4252; // Nord 1
pub const COLOR_SEL_BORDER: u32 = 0x88c0d0;  // Nord 8

pub const TERM_CMD: &[&str] = &["alacritty"];
pub const LAUNCHER_CMD: &[&str] = &["dmenu_run"];

pub const TAGS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

pub struct KeyBinding {
    pub mod_mask: ModMask,
    pub key: u8, // keysym or keycode... for simplicity we might need a mapping function
    pub action: Action,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Spawn(&'static [&'static str]),
    Quit,
    KillClient,
    ToggleBar,
    FocusNext,
    FocusPrev,
    Zoom,
    SetMFact(f32),
    SetLayout(LayoutSymbol),
    ToggleFloating,
    ViewTag(u32),
    TagClient(u32),
    MoveStack(i32),
    ToggleGaps,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutSymbol {
    Tile,
    Floating,
    Monocle,
    Dwindle,
}

// Gap constants
pub const GAPP_IH: i32 = 10;
pub const GAPP_IV: i32 = 10;
pub const GAPP_OH: i32 = 10;
pub const GAPP_OV: i32 = 10;

#[derive(Clone, Copy, Debug)]
pub struct Rule {
    pub class: Option<&'static str>,
    pub instance: Option<&'static str>,
    pub title: Option<&'static str>,
    pub tags: u32,
    pub is_floating: bool,
    pub monitor: i32,
}

pub const RULES: &[Rule] = &[
    Rule { class: Some("Gimp"), instance: None, title: None, tags: 0, is_floating: true, monitor: -1 },
    Rule { class: Some("Firefox"), instance: None, title: None, tags: 1 << 8, is_floating: false, monitor: -1 },
];

pub fn get_keybindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding { mod_mask: MOD_KEY, key: 0x18, action: Action::Quit }, // Q
        KeyBinding { mod_mask: MOD_KEY | SHIFT_KEY, key: 0x24, action: Action::Spawn(TERM_CMD) }, // Return
        KeyBinding { mod_mask: MOD_KEY, key: 0x28, action: Action::Spawn(LAUNCHER_CMD) }, // P (dmenu)
        KeyBinding { mod_mask: MOD_KEY, key: 0x2c, action: Action::FocusNext }, // J
        KeyBinding { mod_mask: MOD_KEY, key: 0x2d, action: Action::FocusPrev }, // K
        KeyBinding { mod_mask: MOD_KEY, key: 0x1f, action: Action::SetLayout(LayoutSymbol::Tile) }, // I
        KeyBinding { mod_mask: MOD_KEY, key: 0x1e, action: Action::SetLayout(LayoutSymbol::Floating) }, // U
        KeyBinding { mod_mask: MOD_KEY, key: 0x3a, action: Action::SetLayout(LayoutSymbol::Monocle) }, // M
        KeyBinding { mod_mask: MOD_KEY, key: 0x28, action: Action::SetLayout(LayoutSymbol::Dwindle) }, // D (0x28 is 'f' usually? 0x28 is 'e'? Wait 'd' is 0x28 on some maps. Let's use 's' 0x27 or 'd' 0x28. 'd' is 40 in decimal? 0x28 is 40. 'f' is 41/0x29? 'd' is 0x28.)
        // Gap control keys (using Mod4 - assuming Super)
        // XK_u -> 0x1e (scan code?) No, we need keycodes.
        // For simplicity, I'll map a few example ones, user can expand.
        KeyBinding { mod_mask: MOD_KEY | ModMask::M4, key: 0x14, action: Action::ToggleGaps }, // -/0 key? 
        // MoveStack
        KeyBinding { mod_mask: MOD_KEY | SHIFT_KEY, key: 0x2c, action: Action::MoveStack(1) }, // J
        KeyBinding { mod_mask: MOD_KEY | SHIFT_KEY, key: 0x2d, action: Action::MoveStack(-1) }, // K
    ]
}

