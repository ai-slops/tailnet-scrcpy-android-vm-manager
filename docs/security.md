# Security Model

## Security objectives

The system must:

1. authorize Android control with project-managed ADB public keys;
2. keep VM administration separate from the Android data path;
3. isolate the tailnet, KVM host, and Android guest networks;
4. minimize adbd exposure with explicit source-to-guest mappings;
5. support independent, prompt credential revocation; and
6. protect persistent guest data and security-sensitive audit records.

## Threat model

In scope are a compromised or malicious Tailscale coordination plane, widened
Tailscale policy, another tailnet node scanning reachable addresses, a lost iOS
controller, a malicious guest, stale router state, malformed future Manager API
input, and stolen VM disks or backups.

Full KVM host or kernel compromise, extraction of keys from a fully compromised
authorized controller, hypervisor side channels, malicious firmware, and
Tailscale/DERP availability are outside the MVP threat model.

## Trust boundaries

### Tailscale transport

Tailscale supplies encrypted peer transport, address assignment, and route
coordination. Its identities, tags, Grants, IP assignments, and `whois` output
are not authorization evidence for Android or manager operations.

The router source allowlist reduces the number of nodes that can reach an adbd
socket during normal operation. It is not independent authentication because a
compromised coordination plane may manipulate peer identity, policy, or address
assignment. Route approval and Tailscale policy are defense in depth.

### Android control

ADB public-key challenge-response is the authoritative boundary compatible with
Scrcpy Remote. A unique public-key fingerprint is scoped to each VM in the local
configuration or future database. The private key remains on the iOS device.
Shared client keys are prohibited because they prevent independent audit and
revocation.

ADB grants broad guest control, including shell and package operations. It must
not be described as scrcpy-only permission.

### Host management

`hostctl` runs locally on the KVM host. An iOS operator may reach it through
key-authenticated SSH; SSH host and client key validation form the remote
management boundary. The Android tailnet route exposes neither SSH nor a
manager port.

A future web UI/API must have independently reviewed authentication, CSRF and
session handling, authorization, audit, and privilege separation. If it is
network-exposed, prefer a separate management network or independent WireGuard
or mTLS credentials. Being a Tailscale node alone must not grant manager access.

## Coordination-plane compromise

If Tailscale's official infrastructure is compromised, assume the attacker can
introduce or remap nodes, widen distributed packet policy, and create network
reachability to advertised Android endpoints. Consequently, neither the router
source IP nor tailnet membership is a trustworthy identity in that scenario.

The attacker still cannot normally complete Android's ADB authentication
without an authorized controller private key. This preserves control
authorization, but not perfect network isolation. Remaining risks include:

- denial of service and route disruption;
- scanning and traffic delivery to TCP 5555;
- exploitation of a remotely reachable adbd vulnerability before ADB
  authentication; and
- compromise of an already authorized iOS controller.

Keep Android and adbd patched, expose only TCP 5555 to selected `/32`
destinations, and remove stale mappings promptly.

## Firewall invariants

```text
public/LAN -> host management        DENY except explicitly managed SSH path
public/LAN -> Android guest subnet   DENY
router tailscale0 -> VM subnet       DENY by default
mapped source -> selected ADB        ALLOW TCP 5555
VM -> host management plane          DENY
VM -> other VM                       DENY
VM -> internet                       ALLOW through controlled NAT
```

Rules bind explicit interfaces, addresses, protocols, ports, and destinations.
No wildcard forwarding or subnet-wide advertisement is allowed.

## Key management and revocation

For ADB, generate one key pair per physical iOS device, confirm its public-key
fingerprint during enrollment, and never ingest the private key into the
manager. Revocation removes the router mapping, terminates existing ADB
sessions, removes the public key from every guest, and records an audit event.
Removing the Tailscale machine is useful cleanup but does not replace ADB-key
revocation.

For SSH, use unique operator keys, verify the host key, disable password login,
and restrict administrative accounts and `sudo`. A lost management device
requires removal of its SSH public key independently of its ADB and Tailscale
state.

Sensitive data includes VM disks and snapshots, ADB permission mappings, SSH
authorized keys, future Manager server keys, and audit records. Encrypt backups
and keep any future offline CA key outside the host backup set.

At minimum, audit device enrollment and revocation, ADB fingerprint changes,
permission changes, VM lifecycle and snapshot actions, router mapping changes,
forced ADB-session termination, reconciliation changes, and security check
failures. Never record private keys, ADB payloads, clipboard data, or screens.
