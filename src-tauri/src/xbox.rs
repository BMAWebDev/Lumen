//! Importul jocurilor instalate prin aplicația Xbox / PC Game Pass.
//!
//! Jocurile GDK au un fișier `MicrosoftGame.config` în folderul de instalare.
//! Un script PowerShell enumeră pachetele MSIX (`Get-AppxPackage`), le păstrează
//! doar pe cele care au acest fișier și extrage numele, ID-ul aplicației și
//! imaginile declarate.

use std::path::{Path, PathBuf};

use crate::model::Game;
#[cfg(windows)]
use crate::platform;

const SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$out = New-Object System.Collections.ArrayList
foreach ($p in (Get-AppxPackage)) {
  if ($p.IsFramework -or $p.IsResourcePackage) { continue }
  $loc = $p.InstallLocation
  if (-not $loc) { continue }
  $cfg = Join-Path $loc 'MicrosoftGame.config'
  if (-not (Test-Path -LiteralPath $cfg)) { continue }
  $x = $null
  try { $x = [xml](Get-Content -LiteralPath $cfg -Raw -Encoding UTF8) } catch { continue }
  $g = $x.Game
  if (-not $g) { continue }
  $sv = $g.ShellVisuals
  $appId = $null; $display = $null
  try {
    $m = Get-AppxPackageManifest -Package $p.PackageFullName
    $app = @($m.Package.Applications.Application)[0]
    $appId = $app.Id
    $display = $m.Package.Properties.DisplayName
  } catch {}
  if (-not $appId) { $appId = @($g.ExecutableList.Executable)[0].Id }
  $name = $sv.DefaultDisplayName
  if (-not $name -or "$name".StartsWith('ms-resource:')) { $name = $display }
  if (-not $name -or "$name".StartsWith('ms-resource:')) { $name = $p.Name }
  [void]$out.Add([pscustomobject]@{
    name = "$name"
    family = "$($p.PackageFamilyName)"
    appId = "$appId"
    location = "$loc"
    square480 = "$($sv.Square480x480Logo)"
    square150 = "$($sv.Square150x150Logo)"
    splash = "$($sv.SplashScreenImage)"
    store = "$($sv.StoreLogo)"
  })
}
ConvertTo-Json -InputObject @($out) -Depth 3 -Compress
"#;

#[cfg(not(windows))]
pub fn scan() -> Result<Vec<Game>, String> {
    Ok(Vec::new())
}

#[cfg(windows)]
pub fn scan() -> Result<Vec<Game>, String> {
    let out = platform::powershell(SCRIPT)?;
    parse(&out)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn parse(out: &str) -> Result<Vec<Game>, String> {
    let txt = out.trim().trim_start_matches('\u{feff}');
    if txt.is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value =
        serde_json::from_str(txt).map_err(|e| format!("Lista jocurilor Xbox nu a putut fi citită: {e}"))?;
    let arr = match value {
        serde_json::Value::Array(a) => a,
        v @ serde_json::Value::Object(_) => vec![v],
        _ => Vec::new(),
    };

    let mut games = Vec::new();
    for r in &arr {
        let g = |k: &str| {
            r.get(k)
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let (Some(family), Some(app_id), Some(loc)) = (g("family"), g("appId"), g("location")) else {
            continue;
        };
        let name = g("name").unwrap_or_else(|| family.clone());
        let base = PathBuf::from(&loc);
        let cover: Vec<String> = ["square480", "square150", "store"]
            .iter()
            .copied()
            .filter_map(|k| g(k))
            .filter_map(|rel| resolve_asset(&base, &rel))
            .collect();
        let hero: Vec<String> = g("splash").and_then(|rel| resolve_asset(&base, &rel)).into_iter().collect();
        games.push(Game {
            id: format!("xbox:{family}"),
            source: "xbox".into(),
            name,
            install_dir: loc.clone(),
            launch_uri: format!("shell:AppsFolder\\{family}!{app_id}"),
            cover,
            hero,
            logo: Vec::new(),
            size_bytes: 0,
        });
    }
    Ok(games)
}

/// Găsește fișierul unei imagini. Dacă numele exact lipsește, caută variantele
/// cu scală (`StoreLogo.scale-200.png`) și o alege pe cea mai mare.
fn resolve_asset(base: &Path, rel: &str) -> Option<String> {
    let rel = rel.replace('/', "\\");
    let p = base.join(&rel);
    if p.is_file() {
        return Some(p.to_string_lossy().into_owned());
    }
    let dir = p.parent()?;
    let stem = p.file_stem()?.to_string_lossy().to_lowercase();
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let prefix = format!("{stem}.");
    let suffix = format!(".{ext}");
    let mut best: Option<(u32, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let n = e.file_name().to_string_lossy().to_lowercase();
        if n.starts_with(&prefix) && n.ends_with(&suffix) {
            let scale = n
                .split(".scale-")
                .nth(1)
                .and_then(|s| s.split('.').next())
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(100);
            if best.as_ref().map_or(true, |(b, _)| scale > *b) {
                best = Some((scale, e.path()));
            }
        }
    }
    best.map(|(_, p)| p.to_string_lossy().into_owned())
}
