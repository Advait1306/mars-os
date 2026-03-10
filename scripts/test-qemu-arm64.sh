#!/usr/bin/env bash
# MarsOS — QEMU ARM64 Test Script (macOS)
# Boots the ARM64 image in QEMU on Apple Silicon Mac.
# Uses HVF (Hypervisor.framework) for native-speed virtualization.
#
# Usage: bash scripts/test-qemu-arm64.sh
#
# The VM will open in a QEMU window. Close the window to stop.
# SSH: ssh -p 2222 mars@localhost (password: mars)
#
# Default login for Debian nocloud image:
#   user: root  password: (empty, or set via console)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="${PROJECT_DIR}/build"
IMG_FILE="${BUILD_DIR}/mars-os-arm64.qcow2"

if [[ ! -f "${IMG_FILE}" ]]; then
    echo "Error: ${IMG_FILE} not found."
    echo "Download it first: see README or run build-local.sh"
    exit 1
fi

# QEMU EFI firmware for ARM64
QEMU_SHARE="$(brew --prefix qemu)/share/qemu"
EFI_CODE="${QEMU_SHARE}/edk2-aarch64-code.fd"

if [[ ! -f "${EFI_CODE}" ]]; then
    echo "Error: EFI firmware not found at ${EFI_CODE}"
    echo "Try: brew reinstall qemu"
    exit 1
fi

# Create writable EFI vars (64MB) if it doesn't exist
EFI_VARS="${BUILD_DIR}/efi-vars-arm64.fd"
if [[ ! -f "${EFI_VARS}" ]]; then
    echo "Creating EFI vars file..."
    qemu-img create -f raw "${EFI_VARS}" 64M
fi

echo "=== Booting MarsOS ARM64 ==="
echo "  Image: ${IMG_FILE}"
echo "  SSH:   ssh -p 2222 root@localhost"
echo "  Close the QEMU window to stop the VM."
echo ""

qemu-system-aarch64 \
    -machine virt,highmem=on \
    -accel hvf \
    -cpu host \
    -m 4G \
    -smp 4 \
    -drive "if=pflash,format=raw,readonly=on,file=${EFI_CODE}" \
    -drive "if=pflash,format=raw,file=${EFI_VARS}" \
    -drive "file=${IMG_FILE},format=qcow2,if=virtio" \
    -device virtio-gpu-pci \
    -device qemu-xhci \
    -device usb-kbd \
    -device usb-tablet \
    -device virtio-net-pci,netdev=net0 \
    -netdev user,id=net0,hostfwd=tcp::2222-:22 \
    -device virtio-rng-pci \
    -audiodev coreaudio,id=audio0 \
    -device intel-hda \
    -device hda-duplex,audiodev=audio0 \
    -serial mon:stdio \
    -display cocoa
