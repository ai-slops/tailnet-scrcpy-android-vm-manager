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
- ordinary Tailscale auth-key enrollment followed by manual Tailnet Lock
  signing; and
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

- `router.access[].sources`: every approved iPhone/iPad Tailscale IPv4 address;
- `router.access[].guest`: the assigned persistent Android address;
- `android_vms[].base_image`: an absolute immutable qcow2 path;
- `android_vms[].adb_public_key_files`: exported Scrcpy Remote public keys; and
- storage paths, CPUs, and memory.

Validate all three generated libvirt artifacts together:

~~~shell
just libvirt-xml-check android-game-01 .local/phone/config.toml
~~~

Several controller addresses can control the same VM:

~~~toml
[[router.access]]
sources = ["100.64.0.2", "100.64.0.3"]
guest = "10.80.0.2"
~~~

## 2. Define the isolated guest network

Inspect and validate the generated XML before defining it:

~~~shell
just guest-network-xml .local/phone/config.toml >.local/phone/guest-network.xml
virt-xml-validate .local/phone/guest-network.xml
sudo virsh net-define .local/phone/guest-network.xml
sudo virsh net-autostart tailnet-android-guest
sudo virsh net-start tailnet-android-guest
~~~

The network XML intentionally has no `<ip>` or `<forward>` element. The host
therefore receives no guest-subnet address, DHCP service, or forwarding role.
Android interfaces use libvirt port isolation, so they can reach the
non-isolated router port but cannot exchange layer-2 traffic with one another.

## 3. Prepare the router VM

Use a small persistent Ubuntu VM image with Tailscale, nftables, dnsmasq,
netplan, and `routerctl` installed. Put its disk at
`storage.vm_dir/tailnet-router.qcow2`, then generate and define the domain:

~~~shell
just router-xml .local/phone/config.toml >.local/phone/router.xml
virt-xml-validate .local/phone/router.xml
sudo virsh define .local/phone/router.xml
sudo virsh autostart tailnet-android-router
sudo virsh start tailnet-android-router
~~~

Copy the config and `routerctl` into the router. Inside the router, render and
install the deterministic network services:

~~~shell
routerctl netplan-print >/etc/netplan/60-tailnet-android.yaml
chmod 0600 /etc/netplan/60-tailnet-android.yaml
netplan apply
routerctl dnsmasq-print >/etc/dnsmasq.d/tailnet-android.conf
systemctl restart dnsmasq
sysctl -w net.ipv4.ip_forward=1
routerctl firewall-apply
~~~

Make IPv4 forwarding persistent in `/etc/sysctl.d/`. The uplink obtains an
address from the configured libvirt NAT network; the guest NIC owns
`router.lan_address` and serves only static leases declared in `android_vms`.

Enroll with an ordinary one-off Tailscale auth key:

~~~shell
routerctl enroll
tailscale lock status
~~~

Run the displayed signing command on a trusted Tailnet Lock signing node, then
approve only the advertised Android `/32` routes. Do not use a wrapped/signed
auth key. Reapply `routerctl firewall-apply` whenever source mappings change.

## 4. Create and bootstrap Android

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

## 5. Admit Scrcpy Remote's Tailscale node and connect

Current Scrcpy Remote builds contain an embedded `tsnet.Server` and TCP
forwarder. When the app's built-in Tailscale mode is enabled, it is a distinct
tailnet machine with its own persistent state and Tailscale addresses. It is
not the same node or source address as the official Tailscale iOS VPN app.

Use an ordinary, non-ephemeral auth key for the first manual enrollment. Avoid
giving the app a broad OAuth client secret merely to automate key creation.
After Scrcpy Remote joins, find its machine in the Tailscale admin console. It
should be locked out; manually sign that exact node from a trusted Tailnet Lock
signing node. Record the embedded tsnet node's IPv4 and add it to the applicable
`sources` array, then reapply the router firewall.

If instead Scrcpy Remote opens a normal socket through the system Tailscale VPN,
the source is the official Tailscale iOS app's node. Treat these as two separate
modes and confirm the observed source with router counters or logs before
finalizing the allowlist. Do not allow both addresses unless both paths were
deliberately enrolled, signed, and tested.

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
dnsmasq, and Tailnet Lock.

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
   IP; clearing app data creates a new locked-out node that must be signed again.

Do not expose the SPICE listener beyond localhost, advertise the entire guest
subnet, disable `ro.adb.secure`, or place the KVM host in the tailnet.
