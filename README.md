# MarsOS

A custom Linux distribution based on Debian Trixie (13) with KDE Plasma on Wayland.

## Quick Start

### Prerequisites (macOS ARM)

```bash
brew install qemu
```

### Boot MarsOS

```bash
bash scripts/test-qemu-arm64.sh
```

A QEMU window will open with the SDDM login screen.

- **User:** `mars` / **Password:** `mars`
- **Root:** `root` / **Password:** `mars`

### SSH Access

```bash
ssh -p 2222 mars@localhost
# or
ssh -p 2222 root@localhost
```

### Apply Customizations

After modifying files in `overlays/` or `config/kde/`, push changes to the running VM:

```bash
bash scripts/apply-overlays.sh
# Then reboot the VM:
ssh -p 2222 root@localhost 'reboot'
```

## What's Included

- **Desktop:** KDE Plasma on Wayland (Breeze Dark theme, Noto fonts)
- **Display Manager:** SDDM (Wayland mode)
- **Audio:** PipeWire
- **Browser:** Firefox ESR
- **Apps:** Dolphin, Konsole, Kate, KCalc, Gwenview, Okular, System Monitor
- **Networking:** NetworkManager
- **Printing:** CUPS
- **Custom Panel:** Mars-branded start menu icon, task manager, system tray, digital clock

## Project Structure

```
os/
├── config/
│   ├── packages/
│   │   ├── base.list                          # Minimal system packages
│   │   └── desktop.list                       # KDE Plasma + Wayland packages
│   ├── kernel/
│   │   └── mars-kernel.conf                   # Kernel config overrides (future)
│   └── kde/
│       ├── kdeglobals                         # Fonts, theme, icons, colors
│       ├── kwinrc                             # Compositor, effects, window mgmt
│       ├── plasmarc                           # Plasma style/theme
│       ├── plasma-org.kde.plasma.desktop-appletsrc  # Panel & widget layout
│       ├── kcminputrc                         # Touchpad settings
│       ├── kscreenlockerrc                    # Screen lock settings
│       ├── powermanagementprofilesrc          # Power/sleep settings
│       └── sddm.conf                         # Display manager config
├── scripts/
│   ├── build.sh                               # Main build script (x86_64, runs on EC2)
│   ├── build-arm64.sh                         # ARM64 build (runs inside Docker)
│   ├── build-local.sh                         # Local build wrapper for macOS
│   ├── setup-ec2.sh                           # Provisions EC2 build instance
│   ├── chroot-setup.sh                        # Desktop setup inside chroot
│   ├── make-iso.sh                            # Create bootable ISO from disk image
│   ├── patch-image.sh                         # Patch Debian cloud image (Docker)
│   ├── apply-overlays.sh                      # Push overlays & KDE configs to running VM
│   ├── test-qemu.sh                           # Boot x86_64 image in QEMU
│   └── test-qemu-arm64.sh                     # Boot ARM64 image in QEMU (macOS)
├── overlays/                                  # Files copied directly into the image
│   ├── etc/xdg/autostart/                     # Autostart entries
│   └── usr/share/
│       ├── plasma/plasmoids/org.mars-os.branding/  # Mars branding plasmoid
│       └── applications/                      # Desktop entries
├── build/                                     # Build output (gitignored)
│   ├── mars-os-arm64.qcow2                    # Working image (with snapshots)
│   └── mars-os-0.1-arm64.qcow2               # Compressed release image
├── Dockerfile.build                           # Docker build environment
├── .env.example                               # Config template
└── .gitignore
```

## Build Images

### ARM64 (on macOS Apple Silicon)

The ARM64 image is built by customizing a Debian cloud image:

```bash
# 1. Download the Debian Trixie ARM64 cloud image
curl -L -o build/debian-13-nocloud-arm64.qcow2 \
  https://cloud.debian.org/images/cloud/trixie/latest/debian-13-nocloud-arm64.qcow2

# 2. Copy and resize
cp build/debian-13-nocloud-arm64.qcow2 build/mars-os-arm64.qcow2
qemu-img resize build/mars-os-arm64.qcow2 20G

# 3. Patch the image (disable firstboot, set passwords, install SSH)
#    Requires Docker running
qemu-img convert -f qcow2 -O raw build/mars-os-arm64.qcow2 build/mars-os-arm64.raw
docker run --rm --privileged \
  -v $(pwd)/build/mars-os-arm64.raw:/work/disk.raw \
  -v $(pwd)/scripts/patch-image.sh:/work/patch-image.sh \
  debian:trixie bash /work/patch-image.sh
qemu-img convert -f raw -O qcow2 build/mars-os-arm64.raw build/mars-os-arm64.qcow2
rm build/mars-os-arm64.raw

# 4. Boot, then install KDE Plasma via SSH
bash scripts/test-qemu-arm64.sh
# (In another terminal, once VM is up:)
bash scripts/apply-overlays.sh

# 5. Create compressed release image
qemu-img convert -O qcow2 -c build/mars-os-arm64.qcow2 build/mars-os-0.1-arm64.qcow2
```

### x86_64 (on AWS EC2)

```bash
# 1. Set up an EC2 instance (Debian, t3.medium)
# 2. Copy the repo to the instance
# 3. Run:
sudo bash scripts/setup-ec2.sh
sudo bash scripts/build.sh --desktop
sudo bash scripts/make-iso.sh
sudo bash scripts/test-qemu.sh --iso
```

## Architecture

```
┌────────────────────────────────┐
│  KDE Plasma Desktop (Wayland)  │
├────────────────────────────────┤
│  MarsOS customizations/theming │
├────────────────────────────────┤
│  Debian Trixie (13) base       │
├────────────────────────────────┤
│  Linux kernel (stock Debian)   │
└────────────────────────────────┘
```

## Roadmap

- [ ] Branding (wallpapers, Plymouth boot splash, SDDM theme)
- [ ] Calamares installer
- [ ] x86_64 build on EC2
- [ ] Android app support (Waydroid)
