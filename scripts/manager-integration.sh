#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
test_dir=$(mktemp -d /tmp/tailnet-manager-test.XXXXXX)
trap 'rm -r "$test_dir"' EXIT INT TERM
mkdir -p "$test_dir/bin" "$test_dir/images" "$test_dir/vms" "$test_dir/state"

cat >"$test_dir/bin/virsh" <<'EOF'
#!/bin/sh
case "${1:-}" in
  net-info) printf 'Name: test\nActive: yes\n' ;;
  domifaddr) printf ' vnet0  52:54:00:00:00:01  ipv4  192.168.122.20/24\n' ;;
  domstate) printf 'running\n' ;;
  dominfo) printf 'Managed save: no\n' ;;
esac
exit 0
EOF
chmod +x "$test_dir/bin/virsh"
printf '%s\n' '#!/bin/sh' 'exit 0' >"$test_dir/bin/scp"
printf '%s\n' '#!/bin/sh' 'exit 0' >"$test_dir/bin/ssh"
chmod +x "$test_dir/bin/scp" "$test_dir/bin/ssh"
qemu-img create -q -f qcow2 "$test_dir/images/android-base.qcow2" 64M
: >"$test_dir/vms/tailnet-router.qcow2"
: >"$test_dir/vms/tailnet-router-seed.img"

cat >"$test_dir/config.toml" <<EOF
[router]
hostname = "android-tailnet-router"
auth_key_file = "/etc/tailnet-android-vm-manager/secrets/tailscale-authkey"
tailscale_interface = "tailscale0"
uplink_interface = "ens3"
guest_interface = "ens4"
lan_address = "10.80.0.1"

[controllers.test-phone]
sources = ["100.64.0.2"]
adb_public_key_file = "/etc/adb/test-phone.pub"

[vms.game-01]
labels = ["game", "primary"]
address = "10.80.0.2"
base_image = "$test_dir/images/android-base.qcow2"
vcpus = 2
memory_mib = 1024
autostart = true

[vms.game-02]
labels = ["game"]
address = "10.80.0.3"
base_image = "$test_dir/images/android-base.qcow2"
vcpus = 2
memory_mib = 1024
autostart = false

[network]
router_uplink_network = "default"
guest_network = "tailnet-android-test"
guest_bridge = "vmbr-test"
guest_subnet = "10.80.0.0/24"

[storage]
state_dir = "$test_dir/state"
image_dir = "$test_dir/images"
vm_dir = "$test_dir/vms"
EOF

hostctl="$project_dir/target/debug/hostctl"
PATH="$test_dir/bin:$PATH" "$hostctl" --config "$test_dir/config.toml" reconcile >"$test_dir/first"
grep -q 'vm.*game-01.*created' "$test_dir/first"
grep -q 'vm.*game-02.*created' "$test_dir/first"
PATH="$test_dir/bin:$PATH" "$hostctl" --config "$test_dir/config.toml" reconcile >"$test_dir/second"
grep -q 'vm.*game-01.*defined' "$test_dir/second"
grep -q 'vm.*game-02.*defined' "$test_dir/second"

PATH="$test_dir/bin:$PATH" "$hostctl" --config "$test_dir/config.toml" vm list --json >"$test_dir/list.json"
grep -q '"name": "game-01"' "$test_dir/list.json"
grep -q '"state": "Running"' "$test_dir/list.json"
PATH="$test_dir/bin:$PATH" "$hostctl" --config "$test_dir/config.toml" vm status --label primary --jobs 2 >"$test_dir/status"
grep -q '^game-01.*Running' "$test_dir/status"
if PATH="$test_dir/bin:$PATH" "$hostctl" --config "$test_dir/config.toml" vm status game-01 --all >/dev/null 2>&1; then
    echo "ambiguous selector unexpectedly succeeded" >&2
    exit 1
fi
printf 'test-only-private-key\n' >"$test_dir/router.key"
chmod 0600 "$test_dir/router.key"
PATH="$test_dir/bin:$PATH" ROUTER_KNOWN_HOSTS_FILE="$test_dir/known_hosts" \
    sh "$project_dir/scripts/router-sync.sh" \
    "$test_dir/config.toml" "$test_dir/router.key" "$project_dir/target/debug/routerctl" \
    >"$test_dir/router-sync"
grep -q 'advertised /32 routes synchronized' "$test_dir/router-sync"
echo "Manager reconciliation and multi-VM selector integration passed."
