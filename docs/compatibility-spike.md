# Scrcpy Remote Compatibility Spike

This procedure validates the assumptions that block the rest of the MVP. It is
not a production deployment procedure.

## Prerequisites

- A dedicated Tailnet router VM enrolled in the tailnet
- One Android VM on the configured private guest subnet
- Authenticated ADB listening on the VM private TCP port 5555
- An iPhone or iPad with the current Scrcpy Remote release
- A unique ADB key pair generated or imported by that iOS device
- An ignored `.local/config.toml` created from the tracked example

Do not install Tailscale on the KVM host. The isolated router VM must advertise
only the test Android `/32` and forward only mapped controller traffic to ADB.

## 1. Record versions

Record the date and versions of Scrcpy Remote, iOS or iPadOS, both Tailscale
clients, Android, and the host distribution.

Also record whether Scrcpy Remote uses its embedded tsnet forwarder or the iOS
system Tailscale VPN. In embedded mode, record the distinct Scrcpy Remote
machine shown in the Tailscale admin console; its IPv4 is the router allowlist
source.

## 2. Validate the iOS ADB public key

Export the ADB public key from Scrcpy Remote. Never copy its private key to the
host or repository.

~~~shell
just setup adb-fingerprint /path/to/ios-device.adb.pub
~~~

Record the fingerprint. The command rejects private-key material, multiline
input, malformed base64, and unexpected key sizes. Install only this public key
in the test VM using the image-specific bootstrap mechanism. ADR-P03 remains
open until that mechanism is selected.

## 3. Check the host

~~~shell
MANAGER_CONFIG=.local/spike/config.toml just setup preflight
~~~

Every check must pass on the actual KVM host. Keep local configuration and test
artifacts below .local/, which Git ignores.

## 4. Configure the routed endpoint

Apply the router policy inside the router VM:

~~~shell
sudo routerctl firewall-apply
~~~

Approve only the test guest `/32` route in the tailnet. This is intentionally a
manual spike and does not yet reconcile route approval automatically.

## 5. Connect Scrcpy Remote

Configure Scrcpy Remote with the routed Android address and TCP port 5555,
and the unique ADB key whose public fingerprint was recorded. Record whether the
app accepts an arbitrary port and speaks directly to adbd. Verify video, touch,
keyboard, reconnect behavior, and observed scrcpy versions.

## 6. Negative and revocation tests

1. Replace the authorized ADB key with an unrelated public key. Connection must
   fail.
2. Restore the approved key. Connection must succeed.
3. Remove the approved key while connected and restart adbd using the
   image-specific control path. The active or next connection must fail.
4. Remove the controller mapping and reapply router policy. The established
   connection must stop carrying traffic immediately.
5. Restore the mapping. A new authenticated connection must succeed.
6. Try a controller source outside `100.64.0.0/10`, a guest outside the
   configured subnet, an uninventoried guest, and a non-5555 destination. The
   configuration or router policy must reject each case.

## 7. Record the result

Store non-secret results under docs/spikes/ in a dated Markdown file. Do not
commit private keys, auth keys, VM disks, or payload packet captures.

The result must resolve or refine ADR-P01 through ADR-P04. Failure of direct
routed-address support, stable per-device ADB keys, independent key rejection,
or revocation is an architecture blocker. It must not be bypassed by exposing
unauthenticated ADB.
