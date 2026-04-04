//! Minimal NVMe driver: MMIO, admin and I/O queues, BlockDevice implementation.
//! Hardcoded as root device; BAR from BootInfo (0 = not present).

use core::ptr;

use libs::BootInfo;

use crate::drivers::traits::BlockDevice;
use crate::memory::paging;

const PAGE_SIZE: u64 = 4096;

// NVMe register offsets (BAR0)
const CAP: u64 = 0x00;
const CC: u64 = 0x14;
const CSTS: u64 = 0x1C;
const AQA: u64 = 0x24;
const ASQ: u64 = 0x28;
const ACQ: u64 = 0x30;
const DOORBELL_STRIDE: u64 = 4;
fn sq_tdbl(qid: u16) -> u64 { 0x1000 + (qid as u64 * 2) * DOORBELL_STRIDE }
fn cq_hdbl(qid: u16) -> u64 { 0x1000 + (qid as u64 * 2 + 1) * DOORBELL_STRIDE }

const CC_EN: u32 = 1 << 0;
const CSTS_RDY: u32 = 1 << 0;

const ADMIN_QUEUE_SIZE: u32 = 2;
const IO_QUEUE_SIZE: u32 = 2;
const IO_SQ_ID: u16 = 1;
const IO_CQ_ID: u16 = 1;

const NSID: u32 = 1;

// Admin command opcodes
const ADMIN_IDENTIFY: u8 = 0x06;
const ADMIN_CREATE_IO_CQ: u8 = 0x05;
const ADMIN_CREATE_IO_SQ: u8 = 0x01;

// NVM command opcodes
const NVM_READ: u8 = 0x02;
const NVM_WRITE: u8 = 0x01;

#[repr(C, align(4096))]
struct QueueBuf {
    data: [u8; 4096],
}

// Submission queue entry: 64 bytes
#[repr(C)]
struct NvmeSqe {
    cdw0: u32,   // opcode, fuse, cid
    cdw1: u32,
    nsid: u32,
    cdw3: u32,
    cdw4: u32,
    cdw5: u32,
    mptr: u64,
    prp1: u64,
    prp2: u64,
    cdw10: u32,
    cdw11: u32,
    cdw12: u32,
    cdw13: u32,
    cdw14: u32,
    cdw15: u32,
}

#[repr(C)]
struct NvmeCqe {
    cdw0: u32,
    cdw1: u32,
    sq_head: u16,
    sq_id: u16,
    cid: u16,
    status: u16,
}

static mut BAR_VIRT: *mut u8 = ptr::null_mut();
static mut ADMIN_SQ: QueueBuf = QueueBuf { data: [0; 4096] };
static mut ADMIN_CQ: QueueBuf = QueueBuf { data: [0; 4096] };
static mut IO_SQ: QueueBuf = QueueBuf { data: [0; 4096] };
static mut IO_CQ: QueueBuf = QueueBuf { data: [0; 4096] };
static mut ADMIN_SQ_TAIL: u32 = 0;
static mut ADMIN_CQ_HEAD: u32 = 0;
static mut IO_SQ_TAIL: u32 = 0;
static mut IO_CQ_HEAD: u32 = 0;
static mut INITIALIZED: bool = false;
static mut BLOCK_SIZE: u32 = 512;

fn reg_read(offset: u64) -> u32 {
    unsafe {
        let p = BAR_VIRT.add(offset as usize) as *const u32;
        ptr::read_volatile(p)
    }
}

fn reg_write(offset: u64, val: u32) {
    unsafe {
        let p = BAR_VIRT.add(offset as usize) as *mut u32;
        ptr::write_volatile(p, val);
    }
}

fn reg_write64(offset: u64, val: u64) {
    unsafe {
        let p = BAR_VIRT.add(offset as usize) as *mut u64;
        ptr::write_volatile(p, val);
    }
}

/// Physical address of a static queue buffer for DMA. **Identity-mapping requirement:** kernel
/// must identity-map the range containing ADMIN_SQ, ADMIN_CQ, IO_SQ, IO_CQ so physical address
/// equals the pointer value (used for NVMe PRP and doorbell).
fn phys_of_buf(buf: *const QueueBuf) -> u64 {
    buf as usize as u64
}

/// Map BAR and disable controller.
fn map_bar(bar_phys: u64) -> bool {
    if bar_phys == 0 {
        return false;
    }
    let page_size = 4096u64;
    let start = bar_phys & !(page_size - 1);
    let size = 8 * 1024; // 8K for registers + doorbells
    let end = (bar_phys + size as u64 + page_size - 1) & !(page_size - 1);
    let mut pa = start;
    while pa < end {
        if !paging::map_page_identity(pa, paging::EntryFlags::kernel_rw().as_u64()) {
            return false;
        }
        pa += page_size;
    }
    unsafe {
        BAR_VIRT = bar_phys as *mut u8;
    }
    true
}

fn disable_controller() {
    reg_write(CC, reg_read(CC) & !CC_EN);
    for _ in 0..1000 {
        if (reg_read(CSTS) & CSTS_RDY) == 0 {
            return;
        }
        // spin
    }
}

fn enable_controller() -> bool {
    reg_write(CC, CC_EN);
    for _ in 0..1000 {
        if (reg_read(CSTS) & CSTS_RDY) != 0 {
            return true;
        }
    }
    false
}

fn admin_submit(cmd: &NvmeSqe) -> bool {
    unsafe {
        let sq = &mut ADMIN_SQ as *mut QueueBuf as *mut NvmeSqe;
        let idx = (ADMIN_SQ_TAIL % ADMIN_QUEUE_SIZE) as usize;
        ptr::copy_nonoverlapping(cmd as *const NvmeSqe, sq.add(idx), 1);
        ADMIN_SQ_TAIL += 1;
        reg_write(sq_tdbl(0), ADMIN_SQ_TAIL);
    }
    true
}

fn admin_wait_completion(cid: u16) -> bool {
    unsafe {
        let cq = &ADMIN_CQ as *const QueueBuf as *const NvmeCqe;
        for _ in 0..5000 {
            let idx = (ADMIN_CQ_HEAD % ADMIN_QUEUE_SIZE) as usize;
            let phase = (ADMIN_CQ_HEAD / ADMIN_QUEUE_SIZE) & 1;
            let entry = &*cq.add(idx);
            let entry_phase = (entry.cdw0 >> 0) & 1;
            if entry_phase == phase && entry.cid == cid {
                ADMIN_CQ_HEAD += 1;
                reg_write(cq_hdbl(0), ADMIN_CQ_HEAD);
                return (entry.status & 1) == 0;
            }
        }
    }
    false
}

/// Initialize NVMe from BootInfo. If nvme_bar is 0, driver stays inactive.
pub fn init_from_boot_info(boot_info: &BootInfo) -> bool {
    if boot_info.nvme_bar == 0 {
        return false;
    }
    if !map_bar(boot_info.nvme_bar) {
        return false;
    }
    disable_controller();
    unsafe {
        reg_write(AQA, ((ADMIN_QUEUE_SIZE - 1) << 16) | (ADMIN_QUEUE_SIZE - 1));
        reg_write64(ASQ, unsafe { phys_of_buf(&ADMIN_SQ as *const QueueBuf) });
        reg_write64(ACQ, unsafe { phys_of_buf(&ADMIN_CQ as *const QueueBuf) });
    }
    if !enable_controller() {
        return false;
    }
    // Identify controller (optional; we skip parsing)
    let cid = 0;
    let mut identify = NvmeSqe {
        cdw0: (ADMIN_IDENTIFY as u32) | (cid as u32) << 16,
        cdw1: 0,
        nsid: 0,
        cdw3: 0,
        cdw4: 0,
        cdw5: 0,
        mptr: 0,
        prp1: 0,
        prp2: 0,
        cdw10: 1,
        cdw11: 0,
        cdw12: 0,
        cdw13: 0,
        cdw14: 0,
        cdw15: 0,
    };
    if admin_submit(&identify) && admin_wait_completion(cid) {
        // Create I/O CQ (id=1, size=2)
        let mut create_cq = NvmeSqe {
            cdw0: (ADMIN_CREATE_IO_CQ as u32) | (1 << 16),
            cdw1: 0,
            nsid: 0,
            cdw3: 0,
            cdw4: 0,
            cdw5: 0,
            mptr: 0,
            prp1: unsafe { phys_of_buf(&IO_CQ as *const QueueBuf) },
            prp2: 0,
            cdw10: (IO_CQ_ID as u32) | ((IO_QUEUE_SIZE - 1) << 16),
            cdw11: 0, // IEN=0 (no interrupt)
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };
        if !admin_submit(&create_cq) || !admin_wait_completion(1) {
            return false;
        }
        let mut create_sq = NvmeSqe {
            cdw0: (ADMIN_CREATE_IO_SQ as u32) | (2 << 16),
            cdw1: 0,
            nsid: 0,
            cdw3: 0,
            cdw4: 0,
            cdw5: 0,
            mptr: 0,
            prp1: unsafe { phys_of_buf(&IO_SQ as *const QueueBuf) },
            prp2: 0,
            cdw10: (IO_SQ_ID as u32) | ((IO_QUEUE_SIZE - 1) << 16),
            cdw11: IO_CQ_ID as u32,
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };
        if !admin_submit(&create_sq) || !admin_wait_completion(2) {
            return false;
        }
    }
    unsafe { INITIALIZED = true; }
    true
}

fn io_submit_read(slba: u64, prp1: u64, num_blocks: u16, cid: u16) -> bool {
    unsafe {
        let sq = &mut IO_SQ as *mut QueueBuf as *mut NvmeSqe;
        let idx = (IO_SQ_TAIL % IO_QUEUE_SIZE) as usize;
        let cmd = &mut *sq.add(idx);
        cmd.cdw0 = (NVM_READ as u32) | (cid as u32) << 16;
        cmd.nsid = NSID;
        cmd.prp1 = prp1;
        cmd.prp2 = 0;
        cmd.cdw10 = (slba & 0xFFFF_FFFF) as u32;
        cmd.cdw11 = (slba >> 32) as u32;
        cmd.cdw12 = (num_blocks.wrapping_sub(1)) as u32;
        IO_SQ_TAIL += 1;
        reg_write(sq_tdbl(IO_SQ_ID), IO_SQ_TAIL);
    }
    true
}

fn io_wait_completion(cid: u16) -> bool {
    unsafe {
        let cq = &IO_CQ as *const QueueBuf as *const NvmeCqe;
        for _ in 0..10000 {
            let idx = (IO_CQ_HEAD % IO_QUEUE_SIZE) as usize;
            let phase = (IO_CQ_HEAD / IO_QUEUE_SIZE) & 1;
            let entry = &*cq.add(idx);
            let entry_phase = entry.cdw0 & 1;
            if entry_phase == phase && entry.cid == cid {
                IO_CQ_HEAD += 1;
                reg_write(cq_hdbl(IO_CQ_ID), IO_CQ_HEAD);
                return (entry.status & 1) == 0;
            }
        }
    }
    false
}

fn io_submit_write(slba: u64, prp1: u64, num_blocks: u16, cid: u16) -> bool {
    unsafe {
        let sq = &mut IO_SQ as *mut QueueBuf as *mut NvmeSqe;
        let idx = (IO_SQ_TAIL % IO_QUEUE_SIZE) as usize;
        let cmd = &mut *sq.add(idx);
        cmd.cdw0 = (NVM_WRITE as u32) | (cid as u32) << 16;
        cmd.nsid = NSID;
        cmd.prp1 = prp1;
        cmd.prp2 = 0;
        cmd.cdw10 = (slba & 0xFFFF_FFFF) as u32;
        cmd.cdw11 = (slba >> 32) as u32;
        cmd.cdw12 = (num_blocks.wrapping_sub(1)) as u32;
        IO_SQ_TAIL += 1;
        reg_write(sq_tdbl(IO_SQ_ID), IO_SQ_TAIL);
    }
    true
}

/// Return whether the NVMe driver is initialized (root device ready).
pub fn is_initialized() -> bool {
    unsafe { INITIALIZED }
}

pub struct NvmeBlockDevice;

impl BlockDevice for NvmeBlockDevice {
    fn read_blocks(&self, block_offset: u64, buf: &mut [u8]) -> bool {
        if !unsafe { INITIALIZED } {
            return false;
        }
        let block_size = unsafe { BLOCK_SIZE } as usize;
        if buf.len() % block_size != 0 {
            return false;
        }
        let num_blocks = (buf.len() / block_size) as u16;
        if num_blocks == 0 {
            return true;
        }
        let ptr = buf.as_mut_ptr() as u64;
        let cid = 1;
        if !io_submit_read(block_offset, ptr, num_blocks, cid) {
            return false;
        }
        io_wait_completion(cid)
    }

    fn write_blocks(&self, block_offset: u64, buf: &[u8]) -> bool {
        if !unsafe { INITIALIZED } {
            return false;
        }
        let block_size = unsafe { BLOCK_SIZE } as usize;
        if buf.len() % block_size != 0 {
            return false;
        }
        let num_blocks = (buf.len() / block_size) as u16;
        if num_blocks == 0 {
            return true;
        }
        let ptr = buf.as_ptr() as u64;
        let cid = 1;
        if !io_submit_write(block_offset, ptr, num_blocks, cid) {
            return false;
        }
        io_wait_completion(cid)
    }
}

static NVME_DEVICE: NvmeBlockDevice = NvmeBlockDevice;

pub fn get_block_device() -> Option<&'static dyn BlockDevice> {
    if is_initialized() {
        Some(&NVME_DEVICE)
    } else {
        None
    }
}
