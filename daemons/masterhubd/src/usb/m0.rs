//! Shared HID codec for the M0 modules: M003K, M005S, M032LE, and M0064.
//!
//! Byte layouts follow `cooltek-sdk-probe/docs/MasterHUB/modules/`.
//! This module does not open a device.

use serde_json::{json, Value};

use crate::protocol::LightingMode;

pub const PID_BASE: u16 = 0x01f6;
pub const PID_SLIDER: u16 = 0x0200;
pub const PID_KNOB: u16 = 0x0202;
pub const PID_ROLLER: u16 = 0x0204;

pub const KNOB_ID: &str = "knobs";
pub const SLIDER_ID: &str = "faders";
pub const ROLLER_ID: &str = "rollers";
pub const BASE_ID: &str = "base";

/// LED report "module" byte. Base is `0`, sliders are `1`, knobs are `2`.
pub const LED_BASE: u8 = 0x00;
pub const LED_SLIDER: u8 = 0x01;
pub const LED_KNOB: u8 = 0x02;
/// Output report size for M0 LED transfers (`0x40`).
pub const LED_REPORT_LEN: usize = 0x40;

const INPUT_REPORT_ID: u8 = 0x01;
const INPUT_CHANNEL: u8 = 0x2a;
const LED_REPORT_ID: u8 = 0x02;
const LED_MODE: u8 = 0x2c;
const LED_COLOR: u8 = 0x2d;
const LED_BRIGHTNESS: u8 = 0x2e;
const ALL_INDEX: u8 = 0xff;

const KNOB_TURN: u8 = 0x00;
const ROLLER_TURN: u8 = 0x01;
const SLIDER_VALUE: u8 = 0x02;
const KNOB_PRESS: u8 = 0x03;
const NO_CHANGE: u8 = 0xff;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputReport {
    /// `(control, clockwise)` for each knob that moved. `0xff` is omitted.
    KnobTurns(Vec<(u8, bool)>),
    KnobPress([bool; 3]),
    Sliders([u8; 5]),
    /// `(control, up)` for each roller that moved.
    Rollers(Vec<(u8, bool)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedCommand {
    Mode {
        index: u8,
        mode: u8,
        save: bool,
    },
    Color {
        index: u8,
        r: u8,
        g: u8,
        b: u8,
        save: bool,
    },
    Brightness {
        index: u8,
        brightness: u8,
    },
}

impl LedCommand {
    pub fn mode(index: u8, mode: LightingMode, save: bool) -> Self {
        Self::Mode {
            index,
            mode: lighting_code(mode),
            save,
        }
    }

    pub fn color(index: u8, r: u8, g: u8, b: u8, save: bool) -> Self {
        Self::Color {
            index,
            r,
            g,
            b,
            save,
        }
    }

    pub fn brightness(index: u8, brightness: u8) -> Self {
        Self::Brightness { index, brightness }
    }

    /// Same slot means a newer command replaces it instead of queueing behind it.
    pub fn replaces(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Mode { index: left, .. }, Self::Mode { index: right, .. })
            | (Self::Color { index: left, .. }, Self::Color { index: right, .. })
            | (Self::Brightness { index: left, .. }, Self::Brightness { index: right, .. }) => {
                left == right
            }
            _ => false,
        }
    }

    pub fn map_index(mut self, map: impl Fn(u8) -> u8) -> Self {
        match &mut self {
            Self::Mode { index, .. }
            | Self::Color { index, .. }
            | Self::Brightness { index, .. } => *index = map(*index),
        }
        self
    }
}

pub fn led_index_all() -> u8 {
    ALL_INDEX
}

fn lighting_code(mode: LightingMode) -> u8 {
    match mode {
        LightingMode::Disable => 0x00,
        LightingMode::Static => 0x01,
        LightingMode::Breathing => 0x02,
        LightingMode::Strobe => 0x03,
        LightingMode::Spring => 0x04,
        LightingMode::Cycling => 0x05,
        LightingMode::Random => 0x06,
        LightingMode::Wave => 0x07,
        LightingMode::Water => 0x08,
        LightingMode::Rainbow => 0x09,
    }
}

/// Parse one interrupt read from channel 0 (`0x2a`).
///
/// Envelope: report id `0x01`, channel tag, `u16` LE payload length, payload
/// at byte 4. The base heartbeat uses channel `0x2b` and is ignored.
pub fn input_report(report: &[u8]) -> Option<InputReport> {
    if report.get(1) != Some(&INPUT_CHANNEL) {
        return None;
    }
    input_payload(report)
}

/// Same payload parser as [`input_report`], on any channel tag.
pub fn input_payload(report: &[u8]) -> Option<InputReport> {
    if report.len() < 4 || report[0] != INPUT_REPORT_ID {
        return None;
    }
    let len = u16::from_le_bytes([report[2], report[3]]) as usize;
    let payload = report.get(4..4 + len)?;
    parse_payload(payload)
}

/// Protocol control for an M003K report index.
///
/// The report lists bottom-left, bottom-right, then top. Protocol control `0`
/// is the top knob, `1` is bottom-left, and `2` is bottom-right.
pub fn knob_control(report_index: u8) -> u8 {
    match report_index {
        0 => 1,
        1 => 2,
        2 => 0,
        other => other,
    }
}

/// Report index for an M003K protocol control. Inverse of [`knob_control`].
/// `0xff` (all LEDs) is unchanged.
pub fn knob_led_index(control: u8) -> u8 {
    match control {
        0 => 2,
        1 => 0,
        2 => 1,
        other => other,
    }
}

fn parse_payload(payload: &[u8]) -> Option<InputReport> {
    if let Some(body) = body(payload, KNOB_TURN, 3) {
        return Some(InputReport::KnobTurns(directions(body, true)));
    }
    if let Some(body) = body(payload, KNOB_PRESS, 3) {
        let mut pressed = [false; 3];
        for (slot, byte) in pressed.iter_mut().zip(body) {
            *slot = *byte == 0x01;
        }
        return Some(InputReport::KnobPress(pressed));
    }
    if let Some(body) = body(payload, ROLLER_TURN, 2) {
        return Some(InputReport::Rollers(directions(body, false)));
    }
    // The slider heading and example use report id `0x02`. The field table
    // says `0x00`, so accept that too when the length is the five slider bytes.
    if let Some(body) = body(payload, SLIDER_VALUE, 5).or_else(|| body(payload, KNOB_TURN, 5)) {
        if body.iter().any(|value| *value > 100) {
            return None;
        }
        return Some(InputReport::Sliders(body.try_into().ok()?));
    }
    // Some roller frames omit the inner report header and are just the two
    // direction bytes.
    if payload.len() == 2
        && payload
            .iter()
            .all(|byte| matches!(*byte, 0x00 | 0x01 | NO_CHANGE))
    {
        return Some(InputReport::Rollers(directions(payload, false)));
    }
    None
}

fn body(payload: &[u8], id: u8, count: usize) -> Option<&[u8]> {
    if payload.len() >= 2 + count && payload[0] == id && payload[1] == count as u8 {
        Some(&payload[2..2 + count])
    } else {
        None
    }
}

/// `clockwise` selects knob encoding (`0x01` clockwise). Rollers use `0x00` for up.
fn directions(body: &[u8], clockwise: bool) -> Vec<(u8, bool)> {
    body.iter()
        .enumerate()
        .filter_map(|(index, byte)| match *byte {
            0x00 | 0x01 => Some((index as u8, (*byte == 0x01) == clockwise)),
            NO_CHANGE => None,
            _ => None,
        })
        .collect()
}

pub fn encode_led(module: u8, command: &LedCommand) -> [u8; LED_REPORT_LEN] {
    let mut report = [0u8; LED_REPORT_LEN];
    report[0] = LED_REPORT_ID;
    match *command {
        LedCommand::Mode { index, mode, save } => {
            report[1] = LED_MODE;
            report[2] = module;
            report[3] = index;
            report[4] = mode;
            report[5] = u8::from(save);
        }
        LedCommand::Color {
            index,
            r,
            g,
            b,
            save,
        } => {
            report[1] = LED_COLOR;
            report[2] = module;
            report[3] = index;
            report[4] = r;
            report[5] = g;
            report[6] = b;
            report[7] = u8::from(save);
        }
        LedCommand::Brightness { index, brightness } => {
            report[1] = LED_BRIGHTNESS;
            report[2] = module;
            report[3] = index;
            report[4] = brightness;
        }
    }
    report
}

pub fn knob_rotated_payload(control: u8, clockwise: bool) -> Value {
    json!({
        "module": { "model": "M003K", "id": KNOB_ID },
        "control": control,
        "direction": if clockwise { "cw" } else { "ccw" },
    })
}

pub fn knob_state_payload(control: u8, pressed: bool) -> Value {
    json!({
        "module": { "model": "M003K", "id": KNOB_ID },
        "control": control,
        "pressed": pressed,
    })
}

pub fn base_knob_rotated_payload(clockwise: bool) -> Value {
    json!({
        "module": { "model": "M0064", "id": BASE_ID },
        "control": 0,
        "direction": if clockwise { "cw" } else { "ccw" },
    })
}

pub fn base_knob_state_payload(pressed: bool) -> Value {
    json!({
        "module": { "model": "M0064", "id": BASE_ID },
        "control": 0,
        "pressed": pressed,
    })
}

pub fn slider_payload(control: u8, value: u8) -> Value {
    json!({
        "module": { "model": "M005S", "id": SLIDER_ID },
        "control": control,
        "value": value,
    })
}

pub fn roller_payload(control: u8, up: bool) -> Value {
    json!({
        "module": { "model": "M032LE", "id": ROLLER_ID },
        "control": control,
        "direction": if up { "up" } else { "down" },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(payload: &[u8]) -> Vec<u8> {
        let mut report = vec![INPUT_REPORT_ID, INPUT_CHANNEL];
        report.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        report.extend_from_slice(payload);
        report
    }

    #[test]
    fn knob_turn_skips_unchanged_knobs() {
        let report = input_report(&envelope(&[0x00, 0x03, 0x01, 0xff, 0x00])).unwrap();
        assert_eq!(report, InputReport::KnobTurns(vec![(0, true), (2, false)]));
    }

    #[test]
    fn knob_report_order_is_left_right_top() {
        assert_eq!(knob_control(0), 1);
        assert_eq!(knob_control(1), 2);
        assert_eq!(knob_control(2), 0);
        assert_eq!(knob_led_index(0), 2);
        assert_eq!(knob_led_index(1), 0);
        assert_eq!(knob_led_index(2), 1);
        assert_eq!(knob_led_index(0xff), 0xff);
    }

    #[test]
    fn knob_press_treats_only_0x01_as_pressed() {
        let report = input_report(&envelope(&[0x03, 0x03, 0x01, 0x00, 0x02])).unwrap();
        assert_eq!(report, InputReport::KnobPress([true, false, false]));
    }

    #[test]
    fn roller_and_slider_reports() {
        let rollers = input_report(&envelope(&[0x01, 0x02, 0x00, 0xff])).unwrap();
        assert_eq!(rollers, InputReport::Rollers(vec![(0, true)]));
        let right = input_report(&envelope(&[0x01, 0x02, 0xff, 0x01])).unwrap();
        assert_eq!(right, InputReport::Rollers(vec![(1, false)]));
        let bare = input_report(&envelope(&[0xff, 0x01])).unwrap();
        assert_eq!(bare, InputReport::Rollers(vec![(1, false)]));
        let sliders = input_report(&envelope(&[0x02, 0x05, 0x33, 0x00, 0x64, 0x19, 0x4b])).unwrap();
        assert_eq!(
            sliders,
            InputReport::Sliders([0x33, 0x00, 0x64, 0x19, 0x4b])
        );
        let alternate = input_report(&envelope(&[0x00, 0x05, 1, 2, 3, 4, 5])).unwrap();
        assert_eq!(alternate, InputReport::Sliders([1, 2, 3, 4, 5]));
    }

    #[test]
    fn heartbeat_and_out_of_range_slider_are_ignored() {
        let mut heartbeat = vec![0x01, 0x2b, 0x01, 0x00, 0x00];
        heartbeat.resize(64, 0);
        assert!(input_report(&heartbeat).is_none());
        assert!(input_report(&envelope(&[0x02, 0x05, 0, 0, 101, 0, 0])).is_none());
    }

    #[test]
    fn led_reports_match_the_documented_examples() {
        let mode = encode_led(
            LED_KNOB,
            &LedCommand::mode(ALL_INDEX, LightingMode::Rainbow, true),
        );
        assert_eq!(&mode[..6], &[0x02, 0x2c, 0x02, 0xff, 0x09, 0x01]);
        let color = encode_led(
            LED_SLIDER,
            &LedCommand::color(ALL_INDEX, 0x33, 0x24, 0x3f, true),
        );
        assert_eq!(
            &color[..8],
            &[0x02, 0x2d, 0x01, 0xff, 0x33, 0x24, 0x3f, 0x01]
        );
        let brightness = encode_led(LED_BASE, &LedCommand::brightness(0, 0x64));
        assert_eq!(&brightness[..5], &[0x02, 0x2e, 0x00, 0x00, 0x64]);
        assert!(brightness[5..].iter().all(|byte| *byte == 0));
    }
}
