# Tailnet Scrcpy Android VM Manager

Tailnet Scrcpy Android VM Manager is a Linux/KVM service for creating persistent
Android virtual machines and controlling them remotely from the **Scrcpy Remote**
iOS app.

The project is designed around a deliberately narrow trust model:

- Android guests do not run Tailscale.
- Only a dedicated Tailnet router VM joins the tailnet; the KVM host and
  Android guests do not run Tailscale.
- Tailnet Lock controls which physical devices may join that tailnet.
- The Tailscale control plane is not trusted to authorize access by itself.
- Android Debug Bridge (ADB) public-key authentication provides an independent
  second authorization boundary for scrcpy control.
- A local database on the KVM host is the source of truth for device-to-VM
  permissions.

The initial release targets a single x86-64 Ubuntu LTS host with KVM, QEMU,
libvirt, nftables, persistent qcow2 guests, and AOSP images.

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

## Project status

This repository is currently in the design phase. The first implementation task
is a compatibility spike against the current Scrcpy Remote iOS release. No
production-ready service exists yet.
