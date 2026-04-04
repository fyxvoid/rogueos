//! Loader: ELF load into address space.

mod elf;

pub use elf::{load_elf, LoadResult};
