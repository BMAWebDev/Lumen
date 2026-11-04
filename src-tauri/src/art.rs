//! Images for local games: the executable's icon (256 px) and, if the game
//! exists on Steam, the official cover, background and logo from the Steam CDN.
//!
//! The results are cached in `art.json`, so refreshing the library doesn't
//! extract the icons again or ask Steam every time.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::Game;
use crate::{local, platform, steam, store};

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Entry {
    icon: Option<String>,
    icon_tried: bool,
    steam: Option<u32>,
    steam_tried: bool,
}

/// Extracts the large icon of an executable as PNG and looks the game up on Steam by name.
const SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$r = [ordered]@{ icon = $false; steam = $null; ok = $false }
if ($exe) {
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
  $large = [IntPtr]::Zero; $small = [IntPtr]::Zero
  # Requested size: 256 px for the large icon, 16 px for the small one.
  if ([LumenIcon]::SHDefExtractIconW($exe, 0, 0, [ref]$large, [ref]$small, (16 -shl 16) -bor 256) -eq 0 -and $large -ne [IntPtr]::Zero) {
    $bmp = [System.Drawing.Icon]::FromHandle($large).ToBitmap()
    $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    [void][LumenIcon]::DestroyIcon($large)
    if ($small -ne [IntPtr]::Zero) { [void][LumenIcon]::DestroyIcon($small) }
    $r.icon = Test-Path -LiteralPath $out
  }
}
if ($name) {
  try {
    $q = [uri]::EscapeDataString($name)
    $res = Invoke-RestMethod "https://store.steampowered.com/api/storesearch/?term=$q&l=english&cc=US" -TimeoutSec 8
    $r.ok = $true
    $want = ($name -replace '[^\p{L}\p{Nd}]', '').ToLower()
    foreach ($it in $res.items) {
      if ((($it.name -replace '[^\p{L}\p{Nd}]', '').ToLower()) -eq $want) { $r.steam = [int]$it.id; break }
    }
  } catch {}
}
$r | ConvertTo-Json -Compress
"#;

fn ps_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// FNV-1a, for stable file names derived from the game ID.
fn hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// Fills in the images of local games (from the cache or, when needed, from disk and Steam).
pub fn decorate(games: &mut [Game], data_dir: &Path, online: bool) {
    let mut cache: HashMap<String, Entry> = store::load(data_dir, "art.json");
    let dir = data_dir.join("art");
    let _ = fs::create_dir_all(&dir);
    let mut changed = false;

    for g in games.iter_mut().filter(|g| g.source == "local") {
        let e = cache.entry(g.id.clone()).or_default();
        let need_icon = !e.icon_tried || e.icon.as_ref().is_some_and(|p| !Path::new(p).is_file());
        let need_steam = online && !e.steam_tried;
        if need_icon || need_steam {
            let out = dir.join(format!("{:016x}.png", hash(&g.id)));
            let exe = if need_icon { local::icon_source(g) } else { None };
            let mut script = format!(
                "$exe = {}; $out = {}; $name = {};",
                exe.map(|p| ps_str(&p.to_string_lossy())).unwrap_or_else(|| "$null".into()),
                ps_str(&out.to_string_lossy()),
                if need_steam { ps_str(&g.name) } else { "$null".into() },
            );
            script.push_str(SCRIPT);
            let res: serde_json::Value = platform::powershell(&script)
                .ok()
                .and_then(|t| serde_json::from_str(t.trim()).ok())
                .unwrap_or_default();
            if need_icon {
                e.icon_tried = true;
                e.icon = (res["icon"] == true).then(|| out.to_string_lossy().into_owned());
            }
            // No answer from Steam (no internet): we try again next time.
            if need_steam && res["ok"] == true {
                e.steam_tried = true;
                e.steam = res["steam"].as_u64().map(|id| id as u32);
            }
            changed = true;
        }
        g.icon = e.icon.clone();
        if let (true, Some(id)) = (online, e.steam) {
            let (cover, hero, logo) = steam::cdn_art(id);
            g.cover = cover;
            g.hero = hero;
            g.logo = logo;
        }
    }
    if changed {
        store::save(data_dir, "art.json", &cache);
    }
}

/// The game name from the executable's metadata (ProductName / FileDescription),
/// ignoring the generic values of engines and launchers.
pub fn exe_title(exe: &Path) -> Option<String> {
    let script = format!(
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
         $v = (Get-Item -LiteralPath {}).VersionInfo; \"$($v.ProductName)`n$($v.FileDescription)\"",
        ps_str(&exe.to_string_lossy())
    );
    let out = platform::powershell(&script).ok()?;
    const GENERIC: &[&str] = &["launcher", "bootstrap", "unreal", "unity", "epic games", "game", "application", "client"];
    out.lines()
        .map(|l| l.trim().trim_end_matches(['™', '®']).trim())
        .find(|l| l.chars().filter(|c| c.is_alphanumeric()).count() >= 3 && !GENERIC.contains(&l.to_lowercase().as_str()))
        .map(str::to_owned)
}
