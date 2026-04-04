//! Smoke test: run rogue-shot --help so the binary does not regress.

#[test]
fn help_exits_success() {
    let exe = std::env::var("CARGO_BIN_EXE_rogue-shot").or_else(|_| std::env::var("CARGO_BIN_EXE_rogue_shot"));
    let exe = match exe {
        Ok(path) => path,
        Err(_) => return, // not running as cargo test for this binary
    };
    let status = std::process::Command::new(exe).arg("--help").status().expect("run rogue-shot --help");
    assert!(status.success(), "rogue-shot --help should exit 0");
}
