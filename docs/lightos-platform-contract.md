# LightOS integration contract

## Evidence and limits

The user's cross-tool records describe observations from 2026-10-23/24, not a
universal ABI for all future LightOS versions. Source references in Nowledge:

- `d50950ca-e41a-412b-be7a-c860e033bd78` and
  `33cd8f6a-acee-47a4-a4a2-2bd4d051003c`: privileged outer lightos-core,
  systemd and inner runc instance bundles.
- `79fad488-af6a-40ec-afe5-77b5202d77d2`: three separate configuration moments.
- `e92861cf-d6e1-4800-b6b2-df94ecc4ca5a`: the admin proxy/authentication chain.
- `28191210-5f57-45ec-bf4e-48c5a282e6b3` and
  `f9b824cd-a964-482e-98fe-8f2cb8b2d701`: optional VT ownership and ponzS's
  controlling-terminal/seatd adaptation, not a known VT-manager socket API.

The current instance's setup script and service were read without reading the
actual seed or authentication files. Its successful marker remains
`/var/lib/lightos/initialized`, after seed consumption and account creation.
No host account, device permission or service was changed.

## Image is not runtime configuration

An OCI image supplies the filesystem and default process, not a completed runc
bundle, account database, assigned VT or external registration protocol.
LightOS owns rootfs materialization, namespaces, device mounts, storage and
network policy. The kit must not re-create these from inside an instance.

1. At instance creation, admin writes seed.env and the platform setup unit into
   the materialized rootfs. This is not a secret baked into an OCI build layer.
2. At first boot, platform setup consumes the seed, creates the account and
   other instance identity, removes the seed and writes its success marker.
3. At runtime, admin can continue writing the SSH/remote-desktop environment
   files and restarting their services. The first-boot marker does not prove
   that all later remote-desktop configuration has arrived.

The kit neither parses/sources seed.env nor creates competing account/SSH
initializers. Extra keyring import remains after platform setup. A systemd
dependency must fail when the success marker is absent: a skipped Condition is
not a successful readiness gate for a desktop depending on it.

## Remote desktop ownership

Keep `lightos-remote-desktop.service`,
`lightos-remote-desktop-session.service` and the canonical environment paths.
Runtime settings must be consumed by their existing owners; do not copy them
into a second toolkit state file or expose credentials in logs/argv.

The observed external admin proxy uses the platform's current credentials when
connecting to the backend; browser access still goes through platform login.
The kit must not invent a registration endpoint, rewrite the platform password
hash, replace existing authentication, or open an unauthenticated alternate
port. Native package activation can preserve injected credentials, but must
not run unconditionally during image construction or override a disabled
platform setting.

Offline AUR staging is implemented in the remote-desktop program, not by
deleting its scriptlets or hand-writing replacement units in the image recipe.
Its staging feature and release remain subject to that project's remote CI and
review. A staged package is not an activated or validated Niri desktop.

## Desktop does not always mean physical VT

The observed platform supports `vt=false` instances with graphical remote
sessions. Do not make /dev/tty0, /dev/tty7 or physical logind/seatd mandatory for
that mode. Preserve Pond/Niri rather than silently replacing it with the
remote program's existing Cage/Hyprland desktop.

For physical mode, validate the actual mapped device numbers, wait for platform
activation, and keep seatd in the correct controlling-terminal session. Never
allocate/activate a VT, widen host device permissions or guess the VT-manager
socket protocol. Numeric GPU GIDs matter because device mounts and instance
group names need not match; group authorization still requires an explicit
profile policy rather than blindly granting privileged groups.

Remote first-frame/input/audio acceptance and physical VT/PAM/logind acceptance
are separate checks. Neither a running systemd process nor an OCI build proves
either result.

## Current implementation boundary

The accepted adapter handles keyrings and explicit service ordering without
starting/enabling a service owned by the platform. Commit `16083e0` passed full
Actions `40682679574`; independent re-review closed prefix/template-name and
stale-drop-in findings with observed process-level RED-to-GREEN evidence.
Device preparation and real Pond/Niri session support
must receive their own implementation and GitHub evidence before the image
pipeline resumes. This document is not a claim that those features already run.
