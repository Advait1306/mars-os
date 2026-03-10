#!/usr/bin/env bash
# MarsOS — ISO Creation Script
# Converts a built disk image into a bootable live ISO.
# Must be run as root on the build machine.
#
# Usage: sudo bash scripts/make-iso.sh
# Input:  build/mars-os.img (from build.sh)
# Output: build/mars-os.iso

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="${PROJECT_DIR}/build"
IMG_FILE="${BUILD_DIR}/mars-os.img"
ISO_DIR="${BUILD_DIR}/iso-staging"
ISO_FILE="${BUILD_DIR}/mars-os.iso"
ROOTFS_DIR="${BUILD_DIR}/rootfs-iso"

if [[ $EUID -ne 0 ]]; then
    echo "Error: This script must be run as root (sudo)."
    exit 1
fi

if [[ ! -f "${IMG_FILE}" ]]; then
    echo "Error: ${IMG_FILE} not found. Run build.sh first."
    exit 1
fi

echo "=== MarsOS: Creating bootable ISO ==="

# Clean previous
rm -rf "${ISO_DIR}" "${ROOTFS_DIR}"
mkdir -p "${ISO_DIR}"/{boot/grub,live,EFI/BOOT}
mkdir -p "${ROOTFS_DIR}"

# ─── Step 1: Mount the disk image and extract rootfs ───
echo ">>> Step 1: Extracting rootfs from disk image..."
LOOP_DEV=$(losetup --find --show --partscan "${IMG_FILE}")
mount "${LOOP_DEV}p2" "${ROOTFS_DIR}"

# ─── Step 2: Create squashfs ───
echo ">>> Step 2: Creating squashfs (this may take a while)..."
mksquashfs "${ROOTFS_DIR}" "${ISO_DIR}/live/filesystem.squashfs" \
    -comp xz \
    -e boot/efi \
    -noappend

# ─── Step 3: Copy kernel and initrd ───
echo ">>> Step 3: Copying kernel and initramfs..."
cp "${ROOTFS_DIR}"/boot/vmlinuz-* "${ISO_DIR}/boot/vmlinuz"
cp "${ROOTFS_DIR}"/boot/initrd.img-* "${ISO_DIR}/boot/initrd.img"

# ─── Step 4: Create GRUB config for ISO ───
echo ">>> Step 4: Creating GRUB config..."
cat > "${ISO_DIR}/boot/grub/grub.cfg" <<'EOF'
set timeout=5
set default=0

menuentry "MarsOS — Live" {
    linux /boot/vmlinuz boot=live quiet splash
    initrd /boot/initrd.img
}

menuentry "MarsOS — Live (Safe Mode)" {
    linux /boot/vmlinuz boot=live nomodeset
    initrd /boot/initrd.img
}
EOF

# ─── Step 5: Create EFI boot image ───
echo ">>> Step 5: Creating EFI boot image..."

# Copy signed EFI binaries
if [[ -f /usr/lib/grub/x86_64-efi/monolithic/grubx64.efi ]]; then
    cp /usr/lib/grub/x86_64-efi/monolithic/grubx64.efi "${ISO_DIR}/EFI/BOOT/BOOTX64.EFI"
else
    # Build GRUB EFI image
    grub-mkstandalone \
        --format=x86_64-efi \
        --output="${ISO_DIR}/EFI/BOOT/BOOTX64.EFI" \
        --locales="" \
        --fonts="" \
        "boot/grub/grub.cfg=${ISO_DIR}/boot/grub/grub.cfg"
fi

# Create EFI system partition image
dd if=/dev/zero of="${ISO_DIR}/boot/efi.img" bs=1M count=10
mkfs.fat -F 12 "${ISO_DIR}/boot/efi.img"
EFI_MNT=$(mktemp -d)
mount "${ISO_DIR}/boot/efi.img" "${EFI_MNT}"
mkdir -p "${EFI_MNT}/EFI/BOOT"
cp "${ISO_DIR}/EFI/BOOT/BOOTX64.EFI" "${EFI_MNT}/EFI/BOOT/"
umount "${EFI_MNT}"
rmdir "${EFI_MNT}"

# ─── Step 6: Build ISO ───
echo ">>> Step 6: Building ISO image..."
xorriso -as mkisofs \
    -iso-level 3 \
    -full-iso9660-filenames \
    -volid "MARS_OS" \
    -eltorito-alt-boot \
    -e boot/efi.img \
    -no-emul-boot \
    -isohybrid-gpt-basdat \
    -output "${ISO_FILE}" \
    "${ISO_DIR}"

# ─── Cleanup ───
echo ">>> Cleaning up..."
umount "${ROOTFS_DIR}" || true
losetup -d "${LOOP_DEV}" || true
rm -rf "${ISO_DIR}" "${ROOTFS_DIR}"

ISO_SIZE=$(du -h "${ISO_FILE}" | cut -f1)
echo ""
echo "=== MarsOS ISO Complete ==="
echo "  ISO:  ${ISO_FILE}"
echo "  Size: ${ISO_SIZE}"
echo "  Test: sudo bash scripts/test-qemu.sh --iso"
echo ""
