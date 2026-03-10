#!/usr/bin/env bash
# MarsOS — Chroot Setup Script
# This runs INSIDE the chroot during build to install and configure
# the KDE Plasma desktop environment.
# Called by build.sh — do not run directly.

set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

echo ">>> [chroot] Installing desktop packages..."

# Read package list
DESKTOP_PACKAGES=$(grep -v '^#' /tmp/desktop.list | grep -v '^\s*$' | tr '\n' ' ')

apt-get update
apt-get install -y ${DESKTOP_PACKAGES}

# ─── Configure SDDM (display manager) ───
echo ">>> [chroot] Configuring SDDM..."

# Enable SDDM
systemctl enable sddm

# Install SDDM Wayland config
if [[ -f /tmp/sddm.conf ]]; then
    mkdir -p /etc/sddm.conf.d
    cp /tmp/sddm.conf /etc/sddm.conf.d/mars-os.conf
fi

# ─── Configure PipeWire for audio ───
echo ">>> [chroot] Configuring PipeWire audio..."
systemctl --global enable pipewire.socket pipewire-pulse.socket wireplumber.service || true

# ─── Enable NetworkManager ───
echo ">>> [chroot] Enabling NetworkManager..."
systemctl enable NetworkManager

# ─── Apply KDE defaults ───
echo ">>> [chroot] Applying MarsOS KDE defaults..."

# KDE uses config files in /etc/xdg/ for system-wide defaults
KDE_DEFAULTS_DIR="/etc/xdg"
mkdir -p "${KDE_DEFAULTS_DIR}"

for cfg in /tmp/kde-config/*; do
    if [[ -f "$cfg" ]]; then
        cp "$cfg" "${KDE_DEFAULTS_DIR}/$(basename "$cfg")"
        echo "  Installed: $(basename "$cfg")"
    fi
done

# ─── Set default session to Plasma Wayland ───
echo ">>> [chroot] Setting default session to Plasma Wayland..."
mkdir -p /var/lib/AccountsService/users
cat > /var/lib/AccountsService/users/mars <<EOF
[User]
Session=plasma
XSession=plasma
SystemAccount=false
EOF

# ─── Create XDG user directories ───
echo ">>> [chroot] Setting up user directories..."
su - mars -c "xdg-user-dirs-update" || true

# ─── Cleanup ───
apt-get clean

echo ">>> [chroot] Desktop setup complete."
