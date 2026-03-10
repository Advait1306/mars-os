#!/usr/bin/env bash
# MarsOS — Chroot Setup Script
# This runs INSIDE the chroot during build to install and configure
# the GNOME desktop environment.
# Called by build.sh — do not run directly.

set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

echo ">>> [chroot] Installing desktop packages..."

# Read package list
DESKTOP_PACKAGES=$(grep -v '^#' /tmp/desktop.list | grep -v '^\s*$' | tr '\n' ' ')

apt-get update
apt-get install -y ${DESKTOP_PACKAGES}

# ─── Configure GDM (display manager) ───
echo ">>> [chroot] Configuring GDM..."

# Enable GDM
systemctl enable gdm

# Force Wayland as default session
mkdir -p /etc/gdm3
if [[ -f /etc/gdm3/daemon.conf ]]; then
    sed -i 's/#WaylandEnable=false/WaylandEnable=true/' /etc/gdm3/daemon.conf
    sed -i 's/WaylandEnable=false/WaylandEnable=true/' /etc/gdm3/daemon.conf
fi

# ─── Configure PipeWire for audio ───
echo ">>> [chroot] Configuring PipeWire audio..."
systemctl --global enable pipewire.socket pipewire-pulse.socket wireplumber.service || true

# ─── Enable NetworkManager ───
echo ">>> [chroot] Enabling NetworkManager..."
systemctl enable NetworkManager

# ─── Apply GNOME defaults ───
if [[ -f /tmp/mars-defaults.gschema.override ]]; then
    echo ">>> [chroot] Applying MarsOS GNOME defaults..."
    cp /tmp/mars-defaults.gschema.override /usr/share/glib-2.0/schemas/90_mars-defaults.gschema.override
    glib-compile-schemas /usr/share/glib-2.0/schemas/
fi

# ─── Set default session to GNOME on Wayland ───
echo ">>> [chroot] Setting default session to GNOME Wayland..."
mkdir -p /var/lib/AccountsService/users
cat > /var/lib/AccountsService/users/mars <<EOF
[User]
Session=gnome
XSession=gnome
SystemAccount=false
EOF

# ─── Create XDG user directories ───
echo ">>> [chroot] Setting up user directories..."
su - mars -c "xdg-user-dirs-update" || true

# ─── Cleanup ───
apt-get clean

echo ">>> [chroot] Desktop setup complete."
