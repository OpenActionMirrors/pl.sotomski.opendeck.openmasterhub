## MasterHub

This connection sets the knob and base LED color, mode, and brightness. It talks to `masterhubd` at the host and port you enter. The default is `127.0.0.1` port `9123`.

When the host is this computer and nothing is listening, this module starts `masterhubd`. It does not open the USB device. A hello with no modules plugged in is still a working connection.

### Actions

**Set LED color.** Choose any of the top knob, the left knob, the right knob, and the base. All three knobs in one press are sent as one command. The base is a separate command.

**Set LED mode.** The same targets. Set applies one mode. On/off swaps between off and solid. Next mode steps from solid through rainbow and skips off. The first on/off press turns the lights solid. The first next-mode press sends solid. The connection remembers the last mode it sent. The hub does not report the current mode, so another program can change the lights without this module noticing.

**Set LED brightness.** The same targets. Set applies a level from 0 to 100. Increase and decrease step from the level this connection last sent, which starts at 100. The hub does not report the hardware level.

A custom color is shown in solid, breathing, strobe, and spring.

### Feedback and variables

**Module is connected** is true when `masterhubd` lists that module. Base is also true when the dial is connected, because they share the base knob.

`usb` is `true` when any module is open. `modules` is the list from the daemon.

Key images, the dial screen, and the keypad backlight belong to the MasterHub surface plugin.
