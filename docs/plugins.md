# OpenDeck

`plugins/opendeck-masterhub` is the device plugin. OpenDeck starts it. If nothing is listening on `ws://127.0.0.1:9123` (`MASTERHUBD_LISTEN`), the plugin starts `masterhubd` in the same process. A `masterhubd` that is already listening is left running, and the plugin connects to it. While that socket is open, the plugin registers a device named Master Hub (`mh-simulator`). The daemon the plugin started exits with the plugin.

Hardware events from the daemon become OpenDeck key and encoder events:

| Daemon event | OpenDeck control |
| --- | --- |
| M4315 `key_state` | keys 0–14 |
| M003K `knob_rotated` / `knob_state` | encoders 0–2 (top, bottom-left, bottom-right) |
| M032LE `roller_rotated` | encoders 3–4 (left, right) |
| M0064 `knob_rotated` / `knob_state`, control 0 | encoder 5, the dial |

The M005S 5× slider module is not registered. OpenDeck does not handle it, so `slider_moved` is ignored and the sliders do not appear on the device.

On Linux the modules show up as root-only hidraw nodes. The first time the plugin starts and cannot open them, it asks for an administrator password and installs `udev/60-openmasterhub.rules`. Later starts do not ask once that rule is in place. If you dismiss the prompt, install it yourself:

```sh
sudo cp udev/60-openmasterhub.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
```

Key images from OpenDeck are sent to `masterhubd` as `set_button_image`. The dial image, encoder 5, is sent as `set_screen_image` for M0064. A key image larger than 72×72 is scaled down. A dial image is fitted inside 240×240 without stretching.

```sh
pnpm package:opendeck
```

That writes `dist/com.openmasterhub.opendeck.sdPlugin.zip`. Pushing a tag that starts with `v` runs [Release OpenDeck plugin](../.github/workflows/release-opendeck.yml), which runs the same script and attaches that zip to the GitHub release for the tag. The zip includes `opendeck-masterhub.exe` (`CodePathWin`) and `opendeck-masterhub-linux` (`CodePathLin`, x86_64, glibc 2.31 or newer). On macOS, the Windows executable is built with [cargo-xwin](https://github.com/rust-cross/cargo-xwin) (`cargo install cargo-xwin`), and the Linux executable is built with [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) plus Zig. The script installs Rust's `x86_64-pc-windows-msvc` target with rustup when it is missing, and installs the `llvm-tools` component when `llvm-lib` is not already available. That cross build also needs clang (`sudo apt install clang` on Ubuntu and Pop!_OS). cargo-xwin uses `llvm-ar` from `llvm-tools` as `llvm-lib` to archive the Windows build of hidapi. Install the zip from OpenDeck with Plugins → Install from file. The plugin starts `masterhubd` when the socket is closed.
