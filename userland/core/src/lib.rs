#![no_std]

//! Unified userland core: config and display backend trait.

pub mod config;
pub mod backend;

pub use config::{Config, Transparency, CornerRadius, TransparencyRange, CornerRadiusRange, ShortcutAction};
pub use backend::{DisplayBackend, HeadlessBackend};
