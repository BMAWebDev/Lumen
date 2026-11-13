//! Updates: looks for a newer release on GitHub and installs it by running the release's installer.
//! Network access goes through PowerShell, like the rest of Lumen (no HTTP crate).

use std::sync::Mutex;

use serde::Serialize;
use tauri::AppHandle;

use crate::{platform, t};

const REPO: &str = "BMAWebDev/Lumen";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// The new version, without the leading `v`.
    pub version: String,
    /// The release page.
    pub url: String,
    #[serde(skip)]
    asset_url: String,
    #[serde(skip)]
    digest: Option<String>,
}

/// The update found by the last check; `install` uses it, so the interface never supplies a download address.
static FOUND: Mutex<Option<UpdateInfo>> = Mutex::new(None);

const CHECK_SCRIPT: &str = r#"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$ProgressPreference = 'SilentlyContinue'
$r = @{ ok = $false }
try {
  $rel = Invoke-RestMethod "https://api.github.com/repos/__REPO__/releases/latest" -Headers @{ 'User-Agent' = 'Lumen' } -TimeoutSec 10
  $r.ok = $true
  $r.tag = [string]$rel.tag_name
  $r.url = [string]$rel.html_url
  $r.assets = @($rel.assets | ForEach-Object { @{ name = [string]$_.name; url = [string]$_.browser_download_url; digest = [string]$_.digest } })
} catch {}
$r | ConvertTo-Json -Compress -Depth 4
"#;

/// Numeric parts of a version (`v0.3.1-beta` -> [0, 3, 1]).
fn parse_version(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

fn is_newer(candidate: &str, current: &str) -> bool {
    let (mut a, mut b) = (parse_version(candidate), parse_version(current));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

/// Looks for a release newer than this build. `None` means Lumen is up to date.
pub fn check() -> Result<Option<UpdateInfo>, String> {
    let out = platform::powershell(&CHECK_SCRIPT.replace("__REPO__", REPO))?;
    let res: serde_json::Value = serde_json::from_str(out.trim()).unwrap_or_default();
    if res["ok"] != true {
        return Err(t!(
            "Could not check for updates. Check your internet connection.",
            "Nu s-au putut verifica actualizările. Verifică conexiunea la internet."
        )
        .into());
    }
    let tag = res["tag"].as_str().unwrap_or_default();
    if tag.is_empty() || !is_newer(tag, env!("CARGO_PKG_VERSION")) {
        if let Ok(mut f) = FOUND.lock() {
            *f = None;
        }
        return Ok(None);
    }
    // Only an installer published in the project's own releases is accepted.
    let prefix = format!("https://github.com/{REPO}/releases/download/");
    let asset = res["assets"].as_array().and_then(|a| {
        a.iter().find(|x| {
            x["name"].as_str().is_some_and(|n| n.to_lowercase().ends_with(".exe"))
                && x["url"].as_str().is_some_and(|u| u.starts_with(&prefix))
        })
    });
    let Some(asset) = asset else {
        // A release without an installer (still being built): not an update yet.
        return Ok(None);
    };
    let info = UpdateInfo {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        url: res["url"].as_str().unwrap_or_default().to_string(),
        asset_url: asset["url"].as_str().unwrap_or_default().to_string(),
        digest: asset["digest"].as_str().filter(|d| d.starts_with("sha256:")).map(|d| d[7..].to_lowercase()),
    };
    if let Ok(mut f) = FOUND.lock() {
        *f = Some(info.clone());
    }
    Ok(Some(info))
}

fn ps_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Downloads the installer found by the last check, then closes Lumen; a hidden helper runs the installer
/// silently and starts the new version.
pub fn install(app: &AppHandle) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err(t!("Updates cannot be installed in a development build.", "Actualizările nu se pot instala într-o versiune de dezvoltare.").into());
    }
    let info = FOUND.lock().ok().and_then(|f| f.clone()).ok_or(t!("There is no update to install.", "Nu există nicio actualizare de instalat."))?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let installer = std::env::temp_dir().join(format!("Lumen-{}-setup.exe", info.version));

    let download = format!(
        "$ErrorActionPreference = 'Stop'; [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
         $ProgressPreference = 'SilentlyContinue'; $dst = {dst}; $want = {want}; \
         try {{ \
           Invoke-WebRequest -Uri {url} -OutFile $dst -UseBasicParsing -TimeoutSec 900; \
           if ($want -and ((Get-FileHash -LiteralPath $dst -Algorithm SHA256).Hash.ToLower() -ne $want)) {{ Remove-Item -LiteralPath $dst -Force; 'bad-hash' }} else {{ 'ok' }} \
         }} catch {{ 'fail' }}",
        dst = ps_str(&installer.to_string_lossy()),
        want = ps_str(info.digest.as_deref().unwrap_or("")),
        url = ps_str(&info.asset_url),
    );
    match platform::powershell(&download)?.trim() {
        "ok" => {}
        "bad-hash" => {
            return Err(t!(
                "The downloaded update is corrupted. Try again.",
                "Actualizarea descărcată este coruptă. Încearcă din nou."
            )
            .into())
        }
        _ => {
            return Err(t!(
                "The update could not be downloaded. Check your internet connection.",
                "Actualizarea nu a putut fi descărcată. Verifică conexiunea la internet."
            )
            .into())
        }
    }

    // The helper outlives Lumen: it waits for it to close, installs silently, then starts the new version.
    let script = format!(
        "Start-Sleep -Seconds 2; Start-Process -FilePath {inst} -ArgumentList '/S' -Wait; \
         Remove-Item -LiteralPath {inst} -Force -ErrorAction SilentlyContinue; Start-Process -FilePath {exe}",
        inst = ps_str(&installer.to_string_lossy()),
        exe = ps_str(&exe.to_string_lossy()),
    );
    platform::hidden("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand"])
        .arg(crate::util::powershell_encode(&script))
        .spawn()
        .map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.3.1-beta.2"), vec![0, 3, 1]);
        assert!(is_newer("v0.3.0", "0.2.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
        assert!(!is_newer("0.2", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
    }
}
