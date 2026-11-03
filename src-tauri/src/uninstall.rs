//! Dezinstalarea unui joc, în funcție de unde e instalat.

use std::fs;
use std::path::Path;

use crate::model::Game;
use crate::platform;

/// Ce s-a întâmplat, ca interfața să știe dacă jocul a dispărut deja sau așteaptă
/// o confirmare în altă aplicație.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// `true` dacă jocul a fost dezinstalat chiar acum.
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

/// Steam își arată singur fereastra de confirmare.
fn steam(g: &Game) -> Result<Outcome, String> {
    let appid = g.id.strip_prefix("steam:").filter(|a| !a.is_empty() && a.chars().all(|c| c.is_ascii_digit()));
    let appid = appid.ok_or("Id Steam nevalid.")?;
    platform::open_uri(&format!("steam://uninstall/{appid}"))?;
    Ok(Outcome { done: false, message: "Confirmă dezinstalarea în fereastra Steam, apoi reîmprospătează biblioteca.".into() })
}

#[cfg(windows)]
fn xbox(g: &Game) -> Result<Outcome, String> {
    let family = g.id.strip_prefix("xbox:").unwrap_or_default();
    // Numele familiei ajunge într-un script, deci acceptăm doar forma `Nume_hash`.
    if family.is_empty() || !family.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        return Err("Id Xbox nevalid.".into());
    }
    let script = format!(
        "$ErrorActionPreference='Stop'\n\
         try {{ Get-AppxPackage | Where-Object {{ $_.PackageFamilyName -eq '{family}' }} | Remove-AppxPackage; [Console]::Out.Write('OK') }} \
         catch {{ [Console]::Out.Write('ERR:' + $_.Exception.Message) }}"
    );
    let out = platform::powershell(&script)?;
    match out.trim() {
        "OK" => Ok(Outcome { done: true, message: format!("{} a fost dezinstalat.", g.name) }),
        e => Err(format!("Nu am putut dezinstala {}: {}", g.name, e.trim_start_matches("ERR:"))),
    }
}

#[cfg(not(windows))]
fn xbox(_g: &Game) -> Result<Outcome, String> {
    Err("Disponibil doar pe Windows.".into())
}

/// Jocurile din foldere nu au o evidență în Windows; folosim dezinstalatorul din folderul lor
/// (`unins000.exe` etc.), dacă există. Nu ștergem niciodată fișiere noi înșine.
fn local(g: &Game) -> Result<Outcome, String> {
    let exe = find_uninstaller(Path::new(&g.install_dir))
        .ok_or("Jocul nu are dezinstalator. Deschide locația jocului și șterge folderul manual.")?;
    platform::run_exe(&exe, &[])?;
    Ok(Outcome { done: false, message: "Urmează pașii dezinstalatorului, apoi reîmprospătează biblioteca.".into() })
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
