# Persistent VM Lifecycle and Snapshots

Every Android instance is persistent and must be declared in `[[android_vms]]`.
The VM name is also its fixed libvirt domain name, and its fixed address must be
inside `network.guest_subnet`. Router access entries may refer only to addresses
in this inventory.

Each entry names an immutable qcow2 `base_image`. Creation verifies that format,
creates a persistent overlay without overwriting an existing file, and defines
a libvirt domain on the isolated guest network:

~~~shell
just android-xml android-game-01 .local/phone/config.toml
just android-create android-game-01 .local/phone/config.toml
~~~

The VM's MAC is derived from its configured address. Router dnsmasq therefore
returns the same address after VM, router, or host restarts without putting a
DHCP or guest-subnet address on the KVM host.

The Tailnet router advertises an inventoried VM's `/32` independently of its
power state. Turning a game VM on or off therefore does not require a new route
approval or firewall update. A connection to a stopped VM simply times out.

## States

`hostctl` exposes three stable states:

- `Running`: the VM is consuming its configured RAM and vCPUs.
- `Stopped`: no RAM state exists; the next start performs a normal boot.
- `Hibernated`: libvirt managed save has written RAM and device state to host
  storage and stopped QEMU. The next start automatically restores that state.

`Hibernated` is intentionally distinct from libvirt `suspend`, which pauses a
domain while retaining host RAM. This project does not expose RAM-resident
suspend in the MVP.

The managed-save image can be approximately the configured guest RAM size and
must be stored on SSD-backed libvirt storage with sufficient free space. It is
not a durable backup and may become unusable across incompatible QEMU, machine
type, CPU, firmware, or device-model changes.

## Commands

Inspect and start a configured VM:

~~~shell
hostctl vm status android-game-01
hostctl vm start android-game-01 --wait-ready-seconds 120
~~~

The optional readiness wait succeeds when the host can establish TCP to the
VM's fixed address on port 5555. It proves network/adbd availability, not ADB
key authorization or scrcpy compatibility.

Request an ACPI shutdown and wait up to 30 seconds:

~~~shell
hostctl vm stop android-game-01
~~~

The default never forces power loss. If the operator explicitly accepts a
crash-consistent shutdown after the timeout:

~~~shell
hostctl vm stop android-game-01 \
  --timeout-seconds 60 \
  --force-after-timeout
~~~

Write RAM state to SSD and stop QEMU:

~~~shell
hostctl vm hibernate android-game-01
hostctl vm start android-game-01 --wait-ready-seconds 120
~~~

Calling `vm stop` on a hibernated VM removes its managed-save image and changes
the VM to `Stopped`; the next start performs a clean boot.

## Snapshots

Snapshots are accepted only while the VM is `Stopped`. Stop Android cleanly
before creating one:

~~~shell
hostctl vm stop android-game-01
hostctl vm snapshot android-game-01 create before-game-update
hostctl vm snapshot android-game-01 list
hostctl vm snapshot android-game-01 revert before-game-update
hostctl vm snapshot android-game-01 delete before-game-update
~~~

Snapshots are rejected while `Running` because a live disk snapshot without
Android filesystem quiescing would normally be only crash-consistent. They are
also rejected while `Hibernated`: reverting disk state while retaining a newer
managed-save image would create an invalid RAM/disk combination. Run `vm stop`
first to discard saved RAM.

The initial implementation uses libvirt-managed internal qcow2 snapshots.
Snapshot names are limited to 63 ASCII alphanumeric, dot, underscore, or hyphen
characters and are always passed to `virsh` as structured arguments. Snapshot
deletion is explicit; no automatic retention policy is enabled yet.

Snapshots are not backups. A damaged or lost qcow2 file can destroy both the
current state and its internal snapshots. Durable backup/export and retention
policies remain separate work.
