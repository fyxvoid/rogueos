const LINE_MAX: usize = 256;

/// Embedded legacy steward init (kept as fallback).
const INIT_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/init.elf"));
/// Cogman supervisor init (replaces steward as primary first-userland process).
const COGMAN_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cogman.elf"));
/// Embedded shell ELF (built by build.rs from userland --bin shell).
const SHELL_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shell.elf"));
/// Unified session ELF (built by build.rs from userland --bin session).
const SESSION_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/session.elf"));
/// Embedded window manager ELF (built by build.rs from userland --bin wm).
const WM_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wm.elf"));
/// RogueWM ELF — rwm-core powered WM (program id 1, spawned by init).
const RWM_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/rwm.elf"));
/// Embedded editor ELF (built by build.rs from userland --bin editor).
const EDITOR_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/editor.elf"));
/// Embedded viewer ELF (built by build.rs from userland --bin viewer).
const VIEWER_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/viewer.elf"));
/// Embedded copy ELF (built by build.rs from userland --bin copy).
const COPY_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/copy.elf"));
/// Embedded monitor ELF (built by build.rs from userland --bin monitor).
const MONITOR_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/monitor.elf"));
/// Embedded shutdown ELF (built by build.rs from userland --bin shutdown).
const SHUTDOWN_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shutdown.elf"));
/// Embedded exit ELF (stress test: binary that only SYS_EXIT(0)).
const EXIT_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/exit.elf"));

/// Set true to run stress: 10 sequential exit processes before init. System halts after 10th exit.
const STRESS_EXIT_FIRST: bool = false;

#[no_mangle]
pub extern "sysv64" fn kernel_main(bootinfo: *const libs::BootInfo) -> ! {
    unsafe { core::arch::asm!("cli"); }
    // Gatehouse jumps here directly; set stack to kernel image so paging init (CR3 switch) has mapped stack.
    let stack_top = crate::stack_bounds::kernel_stack_bounds().1;
    unsafe {
        core::arch::asm!(
            "mov rsp, {}",
            in(reg) stack_top,
            options(nostack),
        );
    }

    crate::arch::x86_64::serial::init();
    crate::arch::serial::write_str("[KRN] kernel_main_entry\r\n");

    // Phase 0: IDT + GDT FIRST — before any paging/allocation that could fault.
    // OVMF's stale IDT would triple-fault if a page fault occurs before we install ours.
    crate::arch::serial::write_str("[KRN] step0: idt_gdt_init\r\n");
    crate::arch::x86_64::idt::init();
    crate::arch::x86_64::gdt::init();
    crate::arch::x86_64::msr::init_syscall_msrs(
        crate::arch::x86_64::syscall_entry::syscall_entry as *const () as u64,
    );
    // Enable CPU security: CR0.WP, CR4.SMEP, CR4.UMIP (CPUID-gated).
    crate::arch::x86_64::cpuid::init_cpu_security();
    crate::arch::serial::write_str("[KRN] step0: idt_gdt_syscall_ready\r\n");

    // Phase 0b: AMD SME — enable memory encryption BEFORE paging so C-bit is
    // available when page tables are built. Unique feature: no other OS enables
    // SME by default.
    crate::arch::serial::write_str("[KRN] step0b: sme_init\r\n");
    let _sme_active = crate::arch::x86_64::sme::init();
    crate::arch::serial::write_str("[KRN] step0b: sme_done\r\n");

    if bootinfo.is_null() {
        crate::arch::serial::write_str("[KRN] bootinfo ptr is null\r\n");
        loop {
            unsafe { core::arch::asm!("hlt"); }
        }
    }

    // Copy BootInfo to a kernel static BEFORE switching CR3. The bootloader places
    // BootInfo at BOOTINFO_PHYS_ADDR (0x8000) which is outside the kernel identity map
    // (0x100000+). After paging::init() switches CR3, 0x8000 is no longer mapped.
    // Keeping a kernel-space copy lets us pass `bi` to drivers after the switch.
    static mut BOOT_INFO_COPY: libs::BootInfo = unsafe {
        core::mem::zeroed()
    };
    let bi: &libs::BootInfo = unsafe {
        BOOT_INFO_COPY = *bootinfo;
        &BOOT_INFO_COPY
    };

    crate::arch::serial::write_str("[KRN] mem_map_valid=");
    crate::arch::serial::write_hex(bi.mem_map_valid as u64);
    crate::arch::serial::write_str(" rsdp=");
    crate::arch::serial::write_hex(bi.rsdp_addr);
    crate::arch::serial::write_str(" smbios=");
    crate::arch::serial::write_hex(bi.smbios_addr);
    crate::arch::serial::write_str(" rt_services=");
    crate::arch::serial::write_hex(bi.runtime_services_addr);
    crate::arch::serial::write_str("\r\n");

    // Init physical allocator from UEFI memory map (or use fixed region when BootInfo map invalid).
    if bi.mem_map_valid == 0xC0DEF00D {
        if crate::memory::physical::init_from_bootinfo(bi) {
            crate::arch::serial::write_str("[KRN] physical_init_from_bootinfo_ok\r\n");
        } else {
            crate::arch::serial::write_str("[KRN] physical_init_from_bootinfo_failed, using fixed region\r\n");
            crate::memory::physical::init();
        }
    } else {
        crate::arch::serial::write_str("[KRN] BootInfo memory map invalid, using fixed region\r\n");
        crate::memory::physical::init();
    }

    // Paging init: identity-map frame region so alloc_frame-backed mappings are accessible.
    // bi is now a reference into BOOT_INFO_COPY (kernel BSS, identity-mapped) — safe after CR3 switch.
    crate::memory::paging::init();
    crate::arch::serial::write_str("[KRN] paging_init_done\r\n");

    // Phase 1: TTY on top of serial + PS/2 keyboard.
    crate::arch::serial::write_str("[KRN] step1: tty_ps2_init\r\n");
    crate::drivers::tty::init();
    crate::arch::x86_64::ps2::init();
    crate::arch::serial::write_str("[KRN] step1: tty_ps2_ready\r\n");

    // Phase 2: heap + page-fault PID hook.
    crate::arch::serial::write_str("[KRN] step2: heap_init\r\n");
    crate::memory::heap::heap_init();
    crate::memory::paging::fault::set_page_fault_pid_fn(crate::process::current_pid_for_fault);
    crate::arch::serial::write_str("[KRN] step2: heap_ready\r\n");

    // Phase 2b: AMD PMU — enable CR4.PCE so ring-3 can RDPMC directly.
    crate::arch::serial::write_str("[KRN] step2b: pmu_init\r\n");
    crate::arch::x86_64::perf::init();
    crate::arch::serial::write_str("[KRN] step2b: pmu_ready\r\n");

    // Phase 3: NVMe from BootInfo.
    crate::arch::serial::write_str("[KRN] step3: nvme_init\r\n");
    let nvme_ready = crate::drivers::nvme::init_from_boot_info(bi);
    if nvme_ready {
        crate::arch::serial::write_str("[KRN] step3: nvme_ready\r\n");
    } else {
        crate::arch::serial::write_str("[KRN] step3: nvme_absent\r\n");
    }

    // Phase 4: simple_fs root mount.
    crate::arch::serial::write_str("[KRN] step4: mount_root\r\n");
    let root_ok = nvme_ready && crate::fs::mount_root();
    if root_ok {
        crate::arch::serial::write_str("[KRN] step4: mount_root_ok\r\n");
    } else {
        crate::arch::serial::write_str("[KRN] step4: mount_root_skipped\r\n");
    }

    // Phase 5: framebuffer init + test pattern.
    crate::arch::serial::write_str("[KRN] step5: fb_init\r\n");
    let fb_ok = crate::drivers::framebuffer::init_from_boot_info(bi);
    if fb_ok {
        crate::arch::serial::write_str("[KRN] step5: fb_ready\r\n");
        crate::drivers::framebuffer::draw_test_pattern();
        crate::arch::serial::write_str("[KRN] step5: fb_test_pattern_drawn\r\n");
    } else {
        crate::arch::serial::write_str("[KRN] step5: fb_failed\r\n");
    }

    // Phase 6: register embedded userland programs.
    // ID assignments must match program_id constants in userland/src/bin/cogman.rs.
    crate::arch::serial::write_str("[KRN] step6: register_programs\r\n");
    crate::kernel::programs::register(0, SHELL_ELF);      // PROG_SHELL
    crate::kernel::programs::register(1, RWM_ELF);        // PROG_RWM
    crate::kernel::programs::register(2, EDITOR_ELF);     // PROG_EDITOR
    crate::kernel::programs::register(3, VIEWER_ELF);     // PROG_VIEWER
    crate::kernel::programs::register(4, COPY_ELF);       // PROG_COPY
    crate::kernel::programs::register(5, MONITOR_ELF);    // PROG_MONITOR
    crate::kernel::programs::register(6, SHUTDOWN_ELF);   // PROG_SHUTDOWN
    crate::kernel::programs::register(7, EXIT_ELF);       // PROG_EXIT
    crate::kernel::programs::register(8, SESSION_ELF);    // PROG_SESSION
    crate::kernel::programs::register(9, WM_ELF);         // PROG_WM_LEGACY
    crate::kernel::programs::register(10, COGMAN_ELF);    // PROG_COGMAN (self-spawn slot)

    // Stress (optional): 10 sequential short-lived exit processes.
    if STRESS_EXIT_FIRST {
        crate::arch::serial::write_str("[KRN] step6b: stress_exit (10 processes)\r\n");
        let mut indices: [usize; 10] = [0; 10];
        let mut n = 0usize;
        if let Some(exit_elf) = crate::kernel::programs::get_elf(7) {
            if exit_elf.len() >= 4 && exit_elf[0..4] == [0x7f, b'E', b'L', b'F'] {
                while n < 10 {
                    match crate::process::create_user_process(exit_elf, crate::capability::CapSet::none()) {
                        Some(idx) => {
                            indices[n] = idx;
                            n += 1;
                        }
                        None => break,
                    }
                }
            }
        } else {
            crate::arch::serial::write_str("[KRN] step6b: exit_elf missing\r\n");
        }
        if n == 10 {
            crate::arch::serial::write_str("[KRN] step6b: running 10 exit processes in sequence\r\n");
            crate::process::run_first_process(indices[0]);
        } else {
            crate::arch::serial::write_str("[KRN] step6b: stress_exit skipped (slots or elf)\r\n");
        }
    }

    // Step 7: spawn cogman as the first userland process (init replacement).
    // Falls back to the legacy steward init if cogman ELF is empty (not yet built).
    crate::arch::serial::write_str("[KRN] step7: cogman_spawn\r\n");
    let first_elf: &[u8] = if COGMAN_ELF.len() >= 4 && COGMAN_ELF[0..4] == [0x7f, b'E', b'L', b'F'] {
        crate::arch::serial::write_str("[KRN] step7: using cogman as init\r\n");
        COGMAN_ELF
    } else {
        crate::arch::serial::write_str("[KRN] step7: cogman not ready, falling back to steward init\r\n");
        INIT_ELF
    };
    // Cogman (init) is born with ALL capabilities — it is the root of the
    // capability tree and the only process that can grant/revoke caps.
    match crate::process::create_user_process(first_elf, crate::capability::CapSet::all()) {
        Some(idx) => {
            crate::arch::serial::write_str("[KRN] step7: init_idx=");
            crate::arch::serial::write_hex(idx as u64);
            crate::arch::serial::write_str("\r\n");
            crate::process::run_first_process(idx);
        }
        None => {
            crate::arch::serial::write_str(
                "[KRN] step7: init_spawn_failed; falling back to kernel TTY shell\r\n",
            );
            crate::arch::serial::write_str("[KRN] TTY shell ready\r\n");
            kernel_shell();
        }
    }
}

/// In-kernel TTY shell: prompt, line read, built-in commands.
fn kernel_shell() -> ! {
    crate::drivers::tty::write_str("\r\nCrown kernel TTY shell. Type 'help' for commands.\r\n");
    let mut line = [0u8; LINE_MAX];
    loop {
        crate::drivers::tty::write_str("> ");
        let n = crate::drivers::tty::getline(&mut line);
        if n == 0 {
            continue;
        }
        let s = match core::str::from_utf8(&line[..n]) {
            Ok(x) => x.trim(),
            Err(_) => continue,
        };
        if s.is_empty() {
            continue;
        }
        match s {
            "help" => {
                crate::drivers::tty::write_str("  help   - show this message\r\n");
                crate::drivers::tty::write_str("  clear  - clear screen (serial)\r\n");
                crate::drivers::tty::write_str("  echo <text> - echo rest of line\r\n");
            }
            "clear" => {
                crate::drivers::tty::write_str("\r\n");
            }
            _ if s.starts_with("echo ") => {
                crate::drivers::tty::write_str(&s[5..]);
                crate::drivers::tty::write_str("\r\n");
            }
            _ => {
                crate::drivers::tty::write_str("unknown command: ");
                crate::drivers::tty::write_str(s);
                crate::drivers::tty::write_str("\r\n");
            }
        }
    }
}
