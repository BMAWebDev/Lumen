#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art;
mod audio;
mod guide;
mod launcher;
mod local;
mod model;
mod pads;
mod platform;
mod steam;
mod store;
mod uninstall;
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

use model::{AppState, Game, HiddenGame, Inner, Running, Settings, Stat};

const THEME_README_EN: &str = "Put files with these names here to customize Lumen:\r\n\
\r\n\
  move.wav / confirm.wav / back.wav / open.wav / launch.wav   interface sounds (wav, ogg or mp3)\r\n\
  music.mp3                                                   looping background music (mp3, ogg, wav, m4a)\r\n\
  font.ttf                                                    the interface font (ttf, otf, woff, woff2)\r\n\
  wallpaper.jpg                                               default background (jpg, png, webp)\r\n\
\r\n\
Restart Lumen after adding or changing files.\r\n";

const THEME_README_RO: &str = "Pune aici fișiere cu aceste nume ca să personalizezi Lumen:\r\n\
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
    util::state_locked().into()
}

/// The theme folder's README, in the language of the interface. It is written when missing and
/// refreshed when the language changes, but only while it still holds one of our own texts.
fn write_theme_readme(dir: &Path) {
    let (want, other) = if util::is_ro() { (THEME_README_RO, THEME_README_EN) } else { (THEME_README_EN, THEME_README_RO) };
    let path = dir.join("README.txt");
    match fs::read_to_string(&path) {
        Ok(text) if text != other => {}
        _ => {
            let _ = fs::write(&path, want);
        }
    }
    // Older versions called it CITESTE.txt; drop it if the user never edited it.
    let legacy = dir.join("CITESTE.txt");
    if fs::read_to_string(&legacy).is_ok_and(|t| t == THEME_README_RO) {
        let _ = fs::remove_file(&legacy);
    }
}

fn theme_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("theme")
}

/// The recognized theme files, by base name.
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
// Commands called from the interface
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_state(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let s = state.inner.lock().map_err(lock_err)?;
    let settings = s.settings.clone();
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

/// The interface is ready: we show the window (it starts hidden so it doesn't flash white).
#[tauri::command]
fn app_ready(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    };
}

/// Scans the libraries in the background and sends the `library` event.
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
        let Ok((online, folders, data_dir)) = state
            .inner
            .lock()
            .map(|s| (s.settings.online_art, s.settings.game_folders.clone(), s.data_dir.clone()))
        else {
            return;
        };
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
        // Manually added games take priority over the ones found in the same folder.
        let manual: Vec<Game> = store::load(&data_dir, "manual.json");
        let manual: Vec<Game> = manual.into_iter().filter(|g| Path::new(&g.launch_uri).is_file()).collect();
        let taken: Vec<String> = manual.iter().map(|g| util::norm_path(&g.install_dir)).collect();
        games.extend(local::scan(&folders).into_iter().filter(|g| !taken.contains(&util::norm_path(&g.install_dir))));
        games.extend(manual);
        let hidden: Vec<HiddenGame> = store::load(&data_dir, "hidden.json");
        games.retain(|g| !hidden.iter().any(|h| h.id == g.id));
        art::decorate(&mut games, &data_dir, online);
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
fn resume_game(app: AppHandle) -> Result<(), String> {
    launcher::resume(&app)
}

#[tauri::command]
fn close_game(app: AppHandle) -> Result<bool, String> {
    launcher::close_running(&app)
}

#[tauri::command(async)]
fn set_setting(state: State<'_, AppState>, key: String, value: bool) -> Result<Settings, String> {
    if key == "autostart" {
        platform::set_autostart(value)?;
    }
    {
        let mut s = state.inner.lock().map_err(lock_err)?;
        match key.as_str() {
            "autostart" => s.settings.autostart = value,
            "minimizeOnLaunch" => s.settings.minimize_on_launch = value,
            "pauseGame" => s.settings.pause_game = value,
            "sounds" => s.settings.sounds = value,
            "music" => s.settings.music = value,
            "onlineArt" => s.settings.online_art = value,
            "reduceMotion" => s.settings.reduce_motion = value,
            _ => return Err(t!(format!("Unknown setting: {key}"), format!("Setare necunoscută: {key}"))),
        }
        store::save(&s.data_dir, "settings.json", &s.settings);
        Ok(s.settings.clone())
    }
}

/// Changes the interface language (also the language of the backend's own messages) and saves it.
#[tauri::command]
fn set_language(app: AppHandle, state: State<'_, AppState>, language: String) -> Result<Settings, String> {
    if !util::LANGUAGES.contains(&language.as_str()) {
        return Err(t!(format!("Unknown language: {language}"), format!("Limbă necunoscută: {language}")));
    }
    util::set_lang(&language);
    let mut s = state.inner.lock().map_err(lock_err)?;
    s.settings.language = language;
    store::save(&s.data_dir, "settings.json", &s.settings);
    write_theme_readme(&theme_dir(&s.data_dir));
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title("Lumen");
    }
    Ok(s.settings.clone())
}

/// Picks a games folder (the system dialog) and adds it to the settings.
/// Returns `None` if the user cancelled.
#[tauri::command(async)]
fn add_game_folder(app: AppHandle) -> Result<Option<Settings>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app.dialog().file().set_title(t!("Choose the games folder", "Alege folderul cu jocuri"));
    if let Some(w) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&w);
    }
    let Some(picked) = dialog.blocking_pick_folder() else { return Ok(None) };
    let path = picked.into_path().map_err(|e| e.to_string())?.to_string_lossy().into_owned();
    let state = app.state::<AppState>();
    let mut s = state.inner.lock().map_err(lock_err)?;
    let key = util::norm_path(&path);
    if !s.settings.game_folders.iter().any(|f| util::norm_path(f) == key) {
        s.settings.game_folders.push(path);
        store::save(&s.data_dir, "settings.json", &s.settings);
    }
    Ok(Some(s.settings.clone()))
}

/// Picks an executable and adds it to the library. Returns the game name,
/// or `None` if the user cancelled; the library is refreshed separately.
#[tauri::command(async)]
fn add_game(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app.dialog().file().set_title(t!("Choose the game executable", "Alege executabilul jocului")).add_filter(t!("Games", "Jocuri"), &["exe"]);
    if let Some(w) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&w);
    }
    let Some(picked) = dialog.blocking_pick_file() else { return Ok(None) };
    let exe = picked.into_path().map_err(|e| e.to_string())?;
    let game = local::manual_game(&exe)?;
    let name = game.name.clone();
    let state = app.state::<AppState>();
    let s = state.inner.lock().map_err(lock_err)?;
    let mut manual: Vec<Game> = store::load(&s.data_dir, "manual.json");
    if manual.iter().any(|g| g.id == game.id) {
        return Err(t!(format!("{name} is already in the library."), format!("{name} e deja în bibliotecă.")));
    }
    manual.push(game);
    store::save(&s.data_dir, "manual.json", &manual);
    Ok(Some(name))
}

/// Removes a game from the library.
#[tauri::command]
fn remove_game(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut s = state.inner.lock().map_err(lock_err)?;
    if s.running.as_ref().is_some_and(|r| r.id == id) {
        return Err(t!("Close the game before removing it.", "Închide jocul înainte să-l elimini.").into());
    }
    let name = s.games.iter().find(|g| g.id == id).map(|g| g.name.clone()).ok_or(t!("The game is no longer in the library.", "Jocul nu mai există în bibliotecă."))?;
    if local::is_manual(&id) {
        // Manually added games disappear completely; the rest are only hidden, so the scan doesn't bring them back.
        let mut manual: Vec<Game> = store::load(&s.data_dir, "manual.json");
        manual.retain(|g| g.id != id);
        store::save(&s.data_dir, "manual.json", &manual);
    } else {
        let mut hidden: Vec<HiddenGame> = store::load(&s.data_dir, "hidden.json");
        if !hidden.iter().any(|h| h.id == id) {
            hidden.push(HiddenGame { id: id.clone(), name });
            store::save(&s.data_dir, "hidden.json", &hidden);
        }
    }
    s.games.retain(|g| g.id != id);
    store::save(&s.data_dir, "games.json", &s.games);
    Ok(())
}

/// Uninstalls a game. Xbox runs `Remove-AppxPackage` (a few seconds), so it stays off the main thread.
#[tauri::command(async)]
fn uninstall_game(state: State<'_, AppState>, id: String) -> Result<uninstall::Outcome, String> {
    let game = {
        let s = state.inner.lock().map_err(lock_err)?;
        if s.running.as_ref().is_some_and(|r| r.id == id) {
            return Err(t!("Close the game before uninstalling it.", "Închide jocul înainte să-l dezinstalezi.").into());
        }
        s.games.iter().find(|g| g.id == id).cloned().ok_or(t!("The game is no longer in the library.", "Jocul nu mai există în bibliotecă."))?
    };
    uninstall::run(&game)
}

#[tauri::command]
fn hidden_games(state: State<'_, AppState>) -> Result<Vec<HiddenGame>, String> {
    let s = state.inner.lock().map_err(lock_err)?;
    Ok(store::load(&s.data_dir, "hidden.json"))
}

/// Brings a hidden game back to the library; it shows up on the next scan.
#[tauri::command]
fn restore_game(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let s = state.inner.lock().map_err(lock_err)?;
    let mut hidden: Vec<HiddenGame> = store::load(&s.data_dir, "hidden.json");
    hidden.retain(|h| h.id != id);
    store::save(&s.data_dir, "hidden.json", &hidden);
    Ok(())
}

#[tauri::command]
fn remove_game_folder(state: State<'_, AppState>, path: String) -> Result<Settings, String> {
    let mut s = state.inner.lock().map_err(lock_err)?;
    s.settings.game_folders.retain(|f| f != &path);
    store::save(&s.data_dir, "settings.json", &s.settings);
    Ok(s.settings.clone())
}

/// Opens a folder in Explorer (a game's location or the theme folder).
#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.is_dir() {
        return Err(t!("The folder does not exist.", "Folderul nu există.").into());
    }
    platform::open_uri(&path)
}

/// The contents of a theme file (font, sound), as binary data.
#[tauri::command]
fn theme_file(state: State<'_, AppState>, name: String) -> Result<tauri::ipc::Response, String> {
    let dir = {
        let s = state.inner.lock().map_err(lock_err)?;
        theme_dir(&s.data_dir)
    };
    let files = theme_files(&dir);
    let path = files.get(&name).ok_or(t!("The theme file does not exist.", "Fișierul de temă nu există."))?;
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
fn battery() -> Option<platform::Battery> {
    platform::battery()
}

/// Reading HID can take a few hundred ms, so it runs off the main thread.
#[tauri::command(async)]
fn pad_batteries() -> Vec<pads::PadBattery> {
    pads::batteries()
}

/// COM enumeration + compiling C# from PowerShell takes ~1 s, so it runs off the main thread.
#[tauri::command(async)]
fn audio_outputs() -> Result<Vec<audio::AudioDevice>, String> {
    audio::outputs()
}

#[tauri::command(async)]
fn set_audio_output(id: String) -> Result<(), String> {
    audio::set_output(&id)
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

/// The autostart made by old versions (the `Run` key) becomes a logon task, which starts
/// without the delay Windows applies to applications in `Run`.
fn migrate_autostart(app: AppHandle) {
    // A development build would register its own executable (with a console), not the installed one.
    if cfg!(debug_assertions) {
        return;
    }
    thread::spawn(move || {
        if !platform::legacy_autostart_enabled() || platform::set_autostart(true).is_err() {
            return;
        }
        let state = app.state::<AppState>();
        if let Ok(mut s) = state.inner.lock() {
            s.settings.autostart = true;
            store::save(&s.data_dir, "settings.json", &s.settings);
        };
    });
}

/// A mutex held by Lumen while it runs; the listener uses it to stay out of the way of
/// the Guide button, which opens the menu from inside the application.
const APP_MUTEX: &str = r"Local\Lumen.App";
const LISTENER_MUTEX: &str = r"Local\Lumen.Listener";

/// A tiny process (no window) that stays after Lumen closes and starts the
/// application when you hold PS + Options. While Lumen is running it does nothing.
fn listen() {
    if !platform::claim_mutex(LISTENER_MUTEX) {
        return;
    }
    pads::watch_combo(|| {
        if platform::mutex_exists(APP_MUTEX) {
            return;
        }
        if let Ok(exe) = std::env::current_exe() {
            let _ = platform::hidden(&exe.to_string_lossy()).spawn();
        }
    });
}

/// A normally started Lumen makes sure the listener is running, so it can be restarted after exit.
fn ensure_listener() {
    // A development build would start the executable from `target`, not the installed one.
    if cfg!(debug_assertions) || platform::mutex_exists(LISTENER_MUTEX) {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = platform::hidden(&exe.to_string_lossy()).arg("--listen").spawn();
    }
}

fn main() {
    if std::env::args().any(|a| a == "--listen") {
        return listen();
    }
    platform::claim_mutex(APP_MUTEX);
    ensure_listener();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            fs::create_dir_all(&data_dir)?;
            let tdir = theme_dir(&data_dir);
            let mut settings: Settings = store::load(&data_dir, "settings.json");
            if !util::LANGUAGES.contains(&settings.language.as_str()) {
                settings.language = "en".into();
            }
            util::set_lang(&settings.language);
            if fs::create_dir_all(&tdir).is_ok() {
                write_theme_readme(&tdir);
            }
            let inner = Inner {
                games: store::load(&data_dir, "games.json"),
                stats: store::load(&data_dir, "stats.json"),
                settings,
                running: None,
                scanning: false,
                data_dir,
            };
            app.manage(AppState { inner: Mutex::new(inner) });
            guide::start(app.handle().clone());
            migrate_autostart(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            app_ready,
            scan_library,
            launch_game,
            cancel_launch,
            close_game,
            resume_game,
            set_setting,
            set_language,
            add_game_folder,
            remove_game_folder,
            add_game,
            remove_game,
            uninstall_game,
            hidden_games,
            restore_game,
            open_path,
            theme_file,
            battery,
            pad_batteries,
            audio_outputs,
            set_audio_output,
            power,
            exit_app
        ])
        .on_window_event(|_, event| {
            if let tauri::WindowEvent::Focused(false) = event {
                launcher::unpause_after_blur();
            }
        })
        .build(tauri::generate_context!())
        .expect("Lumen could not start")
        .run(|_, event| {
            // A game left frozen after Lumen exits could never be resumed.
            if let tauri::RunEvent::Exit = event {
                launcher::unpause();
            }
        });
}
