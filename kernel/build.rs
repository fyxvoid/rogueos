use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-env=KERNEL_VERSION={}", env!("CARGO_PKG_VERSION"));
    built::write_built_file().expect("built write");

    let target = std::env::var("TARGET").unwrap_or_default();
    let is_kernel_target = target == "x86_64-unknown-none";

    if is_kernel_target {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let use_multiboot2 = std::env::var("CARGO_FEATURE_MULTIBOOT2").is_ok();
        if use_multiboot2 {
            println!("cargo:rustc-link-arg=-T{}", manifest.join("linker_multiboot2.ld").display());
            let arch_dir = manifest.join("arch").join("x86_64");
            let boot_asm = arch_dir.join("boot_multiboot2.S");
            let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
            let obj = out_dir.join("boot_multiboot2.o");
            let status = std::process::Command::new("gcc")
                .args(["-c", boot_asm.to_str().unwrap(), "-o", obj.to_str().unwrap(), "-m64"])
                .status()
                .expect("run gcc for boot_multiboot2.S");
            if !status.success() {
                panic!("gcc failed compiling boot_multiboot2.S");
            }
            println!("cargo:rustc-link-arg={}", obj.display());
        } else {
            println!("cargo:rustc-link-arg=-T{}", manifest.join("linker.ld").display());
        }
    }

    // Workspace root: parent of kernel (system/) or repo root (when members are system/*).
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let w_system = manifest.join("..");
    let workspace = if w_system.join("target").exists() { w_system } else { manifest.join("../..") };
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let init_dst = out_dir.join("init.elf");
    let shell_dst = out_dir.join("shell.elf");
    let session_dst = out_dir.join("session.elf");
    let wm_dst = out_dir.join("wm.elf");
    let editor_dst = out_dir.join("editor.elf");
    let viewer_dst = out_dir.join("viewer.elf");
    let copy_dst = out_dir.join("copy.elf");
    let monitor_dst = out_dir.join("monitor.elf");
    let shutdown_dst = out_dir.join("shutdown.elf");
    let exit_dst = out_dir.join("exit.elf");
    let rwm_dst = out_dir.join("rwm.elf");
    let cogman_dst = out_dir.join("cogman.elf");
    let fbtest_dst   = out_dir.join("fbtest.elf");
    let terminal_dst = out_dir.join("terminal.elf");
    let nova_dst     = out_dir.join("nova.elf");
    if is_kernel_target {
        // Rebuild kernel when any embedded userland binary changes.
        for bin in &["init","shell","session","wm","rwm","editor","viewer","copy","monitor",
                     "shutdown","exit","cogman","fbtest","terminal","nova"] {
            println!("cargo:rerun-if-changed={}",
                workspace.join("target/x86_64-unknown-none/release").join(bin).display());
        }
        let init_src = workspace.join("target/x86_64-unknown-none/release/init");
        let shell_src = workspace.join("target/x86_64-unknown-none/release/shell");
        let session_src = workspace.join("target/x86_64-unknown-none/release/session");
        let wm_src = workspace.join("target/x86_64-unknown-none/release/wm");
        let editor_src = workspace.join("target/x86_64-unknown-none/release/editor");
        let viewer_src = workspace.join("target/x86_64-unknown-none/release/viewer");
        let copy_src = workspace.join("target/x86_64-unknown-none/release/copy");
        let monitor_src = workspace.join("target/x86_64-unknown-none/release/monitor");
        let shutdown_src = workspace.join("target/x86_64-unknown-none/release/shutdown");
        let exit_src = workspace.join("target/x86_64-unknown-none/release/exit");
        let rwm_src    = workspace.join("target/x86_64-unknown-none/release/rwm");
        let cogman_src = workspace.join("target/x86_64-unknown-none/release/cogman");
        let fbtest_src   = workspace.join("target/x86_64-unknown-none/release/fbtest");
        let terminal_src = workspace.join("target/x86_64-unknown-none/release/terminal");
        let nova_src     = workspace.join("target/x86_64-unknown-none/release/nova");
        if init_src.exists() {
            let _ = std::fs::copy(&init_src, &init_dst);
        } else {
            let _ = std::fs::write(&init_dst, &[]);
        }
        if shell_src.exists() {
            let _ = std::fs::copy(&shell_src, &shell_dst);
        } else {
            let _ = std::fs::write(&shell_dst, &[]);
        }
        if session_src.exists() {
            let _ = std::fs::copy(&session_src, &session_dst);
        } else {
            let _ = std::fs::write(&session_dst, &[]);
        }
        if wm_src.exists() {
            let _ = std::fs::copy(&wm_src, &wm_dst);
        } else {
            let _ = std::fs::write(&wm_dst, &[]);
        }
        if editor_src.exists() {
            let _ = std::fs::copy(&editor_src, &editor_dst);
        } else {
            let _ = std::fs::write(&editor_dst, &[]);
        }
        if viewer_src.exists() {
            let _ = std::fs::copy(&viewer_src, &viewer_dst);
        } else {
            let _ = std::fs::write(&viewer_dst, &[]);
        }
        if copy_src.exists() {
            let _ = std::fs::copy(&copy_src, &copy_dst);
        } else {
            let _ = std::fs::write(&copy_dst, &[]);
        }
        if monitor_src.exists() {
            let _ = std::fs::copy(&monitor_src, &monitor_dst);
        } else {
            let _ = std::fs::write(&monitor_dst, &[]);
        }
        if shutdown_src.exists() {
            let _ = std::fs::copy(&shutdown_src, &shutdown_dst);
        } else {
            let _ = std::fs::write(&shutdown_dst, &[]);
        }
        if exit_src.exists() {
            let _ = std::fs::copy(&exit_src, &exit_dst);
        } else {
            let _ = std::fs::write(&exit_dst, &[]);
        }
        if rwm_src.exists() {
            let _ = std::fs::copy(&rwm_src, &rwm_dst);
        } else {
            let _ = std::fs::write(&rwm_dst, &[]);
        }
        if cogman_src.exists() {
            let _ = std::fs::copy(&cogman_src, &cogman_dst);
        } else {
            let _ = std::fs::write(&cogman_dst, &[]);
        }
        if fbtest_src.exists() {
            let _ = std::fs::copy(&fbtest_src, &fbtest_dst);
        } else {
            let _ = std::fs::write(&fbtest_dst, &[]);
        }
        if terminal_src.exists() {
            let _ = std::fs::copy(&terminal_src, &terminal_dst);
        } else {
            let _ = std::fs::write(&terminal_dst, &[]);
        }
        if nova_src.exists() {
            let _ = std::fs::copy(&nova_src, &nova_dst);
        } else {
            let _ = std::fs::write(&nova_dst, &[]);
        }
    } else {
        let _ = std::fs::write(&init_dst, &[]);
        let _ = std::fs::write(&shell_dst, &[]);
        let _ = std::fs::write(&session_dst, &[]);
        let _ = std::fs::write(&wm_dst, &[]);
        let _ = std::fs::write(&editor_dst, &[]);
        let _ = std::fs::write(&viewer_dst, &[]);
        let _ = std::fs::write(&copy_dst, &[]);
        let _ = std::fs::write(&monitor_dst, &[]);
        let _ = std::fs::write(&shutdown_dst, &[]);
        let _ = std::fs::write(&exit_dst, &[]);
        let _ = std::fs::write(&rwm_dst, &[]);
        let _ = std::fs::write(&cogman_dst, &[]);
        let _ = std::fs::write(&fbtest_dst, &[]);
        let _ = std::fs::write(&terminal_dst, &[]);
        let _ = std::fs::write(&nova_dst, &[]);
    }
}
