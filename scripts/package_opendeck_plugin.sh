#!/bin/sh
# Build the OpenDeck device plugin and zip it for "Install from file".
# OpenDeck requires a top-level directory whose name ends in .sdPlugin.
#
# macOS and Linux both launch a binary with no extension, so they cannot share
# a filename. CodePathMac is opendeck-masterhub, a universal Mach-O (arm64 and
# x86_64). CodePathLin is opendeck-masterhub-linux. CodePathWin is
# opendeck-masterhub.exe.
#
# On macOS this script builds that universal binary. The release workflow
# builds it on a macOS runner (MAC_ONLY=1) and passes it to the Ubuntu package
# step as MAC_BIN. OpenDeck on macOS starts CodePathMac during install. A zip
# without that file fails with: No such file or directory.
set -eu

# cargo-xwin archives Windows C objects with llvm-lib. It finds that tool on
# PATH, in Homebrew LLVM, or as llvm-ar from rustup's llvm-tools component.
windows_archive_tool_ready() {
  if command -v llvm-lib >/dev/null 2>&1 || command -v llvm-ar >/dev/null 2>&1; then
    return 0
  fi
  for dir in /opt/homebrew/opt/llvm/bin /usr/local/opt/llvm/bin; do
    if [ -e "$dir/llvm-lib" ] || [ -e "$dir/llvm-ar" ]; then
      return 0
    fi
  done
  if command -v rustc >/dev/null 2>&1; then
    rustlib_bin=$(dirname "$(rustc --print target-libdir)")/bin
    if [ -e "$rustlib_bin/llvm-ar" ] || [ -e "$rustlib_bin/llvm-lib" ]; then
      return 0
    fi
  fi
  return 1
}

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
plugin_id=pl.sotomski.opendeck.openmasterhub.sdPlugin
manifest=$root/plugins/opendeck-masterhub/Cargo.toml
out_dir=$root/dist
stage=$out_dir/$plugin_id
release=$root/plugins/opendeck-masterhub/target/release
win_target=x86_64-pc-windows-msvc
win_exe=$root/plugins/opendeck-masterhub/target/$win_target/release/opendeck-masterhub.exe
linux_rust_target=x86_64-unknown-linux-gnu
# glibc 2.31 matches Ubuntu 20.04 and newer desktop distros.
linux_target=$linux_rust_target.2.31
linux_bin=$root/plugins/opendeck-masterhub/target/$linux_rust_target/release/opendeck-masterhub
mac_target_arm=aarch64-apple-darwin
mac_target_x64=x86_64-apple-darwin
host_os=$(uname -s)

if [ "${MAC_ONLY:-}" = "1" ] && [ "$host_os" != "Darwin" ]; then
  echo "MAC_ONLY=1 builds opendeck-masterhub and must run on macOS." >&2
  exit 1
fi

if [ -n "${MAC_BIN:-}" ] && [ ! -f "$MAC_BIN" ]; then
  echo "MAC_BIN does not exist: $MAC_BIN" >&2
  exit 1
fi

# Manifest MinimumVersion for mac is 11.3. rustc passes this to the linker.
build_universal_mac() {
  if ! command -v rustup >/dev/null 2>&1; then
    echo "rustup is required to install the macOS standard libraries." >&2
    echo "Install rustup from https://rustup.rs, then rerun this script." >&2
    exit 1
  fi
  if ! command -v lipo >/dev/null 2>&1; then
    echo "lipo is required to build the universal macOS plugin binary." >&2
    exit 1
  fi
  MACOSX_DEPLOYMENT_TARGET=11.3
  export MACOSX_DEPLOYMENT_TARGET
  for target in "$mac_target_arm" "$mac_target_x64"; do
    if ! rustup target list --installed | grep -qx "$target"; then
      rustup target add "$target"
    fi
    cargo build --release --target "$target" --manifest-path "$manifest"
  done
  mkdir -p "$out_dir"
  lipo -create \
    -output "$out_dir/opendeck-masterhub" \
    "$root/plugins/opendeck-masterhub/target/$mac_target_arm/release/opendeck-masterhub" \
    "$root/plugins/opendeck-masterhub/target/$mac_target_x64/release/opendeck-masterhub"
  chmod +x "$out_dir/opendeck-masterhub"
}

if [ "${MAC_ONLY:-}" = "1" ]; then
  build_universal_mac
  echo "$out_dir/opendeck-masterhub"
  exit 0
fi

if [ "$host_os" != "Darwin" ] && [ -z "${MAC_BIN:-}" ]; then
  echo "MAC_BIN must point at a macOS build of opendeck-masterhub." >&2
  echo "OpenDeck on macOS starts CodePathMac. Without that file, install fails with: No such file or directory." >&2
  exit 1
fi

if [ "$host_os" != "Darwin" ]; then
  cargo build --release --manifest-path "$manifest"
fi

if [ ! -f "$release/opendeck-masterhub.exe" ]; then
  if ! command -v cargo-xwin >/dev/null 2>&1; then
    echo "cargo-xwin is required to build opendeck-masterhub.exe on this machine." >&2
    echo "Install it with: cargo install cargo-xwin" >&2
    exit 1
  fi
  if ! command -v clang >/dev/null 2>&1; then
    echo "clang is required to build opendeck-masterhub.exe on this machine." >&2
    echo "On Ubuntu and Pop!_OS: sudo apt install clang" >&2
    exit 1
  fi
  if ! command -v rustup >/dev/null 2>&1; then
    echo "rustup is required to install the $win_target standard library." >&2
    echo "Install rustup from https://rustup.rs, then rerun this script." >&2
    exit 1
  fi
  if ! rustup target list --installed | grep -qx "$win_target"; then
    rustup target add "$win_target"
  fi
  if ! windows_archive_tool_ready; then
    rustup component add llvm-tools
  fi
  if ! windows_archive_tool_ready; then
    echo "llvm-lib is required to build opendeck-masterhub.exe on this machine." >&2
    echo "Install it with: rustup component add llvm-tools" >&2
    echo "On macOS, Homebrew LLVM also works: brew install llvm" >&2
    exit 1
  fi
  cargo xwin build --release --target "$win_target" --manifest-path "$manifest"
fi

if [ "$host_os" != "Linux" ]; then
  if ! command -v zig >/dev/null 2>&1 || ! command -v cargo-zigbuild >/dev/null 2>&1; then
    echo "zig and cargo-zigbuild are required to build opendeck-masterhub-linux on this machine." >&2
    echo "Install Zig, then: cargo install cargo-zigbuild" >&2
    exit 1
  fi
  if ! rustup target list --installed | grep -qx "$linux_rust_target"; then
    rustup target add "$linux_rust_target"
  fi
  cargo zigbuild --release --target "$linux_target" --manifest-path "$manifest"
  linux_bin=$root/plugins/opendeck-masterhub/target/$linux_rust_target/release/opendeck-masterhub
else
  linux_bin=$release/opendeck-masterhub
fi

rm -rf "$stage"
mkdir -p "$stage"
cp "$root/plugins/opendeck-masterhub/manifest.json" "$stage/manifest.json"
cp -R "$root/plugins/opendeck-masterhub/assets" "$stage/assets"
cp -R "$root/plugins/opendeck-masterhub/propertyInspector" "$stage/propertyInspector"
mkdir -p "$stage/udev"
cp "$root/udev/60-openmasterhub.rules" "$stage/udev/60-openmasterhub.rules"

if [ -n "${MAC_BIN:-}" ]; then
  mac_bin=$MAC_BIN
else
  build_universal_mac
  mac_bin=$out_dir/opendeck-masterhub
fi
cp "$mac_bin" "$stage/opendeck-masterhub"
chmod +x "$stage/opendeck-masterhub"
if [ -f "$linux_bin" ]; then
  cp "$linux_bin" "$stage/opendeck-masterhub-linux"
  chmod +x "$stage/opendeck-masterhub-linux"
fi
if [ -f "$release/opendeck-masterhub.exe" ]; then
  cp "$release/opendeck-masterhub.exe" "$stage/opendeck-masterhub.exe"
elif [ -f "$win_exe" ]; then
  cp "$win_exe" "$stage/opendeck-masterhub.exe"
fi

if [ ! -f "$stage/opendeck-masterhub" ]; then
  echo "opendeck-masterhub was not packaged; macOS OpenDeck cannot start the plugin" >&2
  exit 1
fi
if [ ! -f "$stage/opendeck-masterhub.exe" ]; then
  echo "opendeck-masterhub.exe was not packaged; Windows OpenDeck cannot start the plugin" >&2
  exit 1
fi
if [ ! -f "$stage/opendeck-masterhub-linux" ]; then
  echo "opendeck-masterhub-linux was not packaged; Linux OpenDeck cannot start the plugin" >&2
  exit 1
fi
if command -v file >/dev/null 2>&1; then
  mac_file=$(file "$stage/opendeck-masterhub")
  if ! printf '%s\n' "$mac_file" | grep -q 'Mach-O'; then
    echo "opendeck-masterhub is not a macOS executable" >&2
    exit 1
  fi
  if ! printf '%s\n' "$mac_file" | grep -q 'arm64'; then
    echo "opendeck-masterhub is missing the arm64 slice" >&2
    exit 1
  fi
  if ! printf '%s\n' "$mac_file" | grep -q 'x86_64'; then
    echo "opendeck-masterhub is missing the x86_64 slice" >&2
    exit 1
  fi
  if ! file "$stage/opendeck-masterhub-linux" | grep -q ELF; then
    echo "opendeck-masterhub-linux is not a Linux executable" >&2
    exit 1
  fi
fi

rm -f "$out_dir/$plugin_id.zip"
(
  cd "$out_dir"
  zip -r "$plugin_id.zip" "$plugin_id"
)
echo "$out_dir/$plugin_id.zip"
