#!/usr/bin/env bash
# MarsOS — Set up UTM virtual machine
# Creates a GPU-accelerated ARM64 VM in UTM with the MarsOS disk image.
#
# Usage: bash scripts/setup-vm.sh
#
# Prerequisites:
#   - build/mars-os-arm64.qcow2 must exist
#   - UTM must be installed (brew install --cask utm)
#
# The VM will appear in UTM as "MarsOS" with:
#   - QEMU backend, aarch64, HVF acceleration
#   - virtio-gpu-gl (GPU acceleration via virgl/ANGLE/Metal)
#   - SSH port forwarding: localhost:2222 -> guest:22
#   - Audio, USB 3.0 input

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="${PROJECT_DIR}/build"
IMG_FILE="${BUILD_DIR}/mars-os-arm64.qcow2"
VM_NAME="MarsOS"

# --- Preflight checks ---
if [[ ! -f "${IMG_FILE}" ]]; then
    echo "Error: ${IMG_FILE} not found."
    echo "Build or download the image first."
    exit 1
fi

if ! command -v utmctl &>/dev/null; then
    if [[ -x "/Applications/UTM.app/Contents/MacOS/utmctl" ]]; then
        UTMCTL="/Applications/UTM.app/Contents/MacOS/utmctl"
    else
        echo "UTM is not installed. Installing via Homebrew..."
        brew install --cask utm
        UTMCTL="/Applications/UTM.app/Contents/MacOS/utmctl"
    fi
else
    UTMCTL="utmctl"
fi

# Check if VM already exists in UTM
if "${UTMCTL}" list 2>/dev/null | grep -q "${VM_NAME}"; then
    echo "VM '${VM_NAME}' already exists in UTM."
    echo "To recreate, delete it from UTM first, then run this script again."
    exit 0
fi

echo "=== Setting up MarsOS UTM VM ==="

# --- Step 1: Create VM via AppleScript (UTM's supported API) ---
echo "Creating VM in UTM..."
open -a UTM
sleep 2

VM_ID=$(osascript -e "
tell application \"UTM\"
    set vm to make new virtual machine with properties {backend:qemu, configuration:{name:\"${VM_NAME}\", architecture:\"aarch64\", memory: 4096}}
    return id of vm
end tell
")
echo "  Created VM: ${VM_ID}"

# --- Step 2: Quit UTM so it saves the default config to disk ---
echo "Saving config..."
sleep 1
osascript -e 'tell application "UTM" to quit'
sleep 3

# --- Step 3: Locate the VM bundle and modify config ---
VM_DIR=~/Library/Containers/com.utmapp.UTM/Data/Documents/${VM_NAME}.utm
PLIST="${VM_DIR}/config.plist"

if [[ ! -f "${PLIST}" ]]; then
    echo "Error: VM config not found at ${PLIST}"
    exit 1
fi

echo "Configuring VM..."

# CPU: host with 4 cores
/usr/libexec/PlistBuddy -c "Set :System:CPU host" "$PLIST"
/usr/libexec/PlistBuddy -c "Set :System:CPUCount 4" "$PLIST"

# Display: virtio-gpu-gl-pci with dynamic resolution
/usr/libexec/PlistBuddy -c "Add :Display:0 dict" "$PLIST"
/usr/libexec/PlistBuddy -c "Add :Display:0:Hardware string virtio-gpu-gl-pci" "$PLIST"
/usr/libexec/PlistBuddy -c "Add :Display:0:DynamicResolution bool true" "$PLIST"
/usr/libexec/PlistBuddy -c "Add :Display:0:NativeResolution bool false" "$PLIST"
/usr/libexec/PlistBuddy -c "Add :Display:0:UpscalingFilter string Nearest" "$PLIST"
/usr/libexec/PlistBuddy -c "Add :Display:0:DownscalingFilter string Linear" "$PLIST"

# Network: Emulated mode with SSH port forwarding (host:2222 -> guest:22)
/usr/libexec/PlistBuddy -c "Set :Network:0:Mode Emulated" "$PLIST"
/usr/libexec/PlistBuddy -c "Add :Network:0:PortForward:0 dict" "$PLIST"
/usr/libexec/PlistBuddy -c "Add :Network:0:PortForward:0:Protocol string TCP" "$PLIST"
/usr/libexec/PlistBuddy -c "Add :Network:0:PortForward:0:HostAddress string 127.0.0.1" "$PLIST"
/usr/libexec/PlistBuddy -c "Add :Network:0:PortForward:0:HostPort integer 2222" "$PLIST"
/usr/libexec/PlistBuddy -c "Add :Network:0:PortForward:0:GuestAddress string " "$PLIST"
/usr/libexec/PlistBuddy -c "Add :Network:0:PortForward:0:GuestPort integer 22" "$PLIST"

# Sound: intel-hda
/usr/libexec/PlistBuddy -c "Add :Sound:0 dict" "$PLIST"
/usr/libexec/PlistBuddy -c "Add :Sound:0:Hardware string intel-hda" "$PLIST"

echo "  Display: virtio-gpu-gl-pci (GPU accelerated)"
echo "  Network: Emulated, SSH forwarding localhost:2222 -> :22"
echo "  Sound:   intel-hda"
echo "  CPU:     host, 4 cores, 4096 MB RAM"

# --- Step 4: Replace the empty disk image with our qcow2 ---
echo "Linking disk image..."
DISK_FILE=$(ls "${VM_DIR}/Data/"*.qcow2 2>/dev/null | head -1)
if [[ -n "${DISK_FILE}" ]]; then
    rm "${DISK_FILE}"
    ln "${IMG_FILE}" "${DISK_FILE}" 2>/dev/null \
        || cp "${IMG_FILE}" "${DISK_FILE}"
    echo "  Disk: $(basename ${DISK_FILE})"
else
    echo "Error: No disk image found in VM bundle"
    exit 1
fi

# --- Step 5: Relaunch UTM ---
echo "Launching UTM..."
open -a UTM
sleep 3

echo ""
echo "=== Done ==="
echo "VM '${VM_NAME}' is ready in UTM."
echo ""
echo "Usage:"
echo "  Start:   ${UTMCTL} start ${VM_NAME}"
echo "  Stop:    ${UTMCTL} stop ${VM_NAME}"
echo "  Serial:  ${UTMCTL} attach ${VM_NAME}"
echo "  SSH:     ssh -p 2222 root@localhost"
