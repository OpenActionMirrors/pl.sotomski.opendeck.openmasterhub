//! Menu and key icons. The action list uses the PNG files in `assets`.
//! A key image is drawn again when it depends on that key's settings.

use std::io::Cursor;

use image::{Rgba, RgbaImage};

const SIZE: u32 = 144;
const SAMPLES: u32 = 4;

const SUN: [u8; 4] = [255, 196, 40, 255];
const BADGE: [u8; 4] = [28, 28, 32, 255];
const MARK: [u8; 4] = [255, 255, 255, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SunMark {
    Plain,
    Plus,
    Minus,
}

pub fn brightness_png(mark: SunMark) -> Vec<u8> {
    encode(&render(|x, y| sun_sample(x, y, mark)))
}

#[cfg(test)]
fn ring_png() -> Vec<u8> {
    encode(&render(ring_sample))
}

pub fn color_png(r: u8, g: u8, b: u8) -> Vec<u8> {
    encode(&render(|x, y| color_sample(x, y, r, g, b)))
}

pub fn data_url(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png)
    )
}

fn encode(image: &RgbaImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("png");
    bytes
}

fn render(sample: impl Fn(f32, f32) -> [u8; 4]) -> RgbaImage {
    let mut image = RgbaImage::new(SIZE, SIZE);
    let steps = SAMPLES as f32;
    let count = (SAMPLES * SAMPLES) as f32;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut acc = [0.0; 4];
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / steps;
                    let py = y as f32 + (sy as f32 + 0.5) / steps;
                    let pixel = sample(px, py);
                    for channel in 0..4 {
                        acc[channel] += pixel[channel] as f32;
                    }
                }
            }
            image.put_pixel(
                x,
                y,
                Rgba(acc.map(|channel| (channel / count).round() as u8)),
            );
        }
    }
    image
}

fn sun_sample(x: f32, y: f32, mark: SunMark) -> [u8; 4] {
    const CX: f32 = 72.0;
    const CY: f32 = 64.0;
    if mark != SunMark::Plain && on_mark(x, y, mark) {
        return MARK;
    }
    if mark != SunMark::Plain && on_badge(x, y) {
        return BADGE;
    }
    let dx = x - CX;
    let dy = y - CY;
    let dist_sq = dx * dx + dy * dy;
    if dist_sq <= 28.0 * 28.0 || on_ray(dx, dy) {
        return SUN;
    }
    CLEAR
}

fn on_ray(dx: f32, dy: f32) -> bool {
    let dist = (dx * dx + dy * dy).sqrt();
    if !(36.0..=60.0).contains(&dist) {
        return false;
    }
    let angle = dy.atan2(dx) + std::f32::consts::FRAC_PI_2;
    let sector = std::f32::consts::TAU / 8.0;
    let frac = (angle / sector).rem_euclid(1.0);
    let to_ray = frac.min(1.0 - frac) * sector;
    to_ray <= (6.0 / dist).atan()
}

fn on_badge(x: f32, y: f32) -> bool {
    let dx = x - 108.0;
    let dy = y - 112.0;
    dx * dx + dy * dy <= 20.0 * 20.0
}

fn on_mark(x: f32, y: f32, mark: SunMark) -> bool {
    let dx = (x - 108.0).abs();
    let dy = (y - 112.0).abs();
    let arm = 11.0;
    let thick = 3.2;
    let horizontal = dx <= arm && dy <= thick;
    let vertical = mark == SunMark::Plus && dy <= arm && dx <= thick;
    horizontal || vertical
}

#[cfg(test)]
fn ring_sample(x: f32, y: f32) -> [u8; 4] {
    let dx = x - 72.0;
    let dy = y - 72.0;
    let dist = (dx * dx + dy * dy).sqrt();
    if !(40.0..=68.0).contains(&dist) {
        return CLEAR;
    }
    // 0 at the top, clockwise. Each arc is centered on a cardinal-adjacent slot
    // so the top of the ring is red, then green, then blue.
    let turns = dx.atan2(-dy).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
    let third = 1.0 / 3.0;
    let gap = 0.04;
    let shifted = (turns + third / 2.0).rem_euclid(1.0);
    let local = shifted % third;
    if local < gap / 2.0 || local > third - gap / 2.0 {
        return CLEAR;
    }
    match (shifted / third) as i32 {
        0 => [255, 64, 48, 255],
        1 => [52, 209, 96, 255],
        _ => [32, 132, 255, 255],
    }
}

fn color_sample(x: f32, y: f32, r: u8, g: u8, b: u8) -> [u8; 4] {
    let dx = x - 72.0;
    let dy = y - 72.0;
    let dist = (dx * dx + dy * dy).sqrt();
    const RADIUS: f32 = 60.0;
    const BORDER: f32 = 5.0;
    if dist > RADIUS {
        return CLEAR;
    }
    if dist > RADIUS - BORDER {
        let luminance = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
        return if luminance > 170.0 {
            [32, 32, 36, 255]
        } else {
            [255, 255, 255, 255]
        };
    }
    [r, g, b, 255]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
        image.get_pixel(x, y).0
    }

    #[test]
    fn the_sun_is_centered_and_the_mark_sits_on_a_badge() {
        let plain = render(|x, y| sun_sample(x, y, SunMark::Plain));
        let plus = render(|x, y| sun_sample(x, y, SunMark::Plus));
        let minus = render(|x, y| sun_sample(x, y, SunMark::Minus));
        assert_eq!(pixel(&plain, 72, 64)[0], 255);
        assert_eq!(pixel(&plain, 72, 64)[3], 255);
        assert_eq!(pixel(&plain, 0, 0)[3], 0);
        assert_eq!(pixel(&plain, 108, 112)[3], 0);
        assert_eq!(pixel(&plus, 108, 112), MARK);
        assert_eq!(pixel(&plus, 108, 104), MARK);
        assert!(pixel(&minus, 108, 104)[0] < 80);
        assert_eq!(pixel(&minus, 116, 112), MARK);
        assert_ne!(pixel(&plus, 108, 104), pixel(&minus, 108, 104));
    }

    #[test]
    fn the_ring_is_red_then_green_then_blue() {
        let ring = render(ring_sample);
        assert_eq!(&pixel(&ring, 72, 16)[..3], &[255, 64, 48]);
        assert_eq!(&pixel(&ring, 120, 100)[..3], &[52, 209, 96]);
        assert_eq!(&pixel(&ring, 24, 100)[..3], &[32, 132, 255]);
        assert_eq!(pixel(&ring, 72, 72)[3], 0);
        assert_eq!(pixel(&ring, 0, 0)[3], 0);
    }

    #[test]
    fn the_color_icon_uses_the_selected_color() {
        let red = render(|x, y| color_sample(x, y, 255, 0, 0));
        let white = render(|x, y| color_sample(x, y, 255, 255, 255));
        assert_eq!(pixel(&red, 72, 72), [255, 0, 0, 255]);
        assert_eq!(&pixel(&red, 72, 16)[..3], &[255, 255, 255]);
        assert_eq!(&pixel(&white, 72, 16)[..3], &[32, 32, 36]);
        assert_eq!(pixel(&red, 0, 0)[3], 0);
    }

    #[test]
    fn saved_icons_match_the_renderer() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let files = [
            ("brightness.png", brightness_png(SunMark::Plain)),
            ("brightness-increase.png", brightness_png(SunMark::Plus)),
            ("brightness-decrease.png", brightness_png(SunMark::Minus)),
            ("rgb.png", ring_png()),
            ("color.png", color_png(255, 0, 0)),
        ];
        if std::env::var_os("WRITE_ICONS").is_some() {
            for (name, bytes) in &files {
                std::fs::write(dir.join(name), bytes).unwrap();
            }
        }
        for (name, bytes) in files {
            let saved = std::fs::read(dir.join(name)).unwrap();
            let saved = image::load_from_memory(&saved).unwrap().to_rgba8();
            let drawn = image::load_from_memory(&bytes).unwrap().to_rgba8();
            assert_eq!(saved.dimensions(), (SIZE, SIZE), "{name}");
            assert_eq!(saved.as_raw(), drawn.as_raw(), "{name}");
        }
    }
}
