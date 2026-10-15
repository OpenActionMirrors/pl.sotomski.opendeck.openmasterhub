# Contributing to OpenMasterHub

Welcome! We are building a fully open-source, cross-platform driver and app suite for the MasterHub. Whether you own the hardware or not, there are many ways to participate.

---

## How You Can Help

### 1. Reverse Engineering & Hardware Probing (Hardware Owners)
- Run `cargo run --bin masterhubd -- --probe` to generate a hardware diagnostic report.
- Inspect USB packet dumps and contribute packet definitions under `cooltek-sdk-probe/docs/MasterHUB/`. The WebSocket API lives in `docs/protocol.md`.

### 2. Plugin & Integration Development (No Hardware Needed!)
You can build plugins for OBS Studio, Spotify, Home Assistant, or OS utilities using our Virtual Hardware Simulator.
- Install the SDK: `pnpm add @openmasterhub/sdk` or `pip install openmasterhub-sdk`.
- Run the simulator GUI: `pnpm run dev:simulator`.

### 3. OpenDeck
- The device plugin lives in `plugins/opendeck-masterhub`. It starts `masterhubd` in the background when the connection fails, then registers the Master Hub in OpenDeck. Quitting OpenDeck leaves that daemon running. The Companion plugins start it the same way for a localhost address.
- Look for open issues tagged with **`good first issue`**.

---

## Local Development Setup

### Prerequisites
- **Rust:** `1.75+` (`rustup update stable`)
- **Node.js / pnpm:** Node `18+`, `pnpm 8+`
- **Linux:** the daemon talks to hidraw and does not need libudev or libusb to build. The first start that cannot open the modules asks, via `pkexec`, to install `udev/60-openmasterhub.rules`.

### Quickstart
```bash
# Clone the repository
git clone [https://github.com/openmasterhub/openmasterhub.git](https://github.com/openmasterhub/openmasterhub.git)
cd openmasterhub

# Install JS dependencies
pnpm install

# Run the masterhubd daemon
cargo run --manifest-path daemons/Cargo.toml --bin masterhubd