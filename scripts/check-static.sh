#!/usr/bin/env bash
set -euo pipefail

if [[ $# != 1 ]]; then
    printf 'usage: bash scripts/check-static.sh BINARY_DIRECTORY\n' >&2
    exit 2
fi

for name in rootfs-fetch rootfs-inspect rootfs-cleaner lighos-init; do
    binary="$1/$name"
    program_headers="$(LC_ALL=C readelf --program-headers --wide "$binary")"
    dynamic="$(LC_ALL=C readelf --dynamic --wide "$binary")"
    case "$program_headers" in
        *INTERP*) printf '%s has a runtime interpreter\n' "$binary" >&2; exit 1 ;;
    esac
    case "$dynamic" in
        *'(NEEDED)'*) printf '%s needs shared libraries\n' "$binary" >&2; exit 1 ;;
    esac
    "$binary" --help >/dev/null
    "$binary" --version
    sha256sum "$binary"
done
