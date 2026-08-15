# Development Host Setup

Host development requires QEMU/KVM and libvirt. Rootless Podman is used only by
the host smoke test. Docker Compose is used only by the disposable local
Headscale network integration test; neither engine is part of the production
router or Android VM data path.

Rust 1.97, Zig 0.15, and just are the core development tools. Mise is an
optional way to install the pinned versions from `mise.toml`; it is not required
to run any recipe. A standalone toolchain works because the `justfile` and
Cargo linker wrapper default their caches to `.local/`. Java 21 is needed only
for the optional Android Emulator spike.

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

Docker is optional unless running the local Headscale integration test. Its
`docker` group effectively grants root-equivalent control of the daemon and
must not be treated as equivalent to rootless Podman. See
[Local Network Integration Test](integration-testing.md).

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
just host-smoke
~~~

The script proves that QEMU can initialize KVM, rather than merely checking that
/dev/kvm exists. It also runs a real container using rootless Podman.

Then run the project-specific checks:

~~~shell
just preflight .local/spike/config.toml
~~~

## Disposable nested Ubuntu validation

On an Ubuntu KVM host with nested virtualization enabled, validate a clean clone
in a disposable Ubuntu 24.04 VM:

~~~shell
just nested-ubuntu-test
~~~

The recipe boots a four-vCPU cloud VM with host CPU passthrough, clones the
current origin and checks out the exact current commit, installs the pinned mise
tools, and runs `just check`, `just router-provision-test`, the real nested-KVM
and rootless-Podman smoke test, and the Docker Headscale integration test. The
VM and its runtime artifacts are removed on exit; the downloaded base image is
cached in `/var/tmp`. Set `KEEP_NESTED_VM=1` to retain a failed VM for
inspection. The current commit must exist on the remote;
set `NESTED_TEST_REPO_URL` and `NESTED_TEST_REPO_REF` to test another source.

The hostctl check additionally validates QEMU, libvirt, nftables, cgroup v2,
and current-process KVM device access. It deliberately does not require
Tailscale on the KVM host.

This check deliberately does not inspect Tailnet Lock or router policy because
the KVM host is not a tailnet node. Run `routerctl preflight` inside the router
appliance after enrollment and manual signing, as described in
[Tailscale Provisioning](tailscale-provisioning.md).

## Optional Android Emulator spike

The official Android Emulator can validate KVM-backed Android boot and local ADB
before the production libvirt image is available. It does not reproduce the
final isolated libvirt network, so it cannot by itself complete the Scrcpy
Remote endpoint test.

The bootstrap script pins the command-line tools archive and verifies its
published SHA-256 checksum. Run each step explicitly:

~~~shell
just android-spike prepare
just android-spike licenses
just android-spike install
just android-spike create
newgrp kvm
just android-spike check
just android-spike start
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
