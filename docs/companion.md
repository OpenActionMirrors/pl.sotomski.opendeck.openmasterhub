# Companion

Two Companion 5.1 plugins talk to `masterhubd` and do not open USB. When the configured host is this computer (`127.0.0.1`, `localhost`, or `::1`) and nothing is listening, the plugin starts `masterhubd` and retries. A second start exits 0 if the port is already taken. A remote host is left alone. Set `MASTERHUBD_BIN` to choose the binary. Otherwise the plugin uses `masterhubd` beside its main file, then `daemons/target/release/masterhubd` or the debug build, then `PATH`. The OpenDeck plugin starts the daemon too, in its own session, so quitting OpenDeck leaves it running. Both can be connected at once: the daemon broadcasts events, and each command is answered only to the client that sent it.

| Plugin | Directory | What it does |
| --- | --- | --- |
| Connection | `plugins/companion-masterhub` | Sets knob and base LED color, mode, and brightness |
| Surface | `plugins/companion-surface-masterhub` | Turns plugged-in modules into Companion controls and draws key and dial images |

The connection does not send key images, dial images, or keypad backlight. The surface does not paint knob LEDs. Slider LEDs are not set by either plugin.

## Build

The plugins are pnpm workspace packages. They import `@openmasterhub/sdk`, which is built first. From the repository root:

```sh
pnpm install
pnpm --filter "companion-module-masterhub..." --filter "companion-surface-masterhub..." build
pnpm --filter companion-module-masterhub --filter companion-surface-masterhub lint
pnpm --filter companion-module-masterhub --filter companion-surface-masterhub test
```

Point Companion's module development folder at the plugin directory. The connection shows up as **MasterHub**. The surface is added as a remote connection; the default address is `127.0.0.1` port `9123`. A localhost connection starts the daemon from `daemons/target/release/masterhubd` or the debug build, so run `pnpm build:daemons` first.

## Package

From the repository root:

```sh
pnpm package:companion
```

That builds the SDK and the daemon, then writes `plugins/companion-masterhub/masterhub-0.1.2.tgz` and `plugins/companion-surface-masterhub/masterhub-surface-0.1.3.tgz`. Each package includes the `masterhubd` binary from `daemons/target/release` or, if that is missing, the debug build. In Companion, open the Modules page and choose **Import module package** for each file.

## Surface layout

Companion gives each control one grid cell, so this is not the OpenDeck pixel canvas. Site rectangles are the same table as the OpenDeck plugin. A keypad is five columns wide and its site is four, so a crowded base can put two controls on one cell. When that happens, or when a site is missing, the controls stack: keys, rollers, sliders, then the knobs beside the dial.

The surface id stays `masterhub` across reconnects. A module change reopens that surface. Companion binds buttons by position, so moving a module moves which buttons it hits.

Roller hardware control 0 is the right roller. A slider's first reading after connect does not turn its encoder. Later readings turn it by the change in the 0–100 position. The absolute value is the input variable `slider_0` through `slider_4`, shown as Slider 1 through Slider 5.
