//! Utilities with no external dependencies.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Language of the messages the backend shows to the user. English is the default.
static ROMANIAN: AtomicBool = AtomicBool::new(false);

pub const LANGUAGES: &[&str] = &["en", "ro"];

/// Selects the language of backend messages; unknown codes fall back to English.
pub fn set_lang(code: &str) {
    ROMANIAN.store(code == "ro", Ordering::Relaxed);
}

pub fn is_ro() -> bool {
    ROMANIAN.load(Ordering::Relaxed)
}

/// Picks the English or the Romanian text, evaluating only the chosen one:
/// `t!("Game not found.", "Jocul nu a fost găsit.")`.
#[macro_export]
macro_rules! t {
    ($en:expr, $ro:expr $(,)?) => {
        if $crate::util::is_ro() {
            $ro
        } else {
            $en
        }
    };
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Shared message for a poisoned application-state mutex.
pub fn state_locked() -> &'static str {
    crate::t!("The application state is locked.", "Starea aplicației este blocată.")
}

/// Normalizes a Windows path for comparisons: `\` separators, lowercase,
/// no trailing separator.
pub fn norm_path(p: &str) -> String {
    p.replace('/', "\\")
        .to_lowercase()
        .trim_end_matches('\\')
        .to_string()
}

/// Checks whether `path` is inside the folder `dir`.
/// Refuses folders that are too short (e.g. `C:`), so we don't match the whole disk.
pub fn is_inside(path: &str, dir: &str) -> bool {
    let dir = norm_path(dir);
    if dir.len() < 4 {
        return false;
    }
    let path = norm_path(path);
    path.len() > dir.len() && path.starts_with(&dir) && path.as_bytes()[dir.len()] == b'\\'
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

/// Encodes a script for `powershell -EncodedCommand` (UTF-16LE + base64).
pub fn powershell_encode(script: &str) -> String {
    let mut bytes = Vec::with_capacity(script.len() * 2);
    for u in script.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    base64(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(powershell_encode("dir"), "ZABpAHIA");
    }

    #[test]
    fn inside() {
        assert!(is_inside(r"D:\SteamLibrary\steamapps\common\Game\bin\game.exe", r"d:/steamlibrary/steamapps/common/game"));
        assert!(!is_inside(r"D:\SteamLibrary\steamapps\common\Game2\game.exe", r"D:\SteamLibrary\steamapps\common\Game"));
        assert!(!is_inside(r"C:\Windows\explorer.exe", r"C:\"));
    }
}
