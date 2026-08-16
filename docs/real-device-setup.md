# Real iPhone Connection Runbook

This runbook produces the first manually operated end-to-end connection from
Scrcpy Remote on iOS to a persistent Android VM. It deliberately does not wait
for the future manager API or authorization database.

## What the repository now provides

- a host-address-free, isolated libvirt guest network;
- a two-NIC router domain with separate NAT uplink and guest interfaces;
- deterministic Android MAC addresses and router-served static DHCP leases;
- persistent qcow2 overlays and libvirt Android domains;
- localhost-only SPICE graphics for initial Android setup and ADB approval;
- typed libnftables JSON with a `(controller source, Android guest)` set;
- guest Internet NAT, default-deny forwarding, and TCP 5555-only tailnet access;
- ordinary Tailscale auth-key enrollment; and
- validation and rendering of per-VM ADB public-key bundles.

The repository does not redistribute an Android image. Supply a legally
obtained, bootable x86-64 Android qcow2 base image that supports VirtIO disk,
VirtIO networking, VirtIO graphics, DHCP, authenticated ADB, and the games you
intend to run. Store the immutable base at the configured `base_image` path.
This remains an explicit operator-supplied artifact because neither Google Play
images nor third-party Android distributions may be silently redistributed by
this project.

## 1. Create local configuration

Copy `config.example.toml` below `.local/` and edit:

- `controllers.NAME.sources`: each Scrcpy Remote Tailscale IPv4 address;
- `controllers.NAME.adb_public_key_file`: that controller's exported public key;
- `vms.NAME.base_image`: an absolute immutable qcow2 path;
- optional `vms.NAME.controllers`: controller names allowed for that VM; and
- storage paths, CPUs, and memory.

Validate all three generated libvirt artifacts together:

~~~shell
just libvirt-xml-check android-game-01 .local/phone/config.toml
~~~

Several observed addresses can belong to one controller. Omitting `controllers`
from a VM allows every active controller:

~~~toml
[controllers.my-iphone]
sources = ["100.64.0.2", "100.64.0.3"]
adb_public_key_file = "/etc/tailnet-android-vm-manager/adb-keys/my-iphone.pub"

[vms.android-game-01]
address = "10.80.0.2"
base_image = "/var/lib/tailnet-android-vm-manager/images/android-base.qcow2"
~~~

## 2. Provision the isolated network and router VM

Install `cloud-image-utils` (for `cloud-localds`) and provide the SSH public
key that may log in as the cloud image's `ubuntu` account:

~~~shell
sudo apt install cloud-image-utils qemu-utils libvirt-clients
ROUTER_SSH_PUBLIC_KEY_FILE=/path/to/id_ed25519.pub \
  just router-provision .local/phone/config.toml
~~~

The recipe downloads and caches the official Ubuntu 24.04 cloud image, creates
an 8 GiB qcow2 overlay and NoCloud seed, embeds the locally built `routerctl`
and configuration, defines/starts the isolated network, and defines/starts the
router domain. It refuses to overwrite either router artifact. Set
`ROUTER_IMAGE_URL` only for a reviewed mirror or pinned image.

Cloud-init installs Tailscale, nftables, and dnsmasq and applies netplan,
forwarding, static guest leases, and the default-deny firewall. Inspect progress
with `just router-console` and exit with `Ctrl+]`. Find its uplink address with
`virsh domifaddr tailnet-android-router --source lease`, then SSH as `ubuntu`.

The seed contains the public SSH key, but no private key or Tailscale auth key.
Inside the router, install an ordinary one-off auth key at the configured path
and enroll:

~~~shell
sudo install -d -m 0700 /etc/tailnet-android-vm-manager/secrets
sudo install -m 0600 /dev/stdin /etc/tailnet-android-vm-manager/secrets/tailscale-authkey
sudo routerctl enroll
~~~

Approve only the advertised Android `/32` routes. Reapply `routerctl
firewall-apply` whenever source mappings change.

## 3. Create and bootstrap Android

Create the persistent overlay and domain:

~~~shell
just android-create android-game-01 .local/phone/config.toml
sudo virsh start android-game-01
~~~

`android-create` refuses an existing disk, verifies that the immutable base is
qcow2, creates the overlay under a temporary name, publishes it without
overwriting another file, and rolls it back if `virsh define` fails.

Open the localhost-only SPICE console through virt-manager or virt-viewer.
Complete Android's initial setup, confirm that DHCP assigned the configured
address, and enable authenticated ADB over TCP 5555. The exact persistent ADB
switch is image-specific; verify after a reboot that TCP 5555 is listening and
that `ro.adb.secure=1` remains enabled. Never disable ADB authentication.

Scrcpy Remote supports generating, importing, and exporting ADB keys. Configure
the app to use a unique key for this physical iOS device. Prefer installing its
exported public key during image provisioning. If the image cannot consume the
rendered bundle directly, make the first connection while viewing the local
SPICE console and approve that key once in Android. Selecting “always allow”
stores the public key in persistent userdata; reboot and verify that it remains
authorized.

The validated bundle is available with:

~~~shell
just android-adb-keys android-game-01 .local/phone/config.toml
~~~

## Controller replacement

Add the replacement as a second controller before revoking the old one:

~~~toml
[controllers.old-iphone]
sources = ["100.64.0.2"]
adb_public_key_file = "/etc/tailnet-android-vm-manager/adb-keys/old-iphone.pub"
active = true

[controllers.new-iphone]
sources = ["100.64.0.9"]
adb_public_key_file = "/etc/tailnet-android-vm-manager/adb-keys/new-iphone.pub"
active = true
~~~

Reconcile the router, render each affected VM's desired key bundle, apply it by
the Android image's console/bootstrap procedure, and test the new controller.
Then set `old-iphone.active = false`, reconcile again, apply the now-reduced key
bundle, restart `adbd`, and terminate existing ADB connections. An inactive
controller is retained as inventory but contributes neither firewall flows nor
ADB keys. A configuration with no active controllers is valid and fail-closed.

The manager currently renders desired keys but cannot claim live guest
application. Automated removal outside the guest requires the planned
read-only key configuration artifact and Android init service described in the
[architecture](architecture.md). Until the selected base image implements that
contract, key installation and removal remain an explicit image-specific step.

## 4. Admit Scrcpy Remote's Tailscale node and connect

Current Scrcpy Remote builds contain an embedded `tsnet.Server` and TCP
forwarder. When the app's built-in Tailscale mode is enabled, it is a distinct
tailnet machine with its own persistent state and Tailscale addresses. It is
not the same node or source address as the official Tailscale iOS VPN app.

Use an ordinary, non-ephemeral auth key for the first manual enrollment. Avoid
giving the app a broad OAuth client secret merely to automate key creation.
After Scrcpy Remote joins, find its machine in the Tailscale admin console.
Record the embedded tsnet node's IPv4 and add it to that controller's `sources`
array, then reconcile the router firewall.

If instead Scrcpy Remote opens a normal socket through the system Tailscale VPN,
the source is the official Tailscale iOS app's node. Treat these as two separate
modes and confirm the observed source with router counters or logs before
finalizing the allowlist. Do not allow both addresses unless both paths were
deliberately enrolled and tested.

In Scrcpy Remote choose ADB mode and connect to the Android address, not the
router or KVM host:

~~~text
Host: 10.80.0.2
Port: 5555
~~~

The equivalent URL scheme is:

~~~text
scrcpy2://10.80.0.2:5555?bit-rate=4M&max-size=1080
~~~

Run `routerctl preflight` before testing. It now checks forwarding, both router
interfaces, the guest address, `tailscale0`, the installed nftables table,
and dnsmasq.

## Acceptance checks

The first setup is complete only when all of these pass:

1. The approved Scrcpy Remote tsnet node controls the VM over mobile data and
   another Wi-Fi.
2. A second enrolled but unlisted tailnet node cannot open `10.80.0.2:5555`.
3. A wrong ADB key is rejected even from an allowed source address.
4. Removing one source from the set and reapplying nftables blocks it.
5. Android reboot, VM stop/start, and host restart retain address and ADB key.
6. Hibernation restores the running game without consuming host RAM while off.
7. A stopped-state snapshot can be created and reverted safely.
8. Reopening Scrcpy Remote reuses the same embedded machine identity and source
   IP; clearing app data creates a new node whose address must be allowed again.

Do not expose the SPICE listener beyond localhost, advertise the entire guest
subnet, disable `ro.adb.secure`, or place the KVM host in the tailnet.
