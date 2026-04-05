pub mod flags;
pub mod levels;
pub mod tlb;
pub mod page_table;
pub mod mapper;
pub mod fault;

pub use flags::{EntryFlags, PageFlag};
pub use levels::{
    PAGE_SIZE, PAGE_SIZE_4K, PAGE_SIZE_2MB, PAGE_SIZE_1GB,
    page_align_down, page_align_up,
    page_align_down_2mb, page_align_up_2mb,
    page_align_down_1gb, page_align_up_1gb,
};
pub use tlb::{read_cr3, write_cr3, flush_address, flush_all, VirtAddr};
pub use mapper::{
    alloc_table_page,
    map_page,
    map_page_identity,
    map_page_in_space,
    map_page_into_space,
    map_page_2mb_into_space,
    map_page_1gb_into_space,
    unmap_page,
    unmap_page_in_space,
    identity_map_range,
    alloc_pml4,
    new_address_space,
    create_process_cr3,
    get_kernel_cr3,
    alloc_pcid,
    probe_pcid,
    walk_pte,
    translate,
    translate_in_space,
    dump_ptes_for_vas_serial,
    dump_ptes_range_serial,
    init,
};

#[cfg(not(test))]
pub use mapper::{debug_walk, debug_walk_in_space};
