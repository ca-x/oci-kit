# CLI acceptance matrix

Date: 2026-10-28. Scope: the four Linux binaries in `SPEC.md`, not arbitrary
operating systems or a validated Pond desktop. Generic mechanisms accept external
policy; release discovery and distribution/platform recipes stay outside them.

## Functional evidence

| Command | Accepted behavior | Evidence |
| --- | --- | --- |
| All four | Help/version, argument parsing, useful nonzero failure exits | Both crates' `tests/cli.rs` run actual binaries |
| `rootfs-fetch` | Explicit source schema, SHA-256, optional size/provenance; HTTPS by default, HTTP opt-in | `tests/fetch.rs` and generic CLI tests |
| `rootfs-fetch` | Verified streaming download and cache reuse, corrupt cache refusal, no partial destination on digest/size/interruption/status failure | HTTP fixture process tests; provenance sidecar compared to input |
| `rootfs-inspect` | Magic-based tar/gzip/zstd, metadata/ownership/mode, selected subtrees and limited UTF-8 text | Library and subprocess archive fixtures |
| `rootfs-inspect` | Reject unsafe paths, missing/nonregular/oversize/non-UTF-8 text, truncated members and corrupt compression trailers | Observed red-to-green regressions and subprocess failures |
| `rootfs-inspect` | Report devices/FIFOs/links, including external link targets, without extracting/following them | Actual archive subprocess test checks metadata and unchanged directory contents |
| `rootfs-cleaner` | Explicit mark, default dry-run, exact-path policy, apply/repeat, all-rule preflight, leaf-link deletion only | `tests/clean.rs` and generic subprocess tests |
| `rootfs-cleaner` | Refuse live root, unsafe paths, marker removal, ancestor links and mounted descendants | Filesystem tests, procfs kernel-boundary probe and subprocess refusals |
| `lighos-init install` | Offline copy of actual binary, profile, unit and enable link, repeatable only in marked staging | Adapter library and subprocess tests check bytes/mode/link and run installed `--version` |
| `lighos-init apply` | Strict non-secret profile, deterministic plan without mutation; explicit root/platform prerequisite | Profile/subprocess tests and isolated real-CLI runtime harness |
| `lighos-init apply --apply` | One JSON success result, `changed`, stderr diagnostics; no success stdout on failure | Real static binary under bubblewrap/chroot |
| `lighos-init apply --apply` | Preflight every keyring; checked `--add` then `--populate`; no completion on command failure | Static executable fixture logs argv and models import failure hidden by populate |
| `lighos-init apply --apply` | State fingerprint, unchanged-run skip, config-change rerun, failure retry, process lock, state schema rejection | Independent real processes sharing only a disposable test root |
| `lighos-init apply --apply` | Reject untrusted modes, state-directory links, state/lock links and FIFOs without blocking | Isolated runtime refusals, bounded subprocess timeouts |

The runtime harness executes the production initializer but substitutes a tiny
static `pacman-key` fixture. It verifies process arguments, exit handling and
state, not GPG cryptography or the target distribution's packaged trust data.
Actual Pond keyring/systemd/desktop behavior still needs image/platform
acceptance. No real seed, password, SSH key or host keyring is used in tests.

## Desktop extension gate

The ordering extension passed full GitHub run `40684692564` at `e682f78` and
independent review. The Pond recipe's actual profile then passed source and
real-toolkit installation in run `40713790188` at `9b0d20e`, including repeated
installation with opaque platform settings present and absent.

The numeric render-node generator mechanism passed full remote run
`40718742320` at `30f7651` and fresh review without P1/P2 after meaningful
schema/install RED `40715702736`. The full run exercises 49 Rust tests per
required build and 20 chroot cases. See
`docs/superpowers/plans/2026-10-28-render-node-groups.md`. The expanded
chroot harness uses synthetic device metadata only, never device I/O or host
device chmod/chown. Generated group policy is not evidence that systemd has
started both services with those credentials or that actual GPU rendering,
the Pond shell, persistent Niri owner or platform proxy works.

The desktop extension gate remains open. Do not resume the suspended headless
Pond image pipeline based on the historical keyring-only completion below.

## Build and execution gates

- Native Linux: 44 Rust tests; formatting, strict clippy, rustdoc and release
  builds pass locally.
- Declared MSRV: Rust 1.89.0 is installed and exercised, not inferred from the
  manifest. Locked GNU workspace tests and musl release tests run on that compiler.
- Static Linux x86-64: all four binaries must have no ELF `INTERP` or `NEEDED`
  entry, and must execute help/version. Release-mode musl tests exercise their
  actual CLI interfaces, not only a successful link step.
- Initialization: 14 real-static-CLI tests run in rootless bubblewrap locally;
  CI uses a disposable chroot and the same executable fixture. These are trusted
  code test harnesses, not a claim to sandbox malicious privileged programs.
- Source CI, MSRV CI and static/runtime CI must all succeed on the published
  source revision. The workflow uploads no rootfs or image.
- Independent whole-change review must have no unresolved important finding.

The CLI gate passed on code/workflow revision
`cbf327f0fa20b4e87b491b14aeed373f20cf223b`: GitHub run `40662397249` completed
`rust`, `msrv` and `static-runtime` successfully, including all 44 Rust tests
on each required test build and all 14 chroot runtime cases. An independent
whole-change review of this revision found no Critical/Important/Minor defect,
re-ran the native tests, static execution checks and 14 bubblewrap cases, and
closed both earlier initialization findings. Later documentation-only evidence
updates do not change these reviewed binaries or workflow definitions.

That initial gate permitted the first construction phase; it did not claim an
image or boot had passed. The later desktop requirement reopens the gate as
described above, and packaging now remains suspended.

## Reproduction

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo doc --locked --workspace --no-deps
cargo +1.89.0 test --locked --workspace

rustup target add x86_64-unknown-linux-musl
export CC_x86_64_unknown_linux_musl=musl-gcc
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc
cargo test --locked --release --workspace --target x86_64-unknown-linux-musl
bash scripts/check-static.sh target/x86_64-unknown-linux-musl/release
rustc --edition 2024 --target x86_64-unknown-linux-musl -C linker=musl-gcc \
  tests/fixtures/pacman-key.rs -o target/pacman-key-fixture
python3 scripts/test-init-runtime.py \
  --binary target/x86_64-unknown-linux-musl/release/lighos-init \
  --pacman-key-fixture target/pacman-key-fixture --runner bwrap
```

Install `musl-tools` and bubblewrap on the test/build host as appropriate. CI
uses `sudo python3 ... --runner chroot` on an ephemeral runner instead of
requiring unprivileged user namespaces. Never substitute a host-root invocation
of `lighos-init apply --apply` for this fixture acceptance.

## Deliberate limits

- Linux only. Protected staging and trusted runtime-state root resolution need
  Linux 5.6+ `openat2`; staging also needs procfs. There is no unsafe fallback.
- Cleanup is exact deletion, not package-manager uninstallation or archive
  extraction; callers supply isolated staging and exclude hostile writers.
- Runtime state is root-controlled. Locks coordinate cooperating initializers,
  not hostile root directory/file replacement. Completion is per tool version
  and serialized profile, not a continuous keyring-health monitor.
- Account/home/password/SSH setup remains owned by LightOS. No unobserved seed
  fields are invented, and installer-only SSH patches are not rootfs changes.
- No generic account/locale initializer or other-platform adapter is promised.
  Pond/Niri, VT, PAM/logind, aarch64 and actual boot acceptance are separate work.
