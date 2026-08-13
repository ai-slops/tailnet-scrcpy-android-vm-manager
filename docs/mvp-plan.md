# MVP Implementation Plan

## Phase 0: Compatibility spike

This phase is a release blocker because Scrcpy Remote is a third-party client
whose detailed protocol and credential behavior are not controlled by this
project.

Deliverables:

- a reproducible test with one iOS device, one Linux host, and one Android VM;
- captured app version and iOS version;
- documented host/port configuration procedure;
- a sample public ADB key and its computed fingerprint, excluding private data;
- proof that an accepted key connects and a rejected key does not;
- proof of key revocation and reconnection behavior;
- confirmation of compatible scrcpy server version and options; and
- a decision for ADR-P01 through ADR-P05 where the spike provides evidence.

No broad application implementation should precede this spike.

## Phase 1: Host foundation

- Implement host preflight checks.
- Define configuration and storage directory layouts.
- Install and validate the isolated libvirt network.
- Create the unprivileged service account and privileged agent boundary.
- Establish structured logging and SQLite migrations.

Exit criteria:

- a clean Ubuntu LTS host can pass or fail preflight with actionable messages;
- the VM network is isolated according to documented firewall invariants; and
- API and host-agent processes run with their intended privileges.

## Phase 2: Persistent VM lifecycle

- Import and verify an immutable AOSP base image.
- Create a VM-specific qcow2 overlay and libvirt domain.
- Implement create, start, stop, reboot, reset, and delete operations.
- Reconcile actual and desired state after process and host restarts.
- Enforce CPU, memory, and disk limits.

Exit criteria:

- VM state survives host and service restarts;
- factory reset replaces only the overlay;
- deletion is constrained to known VM resources; and
- the guest has outbound internet but no path to another VM or host management
  service.

## Phase 3: Device authorization

- Register and fingerprint ADB public keys.
- Model device lifecycle and VM permissions.
- Synchronize authorized keys into selected guests.
- Implement complete revocation and audit logging.
- Prevent private-key ingestion through all APIs.

Exit criteria:

- only a registered key with active VM permission authenticates to that VM;
- the same key cannot access an unassigned VM; and
- revocation terminates existing access and blocks reconnection.

## Phase 4: Leased remote endpoints

- Allocate collision-free ports from a configured range.
- Bind endpoints only on the host Tailscale address.
- Forward each endpoint to exactly one VM ADB address.
- Enforce expiry, connection limits, and immediate teardown.
- Restore or remove state safely during reconciliation.

Exit criteria:

- LAN and public interfaces cannot reach endpoint ports;
- arbitrary tailnet forwarding to the guest subnet remains impossible;
- stale endpoints do not survive reconciliation; and
- Scrcpy Remote can control the selected VM end to end.

## Phase 5: Operational hardening

- Add encrypted backup and documented recovery procedures.
- Add disk-space and resource-pressure guards.
- Add Tailnet Lock readiness checks without making Tailscale identity an
  authorization dependency.
- Add audit export and log retention settings.
- Add upgrade and rollback procedures for schema, images, and services.
- Perform threat-model and privilege-boundary review.

## Test strategy

The MVP requires:

- unit tests for state transitions, permission decisions, lease allocation, and
  configuration validation;
- integration tests against libvirt, nftables, and a disposable Android VM;
- negative network tests from LAN, unauthorized tailnet nodes, and other guests;
- crash-recovery tests for API, agent, gateway, and host restarts;
- revocation tests with an established ADB connection; and
- a manual iOS compatibility suite for every supported Scrcpy Remote update.

## Deferred work

- Native management applications and mTLS enrollment
- Browser UI and browser-based control
- Multiple simultaneous viewers
- Multi-host placement and migration
- PostgreSQL and high availability
- GPU passthrough
- Additional Linux distributions
- Automated image build and signing pipeline
