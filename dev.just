set shell := ["sh", "-eu", "-c"]

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

static-check:
    sh -n scripts/android-spike-sdk.sh scripts/headscale-integration.sh scripts/host-smoke.sh scripts/manager-integration.sh scripts/nested-ubuntu-test.sh scripts/router-enroll.sh scripts/router-provision.sh scripts/router-provision-test.sh scripts/router-sync.sh scripts/zig-cc
    git diff --check

compose-check:
    HEADSCALE_AUTHKEY=validation-only docker compose -f tests/headscale/docker-compose.yml config >/dev/null

# Run all checks that need no KVM or container engine.
check: fmt-check clippy test static-check

# Run the disposable local Headscale routing test.
headscale-test:
    sh scripts/headscale-integration.sh

# Exercise manager reconciliation and selectors with fake host tools.
manager-test:
    cargo build -q -p hostctl
    sh scripts/manager-integration.sh

# Exercise router image and seed creation without touching system libvirt.
router-provision-test:
    cargo build -q -p hostctl -p routerctl
    sh scripts/router-provision-test.sh

# Clone this commit into a disposable nested Ubuntu/KVM VM and run validation.
nested-ubuntu-test:
    sh scripts/nested-ubuntu-test.sh
