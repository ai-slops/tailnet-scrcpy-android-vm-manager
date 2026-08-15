set shell := ["sh", "-eu", "-c"]

export ZIG_LOCAL_CACHE_DIR := env_var_or_default(
  "ZIG_LOCAL_CACHE_DIR",
  justfile_directory() + "/.local/cache/zig",
)
export ZIG_GLOBAL_CACHE_DIR := env_var_or_default(
  "ZIG_GLOBAL_CACHE_DIR",
  justfile_directory() + "/.local/cache/zig-global",
)

# List available project commands.
default:
  @just --list

# Format Rust sources.
fmt:
  cargo fmt --all

# Check formatting without changing files.
fmt-check:
  cargo fmt --all -- --check

# Run Clippy with warnings denied.
clippy:
  cargo clippy --workspace --all-targets -- -D warnings

# Run all Rust unit and documentation tests.
test:
  cargo test --workspace

# Validate repository-local shell and patch syntax.
static-check:
  sh -n scripts/android-spike-sdk.sh scripts/headscale-integration.sh scripts/host-smoke.sh scripts/router-provision.sh scripts/router-provision-test.sh scripts/zig-cc
  git diff --check

# Validate the disposable Headscale Compose model without starting it.
compose-check:
  HEADSCALE_AUTHKEY=validation-only docker compose -f tests/headscale/docker-compose.yml config >/dev/null

# Run all core checks; no mise, KVM, or container engine is required.
check: fmt-check clippy test static-check

# Run the local Headscale routed-access integration test.
headscale-test:
  sh scripts/headscale-integration.sh

# Run the host KVM and rootless Podman smoke test.
host-smoke:
  sh scripts/host-smoke.sh

# Validate the host and project configuration.
preflight config="config.example.toml":
  cargo run -q -p hostctl -- --config "{{config}}" preflight

# Validate an ADB public key and print its fingerprint.
adb-fingerprint public_key:
  cargo run -q -p hostctl -- adb-fingerprint "{{public_key}}"

# Print the router libvirt domain XML.
router-xml config="config.example.toml":
  cargo run -q -p hostctl -- --config "{{config}}" router-domain-xml

# Provision and start a dedicated Ubuntu router VM. Set ROUTER_SSH_PUBLIC_KEY_FILE.
router-provision config="config.example.toml":
  cargo build -q -p hostctl -p routerctl
  sh scripts/router-provision.sh "{{config}}" target/debug/hostctl target/debug/routerctl

# Open the router's serial console (exit with Ctrl+]).
router-console:
  virsh console tailnet-android-router

# Exercise router image and seed creation without touching system libvirt.
router-provision-test:
  cargo build -q -p hostctl -p routerctl
  sh scripts/router-provision-test.sh

# Print the host-address-free Android guest network XML.
guest-network-xml config="config.example.toml":
  cargo run -q -p hostctl -- --config "{{config}}" guest-network-xml

# Print one Android VM's libvirt domain XML.
android-xml name config="config.example.toml":
  cargo run -q -p hostctl -- --config "{{config}}" vm domain-xml "{{name}}"

# Create one persistent Android overlay and define its domain.
android-create name config="config.example.toml":
  cargo run -q -p hostctl -- --config "{{config}}" vm create "{{name}}"

# Validate and print one VM's configured Android ADB public keys.
android-adb-keys name config="config.example.toml":
  cargo run -q -p hostctl -- --config "{{config}}" vm adb-authorized-keys "{{name}}"

# Print the router's two-NIC netplan configuration.
router-netplan config="config.example.toml":
  cargo run -q -p routerctl -- --config "{{config}}" netplan-print

# Print the router's static Android DHCP/DNS configuration.
router-dnsmasq config="config.example.toml":
  cargo run -q -p routerctl -- --config "{{config}}" dnsmasq-print

# Validate generated guest-network, router, and Android domain XML.
libvirt-xml-check name="android-game-01" config="config.example.toml":
  artifact_dir=$(mktemp -d /tmp/tailnet-android-xml.XXXXXX); \
  trap 'rm -r "$artifact_dir"' EXIT INT TERM; \
  cargo run -q -p hostctl -- --config "{{config}}" guest-network-xml >"$artifact_dir/guest.xml"; \
  cargo run -q -p hostctl -- --config "{{config}}" router-domain-xml >"$artifact_dir/router.xml"; \
  cargo run -q -p hostctl -- --config "{{config}}" vm domain-xml "{{name}}" >"$artifact_dir/android.xml"; \
  virt-xml-validate "$artifact_dir/guest.xml"; \
  virt-xml-validate "$artifact_dir/router.xml"; \
  virt-xml-validate "$artifact_dir/android.xml"

# Run one explicit Android emulator spike stage.
android-spike stage:
  sh scripts/android-spike-sdk.sh "{{stage}}"
