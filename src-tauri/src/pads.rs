//! Bateria controlerelor conectate. Gamepad API din webview nu o expune, așa că
//! o citim direct: controlerele Sony prin rapoartele HID, cele Xbox prin XInput.

use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PadBattery {
    /// "playstation" sau "xbox", ca interfața să aleagă iconița potrivită.
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
// DualSense și DualShock 4 (HID)
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
        0x05c4 | 0x09cc | 0x0ba0 => Some(Sony::DualShock4), // v1, v2, adaptorul wireless
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
        // Același controler poate apărea de mai multe ori (o intrare per colecție HID).
        let key = info.serial_number().map(str::to_owned).unwrap_or_else(|| info.path().to_string_lossy().into_owned());
        if !seen.insert(key) {
            continue;
        }
        let Ok(dev) = info.open_device(&api) else { continue };
        if let Some(b) = read_sony(&dev, model) {
            out.push(b);
        }
    }
    out
}

/// Citește un raport de intrare și extrage starea bateriei.
#[cfg(windows)]
fn read_sony(dev: &hidapi::HidDevice, model: Sony) -> Option<PadBattery> {
    let mut buf = [0u8; 128];
    for attempt in 0..2 {
        let n = dev.read_timeout(&mut buf, 250).ok()?;
        if n == 0 {
            return None;
        }
        // Raportul complet are bateria la un offset fix:
        //   DualSense: USB 0x01 → 53, Bluetooth 0x31 → 54
        //   DualShock 4: USB 0x01 → 30, Bluetooth 0x11 → 32
        let status = match (model, buf[0]) {
            (Sony::DualSense, 0x01) if n >= 64 => buf[53],
            (Sony::DualSense, 0x31) if n >= 55 => buf[54],
            (Sony::DualShock4, 0x01) if n >= 64 => buf[30],
            (Sony::DualShock4, 0x11) if n >= 33 => buf[32],
            _ => {
                // Pe Bluetooth controlerul trimite la început doar un raport scurt, fără
                // baterie; citirea raportului de calibrare (0x05) îl trece pe cel complet.
                if attempt == 0 {
                    let mut feat = [0u8; 41];
                    feat[0] = 0x05;
                    let _ = dev.get_feature_report(&mut feat);
                }
                continue;
            }
        };
        return parse_sony(model, status);
    }
    None
}

#[cfg(windows)]
fn parse_sony(model: Sony, status: u8) -> Option<PadBattery> {
    let level = status & 0x0f;
    let (percent, charging) = match model {
        Sony::DualSense => match status >> 4 {
            0 => (level * 10 + 5, false),
            1 => (level * 10 + 5, true),
            2 => (100, false), // încărcat complet
            _ => return None,  // eroare de tensiune sau temperatură
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

/// Urmărește butonul PS pe toate controlerele Sony și apelează `on_press` la fiecare
/// apăsare. Rulează pe un fir propriu, la nesfârșit; funcționează și cu un joc în față,
/// pentru că Windows permite mai multor aplicații să citească același controler.
#[cfg(windows)]
pub fn watch_ps_button(on_press: impl Fn()) {
    use std::time::{Duration, Instant};

    struct Pad {
        dev: hidapi::HidDevice,
        path: std::ffi::CString,
        model: Sony,
        down: bool,
    }
    let Ok(mut api) = hidapi::HidApi::new() else { return };
    let mut pads: Vec<Pad> = Vec::new();
    let mut last_scan: Option<Instant> = None;
    let mut buf = [0u8; 128];
    loop {
        // Controlerele noi apar la o reenumerare, din 3 în 3 secunde.
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
                        pads.push(Pad { dev, path: info.path().to_owned(), model, down: false });
                    }
                }
            }
        }
        if pads.is_empty() {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        // Golim toate rapoartele din coadă, ca să reacționăm la starea cea mai recentă.
        pads.retain_mut(|p| loop {
            match p.dev.read_timeout(&mut buf, 0) {
                Ok(0) => break true,
                Ok(n) => {
                    if let Some(down) = ps_pressed(p.model, &buf[..n]) {
                        if down && !p.down {
                            on_press();
                        }
                        p.down = down;
                    }
                }
                Err(_) => break false, // controler deconectat
            }
        });
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(not(windows))]
pub fn watch_ps_button(_on_press: impl Fn()) {}

/// Starea butonului PS dintr-un raport de intrare (bitul 0 al celui de-al treilea
/// octet de butoane). Raportul scurt de Bluetooth (0x01, ~10 octeți) are aceeași
/// structură ca raportul USB al DualShock 4.
#[cfg(windows)]
fn ps_pressed(model: Sony, r: &[u8]) -> Option<bool> {
    let byte = match (model, r.first()?) {
        (Sony::DualSense, 0x01) if r.len() >= 64 => r[10],
        (Sony::DualSense, 0x31) => *r.get(11)?,
        (_, 0x01) => *r.get(7)?,
        (Sony::DualShock4, 0x11) => *r.get(9)?,
        _ => return None,
    };
    Some(byte & 0x01 != 0)
}

// ---------------------------------------------------------------------------
// Controlere Xbox (XInput)
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
        // 0 = deconectat, 1 = cu fir (fără baterie), 0xFF = necunoscut.
        if matches!(info.battery_type, 0 | 1 | 0xff) {
            continue;
        }
        // XInput dă doar patru trepte: gol, scăzut, mediu, plin.
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
