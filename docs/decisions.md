# Architecture Decisions

This document records decisions that constrain the first implementation. A
change to an accepted decision should be made explicitly and accompanied by a
replacement rationale.

## Accepted

### ADR-001: Treat Tailscale as transport, not authorization

**Decision:** A dedicated Tailscale tailnet carries Scrcpy Remote traffic to the
isolated router appliance. The KVM host does not join. Tailscale node identity
and IP address never grant Android or manager authorization.

**Rationale:** Android's ADB challenge-response provides a client-key boundary
compatible with Scrcpy Remote. A compromised coordination plane may create
reachability, but it does not possess an approved ADB private key.

### ADR-002: Do not install Tailscale in Android guests

**Decision:** Tailscale terminates in a dedicated minimal router VM. The KVM
host and Android guests are not tailnet nodes. The router advertises only one
`/32` per enabled persistent Android VM.

**Rationale:** This isolates Tailscale state from the host, keeps Android images
independent of Tailscale, and exposes only controller-to-ADB flows selected by
router-local forwarding policy.

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

### ADR-005: Route explicit Android `/32` endpoints directly

**Decision:** The isolated router advertises each configured Android VM's
persistent `/32`. Scrcpy Remote connects to that address on TCP 5555. The KVM
host exposes no tailnet listener and allocates no proxy port.

**Rationale:** Direct routing matches the iOS client's supported Tailscale path,
keeps the KVM host outside the tailnet, and removes a per-session proxy and port
allocation subsystem. Router-local source allowlisting narrows exposure, while
Android ADB keys independently authenticate control. Source IP is not an
independent identity proof.

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

**Current scope:** No network API is exposed. Operators run `hostctl` locally,
including through key-authenticated SSH from an iOS SSH client. A future web UI
must use a separately reviewed management authentication mechanism.

### ADR-009: Use SQLite for the single-host MVP

**Decision:** Desired state, authorization, and audit metadata use
SQLite in WAL mode.

**Rationale:** A single-host deployment does not justify a separate database
service. The schema retains `host_id`-compatible boundaries for future growth.

### ADR-010: Use typed libnftables JSON and concatenated sets

**Decision:** Router policy is generated with the `nftables` Rust crate and
applied through nftables' JSON API. Allowed `(controller source, Android guest)`
pairs live in one concatenated set.

**Rationale:** Parsed IP addresses and a typed JSON schema avoid shell syntax
construction. A set scales to several physical controller addresses without
duplicating rule structure, and the complete project-owned table is replaced
as one transaction.

## Pending validation

### ADR-P01: Scrcpy Remote endpoint behavior

Validate whether the current iOS app supports a manually specified Tailscale IP
and arbitrary port, and whether it connects directly to adbd rather than to a
remote ADB server.

### ADR-P02: Scrcpy Remote ADB key handling

Validate key generation, import/export encoding, fingerprint extraction,
persistence across app upgrades, and behavior when Android rejects a key.

### ADR-P03: Guest ADB key synchronization

Choose between offline disk injection, a QEMU guest agent path, an authenticated
bootstrap channel, or controlled adbd configuration. The selected mechanism
must support immediate revocation without exposing a general host-to-guest
shell surface.

### ADR-P04: Manager interaction from iOS

Determine how the operator discovers the persistent Android `/32` and enters it
into Scrcpy Remote. Manual copy is acceptable initially; deep links or
automation must be based on documented app behavior before adoption.
