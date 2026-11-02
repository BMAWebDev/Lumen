//! Utilitare fără dependențe externe.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Normalizează o cale Windows pentru comparații: separatori `\`, litere mici,
/// fără separator final.
pub fn norm_path(p: &str) -> String {
    p.replace('/', "\\")
        .to_lowercase()
        .trim_end_matches('\\')
        .to_string()
}

/// Verifică dacă `path` se află în interiorul folderului `dir`.
/// Refuză folderele prea scurte (de ex. `C:`), ca să nu potrivim tot discul.
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

/// Codifică un script pentru `powershell -EncodedCommand` (UTF-16LE + base64).
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
