#!/bin/sh
set -eu

ssh_key=${1:?usage: router-enroll.sh SSH_PRIVATE_KEY AUTH_KEY}
auth_key=${2:?usage: router-enroll.sh SSH_PRIVATE_KEY AUTH_KEY}

for command in ssh virsh stat; do
    command -v "$command" >/dev/null || { echo "missing required command: $command" >&2; exit 1; }
done

[ -f "$ssh_key" ] && [ ! -L "$ssh_key" ] || {
    echo "SSH private key is not a regular non-symlink file: $ssh_key" >&2
    exit 1
}
[ -f "$auth_key" ] && [ ! -L "$auth_key" ] || {
    echo "Tailscale auth key is not a regular non-symlink file: $auth_key" >&2
    exit 1
}

auth_mode=$(stat -c '%a' "$auth_key")
case "$auth_mode" in
    400|600) ;;
    *)
        echo "Tailscale auth key permissions must be 0400 or 0600, found $auth_mode: $auth_key" >&2
        exit 1
        ;;
esac

router_ip=$(virsh domifaddr tailnet-android-router --source lease |
    awk '/ipv4/ {sub("/.*", "", $4); print $4; exit}')
[ -n "$router_ip" ] || {
    echo "Router VM has no libvirt uplink lease; wait for boot and try again" >&2
    exit 1
}

known_hosts=${ROUTER_KNOWN_HOSTS_FILE:-.local/router-known-hosts}
mkdir -p "$(dirname "$known_hosts")"

run_ssh() {
    ssh -i "$ssh_key" -o BatchMode=yes -o StrictHostKeyChecking=accept-new \
        -o "UserKnownHostsFile=$known_hosts" "ubuntu@$router_ip" "$@"
}

run_ssh cloud-init status --wait

# The secret travels only over SSH stdin. It is not copied into argv, the
# cloud-init seed, a temporary file, or command output.
run_ssh \
    'sudo install -d -m 0700 /etc/tailnet-android-vm-manager/secrets && sudo install -m 0600 /dev/stdin /etc/tailnet-android-vm-manager/secrets/tailscale-authkey' \
    <"$auth_key"

run_ssh \
    'sudo routerctl enroll && sudo routerctl firewall-apply && sudo routerctl preflight'

echo "Router enrolled and preflight passed. Approve its advertised Android /32 routes in Tailscale."
