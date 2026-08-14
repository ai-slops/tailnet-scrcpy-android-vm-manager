#!/bin/sh
set -eu

sdk_root="${ANDROID_SDK_ROOT:-$PWD/.local/android-sdk}"
tools_revision="15859902"
tools_archive="commandlinetools-linux-${tools_revision}_latest.zip"
tools_sha256="4e4c464f145a7512b57d088ac6c278c03c9eea610886b35a5e0804e74eedf583"
tools_url="https://dl.google.com/android/repository/$tools_archive"
system_image="system-images;android-36;default;x86_64"
avd_name="tailnet_android_api36"

sdkmanager="$sdk_root/cmdline-tools/latest/bin/sdkmanager"
avdmanager="$sdk_root/cmdline-tools/latest/bin/avdmanager"
emulator="$sdk_root/emulator/emulator"

prepare_tools() {
  [ ! -x "$sdkmanager" ] || return 0
  mkdir -p "$sdk_root/cmdline-tools" "$PWD/.local/downloads"
  archive="$PWD/.local/downloads/$tools_archive"
  curl -fL "$tools_url" -o "$archive"
  printf '%s  %s\n' "$tools_sha256" "$archive" | sha256sum --check
  unpack="$sdk_root/cmdline-tools/unpacked"
  rm -rf "$unpack"
  mkdir -p "$unpack"
  (
    cd "$unpack"
    jar xf "$archive"
  )
  mv "$unpack/cmdline-tools" "$sdk_root/cmdline-tools/latest"
  rmdir "$unpack"
  chmod +x "$sdk_root/cmdline-tools/latest/bin/"*
}

case "${1:-}" in
  prepare)
    prepare_tools
    "$sdkmanager" --version
    ;;
  licenses)
    prepare_tools
    "$sdkmanager" --licenses
    ;;
  install)
    prepare_tools
    "$sdkmanager" "platform-tools" "emulator" "$system_image"
    ;;
  create)
    [ -x "$avdmanager" ] || {
      printf 'Run install first.\n' >&2
      exit 1
    }
    printf 'no\n' |
      "$avdmanager" create avd --force --name "$avd_name" \
        --package "$system_image" --device pixel_8
    ;;
  check)
    [ -x "$emulator" ] || {
      printf 'Run install first.\n' >&2
      exit 1
    }
    "$emulator" -accel-check
    ;;
  start)
    [ -x "$emulator" ] || {
      printf 'Run install first.\n' >&2
      exit 1
    }
    exec "$emulator" "@$avd_name" -accel on -no-window -no-audio \
      -no-boot-anim -gpu swiftshader
    ;;
  *)
    printf 'Usage: %s {prepare|licenses|install|create|check|start}\n' "$0" >&2
    exit 2
    ;;
esac
