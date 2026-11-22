//! Desktop control, like Steam's "desktop configuration": while Lumen is in the background the
//! controller drives the mouse and the keyboard, so the whole PC can be used from the couch.
//!
//! The left stick moves the cursor, the right stick scrolls, and every button is mapped to a mouse
//! click or a key (the mapping is configurable). Controllers are read through XInput (Xbox, or anything
//! mapped to XInput) and, for Sony controllers, from the HID reports `pads.rs` already decodes.
//! It stays out of the way while Lumen has the focus, while a game started by Lumen runs, and while
//! some other full screen window (a game, Steam Big Picture) is in front: those read the controller.
//!
//! The "keyboard" action opens a small window with the same on-screen keyboard Lumen uses for text. It
//! never takes the focus, so what you type goes to the window that was in front.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::AppHandle;

/// The buttons, in the order of the browser's "standard" gamepad mapping (a bit each in `RawPad::buttons`).
pub const BUTTONS: [&str; 16] = [
    "a", "b", "x", "y", "lb", "rb", "lt", "rt", "view", "menu", "l3", "r3", "up", "down", "left", "right",
];

/// What a button can do. `none` is the first, so a button can be switched off.
pub const ACTIONS: [&str; 26] = [
    "none", "left_click", "right_click", "middle_click", "enter", "escape", "tab", "backspace", "space", "delete", "up",
    "down", "left", "right", "page_up", "page_down", "home", "end", "win", "alt_tab", "back", "forward", "task_view",
    "show_desktop", "close_window", "keyboard",
];

/// The mapping a fresh install starts with.
pub fn default_map() -> HashMap<String, String> {
    [
        ("a", "left_click"),
        ("b", "escape"),
        ("x", "enter"),
        ("y", "keyboard"),
        ("lb", "back"),
        ("rb", "alt_tab"),
        ("lt", "right_click"),
        ("rt", "left_click"),
        ("view", "tab"),
        ("menu", "win"),
        ("l3", "middle_click"),
        ("r3", "none"),
        ("up", "up"),
        ("down", "down"),
        ("left", "left"),
        ("right", "right"),
    ]
    .into_iter()
    .map(|(b, a)| (b.to_owned(), a.to_owned()))
    .collect()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub buttons: Vec<&'static str>,
    pub actions: Vec<&'static str>,
    pub defaults: HashMap<String, String>,
}

pub fn layout() -> Layout {
    Layout { buttons: BUTTONS.to_vec(), actions: ACTIONS.to_vec(), defaults: default_map() }
}

#[derive(Clone)]
struct Config {
    enabled: bool,
    auto_keyboard: bool,
    mouse_speed: u8,
    scroll_speed: u8,
    map: HashMap<String, String>,
}

static CONFIG: Mutex<Option<Config>> = Mutex::new(None);
/// "Switch to Desktop" from the quick menu: the controller drives the desktop whatever the setting says and whatever
/// is running, until Lumen gets the focus back (the Guide / PS button).
static FORCED: AtomicBool = AtomicBool::new(false);
/// The on-screen keyboard is open: the controller drives it instead of the mouse.
static KEYBOARD: AtomicBool = AtomicBool::new(false);

/// Applies the settings. Called at start and whenever one of the desktop settings changes.
pub fn configure(s: &crate::model::Settings) {
    let mut map = default_map();
    for (b, a) in &s.desktop_map {
        if BUTTONS.contains(&b.as_str()) && ACTIONS.contains(&a.as_str()) {
            map.insert(b.clone(), a.clone());
        }
    }
    if let Ok(mut c) = CONFIG.lock() {
        *c = Some(Config {
            enabled: s.desktop_control,
            auto_keyboard: s.desktop_auto_keyboard,
            mouse_speed: s.desktop_mouse_speed.clamp(1, 10),
            scroll_speed: s.desktop_scroll_speed.clamp(1, 10),
            map,
        });
    }
}

#[cfg(not(windows))]
pub fn start(_app: AppHandle) {}

#[cfg(not(windows))]
pub fn switch_to_desktop(_app: &AppHandle) {}

/// Puts Lumen away and gives the controller to the desktop.
#[cfg(windows)]
pub fn switch_to_desktop(app: &AppHandle) {
    use tauri::Manager;
    FORCED.store(true, Ordering::Relaxed);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.minimize();
    }
    // With a game running, it is the window that would come back to the front.
    crate::launcher::minimize_game(app);
    // If something else still covers the desktop (a game we could not find, run from elsewhere), Win+D clears it.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if !win::desktop_in_front() {
            win::chord(&[win::VK_LWIN, 0x44]);
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
        // The game must not keep reading the controller behind the desktop. It is frozen only now, once
        // its window is out of the way: a frozen window cannot minimize.
        crate::launcher::freeze_for_desktop(&app);
    });
}

#[cfg(not(windows))]
pub fn osk_type(_text: String) {}
#[cfg(not(windows))]
pub fn osk_edit(_kind: String) {}
#[cfg(not(windows))]
pub fn osk_close(_app: AppHandle) {}

#[cfg(windows)]
pub fn start(app: AppHandle) {
    let (tx, rx) = std::sync::mpsc::channel::<(i32, i32)>();
    let _ = win::CLICKS.set(Mutex::new(tx));
    let worker = app.clone();
    std::thread::spawn(move || win::fields(worker, rx));
    std::thread::spawn(move || win::run(app));
}

/// Types text (typed on the on-screen keyboard) into the window in front.
#[cfg(windows)]
pub fn osk_type(text: String) {
    for u in text.encode_utf16() {
        win::unicode(u);
    }
}

/// A key of the on-screen keyboard that is not a character: `backspace`, `left`, `right` or `enter`.
#[cfg(windows)]
pub fn osk_edit(kind: String) {
    if let Some(vk) = win::key_of(&kind) {
        win::key_tap(vk);
    }
}

/// Hides the on-screen keyboard.
#[cfg(windows)]
pub fn osk_close(app: AppHandle) {
    win::close_keyboard(&app);
}

#[cfg(windows)]
mod win {
    use super::*;
    use crate::pads::RawPad;
    use std::time::{Duration, Instant};
    use tauri::{Emitter, Manager};

    // ---- Windows API ------------------------------------------------------------------------------------

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct MonitorInfo {
        size: u32,
        monitor: Rect,
        work: Rect,
        flags: u32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MouseInput {
        dx: i32,
        dy: i32,
        data: u32,
        flags: u32,
        time: u32,
        extra: usize,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct KeyInput {
        vk: u16,
        scan: u16,
        flags: u32,
        time: u32,
        extra: usize,
    }
    /// `INPUT` is a union of the mouse and keyboard structures; the padding makes it as large as the biggest.
    #[repr(C)]
    union InputUnion {
        mouse: MouseInput,
        key: KeyInput,
    }
    #[repr(C)]
    struct Input {
        kind: u32,
        u: InputUnion,
    }
    #[repr(C)]
    #[derive(Default)]
    struct XGamepad {
        buttons: u16,
        lt: u8,
        rt: u8,
        lx: i16,
        ly: i16,
        rx: i16,
        ry: i16,
    }
    #[repr(C)]
    #[derive(Default)]
    struct XState {
        packet: u32,
        pad: XGamepad,
    }
    type XGetState = unsafe extern "system" fn(u32, *mut XState) -> u32;

    #[link(name = "user32")]
    extern "system" {
        fn GetCursorPos(p: *mut Point) -> i32;
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn SendInput(n: u32, inputs: *const Input, size: i32) -> u32;
        fn MapVirtualKeyW(code: u32, map: u32) -> u32;
        fn GetForegroundWindow() -> isize;
        fn GetWindowRect(h: isize, r: *mut Rect) -> i32;
        fn GetWindowThreadProcessId(h: isize, pid: *mut u32) -> u32;
        fn GetClassNameW(h: isize, buf: *mut u16, max: i32) -> i32;
        fn MonitorFromWindow(h: isize, flags: u32) -> isize;
        fn GetMonitorInfoW(m: isize, info: *mut MonitorInfo) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcessId() -> u32;
    }

    const INPUT_MOUSE: u32 = 0;
    const INPUT_KEYBOARD: u32 = 1;
    const MOUSEEVENTF_LEFTDOWN: u32 = 0x2;
    const MOUSEEVENTF_LEFTUP: u32 = 0x4;
    const MOUSEEVENTF_RIGHTDOWN: u32 = 0x8;
    const MOUSEEVENTF_RIGHTUP: u32 = 0x10;
    const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x20;
    const MOUSEEVENTF_MIDDLEUP: u32 = 0x40;
    const MOUSEEVENTF_WHEEL: u32 = 0x800;
    const MOUSEEVENTF_HWHEEL: u32 = 0x1000;
    const KEYEVENTF_EXTENDEDKEY: u32 = 0x1;
    const KEYEVENTF_KEYUP: u32 = 0x2;
    const KEYEVENTF_UNICODE: u32 = 0x4;

    fn send(inputs: &[Input]) {
        unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<Input>() as i32) };
    }

    fn mouse(flags: u32, data: i32) {
        send(&[Input {
            kind: INPUT_MOUSE,
            u: InputUnion { mouse: MouseInput { dx: 0, dy: 0, data: data as u32, flags, time: 0, extra: 0 } },
        }]);
    }

    /// Keys that Windows treats as "extended" (arrows, navigation block, right-hand Alt / Ctrl...).
    fn extended(vk: u16) -> bool {
        matches!(vk, 0x21..=0x28 | 0x2D | 0x2E | 0x5B | 0x5C)
    }

    fn key(vk: u16, up: bool) -> Input {
        let scan = unsafe { MapVirtualKeyW(u32::from(vk), 0) } as u16;
        let mut flags = if up { KEYEVENTF_KEYUP } else { 0 };
        if extended(vk) {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        Input { kind: INPUT_KEYBOARD, u: InputUnion { key: KeyInput { vk, scan, flags, time: 0, extra: 0 } } }
    }

    fn key_down(vk: u16) {
        send(&[key(vk, false)]);
    }
    fn key_up(vk: u16) {
        send(&[key(vk, true)]);
    }
    pub fn key_tap(vk: u16) {
        send(&[key(vk, false), key(vk, true)]);
    }

    /// Several keys pressed together, then released in reverse order (Alt + Tab...).
    /// Whether the window in front is the desktop itself (or the taskbar), or Lumen.
    pub fn desktop_in_front() -> bool {
        unsafe {
            let h = GetForegroundWindow();
            if h == 0 {
                return true;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, &mut pid);
            if pid == GetCurrentProcessId() {
                return true;
            }
            let mut class = [0u16; 64];
            let n = GetClassNameW(h, class.as_mut_ptr(), 64).max(0) as usize;
            matches!(String::from_utf16_lossy(&class[..n]).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd")
        }
    }

    pub fn chord(keys: &[u16]) {
        let mut v: Vec<Input> = keys.iter().map(|&k| key(k, false)).collect();
        v.extend(keys.iter().rev().map(|&k| key(k, true)));
        send(&v);
    }

    /// One UTF-16 unit typed as text, whatever the keyboard layout is.
    pub fn unicode(u: u16) {
        let down = Input {
            kind: INPUT_KEYBOARD,
            u: InputUnion { key: KeyInput { vk: 0, scan: u, flags: KEYEVENTF_UNICODE, time: 0, extra: 0 } },
        };
        let up = Input {
            kind: INPUT_KEYBOARD,
            u: InputUnion { key: KeyInput { vk: 0, scan: u, flags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP, time: 0, extra: 0 } },
        };
        send(&[down, up]);
    }

    const VK_BACK: u16 = 0x08;
    const VK_TAB: u16 = 0x09;
    const VK_RETURN: u16 = 0x0D;
    const VK_MENU: u16 = 0x12;
    const VK_ESCAPE: u16 = 0x1B;
    const VK_SPACE: u16 = 0x20;
    pub const VK_LWIN: u16 = 0x5B;

    pub fn key_of(name: &str) -> Option<u16> {
        Some(match name {
            "enter" => VK_RETURN,
            "escape" => VK_ESCAPE,
            "tab" => VK_TAB,
            "backspace" => VK_BACK,
            "space" => VK_SPACE,
            "delete" => 0x2E,
            "up" => 0x26,
            "down" => 0x28,
            "left" => 0x25,
            "right" => 0x27,
            "page_up" => 0x21,
            "page_down" => 0x22,
            "home" => 0x24,
            "end" => 0x23,
            "win" => VK_LWIN,
            _ => return None,
        })
    }

    // ---- What a button does -----------------------------------------------------------------------------

    #[derive(Clone, Copy, PartialEq)]
    enum Act {
        None,
        /// Held for as long as the button is: 0 left, 1 right, 2 middle.
        Click(u8),
        /// Held for as long as the button is; the navigation keys also repeat.
        Key(u16, bool),
        /// Pressed together once, when the button goes down.
        Chord(&'static [u16]),
        Keyboard,
    }

    fn act_of(name: &str) -> Act {
        match name {
            "left_click" => Act::Click(0),
            "right_click" => Act::Click(1),
            "middle_click" => Act::Click(2),
            "alt_tab" => Act::Chord(&[VK_MENU, VK_TAB]),
            "back" => Act::Chord(&[VK_MENU, 0x25]),
            "forward" => Act::Chord(&[VK_MENU, 0x27]),
            "task_view" => Act::Chord(&[VK_LWIN, VK_TAB]),
            "show_desktop" => Act::Chord(&[VK_LWIN, 0x44]),
            "close_window" => Act::Chord(&[VK_MENU, 0x73]),
            "keyboard" => Act::Keyboard,
            other => match key_of(other) {
                Some(vk) => Act::Key(vk, matches!(other, "up" | "down" | "left" | "right" | "backspace" | "delete" | "page_up" | "page_down" | "tab")),
                None => Act::None,
            },
        }
    }

    fn press_click(c: u8, down: bool) {
        let flags = match (c, down) {
            (0, true) => MOUSEEVENTF_LEFTDOWN,
            (0, false) => MOUSEEVENTF_LEFTUP,
            (1, true) => MOUSEEVENTF_RIGHTDOWN,
            (1, false) => MOUSEEVENTF_RIGHTUP,
            (_, true) => MOUSEEVENTF_MIDDLEDOWN,
            (_, false) => MOUSEEVENTF_MIDDLEUP,
        };
        mouse(flags, 0);
    }

    // ---- Reading the controller -------------------------------------------------------------------------

    /// XInput, including the slots that have no controller (those are probed rarely: it is slow).
    struct XInput {
        get: XGetState,
        connected: [bool; 4],
        probed: [Instant; 4],
    }

    impl XInput {
        fn load() -> Option<Self> {
            use crate::platform::win::{wide, GetProcAddress, LoadLibraryW};
            for dll in ["xinput1_4.dll", "xinput1_3.dll"] {
                let name = wide(dll);
                let module = unsafe { LoadLibraryW(name.as_ptr()) };
                if module == 0 {
                    continue;
                }
                let p = unsafe { GetProcAddress(module, b"XInputGetState\0".as_ptr()) };
                if !p.is_null() {
                    let get = unsafe { std::mem::transmute::<*const std::ffi::c_void, XGetState>(p) };
                    return Some(Self { get, connected: [true; 4], probed: [Instant::now(); 4] });
                }
            }
            None
        }

        /// The state of the first controller that is doing something, if any.
        fn read(&mut self) -> Option<RawPad> {
            let mut best: Option<RawPad> = None;
            for i in 0..4 {
                if !self.connected[i] && self.probed[i].elapsed() < Duration::from_secs(2) {
                    continue;
                }
                let mut st = XState::default();
                let ok = unsafe { (self.get)(i as u32, &mut st) } == 0;
                self.probed[i] = Instant::now();
                self.connected[i] = ok;
                if !ok {
                    continue;
                }
                let g = &st.pad;
                let mut b = 0u32;
                let mut set = |bit: u32, on: bool| {
                    if on {
                        b |= 1 << bit;
                    }
                };
                set(0, g.buttons & 0x1000 != 0);
                set(1, g.buttons & 0x2000 != 0);
                set(2, g.buttons & 0x4000 != 0);
                set(3, g.buttons & 0x8000 != 0);
                set(4, g.buttons & 0x0100 != 0);
                set(5, g.buttons & 0x0200 != 0);
                set(6, g.lt > 90);
                set(7, g.rt > 90);
                set(8, g.buttons & 0x0020 != 0);
                set(9, g.buttons & 0x0010 != 0);
                set(10, g.buttons & 0x0040 != 0);
                set(11, g.buttons & 0x0080 != 0);
                set(12, g.buttons & 0x0001 != 0);
                set(13, g.buttons & 0x0002 != 0);
                set(14, g.buttons & 0x0004 != 0);
                set(15, g.buttons & 0x0008 != 0);
                // Screen coordinates: y grows downwards, XInput's grows upwards.
                let f = |v: i16| f32::from(v) / 32768.0;
                let pad = RawPad { buttons: b, lx: f(g.lx), ly: -f(g.ly), rx: f(g.rx), ry: -f(g.ry), sony: false };
                if best.is_none() || pad.active() {
                    best = Some(pad);
                    if pad.active() {
                        break;
                    }
                }
            }
            best
        }
    }

    pub static CLICKS: std::sync::OnceLock<Mutex<std::sync::mpsc::Sender<(i32, i32)>>> = std::sync::OnceLock::new();
    /// The kind of controller used last, for the hints of a keyboard that opens by itself.
    static SONY: AtomicBool = AtomicBool::new(false);

    #[derive(Serialize, Clone, PartialEq, Default)]
    struct Field {
        /// The text of the focused field, when the program lets us read it.
        text: Option<String>,
        password: bool,
    }

    /// What the focused control is, from UI Automation: `None` when it takes no text.
    /// With `click`, only a control that is under that point counts: a field that took the focus by itself (the Start
    /// menu's search box, a window that opens with its input focused) is not what was clicked.
    fn focused_field(auto: &windows::Win32::UI::Accessibility::IUIAutomation, click: Option<(i32, i32)>) -> Option<Field> {
        use windows::core::Interface;
        use windows::Win32::UI::Accessibility::*;
        let el = unsafe { auto.GetFocusedElement() }.ok()?;
        let kind = unsafe { el.CurrentControlType() }.ok()?;
        if kind != UIA_EditControlTypeId && kind != UIA_ComboBoxControlTypeId {
            return None;
        }
        if let Some((x, y)) = click {
            let r = unsafe { el.CurrentBoundingRectangle() }.ok()?;
            if x < r.left || x > r.right || y < r.top || y > r.bottom {
                return None;
            }
        }
        let password = unsafe { el.CurrentIsPassword() }.map(|b| b.as_bool()).unwrap_or(false);
        let value = unsafe { el.GetCurrentPattern(UIA_ValuePatternId) }
            .ok()
            .and_then(|p| p.cast::<IUIAutomationValuePattern>().ok());
        match value {
            Some(v) => {
                if unsafe { v.CurrentIsReadOnly() }.map(|b| b.as_bool()).unwrap_or(false) {
                    return None;
                }
                let text = unsafe { v.CurrentValue() }.ok().map(|b| b.to_string());
                Some(Field { text, password })
            }
            None => Some(Field { text: None, password }),
        }
    }

    /// Watches the focused control through UI Automation, on a thread of its own because a program that does
    /// not answer would block it: opens the keyboard after a click on a text field, and while the keyboard is
    /// up sends the text of the field to it, so what is typed can be seen there.
    pub fn fields(app: AppHandle, rx: std::sync::mpsc::Receiver<(i32, i32)>) {
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
        use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
        let auto: IUIAutomation = unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                return;
            }
            match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                Ok(a) => a,
                Err(_) => return,
            }
        };
        let mut shown = Field::default();
        loop {
            match rx.recv_timeout(Duration::from_millis(150)) {
                Ok(mut at) => {
                    // Only the latest click counts; let it move the focus first.
                    while let Ok(next) = rx.try_recv() {
                        at = next;
                    }
                    std::thread::sleep(Duration::from_millis(180));
                    if !KEYBOARD.load(Ordering::Relaxed) && focused_field(&auto, Some(at)).is_some() {
                        open_keyboard(&app, SONY.load(Ordering::Relaxed));
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return,
            }
            if !KEYBOARD.load(Ordering::Relaxed) {
                shown = Field::default();
                continue;
            }
            let now = focused_field(&auto, None).unwrap_or_default();
            if now != shown {
                let _ = app.emit("osk-text", &now);
                shown = now;
            }
        }
    }

    // ---- The loop ---------------------------------------------------------------------------------------

    const STICK_DEAD: f32 = 0.2;
    /// How far the left stick has to be pushed to start moving the cursor (once it moves, the dead zone is enough).
    /// A resting stick that drifts a little must not nudge the cursor: menus select what is under it.
    const MOUSE_START: f32 = 0.35;
    /// The same, while keys were just pressed (arrows in a menu): only a deliberate push moves the cursor.
    const MOUSE_START_NAV: f32 = 0.75;
    /// How long after a key press the cursor stays put unless pushed hard.
    const NAV_QUIET: Duration = Duration::from_millis(1200);

    /// What the cursor needs to remember between rounds.
    struct Mouse {
        /// Fractions of a pixel (cursor x, y) and of a wheel notch (scroll x, y) not sent yet.
        acc: [f32; 4],
        moving: bool,
        quiet_until: Instant,
    }
    const NAV_STICK: f32 = 0.55;
    const REPEAT_DELAY: Duration = Duration::from_millis(420);
    const REPEAT_RATE: Duration = Duration::from_millis(55);

    /// Whether another window should keep the controller to itself.
    fn foreign_fullscreen() -> bool {
        unsafe {
            let h = GetForegroundWindow();
            if h == 0 {
                return false;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, &mut pid);
            if pid == GetCurrentProcessId() {
                return false;
            }
            let mut class = [0u16; 64];
            let n = GetClassNameW(h, class.as_mut_ptr(), 64).max(0) as usize;
            let class = String::from_utf16_lossy(&class[..n]);
            // The desktop itself, and browsers / Electron apps (a video in full screen should still be usable).
            if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") || class.starts_with("Chrome_WidgetWin") || class.starts_with("MozillaWindowClass") {
                return false;
            }
            let mut r = Rect::default();
            let mut mi = MonitorInfo { size: std::mem::size_of::<MonitorInfo>() as u32, monitor: Rect::default(), work: Rect::default(), flags: 0 };
            if GetWindowRect(h, &mut r) == 0 || GetMonitorInfoW(MonitorFromWindow(h, 2), &mut mi) == 0 {
                return false;
            }
            let m = mi.monitor;
            r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
        }
    }

    /// The Start menu and Search are XAML windows that Windows itself drives with a controller (stick or D-pad
    /// move the selection, A opens, B goes back). Both would act on the same press, so Lumen leaves them alone.
    fn reads_controller_itself() -> bool {
        unsafe {
            let h = GetForegroundWindow();
            let mut class = [0u16; 64];
            let n = GetClassNameW(h, class.as_mut_ptr(), 64).max(0) as usize;
            String::from_utf16_lossy(&class[..n]) == "Windows.UI.Core.CoreWindow"
        }
    }

    fn lumen_focused() -> bool {
        unsafe {
            let h = GetForegroundWindow();
            let mut pid = 0u32;
            h != 0 && GetWindowThreadProcessId(h, &mut pid) != 0 && pid == GetCurrentProcessId()
        }
    }

    /// Whether the controller belongs to the desktop right now.
    fn desktop_is_ours(app: &AppHandle) -> bool {
        if lumen_focused() {
            // Back in Lumen: the switch to the desktop is over.
            FORCED.store(false, Ordering::Relaxed);
            return false;
        }
        if reads_controller_itself() {
            return false;
        }
        if FORCED.load(Ordering::Relaxed) {
            return true;
        }
        let running = app.state::<crate::model::AppState>().inner.lock().map(|s| s.running.is_some()).unwrap_or(true);
        !running && !foreign_fullscreen()
    }

    /// A press that fires once and then repeats while held (used by the on-screen keyboard).
    #[derive(Default)]
    struct Held {
        since: Option<Instant>,
        next: Option<Instant>,
    }

    pub fn run(app: AppHandle) {
        let mut xinput = XInput::load();
        let mut prev = 0u32; // buttons seen on the previous round
        let mut active: [Act; 16] = [Act::None; 16]; // what each held button is doing
        let mut repeat_at: [Option<Instant>; 16] = [None; 16];
        let mut cursor = Mouse { acc: [0.0; 4], moving: false, quiet_until: Instant::now() };
        let mut last = Instant::now();
        let mut ours = false;
        let mut checked = Instant::now() - Duration::from_secs(1);
        let mut nav: HashMap<&'static str, Held> = HashMap::new();
        let mut kb_was_open = false;
        // Buttons that were already down when the keyboard opened: ignored until they are let go.
        let mut kb_mask = 0u32;
        let mut cfg: Option<Config> = None;
        let mut cfg_at = Instant::now() - Duration::from_secs(1);

        loop {
            let now = Instant::now();
            let dt = now.duration_since(last).as_secs_f32().min(0.1);
            last = now;
            if cfg.is_none() || cfg_at.elapsed() > Duration::from_millis(300) {
                cfg = CONFIG.lock().ok().and_then(|c| c.clone());
                cfg_at = now;
            }
            let forced = FORCED.load(Ordering::Relaxed);
            let Some(conf) = cfg.as_ref().filter(|c| c.enabled || forced) else {
                release_all(&mut active, &mut repeat_at);
                if KEYBOARD.load(Ordering::Relaxed) {
                    close_keyboard(&app);
                }
                std::thread::sleep(Duration::from_millis(300));
                continue;
            };

            let kb_open = KEYBOARD.load(Ordering::Relaxed);
            // The foreground checks are cheap but not free; four times a second is plenty.
            if checked.elapsed() > Duration::from_millis(250) {
                ours = desktop_is_ours(&app);
                checked = now;
            }
            let pad = xinput.as_mut().and_then(|x| x.read()).filter(RawPad::active).or_else(crate::pads::raw_pad).unwrap_or_default();

            if !ours && !kb_open {
                // Whatever is held now belongs to someone else; it must not fire when the desktop is ours again.
                release_all(&mut active, &mut repeat_at);
                prev = pad.buttons;
                std::thread::sleep(Duration::from_millis(60));
                continue;
            }

            if kb_open {
                if !kb_was_open {
                    kb_mask = pad.buttons;
                    nav.clear();
                    release_all(&mut active, &mut repeat_at);
                }
                kb_mask &= pad.buttons;
                drive_keyboard(&app, &RawPad { buttons: pad.buttons & !kb_mask, ..pad }, &mut nav);
            } else {
                if kb_was_open {
                    prev = pad.buttons;
                }
                drive_desktop(&app, conf, &pad, prev, dt, &mut active, &mut repeat_at, &mut cursor);
            }
            kb_was_open = kb_open;
            prev = pad.buttons;
            std::thread::sleep(Duration::from_millis(if pad.active() { 6 } else { 16 }));
        }
    }

    fn release_all(active: &mut [Act; 16], repeat_at: &mut [Option<Instant>; 16]) {
        for (i, a) in active.iter_mut().enumerate() {
            release(*a);
            *a = Act::None;
            repeat_at[i] = None;
        }
    }

    fn release(a: Act) {
        match a {
            Act::Click(c) => press_click(c, false),
            Act::Key(vk, _) => key_up(vk),
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn drive_desktop(
        app: &AppHandle,
        conf: &Config,
        pad: &RawPad,
        prev: u32,
        dt: f32,
        active: &mut [Act; 16],
        repeat_at: &mut [Option<Instant>; 16],
        cursor: &mut Mouse,
    ) {
        let now = Instant::now();
        for (i, name) in BUTTONS.iter().enumerate() {
            let down = pad.buttons >> i & 1 != 0;
            let was = prev >> i & 1 != 0;
            if down && !was {
                let a = conf.map.get(*name).map(|s| act_of(s)).unwrap_or(Act::None);
                active[i] = a;
                match a {
                    Act::Click(c) => press_click(c, true),
                    Act::Key(vk, rep) => {
                        cursor.quiet_until = now + NAV_QUIET;
                        key_down(vk);
                        repeat_at[i] = rep.then(|| now + REPEAT_DELAY);
                    }
                    Act::Chord(keys) => {
                        cursor.quiet_until = now + NAV_QUIET;
                        chord(keys)
                    }
                    Act::Keyboard => open_keyboard(app, pad.sony),
                    Act::None => {}
                }
            } else if !down && was {
                if active[i] == Act::Click(0) && conf.auto_keyboard {
                    // A click may have landed on a text field: the worker looks once the focus has settled.
                    let mut p = Point::default();
                    if unsafe { GetCursorPos(&mut p) } != 0 {
                        if let Some(tx) = CLICKS.get().and_then(|t| t.lock().ok()) {
                            let _ = tx.send((p.x, p.y));
                        }
                    }
                }
                release(active[i]);
                active[i] = Act::None;
                repeat_at[i] = None;
            } else if down {
                if let (Act::Key(vk, _), Some(at)) = (active[i], repeat_at[i]) {
                    if now >= at {
                        key_down(vk); // a held key is sent again, as a keyboard's own repeat would
                        repeat_at[i] = Some(now + REPEAT_RATE);
                    }
                }
            }
        }

        // Left stick: the cursor. A radial dead zone and a curve make small pushes precise, big ones fast.
        let curve = |x: f32, y: f32| -> (f32, f32) {
            let mag = (x * x + y * y).sqrt();
            if mag < STICK_DEAD {
                return (0.0, 0.0);
            }
            let m = ((mag - STICK_DEAD) / (1.0 - STICK_DEAD)).min(1.0);
            let k = m.powf(1.8) / mag;
            (x * k, y * k)
        };
        // A key held down (an arrow walking through a menu) keeps the cursor quiet too.
        if active.iter().any(|a| matches!(a, Act::Key(..))) {
            cursor.quiet_until = now + NAV_QUIET;
        }
        let [ax, ay, sx, sy] = &mut cursor.acc;
        let mag = (pad.lx * pad.lx + pad.ly * pad.ly).sqrt();
        let start = if now < cursor.quiet_until { MOUSE_START_NAV } else { MOUSE_START };
        cursor.moving = mag >= start || (cursor.moving && mag >= STICK_DEAD);
        let (mx, my) = if cursor.moving { curve(pad.lx, pad.ly) } else { (0.0, 0.0) };
        if mx != 0.0 || my != 0.0 {
            let speed = 350.0 + 230.0 * f32::from(conf.mouse_speed); // pixels per second at full push
            *ax += mx * speed * dt;
            *ay += my * speed * dt;
            let (dx, dy) = (ax.trunc(), ay.trunc());
            if dx != 0.0 || dy != 0.0 {
                *ax -= dx;
                *ay -= dy;
                let mut p = Point::default();
                unsafe {
                    if GetCursorPos(&mut p) != 0 {
                        SetCursorPos(p.x + dx as i32, p.y + dy as i32);
                    }
                }
            }
        } else {
            *ax = 0.0;
            *ay = 0.0;
        }

        // Right stick: scrolling. One wheel notch is 120 units.
        let (wx, wy) = curve(pad.rx, pad.ry);
        if wx != 0.0 || wy != 0.0 {
            let per_sec = 2.0 + 1.6 * f32::from(conf.scroll_speed); // notches per second at full push
            *sx += wx * per_sec * dt * 120.0;
            *sy -= wy * per_sec * dt * 120.0;
            let (hx, hy) = (sx.trunc() as i32, sy.trunc() as i32);
            if hy != 0 {
                *sy -= hy as f32;
                mouse(MOUSEEVENTF_WHEEL, hy);
            }
            if hx != 0 {
                *sx -= hx as f32;
                mouse(MOUSEEVENTF_HWHEEL, hx);
            }
        } else {
            *sx = 0.0;
            *sy = 0.0;
        }
    }

    // ---- The on-screen keyboard -------------------------------------------------------------------------

    /// Buttons of the pad, as the abstract actions the keyboard window understands (the same names the
    /// interface's own input uses). Directions repeat while held.
    fn drive_keyboard(app: &AppHandle, pad: &RawPad, nav: &mut HashMap<&'static str, Held>) {
        let now = Instant::now();
        let mut state: Vec<(&'static str, bool, bool)> = Vec::new(); // (action, down, repeats)
        let b = |i: usize| pad.buttons >> i & 1 != 0;
        state.push(("confirm", b(0), false));
        state.push(("back", b(1), false));
        state.push(("delete", b(2), true));
        state.push(("options", b(3), false));
        state.push(("prevTab", b(4), false));
        state.push(("nextTab", b(5), false));
        state.push(("first", b(6), false));
        state.push(("last", b(7), false));
        state.push(("menu", b(8), false));
        state.push(("mode", b(11), false));
        state.push(("up", b(12) || pad.ly < -NAV_STICK, true));
        state.push(("down", b(13) || pad.ly > NAV_STICK, true));
        state.push(("left", b(14) || pad.lx < -NAV_STICK, true));
        state.push(("right", b(15) || pad.lx > NAV_STICK, true));
        for (action, down, repeats) in state {
            let h = nav.entry(action).or_default();
            if !down {
                *h = Held::default();
                continue;
            }
            if h.since.is_none() {
                h.since = Some(now);
                h.next = repeats.then(|| now + REPEAT_DELAY);
                let _ = app.emit("osk-action", action);
            } else if let Some(at) = h.next {
                if now >= at {
                    h.next = Some(now + REPEAT_RATE);
                    let _ = app.emit("osk-action", action);
                }
            }
        }
    }

    fn open_keyboard(app: &AppHandle, sony: bool) {
        use tauri::{WebviewUrl, WebviewWindowBuilder};
        let (w, h, x, y) = keyboard_rect(app);
        let win = match app.get_webview_window("osk") {
            Some(w0) => w0,
            None => {
                let built = WebviewWindowBuilder::new(app, "osk", WebviewUrl::App("index.html?osk".into()))
                    .title("Lumen keyboard")
                    .decorations(false)
                    .transparent(true)
                    .shadow(false)
                    .resizable(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .focusable(false)
                    .focused(false)
                    .visible(false)
                    .build();
                match built {
                    Ok(w0) => w0,
                    Err(_) => return,
                }
            }
        };
        let _ = win.set_size(tauri::PhysicalSize::new(w, h));
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
        let _ = win.show();
        KEYBOARD.store(true, Ordering::Relaxed);
        SONY.store(sony, Ordering::Relaxed);
        let kind = if sony { "playstation" } else { "xbox" };
        let _ = app.emit("osk-show", kind);
    }

    pub fn close_keyboard(app: &AppHandle) {
        KEYBOARD.store(false, Ordering::Relaxed);
        if let Some(w) = app.get_webview_window("osk") {
            let _ = w.hide();
        }
    }

    /// The bottom of the monitor under the cursor: full width, tall enough for the keyboard, which the page
    /// sizes by the width of the screen (47 units of 1/192 of it).
    fn keyboard_rect(app: &AppHandle) -> (u32, u32, i32, i32) {
        let monitor = app
            .cursor_position()
            .ok()
            .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
            .or_else(|| app.primary_monitor().ok().flatten());
        match monitor {
            Some(m) => {
                let (size, pos) = (m.size(), m.position());
                let h = ((size.width as f32 * 0.245) as u32).min(size.height * 3 / 5);
                (size.width, h, pos.x, pos.y + (size.height - h) as i32)
            }
            None => (1920, 450, 0, 630),
        }
    }
}
