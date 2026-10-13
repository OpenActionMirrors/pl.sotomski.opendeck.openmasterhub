use std::sync::LazyLock;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::Value;

pub const DEVICE_ID: &str = "masterhub";
pub const DEVICE_NAME: &str = "Master Hub";
pub const ROWS: u8 = 3;
pub const COLUMNS: u8 = 5;
/// Three knobs, two rollers, the dial, then the five sliders. The count stays
/// at 11 while the device is registered so OpenDeck does not shrink a profile.
pub const ENCODERS: u8 = 11;
pub const DEVICE_TYPE: u8 = 0;

pub(crate) const KEYS: &str = "keys";
pub(crate) const KNOBS: &str = "knobs";
const ROLLERS: &str = "rollers";

const ROLLER_ENCODER: u8 = 3;
pub(crate) const BIG_KNOB_ENCODER: u8 = 5;
pub(crate) const SLIDER_ENCODER: u8 = 6;
const FADERS: &str = "faders";

pub(crate) const BASE: &str = "base";

/// Modules that have a control on the OpenDeck canvas. The base is tracked by
/// the daemon and does not appear here. `places` is set when the daemon knows
/// the pogo site. The site table is `docs/base-usb-ports.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConnectedModules {
    pub keys: bool,
    pub knobs: bool,
    pub rollers: bool,
    pub dial: bool,
    pub sliders: bool,
    places: ModulePlaces,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ModulePlaces {
    keys: Option<Placement>,
    knobs: Option<Placement>,
    rollers: Option<Placement>,
    dial: Option<Placement>,
    sliders: Option<Placement>,
}

/// A module rectangle on the base grid. Coordinates are cells from the top left
/// of the base, so a module sitting partly above the base has a negative `y`.
/// Horizontal is the 6×4 side. Vertical is the base turned to 4×6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Placement {
    horizontal: bool,
    x: i16,
    y: i16,
    w: u16,
    h: u16,
}

impl ConnectedModules {
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut modules = Self::default();
        for name in names {
            match name {
                "M4315" => modules.keys = true,
                "M003K" => modules.knobs = true,
                "M032LE" => modules.rollers = true,
                "M005S" => modules.sliders = true,
                "1xDial" => modules.dial = true,
                _ => {}
            }
        }
        modules
    }

    /// `report` is a hello or a `modules` payload: a `modules` array and an
    /// optional `sites` object.
    pub fn from_report(report: &Value) -> Self {
        let mut modules = Self::from_names(
            report
                .get("modules")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        );
        let Some(sites) = report.get("sites").and_then(Value::as_object) else {
            return modules;
        };
        for (name, site) in sites {
            let Some(site) = site.as_str() else {
                continue;
            };
            let Some(place) = placement(name, site) else {
                continue;
            };
            match name.as_str() {
                "M4315" => modules.places.keys = Some(place),
                "M003K" => modules.places.knobs = Some(place),
                "M032LE" => modules.places.rollers = Some(place),
                "M005S" => modules.places.sliders = Some(place),
                "1xDial" => modules.places.dial = Some(place),
                _ => {}
            }
        }
        modules
    }

    pub fn visible(&self) -> bool {
        self.keys || self.knobs || self.rollers || self.dial || self.sliders
    }

    #[cfg(test)]
    fn all() -> Self {
        Self {
            keys: true,
            knobs: true,
            rollers: true,
            dial: true,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    KeyDown(u8),
    KeyUp(u8),
    EncoderDown(u8),
    EncoderUp(u8),
    EncoderChange { position: u8, ticks: i16 },
    SliderMoved { control: u8, value: u8 },
}

/// Turns a slider's absolute position into encoder ticks. The first sample
/// only records the position, so plugging the module in does not fire an action.
pub fn slider_delta(previous: Option<u8>, value: u8) -> Option<i16> {
    let ticks = i16::from(value) - i16::from(previous?);
    (ticks != 0).then_some(ticks)
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
        ("slider_moved", "M005S", FADERS) if control < 5 => {
            let value = u8::try_from(payload.get("value")?.as_u64()?).ok()?;
            if value > 100 {
                return None;
            }
            Some(DeviceEvent::SliderMoved { control, value })
        }
        _ => None,
    }
}

pub fn image_command(
    device: &str,
    controller: Option<&str>,
    position: Option<u8>,
    image: Option<&str>,
) -> Vec<String> {
    if device != DEVICE_ID {
        return Vec::new();
    }
    let Some(image) = image else {
        return clear_commands(controller, position);
    };
    let Some(jpeg) = jpeg_base64_from_data_url(image) else {
        return Vec::new();
    };
    if controller == Some("Encoder") {
        if position == Some(BIG_KNOB_ENCODER) {
            return vec![screen_image_command(&jpeg)];
        }
        return Vec::new();
    }
    let Some(position) = position.filter(|position| *position < ROWS * COLUMNS) else {
        return Vec::new();
    };
    vec![button_image_command(position, &jpeg)]
}

/// OpenDeck sends `setImage` with no image to blank a control. A profile
/// change sends that event with no controller and no position, which blanks
/// every key and the dial before the new profile's images arrive.
fn clear_commands(controller: Option<&str>, position: Option<u8>) -> Vec<String> {
    match (controller, position) {
        (None, None) => {
            let mut commands = (0..ROWS * COLUMNS)
                .map(|control| button_image_command(control, &BLANK_KEY_JPEG))
                .collect::<Vec<_>>();
            commands.push(screen_image_command(&BLANK_DIAL_JPEG));
            commands
        }
        (Some("Encoder"), Some(BIG_KNOB_ENCODER)) => {
            vec![screen_image_command(&BLANK_DIAL_JPEG)]
        }
        (Some("Encoder"), _) => Vec::new(),
        (_, Some(position)) if position < ROWS * COLUMNS => {
            vec![button_image_command(position, &BLANK_KEY_JPEG)]
        }
        _ => Vec::new(),
    }
}

fn solid_jpeg(edge: u32) -> String {
    let image = image::RgbImage::from_pixel(edge, edge, image::Rgb([0, 0, 0]));
    let mut encoded = Vec::new();
    image::DynamicImage::ImageRgb8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut encoded),
            image::ImageFormat::Jpeg,
        )
        .expect("black jpeg");
    STANDARD.encode(encoded)
}

static BLANK_KEY_JPEG: LazyLock<String> = LazyLock::new(|| solid_jpeg(72));
static BLANK_DIAL_JPEG: LazyLock<String> = LazyLock::new(|| solid_jpeg(240));

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

pub fn registration_event(modules: ConnectedModules) -> Value {
    serde_json::json!({
        "event": "registerDevice",
        "payload": registration_payload(modules),
    })
}

pub fn registration_payload(modules: ConnectedModules) -> Value {
    serde_json::json!({
        "id": DEVICE_ID,
        "name": DEVICE_NAME,
        "rows": ROWS,
        "columns": COLUMNS,
        "encoders": ENCODERS,
        "type": DEVICE_TYPE,
        "encoder": encoder_descriptors(),
        "layoutVersion": 1,
        "layout": device_layout(&modules),
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
    for _ in 0..5 {
        encoders.push(descriptor("rotary", false));
    }
    encoders
}

fn descriptor(kind: &str, is_absolute: bool) -> Value {
    serde_json::json!({ "type": kind, "is_absolute": is_absolute })
}

/// Same outer size as OpenDeck's legacy key cell. Explicit layouts are not scaled up.
const CELL: u16 = 132;
const GAP: u16 = 24;

/// Present modules, top to bottom: keys, rollers, sliders, then the knob cluster
/// beside the dial. This is the fallback when a connected module has no pogo
/// site. A missing row closes the gap. Each module keeps its shape.
///
/// The three knobs are a 2×2 block: the top knob is centered on the first row,
/// and the other two fill the second row. The dial's box is that same block.
/// The knobs stay on the left of that row and the dial stays on the right, so
/// unplugging one does not slide the other into the center. Neighboring
/// controls are separated by [`GAP`].
fn device_layout(modules: &ConnectedModules) -> Value {
    if let Some(layout) = grid_layout(modules) {
        return layout;
    }
    stacked_layout(modules)
}

fn stacked_layout(modules: &ConnectedModules) -> Value {
    let pitch = CELL + GAP;
    let keys_width = u16::from(COLUMNS) * CELL + u16::from(COLUMNS - 1) * GAP;
    let keys_height = u16::from(ROWS) * CELL + u16::from(ROWS - 1) * GAP;
    let cluster = CELL * 2 + GAP;
    let roller_width = cluster;
    let roller_height = CELL;
    let slider_width = 5 * CELL + 4 * GAP;
    let slider_height = CELL;
    let bottom_width = if modules.knobs || modules.dial {
        cluster * 2 + GAP
    } else {
        0
    };

    let mut bands = Vec::new();
    if modules.keys {
        bands.push((keys_width, keys_height));
    }
    if modules.rollers {
        bands.push((roller_width, roller_height));
    }
    if modules.sliders {
        bands.push((slider_width, slider_height));
    }
    if bottom_width > 0 {
        bands.push((bottom_width, cluster));
    }
    let canvas_width = bands.iter().map(|(width, _)| *width).max().unwrap_or(0);
    let canvas_height = bands
        .iter()
        .enumerate()
        .fold(0u16, |total, (index, (_, height))| {
            total + height + if index > 0 { GAP } else { 0 }
        });

    let mut controls = Vec::new();
    let mut y = 0u16;
    if modules.keys {
        let left = (canvas_width - keys_width) / 2;
        for position in 0..ROWS * COLUMNS {
            let column = u16::from(position % COLUMNS);
            let row = u16::from(position / COLUMNS);
            controls.push(key(position, left + column * pitch, y + row * pitch));
        }
        y += keys_height;
        if modules.rollers || modules.sliders || bottom_width > 0 {
            y += GAP;
        }
    }
    if modules.rollers {
        let left = (canvas_width - roller_width) / 2;
        controls.push(encoder_cell(ROLLER_ENCODER, left, y));
        controls.push(encoder_cell(ROLLER_ENCODER + 1, left + pitch, y));
        y += roller_height;
        if modules.sliders || bottom_width > 0 {
            y += GAP;
        }
    }
    if modules.sliders {
        let left = (canvas_width - slider_width) / 2;
        for index in 0..5 {
            controls.push(encoder_cell(
                SLIDER_ENCODER + index,
                left + u16::from(index) * pitch,
                y,
            ));
        }
        y += slider_height;
        if bottom_width > 0 {
            y += GAP;
        }
    }
    if bottom_width > 0 {
        let x = (canvas_width - bottom_width) / 2;
        if modules.knobs {
            controls.push(encoder(0, x + cluster / 2, y + CELL / 2, CELL / 2));
            controls.push(encoder_cell(1, x, y + pitch));
            controls.push(encoder_cell(2, x + pitch, y + pitch));
        }
        if modules.dial {
            let dial_x = x + cluster + GAP;
            controls.push(encoder(
                BIG_KNOB_ENCODER,
                dial_x + cluster / 2,
                y + cluster / 2,
                cluster / 2,
            ));
        }
    }

    serde_json::json!({
        "canvas": { "width": canvas_width, "height": canvas_height, "unit": "px" },
        "controls": controls,
    })
}

/// One base cell, in the same pixel pitch as a stacked key plus its gap.
const UNIT: u16 = CELL + GAP;

/// Sites from `docs/base-usb-ports.md`. The key drops the bus byte and the Mac
/// port nibble, so `@02112400` is `124`.
fn placement(module: &str, site: &str) -> Option<Placement> {
    let place = |horizontal, x, y, w, h| Placement {
        horizontal,
        x,
        y,
        w,
        h,
    };
    Some(match (module, site) {
        ("M003K" | "1xDial", "124") => place(true, 0, 0, 2, 2),
        ("M003K" | "1xDial", "111") => place(true, 2, 0, 2, 2),
        ("M003K" | "1xDial", "143") => place(true, 4, 0, 2, 2),
        ("M003K" | "1xDial", "44") => place(true, 0, 2, 2, 2),
        ("M003K" | "1xDial", "43") => place(true, 1, 2, 2, 2),
        ("M003K" | "1xDial", "23") => place(true, 2, 2, 2, 2),
        ("M003K" | "1xDial", "112") => place(true, 3, 2, 2, 2),
        ("M003K" | "1xDial", "134") => place(true, 4, 2, 2, 2),
        ("M003K" | "1xDial", "34") => place(false, 0, 0, 2, 2),
        ("M003K" | "1xDial", "113") => place(false, 2, 0, 2, 2),
        ("M003K" | "1xDial", "31") => place(false, 0, 2, 2, 2),
        ("M003K" | "1xDial", "42") => place(false, 2, 2, 2, 2),
        ("M003K" | "1xDial", "123") => place(false, 0, 4, 2, 2),
        ("M003K" | "1xDial", "122") => place(false, 2, 4, 2, 2),
        ("M4315", "111") => place(true, 0, -1, 4, 3),
        ("M4315", "143") => place(true, 2, -1, 4, 3),
        ("M4315", "22") => place(true, 0, 0, 4, 3),
        ("M4315", "132") => place(true, 2, 0, 4, 3),
        ("M4315", "23") => place(true, 0, 1, 4, 3),
        ("M4315", "112") => place(true, 1, 1, 4, 3),
        ("M4315", "134") => place(true, 2, 1, 4, 3),
        ("M4315", "113") => place(false, 0, -1, 4, 3),
        ("M4315", "24") => place(false, 0, 0, 4, 3),
        ("M4315", "42") => place(false, 0, 1, 4, 3),
        ("M4315", "41") => place(false, 0, 2, 4, 3),
        ("M4315", "122") => place(false, 0, 3, 4, 3),
        ("M032LE", "32") => place(true, 0, 1, 4, 1),
        ("M032LE", "21") => place(true, 0, 2, 4, 1),
        ("M032LE", "114") => place(true, 0, 3, 4, 1),
        ("M032LE", "141") => place(true, 2, 0, 4, 1),
        ("M032LE", "142") => place(true, 2, 1, 4, 1),
        ("M032LE", "131") => place(true, 2, 2, 4, 1),
        ("M032LE", "133") => place(true, 2, 3, 4, 1),
        ("M032LE", "134") => place(false, 0, 0, 4, 1),
        ("M032LE", "112") => place(false, 0, 1, 4, 1),
        ("M032LE", "23") => place(false, 0, 2, 4, 1),
        ("M032LE", "43") => place(false, 0, 3, 4, 1),
        ("M032LE", "44") => place(false, 0, 4, 4, 1),
        ("M032LE", "121") => place(false, 0, 5, 4, 1),
        ("M005S", "111") => place(true, 0, 0, 4, 2),
        ("M005S", "143") => place(true, 2, 0, 4, 2),
        ("M005S", "23") => place(true, 0, 2, 4, 2),
        ("M005S", "112") => place(true, 1, 2, 4, 2),
        ("M005S", "134") => place(true, 2, 2, 4, 2),
        ("M005S", "113") => place(false, 0, 0, 4, 2),
        ("M005S", "24") => place(false, 0, 1, 4, 2),
        ("M005S", "42") => place(false, 0, 2, 4, 2),
        ("M005S", "41") => place(false, 0, 3, 4, 2),
        ("M005S", "122") => place(false, 0, 4, 4, 2),
        _ => return None,
    })
}

fn grid_layout(modules: &ConnectedModules) -> Option<Value> {
    if !modules.visible() {
        return None;
    }
    let mut placed = Vec::new();
    if modules.keys {
        placed.push((modules.places.keys?, GridModule::Keys));
    }
    if modules.knobs {
        placed.push((modules.places.knobs?, GridModule::Knobs));
    }
    if modules.rollers {
        placed.push((modules.places.rollers?, GridModule::Rollers));
    }
    if modules.dial {
        placed.push((modules.places.dial?, GridModule::Dial));
    }
    if modules.sliders {
        placed.push((modules.places.sliders?, GridModule::Sliders));
    }
    let horizontal = placed.iter().any(|(place, _)| place.horizontal);
    let vertical = placed.iter().any(|(place, _)| !place.horizontal);
    let (columns, rows) = match (horizontal, vertical) {
        (true, false) => (6i16, 4i16),
        (false, true) => (4, 6),
        _ => (6, 6),
    };
    let mut min_x = 0i16;
    let mut min_y = 0i16;
    let mut max_x = columns;
    let mut max_y = rows;
    for (place, _) in &placed {
        min_x = min_x.min(place.x);
        min_y = min_y.min(place.y);
        max_x = max_x.max(place.x + i16::try_from(place.w).unwrap_or(0));
        max_y = max_y.max(place.y + i16::try_from(place.h).unwrap_or(0));
    }
    let shift_x = -min_x;
    let shift_y = -min_y;
    let mut controls = Vec::new();
    for (place, module) in placed {
        let rect = Rect {
            x: u16::try_from(place.x + shift_x).unwrap_or(0) * UNIT,
            y: u16::try_from(place.y + shift_y).unwrap_or(0) * UNIT,
            w: place.w * UNIT,
            h: place.h * UNIT,
        };
        match module {
            GridModule::Keys => push_keys(&mut controls, rect),
            GridModule::Knobs => push_knobs(&mut controls, rect),
            GridModule::Rollers => push_rollers(&mut controls, rect),
            GridModule::Dial => push_dial(&mut controls, rect),
            GridModule::Sliders => push_sliders(&mut controls, rect),
        }
    }
    Some(serde_json::json!({
        "canvas": {
            "width": u16::try_from(max_x - min_x).unwrap_or(0) * UNIT,
            "height": u16::try_from(max_y - min_y).unwrap_or(0) * UNIT,
            "unit": "px",
        },
        "controls": controls,
    }))
}

enum GridModule {
    Keys,
    Knobs,
    Rollers,
    Dial,
    Sliders,
}

struct Rect {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}

fn push_keys(controls: &mut Vec<Value>, rect: Rect) {
    let gap = 8u16;
    let pad = 12u16;
    let columns = u16::from(COLUMNS);
    let rows = u16::from(ROWS);
    let inner_w = rect.w.saturating_sub(pad * 2 + gap * (columns - 1));
    let inner_h = rect.h.saturating_sub(pad * 2 + gap * (rows - 1));
    let key_size = (inner_w / columns).min(inner_h / rows);
    let block_w = key_size * columns + gap * (columns - 1);
    let block_h = key_size * rows + gap * (rows - 1);
    let left = rect.x + (rect.w - block_w) / 2;
    let top = rect.y + (rect.h - block_h) / 2;
    let pitch = key_size + gap;
    for position in 0..ROWS * COLUMNS {
        let column = u16::from(position % COLUMNS);
        let row = u16::from(position / COLUMNS);
        controls.push(sized_key(
            position,
            left + column * pitch,
            top + row * pitch,
            key_size,
        ));
    }
}

fn push_knobs(controls: &mut Vec<Value>, rect: Rect) {
    let gap = 16u16;
    let radius = (rect.w - gap * 3) / 4;
    controls.push(encoder(
        0,
        rect.x + rect.w / 2,
        rect.y + gap + radius,
        radius,
    ));
    controls.push(encoder(
        1,
        rect.x + gap + radius,
        rect.y + rect.h - gap - radius,
        radius,
    ));
    controls.push(encoder(
        2,
        rect.x + rect.w - gap - radius,
        rect.y + rect.h - gap - radius,
        radius,
    ));
}

fn push_rollers(controls: &mut Vec<Value>, rect: Rect) {
    let radius = (rect.h / 2).saturating_sub(12).min(CELL / 2);
    let cy = rect.y + rect.h / 2;
    controls.push(encoder(ROLLER_ENCODER, rect.x + rect.w / 4, cy, radius));
    controls.push(encoder(
        ROLLER_ENCODER + 1,
        rect.x + rect.w * 3 / 4,
        cy,
        radius,
    ));
}

fn push_dial(controls: &mut Vec<Value>, rect: Rect) {
    let radius = rect.w.min(rect.h) / 2 - 12;
    controls.push(encoder(
        BIG_KNOB_ENCODER,
        rect.x + rect.w / 2,
        rect.y + rect.h / 2,
        radius,
    ));
}

fn push_sliders(controls: &mut Vec<Value>, rect: Rect) {
    let slot = rect.w / 5;
    let radius = (slot / 2).saturating_sub(8);
    let cy = rect.y + rect.h / 2;
    for index in 0..5 {
        controls.push(encoder(
            SLIDER_ENCODER + index,
            rect.x + slot * u16::from(index) + slot / 2,
            cy,
            radius,
        ));
    }
}

fn key(position: u8, x: u16, y: u16) -> Value {
    sized_key(position, x, y, CELL)
}

fn sized_key(position: u8, x: u16, y: u16, size: u16) -> Value {
    serde_json::json!({
        "controller": "Keypad",
        "position": position,
        "shape": "rect",
        "x": x,
        "y": y,
        "width": size,
        "height": size,
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
            Some(DeviceEvent::SliderMoved {
                control: 2,
                value: 40
            })
        );
        assert_eq!(slider_delta(None, 40), None);
        assert_eq!(slider_delta(Some(40), 55), Some(15));
        assert_eq!(slider_delta(Some(55), 40), Some(-15));
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
        let payload = registration_payload(ConnectedModules::all());
        assert_eq!(payload["name"], "Master Hub");
        assert_eq!(payload["encoders"], 11);
        assert_eq!(payload["encoder"].as_array().unwrap().len(), 11);
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

    #[test]
    fn layout_omits_modules_that_are_not_connected() {
        let keys = registration_payload(ConnectedModules::from_names(["M4315", "M0064"]));
        assert_eq!(keys["rows"], 3);
        assert_eq!(keys["columns"], 5);
        assert_eq!(keys["encoders"], 11);
        let keys_layout = &keys["layout"];
        assert_eq!(keys_layout["canvas"]["width"], 756);
        assert_eq!(keys_layout["canvas"]["height"], 444);
        let keys_controls = keys_layout["controls"].as_array().unwrap();
        assert_eq!(keys_controls.len(), 15);
        assert!(keys_controls
            .iter()
            .all(|control| control["controller"] == "Keypad"));
        assert_bounds(keys_layout);

        let dial = registration_payload(ConnectedModules::from_names(["1xDial"]));
        let dial_controls = dial["layout"]["controls"].as_array().unwrap();
        assert_eq!(dial_controls.len(), 1);
        assert_eq!(dial_controls[0]["position"], 5);
        assert_eq!(dial["layout"]["canvas"]["width"], 600);
        assert_eq!(dial["layout"]["canvas"]["height"], 288);
        let dial_only = &dial_controls[0];
        assert_eq!(
            dial_only["cx"].as_u64().unwrap() - dial_only["r"].as_u64().unwrap(),
            288 + u64::from(GAP)
        );

        let with_keys = ConnectedModules {
            keys: true,
            ..ConnectedModules::default()
        };
        let both = registration_payload(ConnectedModules {
            knobs: true,
            dial: true,
            ..with_keys
        });
        let knobs_only = registration_payload(ConnectedModules {
            knobs: true,
            ..with_keys
        });
        let dial_only_with_keys = registration_payload(ConnectedModules {
            dial: true,
            ..with_keys
        });
        let both_controls = both["layout"]["controls"].as_array().unwrap();
        let knobs_only_controls = knobs_only["layout"]["controls"].as_array().unwrap();
        let dial_only_controls = dial_only_with_keys["layout"]["controls"]
            .as_array()
            .unwrap();
        assert_eq!(
            control(knobs_only_controls, "Encoder", 0)["cx"],
            control(both_controls, "Encoder", 0)["cx"]
        );
        assert_eq!(
            control(dial_only_controls, "Encoder", 5)["cx"],
            control(both_controls, "Encoder", 5)["cx"]
        );
        assert_eq!(knobs_only["layout"]["canvas"]["width"], 756);
        assert_eq!(dial_only_with_keys["layout"]["canvas"]["width"], 756);

        let knobs = registration_payload(ConnectedModules::from_names(["M003K", "1xDial"]));
        let knobs_controls = knobs["layout"]["controls"].as_array().unwrap();
        assert_eq!(knobs_controls.len(), 4);
        for position in [0u64, 1, 2, 5] {
            assert!(knobs_controls
                .iter()
                .any(|control| control["position"] == position));
        }
        assert!(knobs_controls
            .iter()
            .all(|control| control["controller"] == "Encoder"));
        let lower_right = control(knobs_controls, "Encoder", 2);
        let dial_control = control(knobs_controls, "Encoder", 5);
        assert_eq!(
            control(knobs_controls, "Encoder", 0)["cy"]
                .as_u64()
                .unwrap(),
            u64::from(CELL / 2)
        );
        let radius = lower_right["r"].as_u64().unwrap();
        assert_eq!(
            dial_control["cx"].as_u64().unwrap() - dial_control["r"].as_u64().unwrap(),
            lower_right["cx"].as_u64().unwrap() + radius + u64::from(GAP)
        );
        assert_bounds(&knobs["layout"]);

        let none = registration_payload(ConnectedModules::default());
        assert!(none["layout"]["controls"].as_array().unwrap().is_empty());
        assert_eq!(none["layout"]["canvas"]["width"], 0);
        assert_eq!(none["layout"]["canvas"]["height"], 0);
        assert!(!ConnectedModules::default().visible());
        assert!(!ConnectedModules::from_names(["M0064"]).visible());
        assert!(ConnectedModules::from_names(["M005S"]).visible());
    }

    #[test]
    fn layout_puts_a_module_on_its_pogo_site() {
        let vertical = |module: &str, site: &str| {
            let mut sites = serde_json::Map::new();
            sites.insert(module.to_string(), site.into());
            ConnectedModules::from_report(&serde_json::json!({
                "modules": [module],
                "sites": sites,
            }))
        };
        let dial = registration_payload(vertical("1xDial", "122"));
        assert_eq!(dial["layout"]["canvas"]["width"], 4 * u64::from(UNIT));
        assert_eq!(dial["layout"]["canvas"]["height"], 6 * u64::from(UNIT));
        let dial_control = &dial["layout"]["controls"][0];
        assert_eq!(dial_control["position"], 5);
        assert_eq!(dial_control["cx"], 2 * u64::from(UNIT) + u64::from(UNIT));
        assert_eq!(dial_control["cy"], 4 * u64::from(UNIT) + u64::from(UNIT));

        let both = registration_payload(ConnectedModules::from_report(&serde_json::json!({
            "modules": ["M003K", "1xDial"],
            "sites": { "M003K": "123", "1xDial": "122" }
        })));
        let swapped = registration_payload(ConnectedModules::from_report(&serde_json::json!({
            "modules": ["M003K", "1xDial"],
            "sites": { "M003K": "122", "1xDial": "123" }
        })));
        let both_controls = both["layout"]["controls"].as_array().unwrap();
        let swapped_controls = swapped["layout"]["controls"].as_array().unwrap();
        assert!(
            control(both_controls, "Encoder", 0)["cx"].as_u64().unwrap()
                < control(both_controls, "Encoder", 5)["cx"].as_u64().unwrap()
        );
        assert!(
            control(swapped_controls, "Encoder", 0)["cx"]
                .as_u64()
                .unwrap()
                > control(swapped_controls, "Encoder", 5)["cx"]
                    .as_u64()
                    .unwrap()
        );
        assert_eq!(
            control(both_controls, "Encoder", 5)["cx"],
            dial_control["cx"]
        );

        let keys = registration_payload(vertical("M4315", "42"));
        let key = &keys["layout"]["controls"][0];
        assert_eq!(key["controller"], "Keypad");
        assert!(key["y"].as_u64().unwrap() >= u64::from(UNIT));
        assert!(key["y"].as_u64().unwrap() < 2 * u64::from(UNIT));

        let rollers = registration_payload(ConnectedModules::from_report(&serde_json::json!({
            "modules": ["M032LE"],
            "sites": { "M032LE": "141" }
        })));
        assert_eq!(rollers["layout"]["canvas"]["width"], 6 * u64::from(UNIT));
        assert_eq!(rollers["layout"]["canvas"]["height"], 4 * u64::from(UNIT));
        let roller_controls = rollers["layout"]["controls"].as_array().unwrap();
        assert!(
            control(roller_controls, "Encoder", 3)["cx"]
                .as_u64()
                .unwrap()
                > 2 * u64::from(UNIT)
        );

        let left_of_center =
            registration_payload(ConnectedModules::from_report(&serde_json::json!({
                "modules": ["M003K"],
                "sites": { "M003K": "43" }
            })));
        let right_of_center =
            registration_payload(ConnectedModules::from_report(&serde_json::json!({
                "modules": ["1xDial"],
                "sites": { "1xDial": "112" }
            })));
        let bottom_left = registration_payload(ConnectedModules::from_report(&serde_json::json!({
            "modules": ["M003K"],
            "sites": { "M003K": "44" }
        })));
        let bottom_center =
            registration_payload(ConnectedModules::from_report(&serde_json::json!({
                "modules": ["M003K"],
                "sites": { "M003K": "23" }
            })));
        let bottom_right =
            registration_payload(ConnectedModules::from_report(&serde_json::json!({
                "modules": ["M003K"],
                "sites": { "M003K": "134" }
            })));
        let knob_x = |payload: &Value| payload["layout"]["controls"][0]["cx"].as_u64().unwrap();
        let bottom_left = knob_x(&bottom_left);
        let left_of_center = knob_x(&left_of_center);
        let bottom_center = knob_x(&bottom_center);
        let right_of_center = knob_x(&right_of_center);
        let bottom_right = knob_x(&bottom_right);
        assert!(bottom_left < left_of_center);
        assert!(left_of_center < bottom_center);
        assert!(bottom_center < right_of_center);
        assert!(right_of_center < bottom_right);

        let keypad_center =
            registration_payload(ConnectedModules::from_report(&serde_json::json!({
                "modules": ["M4315"],
                "sites": { "M4315": "112" }
            })));
        let keypad_left = registration_payload(ConnectedModules::from_report(&serde_json::json!({
            "modules": ["M4315"],
            "sites": { "M4315": "23" }
        })));
        let keypad_right =
            registration_payload(ConnectedModules::from_report(&serde_json::json!({
                "modules": ["M4315"],
                "sites": { "M4315": "134" }
            })));
        let key_x = |payload: &Value| payload["layout"]["controls"][0]["x"].as_u64().unwrap();
        assert!(key_x(&keypad_left) < key_x(&keypad_center));
        assert!(key_x(&keypad_center) < key_x(&keypad_right));

        let above_left = registration_payload(ConnectedModules::from_report(&serde_json::json!({
            "modules": ["M4315"],
            "sites": { "M4315": "111" }
        })));
        let above_right = registration_payload(ConnectedModules::from_report(&serde_json::json!({
            "modules": ["M4315"],
            "sites": { "M4315": "143" }
        })));
        let above_vertical =
            registration_payload(ConnectedModules::from_report(&serde_json::json!({
                "modules": ["M4315"],
                "sites": { "M4315": "113" }
            })));
        assert_eq!(above_left["layout"]["canvas"]["width"], 6 * u64::from(UNIT));
        assert_eq!(
            above_left["layout"]["canvas"]["height"],
            5 * u64::from(UNIT)
        );
        assert!(key_x(&above_left) < key_x(&above_right));
        assert_eq!(
            above_vertical["layout"]["canvas"]["width"],
            4 * u64::from(UNIT)
        );
        assert_eq!(
            above_vertical["layout"]["canvas"]["height"],
            7 * u64::from(UNIT)
        );

        let sliders = registration_payload(vertical("M005S", "113"));
        let slider_controls = sliders["layout"]["controls"].as_array().unwrap();
        assert_eq!(slider_controls.len(), 5);
        assert_eq!(slider_controls[0]["position"], 6);
        assert!(
            slider_controls[0]["cx"].as_u64().unwrap() < slider_controls[4]["cx"].as_u64().unwrap()
        );

        let unknown = registration_payload(ConnectedModules::from_report(
            &serde_json::json!({ "modules": ["1xDial"], "sites": { "1xDial": "nope" } }),
        ));
        assert_eq!(unknown["layout"]["canvas"]["width"], 600);
        assert_eq!(unknown["layout"]["canvas"]["height"], 288);
    }

    fn assert_bounds(layout: &Value) {
        let controls = layout["controls"].as_array().unwrap();
        let width = layout["canvas"]["width"].as_u64().unwrap();
        let height = layout["canvas"]["height"].as_u64().unwrap();
        let mut right = 0u64;
        let mut bottom = 0u64;
        let mut left = u64::MAX;
        let mut top = u64::MAX;
        for control in controls {
            let (x, y, far_x, far_y) = bounds(control);
            assert!(far_x <= width);
            assert!(far_y <= height);
            left = left.min(x);
            top = top.min(y);
            right = right.max(far_x);
            bottom = bottom.max(far_y);
        }
        assert_eq!(left, 0);
        assert_eq!(top, 0);
        assert_eq!(right, width);
        assert_eq!(bottom, height);
    }

    fn bounds(control: &Value) -> (u64, u64, u64, u64) {
        if control["shape"] == "circle" {
            let cx = control["cx"].as_u64().unwrap();
            let cy = control["cy"].as_u64().unwrap();
            let radius = control["r"].as_u64().unwrap();
            return (cx - radius, cy - radius, cx + radius, cy + radius);
        }
        let x = control["x"].as_u64().unwrap();
        let y = control["y"].as_u64().unwrap();
        (
            x,
            y,
            x + control["width"].as_u64().unwrap(),
            y + control["height"].as_u64().unwrap(),
        )
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
        assert!(image_command(
            DEVICE_ID,
            Some("Encoder"),
            Some(0),
            Some("data:image/png;base64,xx")
        )
        .is_empty());
        let png = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]));
        let mut bytes = Vec::new();
        png.write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
        let data_url = format!("data:image/png;base64,{}", STANDARD.encode(bytes));
        let command = image_command(DEVICE_ID, Some("Keypad"), Some(0), Some(&data_url));
        assert_eq!(command.len(), 1);
        let value: Value = serde_json::from_str(&command[0]).unwrap();
        assert_eq!(value["action"], "set_button_image");
        assert_eq!(value["payload"]["control"], 0);
        let screen = image_command(DEVICE_ID, Some("Encoder"), Some(5), Some(&data_url));
        assert_eq!(screen.len(), 1);
        let screen: Value = serde_json::from_str(&screen[0]).unwrap();
        assert_eq!(screen["action"], "set_screen_image");
        assert_eq!(screen["payload"]["module"]["model"], "M0064");
        let jpeg = STANDARD
            .decode(value["payload"]["data_base64"].as_str().unwrap())
            .unwrap();
        assert!(jpeg.starts_with(&[0xFF, 0xD8]));
    }

    #[test]
    fn missing_image_blanks_the_control_or_the_whole_device() {
        let key = image_command(DEVICE_ID, Some("Keypad"), Some(4), None);
        assert_eq!(key.len(), 1);
        let key: Value = serde_json::from_str(&key[0]).unwrap();
        assert_eq!(key["action"], "set_button_image");
        assert_eq!(key["payload"]["control"], 4);
        assert_eq!(decoded_size(&key), (72, 72));

        let dial = image_command(DEVICE_ID, Some("Encoder"), Some(BIG_KNOB_ENCODER), None);
        assert_eq!(dial.len(), 1);
        let dial: Value = serde_json::from_str(&dial[0]).unwrap();
        assert_eq!(dial["action"], "set_screen_image");
        assert_eq!(decoded_size(&dial), (240, 240));

        assert!(image_command(DEVICE_ID, Some("Encoder"), Some(0), None).is_empty());
        assert!(image_command("sd-other", None, None, None).is_empty());

        let cleared = image_command(DEVICE_ID, None, None, None);
        assert_eq!(cleared.len(), 16);
        for (control, command) in cleared.iter().take(15).enumerate() {
            let value: Value = serde_json::from_str(command).unwrap();
            assert_eq!(value["action"], "set_button_image");
            assert_eq!(value["payload"]["control"], control as u64);
            assert_eq!(decoded_size(&value), (72, 72));
        }
        let screen: Value = serde_json::from_str(cleared.last().unwrap()).unwrap();
        assert_eq!(screen["action"], "set_screen_image");
        assert_eq!(decoded_size(&screen), (240, 240));
    }

    fn decoded_size(command: &Value) -> (u32, u32) {
        let jpeg = STANDARD
            .decode(command["payload"]["data_base64"].as_str().unwrap())
            .unwrap();
        let image = image::load_from_memory(&jpeg).unwrap();
        (image.width(), image.height())
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
