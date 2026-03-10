#!/usr/bin/env bash
# MarsOS — Local Build (macOS)
# Builds the ARM64 image using Docker, then optionally boots it in QEMU.
#
# Usage:
#   bash scripts/build-local.sh                 # Build minimal image
#   bash scripts/build-local.sh --desktop       # Build with GNOME desktop
#   bash scripts/build-local.sh --desktop --run  # Build and boot in QEMU

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="${PROJECT_DIR}/build"

INCLUDE_DESKTOP=false
RUN_AFTER=false

for arg in "$@"; do
    case "$arg" in
        --desktop) INCLUDE_DESKTOP=true ;;
        --run) RUN_AFTER=true ;;
        *) echo "Unknown argument: $arg"; exit 1 ;;
    esac
done

# Ensure Docker is running
if ! docker info &>/dev/null; then
    echo "Error: Docker is not running. Please start Docker Desktop."
    exit 1
fi

# Ensure QEMU is available (for --run)
if [[ "${RUN_AFTER}" == "true" ]] && ! command -v qemu-system-aarch64 &>/dev/null; then
    echo "Error: qemu-system-aarch64 not found. Install: brew install qemu"
    exit 1
fi

mkdir -p "${BUILD_DIR}"

echo "=== MarsOS: Building ARM64 image via Docker ==="
echo ""

# Build the Docker image
echo ">>> Building Docker build environment..."
docker build -f "${PROJECT_DIR}/Dockerfile.build" -t mars-os-builder "${PROJECT_DIR}"

# Run the build
echo ">>> Running ARM64 build (this will take a while)..."
docker run --rm --privileged \
    -e INCLUDE_DESKTOP="${INCLUDE_DESKTOP}" \
    -v "${BUILD_DIR}:/out" \
    mars-os-builder

IMG_FILE="${BUILD_DIR}/mars-os-arm64.img"

if [[ ! -f "${IMG_FILE}" ]]; then
    echo "Error: Build failed — no image produced."
    exit 1
fi

IMG_SIZE=$(du -h "${IMG_FILE}" | cut -f1)
echo ""
echo "=== Build Complete ==="
echo "  Image: ${IMG_FILE}"
echo "  Size:  ${IMG_SIZE}"
echo ""

# Boot in QEMU if requested
if [[ "${RUN_AFTER}" == "true" ]]; then
    echo "=== Launching in QEMU ==="
    bash "${SCRIPT_DIR}/test-qemu-arm64.sh"
fi
