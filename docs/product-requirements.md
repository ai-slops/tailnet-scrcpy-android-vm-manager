# Product Requirements

## 1. Purpose

The system manages persistent Android virtual machines on a KVM-capable Linux
server and makes each VM controllable from the Scrcpy Remote iOS app over a
Tailscale network.

The primary security objective is to retain control over device admission even
if Tailscale's official control-plane infrastructure is compromised. A device
that has not been approved by the operator must not be able to establish a
tailnet data-plane connection to the host.

## 2. Confirmed product decisions

- The server supports Linux hosts with KVM only.
- The MVP officially supports one x86-64 Ubuntu LTS host.
- Android guests do not contain a Tailscale client.
- A dedicated minimal router VM is the only Tailscale-connected part of the VM
  network. The KVM host and Android guests do not run Tailscale.
- The deployment uses a dedicated tailnet with Tailnet Lock enabled.
- Tailnet Lock signing is a manual operator action.
- Users are not modeled. Authorization is attached directly to device
  credentials.
- VMs are persistent.
- The Scrcpy Remote iOS app is the required control client.
- The local authorization mechanism must not depend on Tailscale identity,
  `tailscale whois`, Tailscale IP ownership, Grants capabilities, or the
  Tailscale API.

## 3. MVP capabilities

The MVP must:

1. Validate host prerequisites before installation.
2. Register and inspect one KVM host.
3. create, start, stop, reboot, reset, and delete persistent Android VMs;
4. create each VM from an immutable AOSP base image and a qcow2 overlay;
5. place guests on an isolated libvirt network;
6. expose a separately allocated TCP endpoint for each running VM on the host's
   Tailscale address;
7. forward that endpoint only to the selected guest's authenticated ADB daemon;
8. register, authorize, and revoke a distinct ADB public key per client device;
9. apply device-to-VM `controller` permissions from a host-local database;
10. reconcile firewall rules, endpoint leases, and VM state after restart;
11. record security-sensitive operations in an audit log; and
12. demonstrate screen display and input control from Scrcpy Remote on a current
    supported iOS/iPadOS version.

## 4. Initial authorization model

There is one principal type: `device`.

Each device has:

- a generated immutable identifier;
- a display name assigned by the operator;
- an ADB public-key fingerprint;
- a lifecycle state: `pending`, `active`, or `revoked`; and
- zero or more VM permissions.

The MVP exposes one permission:

- `controller`: permits ADB-backed scrcpy control and therefore grants effective
  administrator access inside the Android guest.

ADB cannot safely provide a scrcpy-only subset of its authority. A controller
must be treated as capable of opening a shell, installing packages, reading
guest-accessible data, and changing the guest configuration.

## 5. VM defaults

- Architecture: x86-64
- Image: project-approved AOSP image without Google Apps
- Storage: immutable base image plus persistent qcow2 overlay
- Default CPU: 4 vCPUs, configurable
- Default memory: 4 GiB, configurable
- Default display: portrait 1080 x 1920, configurable
- Networking: outbound NAT; no inbound LAN or WAN access
- Inter-VM networking: denied
- GPU passthrough: unsupported in the MVP
- Running-state snapshots: unsupported in the MVP
- Stopped-state snapshots: deferred until core lifecycle behavior is stable

## 6. Session policy

- A VM has at most one active controller endpoint lease.
- A lease has an explicit expiry and can be revoked immediately.
- Revoking a device closes its active endpoint leases and removes its ADB key
  authorization from affected guests.
- Disconnecting Scrcpy Remote does not stop the VM.
- Clipboard synchronization, file transfer, audio capture, and microphone
  forwarding are disabled unless explicitly validated and enabled later.

## 7. Non-goals for the MVP

- Browser-based Android control
- A custom scrcpy client
- Multi-host scheduling or live migration
- High availability
- ARM guests
- Google Play or Google Apps distribution
- GPU or device passthrough
- Arbitrary user-supplied VM images
- Multi-user identity, SSO, or role management
- Advertising the entire guest subnet instead of explicit Android VM `/32`
  routes
- Exposing a general-purpose remote ADB server on TCP port 5037

## 8. Compatibility acceptance test

Before the production architecture is implemented, a spike must verify the
current Scrcpy Remote iOS app behavior. It passes only if all of the following
are demonstrated:

1. The app can connect to a routed Android VM IPv4 address on TCP port 5555.
2. The app can import or generate a stable ADB key pair.
3. The corresponding ADB public key can be extracted and fingerprinted.
4. Android authorizes that key without interactive confirmation on every
   connection.
5. An unregistered ADB key cannot start a scrcpy session.
6. Revocation takes effect without rebuilding the VM.
7. Video, touch, keyboard input, and reconnect behavior work through the host
   forwarding path.
8. The app and guest scrcpy server versions are compatible.

If any of items 1 through 6 fail, the direct compatibility design must be
revisited before implementation continues.
