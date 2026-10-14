# OpenDeck

`plugins/opendeck-masterhub` is the device plugin. OpenDeck starts it. If nothing is listening on `ws://127.0.0.1:9123` (`MASTERHUBD_LISTEN`), the plugin starts `masterhubd` in the same process. A `masterhubd` that is already listening is left running, and the plugin connects to it. While that socket is open, the plugin registers a device named Master Hub (`masterhub`) when a module with controls is connected: the keypad, the knobs, the rollers, the sliders, or the dial. The canvas contains only those modules. Plugging or unplugging one sends `registerDevice` again, or `deregisterDevice` when none of them remain. OpenDeck matches a device id by its first two letters, so the manifest namespace is `ma`. The base does not add controls. When the daemon reports a pogo site, the canvas is the base grid and each module sits on that site, so an empty site stays empty. Without a site, the modules stack as keys, rollers, sliders, then the knobs beside the dial. The daemon the plugin started exits with the plugin. Set `MASTERHUBD_DEBUG=1` in the environment before starting OpenDeck to print each published action on that process’s stderr.

Hardware events from the daemon become OpenDeck key and encoder events:

| Daemon event | OpenDeck control |
| --- | --- |
| M4315 `key_state` | keys 0–14 |
| M003K `knob_rotated` / `knob_state` | encoders 0–2 (top, bottom-left, bottom-right) |
| M032LE `roller_rotated` | encoders 3–4 (left, right) |
| M0064 `knob_rotated` / `knob_state`, control 0 | encoder 5, the dial |
| M005S `slider_moved` | encoders 6–10, left to right |

A slider move is the change in its 0–100 position. The first reading after the module connects sets that position and does not turn the encoder. OpenDeck has no slider control, so the five faders are drawn as encoders. The site for each module is the table in [base-usb-ports.md](base-usb-ports.md).

On Linux the modules show up as root-only hidraw nodes. The first time the plugin starts and cannot open them, it asks for an administrator password and installs `udev/60-openmasterhub.rules`. Later starts do not ask once that rule is in place. If you dismiss the prompt, install it yourself:

```sh
sudo cp udev/60-openmasterhub.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
```

Key images from OpenDeck are sent to `masterhubd` as `set_button_image`. The dial image, encoder 5, is sent as `set_screen_image` for M0064. A key image larger than 72×72 is scaled down. A dial image is fitted inside 240×240 without stretching. OpenDeck’s device brightness is `set_backlight` on the keypad.

A `setImage` with no image clears that control: a black frame on one key, or on the dial when the control is encoder 5. A profile change sends `setImage` with no controller, position, or image. That clears every key and the dial, and the images for the new profile replace the blank ones. A key or dial that the new profile leaves empty stays blank.

Three plugin actions run on key press and on encoder press. Brightness keeps one shared keypad level and one shared RGB level. It reads `step` (1–100, default 1) from the control that moved. `output` is `keypad`, `rgb`, or `toggle`. A key uses `action` `increase` or `decrease` and moves the selected level by its step. A knob or the dial multiplies each turn tick by its own step. The two levels are shared, so a key does not keep a separate copy that jumps to 0 or 100. Toggle after press stores `source` as `keypad` or `rgb`. A press on a knob or the dial swaps that source. A press on a key changes the current level, then swaps the source. When the action is on the dial, encoder 5, the screen shows a ring filled to the current percent, with that percent in the center, including when the action first appears. In toggle mode the screen also shows Keypad or RGB. OpenDeck's action icon and title are not drawn on that screen. Any later change from a key, a knob, or the dial sends a new `set_screen_image`.

The action list shows a sun for brightness and an RGB ring for the color and mode actions. On a key, the brightness icon adds a plus or a minus for increase or decrease. The color action draws its key in the selected color.

| Action | Settings | Command |
| --- | --- | --- |
| Set LED color | `targets`: any of `base`, `top`, `left`, `right`. `h` 0–360, `s` 0–100, `v` 0–100 (HSV). A saved `l` without `v` is still read as HSL | `set_led_color`, after converting HSV to RGB |
| Set LED mode | same `targets`. `action` is `set`, `toggle`, or `cycle`. `mode`: `static`, `breathing`, `strobe`, `spring`, `cycling`, `random`, `wave`, `water`, `rainbow`, or `disable`. On/off toggle swaps off and solid. Toggle mode steps through the effects, from solid back around to solid, and skips off. Both hide the mode list | `set_led_mode` |
| Brightness | `output`: `keypad`, `rgb`, or `toggle`. `step` 1–100. On a key, `action` is `increase` or `decrease`. Toggle keeps `source` as `keypad` or `rgb`. Keypad and RGB levels are shared separately | Keypad sends `set_backlight` on the M4315. RGB sends `set_led_brightness` to every knob and the base. A dial also receives `set_screen_image` |

All three knobs in one press become a single M003K command with `control: "all"`. The base is a separate M0064 command on control `0`. A custom color is displayed in static, breathing, strobe, and spring. With no saved settings, the color action targets every LED above and uses red (`h` 0, `s` 100, `v` 100). The mode action uses solid (`static`) when its action is set. On/off toggle remembers the last of off or solid that it sent; a new on/off key starts from off, so the first press turns the lights solid. Toggle mode remembers the last effect it sent and applies the next one; a new key starts at solid. Brightness starts at keypad and uses 100 for both levels. A new toggle-after-press control starts on keypad, so the first press selects RGB.

```sh
pnpm package:opendeck
```

That writes `dist/pl.sotomski.opendeck.openmasterhub.sdPlugin.zip`. Pushing a tag that starts with `v` runs [Release OpenDeck plugin](../.github/workflows/release-opendeck.yml), which runs the same script and attaches that zip to the GitHub release for the tag. The zip includes `opendeck-masterhub.exe` (`CodePathWin`) and `opendeck-masterhub-linux` (`CodePathLin`, x86_64, glibc 2.31 or newer). On macOS, the Windows executable is built with [cargo-xwin](https://github.com/rust-cross/cargo-xwin) (`cargo install cargo-xwin`), and the Linux executable is built with [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) plus Zig. The script installs Rust's `x86_64-pc-windows-msvc` target with rustup when it is missing, and installs the `llvm-tools` component when `llvm-lib` is not already available. That cross build also needs clang (`sudo apt install clang` on Ubuntu and Pop!_OS). cargo-xwin uses `llvm-ar` from `llvm-tools` as `llvm-lib` to archive the Windows build of hidapi. Install the zip from OpenDeck with Plugins → Install from file. The plugin starts `masterhubd` when the socket is closed.
