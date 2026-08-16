#!/bin/sh
set -eu

config=${1:?usage: router-sync.sh CONFIG SSH_PRIVATE_KEY ROUTERCTL}
ssh_key=${2:?usage: router-sync.sh CONFIG SSH_PRIVATE_KEY ROUTERCTL}
routerctl=${3:?usage: router-sync.sh CONFIG SSH_PRIVATE_KEY ROUTERCTL}
for command in scp ssh virsh; do
    command -v "$command" >/dev/null || { echo "missing required command: $command" >&2; exit 1; }
done
[ -f "$ssh_key" ] || { echo "SSH private key is not a regular file: $ssh_key" >&2; exit 1; }
[ -f "$routerctl" ] || { echo "routerctl binary is missing: $routerctl" >&2; exit 1; }

ip=$(virsh domifaddr tailnet-android-router --source lease | awk '/ipv4/ {sub("/.*", "", $4); print $4; exit}')
[ -n "$ip" ] || { echo "could not find the router uplink DHCP lease" >&2; exit 1; }
known_hosts=${ROUTER_KNOWN_HOSTS_FILE:-.local/router-known-hosts}
mkdir -p "$(dirname "$known_hosts")"
scp -i "$ssh_key" -o BatchMode=yes -o StrictHostKeyChecking=accept-new \
    -o "UserKnownHostsFile=$known_hosts" \
    "$config" "ubuntu@$ip:/tmp/tailnet-manager-config.toml"
scp -i "$ssh_key" -o BatchMode=yes -o StrictHostKeyChecking=accept-new \
    -o "UserKnownHostsFile=$known_hosts" \
    "$routerctl" "ubuntu@$ip:/tmp/routerctl"
ssh -i "$ssh_key" -o BatchMode=yes -o StrictHostKeyChecking=accept-new \
    -o "UserKnownHostsFile=$known_hosts" "ubuntu@$ip" \
    "sudo install -d -m 0755 /etc/tailnet-android-vm-manager; sudo install -m 0600 /tmp/tailnet-manager-config.toml /etc/tailnet-android-vm-manager/config.toml; sudo install -m 0755 /tmp/routerctl /usr/local/sbin/routerctl; sudo sh -c '/usr/local/sbin/routerctl dnsmasq-print > /etc/dnsmasq.d/tailnet-android.conf'; sudo systemctl restart dnsmasq; sudo /usr/local/sbin/routerctl firewall-apply; sudo /usr/local/sbin/routerctl reconfigure; rm -f /tmp/tailnet-manager-config.toml /tmp/routerctl"
echo "Router config, DHCP leases, firewall set, and advertised /32 routes synchronized."
