//! Importing the games installed through Steam.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::model::Game;
use crate::{platform, util, vdf};

/// Steam tools that are not games (redistributables, Proton, runtimes).
const EXCLUDED_IDS: &[&str] = &[
    "228980", "1070560", "1391110", "1628350", "1493710", "2180100", "2805730", "1826330",
    "961940", "1580130", "1887720", "2348590", "250820",
];

fn excluded(id: &str, name: &str) -> bool {
    EXCLUDED_IDS.contains(&id)
        || name.starts_with("Proton")
        || name.starts_with("Steam Linux Runtime")
        || name.starts_with("Steamworks")
        || name.contains("Redistributable")
        || name.eq_ignore_ascii_case("Spacewar")
}

#[derive(Clone, Copy)]
enum Art {
    Cover,
    Hero,
    Logo,
}

pub fn scan(online_art: bool) -> Result<Vec<Game>, String> {
    let root = platform::steam_root().ok_or(crate::t!("Steam is not installed on this PC.", "Steam nu este instalat pe acest PC."))?;

    let mut libs = vec![root.clone()];
    if let Ok(text) = fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        let v = vdf::parse(&text);
        if let Some(lf) = v.get("libraryfolders") {
            for (_, entry) in lf.entries() {
                if let Some(p) = entry.str("path") {
                    libs.push(PathBuf::from(p));
                }
            }
        }
    }
    let mut seen = HashSet::new();
    libs.retain(|p| seen.insert(util::norm_path(&p.to_string_lossy())));

    let grids = grid_dirs(&root);
    let cache = root.join("appcache").join("librarycache");
    let mut games = Vec::new();
    let mut ids = HashSet::new();

    for lib in &libs {
        let apps = lib.join("steamapps");
        let Ok(rd) = fs::read_dir(&apps) else { continue };
        for entry in rd.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if !(fname.starts_with("appmanifest_") && fname.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else { continue };
            let v = vdf::parse(&text);
            let Some(st) = v.get("AppState") else { continue };
            let (Some(id), Some(name), Some(dir)) = (st.str("appid"), st.str("name"), st.str("installdir")) else {
                continue;
            };
            if excluded(id, name) || !ids.insert(id.to_string()) {
                continue;
            }
            let install = apps.join("common").join(dir);
            if !install.exists() {
                continue;
            }
            let size = st.str("SizeOnDisk").and_then(|s| s.parse().ok()).unwrap_or(0);
            games.push(Game {
                id: format!("steam:{id}"),
                source: "steam".into(),
                name: name.to_string(),
                install_dir: install.to_string_lossy().into_owned(),
                launch_uri: format!("steam://rungameid/{id}"),
                launch_args: Vec::new(),
                icon: None,
                cover: art(id, &grids, &cache, Art::Cover, online_art),
                hero: art(id, &grids, &cache, Art::Hero, online_art),
                logo: art(id, &grids, &cache, Art::Logo, online_art),
                size_bytes: size,
            });
        }
    }
    Ok(games)
}

/// The folders with custom images (`userdata/<account>/config/grid`).
fn grid_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(root.join("userdata")) {
        for e in rd.flatten() {
            let g = e.path().join("config").join("grid");
            if g.is_dir() {
                out.push(g);
            }
        }
    }
    out
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// The cover, background and logo from the Steam CDN, for a game outside Steam
/// that still has a store page.
pub fn cdn_art(id: u32) -> (Vec<String>, Vec<String>, Vec<String>) {
    let url = |f: &str| format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/{f}");
    (
        vec![url("library_600x900.jpg"), url("header.jpg")],
        vec![url("library_hero.jpg")],
        vec![url("logo.png")],
    )
}

/// The candidates for an image: first the user's custom ones, then the local
/// Steam cache (new and old format), then the Steam CDN.
fn art(id: &str, grids: &[PathBuf], cache: &Path, kind: Art, online: bool) -> Vec<String> {
    let mut v = Vec::new();

    let grid_names: Vec<String> = match kind {
        Art::Cover => vec![format!("{id}p.png"), format!("{id}p.jpg")],
        Art::Hero => vec![format!("{id}_hero.png"), format!("{id}_hero.jpg")],
        Art::Logo => vec![format!("{id}_logo.png")],
    };
    for g in grids {
        for n in &grid_names {
            let p = g.join(n);
            if p.is_file() {
                v.push(s(&p));
            }
        }
    }

    let base = match kind {
        Art::Cover => "library_600x900",
        Art::Hero => "library_hero",
        Art::Logo => "logo",
    };
    let exts: &[&str] = match kind {
        Art::Logo => &["png"],
        _ => &["jpg", "png"],
    };

    let dir = cache.join(id);
    if dir.is_dir() {
        if let Some(p) = find_file(&dir, base, exts, 2) {
            v.push(s(&p));
        }
    }
    for e in exts {
        let p = cache.join(format!("{id}_{base}.{e}"));
        if p.is_file() {
            v.push(s(&p));
        }
    }

    let ext = if matches!(kind, Art::Logo) { "png" } else { "jpg" };
    if online {
        v.push(format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/{base}.{ext}"));
    }

    // The last option for the cover: the horizontal banner.
    if matches!(kind, Art::Cover) {
        if dir.is_dir() {
            if let Some(p) = find_file(&dir, "header", &["jpg"], 2) {
                v.push(s(&p));
            }
        }
        let p = cache.join(format!("{id}_header.jpg"));
        if p.is_file() {
            v.push(s(&p));
        }
        if online {
            v.push(format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/header.jpg"));
        }
    }
    v
}

fn find_file(dir: &Path, base: &str, exts: &[&str], depth: u32) -> Option<PathBuf> {
    let rd = fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
            continue;
        }
        let name = e.file_name().to_string_lossy().to_lowercase();
        if exts.iter().any(|x| name == format!("{base}.{x}")) {
            return Some(p);
        }
    }
    if depth > 0 {
        for d in subdirs {
            if let Some(p) = find_file(&d, base, exts, depth - 1) {
                return Some(p);
            }
        }
    }
    None
}
