# Explicit render-node groups without host device mutation

Status: mechanism accepted at `30f7651`; actual GPU/platform acceptance pending.

## Contract

- Extend only the LightOS adapter with optional `render_node_services` in its
  non-secret schema-v1 profile. Each unique recipient must already be in
  `post_setup_services`, so device policy cannot bypass platform initialization.
  Existing profiles keep their serialized representation and behavior.
- Opt-in authorizes those exact service processes to the numeric groups of
  mapped `/dev/dri/renderD<number>` character devices. Do not infer a username
  from seed/auth, create accounts/groups, or chmod/chown a device. Do not grant
  groups from card/input/TTY nodes or name-based render/video groups. Group membership is
  a group-wide authorization, not an ACL limited to the individual device.
- Install a `lighos-init-generator` symlink to the same Rust binary in the
  systemd system-generator directory. Dispatch its standard three-directory
  invocation by executable name (also accept systemd's one-directory debug
  convention); write only the normal generator output.
  No package-specific shell, new daemon or service activation is needed.
- Read and pin the device directory and nodes without following symlinks.
  Accept only DRM major 226 and a minor matching the renderD suffix, with a
  nonzero, non-sentinel numeric GID. Reject malformed selected nodes before
  writing any service policy. An absent directory or no render nodes is a
  successful no-op, preserving software-rendered graphical operation.
- Write deterministic namespaced service drop-ins containing additive
  `SupplementaryGroups=` numeric IDs, sorted and deduplicated. Never reset
  package-managed group policy, replace a unit, enable/start a service, read
  platform auth, or write runtime completion state.
- Generator output is ephemeral: systemd clears it and invokes generators on
  manager startup/reload. Devices mapped after generation require a platform
  reload; this is not a hotplug controller. All recipients must be stopped
  before expecting new process credentials. Persistent owner upgrades must not
  be restarted merely to refresh group policy.
- Trust systemd's root-controlled output tree and platform-controlled mapped
  device directories, not concurrent malicious root writers. Reject linked or
  writable output paths, and do not follow a preexisting drop-in link.

## Execution

1. Add real-CLI install and disposable-chroot generator regressions. Observe
   RED in GitHub on the old binary, without downloading a Pond rootfs or
   starting a systemd service on the running host.
2. Implement the smallest Rust policy/generator adapter; run native/MSRV and
   static runtime gates remotely, including mknod fixtures that are never
   opened for device I/O. Record what these fixtures do and do not prove.
3. Obtain fresh independent review; fix important findings with observed
   regressions. Only then select the new toolkit/profile in the Pond recipe.
4. Keep full Pond shell, actual service credentials/GPU use, persistent native
   Niri owner, proxy and optional physical VT acceptance separate. Do not
   resume the old headless OCI build or claim desktop completion.

## Evidence ledger

- Ruling: systemd generator output is the process-credential policy boundary,
  avoiding runtime edits to `/etc/group` and a reload inside initialization.
  If generator-time mounts differ from the platform's final device mapping,
  no hotplug support is implied; platform reload and runtime acceptance remain.
- All builds, fixture execution and dependency downloads stay on GitHub.
- RED run `40715702736`, source `ec368373c9b44193e9280b92be38615b8bcf33d0`:
  native, MSRV and musl process tests all reject the new valid profile with
  `unknown field render_node_services`. The four prior ordering tests pass.
  Those failures stop the later chroot step; generator runtime cases have not
  yet executed and must pass after implementation.
- GREEN: complete run `40718742320` at
  `30f76519e9d3c809c892fa1d71e1f8a23d87c577` passes stable, MSRV and static
  runtime gates, including 49 Rust tests per required build and 20 isolated
  chroot cases. The first implementation's `40717635975` had already passed
  MSRV/static-runtime; only its runner-reported import order needed correction.
- Final review: a fresh read-only reviewer found no P1/P2. Clarify the existing
  group-wide contract: not discovering card/input/TTY groups is not absolute
  isolation from same-GID resources. No new permission mechanism is implied.
- Final: minor (deferred): separately opt in owner versus transport in the
  generator fixture. The implementation already iterates only selected
  services; present positive cases select both services.
- Final: minor (deferred): lock down one-directory debugging and empty
  early/late output with explicit assertions. Current source accepts one/three
  arguments and writes only the normal directory.
