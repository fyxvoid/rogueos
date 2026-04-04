pub mod address_space;
pub mod mapping;
pub mod region;

pub use address_space::AddressSpace;
pub use mapping::{alloc_address_space, alloc_table_page, map_page_in_space, map_range};
pub use region::VmArea;
