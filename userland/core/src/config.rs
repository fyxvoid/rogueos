//! Config: transparency, corner_radius, shortcuts. Mandatory; adjust via shortcuts only.

/// Transparency 0..=255 (255 = opaque).
pub type Transparency = u8;
/// Corner radius in px.
pub type CornerRadius = u32;

#[derive(Clone, Copy, Debug)]
pub struct TransparencyRange {
    pub default: Transparency,
    pub min: Transparency,
    pub max: Transparency,
}

impl Default for TransparencyRange {
    fn default() -> Self {
        Self { default: 255, min: 128, max: 255 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CornerRadiusRange {
    pub default: CornerRadius,
    pub min: CornerRadius,
    pub max: CornerRadius,
}

impl Default for CornerRadiusRange {
    fn default() -> Self {
        Self { default: 8, min: 0, max: 24 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ShortcutAction {
    IncreaseTransparency = 0,
    DecreaseTransparency,
    IncreaseCornerRadius,
    DecreaseCornerRadius,
    Screenshot,
    Lock,
    ClipboardPaste,
    FocusLeft,
    FocusRight,
    Confirm,
    Exit,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub transparency: TransparencyRange,
    pub corner_radius: CornerRadiusRange,
    pub shortcuts: [u8; 11],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            transparency: TransparencyRange::default(),
            corner_radius: CornerRadiusRange::default(),
            shortcuts: [0; 11],
        }
    }
}

impl Config {
    pub fn clamp_transparency(&self, v: Transparency) -> Transparency {
        v.clamp(self.transparency.min, self.transparency.max)
    }
    pub fn clamp_corner_radius(&self, v: CornerRadius) -> CornerRadius {
        v.clamp(self.corner_radius.min, self.corner_radius.max)
    }
    pub fn key_for(&self, action: ShortcutAction) -> u8 {
        let i = action as usize;
        if i < self.shortcuts.len() { self.shortcuts[i] } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_clamp_transparency() {
        let c = Config::default();
        assert_eq!(c.clamp_transparency(0), 128);
        assert_eq!(c.clamp_transparency(255), 255);
        assert_eq!(c.clamp_transparency(200), 200);
    }

    #[test]
    fn default_config_clamp_corner_radius() {
        let c = Config::default();
        assert_eq!(c.clamp_corner_radius(0), 0);
        assert_eq!(c.clamp_corner_radius(100), 24);
        assert_eq!(c.clamp_corner_radius(8), 8);
    }

    #[test]
    fn key_for_returns_shortcut() {
        let mut c = Config::default();
        c.shortcuts[ShortcutAction::Exit as usize] = 42;
        assert_eq!(c.key_for(ShortcutAction::Exit), 42);
    }
}
