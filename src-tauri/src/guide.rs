//! The Guide button (the Xbox logo) on the controller, read globally, even while
//! a game has focus. The equivalent of the PS button: it opens the quick menu.
//!
//! Standard XInput doesn't expose the Guide button. The undocumented function
//! `XInputGetStateEx` (ordinal 100 in xinput1_4.dll) exposes it as bit 0x0400.
//! The PS button on DualSense / DualShock 4 is read separately, over HID (`pads.rs`).

use tauri::AppHandle;

#[cfg(not(windows))]
pub fn start(_app: AppHandle) {}

#[cfg(windows)]
pub fn start(app: AppHandle) {
    let xinput = app.clone();
    std::thread::spawn(move || run(xinput));
    std::thread::spawn(move || {
        use tauri::Emitter;
        crate::pads::watch_ps_button(|| fire(&app), || {
            let _ = app.emit("pad-handoff", ());
        })
    });
}

/// Brings Lumen to the front and opens the quick menu. A DualSense also mapped as XInput
/// (Steam Input, DS4Windows) presses "twice"; a second press less than 300 ms later
/// is ignored, otherwise the menu would open and close again.
#[cfg(windows)]
fn fire(app: &AppHandle) {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    use tauri::{Emitter, Manager};

    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    if let Ok(mut last) = LAST.lock() {
        if last.is_some_and(|t| t.elapsed() < Duration::from_millis(300)) {
            return;
        }
        *last = Some(Instant::now());
    }
    // Inside another platform (Steam, Epic, Xbox...) the Guide button belongs to that platform.
    if crate::platforms::in_front() {
        return;
    }
    if let Some(w) = app.get_webview_window("main") {
        crate::platform::bring_to_front(&w);
    }
    crate::launcher::pause(app);
    let _ = app.emit("guide", ());
}

#[cfg(windows)]
fn run(app: AppHandle) {
    use crate::platform::win::{wide, GetProcAddress, LoadLibraryW};
    use std::ffi::c_void;
    use std::time::{Duration, Instant};

    #[repr(C)]
    #[derive(Default)]
    struct Gamepad {
        buttons: u16,
        left_trigger: u8,
        right_trigger: u8,
        lx: i16,
        ly: i16,
        rx: i16,
        ry: i16,
    }
    #[repr(C)]
    #[derive(Default)]
    struct State {
        packet: u32,
        pad: Gamepad,
    }
    type GetStateEx = unsafe extern "system" fn(u32, *mut State) -> u32;

    const GUIDE: u16 = 0x0400;
    const ERROR_SUCCESS: u32 = 0;

    let mut func: Option<GetStateEx> = None;
    for dll in ["xinput1_4.dll", "xinput1_3.dll"] {
        let name = wide(dll);
        let module = unsafe { LoadLibraryW(name.as_ptr()) };
        if module == 0 {
            continue;
        }
        // Ordinal 100 is passed as a "pointer" with the value 100 (MAKEINTRESOURCE).
        let p = unsafe { GetProcAddress(module, 100usize as *const u8) };
        if !p.is_null() {
            func = Some(unsafe { std::mem::transmute::<*const c_void, GetStateEx>(p) });
            break;
        }
    }
    let Some(get_state) = func else { return };

    let mut connected = [true; 4];
    let mut last_probe = [Instant::now(); 4];
    let mut was_down = [false; 4];
    loop {
        for i in 0..4 {
            // Querying an empty slot is expensive: we do it rarely.
            if !connected[i] && last_probe[i].elapsed() < Duration::from_secs(2) {
                continue;
            }
            let mut st = State::default();
            let res = unsafe { get_state(i as u32, &mut st) };
            last_probe[i] = Instant::now();
            connected[i] = res == ERROR_SUCCESS;
            let down = connected[i] && st.pad.buttons & GUIDE != 0;
            if down && !was_down[i] {
                fire(&app);
            }
            was_down[i] = down;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}
