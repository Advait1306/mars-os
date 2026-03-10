#!/usr/bin/env bash
# MarsOS — EC2 Build Instance Setup
# Run this on a fresh Debian/Ubuntu EC2 instance to install build dependencies.
# Usage: ssh into the instance, then: sudo bash setup-ec2.sh

set -euo pipefail

echo "=== MarsOS: Setting up build environment ==="

export DEBIAN_FRONTEND=noninteractive

# Update system
apt-get update
apt-get upgrade -y

# Build dependencies
apt-get install -y \
    debootstrap \
    xorriso \
    grub-pc-bin \
    grub-efi-amd64-bin \
    grub-efi-amd64-signed \
    shim-signed \
    mtools \
    squashfs-tools \
    genisoimage \
    syslinux \
    syslinux-common \
    isolinux \
    dosfstools \
    e2fsprogs \
    parted \
    gdisk \
    qemu-system-x86 \
    qemu-utils \
    ovmf \
    rsync \
    git \
    curl \
    wget \
    sudo \
    gnupg

# Create working directories
mkdir -p /opt/mars-os/{build,cache,output}

# Set up apt cache for faster rebuilds
echo 'Acquire::http::Proxy "";' > /etc/apt/apt.conf.d/99proxy

echo ""
echo "=== MarsOS: Build environment ready ==="
echo "  Working dir: /opt/mars-os/"
echo "  Next step:   Copy the mars-os repo here, then run scripts/build.sh"
echo ""
