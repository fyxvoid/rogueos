//! EventBus tests

use rwm_core::event::{EventBus, WmEvent};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::Arc;

#[test]
fn event_bus_new() {
    let _bus = EventBus::new();
}

#[test]
fn event_bus_emit_no_subscribers() {
    let mut bus = EventBus::new();
    bus.emit(&WmEvent::Startup);
}

#[test]
fn event_bus_subscribe_and_emit() {
    let mut bus = EventBus::new();
    let received = Arc::new(AtomicBool::new(false));
    let r = received.clone();
    bus.subscribe("test", Box::new(move |e| {
        if matches!(e, WmEvent::Startup) {
            r.store(true, Ordering::Relaxed);
        }
    }));
    bus.emit(&WmEvent::Startup);
    assert!(received.load(Ordering::Relaxed));
}

#[test]
fn event_bus_unsubscribe_prefix() {
    let mut bus = EventBus::new();
    let count = Arc::new(AtomicI32::new(0));
    let c1 = count.clone();
    let c2 = count.clone();
    bus.subscribe("plugin.a", Box::new(move |_| { c1.fetch_add(1, Ordering::Relaxed); }));
    bus.subscribe("plugin.b", Box::new(move |_| { c2.fetch_add(1, Ordering::Relaxed); }));
    bus.emit(&WmEvent::Startup);
    assert_eq!(count.load(Ordering::Relaxed), 2);
    bus.unsubscribe_prefix("plugin.");
    bus.emit(&WmEvent::Shutdown);
    assert_eq!(count.load(Ordering::Relaxed), 2);
}

#[test]
fn event_bus_multiple_subscribers() {
    let mut bus = EventBus::new();
    let n = Arc::new(AtomicU32::new(0));
    for _ in 0..3 {
        let n = n.clone();
        bus.subscribe("x", Box::new(move |_| { n.fetch_add(1, Ordering::Relaxed); }));
    }
    bus.emit(&WmEvent::ConfigReloaded);
    assert_eq!(n.load(Ordering::Relaxed), 3);
}
