# Scrcpy Remote Compatibility Spike

This procedure validates the assumptions that block the rest of the MVP. It is
not a production deployment procedure.

## Prerequisites

- An Ubuntu LTS KVM host enrolled in the dedicated locked tailnet
- One Android VM on the configured private guest subnet
- Authenticated ADB listening on the VM private TCP port 5555
- An iPhone or iPad with the current Scrcpy Remote release
- A unique ADB key pair generated or imported by that iOS device
- A config copied from config.example.toml with the actual host Tailscale address

Do not expose the endpoint range on a public or LAN interface. The gateway binds
only the exact configured Tailscale address, but the host firewall must also
deny the range on every other interface.

## 1. Record versions

Record the date and versions of Scrcpy Remote, iOS or iPadOS, both Tailscale
clients, Android, and the host distribution.

## 2. Validate the iOS ADB public key

Export the ADB public key from Scrcpy Remote. Never copy its private key to the
host or repository.

~~~shell
mise exec -- cargo run -p hostctl -- \
  adb-fingerprint /path/to/ios-device.adb.pub
~~~

Record the fingerprint. The command rejects private-key material, multiline
input, malformed base64, and unexpected key sizes. Install only this public key
in the test VM using the image-specific bootstrap mechanism. ADR-P04 remains
open until that mechanism is selected.

## 3. Check the host

~~~shell
mise exec -- cargo run -p hostctl -- \
  --config .local/spike/config.toml \
  preflight
~~~

Every check must pass on the actual KVM host. Keep local configuration and test
artifacts below .local/, which Git ignores.

## 4. Start a bounded endpoint

Choose an unused port inside the configured range. The guest must be inside
network.guest_subnet and must use ADB port 5555.

~~~shell
mise exec -- cargo run -p endpoint-gateway -- \
  --config .local/spike/config.toml \
  --listen-port 31000 \
  --guest 10.80.0.2:5555 \
  --lease-seconds 900
~~~

The process binds only to host.tailnet_address, rejects destinations outside the
guest subnet or port 5555, limits concurrent connections, and closes active
connections at lease expiry or Ctrl-C.

This is intentionally a manual spike. It does not persist leases, consult the
future authorization database, or install firewall rules.

## 5. Connect Scrcpy Remote

Configure Scrcpy Remote with the host Tailscale address, selected endpoint port,
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
