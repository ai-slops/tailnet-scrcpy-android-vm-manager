#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
test_dir=$(mktemp -d /tmp/router-provision-test.XXXXXX)
trap 'rm -r "$test_dir"' EXIT INT TERM
mkdir -p "$test_dir/bin" "$test_dir/images" "$test_dir/vms"
cp "$project_dir/config.example.toml" "$test_dir/config.toml"
sed -i \
    -e "s#/var/lib/tailnet-android-vm-manager/images#$test_dir/images#g" \
    -e "s#/var/lib/tailnet-android-vm-manager/vms#$test_dir/vms#g" \
    -e "s#/var/lib/tailnet-android-vm-manager#$test_dir#g" \
    "$test_dir/config.toml"
printf '%s\n' '#!/bin/sh' 'exit 0' >"$test_dir/bin/virsh"
chmod +x "$test_dir/bin/virsh"
printf '%s\n' 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFakeProvisionTestKeyOnly test' >"$test_dir/key.pub"
qemu-img create -q -f qcow2 "$test_dir/images/ubuntu-24.04-server-cloudimg-amd64.img" 64M

PATH="$test_dir/bin:$PATH" \
    ROUTER_SSH_PUBLIC_KEY_FILE="$test_dir/key.pub" \
    sh "$project_dir/scripts/router-provision.sh" \
    "$test_dir/config.toml" \
    "$project_dir/target/debug/hostctl" \
    "$project_dir/target/debug/routerctl"
test -f "$test_dir/vms/tailnet-router.qcow2"
test -f "$test_dir/vms/tailnet-router-seed.img"

if PATH="$test_dir/bin:$PATH" \
    ROUTER_SSH_PUBLIC_KEY_FILE="$test_dir/key.pub" \
    sh "$project_dir/scripts/router-provision.sh" \
    "$test_dir/config.toml" \
    "$project_dir/target/debug/hostctl" \
    "$project_dir/target/debug/routerctl" >"$test_dir/retry.log" 2>&1; then
    echo "second provision unexpectedly succeeded" >&2
    exit 1
fi
grep -q 'refusing to overwrite' "$test_dir/retry.log"
echo "Disposable router provisioning test passed."
