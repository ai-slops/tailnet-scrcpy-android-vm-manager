# Tailnet Router VM Provisioning

The KVM host must not run `tailscaled`. A dedicated, persistent Linux router VM
is the only node that joins the tailnet. It has one NAT uplink NIC and one NIC
on the host-address-free Android network, and obtains its Tailnet interface
from `tailscaled` inside the VM.

The router advertises one `/32` route for each configured Android VM. It never
advertises the whole Android subnet. Router-local nftables uses a set of
configured controller IPv4 and Android destination pairs. Only listed pairs may
reach ADB TCP 5555. Established replies and guest Internet NAT are allowed; all
other forwarding is dropped.

## Router VM provisioning

The generated persistent Linux VM has 1 vCPU, 512 MiB RAM, and an 8 GiB qcow2
overlay. Attach its uplink only to a libvirt NAT network
and its guest NIC only to `network.guest_network`. Do not attach a host LAN
bridge, macvtap interface, or public interface. The uplink supplies outbound
Internet access for Tailscale coordination, DERP, and guest NAT.

Run the reproducible host-side provisioner:

~~~shell
ROUTER_SSH_PUBLIC_KEY_FILE=/path/to/id_ed25519.pub just router-provision CONFIG
~~~

It caches the official Ubuntu 24.04 image, creates the overlay and NoCloud seed,
installs `routerctl` and its configuration, and starts both the isolated network
and two-NIC domain. Cloud-init installs and configures Tailscale, nftables,
dnsmasq, netplan, and persistent forwarding. Existing artifacts are never
overwritten. Only the public SSH key is embedded; no Tailscale secret is copied
into the VM definition or seed.

## Auth key and enrollment

Create an ordinary, preferably one-off and non-ephemeral, auth key. Store it
only inside the router VM as a regular file with mode `0600`:

~~~shell
sudo install -d -m 0700 /etc/tailnet-android-vm-manager/secrets
sudo tee /etc/tailnet-android-vm-manager/secrets/tailscale-authkey >/dev/null
sudo chmod 0600 /etc/tailnet-android-vm-manager/secrets/tailscale-authkey
~~~

Paste the key into `tee`, press Enter, and then Ctrl-D. `routerctl` passes only
the `file:` path to the Tailscale CLI, so the secret is not placed in argv.

From the KVM host, the preferred recipe waits for cloud-init, transfers the key
only over encrypted SSH stdin, installs it as mode `0600`, enrolls the router,
applies its firewall, and runs preflight:

~~~shell
chmod 0600 .local/secrets/authkey.txt
just router-enroll /path/to/router_id_ed25519
~~~

The private SSH key must match the public key supplied to `router-provision`.
The auth key is not placed in argv, cloud-init, a temporary file, or command
output. The ignored host copy remains in place for operator-controlled recovery;
delete it manually after successful enrollment if the key is one-off and no
longer needed.

The equivalent command from inside the router VM is:

~~~shell
sudo routerctl enroll
~~~

The command disables accepted routes, exit-node service, Tailscale SSH, DNS
acceptance, and posture reporting. It advertises only the deduplicated Android
guest `/32` routes derived from `[vms.NAME]`. If already connected, it
does not access or reuse the auth key.

## Route approval

Approve the advertised `/32` routes in the Tailscale admin console. Route
approval and Grants are defense in depth; router-local nftables remains the
mandatory enforcement point.

## Forwarding policy

Inspect and install the policy inside the router VM:

~~~shell
routerctl netplan-print
routerctl dnsmasq-print
routerctl firewall-print
sudo routerctl firewall-apply
sudo routerctl preflight
~~~

Run `firewall-apply` during router boot before accepting controller traffic and
after every access mapping change. The command owns only the
`inet tailnet_android_router` table and replaces that table atomically through
libnftables JSON. Controller/guest pairs use one concatenated nftables set
instead of one generated rule per controller.

For inventory changes, prefer the host-side synchronized reconciliation:

~~~shell
just reconcile-all /path/to/router_ssh_key CONFIG
~~~

It updates the router config, static leases, nftables set, and advertised `/32`
routes together. Tailscale's `set` command changes only explicitly supplied
settings and does not require the enrollment auth key.

Scrcpy Remote connects directly to the persistent Android address and TCP port
5555, for example `10.80.0.2:5555`. Route approval makes the `/32` reachable,
router nftables checks the controller IP mapping, and Android ADB verifies the
controller's private key. Only the last step authorizes Android control.

## Revocation

1. Set the controller to `active = false` or remove it from configuration.
2. Reconcile router nftables and terminate existing forwarding state.
3. Reconcile the desired ADB key bundle into each Android VM.
4. Remove the controller machine from Tailscale.

Router forwarding and ADB authorization are separate controls; revocation must
update both. Removing the Tailscale machine reduces exposure but is not a
substitute for removing its ADB key.
