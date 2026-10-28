# oci-kit: boundaries and acceptance criteria

Status: initial implementation, 2026-10-28. Target: Linux, Rust.

## Components

| Binary | Responsibility | Must not contain |
| --- | --- | --- |
| `rootfs-fetch` | Fetch an explicitly identified HTTPS artifact, verify SHA-256, atomically cache it | Pond/GitHub release discovery, a hard-coded distribution URL |
| `rootfs-inspect` | Read a tar/tar.gz/tar.zst archive without extracting it; report paths and metadata | Package removal decisions or platform assumptions |
| `rootfs-cleaner` | Plan or explicitly apply exact-path deletions in an offline staging directory | Distribution package lists, arbitrary hooks, package manager commands |
| `lighos-init` | Install/run a LightOS post-setup hook, initially restoring extra Arch keyrings | Reimplementation of LightOS account/SSH setup, claiming platform neutrality |

`lighos-init` preserves the spelling explicitly requested by the user. There is
no binary named `rootfs-init`. That name is reserved until a platform-independent
contract has been implemented and verified on multiple platforms, not merely
because the implementation could theoretically be abstracted.

Pond release discovery, cleanup policy, desktop decisions and image publishing
belong in `dockers-x/pond-oci`, not in the generic binaries. The explicitly
named LightOS adapter is separated into its own crate.

## Download contract

A versioned JSON source document supplies `url`, a required SHA-256, optional
size and arbitrary provenance metadata. Only HTTPS is accepted by default.
Existing files must pass verification before reuse. Interrupted or corrupt
downloads never replace the destination. A source sidecar records provenance.
The downloader neither executes a downloaded file nor extracts archives.

## Offline filesystem contract

Cleanup requires a separately marked staging directory; `/` is never accepted.
Rules are explicit, normalized relative paths, never glob patterns or commands.
The default is a dry run. Reject symlinked ancestors, including ancestors of the
staging root itself, and mounted subtrees before applying any rule. Open and pin
the root directory without following links. Resolve descendants relative to
pinned directory descriptors with Linux `openat2` and `RESOLVE_NO_XDEV`, including
during recursive deletion; a mountinfo snapshot alone is not a safety boundary.
Require Linux 5.6+, usable `openat2` and procfs; fail closed when unavailable.
Symlink leaves are removed as links, never followed. Preserve
all metadata of retained files by operating on an already extracted rootfs.
Package removal is not simulated by deleting package database records.
Inspect the staging marker through `O_PATH` before opening it for I/O; only a
verified regular inode may be reopened through its still-held procfs descriptor.

The caller owns an isolated staging tree and excludes untrusted concurrent
writers. The marker records explicit intent; it is not a sandbox or proof that
arbitrary files are safe to delete. A process that can rename pinned directories
outside staging or modify this process's mount namespace is outside that trust
boundary. Kernel-enforced no-cross-mount resolution still protects descendant
operations from mounts appearing after the initial snapshot.

Inspection must not extract an untrusted archive. Reject traversal and absolute
entry paths; report devices and links. Archive extraction is a separate,
privileged build step using a mature tool preserving owners/ACLs/xattrs.
Explicit text requests must resolve to a present regular UTF-8 file within the
documented limit. Truncated member payloads and compression-decoder errors must
not be reported as successful inspection.

## LightOS contract

The observed platform supplies `lightos-setup.service`, reads its own seed and
creates accounts. The kit does not source or copy seed secrets. A separate
oneshot runs after the platform's successful initialization marker, with a
versioned, non-secret profile and completion state. Failed operations leave no
success marker. SSH remains owned by LightOS unless explicitly redesigned.
Pond's ISO installer SSH patch does not modify its released rootfs.

Initially the adapter only restores configured pacman keyrings. Desktop VT,
PAM/logind and a Pond/Niri session require separate target-platform validation;
this first implementation must not advertise them as completed.

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo doc --workspace --no-deps`
- HTTP fixtures: correct digest, bad digest, size mismatch, interrupted stream,
  cached file verification, no partial destination and HTTP opt-in.
- Filesystem fixtures: dry run, exact paths, parent traversal, symlink escape,
  protected marker, root-ancestor links and kernel-enforced mount detection.
  Mutations use temporary fixtures only; a read-only `/proc` boundary probe
  verifies `EXDEV` without creating mounts or modifying the running rootfs.
- LightOS fixtures: install is offline; unknown profile fields fail; no command
  runs without explicit apply and the platform completion marker.

Public repositories contain source only. Rootfs and image redistribution rights
must be checked separately; no automatic public image publishing is assumed.

## CLI completion gate

Pond image packaging must not start until every agreed CLI function has runtime
acceptance evidence, the declared Rust minimum and Linux x86-64 static build are
verified, and independent review has no unresolved important finding. Exercise
real process interfaces and failure exit codes, not just library helpers. Use
isolated fixtures for initialization; never apply test profiles to the running
LightOS installation. Record the feature matrix and evidence in the work log.
Other architectures and the physical desktop are separate, explicitly bounded
acceptance targets, not silently claimed by the word "generic".
