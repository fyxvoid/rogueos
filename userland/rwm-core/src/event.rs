//! Event bus — typed WM events for hooks and plugin notification.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use crate::client::ClientId;
use crate::monitor::MonitorId;

/// Window manager events that plugins and hooks can subscribe to.
#[derive(Debug, Clone)]
pub enum WmEvent {
    /// A new client was managed.
    ClientCreated { id: ClientId, win: u32, class: String, title: String },
    /// A client was destroyed / unmanaged.
    ClientDestroyed { id: ClientId, win: u32 },
    /// Focus changed to a client.
    ClientFocused { id: ClientId },
    /// A client lost focus.
    ClientUnfocused { id: ClientId },
    /// Tags on a monitor changed.
    TagSwitched { mon: MonitorId, old_tags: u32, new_tags: u32 },
    /// Layout changed on a monitor.
    LayoutChanged { mon: MonitorId, symbol: String },
    /// A bar segment was clicked.
    BarClick { mon: MonitorId, x: i32, button: u32 },
    /// WM startup complete.
    Startup,
    /// WM is shutting down.
    Shutdown,
    /// Config was reloaded.
    ConfigReloaded,
}

/// Callback type for event hooks.
pub type HookFn = Box<dyn FnMut(&WmEvent) + Send>;

/// Simple event bus: register callbacks, fire events.
pub struct EventBus {
    hooks: Vec<(String, HookFn)>,
}

impl EventBus {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    /// Register a named hook.
    pub fn subscribe(&mut self, name: impl Into<String>, hook: HookFn) {
        self.hooks.push((name.into(), hook));
    }

    /// Remove all hooks with the given name prefix (for hot-reload).
    pub fn unsubscribe_prefix(&mut self, prefix: &str) {
        self.hooks.retain(|(name, _)| !name.starts_with(prefix));
    }

    /// Fire an event to all subscribers.
    pub fn emit(&mut self, event: &WmEvent) {
        for (_, hook) in self.hooks.iter_mut() {
            hook(event);
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
