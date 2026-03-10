#!/usr/bin/env bash
# MarsOS — Patch a Debian cloud image to skip firstboot and set defaults.
# Runs inside a Docker container with the raw image mounted.
# Called by prep-image.sh — do not run directly.

set -euo pipefail

RAW_IMG="/work/disk.raw"
MNT="/mnt/rootfs"

echo ">>> Attaching image with kpartx..."
apt-get update -qq > /dev/null 2>&1
apt-get install -y -qq kpartx > /dev/null 2>&1

# Use kpartx to create device mappings for partitions
kpartx -av "${RAW_IMG}"

# Find the ext4 root partition from the mapped devices
ROOT_PART=""
for part in /dev/mapper/loop*; do
    if blkid "$part" 2>/dev/null | grep -q 'TYPE="ext4"'; then
        ROOT_PART="$part"
        break
    fi
done

if [[ -z "$ROOT_PART" ]]; then
    echo "Error: Could not find ext4 root partition"
    ls -la /dev/mapper/loop* 2>/dev/null
    exit 1
fi

echo "  Root partition: ${ROOT_PART}"
mkdir -p "${MNT}"
mount "${ROOT_PART}" "${MNT}"

# ─── Disable systemd-firstboot ───
echo ">>> Disabling systemd-firstboot..."
# firstboot runs when these files are missing. Create them.
echo "en_US.UTF-8" > "${MNT}/etc/locale.conf"
echo "UTC" > "${MNT}/etc/timezone"
echo "mars-os" > "${MNT}/etc/hostname"
echo "LANG=en_US.UTF-8" > "${MNT}/etc/default/locale"

cat > "${MNT}/etc/hosts" <<EOF
127.0.0.1   localhost
127.0.1.1   mars-os
::1         localhost ip6-localhost ip6-loopback
EOF

# Machine ID — presence of this file prevents firstboot
systemd-machine-id-setup --root="${MNT}" 2>/dev/null || true

# Mask the firstboot service entirely
chroot "${MNT}" systemctl mask systemd-firstboot.service 2>/dev/null || \
    ln -sf /dev/null "${MNT}/etc/systemd/system/systemd-firstboot.service"

# ─── Set root password to 'mars' ───
echo ">>> Setting root password..."
chroot "${MNT}" bash -c "echo 'root:mars' | chpasswd"

# ─── Enable root login on serial console ───
echo ">>> Enabling serial console login..."
mkdir -p "${MNT}/etc/systemd/system/serial-getty@ttyAMA0.service.d"
cat > "${MNT}/etc/systemd/system/serial-getty@ttyAMA0.service.d/autologin.conf" <<EOF
[Service]
ExecStart=
ExecStart=-/sbin/agetty --autologin root --noclear %I \$TERM
EOF

# ─── Install and configure SSH ───
echo ">>> Installing and configuring SSH..."
mount --bind /dev "${MNT}/dev"
mount --bind /dev/pts "${MNT}/dev/pts" 2>/dev/null || true
mount -t proc proc "${MNT}/proc"
mount -t sysfs sysfs "${MNT}/sys"
rm -f "${MNT}/etc/resolv.conf"
cp /etc/resolv.conf "${MNT}/etc/resolv.conf"

chroot "${MNT}" bash -c "
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    apt-get install -y -qq openssh-server
    sed -i 's/#PermitRootLogin.*/PermitRootLogin yes/' /etc/ssh/sshd_config
    systemctl enable ssh
    apt-get clean
"

rm -f "${MNT}/etc/resolv.conf"
umount "${MNT}/sys" || true
umount "${MNT}/proc" || true
umount "${MNT}/dev/pts" || true
umount "${MNT}/dev" || true

echo ">>> Cleanup..."
umount "${MNT}"
kpartx -d "${RAW_IMG}"

echo ">>> Done! Image patched successfully."
