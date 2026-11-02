use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    /// `steam:<appid>`, `xbox:<PackageFamilyName>` sau `local:<folder>`
    pub id: String,
    /// `steam`, `xbox` sau `local`
    pub source: String,
    pub name: String,
    /// Folderul jocului; orice proces pornit din el e considerat „jocul”.
    pub install_dir: String,
    /// Un URI (Steam, Xbox) sau, pentru jocurile locale, calea executabilului.
    pub launch_uri: String,
    /// Argumentele executabilului (doar pentru jocurile locale).
    #[serde(default)]
    pub launch_args: Vec<String>,
    /// Candidați pentru imagini, în ordinea preferinței (căi locale sau URL-uri).
    #[serde(default)]
    pub cover: Vec<String>,
    #[serde(default)]
    pub hero: Vec<String>,
    #[serde(default)]
    pub logo: Vec<String>,
    #[serde(default)]
    pub size_bytes: u64,
    /// Iconița executabilului (PNG local), pentru jocurile fără copertă.
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Stat {
    pub playtime_sec: u64,
    pub last_played: u64,
    pub launches: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub minimize_on_launch: bool,
    pub sounds: bool,
    pub music: bool,
    pub online_art: bool,
    pub reduce_motion: bool,
    /// Foldere ale căror subfoldere sunt jocuri instalate separat.
    pub game_folders: Vec<String>,
    /// Citit din registry la fiecare cerere, nu salvat în fișier.
    #[serde(skip_deserializing)]
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            minimize_on_launch: true,
            sounds: true,
            music: true,
            online_art: true,
            reduce_motion: false,
            game_folders: Vec::new(),
            autostart: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub id: String,
    pub started: bool,
}

pub struct Inner {
    pub data_dir: PathBuf,
    pub games: Vec<Game>,
    pub stats: HashMap<String, Stat>,
    pub settings: Settings,
    pub running: Option<Running>,
    pub scanning: bool,
}

pub struct AppState {
    pub inner: Mutex<Inner>,
}
