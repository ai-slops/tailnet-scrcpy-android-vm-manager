# Architecture

## System context and role separation

```text
 Android control plane
 Scrcpy Remote on iOS -- Tailscale transport --> isolated router VM
        ADB private key                         tailscaled + nftables
                                                       |
                                               explicit guest /32 routes
                                                       |
                                             persistent Android VMs
                                               authenticated adbd

 Management plane
 iOS SSH client -- key-authenticated SSH --> Linux KVM host
                                              hostctl + libvirt

 Future management plane
 browser/app -- separately authenticated API --> unprivileged manager
                                                      |
                                              narrow local host agent
```

These planes serve different purposes and do not grant authority to each other:

- Tailscale transports ADB traffic and coordinates routes.
- Android `adbd` verifies whether a Scrcpy Remote device may control a guest.
- The host CLI controls inventory and VM lifecycle. It runs locally, either at
  the host console or through SSH.
- A future web UI/API is a management product surface with its own reviewed
  authentication and authorization. Tailscale identity must not silently become
  its authorization database.

The KVM host and Android guests do not run Tailscale. The dedicated router VM
has its own Tailscale state and network namespace, advertises only configured
guest `/32`s, and forwards only mapped source-to-guest TCP 5555 flows. Scrcpy
Remote connects to the Android address, never a listener on the KVM host.

## Components

### Manager core and host administration

`manager-core` provides validated configuration and bounded primitives for ADB
public keys, router policy, VM lifecycle, hibernation, and stopped-state
snapshots. `hostctl` exposes host administration operations. There is currently
no manager network API: remote operators use an ordinary SSH client to execute
the same local CLI.

A future manager service may own desired state, device enrollment,
device-to-VM permission, reconciliation, and audit records. Network-facing code
must remain unprivileged. Privileged libvirt and storage operations must use a
narrow local interface with structured arguments, never arbitrary commands or
caller-selected paths.

### Tailnet router appliance

The router holds an ordinary Tailscale auth key for initial enrollment and
persistent node state. Route approval remains an external administrative step.
`routerctl` enrolls or reconfigures the node, advertises deduplicated guest
`/32`s, atomically installs the router-owned nftables table, and checks network
readiness.

The forwarding chain is fail-closed. A typed libnftables JSON ruleset stores
each `(Tailscale source IPv4, Android destination IPv4)` pair in a concatenated
set. It permits only set members on TCP 5555, established replies, and guest
Internet NAT, and drops every other forwarded packet.

Controller records own their observed Tailscale source addresses and ADB public
key. VM records optionally reference controller names; an omitted list means
all active controllers. The router flow set and desired guest key bundle are
derived from this single relationship, so address and key policy cannot drift.

The source allowlist narrows exposure and helps operational revocation. It is
not a durable identity proof: a compromised coordination plane may change which
peer receives an allowed address.

### Android VM runtime

Libvirt manages persistent QEMU/KVM guests. Each configured VM has a stable
domain name and private address, a project-approved image lineage, persistent
qcow2 storage, and an isolated virtual NIC. Managed save provides SSD-backed
hibernation. Internal qcow2 snapshots are allowed only while fully stopped.

Authenticated `adbd` is the authoritative Scrcpy Remote control boundary. Each
iOS controller has a unique ADB key, and only its public key is installed in an
authorized guest. A controller permission grants broad ADB authority inside
that VM; it is not a scrcpy-only sandbox.

## Connection flow

```text
1. Enroll the router and Scrcpy Remote node with ordinary Tailscale enrollment.
2. Approve only the router's configured Android /32 routes.
3. Register the iOS device's ADB public key for a VM.
4. Add the observed Scrcpy Remote source address to the router mapping.
5. Start or restore the persistent Android VM.
6. Scrcpy Remote connects to <android-private-ip>:5555 through Tailscale.
7. Router nftables checks source, destination, and port.
8. Android adbd challenges the controller's ADB private key.
```

Steps 1, 2, and 7 provide reachability and exposure reduction. Step 8 grants
Android control. If Tailscale's control plane is compromised, an attacker may
gain network reachability to adbd but still needs an authorized ADB private key.
Availability attacks and exploitation of a vulnerable adbd remain residual
risks.

Powering a VM off does not withdraw its persistent `/32`; connections simply
fail while it is stopped. This avoids route approval churn for frequently
started game VMs.

## Desired-state reconciliation

The future manager compares its local database with libvirt domains,
managed-save state, snapshots, router mappings, and guest ADB public keys.
Unknown mappings and revoked guest keys are removed. Tailscale identity or
`whois` output must never create a device permission.

The host can already validate and render each VM's complete desired
`adb_keys` content. Applying that content remains an Android image integration
boundary: the planned image uses a host-generated read-only configuration
artifact and a narrowly scoped Android init service. Until that service exists,
operators must approve or remove keys through the image-specific console path;
the manager must not claim that rendered desired state has reached a guest.

The likely single-host schema contains `devices`, `vms`, `vm_permissions`, and
`audit_events`. The domain model uses project-owned ADB credentials rather than
Tailscale users or nodes, keeping a future Headscale, plain WireGuard, or LAN
transport possible without replacing Android authorization.
