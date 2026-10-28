# Work log

## 2026-10-28 — Desktop dependency gate remote verification

- Run `40677322538` passed the minimum-Rust and static-runtime jobs, including
  the new platform-service process tests. The stable job stopped at two
  runner-reported rustfmt differences; apply exactly those source changes and
  require a fresh full run. No build, formatter or test ran on this host.
- Retrieved the LightOS lifecycle, seed-injection and remote-authentication
  memories again. Keep platform-owned accounts and authentication unchanged;
  a remote graphical session does not require a physical VT.

## 2026-10-28 — GitHub-only execution policy

- The user now requires all downloads and builds on GitHub. From this point,
  dependency/rootfs downloads, Rust compilation and runtime acceptance run only
  in GitHub Actions. Local work is limited to source editing, static review,
  SSH source pushes and workflow/log inspection; no artifact download is needed.
- Documented remote workflow dispatch without adding GitHub-specific behavior
  to the reusable CLI mechanisms. Earlier local evidence remains historical;
  it does not authorize further local builds or downloads.
- Confirmed the completed CLI gate's documentation head `daaf572` also passes
  all three GitHub CI jobs in run `40663154259`, after the source/runtime gate
  at `cbf327f` passed run `40662397249`.

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

## 2026-10-28 — CLI completion accepted

- Static/MSRV phase `cbf327f0fa20b4e87b491b14aeed373f20cf223b` is pushed over SSH.
  GitHub run `40662397249` is successful; inspected the individual jobs and logs:
  `rust`, `msrv` and `static-runtime` all succeeded. Stable/native, Rust 1.89 and
  musl release each passed 44 Rust tests; static checks executed all four
  binaries; the disposable chroot ran all 14 initializer cases successfully.
- Fresh independent whole-change review of baseline `f7b5f12` through `cbf327f`
  reports no Critical/Important/Minor finding. It re-ran 44 native tests, strict
  clippy/formatting, 14 bubblewrap cases and all four static binary checks.
  Additional temporary probes rejected writable ancestors, differently owned
  state, Unix sockets and oversized state. No source/index/host setup changed.
- Both important contract-audit findings are independently closed. The evidence
  matrix now closes every documented CLI function and states the remaining
  platform/external-command limits. Completion is not inferred from source-only
  CI, a cross-link, or a reviewer that could not start.
- All four tasks in `docs/superpowers/plans/2026-10-28-cli-completion.md` are
  complete. Image packaging is now eligible as the next phase; no image/rootfs
  publishing or Pond build happened before this acceptance. The accepted scope
  excludes unimplemented generic account/locale features and physical desktop
  claims rather than pretending that they work.
- Removed only the empty nested directory left by the earlier wrong-CWD patch;
  it contained no files. The final review slot is closed and its workspace can
  be removed after this durable log is saved.

## 2026-10-28 — Platform contract and reopened desktop gate

- User requests further LightOS underlying-implementation memory retrieval.
  Retrieved inner runc/outer core observations, three-stage seed/runtime config
  injection, admin-proxy credential ownership and optional physical VT records.
  Read only the current setup script, never the actual seed/auth environment.
- Ruling: desktop does not imply a physical VT. vt=false remote instances must
  still run a graphical desktop. Do not invent a registration API, bypass the
  admin proxy or overwrite platform credentials. The Niri session still needs
  real support; the remote program's Hyprland owner is not a substitute.
- Documented those boundaries in `docs/lightos-platform-contract.md`. First
  extension: explicitly order configured platform services after successful
  post-setup initialization, without starting/enabling or replacing them. A
  skipped systemd Condition is not a successful dependency readiness gate.
- Added real offline CLI installation regressions and invalid-unit cases before
  production changes. All executable tests remain GitHub-only. This reopens,
  not silently closes, the original toolkit completion gate.
- Run `40676051140` observed the new valid-profile CLI installation fail in
  MSRV and static-musl tests, before the feature existed. The formatter also
  requested two test layout changes; use that runner-produced diff locally
  without executing rustfmt here. Tightened negative tests to require the
  platform-service validation diagnostic, not an unrelated absent-root error.
- Implemented optional `post_setup_services`, validated names/count/duplicates
  and obvious initialization cycles, and installed only ordering drop-ins.
  Changed the marker Condition to Assert for fail-closed dependencies. Empty
  lists are omitted from serialization to preserve original profile identity.
  GitHub GREEN and independent review are pending; other desktop work stays open.
