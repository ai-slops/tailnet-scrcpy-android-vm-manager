# Tailnet Router VM Provisioning

The KVM host must not run `tailscaled`. A dedicated, persistent Linux router VM
is the only node that joins the tailnet. It has one virtual NIC on the isolated
Android bridge and obtains its Tailnet interface from `tailscaled` inside the
VM.

The router advertises one `/32` route for each configured Android VM. It never
advertises the whole Android subnet. Router-local nftables rules permit only a
configured controller Tailscale IPv4 address to reach its assigned Android
address on ADB TCP 5555. Established reply traffic is allowed; all other
Tailnet-to-Android forwarding is dropped.

## Router VM baseline

Create a small persistent Linux VM with approximately 1 vCPU, 512 MiB RAM, and
an encrypted 4 GiB system disk. Attach only one virtual NIC to the private
`vmbr-android` bridge. Do not attach a host bridge, macvtap interface, or public
interface. The private bridge's controlled NAT path supplies outbound Internet
access for Tailscale coordination and DERP when needed.

Inside the router VM:

1. assign `router.lan_address` (the example uses `10.80.0.1`) to
   `router.guest_interface`;
2. install Tailscale, nftables, and the `routerctl` binary;
3. install the project config at
   `/etc/tailnet-android-vm-manager/config.toml`;
4. enable IPv4 forwarding with `net.ipv4.ip_forward=1`; and
5. keep every Android VM on the same isolated subnet, using the router address
   as its gateway when return routing requires it.

The manager will automate creation of this VM when general VM lifecycle support
lands. The current `routerctl` is the guest-side provisioning and enforcement
component and can be used with a manually created baseline VM now.

## Auth key and enrollment

Create an ordinary, preferably one-off and non-ephemeral, auth key. Do not sign
or wrap the auth key with Tailnet Lock. Store it only inside the router VM as a
regular file with mode `0600`:

~~~shell
sudo install -d -m 0700 /etc/tailnet-android-vm-manager/secrets
sudo tee /etc/tailnet-android-vm-manager/secrets/tailscale-authkey >/dev/null
sudo chmod 0600 /etc/tailnet-android-vm-manager/secrets/tailscale-authkey
~~~

Paste the key into `tee`, press Enter, and then Ctrl-D. `routerctl` passes only
the `file:` path to the Tailscale CLI, so the secret is not placed in argv.

Enroll from inside the router VM:

~~~shell
sudo routerctl enroll
~~~

The command disables accepted routes, exit-node service, Tailscale SSH, DNS
acceptance, and posture reporting. It advertises only the deduplicated Android
guest `/32` routes derived from `[[router.access]]`. If already connected, it
does not access or reuse the auth key.

## Manual Tailnet Lock signature and route approval

The joined router is expected to remain locked out. Run this inside it:

~~~shell
tailscale lock status
~~~

Run the displayed signing command on a separate trusted signing node. The
router VM, KVM host, Android guests, and ordinary iOS controllers must not be
signing nodes.

Approve the advertised `/32` routes in the Tailscale admin console. Route
approval and Grants are defense in depth; router-local nftables remains the
mandatory enforcement point.

## Forwarding policy

Inspect and install the policy inside the router VM:

~~~shell
routerctl firewall-print
sudo routerctl firewall-apply
sudo routerctl preflight
~~~

Run `firewall-apply` during router boot before accepting controller traffic and
after every access mapping change. The command owns only the
`inet tailnet_android_router` table and replaces that table atomically.

Scrcpy Remote connects directly to the persistent Android address and TCP port
5555, for example `10.80.0.2:5555`. Tailnet Lock admits the controller and
router keys, route approval makes the `/32` reachable, router nftables checks
the controller IP mapping, and Android ADB verifies the controller's private
key.

## Revocation

1. Remove the controller-to-guest entry from `[[router.access]]`.
2. Reapply router nftables and terminate existing forwarding state.
3. Remove the controller ADB public key from the Android VM.
4. Remove the controller from Tailscale and revoke its Tailnet Lock key.

Tailnet Lock, router forwarding, and ADB authorization are separate boundaries;
revocation must update all three.
