//! Misc syscalls: reboot, debug_dump_ptes, gettime.

use crate::syscall::user_ptr::{self, SysErr};

// ── CMOS RTC helpers ─────────────────────────────────────────────────────────

#[inline]
fn cmos_read(reg: u8) -> u8 {
    unsafe {
        // Write register index to 0x70, read value from 0x71.
        core::arch::asm!(
            "out 0x70, al",
            in("al") reg,
            options(nomem, nostack, preserves_flags)
        );
        let v: u8;
        core::arch::asm!(
            "in al, 0x71",
            out("al") v,
            options(nomem, nostack, preserves_flags)
        );
        v
    }
}

#[inline]
fn bcd_to_bin(v: u8) -> u8 {
    (v & 0x0F) + (v >> 4) * 10
}

/// Read the CMOS RTC and return a packed u64.
/// Packing: [year:16][month:8][day:8][hour:8][minute:8][second:8][pad:8].
/// Year is the full 4-digit year (e.g. 2026). All other fields are calendar values.
pub(super) fn sys_gettime(out: *mut u64) -> Result<u64, SysErr> {
    // Spin-wait until RTC is not in the middle of an update (Status A bit 7).
    for _ in 0..65536u32 {
        if cmos_read(0x0A) & 0x80 == 0 {
            break;
        }
    }

    let secs_raw  = cmos_read(0x00);
    let mins_raw  = cmos_read(0x02);
    let hours_raw = cmos_read(0x04);
    let day_raw   = cmos_read(0x07);
    let month_raw = cmos_read(0x08);
    let year_raw  = cmos_read(0x09);
    let century   = cmos_read(0x32); // ACPI century register (may be 0 on some hardware)
    let status_b  = cmos_read(0x0B);

    let is_bin = (status_b & 0x04) != 0; // bit 2: 0=BCD, 1=binary

    let secs  = if is_bin { secs_raw  } else { bcd_to_bin(secs_raw)  };
    let mins  = if is_bin { mins_raw  } else { bcd_to_bin(mins_raw)  };
    let hrs   = if is_bin { hours_raw } else { bcd_to_bin(hours_raw & 0x7F) };
    let day   = if is_bin { day_raw   } else { bcd_to_bin(day_raw)   };
    let month = if is_bin { month_raw } else { bcd_to_bin(month_raw) };
    let yr_2  = if is_bin { year_raw  } else { bcd_to_bin(year_raw)  };
    let cent  = if century != 0 {
        if is_bin { century } else { bcd_to_bin(century) }
    } else {
        20 // assume 21st century
    };
    let year  = (cent as u32) * 100 + (yr_2 as u32);

    let packed: u64 = ((year  as u64) << 40)
                    | ((month as u64) << 32)
                    | ((day   as u64) << 24)
                    | ((hrs   as u64) << 16)
                    | ((mins  as u64) << 8)
                    | (secs   as u64);

    if !out.is_null() {
        let cr3 = user_ptr::current_cr3()?;
        user_ptr::validate_user_range(cr3, out as u64, 8, true)?;
        unsafe { core::ptr::write_volatile(out, packed); }
    }
    Ok(packed)
}

pub(super) fn sys_reboot(mode: u32) -> Result<u64, SysErr> {
    let _ = crate::fs::flush_volume_header();
    match mode {
        0 => loop {
            crate::arch::halt();
        },
        1 => crate::arch::x86_64::reboot(),
        _ => Err(SysErr::INVAL),
    }
}

/// Debug: dump PTEs for va_start..va_end. Uses current process CR3 only (user-passed cr3 ignored).
pub(super) fn sys_debug_dump_ptes(_cr3: u64, va_start: u64, va_end: u64) -> Result<u64, SysErr> {
    let cr3 = user_ptr::current_cr3()?;
    crate::memory::paging::dump_ptes_range_serial(cr3, va_start, va_end);
    Ok(0)
}
