# Offline package keyrings for the LightOS recipe

Actual Pond run `40808148330` reaches pacman but rejects its signed repository:
the untouched rootfs has no initialized public keyring. Do not disable signature
checks or forge the platform account-completion marker to reuse runtime apply.

Add `lighos-init prepare-keyrings --root ROOT --config PROFILE [--apply]`:

- This is an offline LightOS/Arch build adapter, not a generic OS initializer.
- Require the existing marked staging-root contract; reject the running root,
  aliases, platform seed, initialized marker and remote authentication files.
  Inspect only their metadata, never their contents.
- Preflight native pacman-key and all public keyring files before any command.
  Include archlinux once, then explicitly declared extra rings once.
- Default is a JSON-only plan. Explicit apply requires root and invokes native
  `pacman-key --init`, then the existing add/populate sequence in an isolated
  bubblewrap PID/IPC/network/UTS/mount namespace with private proc/dev.
- Bind the pinned staging directory through the parent's proc fd, not a
  re-resolved mutable pathname. Import local public files without networking.
- Clear child environment, use fixed executable/PATH, stop on any failure and
  send native diagnostics to stderr. Never create accounts, credentials,
  systemd activation or kit/platform runtime-completion state.
- Repeat safely after interruption. Do not claim mutation counts from a
  synthetic pacman helper; actual GPG/repository acceptance remains a Pond gate.

GitHub-only sequence: observe new CLI subprocess RED, implement, pass existing
Rust/MSRV/static-runtime suites plus new staging refusals/command ordering,
independent review, then update the Pond pin. Install into staging before its
package transaction; clean generated identity/keyring state as appropriate
before the final image. No local tests, downloads, compilation or formatting.
