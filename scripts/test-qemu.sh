#!/usr/bin/env bash
# MarsOS — QEMU Test Script
# Boots the built MarsOS image in QEMU for testing.
# Starts a VNC server on port 5900 for remote desktop access.
#
# Usage:
#   sudo bash scripts/test-qemu.sh           # Boot disk image
#   sudo bash scripts/test-qemu.sh --iso     # Boot ISO
#
# Connect via VNC: vncviewer <ec2-ip>:5900

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="${PROJECT_DIR}/build"

BOOT_ISO=false
QEMU_MEM="2G"
QEMU_CPUS="2"
VNC_DISPLAY=":0"

for arg in "$@"; do
    case "$arg" in
        --iso) BOOT_ISO=true ;;
        *) echo "Unknown argument: $arg"; exit 1 ;;
    esac
done

# Locate OVMF firmware for EFI boot
OVMF_CODE="/usr/share/OVMF/OVMF_CODE.fd"
OVMF_VARS="/usr/share/OVMF/OVMF_VARS.fd"

if [[ ! -f "${OVMF_CODE}" ]]; then
    echo "Error: OVMF not found. Install: sudo apt install ovmf"
    exit 1
fi

# Create a writable copy of OVMF vars
OVMF_VARS_COPY="${BUILD_DIR}/OVMF_VARS.fd"
cp "${OVMF_VARS}" "${OVMF_VARS_COPY}"

QEMU_ARGS=(
    -machine q35
    -cpu host
    -enable-kvm
    -m "${QEMU_MEM}"
    -smp "${QEMU_CPUS}"
    -drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}"
    -drive "if=pflash,format=raw,file=${OVMF_VARS_COPY}"
    -device virtio-vga-gl
    -display vnc="${VNC_DISPLAY}"
    -device virtio-net-pci,netdev=net0
    -netdev user,id=net0,hostfwd=tcp::2222-:22
    -device virtio-rng-pci
)

if [[ "${BOOT_ISO}" == "true" ]]; then
    ISO_FILE="${BUILD_DIR}/mars-os.iso"
    if [[ ! -f "${ISO_FILE}" ]]; then
        echo "Error: ${ISO_FILE} not found. Run make-iso.sh first."
        exit 1
    fi
    echo "=== Booting MarsOS ISO ==="
    echo "  VNC: connect to port 5900"
    echo "  SSH: ssh -p 2222 mars@localhost"
    echo ""
    qemu-system-x86_64 "${QEMU_ARGS[@]}" \
        -cdrom "${ISO_FILE}" \
        -boot d
else
    IMG_FILE="${BUILD_DIR}/mars-os.img"
    if [[ ! -f "${IMG_FILE}" ]]; then
        echo "Error: ${IMG_FILE} not found. Run build.sh first."
        exit 1
    fi
    echo "=== Booting MarsOS Disk Image ==="
    echo "  VNC: connect to port 5900"
    echo "  SSH: ssh -p 2222 mars@localhost"
    echo ""
    qemu-system-x86_64 "${QEMU_ARGS[@]}" \
        -drive "file=${IMG_FILE},format=raw,if=virtio"
fi
