# Protocol actions

Payloads for the [WebSocket API](../protocol.md). Indexes are 0-based. Unknown payload fields are rejected.

## Input actions

Send these as `type: "trigger"`. The daemon broadcasts the same `action` and `payload` as `type: "event"`.

HID input reports are snapshots or per-control edges. These actions are one control at a time. The M4315 reader compares key snapshots and emits one action per key that changed. Knob turns and roller turns are already edges. Knob-press and slider readers compare snapshots the same way. A simulator sends the same actions directly.

### `key_state`

M4315 key `0..14`. `pressed` is the key state, so a release is `false`.

```json
{
  "v": 1,
  "type": "trigger",
  "action": "key_state",
  "payload": {
    "module": { "model": "M4315", "id": "keys" },
    "control": 0,
    "pressed": true
  }
}
```

### `knob_rotated`

M003K knob `0..2`, or the M0064 base knob at control `0`. On M003K, `0` is the top knob, `1` is the bottom-left knob, and `2` is the bottom-right knob. `direction` is `cw` or `ccw`. Each event is one detent. The hardware report has no tick count.

```json
{
  "v": 1,
  "type": "trigger",
  "action": "knob_rotated",
  "payload": {
    "module": { "model": "M003K", "id": "knobs" },
    "control": 0,
    "direction": "cw"
  }
}
```

### `knob_state`

M003K knob `0..2`, or the M0064 base knob at control `0`. `pressed` is the knob button state.

```json
{
  "v": 1,
  "type": "trigger",
  "action": "knob_state",
  "payload": {
    "module": { "model": "M003K", "id": "knobs" },
    "control": 1,
    "pressed": false
  }
}
```

### `slider_moved`

M005S slider `0..4`. `value` is an integer `0..100`.

```json
{
  "v": 1,
  "type": "trigger",
  "action": "slider_moved",
  "payload": {
    "module": { "model": "M005S", "id": "faders" },
    "control": 2,
    "value": 40
  }
}
```

### `roller_rotated`

M032LE roller `0..1`. `direction` is `up` or `down`.

```json
{
  "v": 1,
  "type": "trigger",
  "action": "roller_rotated",
  "payload": {
    "module": { "model": "M032LE", "id": "rollers" },
    "control": 0,
    "direction": "up"
  }
}
```

## Output commands

Send these as `type: "command"`. A valid command returns `result` with `ok: true` to the sender. `set_button_image` is also broadcast as an `event`. Other commands are not broadcast. A connected M4315 receives `set_button_image` as a JPEG on that key. A connected 1xDial receives M0064 `set_screen_image`. A connected M003K, M005S, or M0064 receives its LED commands. M032LE lighting and the M4315 module screen are not sent to hardware.

### `set_button_image`

M4315 key `0..14`. `data_base64` is a standard-base64 JPEG. The known button transfer is JPEG, and the panel is 72×72. The daemon checks the JPEG start marker (`FF D8`) and rejects a decoded image larger than 64 KiB. Before writing to an M4315, it decodes the JPEG. An image wider or taller than 72 pixels is scaled to 72×72, because a larger image stalls the panel and it drops off the bus. A file that is not a decodable JPEG is not written. The examples use `/9j/2Q==`, which is only that marker: the socket accepts it, and the M4315 writer skips it.

```json
{
  "v": 1,
  "type": "command",
  "id": "img-1",
  "action": "set_button_image",
  "payload": {
    "module": { "model": "M4315", "id": "keys" },
    "control": 3,
    "data_base64": "/9j/2Q=="
  }
}
```

### `set_screen_image`

M4315 module screen, or the M0064 dial screen. Same JPEG payload as `set_button_image`, without `control`. The M4315 HID command `0x35` (Display Picture On Screen) is still incomplete in the reverse-engineered notes, so that model is accepted at the API only. The dial screen is 240×240. The JPEG is fitted inside that square before the write, keeping its shape. A file that is not a decodable JPEG is not written.

```json
{
  "v": 1,
  "type": "command",
  "action": "set_screen_image",
  "payload": {
    "module": { "model": "M4315", "id": "keys" },
    "data_base64": "/9j/2Q=="
  }
}
```

### `set_button_color`

M4315 key `0..14`. Channels are integers `0..255`.

```json
{
  "v": 1,
  "type": "command",
  "action": "set_button_color",
  "payload": {
    "module": { "model": "M4315", "id": "keys" },
    "control": 0,
    "r": 51,
    "g": 36,
    "b": 63
  }
}
```

### `set_screen_color`

M4315 module screen. Same channels, no `control`.

```json
{
  "v": 1,
  "type": "command",
  "action": "set_screen_color",
  "payload": {
    "module": { "model": "M4315", "id": "keys" },
    "r": 51,
    "g": 36,
    "b": 63
  }
}
```

### `set_backlight`

M4315 backlight. `brightness` is an integer `0..100`.

```json
{
  "v": 1,
  "type": "command",
  "action": "set_backlight",
  "payload": {
    "module": { "model": "M4315", "id": "keys" },
    "brightness": 100
  }
}
```

### `set_led_mode`

M003K (control `0..2` or `"all"`), M005S (control `0..4` or `"all"`), or M0064 (control `0` only).

`mode` is one of `disable`, `static`, `breathing`, `strobe`, `spring`, `cycling`, `random`, `wave`, `water`, `rainbow`.

`save` is optional and defaults to `false`. A connected module writes that flag on the LED report.

Only `static`, `breathing`, `strobe`, and `spring` display a custom color on the hardware. The daemon still accepts `set_led_color` alongside any mode.

```json
{
  "v": 1,
  "type": "command",
  "action": "set_led_mode",
  "payload": {
    "module": { "model": "M003K", "id": "knobs" },
    "control": "all",
    "mode": "rainbow",
    "save": false
  }
}
```

### `set_led_color`

Same modules and control rules as `set_led_mode`. Channels are integers `0..255`. `save` defaults to `false`.

```json
{
  "v": 1,
  "type": "command",
  "action": "set_led_color",
  "payload": {
    "module": { "model": "M005S", "id": "faders" },
    "control": 1,
    "r": 255,
    "g": 0,
    "b": 0,
    "save": false
  }
}
```

### `set_led_brightness`

Same modules and control rules as `set_led_mode`. `brightness` is an integer `0..100`. There is no `save` field. The hardware brightness report does not have one.

```json
{
  "v": 1,
  "type": "command",
  "action": "set_led_brightness",
  "payload": {
    "module": { "model": "M0064", "id": "base" },
    "control": 0,
    "brightness": 80
  }
}
```
