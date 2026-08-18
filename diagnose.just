set shell := ["sh", "-eu", "-c"]

# Prove host KVM and rootless Podman access while diagnosing a deployment host.
host-smoke:
    sh scripts/host-smoke.sh

# Open the router serial console while diagnosing boot or networking.
router-console:
    virsh console tailnet-android-router

# Print the router libvirt domain XML.
router-xml config="config.example.toml":
    cargo run -q -p hostctl -- --config "{{ config }}" router-domain-xml

# Print the host-address-free Android guest network XML.
guest-network-xml config="config.example.toml":
    cargo run -q -p hostctl -- --config "{{ config }}" guest-network-xml

# Print one Android VM's libvirt domain XML.
android-xml name config="config.example.toml":
    cargo run -q -p hostctl -- --config "{{ config }}" vm domain-xml "{{ name }}"

# Print the router's two-NIC netplan configuration.
router-netplan config="config.example.toml":
    cargo run -q -p routerctl -- --config "{{ config }}" netplan-print

# Print the router's static Android DHCP/DNS configuration.
router-dnsmasq config="config.example.toml":
    cargo run -q -p routerctl -- --config "{{ config }}" dnsmasq-print

# Validate generated network, router, and Android libvirt XML.
libvirt-xml-check name="android-game-01" config="config.example.toml":
    artifact_dir=$(mktemp -d /tmp/tailnet-android-xml.XXXXXX); \
    trap 'rm -r "$artifact_dir"' EXIT INT TERM; \
    cargo run -q -p hostctl -- --config "{{ config }}" guest-network-xml >"$artifact_dir/guest.xml"; \
    cargo run -q -p hostctl -- --config "{{ config }}" router-domain-xml >"$artifact_dir/router.xml"; \
    cargo run -q -p hostctl -- --config "{{ config }}" vm domain-xml "{{ name }}" >"$artifact_dir/android.xml"; \
    virt-xml-validate "$artifact_dir/guest.xml"; \
    virt-xml-validate "$artifact_dir/router.xml"; \
    virt-xml-validate "$artifact_dir/android.xml"
