# OpenMasterHub

Cross-platform (macOS, Linux, Windows) driver for the Master Hub, used from OpenDeck.

When modules are connected, `masterhubd` reads the keys, knobs, rollers, sliders, and the dial, and writes key images, the dial screen, and LED reports over USB. The daemon serves the local WebSocket API on `ws://127.0.0.1:9123`. See [docs/protocol.md](docs/protocol.md). The OpenDeck plugin is described in [docs/plugins.md](docs/plugins.md).

A license has not been chosen.

## Layout

```text
openmasterhub/
├── .github/workflows/          # CI
├── daemons/masterhubd/         # USB HID daemon
├── plugins/opendeck-masterhub/ # OpenDeck device plugin
├── sdks/
│   ├── js/                     # @openmasterhub/sdk
│   └── python/                 # openmasterhub-sdk
├── packages/protocol/          # @openmasterhub/protocol
├── docs/
├── package.json
└── pnpm-workspace.yaml
```

`daemons/Cargo.toml` is the Cargo workspace for `masterhubd`.

## Prerequisites

- Node.js 20+ and pnpm 9
- Rust stable (1.88+ for the daemon and the OpenDeck plugin)
- Python 3.11+

## Develop

```sh
pnpm install
pnpm typecheck
cargo build --manifest-path daemons/Cargo.toml
cargo test --manifest-path plugins/opendeck-masterhub/Cargo.toml
```

## OpenDeck plugin

`plugins/opendeck-masterhub` is an OpenDeck device plugin. OpenDeck starts it. If nothing is listening on `ws://127.0.0.1:9123`, the plugin starts `masterhubd` in the same process. A `masterhubd` that is already listening is left running, and the plugin connects to it. While that socket is open, the plugin registers a device named Master Hub (`mh-simulator`). The daemon the plugin started exits with the plugin.

```sh
pnpm package:opendeck
```

That writes `dist/com.openmasterhub.opendeck.sdPlugin.zip`. Pushing a tag that starts with `v` runs [Release OpenDeck plugin](.github/workflows/release-opendeck.yml), which runs the same script and attaches that zip to the GitHub release for the tag. The zip includes `opendeck-masterhub.exe` for Windows and `opendeck-masterhub-linux` for x86_64 Linux. On macOS, the Windows executable is built with [cargo-xwin](https://github.com/rust-cross/cargo-xwin) (`cargo install cargo-xwin`), and the Linux executable is an x86_64 glibc 2.31 binary built with [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) plus Zig. The script installs Rust's `x86_64-pc-windows-msvc` target with rustup when it is missing, and installs the `llvm-tools` component when `llvm-lib` is not already available. That cross build also needs clang (`sudo apt install clang` on Ubuntu and Pop!_OS). cargo-xwin uses `llvm-ar` from `llvm-tools` as `llvm-lib` to archive the Windows build of hidapi. In OpenDeck, choose Plugins → Install from file and select that zip. The plugin starts `masterhubd` when the socket is closed.

On Linux, the first plugin start that cannot open the modules asks for an administrator password and installs `udev/60-openmasterhub.rules`. Dismissing that prompt leaves the hidraw nodes root-only, so the modules never enter software mode until the rule is installed by hand.

Install the Python SDK with `python3 -m pip install -e sdks/python`.

See [docs/contributing.md](docs/contributing.md) for checks.
