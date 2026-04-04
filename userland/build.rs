use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    println!("cargo:rustc-link-arg=-T{}", manifest.join("linker.ld").display());
}
