//! macOS USB location IDs for modules plugged into the base.
//!
//! A location ID's high byte is the bus. The following nibbles are hub ports
//! until a zero nibble. The first of those ports is the Mac port the base is
//! plugged into. The rest address the pogo site and stay the same when the
//! base moves to another Mac port. `@02112400` is site `124`.

use std::collections::BTreeMap;

pub fn site_key(location: u32) -> String {
    let mut nibbles = Vec::new();
    for shift in [20u32, 16, 12, 8, 4, 0] {
        let nibble = (location >> shift) & 0xF;
        if nibble == 0 {
            break;
        }
        nibbles.push(nibble);
    }
    if !nibbles.is_empty() {
        nibbles.remove(0);
    }
    nibbles
        .into_iter()
        .map(|nibble| format!("{nibble:x}"))
        .collect()
}

/// Serial number to raw location ID, from `ioreg -p IOUSB -w0 -l`.
pub fn locations_by_serial(ioreg: &str) -> BTreeMap<String, u32> {
    let mut current = None;
    let mut found = BTreeMap::new();
    for line in ioreg.lines() {
        if let Some(location) = location_in_node(line) {
            current = Some(location);
        }
        if let Some(serial) = serial_in_line(line) {
            if let Some(location) = current {
                found.insert(serial, location);
            }
        }
    }
    found
}

fn location_in_node(line: &str) -> Option<u32> {
    if !line.contains("+-o ") {
        return None;
    }
    let hex = line.rsplit('@').next()?.split_whitespace().next()?;
    if hex.len() != 8 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

fn serial_in_line(line: &str) -> Option<String> {
    let marker = "\"USB Serial Number\" = \"";
    let start = line.find(marker)? + marker.len();
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_key_drops_the_bus_and_the_host_port() {
        assert_eq!(site_key(0x0211_2400), "124");
        assert_eq!(site_key(0x0214_4000), "44");
        assert_eq!(site_key(0x0214_2000), "42");
        assert_eq!(site_key(0x0211_1100), "111");
        assert_eq!(site_key(0x0231_2400), "124");
    }

    #[test]
    fn serial_is_paired_with_the_device_node_above_it() {
        let ioreg = r#"
  | +-o 1xDial w/ IPS, 2x2@02112200  <class IOUSBHostDevice, id 0x1>
  |   | {
  |   |   "USB Serial Number" = "MHDD01"
  |   | }
  | +-o Base, 4x6@02114400  <class IOUSBHostDevice, id 0x2>
  |   | {
  |   |   "USB Serial Number" = "MHBS01"
  |   | }
"#;
        let found = locations_by_serial(ioreg);
        assert_eq!(found.get("MHDD01").copied(), Some(0x0211_2200));
        assert_eq!(site_key(found["MHBS01"]), "144");
    }
}
