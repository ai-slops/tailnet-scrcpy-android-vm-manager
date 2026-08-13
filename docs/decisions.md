# Architecture Decisions

This document records decisions that constrain the first implementation. A
change to an accepted decision should be made explicitly and accompanied by a
replacement rationale.

## Accepted

### ADR-001: Use a dedicated Tailnet Lock-enabled tailnet

**Decision:** The deployment uses a dedicated Tailscale Personal tailnet. Only
the KVM host, approved control devices, and signing devices join it.

**Rationale:** Tailnet Lock keeps node-key admission under operator-controlled
cryptographic authority even if Tailscale's control plane is compromised. A
dedicated tailnet prevents unrelated signed nodes from sharing the trust domain.

### ADR-002: Do not install Tailscale in Android guests

**Decision:** Tailscale terminates on the KVM host. Guests use an isolated
libvirt network and are not advertised through a subnet router.

**Rationale:** This centralizes network policy, keeps VM images independent of
Tailscale, and avoids exposing the guest subnet to every permitted tailnet peer.

### ADR-003: Use device credentials, not users

**Decision:** The initial authorization principal is a device credential. The
system has no local user or account model.

**Rationale:** The required policy is approval and revocation of particular
physical clients. Adding users would not improve the initial threat model.

### ADR-004: Use ADB keys for the Scrcpy Remote authorization boundary

**Decision:** Each iOS controller uses a unique ADB key. A local database maps
its public-key fingerprint to VMs.

**Rationale:** Scrcpy Remote already speaks ADB and has advertised ADB key
import/export support. mTLS cannot be inserted in an unmodified third-party app
unless that app explicitly supports it. ADB authentication is independent of
Tailscale and is enforced by the Android endpoint.

**Consequence:** A controller has broad ADB authority inside an authorized VM;
the project cannot claim scrcpy-only access.

### ADR-005: Expose per-VM leased endpoints on the host

**Decision:** A running, authorized VM receives a temporary port on the host's
Tailscale address. That port forwards only to the VM's private ADB endpoint.

**Rationale:** The iOS app can use a conventional host and port while guests
remain isolated and unmodified by Tailscale.

### ADR-006: Use persistent qcow2 overlays

**Decision:** Each VM stores persistent state in a qcow2 overlay backed by a
project-approved immutable AOSP base image.

**Rationale:** This provides persistence, efficient provisioning, and a clear
factory-reset operation without mutating the distributed base image.

### ADR-007: Target one Ubuntu LTS x86-64 host first

**Decision:** The MVP supports one x86-64 Ubuntu LTS system with KVM, libvirt,
QEMU, systemd, cgroup v2, and nftables.

**Rationale:** Linux/KVM distributions differ in packaging, security policy, and
network tooling. A narrow initial support matrix is testable.

### ADR-008: Separate the API from privileged host operations

**Decision:** An unprivileged API communicates over a Unix-domain socket with a
narrow privileged host agent.

**Rationale:** A network-facing parsing error must not directly become arbitrary
root command execution.

### ADR-009: Use SQLite for the single-host MVP

**Decision:** Desired state, authorization, leases, and audit metadata use
SQLite in WAL mode.

**Rationale:** A single-host deployment does not justify a separate database
service. The schema retains `host_id`-compatible boundaries for future growth.

## Pending validation

### ADR-P01: Scrcpy Remote endpoint behavior

Validate whether the current iOS app supports a manually specified Tailscale IP
and arbitrary port, and whether it connects directly to adbd rather than to a
remote ADB server.

### ADR-P02: Scrcpy Remote ADB key handling

Validate key generation, import/export encoding, fingerprint extraction,
persistence across app upgrades, and behavior when Android rejects a key.

### ADR-P03: Endpoint implementation

Choose between a dedicated TCP proxy and nftables DNAT after measuring whether
each option can reliably enforce lease expiry, connection termination, port
ownership, and observability.

### ADR-P04: Guest ADB key synchronization

Choose between offline disk injection, a QEMU guest agent path, an authenticated
bootstrap channel, or controlled adbd configuration. The selected mechanism
must support immediate revocation without exposing a general host-to-guest
shell surface.

### ADR-P05: Manager interaction from iOS

Determine how the operator discovers the per-VM endpoint and enters it into
Scrcpy Remote. Manual copy is acceptable initially; deep links or automation
must be based on documented app behavior before adoption.
