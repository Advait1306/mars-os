# Spotlight

A macOS Spotlight-style application launcher for MarsOS (KDE Plasma on Wayland).

## What it does

Press **Meta** (Super/Windows key) to open a floating search bar centered near the top of the screen. Type to fuzzy-search installed applications, use arrow keys to navigate, and press Enter to launch.

## Architecture

```
spotlight          — Wayland app (layer-shell overlay, exclusive keyboard grab)
spotlight-toggle   — Shell script: kills running spotlight or launches a new one
spotlight-shortcut-daemon  — Listens for kglobalaccel D-Bus signal, calls spotlight-toggle
spotlight-setup    — One-time setup: registers Meta key shortcut with KDE
```

### How the shortcut works

Wayland doesn't allow apps to grab global hotkeys directly. The shortcut chain is:

1. **KDE's kglobalaccel** (embedded in KWin) captures the Meta key press
2. It emits a D-Bus signal on `/component/spotlight_toggle_desktop`
3. **spotlight-shortcut-daemon** (a bash script using `gdbus monitor`) listens for that signal
4. It runs **spotlight-toggle**, which either kills a running instance or starts a new one
5. **spotlight** creates a layer-shell overlay surface and grabs keyboard focus

The one-time **spotlight-setup** script registers the shortcut with kglobalaccel via D-Bus and removes Meta from KDE's default app launcher.

### Source modules

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point |
| `src/apps.rs` | Scans `.desktop` files from `/usr/share/applications`, parses Name/Exec/Icon, fuzzy search via `skim` algorithm |
| `src/render.rs` | CPU rendering with `tiny-skia` + text rendering with `fontdue` + icon loading (SVG via `resvg`, PNG via `image`) |
| `src/spotlight.rs` | Wayland setup (smithay-client-toolkit layer-shell), keyboard handling, event loop |

### Rendering

- 600px wide, dynamic height based on result count (max 8 visible)
- Layer: `Overlay`, anchored to top with 200px margin, centered horizontally
- Keyboard interactivity: `Exclusive` (grabs all keyboard input)
- Software rendered to SHM buffer (no GPU required)
- Icons loaded from freedesktop icon theme directories (breeze-dark, breeze, hicolor)
- Font loaded from system TTF (DejaVu, Noto, Liberation, or FreeSans)

### Key bindings (inside spotlight)

| Key | Action |
|-----|--------|
| Any character | Append to search query, re-filter results |
| Backspace | Delete last character |
| Up / Down | Move selection |
| Tab | Move selection down |
| Enter | Launch selected app |
| Escape | Dismiss |

## Building

This **must be built on Linux** (Wayland dependencies). SSH into the QEMU VM:

```bash
# Transfer source to VM
cd /path/to/os/spotlight
sshpass -p mars scp -P 2222 -o PubkeyAuthentication=no \
  Cargo.toml root@localhost:/root/spotlight/
sshpass -p mars scp -P 2222 -o PubkeyAuthentication=no \
  src/*.rs root@localhost:/root/spotlight/src/

# Build on VM
sshpass -p mars ssh -o PubkeyAuthentication=no -p 2222 root@localhost \
  "cd /root/spotlight && cargo build --release"

# Install
sshpass -p mars ssh -o PubkeyAuthentication=no -p 2222 root@localhost \
  "cp /root/target/release/spotlight /usr/local/bin/spotlight"
```

## Files installed on the VM

These live in `overlays/` and are applied via `scripts/apply-overlays.sh`:

| Path | Purpose |
|------|---------|
| `/usr/local/bin/spotlight` | The binary (built separately) |
| `/usr/local/bin/spotlight-toggle` | Toggle script (`pkill` or launch) |
| `/usr/local/bin/spotlight-shortcut-daemon` | D-Bus signal listener |
| `/usr/local/bin/spotlight-setup` | One-time Meta key registration |
| `/usr/share/applications/spotlight-toggle.desktop` | .desktop file for kglobalaccel |
| `/etc/xdg/autostart/spotlight-shortcut-daemon.desktop` | Auto-start listener on login |
| `/etc/xdg/autostart/spotlight-setup.desktop` | Auto-run setup on first login |

## Dependencies

Rust crates: `smithay-client-toolkit` (with xkbcommon), `tiny-skia`, `fontdue`, `fuzzy-matcher`, `resvg`, `image`

System: `libwayland-dev`, `libxkbcommon-dev`, a TTF font (e.g. `fonts-dejavu-core`)
