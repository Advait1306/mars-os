# Running MarsOS on Mac with UTM

UTM provides GPU-accelerated ARM64 virtualization on Apple Silicon Macs via QEMU + HVF, with 3D acceleration through virgl/ANGLE/Metal.

## Prerequisites

- macOS on Apple Silicon (M1/M2/M3/M4)
- [UTM](https://mac.getutm.app/) — install with `brew install --cask utm`
- The MarsOS disk image at `build/mars-os-arm64.qcow2`

## Quick Start

If the VM has never been set up:

```bash
bash scripts/setup-vm.sh
```

This creates a "MarsOS" VM in UTM with the correct settings. Once created, boot it with:

```bash
bash scripts/test-qemu-arm64.sh
```

Or use the UTM GUI directly.

## What `setup-vm.sh` Does

The script automates VM creation in five steps:

1. **Creates the VM via AppleScript** — UTM's supported scripting API. This registers the VM with UTM and generates a valid `.utm` bundle in `~/Library/Containers/com.utmapp.UTM/Data/Documents/`.

2. **Quits UTM** — forces it to flush the default config to disk (`config.plist` inside the `.utm` bundle).

3. **Configures the VM via PlistBuddy** — modifies the config.plist to set:
   - CPU: `host` (native speed via HVF), 4 cores, 4 GB RAM
   - Display: `virtio-gpu-gl-pci` (GPU-accelerated via virgl → ANGLE → Metal)
   - Network: Emulated mode with port forwarding `localhost:2222 → guest:22`
   - Sound: `intel-hda`
   - UEFI boot enabled

4. **Hard-links the disk image** — replaces the empty disk created by UTM with a [hard link](https://en.wikipedia.org/wiki/Hard_link) to `build/mars-os-arm64.qcow2`. A hard link means both paths point to the same data on disk — no space is duplicated, and writes from either path (UTM or build scripts) affect the same file. Falls back to a full copy if hard linking fails (e.g., cross-volume).

5. **Relaunches UTM** — picks up the updated config.

## Day-to-Day Usage

```bash
# Start the VM
utmctl start MarsOS

# SSH in (password: mars)
ssh -p 2222 root@localhost

# Stop the VM
utmctl stop MarsOS

# Serial console (if configured)
utmctl attach MarsOS
```

If `utmctl` is not in your PATH, use the full path:

```
/Applications/UTM.app/Contents/MacOS/utmctl
```

## Applying Overlays

With the VM running and SSH accessible:

```bash
bash scripts/apply-overlays.sh
```

This copies files from `overlays/` into the VM and applies KDE configs.

## Deleting the VM

Because the disk image is hard-linked, UTM cannot delete the `.utm` bundle directly — you'll get a "couldn't be removed" error. To delete the VM:

1. Stop the VM if running: `utmctl stop MarsOS`
2. Remove the hard-linked disk image inside the bundle first:
   ```bash
   rm ~/Library/Containers/com.utmapp.UTM/Data/Documents/MarsOS.utm/Data/*.qcow2
   ```
3. Now delete "MarsOS" from UTM (right-click → Delete)

Your original `build/mars-os-arm64.qcow2` is not affected — removing one side of a hard link does not touch the other.

## Rebuilding the VM

After rebuilding the disk image or to start fresh:

1. Delete the VM (see above)
2. Run `bash scripts/setup-vm.sh` again

## VM Settings Reference

| Setting      | Value                               |
| ------------ | ----------------------------------- |
| Backend      | QEMU                                |
| Architecture | aarch64                             |
| CPU          | host (HVF accelerated)              |
| Cores        | 4                                   |
| RAM          | 4096 MB                             |
| Display      | virtio-gpu-gl-pci (GPU accelerated) |
| Network      | Emulated, virtio-net-pci            |
| SSH          | localhost:2222 → guest:22           |
| Sound        | intel-hda                           |
| Disk         | VirtIO, qcow2                       |
| Boot         | UEFI                                |
| USB          | 3.0 (xHCI)                          |

## Troubleshooting

**VM won't start / blank display**: Check UTM settings in the GUI — ensure a Display device is configured with `virtio-gpu-gl-pci`.

**SSH connection refused**: Verify the VM has finished booting (check the UTM display window for a login prompt). Confirm port forwarding is set: UTM settings → Network → Port Forward should show TCP 127.0.0.1:2222 → :22.

**"couldn't be removed" when deleting VM**: The disk image is hard-linked. See the [Deleting the VM](#deleting-the-vm) section above.

**GPU acceleration not working (software rendering)**: Inside the guest, check `glxinfo | grep renderer`. If it shows `llvmpipe`, the virtio-gpu driver may not be loaded. Ensure the guest kernel has `CONFIG_DRM_VIRTIO_GPU` enabled (Debian includes this by default).
