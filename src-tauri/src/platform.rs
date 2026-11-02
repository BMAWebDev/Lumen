//! Tot ce ține de sistemul de operare: procese, ferestre, registry, alimentare.
//! Pe alte sisteme decât Windows funcțiile sunt înlocuite cu variante inofensive,
//! ca proiectul să se poată compila oriunde.

use std::path::PathBuf;
use std::process::Command;

use crate::util;

/// O comandă care nu deschide o fereastră de consolă.
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
        Err(format!("comanda a eșuat ({status})"))
    }
}

pub fn powershell(script: &str) -> Result<String, String> {
    let encoded = util::powershell_encode(script);
    let out = hidden("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand"])
        .arg(encoded)
        .output()
        .map_err(|e| format!("PowerShell nu a putut fi pornit: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

// ---------------------------------------------------------------------------
// Windows API (declarații directe, fără crate-uri suplimentare)
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
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn SetForegroundWindow(hwnd: isize) -> i32;
        pub fn BringWindowToTop(hwnd: isize) -> i32;
        pub fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
        pub fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
    }

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

/// Lista proceselor care rulează, cu calea completă a executabilului.
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

/// PID-urile proceselor pornite din folderul unui joc.
pub fn procs_under(dir: &str) -> Vec<u32> {
    process_paths()
        .into_iter()
        .filter(|(_, path)| util::is_inside(path, dir))
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

/// Aduce fereastra lansatorului în prim-plan, chiar dacă un joc are focusul.
pub fn bring_to_front(w: &tauri::WebviewWindow) {
    let _ = w.unminimize();
    let _ = w.show();
    #[cfg(windows)]
    {
        if let Ok(hwnd) = w.hwnd() {
            use win::*;
            let h = hwnd.0 as isize;
            unsafe {
                // Windows permite SetForegroundWindow doar procesului care a primit
                // ultimul input; o apăsare scurtă de Alt deblochează restricția.
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

// ---------------------------------------------------------------------------
// Steam, URI-uri, foldere
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

/// În afara Windows, se poate indica un folder Steam pentru teste: LUMEN_STEAM_ROOT.
#[cfg(not(windows))]
pub fn steam_root() -> Option<PathBuf> {
    std::env::var_os("LUMEN_STEAM_ROOT").map(PathBuf::from)
}

pub fn open_uri(uri: &str) -> Result<(), String> {
    #[cfg(windows)]
    let program = "explorer.exe";
    #[cfg(not(windows))]
    let program = "xdg-open";
    hidden(program).arg(uri).spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Pornire odată cu Windows
// ---------------------------------------------------------------------------

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(windows)]
pub fn autostart_enabled() -> bool {
    hidden("reg")
        .args(["query", RUN_KEY, "/v", "Lumen"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn autostart_enabled() -> bool {
    false
}

#[cfg(windows)]
pub fn set_autostart(on: bool) -> Result<(), String> {
    if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = format!("\"{}\"", exe.display());
        run(hidden("reg").args(["add", RUN_KEY, "/v", "Lumen", "/t", "REG_SZ", "/d", value.as_str(), "/f"]))
    } else {
        run(hidden("reg").args(["delete", RUN_KEY, "/v", "Lumen", "/f"]))
    }
}

#[cfg(not(windows))]
pub fn set_autostart(_on: bool) -> Result<(), String> {
    Err("Disponibil doar pe Windows.".into())
}

// ---------------------------------------------------------------------------
// Alimentare
// ---------------------------------------------------------------------------

pub fn power(action: &str) -> Result<(), String> {
    match action {
        "shutdown" => run(hidden("shutdown").args(["/s", "/t", "0"])),
        "restart" => run(hidden("shutdown").args(["/r", "/t", "0"])),
        "sleep" => powershell(
            "Add-Type -AssemblyName System.Windows.Forms; \
             [System.Windows.Forms.Application]::SetSuspendState('Suspend', $false, $false) | Out-Null",
        )
        .map(|_| ()),
        _ => Err(format!("Acțiune necunoscută: {action}")),
    }
}
