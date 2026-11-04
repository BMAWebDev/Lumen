# Lumen

A console-style game launcher for Windows 11: it boots straight into full screen, is driven by a gamepad, and brings you back on its own when you quit a game. It automatically imports games installed through **Steam** and the **Xbox app / PC Game Pass**, plus games you keep elsewhere.

Built with [Tauri 2](https://tauri.app): a Rust backend and a framework-free HTML/CSS/JS interface rendered by WebView2 (included in Windows 11). The executable is a few MB, and the interface only animates `transform` and `opacity`, so it runs on the GPU and stays smooth even at 120 Hz.

The interface is available in English (the default) and Romanian. Switch it in **Settings → Language**; the choice is remembered.

## Features

- **Home screen**: your 12 most recently played games in a horizontal row. The selected cover grows and the background fades smoothly to the game's artwork.
- **Library**: every game in a grid, with filters for Steam, Xbox and local games. Any game can be removed from the game options; Steam, Xbox and folder games stay installed and can be brought back from **Settings → Hidden games**.
- **Platforms**: lists the gaming launchers and stores installed on the PC (Steam, Epic Games Store, GOG Galaxy, EA app, Ubisoft Connect, Battle.net, Rockstar, Riot, Amazon Games, itch.io, Xbox) with their real icons. Pick one to open it; Lumen steps aside and comes back to the front when you close the platform. While a platform is in front, the Guide / PS button is left to it.
- **Games outside Steam and Xbox**: in **Settings → Game folders** you add a folder (e.g. `D:\Games`) and every subfolder becomes a game. The main executable is picked automatically; League of Legends and VALORANT launch through Riot Client. You can also add a single game from **Settings → Add a game** by picking its executable. Local games get the executable's icon and, when the game is also on Steam, its official cover art.
- **Launch and return**: games start through Steam, the Xbox app or their own executable. Lumen watches the processes in the game's folder, minimizes while you play and comes back to the foreground when you quit. Playtime is saved automatically.
- **Battery**: the header shows the PC battery (on laptops and handhelds) and the battery of connected controllers (DualSense / DualShock 4 over HID, Xbox controllers via XInput).
- **PlayStation controllers**: DualSense and DualShock 4 are detected, and button hints switch to ✕ ○ △.
- **Guide / PS button** opens the quick menu from anywhere, even in game. The Xbox Guide button is read through XInput, the PS button on DualSense / DualShock 4 directly over HID, so no Steam Input or DS4Windows is needed.
- **Quick menu**: Home, Close game, Refresh, Settings, Power (sleep, restart, shut down).
- **Start with Windows**, for a console-like experience from boot.
- **Themes**: custom sounds, music, font and wallpaper without touching the code.
- **Languages**: English and Romanian, switchable at runtime from Settings.

## Controls

| Action | Gamepad | Keyboard |
|---|---|---|
| Navigate | D-pad / left stick | Arrow keys |
| Play / Select | A (✕ on DualSense) | Enter |
| Back | B (○) | Esc |
| Game options | Menu (☰) / Y (Options / △ on DualSense) | O |
| Quick menu | View / Guide (Create / PS on DualSense) | Tab |
| Switch Games ↔ Library | LB / RB | Q / E |
| First / last game | LT / RT (L2 / R2) | Home / End |

The mouse works too: one click selects, a second click launches.

## Building

### The easy way: GitHub Actions

Push the project to GitHub. The workflow in `.github/workflows/build.yml` builds on Windows on every push and publishes both `lumen.exe` (portable) and the `Lumen_x.y.z_x64-setup.exe` installer under **Actions → Artifacts**.

If you push a tag (`git tag v0.1.0 && git push --tags`), the installer is published automatically as a **Release**.

### Locally, on Windows

1. Install [Rust](https://rustup.rs) (the default toolchain, `stable-x86_64-pc-windows-msvc`).
2. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development with C++** workload.
3. Install the Tauri CLI (once):
   ```powershell
   cargo install tauri-cli --version "^2" --locked
   ```
4. From the project folder:
   ```powershell
   cargo tauri dev     # run in development mode (F12 = DevTools)
   cargo tauri build   # build the installer into src-tauri\target\release\bundle\nsis\
   ```

Node.js and npm are not needed.

### Just the interface, in a browser

The interface also works without the backend, using mock games, which is handy for design work:

```powershell
cd ui
python -m http.server 8765
```

Then open `http://localhost:8765`. In the browser console, `lumenGuide()` simulates the Guide button.

## Customization

Lumen creates its theme folder on first launch, at `%APPDATA%\dev.lumen.launcher\theme`. You can also open it from **Settings → Open theme folder**. Put files with these names there:

| File | Purpose |
|---|---|
| `move.wav`, `confirm.wav`, `back.wav`, `open.wav`, `launch.wav` | interface sounds (ogg and mp3 also work) |
| `music.mp3` | looping background music (ogg, wav and m4a also work) |
| `font.ttf` | interface font (otf, woff and woff2 also work) |
| `wallpaper.jpg` | default background (png and webp also work) |

Restart Lumen after making changes. Theme files stay on your computer and are not part of the repository.

**Steam artwork** is taken, in order, from: your custom Steam artwork (`userdata\…\config\grid`), the local Steam library cache, then the Steam CDN (downloading can be turned off in Settings).

## How it works

| File | Purpose |
|---|---|
| `src-tauri/src/steam.rs` | reads `libraryfolders.vdf` and `appmanifest_*.acf` from every Steam library |
| `src-tauri/src/xbox.rs` | finds GDK games (those with `MicrosoftGame.config`) via `Get-AppxPackage` |
| `src-tauri/src/local.rs` | scans game folders and builds manually added games |
| `src-tauri/src/art.rs` | extracts executable icons and looks local games up on Steam for cover art |
| `src-tauri/src/launcher.rs` | launches via `steam://rungameid/…`, `shell:AppsFolder\…` or the game's executable, then watches the processes in the game's folder |
| `src-tauri/src/guide.rs` | reads the Guide button via `XInputGetStateEx` and brings Lumen to the front |
| `src-tauri/src/pads.rs` | reads controller battery (HID for Sony, XInput for Xbox) and the PS button over HID |
| `src-tauri/src/platform.rs` | Windows calls: processes, foreground, registry, power, PC battery |
| `ui/js/app.js` | the interface: row, library, menus, navigation |
| `ui/js/input.js` | keyboard, gamepad, mouse wheel |
| `ui/js/sound.js` | sounds synthesized with Web Audio or loaded from the theme |
| `ui/js/i18n.js` | interface translations (English, Romanian) and the `t()` helper |

Messages produced by the backend (errors and system dialogs) follow the same language through the `t!` macro in `src-tauri/src/util.rs`. To add a language, add a block to `dict` and an entry to `LANGUAGES` in `ui/js/i18n.js`, and teach `util::set_lang` and `util::LANGUAGES` about it.

Data (the scanned library, playtime, settings, manually added games and cached artwork) is saved as JSON in `%APPDATA%\dev.lumen.launcher\`.

## Known limitations

- **Older Xbox (UWP) games** without `MicrosoftGame.config` are not detected. Most current Game Pass titles are GDK and do show up.
- **The quick menu over a game** appears on top of games running in borderless windowed mode. Games in exclusive fullscreen are minimized when Lumen comes to the foreground.
- A Steam game that starts another launcher of its own (EA, Ubisoft) may run from a different folder; its playtime is then not recorded correctly.
- **Local games**: the main executable is picked heuristically. If the wrong one is chosen, add the game manually with the right executable; it replaces the automatic entry.

## License

MIT
