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

/// Windows updates waiting to be installed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OsUpdates {
    pub count: usize,
    /// The titles of the first few updates.
    pub titles: Vec<String>,
    /// A restart is needed to finish updates that are already installed.
    pub reboot_required: bool,
}

const OS_SCRIPT: &str = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$r = @{ ok = $false; titles = @(); count = 0; reboot = $false }
try {
  $found = (New-Object -ComObject Microsoft.Update.Session).CreateUpdateSearcher().Search("IsInstalled=0 and IsHidden=0 and Type='Software'")
  $r.ok = $true
  $r.count = [int]$found.Updates.Count
  $r.titles = @($found.Updates | Select-Object -First 5 | ForEach-Object { [string]$_.Title })
  $r.reboot = [bool](New-Object -ComObject Microsoft.Update.SystemInfo).RebootRequired
} catch {}
$r | ConvertTo-Json -Compress -Depth 3
"#;

/// Asks Windows Update (without installing anything) what is waiting. Can take up to a minute.
pub fn check_os() -> Result<OsUpdates, String> {
    let out = platform::powershell_timeout(OS_SCRIPT, 120)?;
    let res: serde_json::Value = serde_json::from_str(out.trim()).unwrap_or_default();
    if res["ok"] != true {
        return Err(t!(
            "Could not check for Windows updates.",
            "Nu s-au putut verifica actualizările Windows."
        )
        .into());
    }
    Ok(OsUpdates {
        count: res["count"].as_u64().unwrap_or(0) as usize,
        titles: res["titles"]
            .as_array()
            .map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_owned)).collect())
            .unwrap_or_default(),
        reboot_required: res["reboot"] == true,
    })
}

/// The outcome of installing Windows updates.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OsInstall {
    pub installed: usize,
    pub failed: usize,
    pub reboot_required: bool,
}

/// Runs in an elevated PowerShell (installing needs administrator rights): downloads and installs the waiting
/// updates one by one, writing its progress to `__PROG__` and the outcome as JSON to `__OUT__`.
const OS_INSTALL_SCRIPT: &str = r#"
$out = __OUT__
$pf = __PROG__
function Progress($stage, $i, $n, $title) {
  @{ stage = $stage; i = $i; n = $n; title = $title } | ConvertTo-Json -Compress | Set-Content -LiteralPath $pf -Encoding UTF8
}
$r = @{ ok = $false; installed = 0; failed = 0; reboot = $false }
try {
  $session = New-Object -ComObject Microsoft.Update.Session
  $found = $session.CreateUpdateSearcher().Search("IsInstalled=0 and IsHidden=0 and Type='Software'")
  $list = @($found.Updates | Where-Object { -not $_.InstallationBehavior.CanRequestUserInput })
  $n = $list.Count
  $i = 0
  foreach ($u in $list) {
    $i++
    try {
      if (-not $u.EulaAccepted) { $u.AcceptEula() }
      $one = New-Object -ComObject Microsoft.Update.UpdateColl
      [void]$one.Add($u)
      Progress 'download' $i $n ([string]$u.Title)
      $dl = $session.CreateUpdateDownloader(); $dl.Updates = $one; [void]$dl.Download()
      Progress 'install' $i $n ([string]$u.Title)
      $inst = $session.CreateUpdateInstaller(); $inst.Updates = $one
      $res = $inst.Install()
      if ($res.GetUpdateResult(0).ResultCode -eq 2) { $r.installed++ } else { $r.failed++ }
      if ($res.RebootRequired) { $r.reboot = $true }
    } catch { $r.failed++ }
  }
  $r.ok = $true
} catch {}
$r | ConvertTo-Json -Compress | Set-Content -LiteralPath $out -Encoding ASCII
"#;

/// Where the installation is: `stage` is `download` or `install`, for update `i` of `n`.
#[derive(Debug, Clone, Serialize, serde::Deserialize, Default)]
pub struct OsProgress {
    pub stage: String,
    pub i: usize,
    pub n: usize,
    pub title: String,
}

/// Installs the waiting Windows updates. Windows asks the user to allow it (UAC); this blocks until the
/// installation ends, which can take many minutes, and sends `os-progress` events meanwhile.
pub fn install_os(app: &AppHandle) -> Result<OsInstall, String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tauri::Emitter;

    let out = std::env::temp_dir().join("lumen-windows-update.json");
    let prog = std::env::temp_dir().join("lumen-windows-update.progress.json");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&prog);
    let inner = OS_INSTALL_SCRIPT
        .replace("__OUT__", &ps_str(&out.to_string_lossy()))
        .replace("__PROG__", &ps_str(&prog.to_string_lossy()));
    let launch = format!(
        "try {{ Start-Process powershell.exe -Verb RunAs -WindowStyle Hidden -Wait -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-EncodedCommand','{}' }} catch {{}}",
        crate::util::powershell_encode(&inner)
    );

    // Reports the progress file to the interface while the elevated script runs.
    let done = Arc::new(AtomicBool::new(false));
    let watcher = {
        let (done, app, prog) = (done.clone(), app.clone(), prog.clone());
        std::thread::spawn(move || {
            let mut last = String::new();
            while !done.load(Ordering::Relaxed) {
                if let Ok(text) = std::fs::read_to_string(&prog) {
                    let text = text.trim_start_matches(char::from_u32(0xFEFF).unwrap_or(' ')).trim().to_owned();
                    if text != last {
                        if let Ok(p) = serde_json::from_str::<OsProgress>(&text) {
                            let _ = app.emit("os-progress", p);
                        }
                        last = text;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(1000));
            }
        })
    };
    let ran = platform::powershell_timeout(&launch, 4 * 3600);
    done.store(true, Ordering::Relaxed);
    let _ = watcher.join();
    let _ = std::fs::remove_file(&prog);
    ran?;

    let text = std::fs::read_to_string(&out).map_err(|_| {
        t!(
            "The installation did not start. Allow the changes when Windows asks.",
            "Instalarea nu a pornit. Permite modificările când Windows întreabă."
        )
    })?;
    let _ = std::fs::remove_file(&out);
    let res: serde_json::Value = serde_json::from_str(text.trim()).unwrap_or_default();
    if res["ok"] != true {
        return Err(t!("The Windows updates could not be installed.", "Actualizările Windows nu au putut fi instalate.").into());
    }
    Ok(OsInstall {
        installed: res["installed"].as_u64().unwrap_or(0) as usize,
        failed: res["failed"].as_u64().unwrap_or(0) as usize,
        reboot_required: res["reboot"] == true,
    })
}

/// Opens the Windows Update page of the system Settings, where the updates are installed.
pub fn open_windows_update() -> Result<(), String> {
    platform::open_uri("ms-settings:windowsupdate")
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

#[cfg(test)]
mod live {
    /// Asks the real Windows Update (needs Windows and a few seconds up to a minute): `cargo test -- --ignored os_check`.
    #[test]
    #[ignore]
    fn os_check() {
        let started = std::time::Instant::now();
        let r = super::check_os();
        println!("{r:?} in {:?}", started.elapsed());
        assert!(r.is_ok());
    }
}
