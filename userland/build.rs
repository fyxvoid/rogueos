// Linker script selection is handled per-binary via RUSTFLAGS in the Makefile.
// Each binary gets its own ldscripts/<name>.ld passed as -C link-arg=-T<path>.
// This build script intentionally emits nothing to avoid overriding those scripts.
fn main() {}
