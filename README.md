# Tailnet Scrcpy Android VM Manager

Tailnet Scrcpy Android VM Manager is a Linux/KVM service for creating persistent
Android virtual machines and controlling them remotely from the **Scrcpy Remote**
iOS app.

The project is designed around a deliberately narrow trust model:

- Android guests do not run Tailscale.
- Only a dedicated Tailnet router appliance joins the tailnet; the current
  implementation uses an isolated VM. The KVM host and Android guests do not
  run Tailscale.
- Tailscale provides transport and route coordination, not Android control
  authorization.
- Android Debug Bridge (ADB) public-key authentication is the authoritative
  authorization boundary for scrcpy control.
- VM administration is a separate plane: run the local CLI directly or through
  an SSH client. A future web UI must use separate management authorization.
- A local database on the KVM host is the source of truth for device-to-VM
  permissions.

The initial release targets a single x86-64 Ubuntu LTS host with KVM, QEMU,
libvirt, nftables, persistent qcow2 guests, and AOSP images.

## Development commands

With `cargo`, Zig, and `just` on `PATH`, use the repository recipes:

~~~shell
just check
just headscale-test
just nested-ubuntu-test
just preflight .local/spike/config.toml
just reconcile .local/phone/config.toml
just vm-list .local/phone/config.toml
just vm-start-label game 2 .local/phone/config.toml
just guest-network-xml .local/phone/config.toml
ROUTER_SSH_PUBLIC_KEY_FILE=/path/to/id_ed25519.pub just router-provision .local/phone/config.toml
just android-create android-game-01 .local/phone/config.toml
just --list
~~~

Mise is optional; `mise install` provides the pinned tool versions when it is
available. The recipes and Cargo linker wrapper work without mise and keep Zig
caches below the ignored `.local/` directory. Individual shell files remain
directly runnable with `sh scripts/...` when a recipe is not appropriate.

## Documentation

- [Tailscale provisioning](docs/tailscale-provisioning.md)
- [Persistent VM lifecycle and snapshots](docs/vm-lifecycle.md)
- [Product requirements](docs/product-requirements.md)
- [Architecture](docs/architecture.md)
- [Security model](docs/security.md)
- [Architecture decisions](docs/decisions.md)
- [MVP plan](docs/mvp-plan.md)
- [Scrcpy Remote compatibility spike](docs/compatibility-spike.md)
- [Development host setup](docs/host-setup.md)
- [Local Headscale integration test](docs/integration-testing.md)
- [Real iPhone connection runbook](docs/real-device-setup.md)

## Project status

This is a pre-MVP implementation, not a production-ready service. Implemented
and tested foundations include validated configuration, router enrollment and
fail-closed source allowlisting, a generated isolated router VM definition,
persistent VM lifecycle with SSD-backed hibernation, stopped-state snapshots,
ADB public-key validation, and a local Headscale routing integration test.

The physical Scrcpy Remote/iOS compatibility spike, Android image lifecycle,
ADB-key synchronization, local authorization database, reconciliation service,
and production packaging remain incomplete. See the [MVP plan](docs/mvp-plan.md)
for the remaining work.
