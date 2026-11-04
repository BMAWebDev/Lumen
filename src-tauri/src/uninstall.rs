//! Uninstalling a game, depending on where it is installed.

use std::fs;
use std::path::Path;

use crate::model::Game;
use crate::platform;

/// What happened, so the interface knows whether the game is already gone or is waiting
/// for a confirmation in another application.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// `true` if the game was uninstalled just now.
    pub done: bool,
    pub message: String,
}

pub fn run(g: &Game) -> Result<Outcome, String> {
    match g.source.as_str() {
        "steam" => steam(g),
        "xbox" => xbox(g),
        _ => local(g),
    }
}

/// Steam shows its own confirmation window.
fn steam(g: &Game) -> Result<Outcome, String> {
    let appid = g.id.strip_prefix("steam:").filter(|a| !a.is_empty() && a.chars().all(|c| c.is_ascii_digit()));
    let appid = appid.ok_or(crate::t!("Invalid Steam id.", "Id Steam nevalid."))?;
    platform::open_uri(&format!("steam://uninstall/{appid}"))?;
    Ok(Outcome { done: false, message: crate::t!("Confirm the uninstall in the Steam window, then refresh the library.", "Confirmă dezinstalarea în fereastra Steam, apoi reîmprospătează biblioteca.").into() })
}

#[cfg(windows)]
fn xbox(g: &Game) -> Result<Outcome, String> {
    let family = g.id.strip_prefix("xbox:").unwrap_or_default();
    // The family name ends up in a script, so we only accept the `Name_hash` form.
    if family.is_empty() || !family.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        return Err(crate::t!("Invalid Xbox id.", "Id Xbox nevalid.").into());
    }
    let script = format!(
        "$ErrorActionPreference='Stop'\n\
         try {{ Get-AppxPackage | Where-Object {{ $_.PackageFamilyName -eq '{family}' }} | Remove-AppxPackage; [Console]::Out.Write('OK') }} \
         catch {{ [Console]::Out.Write('ERR:' + $_.Exception.Message) }}"
    );
    let out = platform::powershell(&script)?;
    match out.trim() {
        "OK" => Ok(Outcome { done: true, message: crate::t!(format!("{} was uninstalled.", g.name), format!("{} a fost dezinstalat.", g.name)) }),
        e => {
            let e = e.trim_start_matches("ERR:");
            Err(crate::t!(format!("Could not uninstall {}: {e}", g.name), format!("Nu am putut dezinstala {}: {e}", g.name)))
        }
    }
}

#[cfg(not(windows))]
fn xbox(_g: &Game) -> Result<Outcome, String> {
    Err(crate::t!("Only available on Windows.", "Disponibil doar pe Windows.").into())
}

/// Games in folders have no record in Windows; we use the uninstaller in their folder
/// (`unins000.exe` etc.), if there is one. We never delete files ourselves.
fn local(g: &Game) -> Result<Outcome, String> {
    let exe = find_uninstaller(Path::new(&g.install_dir))
        .ok_or(crate::t!(
            "The game has no uninstaller. Open the game location and delete the folder manually.",
            "Jocul nu are dezinstalator. Deschide locația jocului și șterge folderul manual."
        ))?;
    platform::run_exe(&exe, &[])?;
    Ok(Outcome { done: false, message: crate::t!("Follow the uninstaller's steps, then refresh the library.", "Urmează pașii dezinstalatorului, apoi reîmprospătează biblioteca.").into() })
}

fn find_uninstaller(dir: &Path) -> Option<String> {
    let mut found: Vec<_> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            p.is_file() && n.starts_with("unins") && n.ends_with(".exe")
        })
        .collect();
    found.sort();
    found.into_iter().next().map(|p| p.to_string_lossy().into_owned())
}
