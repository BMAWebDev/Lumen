# Lumen

Un lansator de jocuri pentru Windows 11, în stil consolă: pornește direct în ecran complet, se controlează din gamepad și te aduce înapoi singur când închizi jocul. Importă automat jocurile instalate prin **Steam** și prin **aplicația Xbox / PC Game Pass**.

Construit cu [Tauri 2](https://tauri.app): backend în Rust, interfață în HTML/CSS/JS fără framework, randată de WebView2 (inclus în Windows 11). Executabilul are câțiva MB, iar interfața animă doar `transform` și `opacity`, deci rulează pe GPU și rămâne fluidă și la 120 Hz.

## Ce face

- **Ecranul principal**: ultimele 12 jocuri jucate, într-un rând orizontal. Coperta selectată se mărește, iar fundalul trece lin la arta jocului.
- **Biblioteca**: toate jocurile într-o grilă, cu filtre pentru Steam, Xbox și jocurile locale.
- **Jocuri din afara Steam și Xbox**: în **Setări → Foldere cu jocuri** adaugi un folder (de ex. `D:\Jocuri`), iar fiecare subfolder devine un joc. Executabilul principal se alege automat; League of Legends și VALORANT pornesc prin Riot Client. Poți adăuga și un singur joc, din **Setări → Adaugă un joc**, alegând executabilul. Jocurile locale primesc iconița executabilului și, dacă există și pe Steam, coperta oficială.
- **Lansare și revenire**: jocul pornește prin Steam sau prin aplicația Xbox. Lumen urmărește procesele din folderul jocului, se minimizează cât joci și revine în prim-plan după ce ieși. Timpul de joc se salvează automat.
- **Butonul Guide** (logo-ul Xbox de pe controller) deschide meniul rapid de oriunde, chiar și din joc.
- **Meniul rapid**: Acasă, Bibliotecă, Închide jocul, Reîmprospătează, Setări, Alimentare (repaus, repornire, oprire).
- **Pornire odată cu Windows**, pentru o experiență de consolă de la boot.
- **Teme**: sunete, muzică, font și fundal personalizate, fără să modifici codul.

## Controale

| Acțiune | Gamepad | Tastatură |
|---|---|---|
| Navigare | D-pad / stick stânga | Săgeți |
| Joacă / Selectează | A (✕ pe DualSense) | Enter |
| Înapoi | B (○) | Esc |
| Opțiuni joc | Menu (☰) / Y (Options / △ pe DualSense) | O |
| Meniu rapid | View / Guide (Create / PS pe DualSense) | Tab |
| Comută Jocuri ↔ Bibliotecă | LB / RB | Q / E |
| Primul / ultimul joc | LT / RT (L2 / R2) | Home / End |

Merge și cu mouse-ul: un clic selectează, al doilea lansează.

## Compilare

### Varianta simplă: GitHub Actions

Urcă proiectul pe GitHub. Workflow-ul din `.github/workflows/build.yml` compilează pe Windows la fiecare push și pune la dispoziție, în tab-ul **Actions → Artifacts**, atât `lumen.exe` (portabil) cât și installer-ul `Lumen_x.y.z_x64-setup.exe`.

Dacă publici un tag (`git tag v0.1.0 && git push --tags`), installer-ul apare automat ca **Release**.

### Local, pe Windows

1. Instalează [Rust](https://rustup.rs) (toolchain-ul implicit, `stable-x86_64-pc-windows-msvc`).
2. Instalează [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) cu sarcina **Desktop development with C++**.
3. Instalează CLI-ul Tauri (o singură dată):
   ```powershell
   cargo install tauri-cli --version "^2" --locked
   ```
4. Din folderul proiectului:
   ```powershell
   cargo tauri dev     # rulează în modul dezvoltare (F12 = DevTools)
   cargo tauri build   # creează installer-ul în src-tauri\target\release\bundle\nsis\
   ```

Nu e nevoie de Node.js sau npm.

### Doar interfața, în browser

Interfața funcționează și fără backend, cu jocuri simulate, ceea ce e util când lucrezi la design:

```powershell
cd ui
python -m http.server 8765
```

Apoi deschide `http://localhost:8765`. În consola browserului, `lumenGuide()` simulează butonul Guide.

## Personalizare

Lumen creează folderul temei la prima pornire, în `%APPDATA%\dev.lumen.launcher\theme`. Îl deschizi și din **Setări → Deschide folderul temei**. Pune acolo fișiere cu aceste nume:

| Fișier | Rol |
|---|---|
| `move.wav`, `confirm.wav`, `back.wav`, `open.wav`, `launch.wav` | sunetele interfeței (acceptă și ogg, mp3) |
| `music.mp3` | muzică de fundal în buclă (acceptă și ogg, wav, m4a) |
| `font.ttf` | fontul interfeței (acceptă și otf, woff, woff2) |
| `wallpaper.jpg` | fundalul implicit (acceptă și png, webp) |

Repornește Lumen după modificări. Fișierele de temă rămân pe calculatorul tău și nu fac parte din repository.

**Imaginile Steam** se iau, în ordine, din: imaginile personalizate din Steam (`userdata\…\config\grid`), cache-ul local al bibliotecii Steam, apoi CDN-ul Steam (poți dezactiva descărcarea din Setări).

## Cum funcționează

| Fișier | Rol |
|---|---|
| `src-tauri/src/steam.rs` | citește `libraryfolders.vdf` și `appmanifest_*.acf` din fiecare bibliotecă Steam |
| `src-tauri/src/xbox.rs` | găsește jocurile GDK (cele cu `MicrosoftGame.config`) prin `Get-AppxPackage` |
| `src-tauri/src/launcher.rs` | lansează prin `steam://rungameid/…` sau `shell:AppsFolder\…`, apoi urmărește procesele din folderul jocului |
| `src-tauri/src/guide.rs` | citește butonul Guide prin `XInputGetStateEx` |
| `src-tauri/src/platform.rs` | apeluri Windows: procese, prim-plan, registry, alimentare |
| `ui/js/app.js` | interfața: rândul, biblioteca, meniurile, navigarea |
| `ui/js/input.js` | tastatură, gamepad, rotița mouse-ului |
| `ui/js/sound.js` | sunete sintetizate cu Web Audio sau încărcate din temă |

Datele (biblioteca scanată, timpul de joc, setările) se salvează ca JSON în `%APPDATA%\dev.lumen.launcher\`.

## Limitări cunoscute

- **Jocurile Xbox mai vechi (UWP)**, fără `MicrosoftGame.config`, nu sunt detectate. Majoritatea titlurilor Game Pass actuale sunt GDK și apar.
- **DualSense**: navigarea în interfață funcționează nativ, dar butonul PS deschide meniul rapid doar dacă controllerul e mapat ca XInput (Steam Input sau DS4Windows).
- **Meniul rapid peste joc** apare peste jocurile în fereastră fără margini (borderless). Jocurile în exclusive fullscreen se minimizează când Lumen trece în prim-plan.
- Un joc Steam care pornește la rândul lui alt launcher (EA, Ubisoft) poate rula din alt folder. Atunci timpul de joc nu se înregistrează corect.

## Licență

MIT
