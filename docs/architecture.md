# Architecture

## 1. System context

```text
                         operator-controlled trust
                    +-------------------------------+
                    | Tailnet Lock signing devices  |
                    +---------------+---------------+
                                    | signs node keys
                                    v
+-------------------+       Tailscale/WireGuard       +---------------------+
| Scrcpy Remote iOS | ------------------------------> | Linux KVM host      |
| - Tailscale node  |                                 | - tailscaled        |
| - per-device ADB  |                                 | - manager-api       |
|   private key     |                                 | - host-agent        |
+-------------------+                                 | - endpoint gateway  |
                                                      +----------+----------+
                                                                 |
                                                      isolated libvirt bridge
                                                                 |
                                                      +----------v----------+
                                                      | Persistent Android  |
                                                      | VM with authenticated|
                                                      | ADB enabled         |
                                                      +---------------------+
```

Tailscale terminates on the host. Guests have private addresses and are never
tailnet nodes or advertised subnet routes.

## 2. Components

### 2.1 Manager API

The Manager API owns desired state and exposes VM lifecycle, device enrollment,
permission, endpoint lease, and audit operations. It runs without root
privileges and cannot execute arbitrary shell commands.

No API authentication protocol for a future management UI is required by the
Scrcpy Remote data path. If a separate native management client is introduced,
it should use project-owned mTLS credentials rather than Tailscale identity.

### 2.2 Host agent

The host agent is a privileged, local-only process. It receives a narrow command
set over a Unix-domain socket and performs:

- libvirt lifecycle operations;
- disk and image operations within configured storage roots;
- VM network configuration;
- nftables endpoint rule installation and removal;
- ADB public-key enrollment and revocation; and
- reconciliation after crashes or host restarts.

The agent must use structured arguments. It must not accept shell fragments,
arbitrary QEMU arguments, or caller-selected filesystem paths.

### 2.3 Endpoint gateway

Scrcpy Remote expects an ADB-accessible address. For each active VM lease, the
host allocates a TCP port from a configured range on its Tailscale address and
forwards it to that VM's private ADB port.

Example:

```text
Host 100.x.y.z:31042 -> Android VM 10.80.0.42:5555
```

The gateway is not a remote ADB server and does not expose TCP 5037. It is a
bounded per-VM transport path. The Android guest's ADB daemon performs the
second cryptographic authentication using the client's ADB key.

An implementation may use an nftables DNAT rule or a small TCP proxy. A proxy is
preferred if connection limits, lease expiry, metrics, and immediate teardown
cannot be enforced cleanly with DNAT alone.

### 2.4 VM runtime

Libvirt manages QEMU/KVM guests. Every VM has:

- an immutable base image reference;
- a persistent qcow2 overlay;
- an isolated virtual NIC;
- stable internal identity independent of its current IP address;
- an ADB key allowlist derived from local authorization state; and
- runtime and desired-state records in the database.

### 2.5 Local database

SQLite in WAL mode is sufficient for the single-host MVP. Suggested logical
tables are:

```text
devices
  id, display_name, adb_public_key, adb_fingerprint, status, created_at

vms
  id, name, image_id, state, desired_state, libvirt_domain, autostart

vm_permissions
  device_id, vm_id, role, expires_at

endpoint_leases
  id, vm_id, device_id, listen_port, expires_at, state

audit_events
  id, occurred_at, actor_device_id, action, target_type, target_id, details
```

The database is authoritative for authorization. Runtime rules and guest ADB
key files are derived state and must be reconciled from it.

## 3. Network design

```text
Internet/LAN                 dedicated tailnet
     |                              |
     X                         tailscale0
     |                              |
     +---------- KVM host ----------+
                    |
           endpoint gateway only
                    |
             vmbr-android
             10.80.0.1/24
              /          \
       VM 10.80.0.2   VM 10.80.0.3
```

Required policy:

- Manager and endpoint ports are not reachable from public or LAN interfaces.
- Tailnet-to-guest forwarding is denied by default.
- Only active endpoint leases may reach a guest ADB port.
- Guests cannot initiate connections to the host management plane.
- Guests cannot communicate with each other.
- Guests receive outbound internet access through controlled NAT.
- No guest subnet route is advertised to the tailnet.

Tailscale Grants should allow only the host and required port range, but local
firewall policy remains mandatory and independent.

## 4. Connection flow

```text
1. Operator admits the iOS node by signing it with Tailnet Lock.
2. Operator imports/registers that device's ADB public key.
3. Operator grants the device `controller` permission for a VM.
4. Manager starts the VM and synchronizes its ADB authorized keys.
5. Manager creates a short-lived endpoint lease and allocates a host port.
6. Scrcpy Remote connects to the host Tailscale IP and allocated port.
7. Tailnet Lock-protected WireGuard admits the network peer.
8. Android adbd challenges and authenticates the device's ADB private key.
9. Scrcpy Remote deploys/starts its compatible scrcpy server and controls the VM.
10. Expiry or revocation closes the endpoint and terminates active connections.
```

The mechanism used to deliver the selected host and port to the iOS app remains
part of the compatibility spike. Manual entry is acceptable for the first MVP.

## 5. Reconciliation

On startup the host agent must compare:

- database desired state;
- libvirt domain state;
- allocated endpoint ports;
- running gateway processes or nftables rules; and
- guest ADB authorized keys.

Unknown forwarding rules are removed. Expired leases are closed. Missing rules
for valid leases are recreated only after the VM and authorization state are
confirmed. Running VMs remain running across Manager API restarts.

## 6. Future portability

The core domain model must not contain Tailscale-specific identities. A
`network-admission provider` interface may report readiness and bind addresses,
but device authorization always uses project-owned credentials.

This permits a future deployment to replace Tailscale with Headscale, plain
WireGuard, a private LAN, or another overlay without changing VM permissions or
ADB enrollment records.
