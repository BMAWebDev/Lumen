//! Launching games and tracking them until they close.

use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::model::{AppState, Game, Running};
use crate::{platform, store, util};

/// How long we wait for the game to show up after launch (Steam may need to start first).
const START_TIMEOUT: Duration = Duration::from_secs(180);

/// How long we wait for the game window once its process appears, before minimizing anyway.
const WINDOW_TIMEOUT: Duration = Duration::from_secs(60);

/// The minimum size of a window that counts as the game window (not a splash or launcher).
const GAME_WINDOW_MIN: (i32, i32) = (640, 400);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameEvent {
    pub id: String,
    pub seconds: u64,
    pub detected: bool,
}

pub fn launch(app: &AppHandle, id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let (game, minimize) = {
        let mut s = state.inner.lock().map_err(|_| util::state_locked())?;
        if let Some(r) = &s.running {
            if r.id == id {
                return Ok(());
            }
            return Err(crate::t!("Another game is already running.", "Rulează deja un alt joc.").into());
        }
        let game = s
            .games
            .iter()
            .find(|g| g.id == id)
            .cloned()
            .ok_or(crate::t!("The game is no longer in the library.", "Jocul nu mai există în bibliotecă."))?;
        s.running = Some(Running { id: id.to_string(), started: false });
        let stat = s.stats.entry(id.to_string()).or_default();
        stat.last_played = util::now();
        stat.launches += 1;
        store::save(&s.data_dir, "stats.json", &s.stats);
        (game, s.settings.minimize_on_launch)
    };

    let started = if game.source == "local" {
        platform::run_exe(&game.launch_uri, &game.launch_args)
    } else {
        platform::open_uri(&game.launch_uri)
    };
    if let Err(e) = started {
        clear(app, id);
        return Err(crate::t!(format!("The game could not be started: {e}"), format!("Jocul nu a putut fi pornit: {e}")));
    }

    let app = app.clone();
    thread::spawn(move || watch(app, game, minimize));
    Ok(())
}

/// Gives up on a launch that has not been detected yet.
pub fn cancel(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Ok(mut s) = state.inner.lock() {
        if s.running.as_ref().is_some_and(|r| !r.started) {
            s.running = None;
        }
    };
}

/// Force-closes all the processes of the running game. If the launch had not been
/// detected yet, it gives up on it too and returns `true`: the interface clears its own
/// state, because `watch` no longer sends `game-exited` for a cancelled launch.
pub fn close_running(app: &AppHandle) -> Result<bool, String> {
    let state = app.state::<AppState>();
    let (dir, pending) = {
        let mut s = state.inner.lock().map_err(|_| util::state_locked())?;
        let r = s.running.as_ref().ok_or(crate::t!("No game is running.", "Niciun joc nu rulează."))?;
        let pending = !r.started;
        let dir = s.games.iter().find(|g| g.id == r.id).map(|g| g.install_dir.clone());
        if pending || dir.is_none() {
            s.running = None;
        }
        (dir, pending)
    };
    unpause();
    if let Some(dir) = &dir {
        for pid in platform::procs_under(dir) {
            platform::kill(pid);
        }
    }
    Ok(pending || dir.is_none())
}

/// The game processes paused while the Lumen menu is open over it, with the time of the pause.
static PAUSED: Mutex<Option<(Vec<u32>, Instant)>> = Mutex::new(None);

/// For how long after the pause we ignore losing focus: bringing the window to the front can shake it.
const PAUSE_SETTLE: Duration = Duration::from_millis(800);

/// Freezes the running game, so presses in the Lumen menu don't reach it too.
pub fn pause(app: &AppHandle) {
    let state = app.state::<AppState>();
    let dir = {
        let Ok(s) = state.inner.lock() else { return };
        if !s.settings.pause_game {
            return;
        }
        let Some(r) = s.running.as_ref().filter(|r| r.started) else { return };
        s.games.iter().find(|g| g.id == r.id).map(|g| g.install_dir.clone())
    };
    let Some(dir) = dir else { return };
    let Ok(mut paused) = PAUSED.lock() else { return };
    if paused.is_some() {
        return;
    }
    let pids: Vec<u32> = platform::procs_under(&dir).into_iter().filter(|&p| platform::set_suspended(p, true)).collect();
    if !pids.is_empty() {
        *paused = Some((pids, Instant::now()));
    }
}

/// Resumes the game paused by `pause`.
pub fn unpause() {
    let Ok(mut paused) = PAUSED.lock() else { return };
    if let Some((pids, _)) = paused.take() {
        for pid in pids {
            platform::set_suspended(pid, false);
        }
    }
}

/// Lumen lost focus (alt-tab, the game was brought to the front): the game has no reason to stay frozen.
pub fn unpause_after_blur() {
    let settled = PAUSED.lock().is_ok_and(|p| p.as_ref().is_some_and(|(_, at)| at.elapsed() > PAUSE_SETTLE));
    if settled {
        unpause();
    }
}

/// Brings the running game back to the foreground and minimizes Lumen.
pub fn resume(app: &AppHandle) -> Result<(), String> {
    unpause();
    let state = app.state::<AppState>();
    let dir = {
        let s = state.inner.lock().map_err(|_| util::state_locked())?;
        let r = s.running.as_ref().filter(|r| r.started).ok_or(crate::t!("No game is running.", "Niciun joc nu rulează."))?;
        s.games.iter().find(|g| g.id == r.id).map(|g| g.install_dir.clone()).ok_or(crate::t!("The game was not found.", "Jocul nu a fost găsit."))?
    };
    if !platform::focus_window_of(&platform::procs_under(&dir)) {
        return Err(crate::t!("The game window was not found.", "Fereastra jocului nu a fost găsită.").into());
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.minimize();
    }
    Ok(())
}

fn is_current(app: &AppHandle, id: &str) -> bool {
    let state = app.state::<AppState>();
    let s = state.inner.lock();
    s.map(|s| s.running.as_ref().is_some_and(|r| r.id == id)).unwrap_or(false)
}

fn clear(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    if let Ok(mut s) = state.inner.lock() {
        if s.running.as_ref().is_some_and(|r| r.id == id) {
            s.running = None;
        }
    };
}

fn watch(app: AppHandle, game: Game, minimize: bool) {
    // 1. We wait for a process started from the game folder to appear.
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        thread::sleep(Duration::from_millis(1500));
        if !is_current(&app, &game.id) {
            return; // launch cancelled
        }
        if !platform::procs_under(&game.install_dir).is_empty() {
            break;
        }
        if Instant::now() > deadline {
            clear(&app, &game.id);
            let _ = app.emit("game-exited", GameEvent { id: game.id.clone(), seconds: 0, detected: false });
            return;
        }
    }

    // Lumen stays on screen while the game loads and only minimizes once the game has a
    // window, brought to the front first; otherwise the desktop would show in between.
    let mut covered = false;
    if minimize {
        let window_deadline = Instant::now() + WINDOW_TIMEOUT;
        loop {
            if !is_current(&app, &game.id) {
                return; // launch cancelled
            }
            if platform::focus_window(&platform::procs_under(&game.install_dir), GAME_WINDOW_MIN) {
                covered = true;
                break;
            }
            if Instant::now() > window_deadline {
                break;
            }
            thread::sleep(Duration::from_millis(400));
        }
    }

    {
        let state = app.state::<AppState>();
        if let Ok(mut s) = state.inner.lock() {
            if let Some(r) = s.running.as_mut() {
                r.started = true;
            }
        };
    }
    let _ = app.emit("game-started", GameEvent { id: game.id.clone(), seconds: 0, detected: true });
    if minimize {
        if covered {
            thread::sleep(Duration::from_millis(400)); // let the game take over the screen
        }
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.minimize();
        }
    }

    // 2. The game is running. We consider it closed after 3 checks in a row without
    //    processes (some games go through a launcher and change their executable).
    let start = Instant::now();
    let mut misses = 0;
    loop {
        thread::sleep(Duration::from_secs(2));
        if platform::procs_under(&game.install_dir).is_empty() {
            misses += 1;
            if misses >= 3 {
                break;
            }
        } else {
            misses = 0;
        }
    }
    let seconds = start.elapsed().as_secs().saturating_sub(4);
    unpause(); // the processes are gone; we don't keep PIDs that may be reused

    {
        let state = app.state::<AppState>();
        if let Ok(mut s) = state.inner.lock() {
            let stat = s.stats.entry(game.id.clone()).or_default();
            stat.playtime_sec += seconds;
            stat.last_played = util::now();
            store::save(&s.data_dir, "stats.json", &s.stats);
        };
    }
    clear(&app, &game.id);
    let _ = app.emit("game-exited", GameEvent { id: game.id.clone(), seconds, detected: true });
    if let Some(w) = app.get_webview_window("main") {
        platform::bring_to_front(&w);
    };
}
