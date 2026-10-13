//! USB sessions for the modules plugged into the hub.
//!
//! HID report bytes live in the sibling modules and in
//! `cooltek-sdk-probe/docs/MasterHUB/modules/`.

mod dial;
mod m0;
mod m4315;
mod site;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};

use crate::hub::Hub;
use crate::protocol::{CommandSink, ControlTarget, ModuleModel, OutputCommand};

use dial::{
    dial_input, encode_screen, screen_jpeg, software_mode_feature as dial_software_mode, DialInput,
};
use m0::{
    base_knob_rotated_payload, base_knob_state_payload, encode_led, input_payload, input_report,
    knob_control, knob_led_index, knob_rotated_payload, knob_state_payload, led_index_all,
    roller_payload, slider_payload, InputReport, LedCommand, LED_BASE, LED_KNOB, LED_SLIDER,
    PID_BASE, PID_KNOB, PID_ROLLER, PID_SLIDER,
};
use m4315::{
    backlight_feature, button_jpeg_for_device, diff_keys, encode_button_image,
    key_snapshot_from_report, key_state_payload, released_snapshot, software_mode_feature,
    KeySnapshot, PID as PID_KEYS, REPORT_LEN, VID,
};

const READ_LEN: usize = 512;
const READ_TIMEOUT_MS: i32 = 50;
const INITIAL_WAIT: Duration = Duration::from_millis(250);
const MAX_WAIT: Duration = Duration::from_secs(2);

/// Latest JPEG for each key. A burst replaces the pending image instead of
/// filling a queue while a write is in progress.
struct ImageQueue {
    latest: Mutex<BTreeMap<u8, Vec<u8>>>,
}

impl ImageQueue {
    fn new() -> Self {
        Self {
            latest: Mutex::new(BTreeMap::new()),
        }
    }

    fn store(&self, control: u8, jpeg: Vec<u8>) {
        self.latest
            .lock()
            .expect("image queue")
            .insert(control, jpeg);
    }

    fn take(&self) -> BTreeMap<u8, Vec<u8>> {
        std::mem::take(&mut *self.latest.lock().expect("image queue"))
    }

    fn clear(&self) {
        self.latest.lock().expect("image queue").clear();
    }
}

struct LedQueue {
    pending: Mutex<Vec<LedCommand>>,
}

impl LedQueue {
    fn new() -> Self {
        Self {
            pending: Mutex::new(Vec::new()),
        }
    }

    fn store(&self, command: LedCommand) {
        let mut pending = self.pending.lock().expect("led queue");
        if let Some(slot) = pending.iter_mut().find(|item| command.replaces(item)) {
            *slot = command;
        } else {
            pending.push(command);
        }
    }

    fn take(&self) -> Vec<LedCommand> {
        std::mem::take(&mut *self.pending.lock().expect("led queue"))
    }

    fn clear(&self) {
        self.pending.lock().expect("led queue").clear();
    }
}

struct BacklightQueue {
    latest: Mutex<Option<u8>>,
}

impl BacklightQueue {
    fn new() -> Self {
        Self {
            latest: Mutex::new(None),
        }
    }

    fn store(&self, brightness: u8) {
        *self.latest.lock().expect("backlight queue") = Some(brightness);
    }

    fn take(&self) -> Option<u8> {
        self.latest.lock().expect("backlight queue").take()
    }

    fn clear(&self) {
        *self.latest.lock().expect("backlight queue") = None;
    }
}

struct ScreenQueue {
    latest: Mutex<Option<Vec<u8>>>,
}

impl ScreenQueue {
    fn new() -> Self {
        Self {
            latest: Mutex::new(None),
        }
    }

    fn store(&self, jpeg: Vec<u8>) {
        *self.latest.lock().expect("screen queue") = Some(jpeg);
    }

    fn take(&self) -> Option<Vec<u8>> {
        self.latest.lock().expect("screen queue").take()
    }

    fn clear(&self) {
        *self.latest.lock().expect("screen queue") = None;
    }
}

pub struct UsbSink {
    images: Arc<ImageQueue>,
    backlight: Arc<BacklightQueue>,
    knob_leds: Arc<LedQueue>,
    slider_leds: Arc<LedQueue>,
    base_leds: Arc<LedQueue>,
    dial_screen: Arc<ScreenQueue>,
}

impl UsbSink {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            images: Arc::new(ImageQueue::new()),
            backlight: Arc::new(BacklightQueue::new()),
            knob_leds: Arc::new(LedQueue::new()),
            slider_leds: Arc::new(LedQueue::new()),
            base_leds: Arc::new(LedQueue::new()),
            dial_screen: Arc::new(ScreenQueue::new()),
        })
    }
}

/// Build the sink, then start the device threads once the hub owns that sink.
pub fn prepare() -> (Arc<UsbSink>, impl FnOnce(Arc<Hub>)) {
    let sink = UsbSink::new();
    let start_sink = Arc::clone(&sink);
    let start = move |hub: Arc<Hub>| {
        spawn(
            "m4315-usb",
            "M4315",
            PID_KEYS,
            &hub,
            &start_sink,
            serve_keys,
        );
        spawn(
            "m003k-usb",
            "M003K",
            PID_KNOB,
            &hub,
            &start_sink,
            serve_knobs,
        );
        spawn(
            "m032le-usb",
            "M032LE",
            PID_ROLLER,
            &hub,
            &start_sink,
            serve_rollers,
        );
        spawn(
            "m005s-usb",
            "M005S",
            PID_SLIDER,
            &hub,
            &start_sink,
            serve_sliders,
        );
        spawn(
            "m0064-usb",
            "M0064",
            PID_BASE,
            &hub,
            &start_sink,
            serve_base,
        );
        spawn(
            "dial-usb",
            "1xDial",
            dial::PID,
            &hub,
            &start_sink,
            serve_dial,
        );
    };
    (sink, start)
}

fn spawn(
    name: &'static str,
    label: &'static str,
    pid: u16,
    hub: &Arc<Hub>,
    sink: &Arc<UsbSink>,
    serve: fn(&HidDevice, &Hub, &UsbSink, &mut Session) -> Result<(), String>,
) {
    let hub = Arc::clone(hub);
    let sink = Arc::clone(sink);
    thread::Builder::new()
        .name(name.to_string())
        .spawn(move || supervise(label, pid, hub, sink, serve))
        .unwrap_or_else(|_| panic!("spawn {name}"));
}

struct Session {
    wait: Duration,
    last_error: Option<String>,
    module: &'static str,
    site: Option<String>,
    open: bool,
}

impl Session {
    /// First successful open in this attempt. Later calls in the same session
    /// do not publish again.
    fn connected(&mut self, hub: &Hub) {
        self.wait = INITIAL_WAIT;
        self.last_error = None;
        if !self.open {
            self.open = true;
            hub.set_module_site(self.module, self.site.clone());
        }
    }

    fn disconnected(&mut self, hub: &Hub) {
        self.site = None;
        if self.open {
            self.open = false;
            hub.set_module_open(self.module, false);
        }
    }
}

fn supervise(
    label: &'static str,
    pid: u16,
    hub: Arc<Hub>,
    sink: Arc<UsbSink>,
    serve: fn(&HidDevice, &Hub, &UsbSink, &mut Session) -> Result<(), String>,
) {
    let mut session = Session {
        wait: INITIAL_WAIT,
        last_error: None,
        module: label,
        site: None,
        open: false,
    };
    loop {
        let result = match open_pid(pid) {
            Ok(device) => {
                session.site = device_site(&device);
                if let Some(site) = session.site.as_deref() {
                    eprintln!("masterhubd: {label} site {site}");
                }
                serve(&device, &hub, &sink, &mut session)
            }
            Err(err) => Err(err),
        };
        session.disconnected(&hub);
        if let Err(err) = result {
            if err.contains("write failed") {
                session.wait = MAX_WAIT;
            }
            if session.last_error.as_deref() != Some(err.as_str()) {
                eprintln!("masterhubd: {label}: {err}");
                session.last_error = Some(err);
            }
            thread::sleep(session.wait);
            session.wait = (session.wait * 2).min(MAX_WAIT);
        }
    }
}

impl CommandSink for UsbSink {
    fn accept(&self, command: &OutputCommand) {
        match command {
            OutputCommand::ButtonImage {
                module,
                control,
                jpeg,
            } if module.model == ModuleModel::M4315 => {
                self.images.store(*control, jpeg.clone());
            }
            OutputCommand::Backlight { module, brightness }
                if module.model == ModuleModel::M4315 =>
            {
                self.backlight.store(*brightness);
            }
            OutputCommand::ScreenImage { module, jpeg } if module.model == ModuleModel::M0064 => {
                self.dial_screen.store(jpeg.clone());
            }
            OutputCommand::LedMode {
                module,
                control,
                mode,
                save,
            } => {
                if let Some(queue) = self.leds_for(module.model) {
                    queue.store(LedCommand::mode(led_index(*control), *mode, *save));
                }
            }
            OutputCommand::LedColor {
                module,
                control,
                r,
                g,
                b,
                save,
            } => {
                if let Some(queue) = self.leds_for(module.model) {
                    queue.store(LedCommand::color(led_index(*control), *r, *g, *b, *save));
                }
            }
            OutputCommand::LedBrightness {
                module,
                control,
                brightness,
            } => {
                if let Some(queue) = self.leds_for(module.model) {
                    queue.store(LedCommand::brightness(led_index(*control), *brightness));
                }
            }
            _ => {}
        }
    }
}

impl UsbSink {
    fn leds_for(&self, model: ModuleModel) -> Option<&LedQueue> {
        match model {
            ModuleModel::M003K => Some(&self.knob_leds),
            ModuleModel::M005S => Some(&self.slider_leds),
            ModuleModel::M0064 => Some(&self.base_leds),
            _ => None,
        }
    }
}

fn led_index(control: ControlTarget) -> u8 {
    match control {
        ControlTarget::All => led_index_all(),
        ControlTarget::Index(index) => index,
    }
}

#[cfg(any(target_os = "linux", test))]
const UDEV_RULES: &str = include_str!("../../../../udev/60-openmasterhub.rules");

fn device_site(device: &HidDevice) -> Option<String> {
    let serial = device.get_serial_number_string().ok().flatten()?;
    let output = std::process::Command::new("ioreg")
        .args(["-p", "IOUSB", "-w0", "-l"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let location = site::locations_by_serial(&text).get(&serial).copied()?;
    Some(site::site_key(location))
}

fn open_pid(pid: u16) -> Result<HidDevice, String> {
    let api = HidApi::new().map_err(|err| format!("HID init failed: {err}"))?;
    match api.open(VID, pid) {
        Ok(device) => Ok(device),
        Err(err) => {
            let text = err.to_string();
            if access_denied(&text) && install_udev_rules_once() {
                let api = HidApi::new().map_err(|err| format!("HID init failed: {err}"))?;
                if let Ok(device) = api.open(VID, pid) {
                    return Ok(device);
                }
            }
            Err(open_error(&text))
        }
    }
}

fn access_denied(text: &str) -> bool {
    text.contains("Permission denied")
}

/// One password prompt per process, the first time a module cannot be opened.
/// Later starts do not ask when the rule is already installed.
fn install_udev_rules_once() -> bool {
    static ATTEMPT: Mutex<Option<bool>> = Mutex::new(None);
    let mut attempt = ATTEMPT.lock().expect("udev install");
    if let Some(installed) = *attempt {
        return installed;
    }
    let installed = offer_udev_rules();
    *attempt = Some(installed);
    installed
}

#[cfg(target_os = "linux")]
fn offer_udev_rules() -> bool {
    eprintln!(
        "masterhubd: the Master Hub hidraw devices are not writable. Approve the prompt to install the udev rule."
    );
    let script = format!(
        "cat > /etc/udev/rules.d/60-openmasterhub.rules << 'OPENMASTERHUB_UDEV_RULES'\n\
         {UDEV_RULES}\
         OPENMASTERHUB_UDEV_RULES\n\
         chmod 644 /etc/udev/rules.d/60-openmasterhub.rules\n\
         udevadm control --reload-rules\n\
         udevadm trigger --subsystem-match=hidraw\n"
    );
    match std::process::Command::new("pkexec")
        .args(["sh", "-c", &script])
        .status()
    {
        Ok(status) if status.success() => {
            eprintln!("masterhubd: installed 60-openmasterhub.rules");
            true
        }
        Ok(status) => {
            eprintln!("masterhubd: udev rule was not installed ({status})");
            false
        }
        Err(error) => {
            eprintln!("masterhubd: could not ask for permission to install the udev rule: {error}");
            false
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn offer_udev_rules() -> bool {
    false
}

/// Linux hidraw nodes for these modules are root-only until
/// `udev/60-openmasterhub.rules` is installed. Without that rule the open
/// fails and the software-mode feature is never sent.
fn open_error(text: &str) -> String {
    if access_denied(text) {
        format!(
            "permission denied opening the module. Install udev/60-openmasterhub.rules into /etc/udev/rules.d/ and replug ({text})"
        )
    } else {
        format!("waiting for device: {text}")
    }
}

fn serve_keys(
    device: &HidDevice,
    hub: &Hub,
    sink: &UsbSink,
    session: &mut Session,
) -> Result<(), String> {
    device
        .send_feature_report(&software_mode_feature())
        .map_err(|err| format!("software mode failed: {err}"))?;
    session.connected(hub);
    eprintln!("masterhubd: M4315 connected");
    let mut keys = released_snapshot();
    let mut buf = [0u8; READ_LEN];
    loop {
        drain_backlight(device, &sink.backlight)?;
        drain_images(device, &sink.images)?;
        match device.read_timeout(&mut buf, READ_TIMEOUT_MS) {
            Ok(0) => {}
            Ok(len) => {
                if let Some(next) = key_snapshot_from_report(&buf[..len]) {
                    emit_key_changes(hub, &keys, &next);
                    keys = next;
                }
            }
            Err(err) => return Err(format!("disconnected: {err}")),
        }
    }
}

fn drain_backlight(device: &HidDevice, backlight: &BacklightQueue) -> Result<(), String> {
    let Some(brightness) = backlight.take() else {
        return Ok(());
    };
    if let Err(err) = device.send_feature_report(&backlight_feature(brightness)) {
        backlight.clear();
        return Err(format!("backlight failed: {err}"));
    }
    Ok(())
}

fn drain_images(device: &HidDevice, images: &ImageQueue) -> Result<(), String> {
    for (control, jpeg) in images.take() {
        let Some(jpeg) = button_jpeg_for_device(&jpeg) else {
            eprintln!("masterhubd: skipping M4315 key {control}: jpeg could not be decoded");
            continue;
        };
        if let Err(err) = write_chunks(device, &encode_button_image(control, &jpeg), "image") {
            images.clear();
            return Err(err);
        }
    }
    Ok(())
}

fn serve_knobs(
    device: &HidDevice,
    hub: &Hub,
    sink: &UsbSink,
    session: &mut Session,
) -> Result<(), String> {
    session.connected(hub);
    eprintln!("masterhubd: M003K connected");
    let mut pressed = [false; 3];
    read_loop(device, |report| {
        drain_leds(device, &sink.knob_leds, LED_KNOB)?;
        if let Some(input) = input_report(report) {
            emit_knobs(hub, &mut pressed, input);
        }
        Ok(())
    })
}

fn serve_rollers(
    device: &HidDevice,
    hub: &Hub,
    _sink: &UsbSink,
    session: &mut Session,
) -> Result<(), String> {
    session.connected(hub);
    eprintln!("masterhubd: M032LE connected");
    let mut seen = Vec::new();
    read_loop(device, |report| {
        if !report.is_empty() {
            note_report("M032LE", &mut seen, report);
        }
        match input_payload(report) {
            Some(InputReport::Rollers(turns)) if !turns.is_empty() => {
                for (control, up) in turns {
                    hub.emit_event("roller_rotated", &roller_payload(control, up));
                }
            }
            _ => {}
        }
        Ok(())
    })
}

fn serve_sliders(
    device: &HidDevice,
    hub: &Hub,
    sink: &UsbSink,
    session: &mut Session,
) -> Result<(), String> {
    session.connected(hub);
    eprintln!("masterhubd: M005S connected");
    let mut values = [None; 5];
    read_loop(device, |report| {
        drain_leds(device, &sink.slider_leds, LED_SLIDER)?;
        if let Some(InputReport::Sliders(next)) = input_report(report) {
            for (control, value) in next.into_iter().enumerate() {
                if values[control] == Some(value) {
                    continue;
                }
                values[control] = Some(value);
                hub.emit_event("slider_moved", &slider_payload(control as u8, value));
            }
        }
        Ok(())
    })
}

fn serve_base(
    device: &HidDevice,
    hub: &Hub,
    sink: &UsbSink,
    session: &mut Session,
) -> Result<(), String> {
    session.connected(hub);
    eprintln!("masterhubd: M0064 connected");
    read_loop(device, |_report| {
        drain_leds(device, &sink.base_leds, LED_BASE)
    })
}

/// One screen JPEG already split into output reports.
struct DialScreen {
    reports: Vec<[u8; REPORT_LEN]>,
    next: usize,
}

fn serve_dial(
    device: &HidDevice,
    hub: &Hub,
    sink: &UsbSink,
    session: &mut Session,
) -> Result<(), String> {
    device
        .send_feature_report(&dial_software_mode())
        .map_err(|err| format!("software mode failed: {err}"))?;
    session.connected(hub);
    eprintln!("masterhubd: 1xDial connected");
    let mut pressed = false;
    let mut seen = Vec::new();
    // The queue keeps only the newest JPEG. A frame already being written is
    // finished, then that newest JPEG is the next frame, so a burst does not
    // play back every intermediate picture.
    let mut screen: Option<DialScreen> = None;
    let mut shown: Option<Vec<u8>> = None;
    let mut buf = [0u8; READ_LEN];
    loop {
        if screen.is_none() {
            screen = next_screen(&sink.dial_screen, &mut shown);
        }
        let wrote = if let Some(current) = screen.as_mut() {
            // One chunk, then read. A full JPEG is dozens of reports, and
            // writing them all here stalls encoder input for the whole transfer.
            if let Err(err) = write_bytes(device, &current.reports[current.next], "screen") {
                sink.dial_screen.clear();
                return Err(err);
            }
            current.next += 1;
            true
        } else {
            false
        };
        if screen
            .as_ref()
            .is_some_and(|current| current.next == current.reports.len())
        {
            screen = None;
        }
        if wrote {
            while let Some(report) = read_report(device, &mut buf, 0)? {
                emit_dial(hub, &mut pressed, &mut seen, report);
            }
        } else if let Some(report) = read_report(device, &mut buf, READ_TIMEOUT_MS)? {
            emit_dial(hub, &mut pressed, &mut seen, report);
        }
    }
}

fn next_screen(queue: &ScreenQueue, shown: &mut Option<Vec<u8>>) -> Option<DialScreen> {
    let jpeg = queue.take()?;
    if shown.as_deref() == Some(jpeg.as_slice()) {
        return None;
    }
    let Some(fitted) = screen_jpeg(&jpeg) else {
        eprintln!("masterhubd: skipping 1xDial screen: jpeg could not be decoded");
        return None;
    };
    let reports = encode_screen(&fitted);
    if reports.is_empty() {
        return None;
    }
    *shown = Some(jpeg);
    Some(DialScreen { reports, next: 0 })
}

fn emit_dial(hub: &Hub, pressed: &mut bool, seen: &mut Vec<[u8; 8]>, report: &[u8]) {
    if report.is_empty() {
        return;
    }
    note_report("1xDial", seen, report);
    match dial_input(report) {
        Some(DialInput::Turn(clockwise)) => {
            hub.emit_event("knob_rotated", &base_knob_rotated_payload(clockwise));
        }
        Some(DialInput::Press(next)) if *pressed != next => {
            *pressed = next;
            hub.emit_event("knob_state", &base_knob_state_payload(next));
        }
        _ => {}
    }
}

fn read_report<'a>(
    device: &HidDevice,
    buf: &'a mut [u8],
    timeout_ms: i32,
) -> Result<Option<&'a [u8]>, String> {
    match device.read_timeout(buf, timeout_ms) {
        Ok(0) => Ok(None),
        Ok(len) => Ok(Some(&buf[..len])),
        Err(err) => Err(format!("disconnected: {err}")),
    }
}

fn read_loop(
    device: &HidDevice,
    mut step: impl FnMut(&[u8]) -> Result<(), String>,
) -> Result<(), String> {
    let mut buf = [0u8; READ_LEN];
    loop {
        step(&[])?;
        match device.read_timeout(&mut buf, READ_TIMEOUT_MS) {
            Ok(0) => {}
            Ok(len) => step(&buf[..len])?,
            Err(err) => return Err(format!("disconnected: {err}")),
        }
    }
}

fn drain_leds(device: &HidDevice, queue: &LedQueue, module: u8) -> Result<(), String> {
    let pending = queue.take();
    for command in pending {
        let command = if module == LED_KNOB {
            command.map_index(knob_led_index)
        } else {
            command
        };
        let report = encode_led(module, &command);
        if let Err(err) = write_bytes(device, &report, "led") {
            queue.clear();
            return Err(err);
        }
    }
    Ok(())
}

fn write_chunks<const N: usize>(
    device: &HidDevice,
    reports: &[[u8; N]],
    kind: &str,
) -> Result<(), String> {
    for report in reports {
        write_bytes(device, report, kind)?;
    }
    Ok(())
}

fn write_bytes(device: &HidDevice, report: &[u8], kind: &str) -> Result<(), String> {
    let written = device
        .write(report)
        .map_err(|err| format!("{kind} write failed: {err}"))?;
    if written != report.len() {
        return Err(format!("short HID write {written}"));
    }
    Ok(())
}

fn emit_key_changes(hub: &Hub, previous: &KeySnapshot, next: &KeySnapshot) {
    for (control, pressed) in diff_keys(previous, next) {
        hub.emit_event("key_state", &key_state_payload(control, pressed));
    }
}

fn emit_knobs(hub: &Hub, pressed: &mut [bool; 3], input: InputReport) {
    match input {
        InputReport::KnobTurns(turns) => {
            for (index, clockwise) in turns {
                hub.emit_event(
                    "knob_rotated",
                    &knob_rotated_payload(knob_control(index), clockwise),
                );
            }
        }
        InputReport::KnobPress(next) => {
            let mut mapped = [false; 3];
            for (index, pressed_now) in next.into_iter().enumerate() {
                mapped[usize::from(knob_control(index as u8))] = pressed_now;
            }
            for (control, (was, now)) in pressed.iter_mut().zip(mapped).enumerate() {
                if *was == now {
                    continue;
                }
                *was = now;
                hub.emit_event("knob_state", &knob_state_payload(control as u8, now));
            }
        }
        _ => {}
    }
}

fn note_report(label: &str, seen: &mut Vec<[u8; 8]>, report: &[u8]) {
    if seen.len() >= 8 {
        return;
    }
    let mut key = [0u8; 8];
    let len = report.len().min(key.len());
    key[..len].copy_from_slice(&report[..len]);
    if seen.contains(&key) {
        return;
    }
    seen.push(key);
    let hex = key[..len]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    eprintln!("masterhubd: {label} report {hex}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::NoopCommandSink;
    use crate::protocol::{LightingMode, ModuleRef};

    fn module(model: ModuleModel) -> ModuleRef {
        ModuleRef {
            model,
            id: "keys".to_string(),
        }
    }

    #[test]
    fn permission_denied_names_the_udev_rule() {
        let message = open_error(
            "hidapi error: failed to open device with path /dev/hidraw9: Permission denied (os error 13)",
        );
        assert!(message.contains("60-openmasterhub.rules"));
        assert!(message.contains("/dev/hidraw9"));
        assert!(UDEV_RULES.ends_with('\n'));
        assert!(!UDEV_RULES.contains("OPENMASTERHUB_UDEV_RULES"));
        for pid in ["01f6", "01fe", "0200", "0202", "0204", "0206"] {
            assert!(UDEV_RULES.contains(pid), "{pid}");
        }
        assert_eq!(
            open_error("hidapi error: device not found"),
            "waiting for device: hidapi error: device not found"
        );
    }

    #[test]
    fn sink_keeps_the_latest_image_and_led_per_target() {
        let sink = UsbSink::new();
        sink.accept(&OutputCommand::ButtonImage {
            module: module(ModuleModel::M4315),
            control: 3,
            jpeg: vec![0xff, 0xd8],
        });
        sink.accept(&OutputCommand::ButtonImage {
            module: module(ModuleModel::M4315),
            control: 3,
            jpeg: vec![0xff, 0xd8, 0xff],
        });
        sink.accept(&OutputCommand::ScreenImage {
            module: module(ModuleModel::M4315),
            jpeg: vec![0xff, 0xd8, 0x11],
        });
        sink.accept(&OutputCommand::ScreenImage {
            module: module(ModuleModel::M0064),
            jpeg: vec![0xff, 0xd8, 0x22],
        });
        sink.accept(&OutputCommand::LedColor {
            module: module(ModuleModel::M003K),
            control: ControlTarget::Index(1),
            r: 1,
            g: 2,
            b: 3,
            save: false,
        });
        sink.accept(&OutputCommand::LedColor {
            module: module(ModuleModel::M003K),
            control: ControlTarget::Index(1),
            r: 9,
            g: 8,
            b: 7,
            save: true,
        });
        sink.accept(&OutputCommand::LedBrightness {
            module: module(ModuleModel::M0064),
            control: ControlTarget::Index(0),
            brightness: 40,
        });
        sink.accept(&OutputCommand::LedMode {
            module: module(ModuleModel::M005S),
            control: ControlTarget::All,
            mode: LightingMode::Static,
            save: false,
        });
        sink.accept(&OutputCommand::Backlight {
            module: module(ModuleModel::M4315),
            brightness: 10,
        });
        sink.accept(&OutputCommand::Backlight {
            module: module(ModuleModel::M4315),
            brightness: 40,
        });

        let pending = sink.images.take();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[&3], vec![0xff, 0xd8, 0xff]);
        assert_eq!(sink.backlight.take(), Some(40));
        assert_eq!(sink.dial_screen.take(), Some(vec![0xff, 0xd8, 0x22]));
        assert_eq!(
            sink.knob_leds.take(),
            vec![LedCommand::color(1, 9, 8, 7, true)]
        );
        assert_eq!(sink.base_leds.take(), vec![LedCommand::brightness(0, 40)]);
        assert_eq!(
            sink.slider_leds.take(),
            vec![LedCommand::mode(0xff, LightingMode::Static, false)]
        );
    }

    #[test]
    fn held_key_emits_key_state() {
        let hub = Hub::new(Arc::new(NoopCommandSink));
        let mut events = hub.subscribe();
        let mut next = released_snapshot();
        next[0] = true;
        emit_key_changes(&hub, &released_snapshot(), &next);

        let event: serde_json::Value = serde_json::from_str(&events.try_recv().unwrap()).unwrap();
        assert_eq!(event["type"], "event");
        assert_eq!(event["action"], "key_state");
        assert_eq!(event["payload"]["module"]["model"], "M4315");
        assert_eq!(event["payload"]["module"]["id"], "keys");
        assert_eq!(event["payload"]["control"], 0);
        assert_eq!(event["payload"]["pressed"], true);
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn knob_press_and_slider_changes_emit_one_event_each() {
        let hub = Hub::new(Arc::new(NoopCommandSink));
        let mut events = hub.subscribe();
        let mut pressed = [false; 3];
        emit_knobs(
            &hub,
            &mut pressed,
            InputReport::KnobPress([false, true, false]),
        );
        let event: serde_json::Value = serde_json::from_str(&events.try_recv().unwrap()).unwrap();
        assert_eq!(event["action"], "knob_state");
        assert_eq!(event["payload"]["module"]["id"], "knobs");
        assert_eq!(event["payload"]["control"], 2);
        assert_eq!(event["payload"]["pressed"], true);

        let mut values = [None; 5];
        let next = [10, 10, 0, 0, 0];
        for (control, value) in next.into_iter().enumerate() {
            if values[control] == Some(value) {
                continue;
            }
            values[control] = Some(value);
            hub.emit_event("slider_moved", &slider_payload(control as u8, value));
        }
        let first: serde_json::Value = serde_json::from_str(&events.try_recv().unwrap()).unwrap();
        assert_eq!(first["payload"]["value"], 10);
        assert!(events.try_recv().is_ok());
    }
}
