//! Everything that has to do with the operating system: processes, windows, registry, power.
//! On systems other than Windows the functions are replaced with harmless variants,
//! so the project can be compiled anywhere.

use std::path::PathBuf;
use std::process::Command;

use crate::util;

/// A command that doesn't open a console window.
pub fn hidden(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let status = cmd.status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(crate::t!(format!("the command failed ({status})"), format!("comanda a eșuat ({status})")))
    }
}

pub fn powershell(script: &str) -> Result<String, String> {
    let encoded = util::powershell_encode(script);
    let out = hidden("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand"])
        .arg(encoded)
        .output()
        .map_err(|e| crate::t!(format!("PowerShell could not be started: {e}"), format!("PowerShell nu a putut fi pornit: {e}")))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

// ---------------------------------------------------------------------------
// Windows API (direct declarations, no extra crates)
// ---------------------------------------------------------------------------

#[cfg(windows)]
#[allow(non_snake_case, clippy::upper_case_acronyms)]
pub(crate) mod win {
    use std::ffi::c_void;

    pub type HANDLE = isize;
    pub const INVALID_HANDLE_VALUE: HANDLE = -1;
    pub const TH32CS_SNAPPROCESS: u32 = 0x2;
    pub const PROCESS_TERMINATE: u32 = 0x0001;
    pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    pub const PROCESS_SUSPEND_RESUME: u32 = 0x0800;

    #[repr(C)]
    pub struct ProcessEntry32W {
        pub dw_size: u32,
        pub cnt_usage: u32,
        pub pid: u32,
        pub default_heap_id: usize,
        pub module_id: u32,
        pub cnt_threads: u32,
        pub parent_pid: u32,
        pub pri_class_base: i32,
        pub flags: u32,
        pub exe_file: [u16; 260],
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[repr(C)]
    pub struct SystemPowerStatus {
        pub ac_line_status: u8,
        pub battery_flag: u8,
        pub battery_life_percent: u8,
        pub system_status_flag: u8,
        pub battery_life_time: u32,
        pub battery_full_life_time: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> HANDLE;
        pub fn Process32FirstW(snap: HANDLE, entry: *mut ProcessEntry32W) -> i32;
        pub fn Process32NextW(snap: HANDLE, entry: *mut ProcessEntry32W) -> i32;
        pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> HANDLE;
        pub fn QueryFullProcessImageNameW(h: HANDLE, flags: u32, buf: *mut u16, size: *mut u32) -> i32;
        pub fn TerminateProcess(h: HANDLE, code: u32) -> i32;
        pub fn CloseHandle(h: HANDLE) -> i32;
        pub fn LoadLibraryW(name: *const u16) -> HANDLE;
        pub fn GetProcAddress(module: HANDLE, name: *const u8) -> *const c_void;
        pub fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
        pub fn CreateMutexW(attrs: *const c_void, initial_owner: i32, name: *const u16) -> HANDLE;
        pub fn OpenMutexW(access: u32, inherit: i32, name: *const u16) -> HANDLE;
        pub fn GetLastError() -> u32;
    }

    #[link(name = "ntdll")]
    extern "system" {
        pub fn NtSuspendProcess(h: HANDLE) -> i32;
        pub fn NtResumeProcess(h: HANDLE) -> i32;
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn GetForegroundWindow() -> isize;
        pub fn EnumChildWindows(parent: isize, cb: extern "system" fn(isize, isize) -> i32, lparam: isize) -> i32;
        pub fn SetForegroundWindow(hwnd: isize) -> i32;
        pub fn BringWindowToTop(hwnd: isize) -> i32;
        pub fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
        pub fn EnumWindows(cb: extern "system" fn(isize, isize) -> i32, lparam: isize) -> i32;
        pub fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
        pub fn IsWindowVisible(hwnd: isize) -> i32;
        pub fn IsIconic(hwnd: isize) -> i32;
        pub fn GetWindow(hwnd: isize, cmd: u32) -> isize;
        pub fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
        pub fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
    }

    #[link(name = "dwmapi")]
    extern "system" {
        pub fn DwmGetWindowAttribute(hwnd: isize, attr: u32, value: *mut c_void, size: u32) -> i32;
    }

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

/// The list of running processes, with the full path of the executable.
#[cfg(windows)]
pub fn process_paths() -> Vec<(u32, String)> {
    use win::*;
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE || snap == 0 {
            return out;
        }
        let mut e: ProcessEntry32W = std::mem::zeroed();
        e.dw_size = std::mem::size_of::<ProcessEntry32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e);
        while ok != 0 {
            let pid = e.pid;
            if pid > 4 {
                let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                if h != 0 {
                    let mut buf = [0u16; 1024];
                    let mut len = buf.len() as u32;
                    if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0 {
                        out.push((pid, String::from_utf16_lossy(&buf[..len as usize])));
                    }
                    CloseHandle(h);
                }
            }
            ok = Process32NextW(snap, &mut e);
        }
        CloseHandle(snap);
    }
    out
}

#[cfg(not(windows))]
pub fn process_paths() -> Vec<(u32, String)> {
    Vec::new()
}

#[cfg(windows)]
extern "system" fn collect_child_pid(hwnd: isize, lparam: isize) -> i32 {
    let mut pid = 0u32;
    unsafe {
        win::GetWindowThreadProcessId(hwnd, &mut pid);
        (*(lparam as *mut Vec<u32>)).push(pid);
    }
    1
}

/// The executables behind a window: its own process and, for Microsoft Store apps (whose window
/// is a frame owned by `ApplicationFrameHost.exe`), the process of the app drawn inside it.
#[cfg(windows)]
pub fn window_exes(hwnd: isize) -> Vec<String> {
    use win::*;
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    let mut pids = vec![pid];
    let paths = process_paths();
    let is_frame_host = |pid: u32| {
        paths.iter().any(|(p, path)| *p == pid && path.to_lowercase().ends_with(r"\applicationframehost.exe"))
    };
    if is_frame_host(pid) {
        let mut children: Vec<u32> = Vec::new();
        unsafe { EnumChildWindows(hwnd, collect_child_pid, &mut children as *mut Vec<u32> as isize) };
        pids.extend(children.into_iter().filter(|c| !is_frame_host(*c)));
    }
    pids.sort_unstable();
    pids.dedup();
    paths.into_iter().filter(|(p, _)| pids.contains(p)).map(|(_, path)| path).collect()
}

#[cfg(windows)]
struct Visible {
    min: (i32, i32),
    wins: Vec<(isize, u32)>,
}

#[cfg(windows)]
extern "system" fn collect_visible(hwnd: isize, lparam: isize) -> i32 {
    use win::*;
    // SAFETY: `lparam` is the address of a `Visible`, valid for the duration of EnumWindows.
    let v = unsafe { &mut *(lparam as *mut Visible) };
    const GW_OWNER: u32 = 4;
    const DWMWA_CLOAKED: u32 = 14;
    if unsafe { IsWindowVisible(hwnd) } == 0 || unsafe { GetWindow(hwnd, GW_OWNER) } != 0 || unsafe { IsIconic(hwnd) } != 0 {
        return 1;
    }
    // Closed Store apps can leave a "visible" but cloaked frame behind.
    let mut cloaked = 0u32;
    unsafe { DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut std::ffi::c_void, 4) };
    if cloaked != 0 {
        return 1;
    }
    let mut r = Rect::default();
    unsafe { GetWindowRect(hwnd, &mut r) };
    if r.right - r.left >= v.min.0 && r.bottom - r.top >= v.min.1 {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        v.wins.push((hwnd, pid));
    }
    1
}

/// The executables that own a visible, normal-sized window right now. Like `window_exes`, it looks
/// inside the frames of Microsoft Store apps. Minimized and cloaked windows don't count.
#[cfg(windows)]
pub fn visible_window_exes(min: (i32, i32)) -> Vec<String> {
    use win::*;
    let mut v = Visible { min, wins: Vec::new() };
    unsafe { EnumWindows(collect_visible, &mut v as *mut Visible as isize) };
    let paths = process_paths();
    let is_frame_host = |pid: u32| paths.iter().any(|(p, path)| *p == pid && path.to_lowercase().ends_with(r"\applicationframehost.exe"));
    let mut pids: Vec<u32> = Vec::new();
    for (hwnd, pid) in &v.wins {
        pids.push(*pid);
        if is_frame_host(*pid) {
            let mut children: Vec<u32> = Vec::new();
            unsafe { EnumChildWindows(*hwnd, collect_child_pid, &mut children as *mut Vec<u32> as isize) };
            pids.extend(children.into_iter().filter(|c| !is_frame_host(*c)));
        }
    }
    pids.sort_unstable();
    pids.dedup();
    paths.into_iter().filter(|(p, _)| pids.contains(p)).map(|(_, path)| path).collect()
}

#[cfg(not(windows))]
pub fn visible_window_exes(_min: (i32, i32)) -> Vec<String> {
    Vec::new()
}

/// The executables behind the window currently in front.
#[cfg(windows)]
pub fn foreground_exes() -> Vec<String> {
    let hwnd = unsafe { win::GetForegroundWindow() };
    if hwnd == 0 {
        return Vec::new();
    }
    window_exes(hwnd)
}

#[cfg(not(windows))]
pub fn foreground_exes() -> Vec<String> {
    Vec::new()
}

/// The real folder behind junctions. Xbox games moved to another disk have
/// `C:\Program Files\WindowsApps\…` → `D:\WindowsApps\…` → `D:\Games\<game>\Content`,
/// and Windows reports the processes with the final path.
fn real_dir(dir: &str) -> Option<String> {
    let p = std::fs::canonicalize(dir).ok()?.to_string_lossy().into_owned();
    let p = match p.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => p.strip_prefix(r"\\?\").unwrap_or(&p).to_string(),
    };
    (util::norm_path(&p) != util::norm_path(dir)).then_some(p)
}

/// The PIDs of the processes started from a game's folder.
pub fn procs_under(dir: &str) -> Vec<u32> {
    let real = real_dir(dir);
    process_paths()
        .into_iter()
        .filter(|(_, path)| util::is_inside(path, dir) || real.as_deref().is_some_and(|r| util::is_inside(path, r)))
        .map(|(pid, _)| pid)
        .collect()
}

#[cfg(windows)]
pub fn kill(pid: u32) {
    use win::*;
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if h != 0 {
            TerminateProcess(h, 1);
            CloseHandle(h);
        }
    }
}

#[cfg(not(windows))]
pub fn kill(_pid: u32) {}

/// Pauses (or resumes) all the threads of a process. A game without focus still
/// receives controller input, so we freeze it while the menu is open.
#[cfg(windows)]
pub fn set_suspended(pid: u32, suspend: bool) -> bool {
    use win::*;
    unsafe {
        let h = OpenProcess(PROCESS_SUSPEND_RESUME, 0, pid);
        if h == 0 {
            return false;
        }
        let status = if suspend { NtSuspendProcess(h) } else { NtResumeProcess(h) };
        CloseHandle(h);
        status >= 0
    }
}

#[cfg(not(windows))]
pub fn set_suspended(_pid: u32, _suspend: bool) -> bool {
    false
}

/// Brings the launcher window to the foreground, even if a game has focus.
pub fn bring_to_front(w: &tauri::WebviewWindow) {
    let _ = w.unminimize();
    let _ = w.show();
    #[cfg(windows)]
    {
        if let Ok(hwnd) = w.hwnd() {
            use win::*;
            let h = hwnd.0 as isize;
            unsafe {
                // Windows only allows SetForegroundWindow for the process that received the
                // last input; a short Alt press lifts the restriction.
                keybd_event(0x12, 0, 0, 0);
                keybd_event(0x12, 0, 2, 0);
                ShowWindow(h, 9);
                BringWindowToTop(h);
                SetForegroundWindow(h);
            }
        }
    }
    let _ = w.set_focus();
    let _ = w.set_fullscreen(true);
}

/// Brings back to the foreground the main window of one of the processes (the largest
/// visible window with no owner), restoring it if it is minimized.
/// Returns `false` if the processes have no window.
pub fn focus_window_of(pids: &[u32]) -> bool {
    focus_window(pids, (0, 0))
}

/// Like `focus_window_of`, but ignores windows smaller than `min` (width, height):
/// a game's splash screens and launchers are not the game window.
#[cfg(windows)]
pub fn focus_window(pids: &[u32], min: (i32, i32)) -> bool {
    use win::*;
    struct Search<'a> {
        pids: &'a [u32],
        min: (i32, i32),
        best: isize,
        area: i64,
    }
    extern "system" fn visit(hwnd: isize, lparam: isize) -> i32 {
        // SAFETY: `lparam` is the address of `Search`, valid for the duration of EnumWindows.
        let s = unsafe { &mut *(lparam as *mut Search) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        const GW_OWNER: u32 = 4;
        if s.pids.contains(&pid) && unsafe { IsWindowVisible(hwnd) } != 0 && unsafe { GetWindow(hwnd, GW_OWNER) } == 0 {
            let mut r = Rect::default();
            unsafe { GetWindowRect(hwnd, &mut r) };
            if r.right - r.left < s.min.0 || r.bottom - r.top < s.min.1 {
                return 1;
            }
            // A minimized window has a small rectangle, but it is still a candidate.
            let area = ((r.right - r.left) as i64 * (r.bottom - r.top) as i64).max(1);
            if area > s.area {
                s.best = hwnd;
                s.area = area;
            }
        }
        1
    }
    let mut s = Search { pids, min, best: 0, area: 0 };
    unsafe { EnumWindows(visit, &mut s as *mut Search as isize) };
    if s.best == 0 {
        return false;
    }
    unsafe {
        // As in `bring_to_front`: an Alt press unlocks SetForegroundWindow.
        keybd_event(0x12, 0, 0, 0);
        keybd_event(0x12, 0, 2, 0);
        ShowWindow(s.best, if IsIconic(s.best) != 0 { 9 } else { 5 }); // SW_RESTORE / SW_SHOW
        BringWindowToTop(s.best);
        SetForegroundWindow(s.best);
    }
    true
}

#[cfg(not(windows))]
pub fn focus_window(_pids: &[u32], _min: (i32, i32)) -> bool {
    false
}

// ---------------------------------------------------------------------------
// Steam, URIs, folders
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub fn steam_root() -> Option<PathBuf> {
    if let Ok(o) = hidden("reg")
        .args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
        .output()
    {
        let text = String::from_utf8_lossy(&o.stdout).to_string();
        for line in text.lines() {
            if let Some(idx) = line.find("REG_SZ") {
                let p = line[idx + 6..].trim();
                if !p.is_empty() {
                    let pb = PathBuf::from(p.replace('/', "\\"));
                    if pb.exists() {
                        return Some(pb);
                    }
                }
            }
        }
    }
    [r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

/// Outside Windows, a Steam folder can be given for tests: LUMEN_STEAM_ROOT.
#[cfg(not(windows))]
pub fn steam_root() -> Option<PathBuf> {
    std::env::var_os("LUMEN_STEAM_ROOT").map(PathBuf::from)
}

/// Starts an executable from its own folder (many games look for their files relative to it).
pub fn run_exe(exe: &str, args: &[String]) -> Result<(), String> {
    let path = std::path::Path::new(exe);
    if !path.is_file() {
        return Err(crate::t!("The game executable no longer exists.", "Executabilul jocului nu mai există.").into());
    }
    let mut cmd = Command::new(path);
    cmd.args(args);
    if let Some(dir) = path.parent() {
        cmd.current_dir(dir);
    }
    match cmd.spawn() {
        Ok(_) => Ok(()),
        // 740: the game requires administrator rights; we start it through UAC.
        #[cfg(windows)]
        Err(e) if e.raw_os_error() == Some(740) => {
            let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
            let dir = path.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
            let mut script = format!("Start-Process -FilePath {} -WorkingDirectory {} -Verb RunAs", q(exe), q(&dir));
            if !args.is_empty() {
                let list: Vec<String> = args.iter().map(|a| q(a)).collect();
                script.push_str(&format!(" -ArgumentList @({})", list.join(",")));
            }
            powershell(&script).map(|_| ())
        }
        Err(e) => Err(e.to_string()),
    }
}

pub fn open_uri(uri: &str) -> Result<(), String> {
    #[cfg(windows)]
    let program = "explorer.exe";
    #[cfg(not(windows))]
    let program = "xdg-open";
    hidden(program).arg(uri).spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Start with Windows
// ---------------------------------------------------------------------------

/// Takes a named mutex, held until the process exits. `false` if another process already holds it.
#[cfg(windows)]
pub fn claim_mutex(name: &str) -> bool {
    const ERROR_ALREADY_EXISTS: u32 = 183;
    let name = win::wide(name);
    let h = unsafe { win::CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    h != 0 && unsafe { win::GetLastError() } != ERROR_ALREADY_EXISTS
}

#[cfg(windows)]
pub fn mutex_exists(name: &str) -> bool {
    const SYNCHRONIZE: u32 = 0x0010_0000;
    let name = win::wide(name);
    let h = unsafe { win::OpenMutexW(SYNCHRONIZE, 0, name.as_ptr()) };
    if h == 0 {
        return false;
    }
    unsafe { win::CloseHandle(h) };
    true
}

#[cfg(not(windows))]
pub fn claim_mutex(_name: &str) -> bool {
    true
}

#[cfg(not(windows))]
pub fn mutex_exists(_name: &str) -> bool {
    false
}

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// Windows delays the applications in the `Run` key by a few seconds. A logon task
/// starts immediately, so autostart uses Task Scheduler.
#[cfg(windows)]
const TASK_NAME: &str = "Lumen";

/// The old autostart, through the registry. Only for migrating to the task.
#[cfg(windows)]
pub fn legacy_autostart_enabled() -> bool {
    hidden("reg")
        .args(["query", RUN_KEY, "/v", "Lumen"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn legacy_autostart_enabled() -> bool {
    false
}

#[cfg(windows)]
pub fn set_autostart(on: bool) -> Result<(), String> {
    if on {
        if cfg!(debug_assertions) {
            return Err(crate::t!("Starting with Windows can only be enabled from the installed version.", "Pornirea cu Windows se activează doar din versiunea instalată.").into());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let q = |s: &str| s.replace('\'', "''");
        let dir = q(&exe.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default());
        let exe = q(&exe.to_string_lossy());
        // Priority 4 (normal): by default a task runs at 7, which is slower.
        let script = format!(
            "$ErrorActionPreference = 'Stop';              $a = New-ScheduledTaskAction -Execute '{exe}' -WorkingDirectory '{dir}';              $t = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME;              $s = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries                   -ExecutionTimeLimit ([TimeSpan]::Zero) -Priority 4;              $p = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited;              Register-ScheduledTask -TaskName '{TASK_NAME}' -Action $a -Trigger $t -Settings $s -Principal $p -Force | Out-Null;              'ok'"
        );
        let out = powershell(&script)?;
        if !out.contains("ok") {
            return Err(crate::t!("Could not create the startup task.", "Nu s-a putut crea task-ul de pornire.").into());
        }
        let _ = hidden("reg").args(["delete", RUN_KEY, "/v", "Lumen", "/f"]).status();
        Ok(())
    } else {
        let _ = powershell(&format!(
            "Unregister-ScheduledTask -TaskName '{TASK_NAME}' -Confirm:$false -ErrorAction SilentlyContinue"
        ));
        let _ = hidden("reg").args(["delete", RUN_KEY, "/v", "Lumen", "/f"]).status();
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn set_autostart(_on: bool) -> Result<(), String> {
    Err(crate::t!("Only available on Windows.", "Disponibil doar pe Windows.").into())
}

// ---------------------------------------------------------------------------
// Power
// ---------------------------------------------------------------------------

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
    pub plugged: bool,
}

/// The battery state, or `None` on a PC without a battery.
#[cfg(windows)]
pub fn battery() -> Option<Battery> {
    use win::*;
    let mut st: SystemPowerStatus = unsafe { std::mem::zeroed() };
    if unsafe { GetSystemPowerStatus(&mut st) } == 0 {
        return None;
    }
    // 128 = no battery, 255 = unknown state.
    if st.battery_flag & 128 != 0 || st.battery_flag == 255 || st.battery_life_percent > 100 {
        return None;
    }
    Some(Battery {
        percent: st.battery_life_percent,
        charging: st.battery_flag & 8 != 0,
        plugged: st.ac_line_status == 1,
    })
}

/// Whether the Windows regional settings use a 24-hour clock (`None` if it can't be read).
#[cfg(windows)]
pub fn clock_24h() -> Option<bool> {
    let out = hidden("reg")
        .args(["query", r"HKCU\Control Panel\International", "/v", "sTimeFormat"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    // The format looks like `HH:mm:ss` (24-hour) or `h:mm:ss tt` (12-hour).
    let format = text.lines().find_map(|l| l.split_once("REG_SZ").map(|(_, v)| v.trim().to_string()))?;
    Some(format.contains('H'))
}

#[cfg(not(windows))]
pub fn clock_24h() -> Option<bool> {
    None
}

#[cfg(not(windows))]
pub fn battery() -> Option<Battery> {
    None
}

pub fn power(action: &str) -> Result<(), String> {
    match action {
        "shutdown" => run(hidden("shutdown").args(["/s", "/t", "0"])),
        "restart" => run(hidden("shutdown").args(["/r", "/t", "0"])),
        "sleep" => powershell(
            "Add-Type -AssemblyName System.Windows.Forms; \
             [System.Windows.Forms.Application]::SetSuspendState('Suspend', $false, $false) | Out-Null",
        )
        .map(|_| ()),
        _ => Err(crate::t!(format!("Unknown action: {action}"), format!("Acțiune necunoscută: {action}"))),
    }
}
