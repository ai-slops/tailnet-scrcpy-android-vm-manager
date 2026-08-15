# Scrcpy Remote Compatibility Spike

This procedure validates the assumptions that block the rest of the MVP. It is
not a production deployment procedure.

## Prerequisites

- A dedicated Tailnet router VM enrolled in the locked tailnet
- One Android VM on the configured private guest subnet
- Authenticated ADB listening on the VM private TCP port 5555
- An iPhone or iPad with the current Scrcpy Remote release
- A unique ADB key pair generated or imported by that iOS device
- A config copied from config.example.toml with controller-to-guest mappings

Do not install Tailscale on the KVM host. The isolated router VM must advertise
only the test Android `/32` and forward only mapped controller traffic to ADB.

## 1. Record versions

Record the date and versions of Scrcpy Remote, iOS or iPadOS, both Tailscale
clients, Android, and the host distribution.

## 2. Validate the iOS ADB public key

Export the ADB public key from Scrcpy Remote. Never copy its private key to the
host or repository.

~~~shell
cargo run -p hostctl -- \
  adb-fingerprint /path/to/ios-device.adb.pub
~~~

Record the fingerprint. The command rejects private-key material, multiline
input, malformed base64, and unexpected key sizes. Install only this public key
in the test VM using the image-specific bootstrap mechanism. ADR-P04 remains
open until that mechanism is selected.

## 3. Check the host

~~~shell
cargo run -p hostctl -- \
  --config .local/spike/config.toml \
  preflight
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
4. Stop the gateway. Its endpoint must close immediately.
5. Let a short lease expire while connected. The connection must close.
6. Try an out-of-range listen port, a guest outside the configured subnet, and a
   non-5555 destination. Startup must fail before accepting traffic.

## 7. Record the result

Store non-secret results under docs/spikes/ in a dated Markdown file. Do not
commit private keys, auth keys, VM disks, or payload packet captures.

The result must resolve or refine ADR-P01 through ADR-P05. Failure of arbitrary
port support, stable per-device ADB keys, independent key rejection, or
revocation is an architecture blocker. It must not be bypassed by exposing
unauthenticated ADB.
