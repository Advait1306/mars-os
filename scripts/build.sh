#!/usr/bin/env bash
# MarsOS — Main Build Script
# Builds a bootable MarsOS disk image from scratch using debootstrap.
# Must be run as root on a Debian/Ubuntu system (e.g., the EC2 build instance).
#
# Usage: sudo bash scripts/build.sh [--desktop]
#   --desktop    Include KDE Plasma desktop packages (Phase 2)
#
# Output: build/mars-os.img (raw disk image)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="${PROJECT_DIR}/build"
ROOTFS_DIR="${BUILD_DIR}/rootfs"
IMG_FILE="${BUILD_DIR}/mars-os.img"
IMG_SIZE="10G"
DEBIAN_SUITE="trixie"
DEBIAN_MIRROR="http://deb.debian.org/debian"

# Parse args
INCLUDE_DESKTOP=false
for arg in "$@"; do
    case "$arg" in
        --desktop) INCLUDE_DESKTOP=true ;;
        *) echo "Unknown argument: $arg"; exit 1 ;;
    esac
done

# Ensure running as root
if [[ $EUID -ne 0 ]]; then
    echo "Error: This script must be run as root (sudo)."
    exit 1
fi

echo "=== MarsOS Build ==="
echo "  Suite:   ${DEBIAN_SUITE}"
echo "  Desktop: ${INCLUDE_DESKTOP}"
echo "  Output:  ${IMG_FILE}"
echo ""

# Clean previous build
rm -rf "${ROOTFS_DIR}"
mkdir -p "${ROOTFS_DIR}" "${BUILD_DIR}"

# ─── Step 1: Create disk image ───
echo ">>> Step 1: Creating ${IMG_SIZE} disk image..."
truncate -s "${IMG_SIZE}" "${IMG_FILE}"

# Partition: 512M EFI + rest ext4
parted -s "${IMG_FILE}" mklabel gpt
parted -s "${IMG_FILE}" mkpart ESP fat32 1MiB 513MiB
parted -s "${IMG_FILE}" set 1 esp on
parted -s "${IMG_FILE}" mkpart root ext4 513MiB 100%

# Set up loop device
LOOP_DEV=$(losetup --find --show --partscan "${IMG_FILE}")
echo "  Loop device: ${LOOP_DEV}"

# Format partitions
mkfs.fat -F 32 "${LOOP_DEV}p1"
mkfs.ext4 -q -L mars-root "${LOOP_DEV}p2"

# Mount
mount "${LOOP_DEV}p2" "${ROOTFS_DIR}"
mkdir -p "${ROOTFS_DIR}/boot/efi"
mount "${LOOP_DEV}p1" "${ROOTFS_DIR}/boot/efi"

# ─── Step 2: Debootstrap ───
echo ">>> Step 2: Running debootstrap (${DEBIAN_SUITE})..."
debootstrap --variant=minbase "${DEBIAN_SUITE}" "${ROOTFS_DIR}" "${DEBIAN_MIRROR}"

# ─── Step 3: Configure sources.list ───
echo ">>> Step 3: Configuring apt sources..."
cat > "${ROOTFS_DIR}/etc/apt/sources.list" <<EOF
deb ${DEBIAN_MIRROR} ${DEBIAN_SUITE} main contrib non-free non-free-firmware
deb ${DEBIAN_MIRROR} ${DEBIAN_SUITE}-updates main contrib non-free non-free-firmware
deb http://security.debian.org/debian-security ${DEBIAN_SUITE}-security main contrib non-free non-free-firmware
EOF

# ─── Step 4: Mount pseudo-filesystems for chroot ───
echo ">>> Step 4: Preparing chroot..."
mount --bind /dev "${ROOTFS_DIR}/dev"
mount --bind /dev/pts "${ROOTFS_DIR}/dev/pts"
mount -t proc proc "${ROOTFS_DIR}/proc"
mount -t sysfs sysfs "${ROOTFS_DIR}/sys"

# Copy resolv.conf for network access in chroot
cp /etc/resolv.conf "${ROOTFS_DIR}/etc/resolv.conf"

# ─── Step 5: Install base packages ───
echo ">>> Step 5: Installing base packages..."
# Read package list, strip comments and blank lines
BASE_PACKAGES=$(grep -v '^#' "${PROJECT_DIR}/config/packages/base.list" | grep -v '^\s*$' | tr '\n' ' ')

chroot "${ROOTFS_DIR}" /bin/bash -c "
    export DEBIAN_FRONTEND=noninteractive
    apt-get update
    apt-get install -y ${BASE_PACKAGES}
"

# ─── Step 6: System configuration ───
echo ">>> Step 6: Configuring system..."

# Hostname
echo "mars-os" > "${ROOTFS_DIR}/etc/hostname"
cat > "${ROOTFS_DIR}/etc/hosts" <<EOF
127.0.0.1   localhost
127.0.1.1   mars-os

::1         localhost ip6-localhost ip6-loopback
EOF

# Locale
chroot "${ROOTFS_DIR}" /bin/bash -c "
    export DEBIAN_FRONTEND=noninteractive
    sed -i 's/# en_US.UTF-8/en_US.UTF-8/' /etc/locale.gen
    locale-gen
    update-locale LANG=en_US.UTF-8
"

# Timezone
chroot "${ROOTFS_DIR}" ln -sf /usr/share/zoneinfo/UTC /etc/localtime

# fstab
ROOT_UUID=$(blkid -s UUID -o value "${LOOP_DEV}p2")
EFI_UUID=$(blkid -s UUID -o value "${LOOP_DEV}p1")
cat > "${ROOTFS_DIR}/etc/fstab" <<EOF
UUID=${ROOT_UUID}  /          ext4  errors=remount-ro  0  1
UUID=${EFI_UUID}   /boot/efi  vfat  umask=0077         0  1
EOF

# ─── Step 7: Install bootloader (GRUB EFI) ───
echo ">>> Step 7: Installing GRUB..."
chroot "${ROOTFS_DIR}" /bin/bash -c "
    export DEBIAN_FRONTEND=noninteractive
    grub-install --target=x86_64-efi --efi-directory=/boot/efi --bootloader-id=mars-os --removable --no-nvram
    update-grub
"

# ─── Step 8: Set root password and create user ───
echo ">>> Step 8: Setting up users..."
chroot "${ROOTFS_DIR}" /bin/bash -c "
    echo 'root:mars' | chpasswd
    useradd -m -s /bin/bash -G sudo mars
    echo 'mars:mars' | chpasswd
"

# ─── Step 9: Desktop (optional) ───
if [[ "${INCLUDE_DESKTOP}" == "true" ]]; then
    echo ">>> Step 9: Installing desktop (KDE Plasma + Wayland)..."

    # Copy chroot-setup script and configs
    cp "${SCRIPT_DIR}/chroot-setup.sh" "${ROOTFS_DIR}/tmp/chroot-setup.sh"
    cp "${PROJECT_DIR}/config/packages/desktop.list" "${ROOTFS_DIR}/tmp/desktop.list"

    # Copy KDE config files and SDDM config
    if [[ -d "${PROJECT_DIR}/config/kde" ]]; then
        mkdir -p "${ROOTFS_DIR}/tmp/kde-config"
        cp "${PROJECT_DIR}/config/kde/"* "${ROOTFS_DIR}/tmp/kde-config/"
    fi
    if [[ -f "${PROJECT_DIR}/config/kde/sddm.conf" ]]; then
        cp "${PROJECT_DIR}/config/kde/sddm.conf" "${ROOTFS_DIR}/tmp/sddm.conf"
    fi

    chroot "${ROOTFS_DIR}" /bin/bash /tmp/chroot-setup.sh

    # Clean up
    rm -rf "${ROOTFS_DIR}/tmp/chroot-setup.sh" "${ROOTFS_DIR}/tmp/desktop.list" "${ROOTFS_DIR}/tmp/kde-config" "${ROOTFS_DIR}/tmp/sddm.conf"
fi

# ─── Step 10: Copy overlays ───
echo ">>> Step 10: Applying overlays..."
if [[ -d "${PROJECT_DIR}/overlays" ]]; then
    rsync -a "${PROJECT_DIR}/overlays/" "${ROOTFS_DIR}/"
fi

# ─── Step 11: Distro identification ───
cat > "${ROOTFS_DIR}/etc/os-release" <<EOF
PRETTY_NAME="MarsOS 0.1"
NAME="MarsOS"
VERSION_ID="0.1"
VERSION="0.1 (Trixie)"
ID=mars-os
ID_LIKE=debian
HOME_URL="https://github.com/mars-os"
BUG_REPORT_URL="https://github.com/mars-os/issues"
EOF

# ─── Cleanup ───
echo ">>> Cleaning up..."
chroot "${ROOTFS_DIR}" apt-get clean
rm -rf "${ROOTFS_DIR}/var/cache/apt/archives"/*.deb
rm -f "${ROOTFS_DIR}/etc/resolv.conf"

# Unmount
umount "${ROOTFS_DIR}/dev/pts" || true
umount "${ROOTFS_DIR}/dev" || true
umount "${ROOTFS_DIR}/proc" || true
umount "${ROOTFS_DIR}/sys" || true
umount "${ROOTFS_DIR}/boot/efi" || true
umount "${ROOTFS_DIR}" || true
losetup -d "${LOOP_DEV}"

echo ""
echo "=== MarsOS Build Complete ==="
echo "  Image: ${IMG_FILE}"
echo "  Test:  sudo bash scripts/test-qemu.sh"
echo ""
