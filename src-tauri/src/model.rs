use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    /// `steam:<appid>`, `xbox:<PackageFamilyName>` or `local:<folder>`
    pub id: String,
    /// `steam`, `xbox` or `local`
    pub source: String,
    pub name: String,
    /// The game folder; any process started from it counts as "the game".
    pub install_dir: String,
    /// A URI (Steam, Xbox) or, for local games, the path of the executable.
    pub launch_uri: String,
    /// The executable's arguments (local games only).
    #[serde(default)]
    pub launch_args: Vec<String>,
    /// Image candidates, in order of preference (local paths or URLs).
    #[serde(default)]
    pub cover: Vec<String>,
    #[serde(default)]
    pub hero: Vec<String>,
    #[serde(default)]
    pub logo: Vec<String>,
    #[serde(default)]
    pub size_bytes: u64,
    /// The executable's icon (local PNG), for games without a cover.
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
    /// Freezes the game while the Lumen menu is open over it.
    pub pause_game: bool,
    pub sounds: bool,
    pub music: bool,
    pub online_art: bool,
    pub reduce_motion: bool,
    /// Folders whose subfolders are separately installed games.
    pub game_folders: Vec<String>,
    pub autostart: bool,
    /// Starts Lumen when a recognized controller connects while it is closed.
    pub start_on_pad: bool,
    /// Looks for a new version of Lumen when it starts.
    pub check_updates: bool,
    /// Interface language: `en` (default) or `ro`.
    pub language: String,
    /// Names the user gave to Bluetooth devices, by Windows device id.
    pub bluetooth_names: HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            minimize_on_launch: true,
            pause_game: true,
            sounds: true,
            music: true,
            online_art: true,
            reduce_motion: false,
            game_folders: Vec::new(),
            autostart: false,
            start_on_pad: false,
            check_updates: true,
            language: "en".into(),
            bluetooth_names: HashMap::new(),
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
