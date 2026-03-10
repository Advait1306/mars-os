#!/usr/bin/env bash
# MarsOS — Apply Overlays to Running VM
# Copies everything from overlays/ into the VM filesystem via SSH,
# then applies KDE Plasma configs and plasmoids as needed.
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
    ssh_cmd -n "mkdir -p '${dest_dir}'" 2>/dev/null
    scp_cmd "${file}" "root@localhost:${dest}" 2>/dev/null
    echo "  ${dest}"
done

# Copy KDE config files to system-wide defaults
echo "Applying KDE config files..."
KDE_CONFIG_DIR="${PROJECT_DIR}/config/kde"
if [[ -d "${KDE_CONFIG_DIR}" ]]; then
    for cfg in "${KDE_CONFIG_DIR}"/*; do
        if [[ -f "$cfg" ]]; then
            cfg_name="$(basename "$cfg")"
            if [[ "$cfg_name" == "sddm.conf" ]]; then
                ssh_cmd "mkdir -p /etc/sddm.conf.d" 2>/dev/null
                scp_cmd "$cfg" "root@localhost:/etc/sddm.conf.d/mars-os.conf" 2>/dev/null
            else
                scp_cmd "$cfg" "root@localhost:/etc/xdg/${cfg_name}" 2>/dev/null
            fi
            echo "  /etc/xdg/${cfg_name}"
        fi
    done
fi

# Also apply KDE configs to existing mars user (per-user overrides /etc/xdg/)
echo "Applying KDE configs to mars user profile..."
if [[ -d "${KDE_CONFIG_DIR}" ]]; then
    for cfg in "${KDE_CONFIG_DIR}"/*; do
        if [[ -f "$cfg" ]]; then
            cfg_name="$(basename "$cfg")"
            [[ "$cfg_name" == "sddm.conf" ]] && continue
            scp_cmd "$cfg" "root@localhost:/home/mars/.config/${cfg_name}" 2>/dev/null
            ssh_cmd "chown mars:mars '/home/mars/.config/${cfg_name}'" 2>/dev/null
            echo "  /home/mars/.config/${cfg_name}"
        fi
    done
fi

echo ""
echo "=== Done! ==="
echo "Restart Plasma to see changes:"
echo "  - Log out and back in"
echo "  - Or reboot:  ssh -p 2222 root@localhost 'reboot'"
