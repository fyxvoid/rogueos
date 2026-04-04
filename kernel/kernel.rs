//! Kernel facade module: re-exports core entrypoints and diagnostics.
//!
//! This provides a stable `crate::kernel::*` surface for the rest of the
//! codebase, while the underlying implementation lives in `init`.

pub use crate::init::kernel_main;

pub mod programs {
    pub use crate::init::programs::*;
}

pub mod diagnostic {
    pub use crate::init::diagnostic::*;
}

