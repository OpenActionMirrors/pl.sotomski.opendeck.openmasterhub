# Unofficial MasterHub plugin for OpenDeck

This is an unofficial [OpenDeck](https://github.com/nekename/OpenDeck) device plugin for the MasterHub.

With the hub plugged in, OpenDeck shows a device named **Master Hub**. Icons you assign in OpenDeck appear on the keys and on the dial screen. Presses, knob turns, and (where they work) roller turns run the actions you set on those controls.

It runs on **Linux**, **macOS**, and **Windows**.

## Install

1. Download `com.openmasterhub.opendeck.sdPlugin.zip` from the [releases](https://github.com/OpenMasterHub/openmasterhub/releases) page.
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

## Not supported

- **Sliders.** The slider module is ignored. Those controls do not appear in OpenDeck.
- **RGB.** Knob, slider, and dial LED color is not controlled from OpenDeck.
- **Rollers on macOS.** The rollers do not work on Mac. Keys, knobs, and the dial still do.
