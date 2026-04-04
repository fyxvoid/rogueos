//! Layout algorithm tests

use rwm_core::layout::{builtin_layouts, TileLayout, MonocleLayout, SpiralLayout, GridLayout};
use rwm_core::layout::Layout;
use rwm_core::{Client, Monitor, Rect};
use slotmap::Key;

fn mon() -> Monitor {
    Monitor::new(0, Rect::new(0, 0, 800, 600))
}

fn client(id: u32) -> Client {
    Client::new(id, Rect::new(0, 0, 100, 100), 0)
}

#[test]
fn tile_symbol() {
    let t = TileLayout { smart_gaps: true };
    assert_eq!(t.symbol(), "[]=");
    assert_eq!(t.name(), "tile");
}

#[test]
fn monocle_symbol() {
    let m = MonocleLayout;
    assert_eq!(m.symbol(), "[M]");
    assert_eq!(m.name(), "monocle");
}

#[test]
fn spiral_symbol() {
    let s = SpiralLayout { smart_gaps: true };
    assert_eq!(s.symbol(), "[@]");
    assert_eq!(s.name(), "spiral");
}

#[test]
fn builtin_layouts_count() {
    let layouts = builtin_layouts(true);
    assert_eq!(layouts.len(), 7);
}

#[test]
fn tile_arrange_zero_clients() {
    let t = TileLayout { smart_gaps: true };
    let m = mon();
    let area = Rect::new(0, 0, 800, 600);
    let arr = t.arrange(&m, &[], area);
    assert!(arr.is_empty());
}

#[test]
fn monocle_arrange_one_client() {
    let layout = MonocleLayout;
    let m = mon();
    let c = client(1);
    use rwm_core::ClientId;
    let id = ClientId::null();
    let area = Rect::new(0, 0, 800, 600);
    let arr = layout.arrange(&m, &[(id, &c)], area);
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0].1, area);
}

#[test]
fn grid_arrange_four_clients() {
    let g = GridLayout { smart_gaps: true };
    let m = mon();
    let clients: Vec<Client> = (0..4).map(client).collect();
    use rwm_core::ClientId;
    let ids: Vec<ClientId> = (0..4).map(|_| ClientId::null()).collect();
    let pairs: Vec<(ClientId, &Client)> = ids.iter().zip(clients.iter()).map(|(i, c)| (*i, c)).collect();
    let area = Rect::new(0, 0, 800, 600);
    let arr = g.arrange(&m, &pairs, area);
    assert_eq!(arr.len(), 4);
}
