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
