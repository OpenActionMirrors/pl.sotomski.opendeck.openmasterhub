//! 1xDial with IPS (PID `0x0206`).
//!
//! The Cooltek module table does not list this product. A capture of the
//! official MasterHUB app shows the takeover feature, the screen writes, and
//! the encoder reports. The panel is 240×240. Events are published as the
//! M0064 base knob, which is the big knob in the WebSocket API.

use super::m4315::{encode_jpeg_transfer, REPORT_LEN};

pub const PID: u16 = 0x0206;
pub const SCREEN_PX: u32 = 240;
/// Output command on report `0x02`. Same chunk layout as a key image.
const SCREEN_COMMAND: u8 = 0x45;
/// Screen slot used for every live frame after the app's first upload.
const SCREEN_SLOT: u8 = 0x30;
const INPUT_CHANNEL: u8 = 0x44;
const TURN_REPORT: u8 = 0x03;
const PRESS_REPORT: u8 = 0x04;

/// Feature report that switches the dial into software mode.
///
/// Captured from MasterHUB as it replaced the hardware animation:
/// report `0x03`, command `0x49`, mode `0x01`, then width `240`, height
/// `240`, and the fixed tail `01 f0 f0`.
pub fn software_mode_feature() -> [u8; 0x20] {
    let mut report = [0u8; 0x20];
    report[0] = 0x03;
    report[1] = 0x49;
    report[2] = 0x01;
    report[6..8].copy_from_slice(&240u16.to_le_bytes());
    report[8..10].copy_from_slice(&240u16.to_le_bytes());
    report[10] = 0x01;
    report[11] = 0xf0;
    report[12] = 0xf0;
    report
}

/// Fit a JPEG on the square panel without stretching it.
///
/// OpenDeck's encoder artwork is a wide strip. Stretching that to 240×240
/// makes it tall and the round screen clips the top and bottom. A matching
/// square is passed through. Anything else is scaled to fit and centered on
/// black.
pub fn screen_jpeg(jpeg: &[u8]) -> Option<std::borrow::Cow<'_, [u8]>> {
    let decoded = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).ok()?;
    if decoded.width() == SCREEN_PX && decoded.height() == SCREEN_PX {
        return Some(std::borrow::Cow::Borrowed(jpeg));
    }
    let fitted = fit_on_square(&decoded, SCREEN_PX);
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(fitted)
        .write_to(&mut encoded, image::ImageFormat::Jpeg)
        .ok()?;
    Some(std::borrow::Cow::Owned(encoded.into_inner()))
}

fn fit_on_square(source: &image::DynamicImage, edge: u32) -> image::RgbImage {
    let width = source.width().max(1);
    let height = source.height().max(1);
    let (target_w, target_h) = if width >= height {
        (edge, (edge * height / width).clamp(1, edge))
    } else {
        ((edge * width / height).clamp(1, edge), edge)
    };
    let resized = source.resize_exact(target_w, target_h, image::imageops::FilterType::Triangle);
    let mut canvas = image::RgbImage::from_pixel(edge, edge, image::Rgb([0, 0, 0]));
    let x = (edge - target_w) / 2;
    let y = (edge - target_h) / 2;
    image::imageops::overlay(&mut canvas, &resized.to_rgb8(), x.into(), y.into());
    canvas
}

pub fn encode_screen(jpeg: &[u8]) -> Vec<[u8; REPORT_LEN]> {
    encode_jpeg_transfer(SCREEN_COMMAND, SCREEN_SLOT, jpeg)
}

/// One detent or press from the dial's input report.
///
/// Channel `0x44`. A turn is `03 01` plus a direction. `0x00` is clockwise.
/// A press is `04 01` plus `0x01` held or `0x00` released.
pub fn dial_input(report: &[u8]) -> Option<DialInput> {
    if report.len() < 4 || report[0] != 0x01 || report[1] != INPUT_CHANNEL {
        return None;
    }
    let len = u16::from_le_bytes([report[2], report[3]]) as usize;
    match report.get(4..4 + len)? {
        [TURN_REPORT, 0x01, 0x00] => Some(DialInput::Turn(true)),
        [TURN_REPORT, 0x01, 0x01] => Some(DialInput::Turn(false)),
        [PRESS_REPORT, 0x01, pressed] => Some(DialInput::Press(*pressed == 0x01)),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialInput {
    Turn(bool),
    Press(bool),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(payload: &[u8]) -> Vec<u8> {
        let mut report = vec![0x01, INPUT_CHANNEL];
        report.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        report.extend_from_slice(payload);
        report
    }

    #[test]
    fn dial_turn_and_press_match_the_capture() {
        assert_eq!(
            dial_input(&envelope(&[0x03, 0x01, 0x00])),
            Some(DialInput::Turn(true))
        );
        assert_eq!(
            dial_input(&envelope(&[0x03, 0x01, 0x01])),
            Some(DialInput::Turn(false))
        );
        assert_eq!(
            dial_input(&envelope(&[0x04, 0x01, 0x01])),
            Some(DialInput::Press(true))
        );
        assert_eq!(
            dial_input(&envelope(&[0x04, 0x01, 0x00])),
            Some(DialInput::Press(false))
        );
        assert!(dial_input(&envelope(&[0x03, 0x01, 0xff])).is_none());
    }

    #[test]
    fn software_mode_matches_the_captured_feature() {
        let report = software_mode_feature();
        assert_eq!(
            &report[..13],
            &[0x03, 0x49, 0x01, 0x00, 0x00, 0x00, 0xf0, 0x00, 0xf0, 0x00, 0x01, 0xf0, 0xf0]
        );
        assert!(report[13..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn screen_transfer_uses_the_captured_slot() {
        let reports = encode_screen(&[0xff, 0xd8, 0xff]);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0][0], 0x02);
        assert_eq!(reports[0][1], 0x45);
        assert_eq!(reports[0][6], 0x30);
        assert_eq!(reports[0][7], 1);
    }

    #[test]
    fn wide_encoder_image_is_letterboxed() {
        let wide = image::RgbImage::from_pixel(200, 100, image::Rgb([220, 20, 20]));
        let mut encoded = Vec::new();
        image::DynamicImage::ImageRgb8(wide)
            .write_to(
                &mut std::io::Cursor::new(&mut encoded),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        let fitted = screen_jpeg(&encoded).unwrap();
        let decoded = image::load_from_memory_with_format(&fitted, image::ImageFormat::Jpeg)
            .unwrap()
            .to_rgb8();
        assert_eq!((decoded.width(), decoded.height()), (SCREEN_PX, SCREEN_PX));
        let top = decoded.get_pixel(120, 8);
        let middle = decoded.get_pixel(120, 120);
        assert!(top[0] < 40 && top[1] < 40 && top[2] < 40);
        assert!(middle[0] > 180 && middle[1] < 80 && middle[2] < 80);
    }
}
