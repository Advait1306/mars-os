#!/usr/bin/env bash
# MarsOS — ARM64 Build Script
# Runs INSIDE the Docker build container on macOS.
# Produces an ARM64 disk image using debootstrap.
#
# Usage (via Docker):
#   docker build -f Dockerfile.build -t mars-os-builder .
#   docker run --privileged -v $(pwd)/build:/out mars-os-builder
#
# Or with desktop:
#   docker run --privileged -e INCLUDE_DESKTOP=true -v $(pwd)/build:/out mars-os-builder

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="/build"
ROOTFS_DIR="${BUILD_DIR}/rootfs"
IMG_FILE="${BUILD_DIR}/mars-os-arm64.img"
OUT_DIR="/out"
IMG_SIZE="10G"
DEBIAN_SUITE="trixie"
DEBIAN_MIRROR="http://deb.debian.org/debian"

INCLUDE_DESKTOP="${INCLUDE_DESKTOP:-false}"

echo "=== MarsOS ARM64 Build ==="
echo "  Suite:   ${DEBIAN_SUITE}"
echo "  Arch:    arm64"
echo "  Desktop: ${INCLUDE_DESKTOP}"
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

# Set up loop device with kpartx (losetup --partscan doesn't work in Docker)
LOOP_DEV=$(losetup --find --show "${IMG_FILE}")
echo "  Loop device: ${LOOP_DEV}"
kpartx -av "${LOOP_DEV}"

# kpartx creates /dev/mapper/loopXp1, /dev/mapper/loopXp2
LOOP_NAME=$(basename "${LOOP_DEV}")
PART1="/dev/mapper/${LOOP_NAME}p1"
PART2="/dev/mapper/${LOOP_NAME}p2"

# Wait for device nodes to appear
sleep 1

# Format partitions
mkfs.fat -F 32 "${PART1}"
mkfs.ext4 -q -L mars-root "${PART2}"

# Mount
mount "${PART2}" "${ROOTFS_DIR}"
mkdir -p "${ROOTFS_DIR}/boot/efi"
mount "${PART1}" "${ROOTFS_DIR}/boot/efi"

# ─── Step 2: Debootstrap (ARM64) ───
echo ">>> Step 2: Running debootstrap (${DEBIAN_SUITE}, arm64)..."
debootstrap --arch=arm64 --variant=minbase "${DEBIAN_SUITE}" "${ROOTFS_DIR}" "${DEBIAN_MIRROR}"

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
cp /etc/resolv.conf "${ROOTFS_DIR}/etc/resolv.conf"

# ─── Step 5: Install base packages ───
echo ">>> Step 5: Installing base packages..."

# Read package list, swap amd64 kernel for arm64
BASE_PACKAGES=$(grep -v '^#' "${PROJECT_DIR}/config/packages/base.list" \
    | grep -v '^\s*$' \
    | sed 's/linux-image-amd64/linux-image-arm64/' \
    | sed 's/grub-pc//' \
    | sed 's/grub-efi-amd64/grub-efi-arm64/' \
    | tr '\n' ' ')

chroot "${ROOTFS_DIR}" /bin/bash -c "
    export DEBIAN_FRONTEND=noninteractive
    apt-get update
    apt-get install -y ${BASE_PACKAGES}
"

# ─── Step 6: System configuration ───
echo ">>> Step 6: Configuring system..."

echo "mars-os" > "${ROOTFS_DIR}/etc/hostname"
cat > "${ROOTFS_DIR}/etc/hosts" <<EOF
127.0.0.1   localhost
127.0.1.1   mars-os
::1         localhost ip6-localhost ip6-loopback
EOF

chroot "${ROOTFS_DIR}" /bin/bash -c "
    export DEBIAN_FRONTEND=noninteractive
    sed -i 's/# en_US.UTF-8/en_US.UTF-8/' /etc/locale.gen
    locale-gen
    update-locale LANG=en_US.UTF-8
"

chroot "${ROOTFS_DIR}" ln -sf /usr/share/zoneinfo/UTC /etc/localtime

# fstab
ROOT_UUID=$(blkid -s UUID -o value "${PART2}")
EFI_UUID=$(blkid -s UUID -o value "${PART1}")
cat > "${ROOTFS_DIR}/etc/fstab" <<EOF
UUID=${ROOT_UUID}  /          ext4  errors=remount-ro  0  1
UUID=${EFI_UUID}   /boot/efi  vfat  umask=0077         0  1
EOF

# ─── Step 7: Install bootloader (GRUB EFI ARM64) ───
echo ">>> Step 7: Installing GRUB (ARM64 EFI)..."

# Tell GRUB to use UUID for root device (kpartx mapper paths don't exist on real hardware)
sed -i "s|^#GRUB_DISABLE_LINUX_UUID=.*|GRUB_DISABLE_LINUX_UUID=false|" "${ROOTFS_DIR}/etc/default/grub"
echo "GRUB_DISABLE_LINUX_UUID=false" >> "${ROOTFS_DIR}/etc/default/grub"

chroot "${ROOTFS_DIR}" /bin/bash -c "
    export DEBIAN_FRONTEND=noninteractive
    grub-install --target=arm64-efi --efi-directory=/boot/efi --bootloader-id=mars-os --removable --no-nvram
    update-grub
"

# Fix any remaining /dev/mapper references in grub.cfg (kpartx artifact)
if grep -q '/dev/mapper/loop' "${ROOTFS_DIR}/boot/grub/grub.cfg"; then
    echo "  Fixing GRUB root device to UUID..."
    sed -i "s|root=/dev/mapper/[^ ]*|root=UUID=${ROOT_UUID}|g" "${ROOTFS_DIR}/boot/grub/grub.cfg"
    sed -i "s|/dev/mapper/${LOOP_NAME}p2|UUID=${ROOT_UUID}|g" "${ROOTFS_DIR}/boot/grub/grub.cfg"
    sed -i "s|/dev/mapper/${LOOP_NAME}p1|UUID=${EFI_UUID}|g" "${ROOTFS_DIR}/boot/grub/grub.cfg"
fi

# ─── Step 8: Users ───
echo ">>> Step 8: Setting up users..."
chroot "${ROOTFS_DIR}" /bin/bash -c "
    echo 'root:mars' | chpasswd
    useradd -m -s /bin/bash -G sudo mars
    echo 'mars:mars' | chpasswd
"

# ─── Step 9: Desktop (optional) ───
if [[ "${INCLUDE_DESKTOP}" == "true" ]]; then
    echo ">>> Step 9: Installing desktop (KDE Plasma + Wayland)..."

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

    rm -rf "${ROOTFS_DIR}/tmp/chroot-setup.sh" "${ROOTFS_DIR}/tmp/desktop.list" "${ROOTFS_DIR}/tmp/kde-config" "${ROOTFS_DIR}/tmp/sddm.conf"
fi

# ─── Step 10: Overlays ───
echo ">>> Step 10: Applying overlays..."
if [[ -d "${PROJECT_DIR}/overlays" ]]; then
    rsync -a "${PROJECT_DIR}/overlays/" "${ROOTFS_DIR}/"
fi

# ─── Step 11: OS identity ───
cat > "${ROOTFS_DIR}/etc/os-release" <<EOF
PRETTY_NAME="MarsOS 0.1 (ARM64)"
NAME="MarsOS"
VERSION_ID="0.1"
VERSION="0.1 (Trixie)"
ID=mars-os
ID_LIKE=debian
HOME_URL="https://github.com/mars-os"
BUG_REPORT_URL="https://github.com/mars-os/issues"
EOF

# ─── Final GRUB fixup ───
# update-grub during package installs uses /dev/mapper paths from kpartx.
# These don't exist on real hardware — fix grub.cfg to use UUIDs.
echo ">>> Fixing GRUB root device to UUID..."
sed -i "s|root=/dev/mapper/[^ ]*|root=UUID=${ROOT_UUID}|g" "${ROOTFS_DIR}/boot/grub/grub.cfg"
sed -i "s|/dev/mapper/${LOOP_NAME}p2|/dev/disk/by-uuid/${ROOT_UUID}|g" "${ROOTFS_DIR}/boot/grub/grub.cfg"
sed -i "s|/dev/mapper/${LOOP_NAME}p1|/dev/disk/by-uuid/${EFI_UUID}|g" "${ROOTFS_DIR}/boot/grub/grub.cfg"
# Verify
echo "  grub.cfg root= lines after fix:"
grep 'root=' "${ROOTFS_DIR}/boot/grub/grub.cfg" | grep -v '^#' | head -5

# ─── Cleanup ───
echo ">>> Cleaning up..."
chroot "${ROOTFS_DIR}" apt-get clean
rm -rf "${ROOTFS_DIR}/var/cache/apt/archives"/*.deb
rm -f "${ROOTFS_DIR}/etc/resolv.conf"

umount "${ROOTFS_DIR}/dev/pts" || true
umount "${ROOTFS_DIR}/dev" || true
umount "${ROOTFS_DIR}/proc" || true
umount "${ROOTFS_DIR}/sys" || true
umount "${ROOTFS_DIR}/boot/efi" || true
umount "${ROOTFS_DIR}" || true
kpartx -dv "${LOOP_DEV}" || true
losetup -d "${LOOP_DEV}"

# Copy to output
cp "${IMG_FILE}" "${OUT_DIR}/mars-os-arm64.img"

echo ""
echo "=== MarsOS ARM64 Build Complete ==="
echo "  Image: ${OUT_DIR}/mars-os-arm64.img"
echo ""
