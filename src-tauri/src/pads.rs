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
        if let Some(b) = read_sony(&dev, model, is_bluetooth(info)) {
            out.push(b);
        }
    }
    out
}

/// Conexiunea trebuie aflată de la dispozitiv, nu din lungimea raportului: pe Bluetooth,
/// Windows completează raportul scurt cu zerouri până la lungimea celui mai mare raport.
#[cfg(windows)]
fn is_bluetooth(info: &hidapi::DeviceInfo) -> bool {
    matches!(info.bus_type(), hidapi::BusType::Bluetooth)
}

/// Citește un raport de intrare și extrage starea bateriei.
#[cfg(windows)]
fn read_sony(dev: &hidapi::HidDevice, model: Sony, bt: bool) -> Option<PadBattery> {
    let mut buf = [0u8; 128];
    let n = dev.read_timeout(&mut buf, 250).ok()?;
    // Raportul complet are bateria la un offset fix:
    //   DualSense: USB 0x01 → 53, Bluetooth 0x31 → 54
    //   DualShock 4: USB 0x01 → 30, Bluetooth 0x11 → 32
    // Pe Bluetooth, controlerul trimite implicit doar raportul scurt (0x01), fără baterie.
    // NU îl trecem noi pe raportul complet (prin raportul de calibrare 0x05): modul acela
    // rămâne activ până la oprirea controlerului, iar Windows și Gamepad API din WebView2
    // nu-l înțeleg, așa că navigarea în Lumen ar îngheța. Afișăm bateria doar dacă alt
    // program (de ex. Steam) a trecut deja controlerul pe raportul complet.
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
/// Când apare un dongle DS5 Bridge capabil de predare, îi cere să cedeze controlerul
/// Bluetooth-ului din PC și apelează `on_handoff`.
#[cfg(windows)]
pub fn watch_ps_button(on_press: impl Fn(), on_handoff: impl Fn()) {
    use std::time::{Duration, Instant};

    struct Pad {
        dev: hidapi::HidDevice,
        path: std::ffi::CString,
        model: Sony,
        bt: bool,
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
                        let bt = is_bluetooth(info);
                        if !bt && bridge_release(&dev) {
                            on_handoff();
                        }
                        pads.push(Pad { dev, path: info.path().to_owned(), model, bt, down: false });
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
                    if let Some(down) = ps_pressed(p.model, p.bt, &buf[..n]) {
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
pub fn watch_ps_button(_on_press: impl Fn(), _on_handoff: impl Fn()) {}

/// Dongle-ul DS5 Bridge (Pico 2 W) se prezintă pe USB ca un DualSense. Firmware-ul cu
/// predare își marchează versiunea (raportul 0xF8) cu „+handoff” când știe adresa și cheia
/// Bluetooth-ului din PC; atunci comanda 0x11 (pe raportul 0xF6) îl face să elibereze
/// controlerul și să-și oprească Bluetooth-ul până când PC-ul intră în repaus.
/// Un DualSense real pe cablu nu are aceste rapoarte, deci nu e afectat.
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

/// Starea butonului PS dintr-un raport de intrare (bitul 0 al celui de-al treilea
/// octet de butoane). Raportul scurt de Bluetooth (0x01) are aceeași structură ca
/// raportul USB al DualShock 4.
#[cfg(windows)]
fn ps_pressed(model: Sony, bt: bool, r: &[u8]) -> Option<bool> {
    let byte = match (model, bt, r.first()?) {
        (Sony::DualSense, false, 0x01) => *r.get(10)?,
        (Sony::DualSense, true, 0x31) => *r.get(11)?,
        (Sony::DualShock4, true, 0x11) => *r.get(9)?,
        (_, _, 0x01) => *r.get(7)?,
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
