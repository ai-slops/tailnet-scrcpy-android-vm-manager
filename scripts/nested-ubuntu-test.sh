#!/bin/sh
set -eu

domain=${NESTED_TEST_DOMAIN:-tailnet-android-devtest}
image_url=${NESTED_TEST_IMAGE_URL:-https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img}
repo_url=${NESTED_TEST_REPO_URL:-$(git config --get remote.origin.url)}
repo_ref=${NESTED_TEST_REPO_REF:-$(git rev-parse HEAD)}
keep=${KEEP_NESTED_VM:-0}
runtime_dir=${NESTED_TEST_RUNTIME_DIR:-/var/tmp/tailnet-android-nested-$USER}
base=${NESTED_TEST_BASE_IMAGE:-/var/tmp/tailnet-android-nested-ubuntu-24.04-$USER.img}

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

cleanup() {
    if [ "$keep" = 1 ]; then
        printf 'Preserved %s and %s for inspection.\n' "$domain" "$runtime_dir" >&2
        return
    fi
    virsh destroy "$domain" >/dev/null 2>&1 || true
    virsh undefine "$domain" >/dev/null 2>&1 || true
    rm -rf "$runtime_dir"
}
trap cleanup EXIT INT TERM

for command in base64 cloud-localds curl git qemu-img ssh ssh-keygen virsh; do
    command -v "$command" >/dev/null 2>&1 || fail "missing required command: $command"
done
[ -n "$repo_url" ] || fail "no origin URL; set NESTED_TEST_REPO_URL"
case "$runtime_dir" in
    /var/tmp/tailnet-android-nested-?*) ;;
    *) fail "NESTED_TEST_RUNTIME_DIR must stay below /var/tmp/tailnet-android-nested-*" ;;
esac
printf '%s' "$domain" | grep -Eq '^[A-Za-z0-9_.-]+$' || fail "invalid domain name"
virsh dominfo "$domain" >/dev/null 2>&1 && fail "domain already exists: $domain"
[ ! -e "$runtime_dir" ] || fail "runtime directory already exists: $runtime_dir"
virsh net-info default >/dev/null 2>&1 || fail "active libvirt default network is required"

mkdir -m 0755 -p "$runtime_dir"
ssh-keygen -q -t ed25519 -N '' -f "$runtime_dir/id_ed25519"
ssh_key_b64=$(base64 -w 0 "$runtime_dir/id_ed25519.pub")
cat >"$runtime_dir/user-data" <<EOF
#cloud-config
ssh_pwauth: false
write_files:
  - path: /home/ubuntu/.ssh/authorized_keys
    owner: ubuntu:ubuntu
    permissions: '0600'
    defer: true
    encoding: b64
    content: $ssh_key_b64
EOF
printf 'instance-id: %s\nlocal-hostname: %s\n' "$domain" "$domain" >"$runtime_dir/meta-data"

if [ ! -f "$base" ]; then
    curl --fail --location --retry 3 --output "$runtime_dir/base.download" "$image_url"
    mv "$runtime_dir/base.download" "$base"
fi
qemu-img info --output=json "$base" | grep -Eq '"format"[[:space:]]*:[[:space:]]*"qcow2"' ||
    fail "downloaded image is not qcow2"
qemu-img create -q -f qcow2 -F qcow2 -b "$base" "$runtime_dir/disk.qcow2" 30G
cloud-localds "$runtime_dir/seed.img" "$runtime_dir/user-data" "$runtime_dir/meta-data"
chmod 0644 "$base" "$runtime_dir/disk.qcow2" "$runtime_dir/seed.img"

cat >"$runtime_dir/domain.xml" <<EOF
<domain type='kvm'>
  <name>$domain</name>
  <memory unit='MiB'>4096</memory>
  <vcpu placement='static'>4</vcpu>
  <os><type arch='x86_64' machine='q35'>hvm</type><boot dev='hd'/></os>
  <features><acpi/><apic/></features>
  <cpu mode='host-passthrough' check='none'/>
  <devices>
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2'/><source file='$runtime_dir/disk.qcow2'/><target dev='vda' bus='virtio'/>
    </disk>
    <disk type='file' device='cdrom'>
      <driver name='qemu' type='raw'/><source file='$runtime_dir/seed.img'/><target dev='sda' bus='sata'/><readonly/>
    </disk>
    <interface type='network'><source network='default'/><model type='virtio'/></interface>
    <serial type='pty'><target type='isa-serial' port='0'/></serial>
    <console type='pty'><target type='serial' port='0'/></console>
  </devices>
</domain>
EOF
virsh define "$runtime_dir/domain.xml" >/dev/null
virsh start "$domain" >/dev/null

ip=
attempt=0
while [ "$attempt" -lt 60 ]; do
    ip=$(virsh domifaddr "$domain" --source lease | awk '/ipv4/ {sub("/.*", "", $4); print $4; exit}')
    [ -n "$ip" ] && break
    attempt=$((attempt + 1))
    sleep 2
done
[ -n "$ip" ] || fail "VM did not obtain a DHCP lease"

attempt=0
while [ "$attempt" -lt 60 ]; do
    if ssh -i "$runtime_dir/id_ed25519" -o BatchMode=yes -o ConnectTimeout=2 \
        -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null "ubuntu@$ip" true 2>/dev/null; then
        break
    fi
    attempt=$((attempt + 1))
    sleep 2
done
[ "$attempt" -lt 60 ] || fail "SSH did not become ready"

repo_url_b64=$(printf '%s' "$repo_url" | base64 -w 0)
repo_ref_b64=$(printf '%s' "$repo_ref" | base64 -w 0)
cat >"$runtime_dir/remote-test.sh" <<'EOF'
#!/bin/bash
set -euo pipefail
sudo apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install --yes --no-install-recommends \
    build-essential ca-certificates cloud-image-utils curl docker.io docker-compose-v2 git \
    libvirt-clients nftables podman qemu-system-x86 qemu-utils uidmap \
    passt slirp4netns fuse-overlayfs
sudo usermod --append --groups docker,kvm ubuntu
sudo systemctl enable --now docker
curl https://mise.run | sh
export PATH="$HOME/.local/bin:$HOME/.local/share/mise/shims:$PATH"
repo_url=$(printf '%s' "$NESTED_REPO_URL_B64" | base64 -d)
repo_ref=$(printf '%s' "$NESTED_REPO_REF_B64" | base64 -d)
git clone "$repo_url" manager
cd manager
git checkout --detach "$repo_ref"
mise trust
mise install
just dev check
just dev router-provision-test
sudo -H -u ubuntu sh -c 'cd /home/ubuntu/manager && sh scripts/host-smoke.sh'
sudo -H -u ubuntu sh -c 'cd /home/ubuntu/manager && PATH=/home/ubuntu/.local/bin:/home/ubuntu/.local/share/mise/shims:$PATH just dev headscale-test'
EOF
chmod 0600 "$runtime_dir/remote-test.sh"
ssh -i "$runtime_dir/id_ed25519" -o BatchMode=yes -o StrictHostKeyChecking=no \
    -o UserKnownHostsFile=/dev/null "ubuntu@$ip" \
    "NESTED_REPO_URL_B64=$repo_url_b64 NESTED_REPO_REF_B64=$repo_ref_b64 bash -s" <"$runtime_dir/remote-test.sh"
printf 'PASS: Ubuntu clone validated commit %s with nested KVM.\n' "$repo_ref"
