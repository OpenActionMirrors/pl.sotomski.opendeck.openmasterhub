use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::Value;

pub const DEVICE_ID: &str = "mh-simulator";
pub const DEVICE_NAME: &str = "Master Hub";
pub const ROWS: u8 = 3;
pub const COLUMNS: u8 = 5;
/// Three knobs, two rollers, then the base knob. The M005S slider module is
/// omitted: OpenDeck does not handle it, so it is not registered.
pub const ENCODERS: u8 = 6;
pub const DEVICE_TYPE: u8 = 0;

const KEYS: &str = "keys";
const KNOBS: &str = "knobs";
const ROLLERS: &str = "rollers";

const ROLLER_ENCODER: u8 = 3;
const BIG_KNOB_ENCODER: u8 = 5;

const BASE: &str = "base";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    KeyDown(u8),
    KeyUp(u8),
    EncoderDown(u8),
    EncoderUp(u8),
    EncoderChange { position: u8, ticks: i16 },
}

pub fn map_event(value: &Value) -> Option<DeviceEvent> {
    if value.get("type")?.as_str()? != "event" {
        return None;
    }
    let action = value.get("action")?.as_str()?;
    let payload = value.get("payload")?;
    let module = payload.get("module")?;
    let model = module.get("model")?.as_str()?;
    let id = module.get("id")?.as_str()?;
    let control = u8::try_from(payload.get("control")?.as_u64()?).ok()?;

    match (action, model, id) {
        ("key_state", "M4315", KEYS) if control < 15 => {
            let pressed = payload.get("pressed")?.as_bool()?;
            Some(if pressed {
                DeviceEvent::KeyDown(control)
            } else {
                DeviceEvent::KeyUp(control)
            })
        }
        ("knob_rotated", "M003K", KNOBS) if control < 3 => {
            let ticks = match payload.get("direction")?.as_str()? {
                "cw" => 1,
                "ccw" => -1,
                _ => return None,
            };
            Some(DeviceEvent::EncoderChange {
                position: control,
                ticks,
            })
        }
        ("knob_state", "M003K", KNOBS) if control < 3 => {
            let pressed = payload.get("pressed")?.as_bool()?;
            Some(if pressed {
                DeviceEvent::EncoderDown(control)
            } else {
                DeviceEvent::EncoderUp(control)
            })
        }
        ("roller_rotated", "M032LE", ROLLERS) if control < 2 => {
            let ticks = match payload.get("direction")?.as_str()? {
                "up" => 1,
                "down" => -1,
                _ => return None,
            };
            Some(DeviceEvent::EncoderChange {
                // Report control 0 is the right roller.
                position: ROLLER_ENCODER + 1 - control,
                ticks,
            })
        }
        ("knob_rotated", "M0064", BASE) if control == 0 => {
            let ticks = match payload.get("direction")?.as_str()? {
                "cw" => 1,
                "ccw" => -1,
                _ => return None,
            };
            Some(DeviceEvent::EncoderChange {
                position: BIG_KNOB_ENCODER,
                ticks,
            })
        }
        ("knob_state", "M0064", BASE) if control == 0 => {
            let pressed = payload.get("pressed")?.as_bool()?;
            Some(if pressed {
                DeviceEvent::EncoderDown(BIG_KNOB_ENCODER)
            } else {
                DeviceEvent::EncoderUp(BIG_KNOB_ENCODER)
            })
        }
        _ => None,
    }
}

pub fn image_command(
    device: &str,
    controller: Option<&str>,
    position: Option<u8>,
    image: Option<&str>,
) -> Option<String> {
    if device != DEVICE_ID {
        return None;
    }
    let jpeg = jpeg_base64_from_data_url(image?)?;
    if controller == Some("Encoder") {
        if position == Some(BIG_KNOB_ENCODER) {
            return Some(screen_image_command(&jpeg));
        }
        return None;
    }
    let position = position.filter(|position| *position < 15)?;
    Some(button_image_command(position, &jpeg))
}

fn screen_image_command(data_base64: &str) -> String {
    serde_json::json!({
        "v": 1,
        "type": "command",
        "action": "set_screen_image",
        "payload": {
            "module": { "model": "M0064", "id": BASE },
            "data_base64": data_base64,
        }
    })
    .to_string()
}

pub fn brightness_command(device: &str, brightness: u8) -> Option<String> {
    if device != DEVICE_ID {
        return None;
    }
    Some(backlight_command(brightness.min(100)))
}

fn button_image_command(control: u8, data_base64: &str) -> String {
    serde_json::json!({
        "v": 1,
        "type": "command",
        "action": "set_button_image",
        "payload": {
            "module": { "model": "M4315", "id": KEYS },
            "control": control,
            "data_base64": data_base64,
        }
    })
    .to_string()
}

fn backlight_command(brightness: u8) -> String {
    serde_json::json!({
        "v": 1,
        "type": "command",
        "action": "set_backlight",
        "payload": {
            "module": { "model": "M4315", "id": KEYS },
            "brightness": brightness,
        }
    })
    .to_string()
}

pub fn registration_event() -> Value {
    serde_json::json!({
        "event": "registerDevice",
        "payload": registration_payload(),
    })
}

pub fn registration_payload() -> Value {
    serde_json::json!({
        "id": DEVICE_ID,
        "name": DEVICE_NAME,
        "rows": ROWS,
        "columns": COLUMNS,
        "encoders": ENCODERS,
        "type": DEVICE_TYPE,
        "encoder": encoder_descriptors(),
        "layoutVersion": 1,
        "layout": device_layout(),
    })
}

fn encoder_descriptors() -> Vec<Value> {
    let mut encoders = Vec::with_capacity(usize::from(ENCODERS));
    for _ in 0..3 {
        encoders.push(descriptor("rotary", false));
    }
    for _ in 0..2 {
        encoders.push(descriptor("roller", false));
    }
    encoders.push(descriptor("rotary", false));
    encoders
}

fn descriptor(kind: &str, is_absolute: bool) -> Value {
    serde_json::json!({ "type": kind, "is_absolute": is_absolute })
}

/// Same outer size as OpenDeck's legacy key cell. Explicit layouts are not scaled up.
const CELL: u16 = 132;
const GAP: u16 = 24;

/// Keys, then the two rollers, then the knob cluster beside the dial.
///
/// The three knobs are a 2×2 block: the top knob is centered on the first row,
/// and the other two fill the second row. The dial's box is that same block.
/// Neighboring controls are separated by [`GAP`].
fn device_layout() -> Value {
    let pitch = CELL + GAP;
    let keys_width = u16::from(COLUMNS) * CELL + u16::from(COLUMNS - 1) * GAP;
    let keys_height = u16::from(ROWS) * CELL + u16::from(ROWS - 1) * GAP;
    let cluster = CELL * 2 + GAP;
    let canvas_width = keys_width;
    let rollers_top = keys_height + GAP;
    let bottom_top = rollers_top + CELL + GAP;
    let canvas_height = bottom_top + cluster;
    let rollers_left = (canvas_width - cluster) / 2;
    let bottom_left = (canvas_width - (cluster * 2 + GAP)) / 2;

    let mut controls = Vec::with_capacity(usize::from(ROWS * COLUMNS + ENCODERS));
    for position in 0..ROWS * COLUMNS {
        let column = u16::from(position % COLUMNS);
        let row = u16::from(position / COLUMNS);
        controls.push(key(position, column * pitch, row * pitch));
    }
    controls.push(encoder_cell(ROLLER_ENCODER, rollers_left, rollers_top));
    controls.push(encoder_cell(
        ROLLER_ENCODER + 1,
        rollers_left + pitch,
        rollers_top,
    ));
    controls.push(encoder(
        0,
        bottom_left + cluster / 2,
        bottom_top + CELL / 2,
        CELL / 2,
    ));
    controls.push(encoder_cell(1, bottom_left, bottom_top + pitch));
    controls.push(encoder_cell(2, bottom_left + pitch, bottom_top + pitch));
    controls.push(encoder(
        BIG_KNOB_ENCODER,
        bottom_left + cluster + GAP + cluster / 2,
        bottom_top + cluster / 2,
        cluster / 2,
    ));

    serde_json::json!({
        "canvas": { "width": canvas_width, "height": canvas_height, "unit": "px" },
        "controls": controls,
    })
}

fn key(position: u8, x: u16, y: u16) -> Value {
    serde_json::json!({
        "controller": "Keypad",
        "position": position,
        "shape": "rect",
        "x": x,
        "y": y,
        "width": CELL,
        "height": CELL,
    })
}

fn encoder_cell(position: u8, x: u16, y: u16) -> Value {
    encoder(position, x + CELL / 2, y + CELL / 2, CELL / 2)
}

fn encoder(position: u8, cx: u16, cy: u16, r: u16) -> Value {
    serde_json::json!({
        "controller": "Encoder",
        "position": position,
        "shape": "circle",
        "cx": cx,
        "cy": cy,
        "r": r,
    })
}

fn jpeg_base64_from_data_url(data_url: &str) -> Option<String> {
    let (header, data) = data_url.split_once(',')?;
    let bytes = STANDARD.decode(data.trim()).ok()?;
    if header.contains("image/jpeg") && bytes.starts_with(&[0xFF, 0xD8]) {
        return Some(STANDARD.encode(bytes));
    }
    let image = image::load_from_memory(&bytes).ok()?;
    let mut encoded = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut encoded),
            image::ImageFormat::Jpeg,
        )
        .ok()?;
    Some(STANDARD.encode(encoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(action: &str, payload: serde_json::Value) -> Value {
        serde_json::json!({ "v": 1, "type": "event", "action": action, "payload": payload })
    }

    #[test]
    fn maps_key_knob_and_roller() {
        assert_eq!(
            map_event(&event(
                "key_state",
                serde_json::json!({"module":{"model":"M4315","id":"keys"},"control":4,"pressed":true})
            )),
            Some(DeviceEvent::KeyDown(4))
        );
        assert_eq!(
            map_event(&event(
                "key_state",
                serde_json::json!({"module":{"model":"M4315","id":"keys"},"control":4,"pressed":false})
            )),
            Some(DeviceEvent::KeyUp(4))
        );
        assert_eq!(
            map_event(&event(
                "knob_rotated",
                serde_json::json!({"module":{"model":"M003K","id":"knobs"},"control":1,"direction":"ccw"})
            )),
            Some(DeviceEvent::EncoderChange {
                position: 1,
                ticks: -1,
            })
        );
        assert_eq!(
            map_event(&event(
                "knob_state",
                serde_json::json!({"module":{"model":"M003K","id":"knobs"},"control":2,"pressed":true})
            )),
            Some(DeviceEvent::EncoderDown(2))
        );
        assert_eq!(
            map_event(&event(
                "roller_rotated",
                serde_json::json!({"module":{"model":"M032LE","id":"rollers"},"control":0,"direction":"up"})
            )),
            Some(DeviceEvent::EncoderChange {
                position: 4,
                ticks: 1,
            })
        );
        assert_eq!(
            map_event(&event(
                "roller_rotated",
                serde_json::json!({"module":{"model":"M032LE","id":"rollers"},"control":1,"direction":"down"})
            )),
            Some(DeviceEvent::EncoderChange {
                position: 3,
                ticks: -1,
            })
        );
        assert_eq!(
            map_event(&event(
                "slider_moved",
                serde_json::json!({"module":{"model":"M005S","id":"faders"},"control":2,"value":40})
            )),
            None
        );
        assert_eq!(
            map_event(&event(
                "knob_rotated",
                serde_json::json!({"module":{"model":"M0064","id":"base"},"control":0,"direction":"cw"})
            )),
            Some(DeviceEvent::EncoderChange {
                position: 5,
                ticks: 1,
            })
        );
        assert_eq!(
            map_event(&event(
                "knob_state",
                serde_json::json!({"module":{"model":"M0064","id":"base"},"control":0,"pressed":true})
            )),
            Some(DeviceEvent::EncoderDown(5))
        );
        assert_eq!(
            map_event(&event(
                "set_button_image",
                serde_json::json!({"module":{"model":"M4315","id":"keys"},"control":0,"data_base64":"/9j/2Q=="})
            )),
            None
        );
    }

    #[test]
    fn registration_describes_masterhub_layout() {
        let payload = registration_payload();
        assert_eq!(payload["name"], "Master Hub");
        assert_eq!(payload["encoders"], 6);
        assert_eq!(payload["encoder"].as_array().unwrap().len(), 6);
        assert_eq!(payload["encoder"][0]["type"], "rotary");
        assert_eq!(payload["encoder"][0]["is_absolute"], false);
        assert_eq!(payload["encoder"][3]["type"], "roller");
        assert_eq!(payload["encoder"][5]["type"], "rotary");
        assert_eq!(payload["encoder"][5]["is_absolute"], false);
        assert!(payload["encoder"]
            .as_array()
            .unwrap()
            .iter()
            .all(|encoder| encoder["type"] != "slider"));

        assert_eq!(payload["layoutVersion"], 1);
        let layout = &payload["layout"];
        assert_eq!(layout["canvas"]["width"], 756);
        assert_eq!(layout["canvas"]["height"], 912);
        assert_eq!(layout["canvas"]["unit"], "px");
        let controls = layout["controls"].as_array().unwrap();
        assert_eq!(controls.len(), 21);

        for position in 0..15u64 {
            let key = control(controls, "Keypad", position);
            assert_eq!(key["shape"], "rect");
            assert_eq!(key["width"], 132);
            assert_eq!(key["height"], 132);
            assert_eq!(key["x"], (position % 5) * 156);
            assert_eq!(key["y"], (position / 5) * 156);
        }

        let top = control(controls, "Encoder", 0);
        let lower_left = control(controls, "Encoder", 1);
        let lower_right = control(controls, "Encoder", 2);
        let dial = control(controls, "Encoder", 5);
        let roller_left = control(controls, "Encoder", 3);
        let roller_right = control(controls, "Encoder", 4);
        for encoder in [
            top,
            lower_left,
            lower_right,
            dial,
            roller_left,
            roller_right,
        ] {
            assert_eq!(encoder["shape"], "circle");
        }

        let radius = lower_left["r"].as_u64().unwrap();
        assert_eq!(top["r"], radius);
        assert_eq!(lower_right["r"], radius);
        assert_eq!(lower_left["cy"], lower_right["cy"]);
        assert_eq!(
            top["cx"].as_u64().unwrap(),
            (lower_left["cx"].as_u64().unwrap() + lower_right["cx"].as_u64().unwrap()) / 2
        );
        assert!(top["cy"].as_u64().unwrap() < lower_left["cy"].as_u64().unwrap());

        let cluster_left = lower_left["cx"].as_u64().unwrap() - radius;
        let cluster_right = lower_right["cx"].as_u64().unwrap() + radius;
        let cluster_top = top["cy"].as_u64().unwrap() - radius;
        let cluster_bottom = lower_left["cy"].as_u64().unwrap() + radius;
        let cluster_size = cluster_right - cluster_left;
        assert_eq!(cluster_bottom - cluster_top, cluster_size);
        assert_eq!(dial["r"].as_u64().unwrap() * 2, cluster_size);
        assert_eq!(
            dial["cy"].as_u64().unwrap(),
            (cluster_top + cluster_bottom) / 2
        );
        assert_eq!(
            dial["cx"].as_u64().unwrap() - dial["r"].as_u64().unwrap(),
            cluster_right + u64::from(GAP)
        );
        assert_eq!(
            lower_right["cx"].as_u64().unwrap()
                - radius
                - (lower_left["cx"].as_u64().unwrap() + radius),
            u64::from(GAP)
        );

        assert_eq!(roller_left["r"], radius);
        assert_eq!(roller_right["r"], radius);
        assert_eq!(roller_left["cy"], roller_right["cy"]);
        assert!(roller_left["cx"].as_u64().unwrap() < roller_right["cx"].as_u64().unwrap());
        assert!(roller_left["cy"].as_u64().unwrap() + radius <= cluster_top);
    }

    fn control<'a>(controls: &'a [Value], controller: &str, position: u64) -> &'a Value {
        controls
            .iter()
            .find(|control| control["controller"] == controller && control["position"] == position)
            .unwrap_or_else(|| panic!("missing {controller} {position}"))
    }

    #[test]
    fn ignores_other_module_ids() {
        let ignored = event(
            "key_state",
            serde_json::json!({"module":{"model":"M4315","id":"other"},"control":0,"pressed":true}),
        );
        assert_eq!(map_event(&ignored), None);
    }

    #[test]
    fn image_command_skips_encoders_and_converts_png() {
        assert_eq!(
            image_command(
                DEVICE_ID,
                Some("Encoder"),
                Some(0),
                Some("data:image/png;base64,xx")
            ),
            None
        );
        let png = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]));
        let mut bytes = Vec::new();
        png.write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
        let data_url = format!("data:image/png;base64,{}", STANDARD.encode(bytes));
        let command = image_command(DEVICE_ID, Some("Keypad"), Some(0), Some(&data_url)).unwrap();
        let value: Value = serde_json::from_str(&command).unwrap();
        assert_eq!(value["action"], "set_button_image");
        assert_eq!(value["payload"]["control"], 0);
        let screen = image_command(DEVICE_ID, Some("Encoder"), Some(5), Some(&data_url)).unwrap();
        let screen: Value = serde_json::from_str(&screen).unwrap();
        assert_eq!(screen["action"], "set_screen_image");
        assert_eq!(screen["payload"]["module"]["model"], "M0064");
        let jpeg = STANDARD
            .decode(value["payload"]["data_base64"].as_str().unwrap())
            .unwrap();
        assert!(jpeg.starts_with(&[0xFF, 0xD8]));
    }

    #[test]
    fn brightness_clamps_to_protocol_range() {
        let command = brightness_command(DEVICE_ID, 255).unwrap();
        let value: Value = serde_json::from_str(&command).unwrap();
        assert_eq!(value["action"], "set_backlight");
        assert_eq!(value["payload"]["brightness"], 100);
        assert!(brightness_command("sd-other", 10).is_none());
    }
}
