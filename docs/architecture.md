# Architecture

## System context

```text
 operator-held Tailnet Lock keys
              |
              | sign node keys
              v
 Scrcpy Remote iOS === dedicated tailnet === Tailnet router appliance
   Tailscale + ADB key                         tailscaled + nftables
                                                        |
                                                advertises explicit /32s
                                                        |
                                                isolated VM network
                                                        |
                                              persistent Android VMs
                                                authenticated adbd

 Linux KVM host: libvirt + manager services (not a tailnet node)
```

The router appliance is the only server-side tailnet node. The current
implementation generates a small persistent libvirt VM, but the security
boundary permits a separately confined container in a future deployment. It
must have its own Tailscale state, network namespace, and least-privilege
forwarding capabilities; it must never share a host `tailscaled` instance.

Android guests contain no Tailscale client. Each persistent guest has a fixed
private IPv4 address. The router advertises only the explicitly configured
guest `/32`s and accepts only mapped controller-IP-to-guest TCP 5555 traffic.
Scrcpy Remote therefore connects directly to the Android address rather than a
port on the KVM host.

## Components

### Manager core and host administration

`manager-core` provides validated configuration and bounded primitives for ADB
public keys, router policy, VM lifecycle, hibernation, and stopped-state
snapshots. `hostctl` exposes the implemented host administration operations.
The KVM host does not run Tailscale and its management surface is local-only.

A future manager service will own desired state, device enrollment,
device-to-VM permission, reconciliation, and audit records. Network-facing code
must remain unprivileged. Privileged libvirt, storage, and firewall operations
must be exposed through a narrow local interface with structured arguments,
never arbitrary commands or caller-selected paths.

### Tailnet router appliance

The router holds its own ordinary Tailscale auth key and persistent node state.
After automatic enrollment, it remains unusable until a trusted signing node
manually signs it under Tailnet Lock. Route approval is a separate manual
control.

`routerctl`:

- enrolls the node and advertises deduplicated guest `/32`s;
- generates and atomically installs the router-owned nftables table; and
- checks IPv4 forwarding and Tailnet Lock readiness.

The forwarding chain is fail-closed. A typed libnftables JSON ruleset stores
each `(Tailscale source IPv4, Android destination IPv4)` pair in a concatenated
set, so multiple approved phones do not duplicate rule structure. It permits
only set members on TCP 5555, established replies and guest Internet NAT, and
drops every other forwarded packet.

### Android VM runtime

Libvirt manages persistent QEMU/KVM guests. Each configured VM has a stable
domain name and private address, a project-approved image lineage, persistent
qcow2 storage, and an isolated virtual NIC. Managed save provides SSD-backed
hibernation. Internal qcow2 snapshots are permitted only when fully stopped.

Authenticated `adbd` is the protocol-compatible authorization boundary used by
Scrcpy Remote. A controller permission grants broad ADB authority inside that
VM; it is not a scrcpy-only sandbox.

## Authorization model

Network admission and VM authorization are independent:

1. Tailnet Lock admits the signed router and iOS node to the WireGuard overlay.
2. Router-local nftables maps the controller's tailnet IPv4 address to one
   Android address. This is a narrow operational allowlist, not durable
   identity by itself.
3. Android `adbd` authenticates the controller's unique ADB private key against
   project-managed public-key authorization.
4. The future local database maps the ADB public-key fingerprint to VM
   permissions and remains independent of Tailscale identity APIs.

A usable attack therefore requires passing both the operator-controlled
network admission boundary and the project-controlled ADB authorization
boundary. Tailscale Grants and route approval remain defense in depth.

## Connection flow

```text
1. Enroll router/iOS nodes with ordinary Tailscale auth.
2. Manually sign their node keys from a trusted Tailnet Lock signing node.
3. Approve only the router's configured Android /32 routes.
4. Register the iOS device's ADB public key and grant access to a VM.
5. Reconcile the guest ADB key and router source-IP mapping.
6. Start or restore the persistent Android VM.
7. Scrcpy Remote connects to <android-private-ip>:5555 through Tailscale.
8. Router nftables checks source, destination, and port.
9. Android adbd challenges the device's ADB key.
```

Powering a VM off does not withdraw its persistent `/32`; connections simply
fail while it is stopped. This avoids route approval churn when game VMs are
started and stopped frequently.

## Desired-state reconciliation

The future manager must compare its local database with libvirt domains,
managed-save state, snapshots, router mappings, and guest ADB public keys.
Unknown mappings are removed and revoked keys are removed from guests. Runtime
state is derived from the database; Tailscale identity or `whois` output must
never create a device permission.

The likely single-host schema contains `devices`, `vms`, `vm_permissions`, and
`audit_events`. A separate endpoint-lease or host-port allocation model is not
part of the direct routed design.

## Portability boundary

The domain model uses project-owned device credentials rather than Tailscale
users or node identities. A network-admission provider may report readiness and
addresses, but it cannot grant VM permission. This keeps a future Headscale,
plain WireGuard, or private-LAN deployment possible without replacing the ADB
authorization model.
