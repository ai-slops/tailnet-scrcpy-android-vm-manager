#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
compose="$root/tests/headscale/docker-compose.yml"
project_dir="$root/.local/headscale-integration"

docker info >/dev/null
mkdir -p "$project_dir" "$root/.local/cache/zig" "$root/.local/cache/zig-global"
# Compose interpolates extension fields even while only Headscale is selected.
# This placeholder is never passed to a client container.
HEADSCALE_AUTHKEY=bootstrap-unused
export HEADSCALE_AUTHKEY

cleanup() {
  [ "${KEEP_HEADSCALE_TEST:-0}" = 1 ] ||
    docker compose -f "$compose" down --volumes --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

docker compose -f "$compose" down --volumes --remove-orphans >/dev/null 2>&1 || true
docker compose -f "$compose" up -d headscale

attempt=0
until docker compose -f "$compose" exec -T headscale headscale health >/dev/null 2>&1; do
  attempt=$((attempt + 1))
  [ "$attempt" -lt 60 ] || {
    docker compose -f "$compose" logs headscale >&2
    exit 1
  }
  sleep 1
done

docker compose -f "$compose" exec -T headscale headscale users create integration >/dev/null
key_json=$(docker compose -f "$compose" exec -T headscale \
  headscale preauthkeys create --user 1 --reusable --expiration 1h --output json)
HEADSCALE_AUTHKEY=$(printf '%s\n' "$key_json" |
  sed -n 's/.*"key"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
[ -n "$HEADSCALE_AUTHKEY" ] || {
  printf 'Could not parse Headscale pre-auth key JSON.\n%s\n' "$key_json" >&2
  exit 1
}
export HEADSCALE_AUTHKEY

wait_node() {
  node=$1
  attempt=0
  until docker compose -f "$compose" exec -T "$node" tailscale ip -4 2>/dev/null |
    sed -n '1p' | grep -q '^100\.'; do
    attempt=$((attempt + 1))
    [ "$attempt" -lt 60 ] || {
      docker compose -f "$compose" logs "$node" >&2
      return 1
    }
    sleep 1
  done
}

docker compose -f "$compose" up -d --build router
wait_node router
docker compose -f "$compose" up -d controller
wait_node controller
docker compose -f "$compose" up -d intruder guest
wait_node intruder

docker compose -f "$compose" exec -T headscale \
  headscale nodes approve-routes --identifier 1 --routes 10.80.0.2/32 >/dev/null

controller_ip=$(docker compose -f "$compose" exec -T controller tailscale ip -4 |
  sed -n '1{s/[[:space:]]//g;p;}')
intruder_ip=$(docker compose -f "$compose" exec -T intruder tailscale ip -4 |
  sed -n '1{s/[[:space:]]//g;p;}')
[ "$controller_ip" != "$intruder_ip" ]

config="$project_dir/config.toml"
{
  printf '[router]\n'
  printf 'hostname = "integration-router"\n'
  printf 'auth_key_file = "/run/secrets/unused"\n'
  printf 'tailscale_interface = "tailscale0"\n'
  printf 'guest_interface = "eth1"\n'
  printf 'lan_address = "10.80.0.3"\n'
  printf '\n'
  printf '[[router.access]]\nsource = "%s"\nguest = "10.80.0.2"\n\n' "$controller_ip"
  printf '[[android_vms]]\nname = "integration-android"\naddress = "10.80.0.2"\n\n'
  printf '[network]\nlibvirt_bridge = "vmbr-android"\nguest_subnet = "10.80.0.0/24"\n'
  printf '[storage]\nstate_dir = "/tmp/state"\nimage_dir = "/tmp/images"\nvm_dir = "/tmp/vms"\n'
} >"$config"

(
  cd "$root"
  ZIG_LOCAL_CACHE_DIR="$root/.local/cache/zig" \
    ZIG_GLOBAL_CACHE_DIR="$root/.local/cache/zig-global" \
    cargo run -q -p routerctl -- --config "$config" firewall-print
) | docker compose -f "$compose" exec -T router nft -f -

attempt=0
response=
until response=$(docker compose -f "$compose" exec -T controller \
  sh -c "printf integration-ok | nc -w 2 10.80.0.2 5555" 2>/dev/null) &&
  [ "$response" = integration-ok ]; do
  attempt=$((attempt + 1))
  [ "$attempt" -lt 30 ] || {
    printf 'Controller could not reach the routed Android endpoint.\n' >&2
    docker compose -f "$compose" logs router controller guest >&2
    exit 1
  }
  sleep 1
done

if docker compose -f "$compose" exec -T intruder \
  sh -c "printf forbidden | nc -w 2 10.80.0.2 5555" >/dev/null 2>&1; then
  printf 'Intruder unexpectedly reached the routed Android endpoint.\n' >&2
  exit 1
fi

printf 'Headscale integration passed: controller %s allowed, intruder %s denied.\n' \
  "$controller_ip" "$intruder_ip"
