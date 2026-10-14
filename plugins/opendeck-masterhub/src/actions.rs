use std::collections::HashMap;

use serde_json::{json, Value};

use crate::dial_feedback;
use crate::map::{self, BASE, KEYS, KNOBS};

pub const SET_COLOR: &str = "pl.sotomski.opendeck.openmasterhub.rgbcolor";
pub const SET_MODE: &str = "pl.sotomski.opendeck.openmasterhub.rgbmode";
pub const SET_BRIGHTNESS: &str = "pl.sotomski.opendeck.openmasterhub.setbrightness";

const MODES: &[&str] = &[
    "disable",
    "static",
    "breathing",
    "strobe",
    "spring",
    "cycling",
    "random",
    "wave",
    "water",
    "rainbow",
];
const DEFAULT_MODE: &str = "static";

const DEFAULT_HUE: f64 = 0.0;
const DEFAULT_SATURATION: f64 = 100.0;
const DEFAULT_VALUE: f64 = 100.0;
const DEFAULT_LIGHTNESS: f64 = 50.0;
const DEFAULT_BRIGHTNESS: u8 = 100;
const DEFAULT_STEP: u8 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Base,
    Top,
    Left,
    Right,
}

/// Commands for a plugin action press. Unknown actions produce nothing.
pub fn commands(action: &str, settings: &Value) -> Vec<String> {
    match action {
        SET_COLOR => color_commands(settings),
        SET_MODE => mode_commands(settings),
        SET_BRIGHTNESS => brightness_commands(settings),
        _ => Vec::new(),
    }
}

fn color_commands(settings: &Value) -> Vec<String> {
    let Some((r, g, b)) = rgb_channels(settings) else {
        return Vec::new();
    };
    let mut commands = Vec::new();
    each_led(settings, |model, id, control| {
        commands.push(led_color(model, id, control, r, g, b));
    });
    commands
}

fn mode_commands(settings: &Value) -> Vec<String> {
    let Some(mode) = mode_to_send(settings) else {
        return Vec::new();
    };
    let mut commands = Vec::new();
    each_led(settings, |model, id, control| {
        commands.push(led_mode(model, id, control, mode));
    });
    commands
}

fn each_led(settings: &Value, mut write: impl FnMut(&str, &str, Value)) {
    let targets = targets_from(settings);
    let knobs: Vec<u8> = [Target::Top, Target::Left, Target::Right]
        .into_iter()
        .filter(|target| targets.contains(target))
        .map(knob_index)
        .collect();
    if knobs.len() == 3 {
        write("M003K", KNOBS, json!("all"));
    } else {
        for control in knobs {
            write("M003K", KNOBS, json!(control));
        }
    }
    if targets.contains(&Target::Base) {
        write("M0064", BASE, json!(0));
    }
}

fn brightness_commands(settings: &Value) -> Vec<String> {
    let Some(source) = active_source(settings) else {
        return Vec::new();
    };
    let Some(level) = stored_level(settings, source) else {
        return Vec::new();
    };
    level_commands(source, level)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Keypad,
    Rgb,
    Toggle,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Light {
    Keypad,
    Rgb,
}

fn output_mode(settings: &Value) -> Option<OutputMode> {
    let Some(object) = settings.as_object() else {
        return Some(OutputMode::Keypad);
    };
    match object.get("output") {
        None => Some(OutputMode::Keypad),
        Some(Value::String(output)) => match output.as_str() {
            "keypad" => Some(OutputMode::Keypad),
            "rgb" => Some(OutputMode::Rgb),
            "toggle" => Some(OutputMode::Toggle),
            _ => None,
        },
        _ => None,
    }
}

fn remembered_source(settings: &Value) -> Option<Light> {
    let Some(object) = settings.as_object() else {
        return Some(Light::Keypad);
    };
    match object.get("source") {
        None => Some(Light::Keypad),
        Some(Value::String(source)) => match source.as_str() {
            "keypad" => Some(Light::Keypad),
            "rgb" => Some(Light::Rgb),
            _ => None,
        },
        _ => None,
    }
}

fn active_source(settings: &Value) -> Option<Light> {
    match output_mode(settings)? {
        OutputMode::Keypad => Some(Light::Keypad),
        OutputMode::Rgb => Some(Light::Rgb),
        OutputMode::Toggle => remembered_source(settings),
    }
}

fn screen_label(settings: &Value) -> Option<&'static str> {
    if output_mode(settings) != Some(OutputMode::Toggle) {
        return None;
    }
    Some(match active_source(settings)? {
        Light::Keypad => "KEYPAD",
        Light::Rgb => "RGB",
    })
}

/// Settings after a toggle-after-press. Keypad and RGB outputs do not flip.
pub fn flip_brightness_source(settings: &Value) -> Option<Value> {
    if output_mode(settings)? != OutputMode::Toggle {
        return None;
    }
    let next = match remembered_source(settings)? {
        Light::Keypad => "rgb",
        Light::Rgb => "keypad",
    };
    let mut updated = if settings.is_object() {
        settings.clone()
    } else {
        json!({})
    };
    updated["output"] = json!("toggle");
    updated["source"] = json!(next);
    Some(updated)
}

fn level_commands(source: Light, level: u8) -> Vec<String> {
    match source {
        Light::Keypad => vec![json!({
            "v": 1,
            "type": "command",
            "action": "set_backlight",
            "payload": {
                "module": { "model": "M4315", "id": KEYS },
                "brightness": level,
            }
        })
        .to_string()],
        Light::Rgb => vec![
            led_brightness("M003K", KNOBS, json!("all"), level),
            led_brightness("M0064", BASE, json!(0), level),
        ],
    }
}

fn led_brightness(model: &str, id: &str, control: Value, level: u8) -> String {
    json!({
        "v": 1,
        "type": "command",
        "action": "set_led_brightness",
        "payload": {
            "module": { "model": model, "id": id },
            "control": control,
            "brightness": level,
        }
    })
    .to_string()
}

fn mode_from(settings: &Value) -> Option<&'static str> {
    let Some(object) = settings.as_object() else {
        return Some(DEFAULT_MODE);
    };
    match object.get("mode") {
        None => Some(DEFAULT_MODE),
        Some(Value::String(mode)) => MODES.iter().copied().find(|known| *known == mode),
        _ => None,
    }
}

fn mode_action(settings: &Value) -> Option<&'static str> {
    let Some(object) = settings.as_object() else {
        return Some("set");
    };
    match object.get("action") {
        None => Some("set"),
        Some(Value::String(action)) => match action.as_str() {
            "set" => Some("set"),
            "toggle" => Some("toggle"),
            "cycle" => Some("cycle"),
            _ => None,
        },
        _ => None,
    }
}

/// The mode a press sends. `set` uses the saved mode. On/off and toggle mode
/// use it after [`stepped_mode`] has moved it on.
fn mode_to_send(settings: &Value) -> Option<&'static str> {
    match mode_action(settings)? {
        "set" | "toggle" | "cycle" => mode_from(settings),
        _ => None,
    }
}

/// Effect order for toggle mode. Off stays on the on/off action.
const EFFECTS: &[&str] = &[
    "static",
    "breathing",
    "strobe",
    "spring",
    "cycling",
    "random",
    "wave",
    "water",
    "rainbow",
];

/// Settings after an on/off or toggle-mode press. `set` leaves the mode alone.
pub fn stepped_mode(settings: &Value) -> Option<Value> {
    let kind = mode_action(settings)?;
    let next = match kind {
        "toggle" => next_power(settings)?,
        "cycle" => next_effect(settings)?,
        _ => return None,
    };
    let mut updated = if settings.is_object() {
        settings.clone()
    } else {
        json!({})
    };
    updated["action"] = json!(kind);
    updated["mode"] = json!(next);
    Some(updated)
}

/// Off becomes solid. Any other mode becomes off. A new key turns the lights solid.
fn next_power(settings: &Value) -> Option<&'static str> {
    let current = if settings.get("mode").is_none() {
        "disable"
    } else {
        mode_from(settings)?
    };
    Some(if current == "disable" {
        "static"
    } else {
        "disable"
    })
}

/// The next effect after the saved one. A new key, or a key left on off, starts at solid.
fn next_effect(settings: &Value) -> Option<&'static str> {
    let Some(current) = settings.get("mode") else {
        return Some(EFFECTS[0]);
    };
    let current = current.as_str()?;
    if !MODES.contains(&current) {
        return None;
    }
    let index = EFFECTS.iter().position(|mode| *mode == current);
    Some(match index {
        Some(index) => EFFECTS[(index + 1) % EFFECTS.len()],
        None => EFFECTS[0],
    })
}

fn led_mode(model: &str, id: &str, control: Value, mode: &str) -> String {
    json!({
        "v": 1,
        "type": "command",
        "action": "set_led_mode",
        "payload": {
            "module": { "model": model, "id": id },
            "control": control,
            "mode": mode,
            "save": false,
        }
    })
    .to_string()
}

fn led_color(model: &str, id: &str, control: Value, r: u8, g: u8, b: u8) -> String {
    json!({
        "v": 1,
        "type": "command",
        "action": "set_led_color",
        "payload": {
            "module": { "model": model, "id": id },
            "control": control,
            "r": r,
            "g": g,
            "b": b,
            "save": false,
        }
    })
    .to_string()
}

fn knob_index(target: Target) -> u8 {
    match target {
        Target::Top => 0,
        Target::Left => 1,
        Target::Right => 2,
        Target::Base => 0,
    }
}

fn targets_from(settings: &Value) -> Vec<Target> {
    let Some(object) = settings.as_object() else {
        return vec![Target::Base, Target::Top, Target::Left, Target::Right];
    };
    let Some(value) = object.get("targets") else {
        return vec![Target::Base, Target::Top, Target::Left, Target::Right];
    };
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    let mut chosen = Vec::new();
    for item in items {
        let Some(name) = item.as_str() else {
            continue;
        };
        let target = match name {
            "base" => Target::Base,
            "top" => Target::Top,
            "left" => Target::Left,
            "right" => Target::Right,
            _ => continue,
        };
        if !chosen.contains(&target) {
            chosen.push(target);
        }
    }
    chosen
}

/// HSV when `v` is set, and when neither `v` nor a legacy `l` is set.
/// A saved profile that still has `l` and no `v` stays on the old HSL mapping.
fn rgb_channels(settings: &Value) -> Option<(u8, u8, u8)> {
    let object = settings.as_object();
    let h = component(object, "h", DEFAULT_HUE)?;
    let s = component(object, "s", DEFAULT_SATURATION)?;
    let h = h.clamp(0.0, 360.0);
    let s = s.clamp(0.0, 100.0);
    let legacy = object.is_some_and(|object| object.contains_key("l") && !object.contains_key("v"));
    if legacy {
        let l = component(object, "l", DEFAULT_LIGHTNESS)?;
        return Some(hsl_to_rgb(h, s, l.clamp(0.0, 100.0)));
    }
    let v = component(object, "v", DEFAULT_VALUE)?;
    Some(hsv_to_rgb(h, s, v.clamp(0.0, 100.0)))
}

/// Key image for an action whose icon depends on its settings.
/// `button` is `Some` when the placement is known. Brightness uses that to
/// decide whether the sun carries a plus or a minus.
pub fn action_image(action: &str, settings: &Value, button: Option<bool>) -> Option<String> {
    match action {
        SET_BRIGHTNESS => Some(crate::icons::data_url(&crate::icons::brightness_png(
            brightness_mark(settings, button),
        ))),
        SET_COLOR => {
            let (r, g, b) = rgb_channels(settings)?;
            Some(crate::icons::data_url(&crate::icons::color_png(r, g, b)))
        }
        _ => None,
    }
}

fn brightness_mark(settings: &Value, button: Option<bool>) -> crate::icons::SunMark {
    let on_button = button.unwrap_or_else(|| {
        settings
            .get("action")
            .and_then(|value| value.as_str())
            .is_some()
    });
    if !on_button {
        return crate::icons::SunMark::Plain;
    }
    match action_from(settings) {
        Some("decrease") => crate::icons::SunMark::Minus,
        Some("increase") => crate::icons::SunMark::Plus,
        _ => crate::icons::SunMark::Plain,
    }
}

fn component(
    object: Option<&serde_json::Map<String, Value>>,
    key: &str,
    default: f64,
) -> Option<f64> {
    let Some(object) = object else {
        return Some(default);
    };
    match object.get(key) {
        None => Some(default),
        Some(Value::Number(number)) => number.as_f64().filter(|value| value.is_finite()),
        _ => None,
    }
}

pub fn is_dial(controller: &str, column: u8) -> bool {
    controller == "Encoder" && column == map::BIG_KNOB_ENCODER
}

/// Brightness command, and the dial screen when `dial_screen` is set.
pub fn brightness_output(settings: &Value, dial_screen: bool, label: Option<&str>) -> Vec<String> {
    let Some(source) = active_source(settings) else {
        return Vec::new();
    };
    let Some(level) = stored_level(settings, source) else {
        return Vec::new();
    };
    let mut commands = level_commands(source, level);
    if dial_screen {
        if let Some(command) = dial_screen_command(level, label) {
            commands.push(command);
        }
    }
    commands
}

pub fn dial_screen_command(level: u8, label: Option<&str>) -> Option<String> {
    let jpeg = dial_feedback::jpeg(level, label)?;
    Some(
        json!({
            "v": 1,
            "type": "command",
            "action": "set_screen_image",
            "payload": {
                "module": { "model": "M0064", "id": BASE },
                "data_base64": base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    jpeg,
                ),
            }
        })
        .to_string(),
    )
}

/// Button press. `current` is the shared keypad level, not this key's saved copy.
#[cfg(test)]
fn brightness_on_button(settings: &Value, current: u8) -> Option<Value> {
    let source = active_source(settings)?;
    apply_delta(settings, current, button_delta(settings)?, source)
}

/// Encoder or dial turn. Each tick moves the shared level by `step`.
#[cfg(test)]
fn brightness_after_turn(settings: &Value, ticks: i16, current: u8) -> Option<Value> {
    let source = active_source(settings)?;
    apply_delta(settings, current, turn_delta(settings, ticks)?, source)
}

pub fn button_delta(settings: &Value) -> Option<i32> {
    let step = i32::from(step_from(settings)?);
    match action_from(settings)? {
        "increase" => Some(step),
        "decrease" => Some(-step),
        _ => None,
    }
}

pub fn turn_delta(settings: &Value, ticks: i16) -> Option<i32> {
    let step = i32::from(step_from(settings)?);
    Some(i32::from(ticks).saturating_mul(step))
}

fn apply_delta(settings: &Value, current: u8, delta: i32, source: Light) -> Option<Value> {
    let next = i32::from(current).saturating_add(delta).clamp(0, 100) as u8;
    let mut updated = if settings.is_object() {
        settings.clone()
    } else {
        json!({})
    };
    let key = match source {
        Light::Keypad => "brightness",
        Light::Rgb => "rgbBrightness",
    };
    updated[key] = json!(next);
    Some(updated)
}

/// One keypad level and one RGB level, each shared by every brightness action.
///
/// Each key otherwise keeps its own copy. An increase key saved at 100 and a
/// decrease key saved at 0 then jump that light to those ends.
#[derive(Default)]
pub struct BrightnessBook {
    keypad: Option<u8>,
    rgb: Option<u8>,
    keypad_changed: bool,
    rgb_changed: bool,
    slots: HashMap<String, Slot>,
}

#[derive(Clone)]
struct Slot {
    settings: Value,
    dial: bool,
    button: bool,
}

pub struct DialFrame {
    pub level: u8,
    pub label: Option<&'static str>,
}

pub struct BrightnessChange {
    /// Asserted in tests. The running plugin writes this value into settings.
    #[cfg_attr(not(test), allow(dead_code))]
    pub level: u8,
    pub source_settings: Value,
    /// Other dial instances whose saved level for this light must follow `level`.
    pub dials: Vec<(String, Value)>,
    pub screen: bool,
    pub label: Option<&'static str>,
}

pub struct BrightnessFlip {
    pub settings: Value,
    pub screen: Option<DialFrame>,
}

impl BrightnessBook {
    /// Remember a visible action. A dial with no shared level yet seeds it
    /// from the percent already on that screen.
    pub fn appear(
        &mut self,
        context: &str,
        settings: &Value,
        dial: bool,
        button: bool,
    ) -> Option<DialFrame> {
        if dial && self.keypad.is_none() && !self.keypad_changed {
            self.keypad = Some(brightness_from(settings).unwrap_or(DEFAULT_BRIGHTNESS));
        }
        if dial && self.rgb.is_none() && !self.rgb_changed {
            if let Some(level) = explicit_level(settings, "rgbBrightness") {
                self.rgb = Some(level);
            }
        }
        self.slots.insert(
            context.to_string(),
            Slot {
                settings: settings.clone(),
                dial,
                button,
            },
        );
        dial.then(|| self.frame(context)).flatten()
    }

    pub fn disappear(&mut self, context: &str) {
        self.slots.remove(context);
    }

    /// Inspector edits replace that instance's step and action. The shared level stays.
    pub fn note(&mut self, context: &str, settings: Value) {
        if let Some(slot) = self.slots.get_mut(context) {
            slot.settings = settings;
        }
    }

    /// `Some` after [`appear`](Self::appear). `None` when this context is not on screen yet.
    pub fn slot_button(&self, context: &str) -> Option<bool> {
        self.slots.get(context).map(|slot| slot.button)
    }

    /// Saved global levels from a previous session. A light changed in this
    /// session keeps its level. Returns a frame when a dial screen should redraw.
    pub fn apply_global(&mut self, settings: &Value) -> Option<DialFrame> {
        let mut applied = false;
        if !self.keypad_changed {
            if let Some(level) = saved_brightness(settings) {
                self.keypad = Some(level);
                applied = true;
            }
        }
        if !self.rgb_changed {
            if let Some(level) = saved_rgb_brightness(settings) {
                self.rgb = Some(level);
                applied = true;
            }
        }
        if !applied {
            return None;
        }
        let context = self
            .slots
            .iter()
            .find(|(_, slot)| slot.dial)
            .map(|(context, _)| context.clone())?;
        self.frame(&context)
    }

    #[cfg(test)]
    fn level(&self) -> Option<u8> {
        self.keypad
    }

    pub fn global_settings(&self) -> Value {
        json!({
            "brightness": self.keypad.unwrap_or(DEFAULT_BRIGHTNESS),
            "rgbBrightness": self.rgb.unwrap_or(DEFAULT_BRIGHTNESS),
        })
    }

    /// The brightness action is on the dial, so that screen shows the ring
    /// instead of the action icon and title.
    pub fn shows_dial(&self) -> bool {
        self.slots.values().any(|slot| slot.dial)
    }

    pub fn step(
        &mut self,
        context: &str,
        settings: &Value,
        delta: i32,
        dial: bool,
    ) -> Option<BrightnessChange> {
        let source = active_source(settings)?;
        let current = self.current(source, settings);
        let source_settings = apply_delta(settings, current, delta, source)?;
        let level = stored_level(&source_settings, source)?;
        self.store(source, level);
        let known = self.slots.get(context);
        let dial = known.is_some_and(|slot| slot.dial) || dial;
        let button = known.is_some_and(|slot| slot.button);
        self.slots.insert(
            context.to_string(),
            Slot {
                settings: source_settings.clone(),
                dial,
                button,
            },
        );
        let key = level_key(source);
        let mut screen = false;
        let mut dials = Vec::new();
        for (ctx, slot) in &mut self.slots {
            if !slot.dial || active_source(&slot.settings) != Some(source) {
                continue;
            }
            screen = true;
            slot.settings[key] = json!(level);
            if ctx != context {
                dials.push((ctx.clone(), slot.settings.clone()));
            }
        }
        let label = self
            .slots
            .values()
            .find(|slot| slot.dial && active_source(&slot.settings) == Some(source))
            .and_then(|slot| screen_label(&slot.settings));
        Some(BrightnessChange {
            level,
            source_settings,
            dials,
            screen,
            label,
        })
    }

    /// Encoder or dial press while the output is toggle-after-press.
    pub fn flip(&mut self, context: &str, settings: &Value, dial: bool) -> Option<BrightnessFlip> {
        let mut updated = flip_brightness_source(settings)?;
        let known = self.slots.get(context);
        let dial = known.is_some_and(|slot| slot.dial) || dial;
        let button = known.is_some_and(|slot| slot.button);
        let source = active_source(&updated)?;
        let level = self.current(source, &updated);
        if let Some(keypad) = self.keypad {
            updated["brightness"] = json!(keypad);
        }
        if let Some(rgb) = self.rgb {
            updated["rgbBrightness"] = json!(rgb);
        }
        updated[level_key(source)] = json!(level);
        self.slots.insert(
            context.to_string(),
            Slot {
                settings: updated.clone(),
                dial,
                button,
            },
        );
        let screen = dial.then(|| DialFrame {
            level,
            label: screen_label(&updated),
        });
        Some(BrightnessFlip {
            settings: updated,
            screen,
        })
    }

    fn current(&self, source: Light, settings: &Value) -> u8 {
        match source {
            Light::Keypad => self.keypad.unwrap_or_else(|| self.seed(source, settings)),
            Light::Rgb => self.rgb.unwrap_or_else(|| self.seed(source, settings)),
        }
    }

    fn store(&mut self, source: Light, level: u8) {
        match source {
            Light::Keypad => {
                self.keypad = Some(level);
                self.keypad_changed = true;
            }
            Light::Rgb => {
                self.rgb = Some(level);
                self.rgb_changed = true;
            }
        }
    }

    fn seed(&self, source: Light, settings: &Value) -> u8 {
        let key = level_key(source);
        self.slots
            .values()
            .find(|slot| slot.dial && active_source(&slot.settings) == Some(source))
            .map(|slot| explicit_level(&slot.settings, key).unwrap_or(DEFAULT_BRIGHTNESS))
            .unwrap_or_else(|| explicit_level(settings, key).unwrap_or(DEFAULT_BRIGHTNESS))
    }

    fn frame(&self, context: &str) -> Option<DialFrame> {
        let settings = {
            let slot = self.slots.get(context)?;
            if !slot.dial {
                return None;
            }
            slot.settings.clone()
        };
        let source = active_source(&settings)?;
        Some(DialFrame {
            level: self.current(source, &settings),
            label: screen_label(&settings),
        })
    }
}

fn level_key(source: Light) -> &'static str {
    match source {
        Light::Keypad => "brightness",
        Light::Rgb => "rgbBrightness",
    }
}

fn stored_level(settings: &Value, source: Light) -> Option<u8> {
    match source {
        Light::Keypad => brightness_from(settings),
        Light::Rgb => rgb_brightness_from(settings),
    }
}

/// Brightness saved in global settings. Missing means there is no saved level.
pub fn saved_brightness(settings: &Value) -> Option<u8> {
    explicit_level(settings, "brightness")
}

pub fn saved_rgb_brightness(settings: &Value) -> Option<u8> {
    explicit_level(settings, "rgbBrightness")
}

fn explicit_level(settings: &Value, key: &str) -> Option<u8> {
    let number = settings.get(key)?;
    match number {
        Value::Number(number) => {
            let value = number.as_f64().filter(|value| value.is_finite())?;
            Some(value.round().clamp(0.0, 100.0) as u8)
        }
        _ => None,
    }
}

fn step_from(settings: &Value) -> Option<u8> {
    let Some(object) = settings.as_object() else {
        return Some(DEFAULT_STEP);
    };
    match object.get("step") {
        None => Some(DEFAULT_STEP),
        Some(Value::Number(number)) => {
            let value = number.as_f64().filter(|value| value.is_finite())?;
            Some(value.round().clamp(1.0, 100.0) as u8)
        }
        _ => None,
    }
}

fn action_from(settings: &Value) -> Option<&'static str> {
    let Some(object) = settings.as_object() else {
        return Some("increase");
    };
    match object.get("action") {
        None => Some("increase"),
        Some(Value::String(action)) => match action.as_str() {
            "increase" => Some("increase"),
            "decrease" => Some("decrease"),
            _ => None,
        },
        _ => None,
    }
}

fn brightness_from(settings: &Value) -> Option<u8> {
    leveled(settings, "brightness")
}

fn rgb_brightness_from(settings: &Value) -> Option<u8> {
    leveled(settings, "rgbBrightness")
}

fn leveled(settings: &Value, key: &str) -> Option<u8> {
    let Some(object) = settings.as_object() else {
        return Some(DEFAULT_BRIGHTNESS);
    };
    match object.get(key) {
        None => Some(DEFAULT_BRIGHTNESS),
        Some(_) => explicit_level(settings, key),
    }
}

/// HSV to sRGB. `s` and `v` are percentages. Value `100` is the full color,
/// not white. White is saturation `0` at value `100`.
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0) / 60.0;
    let s = s / 100.0;
    let v = v / 100.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let channel = |value: f64| ((value + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (channel(r), channel(g), channel(b))
}

/// CSS Color 4 HSL to sRGB. `s` and `l` are percentages.
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0);
    let s = s / 100.0;
    let l = l / 100.0;
    let a = s * l.min(1.0 - l);
    let channel = |n: f64| {
        let k = (n + h / 30.0).rem_euclid(12.0);
        let value = l - a * (k - 3.0).min(9.0 - k).min(1.0).max(-1.0);
        (value * 255.0).round().clamp(0.0, 255.0) as u8
    };
    (channel(0.0), channel(8.0), channel(4.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color(settings: Value) -> Vec<Value> {
        commands(SET_COLOR, &settings)
            .into_iter()
            .map(|text| serde_json::from_str(&text).unwrap())
            .collect()
    }

    fn payload<'a>(command: &'a Value) -> &'a Value {
        &command["payload"]
    }

    #[test]
    fn primary_hues_become_rgb() {
        assert_eq!(hsl_to_rgb(0.0, 100.0, 50.0), (255, 0, 0));
        assert_eq!(hsl_to_rgb(120.0, 100.0, 50.0), (0, 255, 0));
        assert_eq!(hsl_to_rgb(240.0, 100.0, 50.0), (0, 0, 255));
        assert_eq!(hsl_to_rgb(0.0, 0.0, 100.0), (255, 255, 255));
        assert_eq!(hsl_to_rgb(0.0, 0.0, 0.0), (0, 0, 0));
        assert_eq!(hsl_to_rgb(360.0, 100.0, 50.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(0.0, 100.0, 100.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(120.0, 100.0, 100.0), (0, 255, 0));
        assert_eq!(hsv_to_rgb(0.0, 0.0, 100.0), (255, 255, 255));
        assert_eq!(hsv_to_rgb(0.0, 100.0, 0.0), (0, 0, 0));
        assert_eq!(hsv_to_rgb(0.0, 100.0, 50.0), (128, 0, 0));
    }

    #[test]
    fn all_knobs_use_one_command_and_base_is_separate() {
        let commands = color(json!({
            "targets": ["right", "base", "top", "left", "top"],
            "h": 0,
            "s": 100,
            "l": 50
        }));
        assert_eq!(commands.len(), 2);
        assert_eq!(payload(&commands[0])["module"]["model"], "M003K");
        assert_eq!(payload(&commands[0])["module"]["id"], KNOBS);
        assert_eq!(payload(&commands[0])["control"], "all");
        assert_eq!(payload(&commands[0])["r"], 255);
        assert_eq!(payload(&commands[0])["g"], 0);
        assert_eq!(payload(&commands[0])["b"], 0);
        assert_eq!(payload(&commands[0])["save"], false);
        assert_eq!(payload(&commands[1])["module"]["model"], "M0064");
        assert_eq!(payload(&commands[1])["module"]["id"], BASE);
        assert_eq!(payload(&commands[1])["control"], 0);
    }

    #[test]
    fn a_subset_of_knobs_is_addressed_one_by_one() {
        let commands = color(json!({
            "targets": ["right", "top"],
            "h": 120,
            "s": 100,
            "l": 50
        }));
        assert_eq!(commands.len(), 2);
        assert_eq!(payload(&commands[0])["control"], 0);
        assert_eq!(payload(&commands[1])["control"], 2);
        assert_eq!(payload(&commands[0])["g"], 255);
        assert_eq!(payload(&commands[0])["r"], 0);
    }

    #[test]
    fn missing_settings_use_the_inspector_defaults() {
        let commands = color(json!({}));
        assert_eq!(commands.len(), 2);
        assert_eq!(payload(&commands[0])["control"], "all");
        assert_eq!(payload(&commands[0])["r"], 255);
        assert_eq!(payload(&commands[1])["control"], 0);
    }

    #[test]
    fn an_empty_target_list_sends_nothing() {
        assert!(color(json!({"targets": []})).is_empty());
        assert!(color(json!({"targets": "all", "h": 0, "s": 0, "l": 0})).is_empty());
        assert!(commands("pl.sotomski.opendeck.openmasterhub.other", &json!({})).is_empty());
    }

    #[test]
    fn hsv_is_the_color_and_a_saved_lightness_stays_on_hsl() {
        let hsv = color(json!({"targets": ["left"], "h": 0, "s": 100, "v": 100}));
        assert_eq!(payload(&hsv[0])["r"], 255);
        assert_eq!(payload(&hsv[0])["g"], 0);
        let dim = color(json!({"targets": ["left"], "h": 0, "s": 100, "v": 0}));
        assert_eq!(payload(&dim[0])["r"], 0);
        let legacy = color(json!({"targets": ["left"], "h": 0, "s": 100, "l": 50}));
        assert_eq!(payload(&legacy[0])["r"], 255);
        assert_eq!(payload(&legacy[0])["g"], 0);
        let both = color(json!({"targets": ["left"], "h": 0, "s": 0, "l": 0, "v": 100}));
        assert_eq!(payload(&both[0])["r"], 255);
        assert_eq!(payload(&both[0])["g"], 255);
        assert_eq!(payload(&both[0])["b"], 255);
    }

    #[test]
    fn out_of_range_hsl_is_clamped() {
        let commands = color(json!({"targets": ["left"], "h": 400, "s": -10, "l": 200}));
        assert_eq!(commands.len(), 1);
        assert_eq!(payload(&commands[0])["control"], 1);
        assert_eq!(payload(&commands[0])["r"], 255);
        assert_eq!(payload(&commands[0])["g"], 255);
        assert_eq!(payload(&commands[0])["b"], 255);
    }

    #[test]
    fn keypad_brightness_clamps_and_rejects_text() {
        let command = commands(SET_BRIGHTNESS, &json!({"brightness": 140}));
        let value: Value = serde_json::from_str(&command[0]).unwrap();
        assert_eq!(value["action"], "set_backlight");
        assert_eq!(value["payload"]["module"]["id"], KEYS);
        assert_eq!(value["payload"]["brightness"], 100);

        let low = commands(SET_BRIGHTNESS, &json!({"brightness": 12.4}));
        let low: Value = serde_json::from_str(&low[0]).unwrap();
        assert_eq!(low["payload"]["brightness"], 12);

        assert!(commands(SET_BRIGHTNESS, &json!({"brightness": "80"})).is_empty());
        assert!(commands(SET_BRIGHTNESS, &json!({"output": "lamp"})).is_empty());

        let missing = commands(SET_BRIGHTNESS, &json!({}));
        let missing: Value = serde_json::from_str(&missing[0]).unwrap();
        assert_eq!(missing["payload"]["brightness"], DEFAULT_BRIGHTNESS);
    }

    #[test]
    fn turning_an_encoder_steps_keypad_brightness() {
        let up = brightness_after_turn(&json!({"brightness": 0, "step": 5}), 2, 40).unwrap();
        assert_eq!(up["brightness"], 50);
        let down = brightness_after_turn(&json!({"brightness": 2, "step": 5}), -1, 2).unwrap();
        assert_eq!(down["brightness"], 0);
        let capped = brightness_after_turn(&json!({}), 1, DEFAULT_BRIGHTNESS).unwrap();
        assert_eq!(capped["brightness"], 100);
        let command = commands(SET_BRIGHTNESS, &capped);
        let command: Value = serde_json::from_str(&command[0]).unwrap();
        assert_eq!(command["payload"]["brightness"], 100);
    }

    #[test]
    fn a_button_increases_or_decreases_by_the_step() {
        let up = brightness_on_button(
            &json!({"brightness": 100, "step": 10, "action": "increase"}),
            40,
        )
        .unwrap();
        assert_eq!(up["brightness"], 50);
        let down = brightness_on_button(
            &json!({"brightness": 0, "step": 10, "action": "decrease"}),
            40,
        )
        .unwrap();
        assert_eq!(down["brightness"], 30);
        assert!(brightness_on_button(&json!({"action": "set"}), 40).is_none());
        let missing = brightness_on_button(&json!({"brightness": 20}), 20).unwrap();
        assert_eq!(missing["brightness"], 21);
    }

    #[test]
    fn keys_share_the_dial_level_and_the_screen_follows() {
        let mut book = BrightnessBook::default();
        let dial = json!({"brightness": 60, "step": 5});
        assert_eq!(book.appear("dial", &dial, true, false).unwrap().level, 60);
        book.appear(
            "encoder",
            &json!({"brightness": 90, "step": 10}),
            false,
            false,
        );
        let increase = json!({"brightness": 100, "step": 10, "action": "increase"});
        let decrease = json!({"brightness": 0, "step": 10, "action": "decrease"});
        book.appear("up", &increase, false, true);
        book.appear("down", &decrease, false, true);

        let up = book
            .step("up", &increase, button_delta(&increase).unwrap(), false)
            .unwrap();
        assert_eq!(up.level, 70);
        assert!(up.screen);
        assert_eq!(up.dials.len(), 1);
        assert_eq!(up.dials[0].0, "dial");
        assert_eq!(up.dials[0].1["brightness"], 70);

        let down = book
            .step("down", &decrease, button_delta(&decrease).unwrap(), false)
            .unwrap();
        assert_eq!(down.level, 60);
        assert!(down.screen);

        let turned = book
            .step("dial", &dial, turn_delta(&dial, 1).unwrap(), true)
            .unwrap();
        assert_eq!(turned.level, 65);
        assert!(turned.dials.is_empty());
        assert!(book.apply_global(&json!({ "brightness": 10 })).is_none());
        assert_eq!(book.level(), Some(65));
        assert!(book.shows_dial());
        book.disappear("dial");
        assert!(!book.shows_dial());
    }

    #[test]
    fn keypad_and_rgb_levels_stay_separate() {
        let mut book = BrightnessBook::default();
        let dial = json!({"output": "toggle", "source": "keypad", "brightness": 20, "step": 10});
        let frame = book.appear("dial", &dial, true, false).unwrap();
        assert_eq!(frame.level, 20);
        assert_eq!(frame.label, Some("KEYPAD"));
        let turned = book
            .step("dial", &dial, turn_delta(&dial, 1).unwrap(), true)
            .unwrap();
        assert_eq!(turned.level, 30);
        assert_eq!(turned.label, Some("KEYPAD"));
        let flipped = book.flip("dial", &dial, true).unwrap();
        assert_eq!(flipped.settings["source"], "rgb");
        let screen = flipped.screen.unwrap();
        assert_eq!(screen.label, Some("RGB"));
        assert_eq!(screen.level, 100);
        assert_eq!(book.level(), Some(30));
    }

    #[test]
    fn the_dial_sends_a_ring_with_the_percent() {
        let settings = brightness_after_turn(&json!({"brightness": 0, "step": 50}), 1, 0).unwrap();
        let sent = brightness_output(&settings, true, None);
        assert_eq!(sent.len(), 2);
        let backlight: Value = serde_json::from_str(&sent[0]).unwrap();
        assert_eq!(backlight["action"], "set_backlight");
        assert_eq!(backlight["payload"]["brightness"], 50);
        let screen: Value = serde_json::from_str(&sent[1]).unwrap();
        assert_eq!(screen["action"], "set_screen_image");
        assert_eq!(screen["payload"]["module"]["model"], "M0064");
        let jpeg = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            screen["payload"]["data_base64"].as_str().unwrap(),
        )
        .unwrap();
        let image = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((image.width(), image.height()), (240, 240));
        assert!(is_dial("Encoder", 5));
        assert!(!is_dial("Encoder", 0));
        assert!(!is_dial("Keypad", 5));
    }

    #[test]
    fn rgb_brightness_reaches_the_knobs_and_the_base() {
        let sent: Vec<Value> = commands(
            SET_BRIGHTNESS,
            &json!({"output": "rgb", "rgbBrightness": 40}),
        )
        .into_iter()
        .map(|text| serde_json::from_str(&text).unwrap())
        .collect();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0]["action"], "set_led_brightness");
        assert_eq!(sent[0]["payload"]["module"]["model"], "M003K");
        assert_eq!(sent[0]["payload"]["control"], "all");
        assert_eq!(sent[0]["payload"]["brightness"], 40);
        assert_eq!(sent[1]["payload"]["module"]["model"], "M0064");
        assert_eq!(sent[1]["payload"]["control"], 0);
        assert_eq!(sent[1]["payload"]["brightness"], 40);
    }

    #[test]
    fn toggle_after_press_switches_keypad_and_rgb() {
        assert!(flip_brightness_source(&json!({"output": "keypad"})).is_none());
        let rgb = flip_brightness_source(&json!({"output": "toggle", "step": 5})).unwrap();
        assert_eq!(rgb["source"], "rgb");
        assert_eq!(rgb["step"], 5);
        let keypad = flip_brightness_source(&rgb).unwrap();
        assert_eq!(keypad["source"], "keypad");
        let sent = brightness_output(&rgb, true, Some("RGB"));
        assert_eq!(sent.len(), 3);
        let first: Value = serde_json::from_str(&sent[0]).unwrap();
        assert_eq!(first["action"], "set_led_brightness");
        let screen: Value = serde_json::from_str(&sent[2]).unwrap();
        let jpeg = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            screen["payload"]["data_base64"].as_str().unwrap(),
        )
        .unwrap();
        let image = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        let pixel = image.get_pixel(88, 170);
        assert!(pixel[0] > 180 && pixel[1] > 180 && pixel[2] > 180);
    }

    #[test]
    fn mode_follows_the_same_targets_as_color() {
        let sent: Vec<Value> = commands(
            SET_MODE,
            &json!({"targets": ["base", "left"], "mode": "breathing"}),
        )
        .into_iter()
        .map(|text| serde_json::from_str(&text).unwrap())
        .collect();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0]["action"], "set_led_mode");
        assert_eq!(payload(&sent[0])["module"]["model"], "M003K");
        assert_eq!(payload(&sent[0])["control"], 1);
        assert_eq!(payload(&sent[0])["mode"], "breathing");
        assert_eq!(payload(&sent[0])["save"], false);
        assert_eq!(payload(&sent[1])["module"]["model"], "M0064");
        assert_eq!(payload(&sent[1])["control"], 0);
        assert_eq!(payload(&sent[1])["mode"], "breathing");

        let all = commands(SET_MODE, &json!({"mode": "rainbow"}));
        let all: Vec<Value> = all
            .into_iter()
            .map(|text| serde_json::from_str(&text).unwrap())
            .collect();
        assert_eq!(payload(&all[0])["control"], "all");
        assert_eq!(payload(&all[0])["mode"], "rainbow");
        assert_eq!(payload(&all[1])["mode"], "rainbow");
    }

    #[test]
    fn an_unknown_mode_sends_nothing() {
        assert!(commands(SET_MODE, &json!({"mode": "sparkle"})).is_empty());
        assert!(commands(SET_MODE, &json!({"targets": [], "mode": "static"})).is_empty());
        assert!(commands(SET_MODE, &json!({"action": "nope", "mode": "static"})).is_empty());
        let missing = commands(SET_MODE, &json!({}));
        let missing: Value = serde_json::from_str(&missing[0]).unwrap();
        assert_eq!(missing["payload"]["mode"], DEFAULT_MODE);
    }

    #[test]
    fn toggle_flips_between_off_and_solid() {
        assert!(stepped_mode(&json!({"action": "set", "mode": "wave"})).is_none());
        assert!(stepped_mode(&json!({"mode": "static"})).is_none());
        let off = stepped_mode(&json!({
            "targets": ["base"],
            "action": "toggle",
            "mode": "static"
        }))
        .unwrap();
        assert_eq!(off["mode"], "disable");
        assert_eq!(off["targets"][0], "base");
        let sent: Value = serde_json::from_str(&commands(SET_MODE, &off)[0]).unwrap();
        assert_eq!(sent["payload"]["mode"], "disable");
        let on = stepped_mode(&off).unwrap();
        assert_eq!(on["mode"], "static");
        let from_effect = stepped_mode(&json!({"action": "toggle", "mode": "breathing"})).unwrap();
        assert_eq!(from_effect["mode"], "disable");
        let first = stepped_mode(&json!({"action": "toggle"})).unwrap();
        assert_eq!(first["mode"], "static");
        assert!(stepped_mode(&json!({"action": "toggle", "mode": "sparkle"})).is_none());
    }

    #[test]
    fn toggle_mode_walks_the_effects_and_wraps_after_rainbow() {
        let next = stepped_mode(&json!({
            "targets": ["left"],
            "action": "cycle",
            "mode": "static"
        }))
        .unwrap();
        assert_eq!(next["mode"], "breathing");
        assert_eq!(next["targets"][0], "left");
        let sent: Value = serde_json::from_str(&commands(SET_MODE, &next)[0]).unwrap();
        assert_eq!(sent["payload"]["mode"], "breathing");
        let rainbow = stepped_mode(&json!({"action": "cycle", "mode": "water"})).unwrap();
        assert_eq!(rainbow["mode"], "rainbow");
        let wrapped = stepped_mode(&rainbow).unwrap();
        assert_eq!(wrapped["mode"], "static");
        let from_off = stepped_mode(&json!({"action": "cycle", "mode": "disable"})).unwrap();
        assert_eq!(from_off["mode"], "static");
        let first = stepped_mode(&json!({"action": "cycle"})).unwrap();
        assert_eq!(first["mode"], "static");
        assert!(stepped_mode(&json!({"action": "cycle", "mode": "sparkle"})).is_none());
    }

    #[test]
    fn key_images_follow_the_action_settings() {
        let up = action_image(SET_BRIGHTNESS, &json!({"action": "increase"}), Some(true)).unwrap();
        let down =
            action_image(SET_BRIGHTNESS, &json!({"action": "decrease"}), Some(true)).unwrap();
        let knob = action_image(SET_BRIGHTNESS, &json!({}), Some(false)).unwrap();
        assert_ne!(up, down);
        assert_ne!(up, knob);
        let red = action_image(SET_COLOR, &json!({"h": 0, "s": 100, "v": 100}), None).unwrap();
        let green = action_image(SET_COLOR, &json!({"h": 120, "s": 100, "v": 100}), None).unwrap();
        assert!(red.starts_with("data:image/png;base64,"));
        assert_ne!(red, green);
        assert!(action_image(SET_MODE, &json!({}), None).is_none());
        assert!(action_image(SET_COLOR, &json!({"h": "red"}), None).is_none());
    }

    #[test]
    fn manifest_lists_the_actions() {
        let manifest: Value = serde_json::from_str(include_str!("../manifest.json")).unwrap();
        let actions = manifest["Actions"].as_array().unwrap();
        assert!(actions.iter().any(|action| action["UUID"] == SET_COLOR
            && action["Icon"] == "assets/rgb"
            && action["States"][0]["Image"] == "assets/color"
            && action["PropertyInspectorPath"] == "propertyInspector/set-color.html"));
        assert!(actions.iter().any(|action| action["UUID"] == SET_MODE
            && action["Icon"] == "assets/rgb"
            && action["States"][0]["Image"] == "assets/rgb"
            && action["PropertyInspectorPath"] == "propertyInspector/set-mode.html"));
        assert!(actions.iter().any(|action| action["UUID"] == SET_BRIGHTNESS
            && action["Name"] == "Brightness"
            && action["Icon"] == "assets/brightness"
            && action["States"][0]["Image"] == "assets/brightness"
            && action["PropertyInspectorPath"] == "propertyInspector/set-brightness.html"
            && action["Controllers"]
                .as_array()
                .is_some_and(|controllers| controllers
                    .iter()
                    .any(|controller| controller == "Encoder"))
            && action["Encoder"]["TriggerDescription"]["Rotate"].is_string()));
        assert_eq!(manifest["Category"], "OpenMasterHub");
        let mode_inspector = include_str!("../propertyInspector/set-mode.html");
        for mode in MODES {
            assert!(mode_inspector.contains(&format!("value=\"{mode}\"")));
        }
        assert!(mode_inspector.contains("value=\"set\""));
        assert!(mode_inspector.contains("value=\"toggle\""));
        assert!(mode_inspector.contains("On/off toggle"));
        assert!(mode_inspector.contains("value=\"cycle\""));
        assert!(mode_inspector.contains("Toggle mode"));
        assert!(mode_inspector.contains("mode-row"));
        let color_inspector = include_str!("../propertyInspector/set-color.html");
        assert!(color_inspector.contains("id=\"v\""));
        assert!(color_inspector.contains("type=\"color\""));
        assert!(color_inspector.contains("Brightness"));
        let brightness_inspector = include_str!("../propertyInspector/set-brightness.html");
        assert!(brightness_inspector.contains("value=\"keypad\""));
        assert!(brightness_inspector.contains("value=\"rgb\""));
        assert!(brightness_inspector.contains("value=\"toggle\""));
        assert!(brightness_inspector.contains("Toggle after press"));
    }
}
