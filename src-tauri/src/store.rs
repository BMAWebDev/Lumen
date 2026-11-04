//! Simple persistence in JSON files in the application data folder.

use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::Path;

pub fn load<T: DeserializeOwned + Default>(dir: &Path, name: &str) -> T {
    fs::read(dir.join(name))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Writes atomically: first to a temporary file, then renames it.
pub fn save<T: Serialize>(dir: &Path, name: &str, value: &T) {
    let tmp = dir.join(format!("{name}.tmp"));
    if let Ok(bytes) = serde_json::to_vec_pretty(value) {
        if fs::write(&tmp, &bytes).is_ok() {
            let _ = fs::rename(&tmp, dir.join(name));
        }
    }
}
