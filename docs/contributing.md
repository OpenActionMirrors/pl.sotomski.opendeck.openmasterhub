# Contributing

OpenMasterHub is a hybrid monorepo: pnpm for TypeScript, Cargo for Rust, and a plain Python package for the Python SDK. A license has not been chosen.

## Prerequisites

- Node.js 20 or newer, and pnpm 9
- Rust stable, 1.88 or newer
- Python 3.11 or newer

## Checks

From the repository root:

```sh
pnpm install
pnpm typecheck
pnpm --filter @openmasterhub/sdk test
cargo test --manifest-path daemons/Cargo.toml
cargo test --manifest-path plugins/opendeck-masterhub/Cargo.toml
python3 -m pip install -e sdks/python
python3 -m unittest discover -s sdks/python/tests
python3 -m unittest discover -s examples/volume-knobs
python3 -m compileall -q sdks/python scripts
python3 scripts/validate_plugins.py examples
```

`daemons/Cargo.toml` is the Cargo workspace for `masterhubd`.

## OpenDeck releases

Push a tag that starts with `v`, for example `v0.1.0`. [Release OpenDeck plugin](../.github/workflows/release-opendeck.yml) runs `scripts/package_opendeck_plugin.sh` and attaches `pl.sotomski.opendeck.openmasterhub.sdPlugin.zip` to the GitHub release for that tag. Running the workflow manually only publishes when the selected ref is a tag.

## Layout notes

`masterhubd` listens on `ws://127.0.0.1:9123` (override with `MASTERHUBD_LISTEN`). Set `MASTERHUBD_DEBUG=1` to print each published action on stderr. It opens an M4315, M003K, M032LE, M0064, M005S, and the 1xDial module when that device is present. On Linux, the first start that cannot open those hidraw nodes asks to install `udev/60-openmasterhub.rules`. The OpenDeck plugin in `plugins/opendeck-masterhub` starts `masterhubd` when that socket is closed, then connects and registers the device. See [plugins.md](plugins.md).

The JavaScript and Python SDKs are WebSocket clients for other programs that talk to `masterhubd` directly.

## Protocol changes

Update `packages/protocol/src/index.ts` and `docs/protocol.md` in the same change. Do not add USB HID report layouts to the WebSocket spec. Those notes belong in `cooltek-sdk-probe/docs/MasterHUB/`.
