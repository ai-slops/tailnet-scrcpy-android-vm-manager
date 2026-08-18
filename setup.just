set shell := ["sh", "-eu", "-c"]
default_config := env_var_or_default("MANAGER_CONFIG", justfile_directory() + "/.local/config.toml")

# Create an ignored local deployment config without overwriting an existing one.
init-config destination=default_config:
    test ! -e "{{ destination }}"
    mkdir -p "$(dirname "{{ destination }}")"
    install -m 0600 config.example.toml "{{ destination }}"
    echo "Created {{ destination }}"

# Validate a host and deployment configuration before initial setup.
preflight config=default_config:
    cargo run -q -p hostctl -- --config "{{ config }}" preflight

# Provision and start the dedicated router VM. Set ROUTER_SSH_PUBLIC_KEY_FILE.
router-provision config=default_config:
    cargo build -q -p hostctl -p routerctl
    sh scripts/router-provision.sh "{{ config }}" target/debug/hostctl target/debug/routerctl

# Install an auth key over SSH stdin, enroll the router, and run preflight.
router-enroll ssh_private_key auth_key=".local/secrets/authkey.txt":
    sh scripts/router-enroll.sh "{{ ssh_private_key }}" "{{ auth_key }}"

# Create one persistent Android overlay and define its VM.
android-create name config=default_config:
    cargo run -q -p hostctl -- --config "{{ config }}" vm create "{{ name }}"

# Validate an ADB public key and print its fingerprint during controller enrollment.
adb-fingerprint public_key:
    cargo run -q -p hostctl -- adb-fingerprint "{{ public_key }}"

# Print one VM's desired ADB authorized-key bundle during controller changes.
android-adb-keys name config=default_config:
    cargo run -q -p hostctl -- --config "{{ config }}" vm adb-authorized-keys "{{ name }}"
