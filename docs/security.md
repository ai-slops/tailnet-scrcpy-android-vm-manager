# Security Model

## 1. Security objectives

The system must provide:

1. operator-controlled admission to the overlay network;
2. authorization independent of Tailscale identity and policy services;
3. isolation between the tailnet, host, and Android guest networks;
4. explicit, revocable device-to-VM control permission;
5. protection of persistent guest data and device credentials; and
6. auditable security-sensitive state changes.

## 2. Threat model

### In scope

- Compromise or malicious operation of Tailscale's coordination infrastructure
- Unauthorized users authenticating to the operator's Tailscale account
- Accidental or malicious widening of Tailscale Grants
- Network scanning from another tailnet device
- A lost or stolen authorized iOS device
- A malicious or compromised Android guest
- Malformed Manager API input
- Stale firewall rules or endpoint processes after a crash
- Theft of VM disk files or database backups

### Out of scope for the MVP

- Full compromise of the KVM host or its kernel
- Extraction of keys from a fully compromised authorized client device
- Side-channel attacks between co-resident VMs
- Malicious host firmware or hypervisor supply-chain compromise
- Availability of Tailscale's coordination or DERP infrastructure

## 3. Trust boundaries

### 3.1 Tailnet admission

Tailnet Lock is the only Tailscale feature treated as a security root. Every
router VM and iOS control node must carry a node-key signature rooted in
operator-managed Tailnet Lock keys. The KVM host is not a tailnet node.

Grants are defense in depth. They are not the local application's authorization
database, and a Tailscale user name, device name, tag, IP address, or `whois`
response must not create a VM permission.

### 3.2 VM control authorization

ADB public-key authentication is the protocol-compatible second boundary for
Scrcpy Remote. Authorization is granted to the fingerprint of a public key and
is scoped to a VM in the local database.

The ADB private key must remain on the iOS device. The server stores only its
public key and fingerprint. A shared key across multiple client devices is not
permitted because it prevents independent audit and revocation.

ADB authorization grants broad control of the guest. It is not an application-
level sandbox and must not be described as scrcpy-only permission.

### 3.3 Management clients

If a management UI or native enrollment client is added, it must authenticate
with a project-controlled protocol such as mTLS. That credential is separate
from the ADB key unless an explicit, reviewed binding protocol is designed.

## 4. Control-plane compromise analysis

Under the assumed Tailscale control-plane compromise:

- an attacker cannot introduce a new usable WireGuard node without a valid
  Tailnet Lock signature;
- Grants and Tailscale-supplied identity metadata are not sufficient evidence
  of VM authorization;
- local firewall rules expose only the bounded Manager and endpoint surface;
- an ADB session still requires a locally registered public key; and
- the local permission database, not the Tailscale control plane, decides which
  key may control which VM.

The dedicated tailnet is important. Only the host, approved control devices, and
signing devices should be admitted. This reduces the risk that an already-signed
but lower-trust node benefits from a maliciously widened packet filter.

## 5. Key management

### Tailnet Lock

- Configure at least two signing nodes.
- Do not make ordinary iOS control devices signing nodes.
- Keep at least one signing node offline during normal operation.
- Store disablement secrets encrypted and offline in separate locations.
- Do not use signed reusable auth keys for automatic enrollment.
- Revoke a lost node's Tailnet Lock key promptly.

### ADB keys

- Generate a unique key pair per iOS device.
- Record and confirm the public-key fingerprint during enrollment.
- Never upload or back up client private keys to the Manager.
- Store only public material in the Manager database.
- Remove revoked keys from guest authorization and terminate matching leases.
- Confirm the exact Scrcpy Remote import/export key format during the
  compatibility spike.

### Management mTLS

If implemented later, use an offline root CA and per-device non-exportable
private keys where the platform supports them. mTLS must not be placed in the
Scrcpy Remote data path unless the application explicitly supports it.

## 6. Firewall invariants

The router VM and host firewall must preserve these invariants even when
Tailscale Grants are overly permissive:

```text
public/LAN -> management ports       DENY
public/LAN -> endpoint port range    DENY
router tailscale0 -> VM subnet       DENY by default
mapped controller -> selected ADB    ALLOW while authorized
VM -> host management plane          DENY
VM -> other VM                       DENY
VM -> internet                       ALLOW through controlled NAT
```

Rules must bind to explicit interfaces, addresses, protocols, ports, and VM
destinations. No wildcard DNAT from the tailnet to the guest network is allowed.

## 7. Revocation

Revocation of an iOS control device is complete only after all of the following:

1. mark the device `revoked` in the local database;
2. reject creation or renewal of its endpoint leases;
3. terminate its active endpoint connections;
4. remove its ADB public key from every authorized guest;
5. remove the device from the tailnet and revoke its Tailnet Lock key; and
6. append an immutable audit event.

Local ADB revocation and Tailnet Lock revocation are intentionally independent.
Either boundary should block useful VM control.

The host additionally limits endpoint traffic to configured controller
Tailscale IPv4 addresses in both nftables and the endpoint gateway. This is a
defense-in-depth operational restriction, not an independent identity proof:
without Tailnet Lock, a compromised Tailscale control plane could change the
node-key-to-IP mapping supplied to the host.

## 8. Sensitive data

Sensitive server-side material includes:

- VM disks and snapshots;
- ADB public keys and their VM permission mappings;
- Manager server keys, if introduced;
- Tailnet Lock state on signing nodes;
- disablement secrets; and
- audit records that describe device activity.

Backups must be encrypted. Disablement secrets and an offline CA private key must
not be stored in the same backup set as the KVM host.

## 9. Audit events

At minimum, record:

- device enrollment, activation, and revocation;
- ADB fingerprint changes;
- permission grants and removals;
- VM creation, reset, snapshot, and deletion;
- endpoint lease creation, expiry, and forced termination;
- host-agent reconciliation changes; and
- security check failures.

Do not record ADB private keys, session payloads, clipboard contents, or guest
screen contents.
