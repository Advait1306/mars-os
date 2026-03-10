#!/usr/bin/env bash
# MarsOS — Apply Overlays to Running VM
# Copies everything from overlays/ into the VM filesystem via SSH,
# then enables extensions and recompiles schemas as needed.
#
# Usage: bash scripts/apply-overlays.sh
#
# Requires: VM running with SSH on port 2222

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
OVERLAYS_DIR="${PROJECT_DIR}/overlays"

ssh_cmd() {
    sshpass -p mars ssh \
        -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null \
        -o PubkeyAuthentication=no \
        -p 2222 root@localhost "$@"
}

scp_cmd() {
    sshpass -p mars scp \
        -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null \
        -o PubkeyAuthentication=no \
        -P 2222 "$@"
}

echo "=== MarsOS: Applying Overlays ==="

# Check VM is reachable
if ! ssh_cmd "echo ok" &>/dev/null; then
    echo "Error: Cannot reach VM on port 2222."
    echo "Start it first: bash scripts/test-qemu-arm64.sh"
    exit 1
fi

# Copy overlay files into the VM, preserving directory structure
echo "Copying overlay files..."
cd "${OVERLAYS_DIR}"
find . -type f -print0 | while IFS= read -r -d '' file; do
    dest="${file#.}"  # strip leading dot
    dest_dir="$(dirname "${dest}")"
    ssh_cmd "mkdir -p '${dest_dir}'" 2>/dev/null
    scp_cmd "${file}" "root@localhost:${dest}" 2>/dev/null
    echo "  ${dest}"
done

# Recompile GSettings schemas if any were copied
if ssh_cmd "ls /usr/share/glib-2.0/schemas/*.gschema.override" &>/dev/null; then
    echo "Recompiling GSettings schemas..."
    ssh_cmd "glib-compile-schemas /usr/share/glib-2.0/schemas/" 2>/dev/null
fi

# Enable the branding extension system-wide for all users
EXT_UUID="mars-branding@mars-os.io"
if ssh_cmd "test -d /usr/share/gnome-shell/extensions/${EXT_UUID}" 2>/dev/null; then
    echo "Enabling ${EXT_UUID} extension for all users..."
    ssh_cmd "cat > /usr/share/glib-2.0/schemas/01-mars-extensions.gschema.override << 'SCHEMA'
[org.gnome.shell]
enabled-extensions=['${EXT_UUID}']
SCHEMA" 2>/dev/null
    ssh_cmd "glib-compile-schemas /usr/share/glib-2.0/schemas/" 2>/dev/null
fi

echo ""
echo "=== Done! ==="
echo "Restart GNOME Shell to see changes:"
echo "  - On Wayland: log out and back in"
echo "  - Or reboot:  ssh -p 2222 root@localhost 'reboot'"
