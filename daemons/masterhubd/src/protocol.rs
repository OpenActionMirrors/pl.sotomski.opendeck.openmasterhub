use std::fmt;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;
use serde_json::Value;

pub const PROTOCOL_VERSION: u8 = 1;
const MAX_JPEG_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ModuleModel {
    M003K,
    M005S,
    M0064,
    M032LE,
    M4315,
}

impl fmt::Display for ModuleModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::M003K => "M003K",
            Self::M005S => "M005S",
            Self::M0064 => "M0064",
            Self::M032LE => "M032LE",
            Self::M4315 => "M4315",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleRef {
    pub model: ModuleModel,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlTarget {
    Index(u8),
    All,
}

impl<'de> Deserialize<'de> for ControlTarget {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ControlVisitor;

        impl Visitor<'_> for ControlVisitor {
            type Value = ControlTarget;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a control index or \"all\"")
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                let index =
                    u8::try_from(value).map_err(|_| E::custom("control index out of range"))?;
                Ok(ControlTarget::Index(index))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                let value =
                    u64::try_from(value).map_err(|_| E::custom("control index out of range"))?;
                self.visit_u64(value)
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value == "all" {
                    Ok(ControlTarget::All)
                } else {
                    Err(E::custom("control must be an integer or \"all\""))
                }
            }
        }

        deserializer.deserialize_any(ControlVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightingMode {
    Disable,
    Static,
    Breathing,
    Strobe,
    Spring,
    Cycling,
    Random,
    Wave,
    Water,
    Rainbow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum KnobDirection {
    Cw,
    Ccw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RollerDirection {
    Up,
    Down,
}

#[derive(Debug)]
pub struct ProtocolError {
    pub id: Option<String>,
    pub message: String,
}

#[derive(Debug)]
pub enum ParsedMessage {
    Trigger {
        id: Option<String>,
        action: &'static str,
        payload: Value,
    },
    Command {
        id: Option<String>,
        command: OutputCommand,
    },
}

/// Output accepted by [`CommandSink`]. The USB backend writes M4315 button
/// images, M0064 dial screens, and LED reports for M003K, M005S, and M0064.
#[allow(dead_code)]
#[derive(Debug)]
pub enum OutputCommand {
    ButtonImage {
        module: ModuleRef,
        control: u8,
        jpeg: Vec<u8>,
    },
    ScreenImage {
        module: ModuleRef,
        jpeg: Vec<u8>,
    },
    ButtonColor {
        module: ModuleRef,
        control: u8,
        r: u8,
        g: u8,
        b: u8,
    },
    ScreenColor {
        module: ModuleRef,
        r: u8,
        g: u8,
        b: u8,
    },
    Backlight {
        module: ModuleRef,
        brightness: u8,
    },
    LedMode {
        module: ModuleRef,
        control: ControlTarget,
        mode: LightingMode,
        save: bool,
    },
    LedColor {
        module: ModuleRef,
        control: ControlTarget,
        r: u8,
        g: u8,
        b: u8,
        save: bool,
    },
    LedBrightness {
        module: ModuleRef,
        control: ControlTarget,
        brightness: u8,
    },
}

impl OutputCommand {
    #[cfg(test)]
    pub fn action(&self) -> &'static str {
        match self {
            Self::ButtonImage { .. } => "set_button_image",
            Self::ScreenImage { .. } => "set_screen_image",
            Self::ButtonColor { .. } => "set_button_color",
            Self::ScreenColor { .. } => "set_screen_color",
            Self::Backlight { .. } => "set_backlight",
            Self::LedMode { .. } => "set_led_mode",
            Self::LedColor { .. } => "set_led_color",
            Self::LedBrightness { .. } => "set_led_brightness",
        }
    }

    /// Event a simulator should paint. Other commands stay on the sender's result only.
    pub fn simulator_event(&self) -> Option<(&'static str, Value)> {
        match self {
            Self::ButtonImage {
                module,
                control,
                jpeg,
            } => Some((
                "set_button_image",
                serde_json::json!({
                    "module": { "model": module.model.to_string(), "id": module.id },
                    "control": control,
                    "data_base64": STANDARD.encode(jpeg),
                }),
            )),
            _ => None,
        }
    }
}

pub trait CommandSink: Send + Sync {
    fn accept(&self, command: &OutputCommand);
}

/// `modules` is the open set, in a fixed order. `usb` is true when that list
/// is not empty. `sites` maps a module name to its pogo-site key when the USB
/// port is known.
pub fn hello_json(modules: &[String], sites: &serde_json::Map<String, Value>) -> String {
    serde_json::json!({
        "v": PROTOCOL_VERSION,
        "type": "hello",
        "daemon": "masterhubd",
        "usb": !modules.is_empty(),
        "modules": modules,
        "sites": sites,
    })
    .to_string()
}

pub fn event_json(action: &str, payload: &Value) -> String {
    serde_json::json!({
        "v": PROTOCOL_VERSION,
        "type": "event",
        "action": action,
        "payload": payload,
    })
    .to_string()
}

pub fn result_json(id: Option<&str>) -> String {
    match id {
        Some(id) => serde_json::json!({
            "v": PROTOCOL_VERSION,
            "type": "result",
            "id": id,
            "ok": true,
        }),
        None => serde_json::json!({
            "v": PROTOCOL_VERSION,
            "type": "result",
            "ok": true,
        }),
    }
    .to_string()
}

pub fn error_json(id: Option<&str>, message: &str) -> String {
    match id {
        Some(id) => serde_json::json!({
            "v": PROTOCOL_VERSION,
            "type": "error",
            "id": id,
            "message": message,
        }),
        None => serde_json::json!({
            "v": PROTOCOL_VERSION,
            "type": "error",
            "message": message,
        }),
    }
    .to_string()
}

pub fn parse_client_message(text: &str) -> Result<ParsedMessage, ProtocolError> {
    let value: Value = serde_json::from_str(text).map_err(|_| error(None, "invalid json"))?;
    let Some(object) = value.as_object() else {
        return Err(error(None, "message must be an object"));
    };

    let id = match object.get("id") {
        None => None,
        Some(Value::String(id)) if !id.is_empty() => Some(id.clone()),
        Some(Value::String(_)) => return Err(error(None, "id must be a non-empty string")),
        Some(_) => return Err(error(None, "id must be a string")),
    };

    for key in object.keys() {
        if !matches!(key.as_str(), "v" | "type" | "id" | "action" | "payload") {
            return Err(error(id, format!("unknown field {key}")));
        }
    }

    match object.get("v") {
        Some(Value::Number(number)) if number.as_u64() == Some(u64::from(PROTOCOL_VERSION)) => {}
        None => return Err(error(id, "missing protocol version")),
        Some(_) => return Err(error(id, "unsupported protocol version")),
    }

    let kind = match object.get("type") {
        Some(Value::String(kind)) => kind.clone(),
        Some(_) => return Err(error(id, "type must be a string")),
        None => return Err(error(id, "missing type")),
    };
    let action = match object.get("action") {
        Some(Value::String(action)) => action.clone(),
        Some(_) => return Err(error(id, "action must be a string")),
        None => return Err(error(id, "missing action")),
    };
    let payload = match object.get("payload") {
        Some(payload @ Value::Object(_)) => payload.clone(),
        Some(_) => return Err(error(id, "payload must be an object")),
        None => return Err(error(id, "missing payload")),
    };

    match kind.as_str() {
        "trigger" => parse_trigger(id, &action, payload),
        "command" => parse_command(id, &action, payload),
        other => Err(error(id, format!("unknown message type {other}"))),
    }
}

fn parse_trigger(
    id: Option<String>,
    action: &str,
    payload: Value,
) -> Result<ParsedMessage, ProtocolError> {
    match action {
        "key_state" => {
            let parsed: KeyStatePayload = from_payload(&id, payload.clone())?;
            check(id.clone(), validate_key_state(&parsed))?;
            Ok(trigger(id, "key_state", payload))
        }
        "knob_rotated" => {
            let parsed: KnobRotatedPayload = from_payload(&id, payload.clone())?;
            check(id.clone(), validate_knob_rotated(&parsed))?;
            Ok(trigger(id, "knob_rotated", payload))
        }
        "knob_state" => {
            let parsed: KnobStatePayload = from_payload(&id, payload.clone())?;
            check(id.clone(), validate_knob_state(&parsed))?;
            Ok(trigger(id, "knob_state", payload))
        }
        "slider_moved" => {
            let parsed: SliderMovedPayload = from_payload(&id, payload.clone())?;
            check(id.clone(), validate_slider_moved(&parsed))?;
            Ok(trigger(id, "slider_moved", payload))
        }
        "roller_rotated" => {
            let parsed: RollerRotatedPayload = from_payload(&id, payload.clone())?;
            check(id.clone(), validate_roller_rotated(&parsed))?;
            Ok(trigger(id, "roller_rotated", payload))
        }
        "set_button_image" | "set_screen_image" | "set_button_color" | "set_screen_color"
        | "set_backlight" | "set_led_mode" | "set_led_color" | "set_led_brightness" => {
            Err(error(id, format!("{action} must be sent as a command")))
        }
        other => Err(error(id, format!("unknown action \"{other}\""))),
    }
}

fn parse_command(
    id: Option<String>,
    action: &str,
    payload: Value,
) -> Result<ParsedMessage, ProtocolError> {
    let command = match action {
        "set_button_image" => {
            let parsed: ButtonImagePayload = from_payload(&id, payload)?;
            check(id.clone(), prepare_module(&parsed.module))?;
            check(
                id.clone(),
                require_model("set_button_image", &parsed.module, ModuleModel::M4315),
            )?;
            check(
                id.clone(),
                check_index(parsed.module.model, parsed.control, 14),
            )?;
            let jpeg = check(id.clone(), decode_jpeg(&parsed.data_base64))?;
            OutputCommand::ButtonImage {
                module: parsed.module,
                control: parsed.control,
                jpeg,
            }
        }
        "set_screen_image" => {
            let parsed: ScreenImagePayload = from_payload(&id, payload)?;
            check(id.clone(), prepare_module(&parsed.module))?;
            check(
                id.clone(),
                require_models(
                    "set_screen_image",
                    &parsed.module,
                    &[ModuleModel::M4315, ModuleModel::M0064],
                ),
            )?;
            let jpeg = check(id.clone(), decode_jpeg(&parsed.data_base64))?;
            OutputCommand::ScreenImage {
                module: parsed.module,
                jpeg,
            }
        }
        "set_button_color" => {
            let parsed: ButtonColorPayload = from_payload(&id, payload)?;
            check(id.clone(), prepare_module(&parsed.module))?;
            check(
                id.clone(),
                require_model("set_button_color", &parsed.module, ModuleModel::M4315),
            )?;
            check(
                id.clone(),
                check_index(parsed.module.model, parsed.control, 14),
            )?;
            OutputCommand::ButtonColor {
                module: parsed.module,
                control: parsed.control,
                r: parsed.r,
                g: parsed.g,
                b: parsed.b,
            }
        }
        "set_screen_color" => {
            let parsed: ScreenColorPayload = from_payload(&id, payload)?;
            check(id.clone(), prepare_module(&parsed.module))?;
            check(
                id.clone(),
                require_model("set_screen_color", &parsed.module, ModuleModel::M4315),
            )?;
            OutputCommand::ScreenColor {
                module: parsed.module,
                r: parsed.r,
                g: parsed.g,
                b: parsed.b,
            }
        }
        "set_backlight" => {
            let parsed: BacklightPayload = from_payload(&id, payload)?;
            check(id.clone(), prepare_module(&parsed.module))?;
            check(
                id.clone(),
                require_model("set_backlight", &parsed.module, ModuleModel::M4315),
            )?;
            let brightness = check(id.clone(), check_percent("brightness", parsed.brightness))?;
            OutputCommand::Backlight {
                module: parsed.module,
                brightness,
            }
        }
        "set_led_mode" => {
            let parsed: LedModePayload = from_payload(&id, payload)?;
            check(id.clone(), prepare_module(&parsed.module))?;
            check(
                id.clone(),
                check_led_control("set_led_mode", parsed.module.model, parsed.control),
            )?;
            OutputCommand::LedMode {
                module: parsed.module,
                control: parsed.control,
                mode: parsed.mode,
                save: parsed.save,
            }
        }
        "set_led_color" => {
            let parsed: LedColorPayload = from_payload(&id, payload)?;
            check(id.clone(), prepare_module(&parsed.module))?;
            check(
                id.clone(),
                check_led_control("set_led_color", parsed.module.model, parsed.control),
            )?;
            OutputCommand::LedColor {
                module: parsed.module,
                control: parsed.control,
                r: parsed.r,
                g: parsed.g,
                b: parsed.b,
                save: parsed.save,
            }
        }
        "set_led_brightness" => {
            let parsed: LedBrightnessPayload = from_payload(&id, payload)?;
            check(id.clone(), prepare_module(&parsed.module))?;
            check(
                id.clone(),
                check_led_control("set_led_brightness", parsed.module.model, parsed.control),
            )?;
            let brightness = check(id.clone(), check_percent("brightness", parsed.brightness))?;
            OutputCommand::LedBrightness {
                module: parsed.module,
                control: parsed.control,
                brightness,
            }
        }
        "key_state" | "knob_rotated" | "knob_state" | "slider_moved" | "roller_rotated" => {
            return Err(error(id, format!("{action} must be sent as a trigger")));
        }
        other => return Err(error(id, format!("unknown action \"{other}\""))),
    };
    Ok(ParsedMessage::Command { id, command })
}

fn trigger(id: Option<String>, action: &'static str, payload: Value) -> ParsedMessage {
    ParsedMessage::Trigger {
        id,
        action,
        payload,
    }
}

fn error(id: Option<String>, message: impl Into<String>) -> ProtocolError {
    ProtocolError {
        id,
        message: message.into(),
    }
}

fn check<T>(id: Option<String>, result: Result<T, String>) -> Result<T, ProtocolError> {
    result.map_err(|message| error(id, message))
}

fn from_payload<T: for<'de> Deserialize<'de>>(
    id: &Option<String>,
    payload: Value,
) -> Result<T, ProtocolError> {
    serde_json::from_value(payload).map_err(|err| error(id.clone(), err.to_string()))
}

fn prepare_module(module: &ModuleRef) -> Result<(), String> {
    if module.id.is_empty() {
        Err("module id must be non-empty".into())
    } else {
        Ok(())
    }
}

fn require_model(action: &str, module: &ModuleRef, expected: ModuleModel) -> Result<(), String> {
    require_models(action, module, &[expected])
}

fn require_models(
    action: &str,
    module: &ModuleRef,
    expected: &[ModuleModel],
) -> Result<(), String> {
    if expected.contains(&module.model) {
        Ok(())
    } else {
        let names = expected
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" or ");
        Err(format!("{action} requires module model {names}"))
    }
}

fn check_index(model: ModuleModel, control: u8, max: u8) -> Result<(), String> {
    if control <= max {
        Ok(())
    } else {
        Err(format!("control {control} is out of range for {model}"))
    }
}

fn check_percent(name: &str, value: u8) -> Result<u8, String> {
    if value <= 100 {
        Ok(value)
    } else {
        Err(format!("{name} must be 0..=100"))
    }
}

fn check_led_control(
    action: &str,
    model: ModuleModel,
    control: ControlTarget,
) -> Result<(), String> {
    match (model, control) {
        (ModuleModel::M003K, ControlTarget::All) => Ok(()),
        (ModuleModel::M003K, ControlTarget::Index(index)) if index <= 2 => Ok(()),
        (ModuleModel::M003K, ControlTarget::Index(index)) => {
            Err(format!("control {index} is out of range for M003K"))
        }
        (ModuleModel::M005S, ControlTarget::All) => Ok(()),
        (ModuleModel::M005S, ControlTarget::Index(index)) if index <= 4 => Ok(()),
        (ModuleModel::M005S, ControlTarget::Index(index)) => {
            Err(format!("control {index} is out of range for M005S"))
        }
        (ModuleModel::M0064, ControlTarget::Index(0)) => Ok(()),
        (ModuleModel::M0064, ControlTarget::Index(index)) => {
            Err(format!("control {index} is out of range for M0064"))
        }
        (ModuleModel::M0064, ControlTarget::All) => {
            Err("control \"all\" is not valid for M0064".into())
        }
        (other, _) => Err(format!(
            "{action} requires module model M003K, M005S, or M0064, not {other}"
        )),
    }
}

fn decode_jpeg(data_base64: &str) -> Result<Vec<u8>, String> {
    let bytes = STANDARD
        .decode(data_base64)
        .map_err(|_| "invalid base64".to_string())?;
    if bytes.len() > MAX_JPEG_BYTES {
        return Err("image exceeds 64 KiB".into());
    }
    if bytes.len() < 2 || bytes[0] != 0xff || bytes[1] != 0xd8 {
        return Err("image must be a JPEG".into());
    }
    Ok(bytes)
}

fn validate_key_state(payload: &KeyStatePayload) -> Result<(), String> {
    prepare_module(&payload.module)?;
    require_model("key_state", &payload.module, ModuleModel::M4315)?;
    check_index(payload.module.model, payload.control, 14)?;
    let _ = payload.pressed;
    Ok(())
}

fn validate_knob_rotated(payload: &KnobRotatedPayload) -> Result<(), String> {
    prepare_module(&payload.module)?;
    validate_knob_module("knob_rotated", &payload.module, payload.control)?;
    match payload.direction {
        KnobDirection::Cw | KnobDirection::Ccw => Ok(()),
    }
}

fn validate_knob_state(payload: &KnobStatePayload) -> Result<(), String> {
    prepare_module(&payload.module)?;
    validate_knob_module("knob_state", &payload.module, payload.control)?;
    let _ = payload.pressed;
    Ok(())
}

fn validate_knob_module(action: &str, module: &ModuleRef, control: u8) -> Result<(), String> {
    match module.model {
        ModuleModel::M003K => check_index(module.model, control, 2),
        ModuleModel::M0064 => check_index(module.model, control, 0),
        _ => Err(format!("{action} requires module model M003K or M0064")),
    }
}

fn validate_slider_moved(payload: &SliderMovedPayload) -> Result<(), String> {
    prepare_module(&payload.module)?;
    require_model("slider_moved", &payload.module, ModuleModel::M005S)?;
    check_index(payload.module.model, payload.control, 4)?;
    check_percent("value", payload.value).map(|_| ())
}

fn validate_roller_rotated(payload: &RollerRotatedPayload) -> Result<(), String> {
    prepare_module(&payload.module)?;
    require_model("roller_rotated", &payload.module, ModuleModel::M032LE)?;
    check_index(payload.module.model, payload.control, 1)?;
    match payload.direction {
        RollerDirection::Up | RollerDirection::Down => Ok(()),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyStatePayload {
    module: ModuleRef,
    control: u8,
    pressed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KnobRotatedPayload {
    module: ModuleRef,
    control: u8,
    direction: KnobDirection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KnobStatePayload {
    module: ModuleRef,
    control: u8,
    pressed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SliderMovedPayload {
    module: ModuleRef,
    control: u8,
    value: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RollerRotatedPayload {
    module: ModuleRef,
    control: u8,
    direction: RollerDirection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ButtonImagePayload {
    module: ModuleRef,
    control: u8,
    data_base64: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenImagePayload {
    module: ModuleRef,
    data_base64: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ButtonColorPayload {
    module: ModuleRef,
    control: u8,
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenColorPayload {
    module: ModuleRef,
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BacklightPayload {
    module: ModuleRef,
    brightness: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LedModePayload {
    module: ModuleRef,
    control: ControlTarget,
    mode: LightingMode,
    #[serde(default)]
    save: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LedColorPayload {
    module: ModuleRef,
    control: ControlTarget,
    r: u8,
    g: u8,
    b: u8,
    #[serde(default)]
    save: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LedBrightnessPayload {
    module: ModuleRef,
    control: ControlTarget,
    brightness: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg_b64() -> String {
        STANDARD.encode([0xff, 0xd8, 0xff, 0xd9])
    }

    fn trigger(action: &str, payload: &str) -> String {
        format!(r#"{{"v":1,"type":"trigger","action":"{action}","payload":{payload}}}"#)
    }

    fn command(action: &str, payload: &str) -> String {
        format!(r#"{{"v":1,"type":"command","action":"{action}","payload":{payload}}}"#)
    }

    #[test]
    fn accepts_each_input_action() {
        let cases = [
            trigger(
                "key_state",
                r#"{"module":{"model":"M4315","id":"keys"},"control":0,"pressed":true}"#,
            ),
            trigger(
                "key_state",
                r#"{"module":{"model":"M4315","id":"keys"},"control":14,"pressed":false}"#,
            ),
            trigger(
                "knob_rotated",
                r#"{"module":{"model":"M003K","id":"knobs"},"control":2,"direction":"cw"}"#,
            ),
            trigger(
                "knob_rotated",
                r#"{"module":{"model":"M003K","id":"knobs"},"control":0,"direction":"ccw"}"#,
            ),
            trigger(
                "knob_state",
                r#"{"module":{"model":"M003K","id":"knobs"},"control":1,"pressed":false}"#,
            ),
            trigger(
                "slider_moved",
                r#"{"module":{"model":"M005S","id":"faders"},"control":4,"value":0}"#,
            ),
            trigger(
                "slider_moved",
                r#"{"module":{"model":"M005S","id":"faders"},"control":0,"value":100}"#,
            ),
            trigger(
                "knob_rotated",
                r#"{"module":{"model":"M0064","id":"base"},"control":0,"direction":"cw"}"#,
            ),
            trigger(
                "knob_state",
                r#"{"module":{"model":"M0064","id":"base"},"control":0,"pressed":true}"#,
            ),
            trigger(
                "roller_rotated",
                r#"{"module":{"model":"M032LE","id":"rollers"},"control":1,"direction":"down"}"#,
            ),
            trigger(
                "roller_rotated",
                r#"{"module":{"model":"M032LE","id":"rollers"},"control":0,"direction":"up"}"#,
            ),
        ];
        for case in cases {
            parse_client_message(&case).unwrap_or_else(|err| panic!("{case}: {}", err.message));
        }
    }

    #[test]
    fn accepts_each_output_command() {
        let jpeg = jpeg_b64();
        let cases = vec![
            command(
                "set_button_image",
                &format!(
                    r#"{{"module":{{"model":"M4315","id":"keys"}},"control":14,"data_base64":"{jpeg}"}}"#
                ),
            ),
            command(
                "set_screen_image",
                &format!(r#"{{"module":{{"model":"M4315","id":"keys"}},"data_base64":"{jpeg}"}}"#),
            ),
            command(
                "set_button_color",
                r#"{"module":{"model":"M4315","id":"keys"},"control":0,"r":51,"g":36,"b":63}"#,
            ),
            command(
                "set_screen_color",
                r#"{"module":{"model":"M4315","id":"keys"},"r":0,"g":0,"b":255}"#,
            ),
            command(
                "set_backlight",
                r#"{"module":{"model":"M4315","id":"keys"},"brightness":100}"#,
            ),
            command(
                "set_backlight",
                r#"{"module":{"model":"M4315","id":"keys"},"brightness":0}"#,
            ),
            command(
                "set_led_mode",
                r#"{"module":{"model":"M003K","id":"knobs"},"control":"all","mode":"rainbow"}"#,
            ),
            command(
                "set_led_mode",
                r#"{"module":{"model":"M005S","id":"faders"},"control":4,"mode":"static","save":true}"#,
            ),
            command(
                "set_led_mode",
                r#"{"module":{"model":"M0064","id":"base"},"control":0,"mode":"disable","save":false}"#,
            ),
            command(
                "set_led_color",
                r#"{"module":{"model":"M003K","id":"knobs"},"control":2,"r":255,"g":0,"b":0,"save":true}"#,
            ),
            command(
                "set_led_brightness",
                r#"{"module":{"model":"M005S","id":"faders"},"control":"all","brightness":80}"#,
            ),
            command(
                "set_led_brightness",
                r#"{"module":{"model":"M0064","id":"base"},"control":0,"brightness":1}"#,
            ),
        ];
        for case in cases {
            parse_client_message(&case).unwrap_or_else(|err| panic!("{case}: {}", err.message));
        }
    }

    #[test]
    fn command_fields_survive_parsing() {
        let jpeg = jpeg_b64();
        let message = command(
            "set_button_image",
            &format!(
                r#"{{"module":{{"model":"M4315","id":"keys"}},"control":3,"data_base64":"{jpeg}"}}"#
            ),
        );
        let ParsedMessage::Command {
            command: button_image,
            ..
        } = parse_client_message(&message).unwrap()
        else {
            panic!("expected command");
        };
        match button_image {
            OutputCommand::ButtonImage { control, jpeg, .. } => {
                assert_eq!(control, 3);
                assert_eq!(jpeg, vec![0xff, 0xd8, 0xff, 0xd9]);
            }
            other => panic!("unexpected {other:?}"),
        }

        let message = command(
            "set_led_mode",
            r#"{"module":{"model":"M003K","id":"knobs"},"control":"all","mode":"breathing"}"#,
        );
        let ParsedMessage::Command {
            command: led_mode, ..
        } = parse_client_message(&message).unwrap()
        else {
            panic!("expected command");
        };
        match led_mode {
            OutputCommand::LedMode {
                control,
                mode,
                save,
                ..
            } => {
                assert_eq!(control, ControlTarget::All);
                assert_eq!(mode, LightingMode::Breathing);
                assert!(!save);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_messages() {
        let too_big = STANDARD.encode([0xff, 0xd8].repeat((MAX_JPEG_BYTES / 2) + 1));
        let cases = [
            (
                trigger(
                    "key_state",
                    r#"{"module":{"model":"M003K","id":"knobs"},"control":0,"pressed":true}"#,
                ),
                "key_state requires module model M4315",
            ),
            (
                trigger(
                    "key_state",
                    r#"{"module":{"model":"M4315","id":"keys"},"control":15,"pressed":true}"#,
                ),
                "control 15 is out of range for M4315",
            ),
            (
                trigger(
                    "knob_rotated",
                    r#"{"module":{"model":"M003K","id":"knobs"},"control":3,"direction":"cw"}"#,
                ),
                "control 3 is out of range for M003K",
            ),
            (
                trigger(
                    "knob_rotated",
                    r#"{"module":{"model":"M0064","id":"base"},"control":1,"direction":"cw"}"#,
                ),
                "control 1 is out of range for M0064",
            ),
            (
                trigger(
                    "knob_rotated",
                    r#"{"module":{"model":"M003K","id":"knobs"},"control":0,"direction":"left"}"#,
                ),
                "unknown variant",
            ),
            (
                trigger(
                    "slider_moved",
                    r#"{"module":{"model":"M005S","id":"faders"},"control":0,"value":101}"#,
                ),
                "value must be 0..=100",
            ),
            (
                trigger(
                    "roller_rotated",
                    r#"{"module":{"model":"M032LE","id":"rollers"},"control":2,"direction":"up"}"#,
                ),
                "control 2 is out of range for M032LE",
            ),
            (
                trigger(
                    "knob_state",
                    r#"{"module":{"model":"M003K","id":""},"control":0,"pressed":true}"#,
                ),
                "module id must be non-empty",
            ),
            (
                trigger("spin", r#"{"module":{"model":"M003K","id":"knobs"}}"#),
                "unknown action \"spin\"",
            ),
            (
                command(
                    "key_state",
                    r#"{"module":{"model":"M4315","id":"keys"},"control":0,"pressed":true}"#,
                ),
                "key_state must be sent as a trigger",
            ),
            (
                trigger(
                    "set_led_color",
                    r#"{"module":{"model":"M003K","id":"knobs"},"control":0,"r":1,"g":2,"b":3}"#,
                ),
                "set_led_color must be sent as a command",
            ),
            (
                r#"{"v":1,"type":"trigger","action":"knob_rotated","payload":{"module":{"model":"M003K","id":"knobs"},"control":0,"direction":"cw"},"extra":1}"#.to_string(),
                "unknown field extra",
            ),
            (r#"{"type":"trigger"}"#.to_string(), "missing protocol version"),
            (
                r#"{"v":2,"type":"trigger","action":"knob_rotated","payload":{}}"#.to_string(),
                "unsupported protocol version",
            ),
            (
                command(
                    "set_button_image",
                    r#"{"module":{"model":"M4315","id":"keys"},"control":0,"data_base64":"AAAA"}"#,
                ),
                "image must be a JPEG",
            ),
            (
                command(
                    "set_button_image",
                    r#"{"module":{"model":"M4315","id":"keys"},"control":0,"data_base64":"!!!!"}"#,
                ),
                "invalid base64",
            ),
            (
                command(
                    "set_screen_image",
                    &format!(
                        r#"{{"module":{{"model":"M4315","id":"keys"}},"data_base64":"{too_big}"}}"#
                    ),
                ),
                "image exceeds 64 KiB",
            ),
            (
                command(
                    "set_led_mode",
                    r#"{"module":{"model":"M0064","id":"base"},"control":"all","mode":"static"}"#,
                ),
                "control \"all\" is not valid for M0064",
            ),
            (
                command(
                    "set_led_brightness",
                    r#"{"module":{"model":"M003K","id":"knobs"},"control":0,"brightness":101}"#,
                ),
                "brightness must be 0..=100",
            ),
            (
                command(
                    "set_led_mode",
                    r#"{"module":{"model":"M4315","id":"keys"},"control":0,"mode":"wave"}"#,
                ),
                "set_led_mode requires module model M003K, M005S, or M0064, not M4315",
            ),
            (
                command(
                    "set_backlight",
                    r#"{"module":{"model":"M003K","id":"knobs"},"brightness":10}"#,
                ),
                "set_backlight requires module model M4315",
            ),
            (
                command(
                    "set_led_brightness",
                    r#"{"module":{"model":"M0064","id":"base"},"control":0,"brightness":10,"save":true}"#,
                ),
                "unknown field",
            ),
            ("not json".to_string(), "invalid json"),
            (
                r#"{"v":1,"type":"trigger","id":"","action":"knob_rotated","payload":{}}"#.to_string(),
                "id must be a non-empty string",
            ),
        ];

        for (case, needle) in cases {
            let err = parse_client_message(&case).expect_err(&case);
            assert!(
                err.message.contains(needle),
                "{case}\n  message: {}\n  expected: {needle}",
                err.message
            );
        }
    }

    #[test]
    fn minimal_jpeg_matches_the_documented_example() {
        assert_eq!(jpeg_b64(), "/9j/2Q==");
    }
}
