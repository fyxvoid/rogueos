//! rogue-clip — Copy stdin to clipboard, or paste clipboard to stdout.

use std::process;
use rogue_clip::{parse_args, run};

fn main() {
    let bin = "rogue-clip";
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (operation, selection, mime) = match parse_args(bin, &args) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}: {}", bin, e);
            eprintln!("Try '{} --help' for usage.", bin);
            process::exit(1);
        }
    };
    if let Err(e) = run(operation, selection, mime) {
        eprintln!("{}: {}", bin, e);
        process::exit(1);
    }
}
