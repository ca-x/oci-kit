# Work log

## 2026-10-28 — Initial implementation

- Fixed the domain boundary before implementation: generic fetch/inspect/clean,
  an explicitly named LightOS adapter, and separate Pond release/build recipes.
- Implemented verified atomic fetching, read-only archive inspection and
  explicit offline cleanup policies; exercised them against temporary fixtures.
- Added `lighos-init` post-setup integration without taking ownership of platform
  account creation, credentials or SSH. Tested failure retry and idempotence.
- Downloaded and verified the Pond artifact, then inspected it without execution;
  the payload remains excluded from version control and public distribution.
- Read-only review identified root-ancestor symlink resolution and mount-check
  TOCTOU risks. Added three regression tests and observed all three fail on the
  original implementation before changing the filesystem implementation.
- Replaced ambient reopening and generic recursive deletion with pinned directory
  descriptors and `openat2` no-symlink/no-cross-mount traversal. Removed `cap-std`
  rather than adding another filesystem abstraction alongside it.
- Re-ran targeted tests: the mount-boundary probe and all nine cleanup integration
  tests pass. The mount test only queries existing procfs; it creates no mounts.
- Documented Linux requirements, the isolated-staging trust boundary and why
  distribution-neutral tools are not universal operating-system adapters.
- `crapper`, `dryer` and `mutator` are not installed in this environment; no result
  from these tools is claimed. Rust formatting, clippy and behavior tests provide
  the available automated checks.
- First full local verification passes: fmt check, clippy with warnings denied,
  26 Rust tests, rustdoc and release builds of all four binaries.
- Git had no author identity. Configured only these two repositories with the
  authenticated GitHub account's public name and noreply address; global Git
  configuration is unchanged. Aligned their initial branch names with `main`.
- Follow-up review closed the two original findings and identified a special-file
  marker risk: opening before checking the file type can invoke device behavior.
  A Unix socket fixture reproduces the wrong I/O-first error without any device
  access. Changed validation to pin with `O_PATH`, require a regular inode, then
  read that exact inode through its held procfs descriptor instead of its name.
- Final local verification passes with 27 Rust tests, fmt check, strict clippy,
  rustdoc and all release binaries. Read-only re-review closes all three safety
  findings and reports no new blocker in the fixes.
- GitHub API access works, but ordinary Git HTTPS and a bounded HTTP/1.1 retry
  fail or time out. Publication is pending a working authenticated transport;
  local commit success must not be reported as remote push success.

Source publishing and CI results are recorded only after their commands succeed.
OCI construction and a real LightOS/Pond boot remain separate acceptance gates.

## 2026-10-28 — Source publication over SSH

- The user explicitly requests SSH transport. Existing SSH configuration already
  maps GitHub to `ssh.github.com:443`; authentication succeeds as `czyt` using
  existing keys and known hosts. No SSH configuration or key changes are needed.
- Changed this repository's origin to `git@github.com:ca-x/oci-kit.git` and pushed
  initial source commit `2ef0ba1` to `main`; Git confirms the new remote branch and
  configures `origin/main` tracking.
- A missing ssh-agent did not mean that on-disk identities were unavailable;
  inspect the existing SSH configuration before diagnosing authentication.
- The earlier HTTPS failures do not block this SSH push. Remote CI and image
  construction still require their own evidence; publication alone proves neither.

## 2026-10-28 — CLI completion gate

- The user requires full CLI completion before starting the image project,
  continuous execution afterwards, and independent agents for review. No image
  workflow has been dispatched. The follow-up plan is
  `docs/superpowers/plans/2026-10-28-cli-completion.md`.
- Existing helper/fixture tests and source CI do not close process-interface,
  minimum-Rust or static-binary acceptance gaps. These are now explicit gates.
- A read-only adapter audit was requested, but the reviewer could not start due
  to model capacity. Its slot was closed. Review remains pending, not approved.
- Recorded the CLI-first ordering and continuous SSH/review workflow in Nowledge
  preference memory `64fa168c-9cf9-4f88-a874-00a8c5233986`.
- Task 1: observed three real inspector failures: absent requested text silently
  succeeded, and corrupt gzip/zstd checksums beyond the tar end marker escaped
  validation. Added regressions, observed all three fail, then drained the decoder
  and required every requested text path. All seven inspector tests now pass;
  the truncated-member test already passed and required no speculative fix.
- Added eight real generic CLI subprocess tests covering help/version, verified
  download/provenance/cache corruption, interrupted/digest/size/HTTP failures,
  source rejection, three archive formats/text errors, and marked cleanup with
  dry-run/apply/repetition/link safety/all-rule preflight. The initial cleanup
  test expected the wrong diagnostic wording; corrected the test, not production.
- All 39 workspace tests and strict clippy pass after this slice. The adapter's
  process contract, MSRV/static acceptance and whole-change review remain open.

## 2026-10-28 — Initialization runtime acceptance

- Generic CLI phase `a15b20c` is pushed over SSH; its source CI run
  `40659011161` succeeded. Pond packaging remains untouched.
- Added four adapter subprocess tests for help/version, deterministic dry-run,
  profile rejection, root refusal and repeatable offline installation of the
  actual executable/profile/unit/enable link. Observed explicit failure emitting
  a success plan, then changed application to emit one JSON result only after
  success, with `changed`; failure stdout is empty.
- Built real musl binaries locally using the installed Zig compiler and Rust's
  self-contained linker. Direct `zig cc` initially failed because cc-rs supplied
  a Rust target triple; a disposable workspace wrapper translates that argument.
  This is a local toolchain workaround, not a production dependency; CI uses
  musl-gcc. No GNU compiler is passed off as a musl compiler.
- Added a static, non-GPG pacman-key fixture and a process harness that uses
  rootless bubblewrap on this host, or disposable chroot on CI. It runs the real
  initializer as namespace root, mutating only disposable roots with fixture
  credentials. It does not change host system configuration/services, execute
  Pond rootfs programs, or perform network operations. Chroot is a trusted-code
  test harness, not a sandbox for hostile root programs.
- The first eight runtime checks caught child stdout contaminating JSON on
  success and failure. Observed failures, redirected child stdout to stderr,
  then re-ran the real static CLI successfully.
- Independent read-only contract audit confirms that account/seed/SSH lifecycle
  remains platform-owned, and that keyring is the specified adapter action, not
  complete desktop initialization. The audit found two important issues.
- Confirmed in the installed pacman-key source that populate can ignore import
  failure while `--add` propagates it. A failing-import/successful-populate fixture
  reproduced a false success state. Now explicitly add each preflighted keyring
  before populate; failures do not commit state.
- Observed unsafe state directory/link acceptance and blocking FIFO state reads.
  Now pin and validate root-controlled state ancestors, reject symlink/special
  lock/state files before I/O, bound state reads, use nonblocking no-follow opens,
  and sync the state directory after atomic replacement. Hostile concurrent root
  processes are outside the cooperative-lock contract.
- All 14 isolated real-CLI runtime cases pass, including failure retry, changed
  profiles, unchanged-run idempotence, real process lock contention, corrupted
  state, failed import, unsafe modes, symlinks and FIFO rejection. Added a ninth
  generic process case for links/devices/FIFOs without extraction. Its first tar
  fixture omitted UID/GID; fixed that invalid fixture, not production parsing.
- Full local workspace verification now covers 44 Rust tests. A fresh whole-code
  review and CI MSRV/static gates are still pending; this is not yet permission
  to begin image packaging.

## 2026-10-28 — MSRV and static acceptance gates

- Initialization phase `2052285` is committed and pushed over SSH.
- Installed Rust 1.89.0 without changing the default toolchain. The locked
  workspace tests pass on the declared minimum; musl release tests also run on
  1.89.0, including all real CLI subprocess interfaces.
- All four 1.89.0 musl binaries pass readelf checks for no INTERP/NEEDED entries
  and execute help/version. Their actual initializer and a 1.89.0 static fixture
  pass all 14 rootless isolated runtime tests. This is execution evidence, not
  merely a successful cross-link.
- Added CI jobs for MSRV tests, full musl release tests, linkage/execution checks
  and real initializer chroot acceptance. No image/rootfs/artifact upload step
  is added. Added `docs/cli-acceptance.md` as the per-command evidence/limit matrix.
- Workflow syntax passes actionlint 1.7.7 from its existing installation; the
  bare shim initially had no selected version, so no global mise setting was
  changed. Shell syntax and fixture Rust formatting also pass.
- Remote CI on these new gates and the fresh whole-change review remain pending.
