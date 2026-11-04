//! Detects the gaming platforms and game stores installed on this PC and opens them.
//!
//! Detection only looks at well-known install locations (no registry or PowerShell), so it is instant.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::model::AppState;
use crate::platform;

/// How a platform is started.
enum Launch {
    /// One of these executables, relative to one of the base folders below.
    Exe(&'static [&'static str]),
    /// A package that is started through the shell (Microsoft Store apps).
    Shell { package_dir: &'static str, aumid: &'static str },
}

struct Def {
    id: &'static str,
    name: &'static str,
    launch: Launch,
}

/// Executable paths are tried under each of the base folders (`%ProgramFiles%`, `%ProgramFiles(x86)%`,
/// `%LOCALAPPDATA%`, `%ProgramData%`) and, for Riot, at the root of every drive.
const DEFS: &[Def] = &[
    Def { id: "steam", name: "Steam", launch: Launch::Exe(&[]) },
    Def {
        id: "epic",
        name: "Epic Games Store",
        launch: Launch::Exe(&[
            r"Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe",
            r"Epic Games\Launcher\Portal\Binaries\Win32\EpicGamesLauncher.exe",
        ]),
    },
    Def { id: "gog", name: "GOG Galaxy", launch: Launch::Exe(&[r"GOG Galaxy\GalaxyClient.exe"]) },
    Def { id: "ea", name: "EA app", launch: Launch::Exe(&[r"Electronic Arts\EA Desktop\EA Desktop\EADesktop.exe"]) },
    Def { id: "origin", name: "Origin", launch: Launch::Exe(&[r"Origin\Origin.exe"]) },
    Def {
        id: "ubisoft",
        name: "Ubisoft Connect",
        launch: Launch::Exe(&[r"Ubisoft\Ubisoft Game Launcher\UbisoftConnect.exe", r"Ubisoft\Ubisoft Game Launcher\upc.exe"]),
    },
    Def { id: "battlenet", name: "Battle.net", launch: Launch::Exe(&[r"Battle.net\Battle.net Launcher.exe", r"Battle.net\Battle.net.exe"]) },
    Def { id: "rockstar", name: "Rockstar Games Launcher", launch: Launch::Exe(&[r"Rockstar Games\Launcher\Launcher.exe"]) },
    Def { id: "riot", name: "Riot Client", launch: Launch::Exe(&[r"Riot Games\Riot Client\RiotClientServices.exe"]) },
    Def { id: "amazon", name: "Amazon Games", launch: Launch::Exe(&[r"Amazon Games\App\Amazon Games.exe"]) },
    Def { id: "itch", name: "itch.io", launch: Launch::Exe(&[r"itch\itch.exe"]) },
    Def {
        id: "xbox",
        name: "Xbox",
        launch: Launch::Shell { package_dir: "Microsoft.GamingApp_8wekyb3d8bbwe", aumid: r"Microsoft.GamingApp_8wekyb3d8bbwe!Microsoft.Xbox.App" },
    },
];

#[derive(Debug, Clone, Serialize)]
pub struct Platform {
    pub id: String,
    pub name: String,
    /// The platform's own icon (a PNG cached in the data folder), when it could be extracted.
    pub icon: Option<String>,
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from).filter(|p| p.is_dir())
}

/// The folders where launchers usually live, without duplicates.
fn bases() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA", "ProgramData"].iter().filter_map(|n| env_dir(n)).collect();
    // Riot installs at the root of a drive (C:\Riot Games by default).
    v.extend(('C'..='Z').map(|d| PathBuf::from(format!("{d}:\\"))).filter(|p| p.is_dir()));
    let mut seen = std::collections::HashSet::new();
    v.retain(|p| seen.insert(crate::util::norm_path(&p.to_string_lossy())));
    v
}

fn steam_exe() -> Option<PathBuf> {
    platform::steam_root().map(|r| r.join("steam.exe")).filter(|p| p.is_file())
}

/// The executable of an `Exe` platform, if it is installed.
fn find_exe(def: &Def) -> Option<PathBuf> {
    let Launch::Exe(rels) = &def.launch else { return None };
    if def.id == "steam" {
        return steam_exe();
    }
    let bases = bases();
    rels.iter().flat_map(|rel| bases.iter().map(move |b| b.join(rel))).find(|p| p.is_file())
}

fn installed(def: &Def) -> bool {
    match &def.launch {
        Launch::Exe(_) => find_exe(def).is_some(),
        Launch::Shell { package_dir, .. } => env_dir("LOCALAPPDATA").is_some_and(|d| d.join("Packages").join(package_dir).is_dir()),
    }
}

/// Saves the real icons: the large icon of the executable, or the logo from the manifest of a
/// Microsoft Store package. One PowerShell run handles every missing icon; the PNGs are cached.
const ICON_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class LumenIcon {
  [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
  public static extern int SHDefExtractIconW(string file, int index, uint flags, out IntPtr large, out IntPtr small, uint size);
  [DllImport("user32.dll")]
  public static extern bool DestroyIcon(IntPtr h);
}
'@
foreach ($it in $items) {
  $kind, $src, $out = $it
  if ($kind -eq 'exe') {
    $large = [IntPtr]::Zero; $small = [IntPtr]::Zero
    if ([LumenIcon]::SHDefExtractIconW($src, 0, 0, [ref]$large, [ref]$small, (16 -shl 16) -bor 256) -eq 0 -and $large -ne [IntPtr]::Zero) {
      $bmp = [System.Drawing.Icon]::FromHandle($large).ToBitmap()
      $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
      $bmp.Dispose()
      [void][LumenIcon]::DestroyIcon($large)
      if ($small -ne [IntPtr]::Zero) { [void][LumenIcon]::DestroyIcon($small) }
    }
  } else {
    # A Store package: $src is the package name; the largest variant of its logo is copied.
    $p = Get-AppxPackage -Name $src | Select-Object -First 1
    if ($p) {
      $m = [xml](Get-Content -LiteralPath (Join-Path $p.InstallLocation 'AppxManifest.xml') -Raw)
      $ve = $m.Package.Applications.Application | Select-Object -First 1 | ForEach-Object { $_.VisualElements }
      $logo = if ($ve.Square150x150Logo) { $ve.Square150x150Logo } else { $ve.Square44x44Logo }
      if ($logo) {
        $full = Join-Path $p.InstallLocation $logo
        $dir = Split-Path $full; $base = [IO.Path]::GetFileNameWithoutExtension($full)
        $best = Get-ChildItem -LiteralPath $dir -Filter "$base*.png" | Sort-Object Length -Descending | Select-Object -First 1
        if ($best) {
          # Store logos come with wide transparent margins: crop to the visible part so the icon fills its tile.
          $src = [System.Drawing.Bitmap]::new($best.FullName)
          $x0 = $src.Width; $y0 = $src.Height; $x1 = -1; $y1 = -1
          for ($y = 0; $y -lt $src.Height; $y++) { for ($x = 0; $x -lt $src.Width; $x++) {
            if ($src.GetPixel($x, $y).A -gt 8) { if ($x -lt $x0) { $x0 = $x }; if ($x -gt $x1) { $x1 = $x }; if ($y -lt $y0) { $y0 = $y }; if ($y -gt $y1) { $y1 = $y } }
          } }
          if ($x1 -ge $x0 -and $y1 -ge $y0) {
            $rect = [System.Drawing.Rectangle]::new($x0, $y0, $x1 - $x0 + 1, $y1 - $y0 + 1)
            $crop = $src.Clone($rect, $src.PixelFormat)
            $crop.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
            $crop.Dispose()
          } else { Copy-Item -LiteralPath $best.FullName -Destination $out -Force }
          $src.Dispose()
        }
      }
    }
  }
}
"#;

fn ps_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The cached icon of each detected platform, extracting the ones that are missing.
fn icons(found: &[&Def], data_dir: &Path) -> Vec<Option<String>> {
    let dir = data_dir.join("platform-icons");
    let _ = fs::create_dir_all(&dir);
    let path = |d: &Def| dir.join(format!("{}.v2.png", d.id));
    let mut jobs = Vec::new();
    for d in found {
        let out = path(d);
        if out.is_file() {
            continue;
        }
        let job = match &d.launch {
            Launch::Exe(_) => find_exe(d).map(|exe| ("exe", exe.to_string_lossy().into_owned())),
            Launch::Shell { package_dir, .. } => {
                // `Microsoft.GamingApp_8wekyb3d8bbwe` -> package name `Microsoft.GamingApp`.
                Some(("appx", package_dir.split('_').next().unwrap_or_default().to_owned()))
            }
        };
        if let Some((kind, src)) = job {
            jobs.push(format!("$items += ,@({},{},{});", ps_str(kind), ps_str(&src), ps_str(&out.to_string_lossy())));
        }
    }
    if !jobs.is_empty() {
        let _ = platform::powershell(&format!("$items = @(); {} {}", jobs.concat(), ICON_SCRIPT));
    }
    found.iter().map(|d| Some(path(d)).filter(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned())).collect()
}

/// Every supported platform that is installed on this PC, with its real icon.
pub fn detect(data_dir: &Path) -> Vec<Platform> {
    let found: Vec<&Def> = DEFS.iter().filter(|d| installed(d)).collect();
    let icons = icons(&found, data_dir);
    found.iter().zip(icons).map(|(d, icon)| Platform { id: d.id.into(), name: d.name.into(), icon }).collect()
}

/// `true` when the window in front belongs to one of the installed platforms (Steam, Epic, Xbox...).
/// Games installed by a platform live outside its own folder, so they don't count.
pub fn in_front() -> bool {
    platform::foreground_exes().iter().any(|exe| is_platform_exe(exe))
}

/// Whether an executable path belongs to a platform's own program (not to a game it installed).
fn is_platform_exe(exe: &str) -> bool {
    DEFS.iter().any(|d| matches_def(d, exe))
}

fn matches_def(d: &Def, exe: &str) -> bool {
    let lower = exe.to_lowercase();
    // Steam keeps its games under `steamapps`, itch.io under `itch\apps`.
    if lower.contains(r"\steamapps\") || lower.contains(r"\itch\apps\") {
        return false;
    }
    match &d.launch {
        Launch::Exe(_) => find_exe(d)
            .and_then(|e| e.parent().map(|p| p.to_string_lossy().into_owned()))
            .is_some_and(|root| crate::util::is_inside(exe, &root)),
        // The installed folder carries the version: `Microsoft.GamingApp_2611.1001.10.0_x64__8wekyb3d8bbwe`.
        Launch::Shell { package_dir, .. } => {
            let name = package_dir.split('_').next().unwrap_or_default().to_lowercase();
            lower.contains(&format!(r"\{name}_"))
        }
    }
}

/// Windows smaller than this (tray popups, helpers) don't count as the platform being open.
const MIN_WINDOW: (i32, i32) = (400, 300);

/// Incremented by every new session, so an older watcher stops when another platform is opened.
static SESSION: AtomicU64 = AtomicU64::new(0);

fn showing(d: &Def) -> bool {
    platform::visible_window_exes(MIN_WINDOW).iter().any(|e| matches_def(d, e))
}

/// Follows an opened platform: once its window is up, Lumen steps aside (if asked to), and when the
/// platform has no window left Lumen comes back to the front. It gives up if the user returns to
/// Lumen by hand or a game takes over, so it never pulls Lumen over something else.
pub fn watch(app: &AppHandle, id: &str, minimize: bool) {
    let me = SESSION.fetch_add(1, Ordering::SeqCst) + 1;
    let (app, id) = (app.clone(), id.to_owned());
    thread::spawn(move || session(&app, &id, minimize, me));
}

fn session(app: &AppHandle, id: &str, minimize: bool, me: u64) {
    let Some(def) = DEFS.iter().find(|d| d.id == id) else { return };
    let cancelled = || SESSION.load(Ordering::SeqCst) != me;
    let window = || app.get_webview_window("main");
    let tick = Duration::from_millis(600);

    // 1. Wait for the platform's window (an already running client answers quickly; a cold start takes longer).
    let deadline = Instant::now() + Duration::from_secs(45);
    while !showing(def) {
        if cancelled() || Instant::now() > deadline {
            return;
        }
        thread::sleep(tick);
    }
    if minimize {
        thread::sleep(Duration::from_millis(400));
        if cancelled() {
            return;
        }
        if let Some(w) = window() {
            let _ = w.minimize();
        }
    }

    // 2. Wait until the window is gone for a few checks in a row (it may flicker while it reloads).
    let mut gone = 0;
    loop {
        thread::sleep(tick);
        if cancelled() || lumen_in_front() || game_running(app) {
            return;
        }
        if showing(def) {
            gone = 0;
            continue;
        }
        gone += 1;
        if gone >= 3 {
            if let Some(w) = window() {
                platform::bring_to_front(&w);
            }
            return;
        }
    }
}

/// The user came back to Lumen on their own.
fn lumen_in_front() -> bool {
    let Ok(me) = std::env::current_exe() else { return false };
    let me = crate::util::norm_path(&me.to_string_lossy());
    platform::foreground_exes().iter().any(|e| crate::util::norm_path(e) == me)
}

/// A game started from Lumen is running, so Lumen must not take the screen back.
fn game_running(app: &AppHandle) -> bool {
    app.state::<AppState>().inner.lock().map(|s| s.running.is_some()).unwrap_or(false)
}

/// Starts a platform by its id. The path is looked up again here, never taken from the interface.
pub fn open(id: &str) -> Result<(), String> {
    let def = DEFS.iter().find(|d| d.id == id).ok_or(crate::t!("Unknown platform.", "Platformă necunoscută."))?;
    let missing = || crate::t!(format!("{} is not installed anymore.", def.name), format!("{} nu mai este instalat.", def.name));
    match &def.launch {
        Launch::Exe(_) => {
            let exe = find_exe(def).ok_or_else(missing)?;
            platform::run_exe(&exe.to_string_lossy(), &[])
        }
        Launch::Shell { aumid, .. } => {
            if !installed(def) {
                return Err(missing());
            }
            platform::open_uri(&format!(r"shell:AppsFolder\{aumid}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = DEFS.iter().map(|d| d.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), DEFS.len());
    }

    #[test]
    fn xbox_app_is_recognized_by_package_name() {
        let app = r"C:\Program Files\WindowsApps\Microsoft.GamingApp_2611.1001.10.0_x64__8wekyb3d8bbwe\XboxPcApp.exe";
        assert!(is_platform_exe(app));
        assert!(!is_platform_exe(r"C:\Windows\explorer.exe"));
        assert!(!is_platform_exe(r"C:\Program Files (x86)\Steam\steamapps\common\Game\game.exe"));
    }

    #[test]
    fn unknown_platform_is_rejected() {
        assert!(open("nope").is_err());
    }
}

#[cfg(all(test, windows))]
mod live {
    /// Manual check on a real PC: `cargo test live -- --ignored --nocapture`.
    /// Pass a window handle: `$env:HWND=<decimal handle>; cargo test live -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn window_belongs_to_platform() {
        let hwnd: isize = std::env::var("HWND").expect("set HWND").parse().unwrap();
        let exes = crate::platform::window_exes(hwnd);
        println!("exes={exes:?}");
        println!("platform={}", exes.iter().any(|e| super::is_platform_exe(e)));
    }

    /// Lists which platforms have a visible window right now.
    #[test]
    #[ignore]
    fn visible_platforms() {
        for e in crate::platform::visible_window_exes(super::MIN_WINDOW) {
            println!("window: {e}");
        }
        for d in super::DEFS {
            println!("{:10} showing={}", d.id, super::showing(d));
        }
    }

    #[test]
    #[ignore]
    fn detect_with_icons() {
        let dir = std::env::temp_dir().join("lumen-platform-test");
        for p in super::detect(&dir) {
            println!("{:12} {:24} icon={:?}", p.id, p.name, p.icon);
        }
    }
}
