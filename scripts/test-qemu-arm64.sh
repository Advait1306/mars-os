#!/usr/bin/env bash
# MarsOS — Boot ARM64 VM via UTM (macOS)
# GPU-accelerated via virtio-gpu-gl + virgl/ANGLE/Metal.
#
# Usage: bash scripts/test-qemu-arm64.sh
#
# Prerequisites: brew install --cask utm
# SSH: ssh -p 2222 root@localhost (password: mars)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="${PROJECT_DIR}/build"
IMG_FILE="${BUILD_DIR}/mars-os-arm64.qcow2"
VM_NAME="MarsOS"

if [[ ! -f "${IMG_FILE}" ]]; then
    echo "Error: ${IMG_FILE} not found."
    echo "Download it first: see README or run build-local.sh"
    exit 1
fi

# Locate utmctl
if command -v utmctl &>/dev/null; then
    UTMCTL="utmctl"
elif [[ -x "/Applications/UTM.app/Contents/MacOS/utmctl" ]]; then
    UTMCTL="/Applications/UTM.app/Contents/MacOS/utmctl"
else
    echo "Error: UTM is not installed."
    echo "Install with: brew install --cask utm"
    exit 1
fi

# Set up VM if it doesn't exist yet
if ! "${UTMCTL}" list 2>/dev/null | grep -q "${VM_NAME}"; then
    echo "VM '${VM_NAME}' not found in UTM. Setting it up..."
    bash "${SCRIPT_DIR}/setup-vm.sh"
    sleep 2
fi

echo "=== Booting MarsOS ARM64 (UTM — GPU accelerated) ==="
echo "  SSH: ssh -p 2222 root@localhost"
echo ""

# Start the VM if not already running
STATUS=$("${UTMCTL}" status "${VM_NAME}" 2>/dev/null || echo "unknown")
if echo "${STATUS}" | grep -qi "started"; then
    echo "VM is already running."
else
    "${UTMCTL}" start "${VM_NAME}"
    echo "VM started. Use UTM window for display."
fi

echo ""
echo "Commands:"
echo "  Stop:    ${UTMCTL} stop ${VM_NAME}"
echo "  Serial:  ${UTMCTL} attach ${VM_NAME}"
echo "  SSH:     ssh -p 2222 root@localhost"
