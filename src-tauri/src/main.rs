#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod guide;
mod launcher;
mod model;
mod platform;
mod steam;
mod store;
mod util;
mod vdf;
mod xbox;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use model::{AppState, Game, Inner, Running, Settings, Stat};

const THEME_README: &str = "Pune aici fișiere cu aceste nume ca să personalizezi Lumen:\r\n\
\r\n\
  move.wav / confirm.wav / back.wav / open.wav / launch.wav   sunetele interfeței (wav, ogg sau mp3)\r\n\
  music.mp3                                                   muzică de fundal în buclă (mp3, ogg, wav, m4a)\r\n\
  font.ttf                                                    fontul interfeței (ttf, otf, woff, woff2)\r\n\
  wallpaper.jpg                                               fundal implicit (jpg, png, webp)\r\n\
\r\n\
Repornește Lumen după ce adaugi sau schimbi fișiere.\r\n";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    games: Vec<Game>,
    stats: HashMap<String, Stat>,
    settings: Settings,
    theme: HashMap<String, String>,
    theme_dir: String,
    running: Option<Running>,
    user: String,
    version: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryEvent {
    games: Vec<Game>,
    warnings: Vec<String>,
}

fn lock_err<T>(_: T) -> String {
    "Starea aplicației este blocată.".into()
}

fn theme_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("theme")
}

/// Fișierele de temă recunoscute, după numele de bază.
fn theme_files(dir: &Path) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let Ok(rd) = fs::read_dir(dir) else { return m };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        let ext = p.extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        let ok = match stem.as_str() {
            "move" | "confirm" | "back" | "open" | "launch" => matches!(ext.as_str(), "wav" | "ogg" | "mp3"),
            "music" => matches!(ext.as_str(), "mp3" | "ogg" | "wav" | "m4a"),
            "font" => matches!(ext.as_str(), "ttf" | "otf" | "woff" | "woff2"),
            "wallpaper" => matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp"),
            _ => false,
        };
        if ok {
            m.insert(stem, p.to_string_lossy().into_owned());
        }
    }
    m
}

// ---------------------------------------------------------------------------
// Comenzi apelate din interfață
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_state(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let s = state.inner.lock().map_err(lock_err)?;
    let mut settings = s.settings.clone();
    settings.autostart = platform::autostart_enabled();
    let tdir = theme_dir(&s.data_dir);
    Ok(Snapshot {
        games: s.games.clone(),
        stats: s.stats.clone(),
        settings,
        theme: theme_files(&tdir),
        theme_dir: tdir.to_string_lossy().into_owned(),
        running: s.running.clone(),
        user: std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

/// Interfața e gata: arătăm fereastra (pornește ascunsă ca să nu clipească alb).
#[tauri::command]
fn app_ready(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    };
}

/// Scanează bibliotecile în fundal și trimite evenimentul `library`.
#[tauri::command]
fn scan_library(app: AppHandle) {
    {
        let state = app.state::<AppState>();
        let Ok(mut s) = state.inner.lock() else { return };
        if s.scanning {
            return;
        }
        s.scanning = true;
    }
    thread::spawn(move || {
        let state = app.state::<AppState>();
        let online = state.inner.lock().map(|s| s.settings.online_art).unwrap_or(true);
        let mut games = Vec::new();
        let mut warnings = Vec::new();
        match steam::scan(online) {
            Ok(g) => games.extend(g),
            Err(e) => warnings.push(e),
        }
        match xbox::scan() {
            Ok(g) => games.extend(g),
            Err(e) => warnings.push(e),
        }
        games.sort_by_key(|g| g.name.to_lowercase());
        if let Ok(mut s) = state.inner.lock() {
            s.games = games.clone();
            s.scanning = false;
            store::save(&s.data_dir, "games.json", &s.games);
        };
        let _ = app.emit("library", LibraryEvent { games, warnings });
    });
}

#[tauri::command]
fn launch_game(app: AppHandle, id: String) -> Result<(), String> {
    launcher::launch(&app, &id)
}

#[tauri::command]
fn cancel_launch(app: AppHandle) {
    launcher::cancel(&app);
}

#[tauri::command]
fn close_game(app: AppHandle) -> Result<(), String> {
    launcher::close_running(&app)
}

#[tauri::command]
fn set_setting(state: State<'_, AppState>, key: String, value: bool) -> Result<Settings, String> {
    if key == "autostart" {
        platform::set_autostart(value)?;
    } else {
        let mut s = state.inner.lock().map_err(lock_err)?;
        match key.as_str() {
            "minimizeOnLaunch" => s.settings.minimize_on_launch = value,
            "sounds" => s.settings.sounds = value,
            "music" => s.settings.music = value,
            "onlineArt" => s.settings.online_art = value,
            "reduceMotion" => s.settings.reduce_motion = value,
            _ => return Err(format!("Setare necunoscută: {key}")),
        }
        store::save(&s.data_dir, "settings.json", &s.settings);
    }
    let s = state.inner.lock().map_err(lock_err)?;
    let mut settings = s.settings.clone();
    settings.autostart = platform::autostart_enabled();
    Ok(settings)
}

/// Deschide un folder în Explorer (locația unui joc sau folderul temei).
#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.is_dir() {
        return Err("Folderul nu există.".into());
    }
    platform::open_uri(&path)
}

/// Conținutul unui fișier de temă (font, sunet), ca date binare.
#[tauri::command]
fn theme_file(state: State<'_, AppState>, name: String) -> Result<tauri::ipc::Response, String> {
    let dir = {
        let s = state.inner.lock().map_err(lock_err)?;
        theme_dir(&s.data_dir)
    };
    let files = theme_files(&dir);
    let path = files.get(&name).ok_or("Fișierul de temă nu există.")?;
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
fn power(action: String) {
    thread::spawn(move || {
        let _ = platform::power(&action);
    });
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            fs::create_dir_all(&data_dir)?;
            let tdir = theme_dir(&data_dir);
            if fs::create_dir_all(&tdir).is_ok() && !tdir.join("CITESTE.txt").exists() {
                let _ = fs::write(tdir.join("CITESTE.txt"), THEME_README);
            }
            let inner = Inner {
                games: store::load(&data_dir, "games.json"),
                stats: store::load(&data_dir, "stats.json"),
                settings: store::load(&data_dir, "settings.json"),
                running: None,
                scanning: false,
                data_dir,
            };
            app.manage(AppState { inner: Mutex::new(inner) });
            guide::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            app_ready,
            scan_library,
            launch_game,
            cancel_launch,
            close_game,
            set_setting,
            open_path,
            theme_file,
            power,
            exit_app
        ])
        .run(tauri::generate_context!())
        .expect("Lumen nu a putut porni");
}
