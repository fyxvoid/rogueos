//! rogue-clip / rogue-clipboard — Copy stdin to clipboard, paste to stdout.
//! Prefers Wayland when WAYLAND_DISPLAY is set, else X11.
//! Supports --primary/--clipboard on X11, --clear, and image/png for screenshots.

use std::io::{self, Read, Write};

/// X11 selection target: PRIMARY (mouse selection) or CLIPBOARD (Ctrl+C).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum X11Selection {
    #[default]
    Clipboard,
    Primary,
}

/// Operation: copy stdin to clipboard, paste to stdout, or clear.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Copy,
    Paste,
    Clear,
}

/// MIME type for copy (text or image/png for screenshots).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum MimeType {
    #[default]
    Text,
    ImagePng,
}

/// Parse CLI args. Returns (operation, X11 selection, mime type).
/// Exits with --help/--version; returns error for invalid args.
pub fn parse_args(bin_name: &str, args: &[String]) -> Result<(Operation, X11Selection, MimeType), String> {
    let mut paste = false;
    let mut clear = false;
    let mut primary = false;
    let mut _clipboard = false;
    let mut image = false;

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-h" | "--help" => print_help(bin_name),
            "-V" | "--version" => print_version(bin_name),
            "-p" | "--paste" => paste = true,
            "-c" | "--clear" => clear = true,
            "--primary" => primary = true,
            "--clipboard" => _clipboard = true,
            "-t" | "--type" => {
                i += 1;
                let typ = args.get(i).ok_or("--type requires an argument (text or image/png)")?;
                match typ.as_str() {
                    "text" | "text/plain" => {}
                    "image/png" | "png" => image = true,
                    _ => return Err(format!("unsupported type: {}", typ)),
                }
            }
            _ => return Err(format!("unexpected argument: {}", a)),
        }
        i += 1;
    }

    if clear && paste {
        return Err("cannot use --clear and --paste together".into());
    }

    let operation = if clear {
        Operation::Clear
    } else if paste {
        Operation::Paste
    } else {
        Operation::Copy
    };

    let selection = if primary {
        X11Selection::Primary
    } else {
        X11Selection::Clipboard
    };

    let mime = if image {
        MimeType::ImagePng
    } else {
        MimeType::Text
    };

    Ok((operation, selection, mime))
}

fn print_help(bin_name: &str) -> ! {
    eprintln!(
        r#"{} — copy stdin to clipboard, or paste clipboard to stdout.
Usage:
  {} [OPTIONS]              Copy stdin to clipboard (default: CLIPBOARD on X11).
  {} -p|--paste [OPTIONS]   Paste clipboard to stdout.
  {} -c|--clear [OPTIONS]  Clear the selection.

Options:
  -p, --paste       Paste instead of copy.
  -c, --clear       Clear the selection.
  --primary         Use X11 PRIMARY selection (mouse selection). Default is CLIPBOARD.
  --clipboard       Use X11 CLIPBOARD selection (explicit, default).
  -t, --type TYPE   MIME type for copy: text (default) or image/png.
  -h, --help        Show this help.
  -V, --version     Show version.

Environment:
  WAYLAND_DISPLAY   If set, use Wayland clipboard. Otherwise X11.
  DISPLAY           X11 display (for X11 backend).

Exit codes: 0 success, 1 error (e.g. no display, clipboard unavailable).
"#,
        bin_name, bin_name, bin_name, bin_name
    );
    std::process::exit(0);
}

fn print_version(bin_name: &str) -> ! {
    eprintln!("{} {}", bin_name, env!("CARGO_PKG_VERSION"));
    std::process::exit(0);
}

/// Run the requested operation. Uses Wayland if WAYLAND_DISPLAY is set, else X11.
pub fn run(operation: Operation, selection: X11Selection, mime: MimeType) -> Result<(), String> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();

    match operation {
        Operation::Copy => {
            if wayland {
                copy_wayland(mime)
            } else {
                copy_x11(selection, mime)
            }
        }
        Operation::Paste => {
            if wayland {
                paste_wayland(mime)
            } else {
                paste_x11(selection, mime)
            }
        }
        Operation::Clear => {
            if wayland {
                clear_wayland()
            } else {
                clear_x11(selection)
            }
        }
    }
}

fn copy_wayland(mime: MimeType) -> Result<(), String> {
    use wl_clipboard_rs::copy::{MimeType as WlMime, Options, Source};
    let mut stdin = Vec::new();
    io::stdin().read_to_end(&mut stdin).map_err(|e| format!("read stdin: {}", e))?;
    let wl_mime = match mime {
        MimeType::Text => WlMime::Autodetect,
        MimeType::ImagePng => WlMime::Specific("image/png".to_string()),
    };
    Options::new()
        .copy(Source::Bytes(stdin.into()), wl_mime)
        .map_err(|e| format!("Wayland copy failed: {}", e))?;
    Ok(())
}

fn paste_wayland(mime: MimeType) -> Result<(), String> {
    use wl_clipboard_rs::paste::{get_contents, ClipboardType, Error, MimeType as WlMime, Seat};
    let wl_mime = match mime {
        MimeType::Text => WlMime::Text,
        MimeType::ImagePng => WlMime::Specific("image/png"),
    };
    match get_contents(ClipboardType::Regular, Seat::Unspecified, wl_mime) {
        Ok((mut pipe, _)) => {
            let mut buf = Vec::new();
            pipe.read_to_end(&mut buf).map_err(|e| format!("read paste: {}", e))?;
            io::stdout().write_all(&buf).map_err(|e| format!("write stdout: {}", e))?;
            Ok(())
        }
        Err(Error::NoSeats) => Err("No Wayland seat available. Is WAYLAND_DISPLAY set correctly?".into()),
        Err(Error::ClipboardEmpty) | Err(Error::NoMimeType) => Ok(()), // empty: output nothing
        Err(e) => Err(format!("Wayland paste failed: {}", e)),
    }
}

fn clear_wayland() -> Result<(), String> {
    use wl_clipboard_rs::copy::{MimeType, Options, Source};
    Options::new()
        .copy(Source::Bytes(vec![].into()), MimeType::Text)
        .map_err(|e| format!("Wayland clear failed: {}", e))?;
    Ok(())
}

fn copy_x11(selection: X11Selection, _mime: MimeType) -> Result<(), String> {
    use x11_clipboard::Clipboard;
    let clip = Clipboard::new().map_err(|e| format!("X11 connection failed: {}. Is DISPLAY set?", e))?;
    let mut stdin = Vec::new();
    io::stdin().read_to_end(&mut stdin).map_err(|e| format!("read stdin: {}", e))?;
    let sel = match selection {
        X11Selection::Primary => clip.getter.atoms.primary,
        X11Selection::Clipboard => clip.getter.atoms.clipboard,
    };
    // X11: UTF8_STRING used for both text and image bytes (common for clipboard image)
    let target = clip.getter.atoms.utf8_string;
    clip.store(sel, target, stdin)
        .map_err(|e| format!("X11 store failed: {}", e))?;
    Ok(())
}

fn paste_x11(selection: X11Selection, _mime: MimeType) -> Result<(), String> {
    use x11_clipboard::Clipboard;
    let clip = Clipboard::new().map_err(|e| format!("X11 connection failed: {}. Is DISPLAY set?", e))?;
    let sel = match selection {
        X11Selection::Primary => clip.getter.atoms.primary,
        X11Selection::Clipboard => clip.getter.atoms.clipboard,
    };
    let target = clip.getter.atoms.utf8_string;
    let data = clip
        .load(sel, target, clip.getter.atoms.property, None::<std::time::Duration>)
        .map_err(|e| format!("X11 load failed: {} (clipboard may be empty or not text/image)", e))?;
    io::stdout().write_all(&data).map_err(|e| format!("write stdout: {}", e))?;
    Ok(())
}

fn clear_x11(selection: X11Selection) -> Result<(), String> {
    use x11_clipboard::Clipboard;
    let clip = Clipboard::new().map_err(|e| format!("X11 connection failed: {}. Is DISPLAY set?", e))?;
    let sel = match selection {
        X11Selection::Primary => clip.getter.atoms.primary,
        X11Selection::Clipboard => clip.getter.atoms.clipboard,
    };
    clip.store(sel, clip.getter.atoms.utf8_string, vec![])
        .map_err(|e| format!("X11 clear failed: {}", e))?;
    Ok(())
}
