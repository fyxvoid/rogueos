# Building Xorg from Source

This document describes how to build the X Window System from source for use with Rogue Desktop (e.g. testing or a custom install prefix).

## Prerequisites

Install the tools and libraries required by the [X.org build process](https://www.x.org/wiki/Building_the_X_Window_System/). Typical requirements include:

- **GNU build system:** git, pkg-config (≥0.22), libtool (≥2.2), gmake, autoconf (≥2.62), automake (≥1.11), autopoint
- **Development:** gcc, flex, bison, m4, gettext, gperf
- **Libraries:** libudev, libxcb, libX11, libXext, pixman, mesa (and development headers)
- **Crypto:** OpenSSL or libgcrypt (for the X server)
- **Fonts:** freetype, fontconfig

### Debian / Ubuntu

```bash
sudo apt-get build-dep xserver-xorg
sudo apt-get install git pkg-config libtool autoconf automake
```

### Arch Linux

```bash
sudo pacman -S base-devel git xorg-server xorg-xwayland
# Install deps for building xorg-server from source:
sudo pacman -S libxcb libx11 pixman mesa libxcvt
```

### Fedora

```bash
sudo dnf builddep xorg-x11-server
sudo dnf install git pkg-config libtool autoconf automake
```

Check [RequiredPackages](https://www.x.org/wiki/RequiredPackages/) for your distribution.

## Build steps

1. From the repo root:

   ```bash
   chmod +x scripts/build-xorg.sh
   ./scripts/build-xorg.sh
   ```

2. The script will:
   - Create `xorg-src/` and clone the official Xorg modular build script into `xorg-src/util/modular`
   - Use `xorg-src/build` as the build directory (sources and objects)
   - Install to `$HOME/xorg-install` by default (override with `PREFIX=/opt/rogue-xorg ./scripts/build-xorg.sh`)

3. Build time: building the full X stack (200+ packages) can take a long time. Ensure sufficient disk space and that all prerequisites are installed to avoid "build without error" failures.

## Using the built Xorg

After a successful build:

```bash
export PREFIX="${PREFIX:-$HOME/xorg-install}"
export PATH="$PREFIX/bin:$PATH"
export LD_LIBRARY_PATH="$PREFIX/lib:$LD_LIBRARY_PATH"
which Xorg   # should point to $PREFIX/bin/Xorg
```

Then start X (or your display manager) as usual. To avoid affecting your system X, use a separate prefix and run Xephyr or a test session with this `PATH`/`LD_LIBRARY_PATH`.

## Troubleshooting

- **Configure fails:** Install the missing library or tool indicated in the error (often a `-devel` or `-dev` package).
- **Build fails in a module:** Note the module name; fix the dependency or patch and re-run. The build script does not resume automatically; you may need to clean that module and re-run.
- **No space left:** The full build requires several GB. Use a dedicated partition or increase disk space.
