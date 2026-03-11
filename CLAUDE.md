# MarsOS Development

## Build Environment

This project targets Linux (ARM64/x86_64). It **cannot be built on macOS** — Wayland dependencies like `xkbcommon`, `smithay-client-toolkit`, etc. are Linux-only.

### Building & Testing

- A QEMU ARM64 VM runs locally for building and testing
- SSH into VM: `sshpass -p mars ssh -o PubkeyAuthentication=no -p 2222 root@localhost`
- Transfer files to VM: `scp -P 2222 -o PubkeyAuthentication=no <files> root@localhost:<dest>`
- To build `dock` or other Rust projects, transfer the source to the VM and build there:
  - Transfer: `sshpass -p mars scp -P 2222 -o PubkeyAuthentication=no <files> root@localhost:<dest>`
  - Build: `sshpass -p mars ssh -o PubkeyAuthentication=no -p 2222 root@localhost "cd /root/dock && cargo build --release"`
  - dock source lives at `/root/dock/` on the VM
- Boot VM with: `bash scripts/test-qemu-arm64.sh`
- Do NOT run `cargo check` or `cargo build` on the host Mac — it will fail

### Overlay Workflow

- `overlays/` mirrors the VM filesystem — drop files at their target paths
- `scripts/apply-overlays.sh` copies overlays to running VM via SSH
- scp uses `-P` (uppercase) for port, not `-p` like ssh
