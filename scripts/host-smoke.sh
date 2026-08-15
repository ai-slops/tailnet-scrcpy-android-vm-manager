#!/bin/sh
set -eu

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

command -v qemu-system-x86_64 >/dev/null 2>&1 ||
  fail "qemu-system-x86_64 is not installed"

[ -r /dev/kvm ] && [ -w /dev/kvm ] ||
  fail "/dev/kvm is not accessible; start a new login session or run: newgrp kvm"

qmp_output="$(
  printf '%s\n' \
    '{"execute":"qmp_capabilities"}' \
    '{"execute":"query-kvm"}' \
    '{"execute":"quit"}' |
    qemu-system-x86_64 \
      -accel kvm \
      -cpu host \
      -m 64M \
      -display none \
      -nodefaults \
      -S \
      -qmp stdio
)"

printf '%s\n' "$qmp_output" | grep -Eq '"enabled"[[:space:]]*:[[:space:]]*true' ||
  fail "QEMU did not report KVM acceleration enabled"
printf 'PASS: QEMU initialized KVM acceleration\n'

if command -v podman >/dev/null 2>&1; then
  rootless="$(podman info --format '{{.Host.Security.Rootless}}')"
  [ "$rootless" = "true" ] || fail "Podman is installed but not running rootless"
  podman run --rm --network none docker.io/library/alpine:3.22 true
  printf 'PASS: rootless Podman ran an Alpine container\n'
else
  fail "podman is not installed; install the Ubuntu podman package"
fi
