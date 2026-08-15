# Development Host Setup

The compatibility spike requires QEMU/KVM and uses rootless Podman for
reproducible host-side tooling. Production packaging may later remove the
container-engine dependency.

## Ubuntu packages

Install distribution packages so AppArmor profiles and helper binaries use
their expected system paths:

~~~shell
sudo apt-get update
sudo apt-get install --yes \
  podman \
  qemu-system-x86 \
  qemu-utils \
  libvirt-clients \
  libvirt-daemon-system \
  nftables \
  uidmap \
  passt \
  slirp4netns \
  fuse-overlayfs
~~~

The optional Android Emulator spike also needs the emulator's XKB runtime
library, even when it runs without a window:

~~~shell
sudo apt-get install --yes libxkbfile1
~~~

Do not use a Podman binary unpacked beneath the home directory on Ubuntu systems
with AppArmor unprivileged-user-namespace restrictions. Applications that need
unprivileged namespaces must be explicitly allowed by an AppArmor profile; the
distribution-installed binary has the stable path expected by host policy.

Do not disable kernel.apparmor_restrict_unprivileged_userns as a workaround.

## Group activation

Add the account once if it is not already listed:

~~~shell
sudo usermod --append --groups kvm,libvirt USER
~~~

A new login session is the least surprising way to activate group membership.
For a temporary shell, newgrp kvm is sufficient for the KVM smoke test. Running
newgrp commands sequentially does not reliably preserve both nested group
contexts, so prefer logging out and back in.

Docker is not required by the project. If it is used as a temporary fallback,
its docker group effectively grants root-equivalent control of the daemon and
must not be treated as equivalent to rootless Podman.

## Rootless prerequisites

Verify that the account has subordinate UID and GID ranges:

~~~shell
grep "^$USER:" /etc/subuid
grep "^$USER:" /etc/subgid
~~~

Both entries should normally provide at least 65536 IDs. The Ubuntu uidmap
package provides newuidmap and newgidmap.

## Smoke test

After starting a fresh login session:

~~~shell
sh scripts/host-smoke.sh
~~~

The script proves that QEMU can initialize KVM, rather than merely checking that
/dev/kvm exists. It also runs a real container using rootless Podman.

Then run the project-specific checks:

~~~shell
cargo run -p hostctl -- \
  --config .local/spike/config.toml \
  preflight
~~~

The hostctl check additionally validates QEMU, libvirt, nftables, cgroup v2,
and current-process KVM device access. It deliberately does not require
Tailscale on the KVM host.

Before this check can pass on a deployment host, enroll and manually sign the
host and install the controller source-IP allowlist as described in
[Tailscale Provisioning](tailscale-provisioning.md).

## Optional Android Emulator spike

The official Android Emulator can validate KVM-backed Android boot and local ADB
before the production libvirt image is available. It does not reproduce the
final isolated libvirt network, so it cannot by itself complete the Scrcpy
Remote endpoint test.

The bootstrap script pins the command-line tools archive and verifies its
published SHA-256 checksum. Run each step explicitly:

~~~shell
sh scripts/android-spike-sdk.sh prepare
sh scripts/android-spike-sdk.sh licenses
sh scripts/android-spike-sdk.sh install
sh scripts/android-spike-sdk.sh create
newgrp kvm
sh scripts/android-spike-sdk.sh check
sh scripts/android-spike-sdk.sh start
~~~

The licenses step displays Google's Android SDK terms and requires the operator
to accept or reject them interactively. The project does not auto-accept legal
terms.

With the current API 36 default system image, `avdmanager` may print that the
image's optional `devices.xml` could not be loaded. The selected `pixel_8`
profile is bundled with the command-line tools, so this message is non-fatal if
the script subsequently reports that it created the AVD. The script verifies
that the expected `config.ini` exists instead of relying on the message text.

The downloaded SDK, system image, AVD, and VM data remain below .local/ and are
not committed. See the [official SDK manager documentation](https://developer.android.com/tools/sdkmanager)
and [emulator acceleration documentation](https://developer.android.com/studio/run/emulator-acceleration).
