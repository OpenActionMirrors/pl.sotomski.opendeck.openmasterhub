## MasterHub surface

This surface uses the MasterHub keys, knobs, rollers, sliders, and dial as Companion controls. Add a remote connection and enter the `masterhubd` host and port. The default is `127.0.0.1` port `9123`.

When the host is this computer and nothing is listening, this plugin starts `masterhubd`. It does not open the USB device. The surface appears when a keypad, knob, roller, slider, or dial module is plugged in. The base on its own does not appear.

Companion draws the 15 keys at 72×72 and the dial at 240×240. Knob, roller, and slider cells do not show button graphics. The surface brightness control is the keypad backlight.

### Layout

When every plugged-in module has a known pogo site and the controls do not land on the same cell, each module sits on that site. Otherwise the controls stack: keys, rollers, sliders, then the knobs beside the dial.

Companion remembers buttons by grid position. Moving a module on the base moves which buttons it presses. Hot-plugging a module reopens the surface with the new grid.

Roller hardware control 0 is the right roller. Slider moves become encoder turns. The first reading after connect only stores the position. Each slider also publishes `slider_0` through `slider_4` (Slider 1 through Slider 5) as a value from 0 to 100.

With the keypad connected, pincode entry uses the first ten keys, left to right and top to bottom, as digits 0 through 9. While the surface is locked, the keys and the dial are cleared.

Rollers can stay silent on macOS. That is a limit of the daemon, not of this surface. Knob and base LED color, mode, and brightness are the MasterHub connection module.
