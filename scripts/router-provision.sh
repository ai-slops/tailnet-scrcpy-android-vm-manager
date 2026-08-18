#!/bin/sh
set -eu

config=${1:?usage: router-provision.sh CONFIG HOSTCTL ROUTERCTL}
hostctl=${2:?usage: router-provision.sh CONFIG HOSTCTL ROUTERCTL}
routerctl=${3:?usage: router-provision.sh CONFIG HOSTCTL ROUTERCTL}
image_url=${ROUTER_IMAGE_URL:-https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img}
ssh_public_key_file=${ROUTER_SSH_PUBLIC_KEY_FILE:?set ROUTER_SSH_PUBLIC_KEY_FILE to an SSH public key for the ubuntu account}
[ -f "$ssh_public_key_file" ] || { echo "SSH public key is not a regular file: $ssh_public_key_file" >&2; exit 1; }

for command in curl qemu-img cloud-localds virsh base64; do
    command -v "$command" >/dev/null || { echo "missing required command: $command" >&2; exit 1; }
done

paths=$($hostctl --config "$config" router-artifact-paths)
disk=$(printf '%s\n' "$paths" | sed -n 's/^disk=//p')
seed=$(printf '%s\n' "$paths" | sed -n 's/^seed=//p')
base=$(printf '%s\n' "$paths" | sed -n 's/^base=//p')
guest_network=$(printf '%s\n' "$paths" | sed -n 's/^guest_network=//p')
hostname=$(printf '%s\n' "$paths" | sed -n 's/^hostname=//p')
[ -n "$disk" ] && [ -n "$seed" ] && [ -n "$base" ] && [ -n "$guest_network" ] && [ -n "$hostname" ] || { echo "invalid router artifact paths" >&2; exit 1; }
[ ! -e "$disk" ] || { echo "refusing to overwrite existing router disk: $disk" >&2; exit 1; }
[ ! -e "$seed" ] || { echo "refusing to overwrite existing router seed: $seed" >&2; exit 1; }

artifact_dir=$(mktemp -d /tmp/tailnet-router-provision.XXXXXX)
trap 'rm -r "$artifact_dir"' EXIT INT TERM
mkdir -p "$(dirname "$base")" "$(dirname "$disk")"
if [ ! -f "$base" ]; then
    echo "Downloading official Ubuntu 24.04 cloud image..."
    curl --fail --location --retry 3 --output "$artifact_dir/base.img" "$image_url"
    mv "$artifact_dir/base.img" "$base"
fi
qemu-img info --output=json "$base" | grep -q '"format": "qcow2"' || { echo "router base image is not qcow2" >&2; exit 1; }

config_b64=$(base64 -w 0 "$config")
routerctl_b64=$(base64 -w 0 "$routerctl")
netplan_b64=$($routerctl --config "$config" netplan-print | base64 -w 0)
dnsmasq_b64=$($routerctl --config "$config" dnsmasq-print | base64 -w 0)
ssh_key_b64=$(base64 -w 0 "$ssh_public_key_file")
sysctl_b64=$(printf 'net.ipv4.ip_forward=1\n' | base64 -w 0)
cat >"$artifact_dir/user-data" <<EOF
#cloud-config
package_update: true
packages: [ca-certificates, curl, dnsmasq, nftables]
write_files:
  - path: /etc/tailnet-android-vm-manager/config.toml
    permissions: '0600'
    encoding: b64
    content: $config_b64
  - path: /usr/local/sbin/routerctl
    permissions: '0755'
    encoding: b64
    content: $routerctl_b64
  - path: /etc/netplan/60-tailnet-android.yaml
    permissions: '0600'
    encoding: b64
    content: $netplan_b64
  - path: /etc/dnsmasq.d/tailnet-android.conf
    permissions: '0644'
    encoding: b64
    content: $dnsmasq_b64
  - path: /etc/sysctl.d/60-tailnet-android.conf
    permissions: '0644'
    encoding: b64
    content: $sysctl_b64
  - path: /home/ubuntu/.ssh/authorized_keys
    owner: ubuntu:ubuntu
    permissions: '0600'
    defer: true
    encoding: b64
    content: $ssh_key_b64
runcmd:
  - [sh, -c, 'curl -fsSL https://tailscale.com/install.sh | sh']
  - [netplan, apply]
  - [sysctl, --system]
  - [systemctl, enable, --now, dnsmasq]
  - [/usr/local/sbin/routerctl, firewall-apply]
  - [touch, /var/lib/cloud/tailnet-router-ready]
final_message: 'Tailnet Android router provisioning complete'
EOF
printf 'instance-id: tailnet-android-router\nlocal-hostname: %s\n' "$hostname" >"$artifact_dir/meta-data"

qemu-img create -q -f qcow2 -F qcow2 -b "$base" "$artifact_dir/router.qcow2" 8G
cloud-localds "$artifact_dir/seed.img" "$artifact_dir/user-data" "$artifact_dir/meta-data"
mv "$artifact_dir/router.qcow2" "$disk"
mv "$artifact_dir/seed.img" "$seed"

$hostctl --config "$config" guest-network-xml >"$artifact_dir/network.xml"
if ! virsh net-info "$guest_network" >/dev/null 2>&1; then
    virsh net-define "$artifact_dir/network.xml"
fi
virsh net-autostart "$guest_network"
virsh net-start "$guest_network" >/dev/null 2>&1 || true
$hostctl --config "$config" router-domain-xml >"$artifact_dir/router.xml"
virsh define "$artifact_dir/router.xml"
virsh autostart tailnet-android-router
virsh start tailnet-android-router
echo "Router VM started. Use 'just diagnose router-console' and wait for cloud-init before enrollment."
