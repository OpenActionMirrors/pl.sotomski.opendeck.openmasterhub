# Unofficial MasterHub plugin for OpenDeck

This is an unofficial [OpenDeck](https://github.com/nekename/OpenDeck) device plugin for the MasterHub.

When a keypad, knob, roller, or dial module is connected, OpenDeck shows a device named **Master Hub**. The canvas shows only the modules that are plugged in, and a module appears or disappears when it is hot-swapped. Icons you assign in OpenDeck appear on the keys and on the dial screen. Presses, knob turns, and (where they work) roller turns run the actions you set on those controls.

It runs on **Linux**, **macOS**, and **Windows**.

## Install

1. Download `pl.sotomski.opendeck.openmasterhub.sdPlugin.zip` from the [releases](https://github.com/OpenMasterHub/openmasterhub/releases) page.
2. In OpenDeck, choose **Plugins → Install from file** and select that zip.
3. Plug in the MasterHub. OpenDeck starts the plugin, and the plugin talks to the hub on its own.

macOS 11.3 or newer. Windows 10 or newer.

### Linux permissions

The hub’s USB devices are root-only until a udev rule is installed. The first time the plugin cannot open them, it asks for an administrator password and installs that rule. Approve the prompt. Later starts do not ask again.

If you dismiss the prompt, the keys and knobs stay silent. Unzip the plugin and install the rule bundled at `udev/60-openmasterhub.rules`:

```sh
sudo cp udev/60-openmasterhub.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
```

Unplug the hub, plug it back in, and restart OpenDeck.

## Supported

| Control | What works |
| --- | --- |
| 15 keys (3×5) | Press and release. Icons from OpenDeck are shown on the keys. Key backlight brightness follows OpenDeck’s brightness setting. |
| 3 knobs | Turn and press. Top knob, then the two knobs under it. |
| Dial | Turn, press, and the image on the round screen. |
| 2 rollers | Turn, on Linux and Windows. |
| 5 sliders | Move. OpenDeck shows them as encoders, left to right. |

## Actions

**Set LED color.** Choose any combination of the base, the top knob, the left knob, and the right knob. The color is hue, saturation, and brightness, or the color picker. A press applies that color. It shows in solid, breathing, strobe, and spring. The key shows the selected color.

**Set LED mode.** The same targets. Set applies one mode: solid, breathing, strobe, spring, cycling, random, wave, water, rainbow, or off. On/off toggle swaps between off and solid. Toggle mode steps through those effects, from solid through rainbow and back to solid. A new on/off key turns the lights solid on the first press. A new toggle-mode key starts at solid.

**Brightness.** Choose the keypad backlight, the RGB lights, or toggle after press. On a key, choose increase or decrease and a step. The key shows a sun with a plus or a minus. On a knob, the step is how far one detent moves the selected brightness. Toggle after press switches a knob or the dial between keypad and RGB; a key changes the current one and then switches. On the dial, the screen shows a ring and the percent as soon as the action is placed there, and Keypad or RGB after a toggle. Keys, knobs, and the dial share one keypad level and one RGB level, and a dial that is showing that light follows a change from any of them.

## Not supported

- **Slider LEDs.** Moving a slider runs its encoder action. The slider lights are not set from OpenDeck.
- **Rollers on macOS.** The rollers do not work on Mac. Keys, knobs, and the dial still do.

## License

[MIT](LICENSE).
