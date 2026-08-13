# Tailnet Scrcpy Android VM Manager

Tailnet Scrcpy Android VM Manager is a Linux/KVM service for creating persistent
Android virtual machines and controlling them remotely from the **Scrcpy Remote**
iOS app.

The project is designed around a deliberately narrow trust model:

- Android guests do not run Tailscale.
- Only the KVM host joins a dedicated tailnet.
- Tailnet Lock controls which physical devices may join that tailnet.
- The Tailscale control plane is not trusted to authorize access by itself.
- Android Debug Bridge (ADB) public-key authentication provides an independent
  second authorization boundary for scrcpy control.
- A local database on the KVM host is the source of truth for device-to-VM
  permissions.

The initial release targets a single x86-64 Ubuntu LTS host with KVM, QEMU,
libvirt, nftables, persistent qcow2 guests, and AOSP images.

## Documentation

- [Product requirements](docs/product-requirements.md)
- [Architecture](docs/architecture.md)
- [Security model](docs/security.md)
- [Architecture decisions](docs/decisions.md)
- [MVP plan](docs/mvp-plan.md)

## Project status

This repository is currently in the design phase. The first implementation task
is a compatibility spike against the current Scrcpy Remote iOS release. No
production-ready service exists yet.
