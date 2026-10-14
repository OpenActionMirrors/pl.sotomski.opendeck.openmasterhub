#!/bin/sh
# Build the OpenDeck device plugin and zip it for "Install from file".
# OpenDeck requires a top-level directory whose name ends in .sdPlugin.
#
# macOS and Linux both launch a binary with no extension, so they cannot share
# a filename. CodePathMac is opendeck-masterhub. CodePathLin is
# opendeck-masterhub-linux. CodePathWin is opendeck-masterhub.exe.
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
host_os=$(uname -s)

cargo build --release --manifest-path "$manifest"

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

if [ "$host_os" = "Darwin" ] && [ -f "$release/opendeck-masterhub" ]; then
  cp "$release/opendeck-masterhub" "$stage/opendeck-masterhub"
  chmod +x "$stage/opendeck-masterhub"
fi
if [ -f "$linux_bin" ]; then
  cp "$linux_bin" "$stage/opendeck-masterhub-linux"
  chmod +x "$stage/opendeck-masterhub-linux"
fi
if [ -f "$release/opendeck-masterhub.exe" ]; then
  cp "$release/opendeck-masterhub.exe" "$stage/opendeck-masterhub.exe"
elif [ -f "$win_exe" ]; then
  cp "$win_exe" "$stage/opendeck-masterhub.exe"
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
