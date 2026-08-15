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
  sh -n scripts/android-spike-sdk.sh scripts/headscale-integration.sh scripts/host-smoke.sh scripts/zig-cc
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

# Run one explicit Android emulator spike stage.
android-spike stage:
  sh scripts/android-spike-sdk.sh "{{stage}}"
