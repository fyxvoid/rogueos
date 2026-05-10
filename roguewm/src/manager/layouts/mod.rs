pub mod geometry;
pub mod tile;
pub mod monocle;
pub mod dwindle;

pub use geometry::{Rect, LayoutState, Gaps};
use crate::roguewm::config::LayoutSymbol;

pub fn calculate_layout(symbol: LayoutSymbol, state: &LayoutState) -> Vec<Rect> {
    match symbol {
        LayoutSymbol::Tile => tile::tile(state),
        LayoutSymbol::Floating => Vec::new(), 
        LayoutSymbol::Monocle => monocle::monocle(state),
        LayoutSymbol::Dwindle => dwindle::dwindle(state),
    }
}
