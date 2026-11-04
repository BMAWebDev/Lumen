//! The battery of the connected controllers. The webview's Gamepad API doesn't expose it, so
//! we read it directly: Sony controllers through HID reports, Xbox ones through XInput.

use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PadBattery {
    /// "playstation" or "xbox", so the interface picks the right icon.
    pub kind: &'static str,
    pub percent: u8,
    pub charging: bool,
}

#[cfg(windows)]
pub fn batteries() -> Vec<PadBattery> {
    let mut out = sony();
    out.extend(xinput());
    out
}

#[cfg(not(windows))]
pub fn batteries() -> Vec<PadBattery> {
    Vec::new()
}

// ---------------------------------------------------------------------------
// DualSense and DualShock 4 (HID)
// ---------------------------------------------------------------------------

#[cfg(windows)]
const SONY_VID: u16 = 0x054c;

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq)]
enum Sony {
    DualShock4,
    DualSense,
}

#[cfg(windows)]
fn sony_model(pid: u16) -> Option<Sony> {
    match pid {
        0x05c4 | 0x09cc | 0x0ba0 => Some(Sony::DualShock4), // v1, v2, the wireless adapter
        0x0ce6 | 0x0df2 => Some(Sony::DualSense),           // DualSense, DualSense Edge
        _ => None,
    }
}

#[cfg(windows)]
fn sony() -> Vec<PadBattery> {
    let Ok(api) = hidapi::HidApi::new() else { return Vec::new() };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for info in api.device_list() {
        if info.vendor_id() != SONY_VID {
            continue;
        }
        let Some(model) = sony_model(info.product_id()) else { continue };
        // The same controller can show up several times (one entry per HID collection).
        let key = info.serial_number().map(str::to_owned).unwrap_or_else(|| info.path().to_string_lossy().into_owned());
        if !seen.insert(key) {
            continue;
        }
        let Ok(dev) = info.open_device(&api) else { continue };
        if let Some(b) = read_sony(&dev, model, is_bluetooth(info)) {
            out.push(b);
        }
    }
    out
}

/// The connection has to be asked from the device, not inferred from the report length: over
/// Bluetooth, Windows pads the short report with zeros up to the length of the largest report.
#[cfg(windows)]
fn is_bluetooth(info: &hidapi::DeviceInfo) -> bool {
    matches!(info.bus_type(), hidapi::BusType::Bluetooth)
}

/// Reads an input report and extracts the battery state.
#[cfg(windows)]
fn read_sony(dev: &hidapi::HidDevice, model: Sony, bt: bool) -> Option<PadBattery> {
    let mut buf = [0u8; 128];
    let n = dev.read_timeout(&mut buf, 250).ok()?;
    // The full report has the battery at a fixed offset:
    //   DualSense: USB 0x01 → 53, Bluetooth 0x31 → 54
    //   DualShock 4: USB 0x01 → 30, Bluetooth 0x11 → 32
    // Over Bluetooth, the controller only sends the short report (0x01) by default, without battery.
    // We do NOT switch it to the full report ourselves (via the calibration report 0x05): that mode
    // stays active until the controller is turned off, and Windows and the Gamepad API in WebView2
    // don't understand it, so navigating in Lumen would freeze. We only show the battery if another
    // program (e.g. Steam) has already switched the controller to the full report.
    let status = match (model, bt, buf[0]) {
        (Sony::DualSense, false, 0x01) if n >= 64 => buf[53],
        (Sony::DualSense, true, 0x31) if n >= 55 => buf[54],
        (Sony::DualShock4, false, 0x01) if n >= 64 => buf[30],
        (Sony::DualShock4, true, 0x11) if n >= 33 => buf[32],
        _ => return None,
    };
    parse_sony(model, status)
}

#[cfg(windows)]
fn parse_sony(model: Sony, status: u8) -> Option<PadBattery> {
    let level = status & 0x0f;
    let (percent, charging) = match model {
        Sony::DualSense => match status >> 4 {
            0 => (level * 10 + 5, false),
            1 => (level * 10 + 5, true),
            2 => (100, false), // fully charged
            _ => return None,  // voltage or temperature error
        },
        Sony::DualShock4 => {
            let cable = status & 0x10 != 0;
            match (cable, level) {
                (true, 0..=9) => (level * 10 + 5, true),
                (true, 10) => (100, true),
                (true, 11) => (100, false),
                (true, _) => return None,
                (false, 0..=9) => (level * 10 + 5, false),
                (false, _) => (100, false),
            }
        }
    };
    Some(PadBattery { kind: "playstation", percent: percent.min(100), charging })
}

/// Watches the PS button on all Sony controllers and calls `on_press` on every
/// press. It runs on its own thread, forever; it also works with a game in front,
/// because Windows lets several applications read the same controller.
/// When a DS5 Bridge dongle capable of handoff shows up, it asks it to give the controller
/// up to the PC's Bluetooth and calls `on_handoff`.
#[cfg(windows)]
pub fn watch_ps_button(on_press: impl Fn(), on_handoff: impl Fn()) {
    watch(on_press, || {}, Some(&on_handoff));
}

/// How long the PS + Options combination has to be held to start Lumen.
#[cfg(windows)]
const COMBO_HOLD: std::time::Duration = std::time::Duration::from_millis(1200);

/// Like `watch_ps_button`, but without the dongle handoff and without reacting to the short press:
/// it calls `on_combo` once when PS and Options are held together.
/// Used by the process that waits for the combination while Lumen is closed.
#[cfg(windows)]
pub fn watch_combo(on_combo: impl Fn()) {
    watch(|| {}, on_combo, None);
}

#[cfg(windows)]
fn watch(on_press: impl Fn(), on_combo: impl Fn(), on_handoff: Option<&dyn Fn()>) {
    use std::time::{Duration, Instant};

    struct Pad {
        dev: hidapi::HidDevice,
        path: std::ffi::CString,
        model: Sony,
        bt: bool,
        down: bool,
        /// The moment PS and Options were pressed together.
        combo_since: Option<Instant>,
        combo_fired: bool,
    }
    let Ok(mut api) = hidapi::HidApi::new() else { return };
    let mut pads: Vec<Pad> = Vec::new();
    let mut last_scan: Option<Instant> = None;
    let mut buf = [0u8; 128];
    loop {
        // New controllers show up on a re-enumeration, every 3 seconds.
        if last_scan.is_none_or(|t| t.elapsed() > Duration::from_secs(3)) {
            last_scan = Some(Instant::now());
            if api.refresh_devices().is_ok() {
                for info in api.device_list() {
                    let Some(model) = (info.vendor_id() == SONY_VID).then(|| sony_model(info.product_id())).flatten() else {
                        continue;
                    };
                    if pads.iter().any(|p| p.path.as_c_str() == info.path()) {
                        continue;
                    }
                    if let Ok(dev) = info.open_device(&api) {
                        let bt = is_bluetooth(info);
                        if let Some(on_handoff) = on_handoff {
                            if !bt && bridge_release(&dev) {
                                on_handoff();
                            }
                        }
                        pads.push(Pad {
                            dev,
                            path: info.path().to_owned(),
                            model,
                            bt,
                            down: false,
                            combo_since: None,
                            combo_fired: false,
                        });
                    }
                }
            }
        }
        if pads.is_empty() {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        // We drain all the reports in the queue, so we react to the most recent state.
        pads.retain_mut(|p| loop {
            match p.dev.read_timeout(&mut buf, 0) {
                Ok(0) => break true,
                Ok(n) => {
                    if let Some((down, options)) = ps_options(p.model, p.bt, &buf[..n]) {
                        if down && !p.down {
                            on_press();
                        }
                        p.down = down;
                        if down && options {
                            p.combo_since.get_or_insert_with(Instant::now);
                        } else {
                            p.combo_since = None;
                            p.combo_fired = false;
                        }
                    }
                }
                Err(_) => break false, // controller disconnected
            }
        });
        for p in &mut pads {
            if !p.combo_fired && p.combo_since.is_some_and(|t| t.elapsed() >= COMBO_HOLD) {
                p.combo_fired = true;
                on_combo();
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(not(windows))]
pub fn watch_ps_button(_on_press: impl Fn(), _on_handoff: impl Fn()) {}

#[cfg(not(windows))]
pub fn watch_combo(_on_combo: impl Fn()) {}

/// The DS5 Bridge dongle (Pico 2 W) presents itself over USB as a DualSense. The firmware with
/// handoff marks its version (report 0xF8) with "+handoff" when it knows the address and key of
/// the PC's Bluetooth; then command 0x11 (on report 0xF6) makes it release the
/// controller and turn its Bluetooth off until the PC goes to sleep.
/// A real DualSense on a cable doesn't have these reports, so it is not affected.
#[cfg(windows)]
fn bridge_release(dev: &hidapi::HidDevice) -> bool {
    let mut version = [0u8; 64];
    version[0] = 0xF8;
    let Ok(n) = dev.get_feature_report(&mut version) else { return false };
    let text = String::from_utf8_lossy(&version[1..n.min(64)]);
    if !text.trim_end_matches('\0').ends_with("+handoff") {
        return false;
    }
    let mut cmd = [0u8; 64];
    cmd[0] = 0xF6;
    cmd[1] = 0x11;
    dev.send_feature_report(&cmd).is_ok()
}

/// The state of the PS and Options buttons from an input report. PS is bit 0 of the third
/// button byte, Options is bit 5 of the one before it. The short Bluetooth report (0x01)
/// has the same structure as the DualShock 4 USB report.
#[cfg(windows)]
fn ps_options(model: Sony, bt: bool, r: &[u8]) -> Option<(bool, bool)> {
    let at = match (model, bt, r.first()?) {
        (Sony::DualSense, false, 0x01) => 10,
        (Sony::DualSense, true, 0x31) => 11,
        (Sony::DualShock4, true, 0x11) => 9,
        (_, _, 0x01) => 7,
        _ => return None,
    };
    Some((*r.get(at)? & 0x01 != 0, *r.get(at - 1)? & 0x20 != 0))
}

// ---------------------------------------------------------------------------
// Xbox controllers (XInput)
// ---------------------------------------------------------------------------

#[cfg(windows)]
#[repr(C)]
struct XInputBattery {
    battery_type: u8,
    battery_level: u8,
}

#[cfg(windows)]
#[link(name = "xinput")]
extern "system" {
    fn XInputGetBatteryInformation(user: u32, dev_type: u8, info: *mut XInputBattery) -> u32;
}

#[cfg(windows)]
fn xinput() -> Vec<PadBattery> {
    const BATTERY_DEVTYPE_GAMEPAD: u8 = 0;
    let mut out = Vec::new();
    for user in 0..4 {
        let mut info = XInputBattery { battery_type: 0, battery_level: 0 };
        if unsafe { XInputGetBatteryInformation(user, BATTERY_DEVTYPE_GAMEPAD, &mut info) } != 0 {
            continue;
        }
        // 0 = disconnected, 1 = wired (no battery), 0xFF = unknown.
        if matches!(info.battery_type, 0 | 1 | 0xff) {
            continue;
        }
        // XInput only gives four levels: empty, low, medium, full.
        let percent = match info.battery_level {
            0 => 5,
            1 => 30,
            2 => 65,
            _ => 100,
        };
        out.push(PadBattery { kind: "xbox", percent, charging: false });
    }
    out
}
