//! Ring and percent drawn on the dial screen for brightness.

use std::io::Cursor;

use image::{Rgb, RgbImage};

const SIZE: u32 = 240;
const CENTER: i32 = 120;
const OUTER: i32 = 104;
const INNER: i32 = 84;

const BACKGROUND: Rgb<u8> = Rgb([0, 0, 0]);
const TRACK: Rgb<u8> = Rgb([48, 48, 48]);
const PROGRESS: Rgb<u8> = Rgb([80, 168, 255]);
const TEXT: Rgb<u8> = Rgb([255, 255, 255]);

pub fn jpeg(brightness: u8, label: Option<&str>) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgb8(render(brightness, label))
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
        .ok()?;
    Some(bytes)
}

pub(crate) fn render(brightness: u8, label: Option<&str>) -> RgbImage {
    let brightness = brightness.min(100);
    let mut image = RgbImage::from_pixel(SIZE, SIZE, BACKGROUND);
    let sweep = brightness as f32 / 100.0 * std::f32::consts::TAU;
    let inner_sq = INNER * INNER;
    let outer_sq = OUTER * OUTER;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as i32 - CENTER;
            let dy = y as i32 - CENTER;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < inner_sq || dist_sq > outer_sq {
                continue;
            }
            let from_top = (dy as f32).atan2(dx as f32) + std::f32::consts::FRAC_PI_2;
            let from_top = from_top.rem_euclid(std::f32::consts::TAU);
            let color = if from_top < sweep { PROGRESS } else { TRACK };
            image.put_pixel(x, y, color);
        }
    }
    draw_text(&mut image, &format!("{brightness}%"), 5, None);
    if let Some(label) = label {
        draw_text(&mut image, label, 4, Some(168));
    }
    image
}

fn draw_text(image: &mut RgbImage, text: &str, scale: u32, origin_y: Option<u32>) {
    const GLYPH_W: u32 = 5;
    const GAP: u32 = 1;
    let cell = (GLYPH_W + GAP) * scale;
    let width = text.chars().count() as u32 * cell - GAP * scale;
    let mut origin_x = (SIZE - width) / 2;
    let origin_y = origin_y.unwrap_or((SIZE - 7 * scale) / 2);
    for ch in text.chars() {
        let Some(rows) = glyph(ch) else {
            origin_x += cell;
            continue;
        };
        for (row, bits) in rows.into_iter().enumerate() {
            for col in 0..GLYPH_W {
                if bits & (1 << (GLYPH_W - 1 - col)) == 0 {
                    continue;
                }
                let x0 = origin_x + col * scale;
                let y0 = origin_y + row as u32 * scale;
                for y in y0..y0 + scale {
                    for x in x0..x0 + scale {
                        if x < SIZE && y < SIZE {
                            image.put_pixel(x, y, TEXT);
                        }
                    }
                }
            }
        }
        origin_x += cell;
    }
}

fn glyph(ch: char) -> Option<[u8; 7]> {
    Some(match ch {
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b01110, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        '%' => [
            0b11001, 0b11010, 0b00100, 0b01000, 0b10110, 0b10011, 0b00000,
        ],
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'G' => [
            0b01110, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_fills_clockwise_from_the_top() {
        let empty = render(0, None);
        let half = render(50, None);
        let full = render(100, None);
        assert_eq!(*empty.get_pixel(120, 16), TRACK);
        assert_eq!(*half.get_pixel(120, 16), PROGRESS);
        assert_eq!(*full.get_pixel(120, 16), PROGRESS);
        assert_eq!(*half.get_pixel(216, 120), PROGRESS);
        assert_eq!(*empty.get_pixel(216, 120), TRACK);
        assert_eq!(*render(10, None).get_pixel(216, 120), TRACK);
        assert_eq!(*render(40, None).get_pixel(86, 168), BACKGROUND);
        assert_eq!(*render(40, Some("RGB")).get_pixel(86, 168), TEXT);
        assert_eq!(*render(40, Some("KEYPAD")).get_pixel(50, 168), TEXT);
    }
}
