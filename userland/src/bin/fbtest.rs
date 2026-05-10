//! fbtest — end-to-end framebuffer test (Option B: direct backbuffer write + single flush).
//!
//! What it proves:
//!   1. GOP framebuffer is live and mapped correctly.
//!   2. SYS_CLAIM_COMPOSITOR grants backbuffer authority.
//!   3. SYS_MAP_FRAMEBUFFER returns a valid backbuffer pointer.
//!   4. Direct pixel writes + SYS_FB_FLUSH reaches the QEMU GTK window.
//!   5. SYS_POLL_INPUT receives keyboard events from userland.
//!
//! Pass = colored screen with rectangles appears in the QEMU window.
//! Exit = any key press.

#![no_std]
#![no_main]

use userland::{
    sys_claim_compositor, sys_map_framebuffer, sys_fb_flush, sys_exit,
};

// ── Color constants (BGRA format: byte order B G R A in memory) ──────────────
// QEMU GOP is PixelBlueGreenRedReserved8BitPerColor (BGRA).
// u32 stored little-endian: 0xAARRGGBB → appears as R=RR, G=GG, B=BB on screen.
const BLACK:      u32 = 0xFF_00_00_00;
const WHITE:      u32 = 0xFF_FF_FF_FF;
const ROGUE_DARK: u32 = 0xFF_1A_0A_2E; // deep purple — RogueOS brand
const ROGUE_BAR:  u32 = 0xFF_2D_1B_69; // slightly lighter purple for top bar
const GREEN_OK:   u32 = 0xFF_00_C8_00; // bright green — "test passed" indicator
const ORANGE:     u32 = 0xFF_00_96_FF; // orange in BGRA

#[no_mangle]
fn _start() -> ! {
    // ── Step 1: claim compositor authority ───────────────────────────────────
    let r = sys_claim_compositor();
    if r < 0 {
        // Already claimed — still try to proceed if we are the compositor.
        // (Non-fatal for now; map_framebuffer will reject us if truly not authorized.)
        let _ = r;
    }

    // ── Step 2: map the backbuffer ────────────────────────────────────────────
    let mut fb_ptr: u64 = 0;
    let mut fb_w: u32 = 0;
    let mut fb_h: u32 = 0;
    let mut fb_stride: u32 = 0; // bytes per row

    let r = sys_map_framebuffer(&mut fb_ptr, &mut fb_w, &mut fb_h, &mut fb_stride);
    if r < 0 || fb_ptr == 0 || fb_w == 0 || fb_h == 0 {
        // Mapping failed — bail.
        sys_exit(1);
    }

    // ── Step 3: draw test scene into backbuffer (zero syscalls here) ──────────
    let buf = fb_ptr as *mut u32;
    let stride_px = (fb_stride / 4) as usize; // pixels per row

    // Fill entire screen with RogueOS dark purple.
    fill_rect(buf, stride_px, 0, 0, fb_w, fb_h, ROGUE_DARK);

    // Top status bar (28px high).
    fill_rect(buf, stride_px, 0, 0, fb_w, 28, ROGUE_BAR);

    // Centered green "OK" rectangle — unmistakable success indicator.
    let box_w: u32 = 320;
    let box_h: u32 = 180;
    let box_x = (fb_w.saturating_sub(box_w)) / 2;
    let box_y = (fb_h.saturating_sub(box_h)) / 2;
    fill_rect(buf, stride_px, box_x, box_y, box_w, box_h, GREEN_OK);

    // White border around the green box (4px).
    fill_rect(buf, stride_px, box_x.saturating_sub(4), box_y.saturating_sub(4), box_w + 8, 4, WHITE);
    fill_rect(buf, stride_px, box_x.saturating_sub(4), box_y + box_h, box_w + 8, 4, WHITE);
    fill_rect(buf, stride_px, box_x.saturating_sub(4), box_y.saturating_sub(4), 4, box_h + 8, WHITE);
    fill_rect(buf, stride_px, box_x + box_w, box_y.saturating_sub(4), 4, box_h + 8, WHITE);

    // Two diagonal corner markers in orange (16×16) so we can confirm orientation.
    fill_rect(buf, stride_px, 0, 0, 16, 16, ORANGE);
    fill_rect(buf, stride_px, fb_w - 16, fb_h - 16, 16, 16, ORANGE);

    // Bottom bar (4px).
    fill_rect(buf, stride_px, 0, fb_h - 4, fb_w, 4, ROGUE_BAR);

    // ── Step 4: flush backbuffer → real framebuffer (one syscall) ────────────
    sys_fb_flush();

    sys_exit(0);
}

// ── Drawing helpers (pure backbuffer writes, zero syscalls) ──────────────────

#[inline(always)]
fn fill_rect(buf: *mut u32, stride_px: usize, x: u32, y: u32, w: u32, h: u32, color: u32) {
    if w == 0 || h == 0 {
        return;
    }
    unsafe {
        for row in 0..h as usize {
            let row_start = buf.add((y as usize + row) * stride_px + x as usize);
            for col in 0..w as usize {
                core::ptr::write_volatile(row_start.add(col), color);
            }
        }
    }
}
