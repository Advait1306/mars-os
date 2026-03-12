# SyncFS — FUSE-based Home Directory with Cloud Sync

## Overview

Each user's home directory (`/home/<user>/`) is a FUSE filesystem from first login. All file operations are intercepted by a per-user daemon that persists data to a local cache and streams operations to a cloud server in real time. Other devices receive the same operation stream and replay it locally, keeping all devices in sync without polling, hashing, or diffing.

The system is user-agnostic — any user who logs in gets a FUSE-backed home directory, triggered automatically via PAM.

## Architecture

```
┌──────────────────────────────────────────────────┐
│                  User Applications               │
│           (Dolphin, apps, shell, etc.)            │
└──────────────────┬───────────────────────────────┘
                   │ syscalls (open, read, write, rename, ...)
                   ▼
┌──────────────────────────────────────────────────┐
│           FUSE Mount: /home/<user>/              │
│          (one daemon per logged-in user)         │
│                                                  │
│                 syncfs daemon                    │
│         (Rust, using fuser crate)                │
│                                                  │
│  ┌─────────────────┐   ┌──────────────────────┐  │
│  │  Local Cache     │   │  Op Stream Writer    │  │
│  │  r/w to:         │   │  Streams ops over    │  │
│  │  /var/lib/syncfs/│   │  WebSocket/gRPC to   │  │
│  │  <user>/         │   │  cloud server        │  │
│  └─────────────────┘   └──────────────────────┘  │
└──────────────────────────────────────────────────┘
                                    │
                                    ▼
                   ┌───────────────────────────┐
                   │       Cloud Server        │
                   │                           │
                   │  ┌─────────────────────┐  │
                   │  │   Operation Log     │  │
                   │  │   (append-only)     │  │
                   │  └─────────────────────┘  │
                   │  ┌─────────────────────┐  │
                   │  │   Object Store      │  │
                   │  │   (file content)    │  │
                   │  └─────────────────────┘  │
                   │  ┌─────────────────────┐  │
                   │  │   Device Registry   │  │
                   │  │   (connected peers) │  │
                   │  └─────────────────────┘  │
                   └──────────┬────────────────┘
                              │
                    ┌─────────┴─────────┐
                    ▼                   ▼
              Device B              Device C
              (replays ops)         (replays ops)
```

## Components

### 1. syncfs daemon (Rust binary)

The core of the system. Implements the `fuser::Filesystem` trait. One instance runs per user, mounting at `/home/<user>/`. Each daemon handles that user's file operations independently.

#### Responsibilities

- **Passthrough to local cache**: All reads/writes go to `/var/lib/syncfs/<user>/` on the real filesystem (ext4/btrfs)
- **Operation streaming**: Every mutating operation (write, create, rename, unlink, mkdir, rmdir, setattr, symlink, link) is serialized and sent to the cloud server
- **Inbound replay**: Receives operations from other devices via the cloud server and applies them to the local cache
- **Offline queue**: If the network is down, ops are queued to a local WAL (write-ahead log) and flushed when connectivity returns
- **Conflict handling**: Sequence-based ordering with last-writer-wins per file, or fork-on-conflict for simultaneous edits

#### Key design decisions

- The daemon is a **single-threaded async** process (tokio) with the FUSE session on a dedicated thread
- Local cache is the source of truth for reads — the daemon never blocks a read on network
- Writes return immediately after persisting to local cache; cloud sync is async
- Large file writes are chunked (e.g., 4MB chunks) so the stream doesn't send multi-GB payloads as single ops

### 2. Operation stream protocol

Every mutating filesystem operation becomes a message in an ordered stream.

#### Op message format

```rust
struct SyncOp {
    seq: u64,              // monotonically increasing per-device
    user_id: Uuid,         // which user account this belongs to
    device_id: Uuid,       // which device originated this
    timestamp: u64,        // unix millis
    op: OpKind,
}

enum OpKind {
    Create { path: String, mode: u32 },
    Write { path: String, offset: u64, len: u64, chunk_id: Uuid },
    Truncate { path: String, size: u64 },
    Rename { from: String, to: String },
    Unlink { path: String },
    Mkdir { path: String, mode: u32 },
    Rmdir { path: String },
    Symlink { target: String, link: String },
    Link { src: String, dst: String },
    SetAttr { path: String, attrs: AttrDelta },
    SetXattr { path: String, name: String, value: Vec<u8> },
    RemoveXattr { path: String, name: String },
}
```

#### Data transfer

- `Write` ops reference a `chunk_id` rather than inlining data
- Chunks are uploaded separately to the object store (S3-compatible)
- This keeps the op stream lightweight and allows deduplication

### 3. Cloud server

A relatively simple service that acts as an ordered message broker + object store.

#### Subcomponents

**Operation log** (Postgres or SQLite + Litestream):

- Append-only log of all ops across all devices
- Each op has a global sequence number assigned on receipt
- Devices poll with "give me ops after global_seq N" or subscribe via WebSocket

**Object store** (S3-compatible — MinIO for self-hosted):

- Stores file content chunks referenced by `chunk_id`
- Content-addressable (chunk_id = hash of content) for dedup
- Garbage collected when no ops reference a chunk

**Device registry**:

- Tracks connected devices, their last-seen global_seq, and online status
- Handles auth (device tokens tied to user accounts)

#### API surface

```
POST   /users/{user_id}/ops              — submit new ops (from device)
GET    /users/{user_id}/ops?after=N       — poll for new ops
WS     /users/{user_id}/ops/stream        — subscribe to real-time ops
POST   /users/{user_id}/chunks/{id}       — upload a data chunk
GET    /users/{user_id}/chunks/{id}       — download a data chunk
POST   /users/{user_id}/devices/register  — register a new device
POST   /auth/login                         — authenticate, get token
POST   /auth/register                      — create account
```

### 4. Local WAL (offline support)

When the network is unavailable:

1. Ops are written to a local WAL file (`/var/lib/syncfs/<user>/wal/pending.log`)
2. Chunks are stored locally in `/var/lib/syncfs/<user>/wal/chunks/`
3. When connectivity returns, the daemon replays the WAL to the server in order
4. After server acknowledgement, WAL entries are pruned

The WAL uses a simple append-only binary format with CRC checksums per entry.

### 5. Inbound sync (receiving changes from other devices)

The daemon maintains a persistent WebSocket connection to the server. When ops arrive:

1. Check for conflicts (see below)
2. Apply the op to the local cache:
   - `Create` → create file in cache
   - `Write` → download chunk from object store, write to cache at offset
   - `Rename` → rename in cache
   - `Unlink` → delete from cache
   - etc.
3. Update the local global_seq cursor
4. The FUSE layer automatically serves the updated data on next read

### 6. Conflict resolution

Conflicts arise when two devices modify the same file before syncing.

**Strategy: last-writer-wins with conflict copies**

- Each file tracks a vector clock: `{ device_a: 5, device_b: 3 }`
- If an inbound op's causal history doesn't include the local version, it's a conflict
- On conflict:
  - The server's version wins (it has the canonical order)
  - The local version is saved as `filename.conflict-<device_id>-<timestamp>`
  - A desktop notification informs the user
- For directories, conflicts are resolved structurally (merges are usually safe)

**Future improvement**: For text files, operational transform or CRDT-based merging could avoid conflict copies entirely.

## Systemd integration

### Template unit file: `syncfs@.service`

A systemd **template unit** — the `%i` parameter is the username. One instance runs per user.

```ini
[Unit]
Description=SyncFS - FUSE home directory for %i
DefaultDependencies=no
After=local-fs.target network-online.target
Wants=network-online.target
RequiresMountsFor=/var/lib/syncfs

[Service]
Type=notify
ExecStartPre=/usr/bin/mkdir -p /var/lib/syncfs/%i
ExecStart=/usr/bin/syncfs --user %i --mount /home/%i --cache /var/lib/syncfs/%i
Restart=always
RestartSec=1
WatchdogSec=10

# Security hardening
ProtectSystem=strict
ReadWritePaths=/var/lib/syncfs/%i /home/%i
PrivateTmp=yes
NoNewPrivileges=yes

[Install]
WantedBy=multi-user.target
```

Usage: `systemctl start syncfs@mars`, `systemctl start syncfs@alice`, etc.

### PAM integration: auto-mount on login

A PAM module triggers the FUSE mount when any user logs in, so no per-user systemd enablement is needed.

**`/etc/pam.d/common-session`** (appended):
```
session optional pam_exec.so /usr/lib/syncfs/pam-mount.sh
```

**`/usr/lib/syncfs/pam-mount.sh`**:
```bash
#!/bin/bash
# Called by PAM on session open/close
# PAM_USER and PAM_TYPE are set by pam_exec

if [ "$PAM_TYPE" = "open_session" ]; then
    # Only mount if not already mounted
    if ! mountpoint -q "/home/$PAM_USER"; then
        systemctl start "syncfs@${PAM_USER}.service"
    fi
fi

# On close_session: leave mounted — other sessions may still be active.
# The daemon stays up; systemd handles lifecycle.
```

This means:
- First user logs in → PAM triggers `syncfs@alice.service` → FUSE mounts `/home/alice`
- Second user logs in → PAM triggers `syncfs@bob.service` → FUSE mounts `/home/bob`
- User logs out → daemon keeps running (other sessions, background processes may need it)
- Daemon is stopped explicitly or on shutdown

### Pre-login users (SDDM greeter)

SDDM runs as the `sddm` user, which does NOT need a synced home dir. The PAM script only triggers for real users (uid >= 1000):

```bash
# Skip system users
USER_ID=$(id -u "$PAM_USER" 2>/dev/null)
if [ -z "$USER_ID" ] || [ "$USER_ID" -lt 1000 ]; then
    exit 0
fi
```

### Boot sequence

```
local-fs.target
      │
      ▼
sddm.service              ← login screen (no syncfs needed)
      │
      ▼
user logs in via SDDM
      │
      ▼
PAM open_session
      │
      ▼
syncfs@<user>.service      ← mounts FUSE at /home/<user>
      │
      ▼
user session starts        ← Plasma sees /home/<user> as normal
```

### Crash recovery

- systemd restarts the per-user daemon within 1 second (`RestartSec=1`)
- The daemon re-mounts FUSE, reopens the local cache, and resumes
- Any in-flight ops are re-read from the local WAL
- Watchdog (sd_notify) ensures systemd detects hangs, not just crashes
- Other users' daemons are unaffected — each is an independent process

## Excluded paths

Not everything in the home dir should sync. The daemon maintains an exclusion list:

```
# Default excludions — never sync these
.cache/
.local/share/Trash/
.local/share/baloo/
.nv/
.mozilla/firefox/*/cache2/
.config/pulse/
snap/
Downloads/          # optional, user-configurable
```

These paths still work normally (passthrough to local cache) but generate no sync ops.

## File layout in this repo

```
syncfs/
├── Cargo.toml
├── src/
│   ├── main.rs              — CLI, systemd notify, mount setup
│   ├── fs.rs                — fuser::Filesystem trait impl
│   ├── cache.rs             — local cache management (r/w to /var/lib/syncfs/)
│   ├── ops.rs               — SyncOp types and serialization
│   ├── stream.rs            — WebSocket client, op send/receive
│   ├── wal.rs               — write-ahead log for offline support
│   ├── conflict.rs          — conflict detection and resolution
│   ├── config.rs            — exclusion list, server URL, device ID
│   └── auth.rs              — user authentication, token management
├── pam/
│   └── pam-mount.sh         — PAM hook to start per-user daemon
├── tests/
│   ├── fs_test.rs           — FUSE operation tests (mount in tmpdir)
│   └── sync_test.rs         — op stream round-trip tests
└── syncfs@.service          — systemd template unit file
```

## Implementation order

### Phase 1 — Local FUSE passthrough

- Implement `Filesystem` trait with full passthrough to a local cache dir
- Mount at a test path (not /home yet), verify all ops work
- Goal: `cp`, `ls`, `cat`, `vim`, `cargo build` all work transparently on the mount

### Phase 2 — Operation capture

- Add `SyncOp` serialization for all mutating callbacks
- Write ops to local WAL
- Add a CLI tool to dump/inspect the WAL (debugging)
- Goal: every file change produces a replayable op

### Phase 3 — Cloud server (MVP)

- Simple Rust server (axum) with Postgres op log + S3 chunk store
- Implement the API surface (submit ops, poll ops, upload/download chunks)
- Goal: single device can push ops to server and pull them back

### Phase 4 — Multi-device sync

- Inbound op replay on the FUSE daemon
- WebSocket subscription for real-time sync
- Conflict detection and resolution
- Goal: two VMs with syncfs see each other's changes in real time

### Phase 5 — Production hardening

- Systemd integration, watchdog, boot ordering
- Exclusion list and configuration
- Offline queue + reconnection logic
- KDE Plasma system tray widget (sync status indicator)
- Dolphin integration (sync status icons on files)
- Goal: usable as the real /home/mars on MarsOS

### Phase 6 — Performance

- Chunk deduplication (content-addressable storage)
- Lazy fetching (don't download file content until first read)
- Batching small ops (coalesce rapid writes)
- Compression on the wire (zstd)
- Goal: handles large home directories without noticeable lag
