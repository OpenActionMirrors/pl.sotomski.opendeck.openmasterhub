//! M4315 HID codec.
//!
//! Byte layouts follow `cooltek-sdk-probe/docs/MasterHUB/modules/m4315/`.
//! This module does not open a device.

use std::borrow::Cow;
use std::io::Cursor;

use serde_json::{json, Value};

pub const VID: u16 = 0x2516;
pub const PID: u16 = 0x01fe;
pub const KEY_COUNT: usize = 15;
pub const MODULE_ID: &str = "keys";

/// Button panel size. A wider or taller JPEG stalls the module on a later write.
const BUTTON_PX: u32 = 72;

/// Output report size for button JPEG transfers (`0x0140`).
pub const REPORT_LEN: usize = 320;
/// JPEG bytes per chunk (`REPORT_LEN - 8`).
pub const CHUNK_MAX: usize = REPORT_LEN - 8;

const INPUT_REPORT_ID: u8 = 0x01;
const KEY_CHANNEL: u8 = 0x34;
const KEY_REPORT_ID: u8 = 0x02;
const KEY_REPORT_LEN: u8 = 0x0f;
const IMAGE_REPORT_ID: u8 = 0x02;
const IMAGE_COMMAND: u8 = 0x36;

pub type KeySnapshot = [bool; KEY_COUNT];

pub fn released_snapshot() -> KeySnapshot {
    [false; KEY_COUNT]
}

/// Feature report that sets the keypad backlight.
///
/// Report id `0x03`, command `0x34`, brightness at byte 2. `brightness` is
/// `0..=100`. Layout: `cooltek-sdk-probe` `adjust_backlight_cm_m4315`.
pub fn backlight_feature(brightness: u8) -> [u8; 0x20] {
    let mut report = [0u8; 0x20];
    report[0] = 0x03;
    report[1] = 0x34;
    report[2] = brightness.min(100);
    report
}

/// Feature report that switches the module into software mode.
///
/// Report id `0x03`, feature `0x3a`, mode `0x01`. The module ignores input and
/// output until this is sent. Layout: `cooltek-sdk-probe` `set_sw_mode_cm_m4315`.
pub fn software_mode_feature() -> [u8; 0x20] {
    let mut report = [0u8; 0x20];
    report[0] = 0x03;
    report[1] = 0x3a;
    report[2] = 0x01;
    report
}

/// Parse one interrupt read into a 15-key snapshot.
///
/// Envelope: report id `0x01`, channel tag `0x34`, `u16` LE payload length,
/// payload at byte 4. The payload is report `0x02`, length `0x0f`, then key 1
/// through key 15. Control `0` is key 1. `0x01` is pressed.
pub fn key_snapshot_from_report(report: &[u8]) -> Option<KeySnapshot> {
    if report.len() < 4 || report[0] != INPUT_REPORT_ID || report[1] != KEY_CHANNEL {
        return None;
    }
    let len = u16::from_le_bytes([report[2], report[3]]) as usize;
    let payload = report.get(4..4 + len)?;
    parse_key_payload(payload)
}

fn parse_key_payload(payload: &[u8]) -> Option<KeySnapshot> {
    if payload.len() < 2 + KEY_COUNT || payload[0] != KEY_REPORT_ID || payload[1] != KEY_REPORT_LEN
    {
        return None;
    }
    let mut keys = released_snapshot();
    for (key, byte) in keys.iter_mut().zip(payload[2..2 + KEY_COUNT].iter()) {
        *key = *byte == 0x01;
    }
    Some(keys)
}

/// Press and release edges. `pressed` is the new state of that control.
pub fn diff_keys(previous: &KeySnapshot, next: &KeySnapshot) -> Vec<(u8, bool)> {
    previous
        .iter()
        .zip(next.iter())
        .enumerate()
        .filter_map(|(index, (was_pressed, pressed))| {
            if was_pressed == pressed {
                None
            } else {
                Some((index as u8, *pressed))
            }
        })
        .collect()
}

pub fn key_state_payload(control: u8, pressed: bool) -> Value {
    json!({
        "module": { "model": "M4315", "id": MODULE_ID },
        "control": control,
        "pressed": pressed,
    })
}

/// JPEG bytes safe to write to a panel of `max_edge` pixels on a side.
///
/// An image that already fits is returned unchanged. A larger image is scaled
/// to a square of that size. `None` means the bytes are not a decodable JPEG
/// and must not be written: a bad payload stalls the module.
pub(crate) fn fit_jpeg(jpeg: &[u8], max_edge: u32) -> Option<Cow<'_, [u8]>> {
    let decoded = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).ok()?;
    if decoded.width() <= max_edge && decoded.height() <= max_edge {
        return Some(Cow::Borrowed(jpeg));
    }
    let scaled = decoded.resize_exact(max_edge, max_edge, image::imageops::FilterType::Triangle);
    let mut encoded = Cursor::new(Vec::new());
    scaled
        .write_to(&mut encoded, image::ImageFormat::Jpeg)
        .ok()?;
    Some(Cow::Owned(encoded.into_inner()))
}

/// JPEG bytes safe to write to one button.
pub fn button_jpeg_for_device(jpeg: &[u8]) -> Option<Cow<'_, [u8]>> {
    fit_jpeg(jpeg, BUTTON_PX)
}

/// Split a JPEG into 320-byte `0x02` output reports.
///
/// An empty buffer produces no reports. Chunk index and chunk length are
/// little-endian `u16`. The final-chunk flag is `0x01` only on the last report.
pub(crate) fn encode_jpeg_transfer(command: u8, index: u8, jpeg: &[u8]) -> Vec<[u8; REPORT_LEN]> {
    if jpeg.is_empty() {
        return Vec::new();
    }
    let mut reports = Vec::new();
    let mut offset = 0;
    let mut chunk_index: u16 = 0;
    while offset < jpeg.len() {
        let chunk_len = (jpeg.len() - offset).min(CHUNK_MAX);
        let is_last = offset + chunk_len == jpeg.len();
        let mut report = [0u8; REPORT_LEN];
        report[0] = IMAGE_REPORT_ID;
        report[1] = command;
        report[2..4].copy_from_slice(&chunk_index.to_le_bytes());
        report[4..6].copy_from_slice(&(chunk_len as u16).to_le_bytes());
        report[6] = index;
        report[7] = u8::from(is_last);
        report[8..8 + chunk_len].copy_from_slice(&jpeg[offset..offset + chunk_len]);
        reports.push(report);
        offset += chunk_len;
        chunk_index = chunk_index.wrapping_add(1);
    }
    reports
}

/// Split a button JPEG into `0x36` reports.
pub fn encode_button_image(index: u8, jpeg: &[u8]) -> Vec<[u8; REPORT_LEN]> {
    encode_jpeg_transfer(IMAGE_COMMAND, index, jpeg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(payload: &[u8]) -> Vec<u8> {
        let mut report = vec![0x01, 0x34];
        report.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        report.extend_from_slice(payload);
        report
    }

    fn key_payload(states: [u8; KEY_COUNT]) -> Vec<u8> {
        let mut payload = vec![0x02, 0x0f];
        payload.extend_from_slice(&states);
        payload
    }

    #[test]
    fn held_key_against_released_baseline_is_a_press() {
        let mut states = [0u8; KEY_COUNT];
        states[0] = 0x01;
        let next = key_snapshot_from_report(&envelope(&key_payload(states))).unwrap();
        assert!(next[0]);
        assert!(next[1..].iter().all(|pressed| !pressed));
        assert_eq!(diff_keys(&released_snapshot(), &next), vec![(0, true)]);
    }

    #[test]
    fn release_and_unchanged_keys_are_edges() {
        let mut previous = released_snapshot();
        previous[14] = true;
        previous[2] = true;
        let mut next = released_snapshot();
        next[2] = true;
        next[4] = true;
        assert_eq!(diff_keys(&previous, &next), vec![(4, true), (14, false)]);
        assert!(diff_keys(&next, &next).is_empty());
    }

    #[test]
    fn non_press_byte_and_wrong_channel_are_ignored() {
        let mut states = [0u8; KEY_COUNT];
        states[3] = 0x02;
        let snapshot = key_snapshot_from_report(&envelope(&key_payload(states))).unwrap();
        assert!(snapshot.iter().all(|pressed| !pressed));

        let mut report = envelope(&key_payload([0x01; KEY_COUNT]));
        report[1] = 0x35;
        assert!(key_snapshot_from_report(&report).is_none());
        assert!(key_snapshot_from_report(&[0x01, 0x34, 0x02, 0x00, 0x02, 0x0f]).is_none());
    }

    #[test]
    fn jpeg_longer_than_one_chunk_splits() {
        let jpeg: Vec<u8> = (0..313).map(|index| index as u8).collect();
        let reports = encode_button_image(4, &jpeg);
        assert_eq!(reports.len(), 2);

        assert_eq!(reports[0][0], 0x02);
        assert_eq!(reports[0][1], 0x36);
        assert_eq!(u16::from_le_bytes([reports[0][2], reports[0][3]]), 0);
        assert_eq!(u16::from_le_bytes([reports[0][4], reports[0][5]]), 312);
        assert_eq!(reports[0][6], 4);
        assert_eq!(reports[0][7], 0);
        assert_eq!(&reports[0][8..], &jpeg[..312]);

        assert_eq!(u16::from_le_bytes([reports[1][2], reports[1][3]]), 1);
        assert_eq!(u16::from_le_bytes([reports[1][4], reports[1][5]]), 1);
        assert_eq!(reports[1][6], 4);
        assert_eq!(reports[1][7], 1);
        assert_eq!(reports[1][8], jpeg[312]);
        assert!(reports[1][9..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn empty_jpeg_sends_nothing() {
        assert!(encode_button_image(0, &[]).is_empty());
    }

    fn jpeg_of_size(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 40])
        });
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut encoded, image::ImageFormat::Jpeg)
            .unwrap();
        encoded.into_inner()
    }

    #[test]
    fn fitting_jpeg_is_sent_unchanged() {
        let jpeg = jpeg_of_size(72, 72);
        let fitted = button_jpeg_for_device(&jpeg).unwrap();
        assert_eq!(&*fitted, jpeg.as_slice());
    }

    #[test]
    fn oversized_jpeg_is_scaled_to_the_panel() {
        let jpeg = jpeg_of_size(80, 144);
        let fitted = button_jpeg_for_device(&jpeg).unwrap();
        let decoded = image::load_from_memory(fitted.as_ref()).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (72, 72));
    }

    #[test]
    fn undecodable_jpeg_is_not_written() {
        assert!(button_jpeg_for_device(&[0xff, 0xd8, 0xff]).is_none());
        assert!(button_jpeg_for_device(&[]).is_none());
    }

    #[test]
    fn software_mode_feature_enables_software_mode() {
        let report = software_mode_feature();
        assert_eq!(&report[..3], &[0x03, 0x3a, 0x01]);
        assert!(report[3..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn backlight_feature_sets_command_0x34() {
        let report = backlight_feature(100);
        assert_eq!(&report[..3], &[0x03, 0x34, 100]);
        assert!(report[3..].iter().all(|byte| *byte == 0));
        assert_eq!(backlight_feature(255)[2], 100);
    }
}
