//! Jocuri instalate separat, în afara Steam și Xbox (de ex. `D:\Jocuri`).
//!
//! Fiecare subfolder al unui folder ales în Setări e considerat un joc. Alegem
//! executabilul principal după nume (seamănă cu numele folderului) și mărime,
//! ignorând dezinstalatoarele, redistribuabilele și utilitarele de raportare.

use std::fs;
use std::path::{Path, PathBuf};

use crate::model::Game;
use crate::util;

/// Fragmente de nume care indică un executabil auxiliar, nu jocul.
const SKIP_EXE: &[&str] = &[
    "unins", "uninstall", "setup", "install", "redist", "dxsetup", "crash", "report", "helper",
    "update", "patch", "prereq", "dotnet", "easyanticheat", "eac_", "battleye", "be_service",
    "cefprocess", "webhelper", "touchup", "cleanup", "config", "settings", "dxwebsetup",
];

/// Subfoldere care conțin doar instalatoare sau dependențe.
const SKIP_DIR: &[&str] = &[
    "_uninstall", "redist", "_redist", "vcredist", "directx", "installerresources", "__installer",
    "_commonredist", "commonredist", "prerequisites", "support", "dotnet", "easyanticheat", "battleye",
];

/// Produsele Riot pornesc prin Riot Client, nu direct din executabil.
const RIOT: &[(&str, &str, &str)] = &[
    ("League of Legends", "league_of_legends", "League of Legends"),
    ("VALORANT", "valorant", "VALORANT"),
    ("LoR", "bacon", "Legends of Runeterra"),
];

pub fn scan(folders: &[String]) -> Vec<Game> {
    let mut games = Vec::new();
    for root in folders {
        let Ok(rd) = fs::read_dir(root) else { continue };
        for e in rd.flatten() {
            let dir = e.path();
            if !dir.is_dir() {
                continue;
            }
            // Jocurile Xbox instalate aici le importă deja `xbox.rs`.
            if has_file(&dir, "MicrosoftGame.config", 2) {
                continue;
            }
            if let Some(riot) = riot(&dir) {
                games.extend(riot);
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(exe) = pick_exe(&dir, &name) {
                games.push(game(&dir, &name, &exe, Vec::new()));
            }
        }
    }
    games
}

fn game(dir: &Path, name: &str, exe: &Path, args: Vec<String>) -> Game {
    let install = dir.to_string_lossy().into_owned();
    Game {
        id: format!("local:{}", util::norm_path(&install)),
        source: "local".into(),
        name: name.to_string(),
        launch_uri: exe.to_string_lossy().into_owned(),
        launch_args: args,
        icon: None,
        size_bytes: dir_size(dir),
        install_dir: install,
        cover: Vec::new(),
        hero: Vec::new(),
        logo: Vec::new(),
    }
}

fn riot(dir: &Path) -> Option<Vec<Game>> {
    let client = dir.join("Riot Client").join("RiotClientServices.exe");
    if !client.is_file() {
        return None;
    }
    let games = RIOT
        .iter()
        .filter(|(sub, _, _)| dir.join(sub).is_dir())
        .map(|(sub, product, name)| {
            let args = vec![format!("--launch-product={product}"), "--launch-patchline=live".into()];
            // Urmărim doar folderul produsului, ca Riot Client rămas deschis să nu conteze ca „jocul”.
            game(&dir.join(sub), name, &client, args)
        })
        .collect();
    Some(games)
}

/// Litere și cifre, cu litere mici: „Split Fiction” → „splitfiction”.
fn simplify(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn pick_exe(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut exes = Vec::new();
    collect_exes(dir, 4, &mut exes);
    let folder = simplify(name);
    exes.into_iter()
        .filter(|(p, _)| {
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            !SKIP_EXE.iter().any(|k| stem.contains(k))
        })
        .max_by_key(|(p, size)| {
            let stem = simplify(&p.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default());
            // Un nume care seamănă cu folderul bate mărimea: „SplitFiction.exe” în „Split Fiction”.
            let similar = stem.len() >= 3 && (folder.contains(&stem) || stem.contains(&folder));
            (similar, *size)
        })
        .map(|(p, _)| p)
}

fn collect_exes(dir: &Path, depth: u32, out: &mut Vec<(PathBuf, u64)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            let n = e.file_name().to_string_lossy().to_lowercase();
            if depth > 0 && !SKIP_DIR.contains(&n.as_str()) {
                collect_exes(&p, depth - 1, out);
            }
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
            out.push((p, e.metadata().map(|m| m.len()).unwrap_or(0)));
        }
    }
}

fn has_file(dir: &Path, name: &str, depth: u32) -> bool {
    if dir.join(name).is_file() {
        return true;
    }
    depth > 0
        && fs::read_dir(dir)
            .map(|rd| rd.flatten().any(|e| e.path().is_dir() && has_file(&e.path(), name, depth - 1)))
            .unwrap_or(false)
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

// ---------------------------------------------------------------------------
// Jocuri adăugate manual (un .exe ales din interfață)
// ---------------------------------------------------------------------------

/// Foldere intermediare care nu spun nimic despre numele jocului.
const GENERIC_DIRS: &[&str] = &[
    "bin", "bin64", "binaries", "win64", "win32", "x64", "x86", "game", "content", "retail", "shipping", "live",
];

pub fn is_manual(id: &str) -> bool {
    id.starts_with("manual:")
}

/// Creează un joc dintr-un executabil ales de utilizator. Numele vine din
/// metadatele executabilului, iar dacă lipsesc, din folderul jocului.
pub fn manual_game(exe: &Path) -> Result<Game, String> {
    if !exe.is_file() {
        return Err("Fișierul ales nu există.".into());
    }
    // Folderul jocului: urcăm peste `Binaries\Win64` și altele asemenea.
    let mut dir = exe.parent().ok_or("Executabil fără folder.")?;
    while let Some(parent) = dir.parent() {
        let n = dir.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !GENERIC_DIRS.contains(&n.as_str()) || parent.parent().is_none() {
            break;
        }
        dir = parent;
    }
    // Unreal Engine: `<Joc>\<Proiect>\Binaries\Win64`, cu `<Joc>\Engine` alături de proiect.
    if let Some(parent) = dir.parent().filter(|p| p.join("Engine").is_dir() && p.parent().is_some()) {
        dir = parent;
    }
    let folder = dir.file_name().map(|s| s.to_string_lossy().into_owned());
    let stem = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = crate::art::exe_title(exe).or(folder).unwrap_or(stem);
    let exe_s = exe.to_string_lossy().into_owned();
    Ok(Game {
        id: format!("manual:{}", util::norm_path(&exe_s)),
        source: "local".into(),
        name,
        launch_uri: exe_s,
        launch_args: Vec::new(),
        icon: None,
        size_bytes: dir_size(dir),
        install_dir: dir.to_string_lossy().into_owned(),
        cover: Vec::new(),
        hero: Vec::new(),
        logo: Vec::new(),
    })
}

/// Executabilul din care luăm iconița: cel lansat, sau, când jocul pornește printr-un
/// launcher din alt folder (Riot Client), cel mai probabil executabil din folderul jocului.
pub fn icon_source(g: &Game) -> Option<PathBuf> {
    let exe = PathBuf::from(&g.launch_uri);
    if util::is_inside(&g.launch_uri, &g.install_dir) {
        return Some(exe);
    }
    pick_exe(Path::new(&g.install_dir), &g.name).or(Some(exe))
}
