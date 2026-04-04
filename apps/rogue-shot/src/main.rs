//! rogue-shot — Capture screen (full or region) to PNG.
//! X11: x11-screenshot. Wayland: not implemented (run under XWayland).
//! Use -s/--selection with no args for interactive region (draw with mouse).

use std::io::{self, Write};
use std::path::PathBuf;
use std::process;

use image::{EncodableLayout, ImageEncoder};

fn main() {
    if let Err(e) = run() {
        eprintln!("rogue-shot: {}", e);
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut region: Option<(i32, i32, u32, u32)> = None;
    let mut interactive_selection = false;
    let mut output: Option<PathBuf> = None;

    while let Some(a) = args.next() {
        match a.as_str() {
            "-s" | "--selection" => {
                let next = args.next();
                if let Some(x_str) = next {
                    let x: i32 = x_str.parse().map_err(|_| "region x must be integer")?;
                    let y: i32 = args.next().ok_or("region requires x y width height")?.parse().map_err(|_| "region y must be integer")?;
                    let w: u32 = args.next().ok_or("region requires x y width height")?.parse().map_err(|_| "region width must be integer")?;
                    let h: u32 = args.next().ok_or("region requires x y width height")?.parse().map_err(|_| "region height must be integer")?;
                    region = Some((x, y, w, h));
                } else {
                    interactive_selection = true;
                }
            }
            "-o" | "--output" => {
                output = Some(args.next().ok_or("rogue-shot: -o requires path")?.into());
            }
            "-h" | "--help" => print_help(),
            "-V" | "--version" => print_version(),
            v if v.starts_with('-') => return Err(format!("unknown option: {}", v)),
            _ => return Err(format!("unexpected argument: {}", a)),
        }
    }

    if interactive_selection {
        region = Some(interactive_region_x11()?);
    }

    if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("DISPLAY").is_none() {
        return Err("Wayland capture not implemented; run under XWayland (DISPLAY=:0) or use a Wayland-native capture tool".to_string());
    }

    let screen = x11_screenshot::Screen::open().ok_or("could not open X11 display")?;
    let image = if let Some((x, y, w, h)) = region {
        if w == 0 || h == 0 {
            return Err("region width and height must be positive".to_string());
        }
        screen.capture_area(w, h, x, y).ok_or("capture_area failed")?
    } else {
        screen.capture().ok_or("capture failed")?
    };

    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(
            image.as_bytes(),
            image.width(),
            image.height(),
            image::ColorType::Rgb8,
        )
        .map_err(|e: image::ImageError| e.to_string())?;

    if let Some(path) = output {
        std::fs::write(&path, &buf).map_err(|e| e.to_string())?;
    } else {
        io::stdout().write_all(&buf).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Interactive region selection: grab pointer, wait for button press then release, return (x, y, w, h).
#[allow(unused_assignments)] // end_x/end_y updated in loop; only final values used
fn interactive_region_x11() -> Result<(i32, i32, u32, u32), String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::*;

    let (conn, screen_num) = x11rb::connect(None).map_err(|e| format!("X11 connect: {}", e))?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;

    // Grab pointer (async so we can receive events)
    conn.grab_pointer(
        false,
        root,
        EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
        x11rb::NONE,
        x11rb::NONE,
        x11rb::CURRENT_TIME,
    )
    .map_err(|e| format!("X11 grab_pointer: {:?}", e))?
    .reply()
    .map_err(|e| format!("X11 grab_pointer reply: {:?}", e))?;

    let mut start_x: i32 = 0;
    let mut start_y: i32 = 0;
    let mut end_x: i32 = 0;
    let mut end_y: i32 = 0;
    let mut pressed = false;

    loop {
        let event = conn.wait_for_event().map_err(|e| format!("X11 wait_for_event: {}", e))?;
        match event {
            x11rb::protocol::Event::ButtonPress(ev) => {
                start_x = ev.event_x as i32;
                start_y = ev.event_y as i32;
                end_x = ev.event_x as i32;
                end_y = ev.event_y as i32;
                pressed = true;
            }
            x11rb::protocol::Event::ButtonRelease(ev) => {
                if pressed {
                    end_x = ev.event_x as i32;
                    end_y = ev.event_y as i32;
                    break;
                }
            }
            x11rb::protocol::Event::MotionNotify(ev) => {
                if pressed {
                    end_x = ev.event_x as i32;
                    end_y = ev.event_y as i32;
                }
            }
            _ => {}
        }
    }

    conn.ungrab_pointer(x11rb::CURRENT_TIME)
        .map_err(|e| format!("X11 ungrab_pointer: {:?}", e))?;
    conn.flush().map_err(|e| format!("X11 flush: {}", e))?;

    let x = start_x.min(end_x);
    let y = start_y.min(end_y);
    let w = (start_x - end_x).unsigned_abs();
    let h = (start_y - end_y).unsigned_abs();

    if w < 2 || h < 2 {
        return Err("selection too small (drag to draw a rectangle)".to_string());
    }

    Ok((x, y, w, h))
}

fn print_help() -> ! {
    eprintln!(
        r#"rogue-shot — Capture screen (full or region) to PNG.
Usage:
  rogue-shot [OPTIONS]                    Capture full screen to stdout (PNG).
  rogue-shot -s|--selection [x y w h]     Capture region (x,y = top-left, w,h = size).
  rogue-shot -s|--selection               Interactive: drag to select region.
  rogue-shot -o PATH                      Write PNG to file.

Options:
  -s, --selection [x y w h]   Region: 4 integers, or omit for interactive.
  -o, --output PATH           Write to file instead of stdout.
  -h, --help                  Show this help.
  -V, --version               Show version.

Exit codes: 0 success, 1 error.
"#
    );
    process::exit(0);
}

fn print_version() -> ! {
    eprintln!("rogue-shot {}", env!("CARGO_PKG_VERSION"));
    process::exit(0);
}
