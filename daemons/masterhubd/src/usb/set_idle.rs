//! HID SET_IDLE for the 2xRoller on macOS.
//!
//! The firmware stays silent until the host sends SET_IDLE. Linux and Windows
//! send it when they bind a HID device. macOS sends it only to keyboards and
//! mice, so the roller enumerates and answers feature reports while its
//! interrupt endpoint never starts. One SET_IDLE(0) on the control pipe,
//! without taking the device from the HID driver, starts the reports. It
//! lasts until the module resets, so it is sent on every connect.

/// hidapi's macOS path is `DevSrvsID:<registry id>` for the HID service.
pub fn registry_id(path: &str) -> Option<u64> {
    let id = path.strip_prefix("DevSrvsID:")?;
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    id.parse().ok()
}

#[cfg(target_os = "macos")]
pub fn set_idle(device: &hidapi::HidDevice) -> Result<(), String> {
    let info = device
        .get_device_info()
        .map_err(|err| format!("SET_IDLE failed: {err}"))?;
    let path = info
        .path()
        .to_str()
        .map_err(|_| "SET_IDLE failed: hid path is not utf-8".to_string())?;
    let id = registry_id(path)
        .ok_or_else(|| format!("SET_IDLE failed: hid path {path} has no registry id"))?;
    match usb_set_idle(id) {
        0 => Ok(()),
        code => Err(failure(code)),
    }
}

#[cfg(target_os = "macos")]
fn usb_set_idle(registry_id: u64) -> i32 {
    // `mh_usb_set_idle` accepts any registry id and borrows nothing from Rust.
    unsafe { mh_usb_set_idle(registry_id) }
}

#[cfg(target_os = "macos")]
fn failure(code: i32) -> String {
    let reason = match code {
        1 => "HID service was not in the registry",
        2 => "USB device was not found above the HID service",
        3 => "USB device could not be opened",
        4 => "roller rejected SET_IDLE",
        _ => return format!("SET_IDLE failed: unexpected status {code}"),
    };
    format!("SET_IDLE failed: {reason}")
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn mh_usb_set_idle(hid_registry_id: u64) -> i32;
}

#[cfg(test)]
mod tests {
    use super::registry_id;

    #[test]
    fn hid_path_is_a_registry_id() {
        assert_eq!(registry_id("DevSrvsID:4294974957"), Some(4_294_974_957));
        assert_eq!(registry_id("DevSrvsID:0"), Some(0));
        assert_eq!(registry_id("DevSrvsID:"), None);
        assert_eq!(registry_id("DevSrvsID:12x"), None);
        assert_eq!(registry_id("/dev/hidraw0"), None);
        assert_eq!(registry_id("DevSrvsID:18446744073709551616"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn missing_registry_entry_is_not_a_device() {
        assert_eq!(super::usb_set_idle(u64::MAX), 1);
    }
}
