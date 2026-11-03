//! Pornirea jocurilor și urmărirea lor până la închidere.

use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::model::{AppState, Game, Running};
use crate::{platform, store, util};

/// Cât așteptăm ca jocul să apară după lansare (Steam poate avea nevoie să pornească întâi).
const START_TIMEOUT: Duration = Duration::from_secs(180);

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
        let mut s = state.inner.lock().map_err(|_| "Starea aplicației este blocată.")?;
        if let Some(r) = &s.running {
            if r.id == id {
                return Ok(());
            }
            return Err("Rulează deja un alt joc.".into());
        }
        let game = s
            .games
            .iter()
            .find(|g| g.id == id)
            .cloned()
            .ok_or("Jocul nu mai există în bibliotecă.")?;
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
        return Err(format!("Jocul nu a putut fi pornit: {e}"));
    }

    let app = app.clone();
    thread::spawn(move || watch(app, game, minimize));
    Ok(())
}

/// Renunță la o lansare care încă nu a fost detectată.
pub fn cancel(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Ok(mut s) = state.inner.lock() {
        if s.running.as_ref().is_some_and(|r| !r.started) {
            s.running = None;
        }
    };
}

/// Închide forțat toate procesele jocului care rulează. Dacă lansarea încă nu fusese
/// detectată, renunță și la ea și întoarce `true`: interfața își curăță singură starea,
/// pentru că `watch` nu mai trimite `game-exited` pentru o lansare anulată.
pub fn close_running(app: &AppHandle) -> Result<bool, String> {
    let state = app.state::<AppState>();
    let (dir, pending) = {
        let mut s = state.inner.lock().map_err(|_| "Starea aplicației este blocată.")?;
        let r = s.running.as_ref().ok_or("Niciun joc nu rulează.")?;
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

/// Procesele jocului puse în pauză cât meniul Lumen e deschis peste el, cu momentul pauzei.
static PAUSED: Mutex<Option<(Vec<u32>, Instant)>> = Mutex::new(None);

/// Cât după pauză ignorăm pierderea focusului: aducerea ferestrei în față o poate zgudui.
const PAUSE_SETTLE: Duration = Duration::from_millis(800);

/// Îngheață jocul care rulează, ca apăsările din meniul Lumen să nu ajungă și în el.
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

/// Reia jocul pus în pauză de `pause`.
pub fn unpause() {
    let Ok(mut paused) = PAUSED.lock() else { return };
    if let Some((pids, _)) = paused.take() {
        for pid in pids {
            platform::set_suspended(pid, false);
        }
    }
}

/// Lumen a pierdut focusul (alt-tab, jocul adus în față): jocul nu mai are de ce să stea înghețat.
pub fn unpause_after_blur() {
    let settled = PAUSED.lock().is_ok_and(|p| p.as_ref().is_some_and(|(_, at)| at.elapsed() > PAUSE_SETTLE));
    if settled {
        unpause();
    }
}

/// Readuce jocul care rulează în prim-plan și minimizează Lumen.
pub fn resume(app: &AppHandle) -> Result<(), String> {
    unpause();
    let state = app.state::<AppState>();
    let dir = {
        let s = state.inner.lock().map_err(|_| "Starea aplicației este blocată.")?;
        let r = s.running.as_ref().filter(|r| r.started).ok_or("Niciun joc nu rulează.")?;
        s.games.iter().find(|g| g.id == r.id).map(|g| g.install_dir.clone()).ok_or("Jocul nu a fost găsit.")?
    };
    if !platform::focus_window_of(&platform::procs_under(&dir)) {
        return Err("Fereastra jocului nu a fost găsită.".into());
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
    // 1. Așteptăm să apară un proces pornit din folderul jocului.
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        thread::sleep(Duration::from_millis(1500));
        if !is_current(&app, &game.id) {
            return; // lansare anulată
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
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.minimize();
        }
    }

    // 2. Jocul rulează. Considerăm că s-a închis după 3 verificări la rând fără
    //    procese (unele jocuri trec printr-un launcher și își schimbă executabilul).
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
    unpause(); // procesele au dispărut; nu păstrăm PID-uri care pot fi refolosite

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
