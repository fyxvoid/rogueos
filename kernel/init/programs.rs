//! Program registry for SYS_SPAWN: program_id -> embedded ELF.

// 0=shell, 1=rwm, 2=editor, 3=viewer, 4=copy, 5=monitor, 6=shutdown, 7=exit,
// 8=session, 9=wm-legacy, 10=cogman (self-spawn slot), 11=fbtest, 12=terminal, 13=nova
const MAX_PROGRAMS: usize = 14;
static mut PROGRAMS: [Option<&'static [u8]>; MAX_PROGRAMS] = [None; MAX_PROGRAMS];

/// Register an ELF for the given program id. Called from kernel_main at init.
pub fn register(program_id: u32, elf: &'static [u8]) {
    let id = program_id as usize;
    if id < MAX_PROGRAMS {
        unsafe {
            PROGRAMS[id] = Some(elf);
        }
    }
}

/// Get the ELF for the given program id (0=shell, 1=wm, etc.).
pub fn get_elf(program_id: u32) -> Option<&'static [u8]> {
    let id = program_id as usize;
    if id >= MAX_PROGRAMS {
        return None;
    }
    unsafe { PROGRAMS[id] }
}
