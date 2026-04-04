# DWM C source (suckless)

This directory holds the official [dwm](https://dwm.suckless.org/) (dynamic window manager) C source for reference and comparison.

## Fetch source

From repo root:

```bash
./scripts/fetch-dwm.sh
```

This clones (or updates) from `git.suckless.org/dwm` into this directory, pinned to tag **6.4**.

## Build

```bash
make clean
make
sudo make install
```

Edit `config.mk` for paths (PREFIX, etc.) and `config.h` for keybinds, tags, and rules before building. dwm is configured at compile time.

## Rust port

See **dwm-rs/** in this repo for a direct C-to-Rust port (x11rb) of dwm for parity testing.
