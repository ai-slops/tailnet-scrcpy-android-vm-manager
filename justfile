set shell := ["sh", "-eu", "-c"]

export ZIG_LOCAL_CACHE_DIR := env_var_or_default("ZIG_LOCAL_CACHE_DIR", justfile_directory() + "/.local/cache/zig")
export ZIG_GLOBAL_CACHE_DIR := env_var_or_default("ZIG_GLOBAL_CACHE_DIR", justfile_directory() + "/.local/cache/zig-global")
default_config := env_var_or_default("MANAGER_CONFIG", justfile_directory() + "/.local/config.toml")

mod setup
mod diagnose
mod dev
mod android-dev

# List day-to-day operator commands. Use `just --list MODULE` for specialist commands.
default:
    @just --list

# Reconcile declared networks, router, and Android VM inventory on this host.
reconcile config=default_config:
    cargo run -q -p hostctl -- --config "{{ config }}" reconcile

# Reconcile host state and synchronize a running router VM.
reconcile-all ssh_private_key config=default_config:
    cargo build -q -p hostctl -p routerctl
    target/debug/hostctl --config "{{ config }}" reconcile
    sh scripts/router-sync.sh "{{ config }}" "{{ ssh_private_key }}" target/debug/routerctl

# List all Android VMs and their current state.
vm-list config=default_config:
    cargo run -q -p hostctl -- --config "{{ config }}" vm list

# Start one Android VM.
vm-start name config=default_config:
    cargo run -q -p hostctl -- --config "{{ config }}" vm start "{{ name }}"

# Start every Android VM with bounded concurrency.
vm-start-all jobs="2" config=default_config:
    cargo run -q -p hostctl -- --config "{{ config }}" vm start --all --jobs "{{ jobs }}"

# Start every Android VM carrying a label.
vm-start-label label jobs="2" config=default_config:
    cargo run -q -p hostctl -- --config "{{ config }}" vm start --label "{{ label }}" --jobs "{{ jobs }}"

# Stop every Android VM with bounded concurrency.
vm-stop-all jobs="2" config=default_config:
    cargo run -q -p hostctl -- --config "{{ config }}" vm stop --all --jobs "{{ jobs }}"

# Stop every Android VM carrying a label.
vm-stop-label label jobs="2" config=default_config:
    cargo run -q -p hostctl -- --config "{{ config }}" vm stop --label "{{ label }}" --jobs "{{ jobs }}"

# Hibernate every Android VM to SSD with bounded concurrency.
vm-hibernate-all jobs="2" config=default_config:
    cargo run -q -p hostctl -- --config "{{ config }}" vm hibernate --all --jobs "{{ jobs }}"

# Hibernate every Android VM carrying a label.
vm-hibernate-label label jobs="2" config=default_config:
    cargo run -q -p hostctl -- --config "{{ config }}" vm hibernate --label "{{ label }}" --jobs "{{ jobs }}"
