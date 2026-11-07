// Interface translations. English is the default; Romanian keeps the original texts.
// A value is either a string with {placeholders} or a function that receives the variables.
// Adding a language means adding a block to `dict` and an entry to `LANGUAGES`.

export const LANGUAGES = [
  { code: 'en', name: 'English', locale: 'en-US' },
  { code: 'ro', name: 'Română', locale: 'ro-RO' },
];
export const DEFAULT_LANG = 'en';

// Romanian needs "de" after numbers from 20 up: "20 de zile", but "19 zile".
const roCount = (n, one, many) => {
  if (n === 1) return one;
  const r = n % 100;
  return `${n} ${n >= 20 && (r === 0 || r >= 20) ? 'de ' : ''}${many}`;
};
const enCount = (n, one, many) => (n === 1 ? one : `${n} ${many}`);

const dict = {
  en: {
    // Static page
    'tab.games': 'Games',
    'tab.library': 'Library',
    'tab.platforms': 'Platforms',
    'lib.title': 'Your library',
    'aria.settings': 'Settings',

    // Relative time and sizes
    'time.never': 'never',
    'time.now': 'just now',
    'time.minutes': ({ n }) => `${enCount(n, 'a minute', 'minutes')} ago`,
    'time.hours': ({ n }) => `${enCount(n, 'an hour', 'hours')} ago`,
    'time.yesterday': 'yesterday',
    'time.days': ({ n }) => `${enCount(n, 'a day', 'days')} ago`,
    'duration.short': 'under a minute',

    // Platforms and filters
    'source.local': 'Local',
    'filter.all': 'All',
    'filter.steam': 'Steam',
    'filter.xbox': 'Xbox',
    'filter.local': 'Local',

    // Home screen
    'game.play': 'Play',
    'game.back': 'Back to game',
    'game.starting': 'Starting',
    'game.moreOptions': 'More options',
    'fact.platform': 'Platform',
    'fact.lastPlayed': 'Last played',
    'fact.timePlayed': 'Time played',
    'fact.diskSpace': 'Disk space',
    'detail.emptyTitle': 'No games found',
    'detail.emptyText': 'Lumen looks for games installed through Steam and the Xbox app. Install a game, then refresh the library.',
    'detail.refresh': 'Refresh library',

    // Library
    'lib.count': ({ n }) => enCount(n, 'One game', 'games'),
    'lib.emptyNoFolders': 'You haven’t added any local games yet. Open Settings → Add a game, or Settings → Game folders.',
    'lib.emptyFilter': 'No games for this filter.',
    'lib.emptyAll': 'The library is empty. Install a game from Steam or the Xbox app, then refresh the library from the quick menu.',
    'scan.start': 'Looking for installed games…',
    'scan.done': ({ n }) => `Library updated: ${enCount(n, 'one game', 'games')}.`,

    // Platforms
    'plat.title': 'Your platforms',
    'plat.count': ({ n }) => enCount(n, 'One platform', 'platforms'),
    'plat.empty': 'No gaming platforms were found on this PC. Install Steam, the Epic Games Store, GOG Galaxy or another launcher and it will show up here.',
    'plat.opening': ({ name }) => `Opening ${name}…`,

    // Settings
    'settings.title': 'Settings',
    'settings.autostart': 'Start with Windows',
    'settings.autostart.desc': 'Lumen opens in full screen after you sign in',
    'settings.minimize': 'Minimize when a game starts',
    'settings.minimize.desc': 'Lumen comes back on its own after you close the game',
    'settings.pause': 'Pause the game in the menu',
    'settings.pause.desc': 'Freezes the game while Lumen is open over it, so button presses don’t reach the game',
    'settings.sounds': 'Menu sounds',
    'settings.music': 'Background music',
    'settings.music.desc': 'Add a music.mp3 file to the theme folder',
    'settings.onlineArt': 'Online artwork for Steam',
    'settings.onlineArt.desc': 'Downloads missing covers from the Steam cache',
    'settings.reduceMotion': 'Reduce animations',
    'settings.language': 'Language',
    'set.general': 'General',
    'set.library': 'Library',
    'set.audio': 'Audio',
    'set.appearance': 'Appearance',
    'settings.addGame': 'Add a game',
    'settings.addGame.desc': 'Pick the executable (.exe) of a game outside Steam and Xbox',
    'settings.folders': 'Game folders',
    'settings.folders.desc': 'Add the folders with games outside Steam and Xbox',
    'settings.hidden': 'Hidden games',
    'settings.hidden.desc': 'Bring removed games back to the library',
    'settings.theme': 'Open theme folder',
    'settings.theme.desc': 'Sounds, music, font and custom wallpaper',
    'language.title': 'Language',
    'language.note': 'Choose the language of the interface.',
    'language.changed': 'Language changed to English.',

    // Game management
    'add.done': ({ name }) => `${name} was added. Looking for artwork…`,
    'remove.title': ({ name }) => `Remove ${name}?`,
    'remove.noteManual': 'The game only disappears from Lumen; its files stay on disk.',
    'remove.noteOther': 'The game stays installed. You can bring it back from Settings → Hidden games.',
    'remove.confirm': 'Remove from library',
    'remove.done': ({ name }) => `${name} was removed from the library.`,
    'uninstall.title': ({ name }) => `Uninstall ${name}?`,
    'uninstall.note': 'The game and its files are deleted from disk.',
    'uninstall.confirm': 'Uninstall',
    'uninstall.progress': ({ name }) => `Uninstalling ${name}…`,
    'common.cancel': 'Cancel',
    'audio.title': 'Audio output',
    'audio.note': 'Choose where Windows and game sound plays.',
    'audio.none': 'No audio device found.',
    'audio.done': ({ name }) => `Sound now plays on ${name}.`,
    'bt.title': 'Bluetooth',
    'bt.note': 'Select a device to remove it, or add a new one.',
    'bt.none': 'No paired Bluetooth devices.',
    'bt.connected': 'Connected',
    'bt.paired': 'Paired',
    'bt.add': 'Add a device',
    'bt.add.desc': 'Put the device in pairing mode, then search',
    'bt.scanning': 'Searching…',
    'bt.scanning.note': 'Looking for devices in range. This takes a few seconds.',
    'bt.found.note': 'Select a device to pair with it.',
    'bt.found.none': 'No new devices found. Make sure the device is in pairing mode.',
    'bt.rescan': 'Search again',
    'bt.pairing': ({ name }) => `Pairing with ${name}…`,
    'bt.pairDone': ({ name }) => `${name} is paired.`,
    'bt.remove': 'Remove device',
    'bt.removeNote': 'It will have to be paired again to be used.',
    'bt.removed': ({ name }) => `${name} was removed.`,
    'hidden.title': 'Hidden games',
    'hidden.note': 'Pick a game to bring it back to the library.',
    'hidden.none': 'You haven’t hidden any games.',
    'hidden.restore': 'Bring back to library',
    'hidden.done': ({ name }) => `${name} is back in the library.`,
    'folders.title': 'Game folders',
    'folders.note': 'Each subfolder is treated as a game. Xbox games in these folders are already imported.',
    'folders.remove': 'Remove folder',
    'folders.removeTitle': 'Remove the folder?',
    'folders.removeConfirm': 'Remove',
    'folders.add': 'Add folder',
    'folders.add.desc': 'Opens the Windows folder picker',

    // Game options and closing
    'opt.close': 'Close game',
    'opt.closeNamed': ({ name }) => `Close ${name}`,
    'opt.runningNow': 'Running now',
    'opt.openLocation': 'Open game location',
    'opt.uninstall': 'Uninstall',
    'close.title': ({ name }) => `Close ${name}?`,
    'close.note': 'Unsaved progress will be lost.',
    'close.done': ({ name }) => `${name} was closed.`,

    // Power
    'power.title': 'Power',
    'power.sleep': 'Sleep',
    'power.restart': 'Restart PC',
    'power.restartTitle': 'Restart your PC?',
    'power.restartConfirm': 'Restart',
    'power.shutdown': 'Shut down PC',
    'power.shutdownTitle': 'Shut down your PC?',
    'power.shutdownConfirm': 'Shut down',
    'power.note': 'Open apps will close.',
    'power.exit': 'Exit to Windows',
    'power.exit.desc': 'Closes Lumen',

    // Quick menu
    'quick.title': 'Quick menu',
    'quick.home': 'Home',
    'quick.running': 'Running now',
    'quick.starting': 'Starting',

    // Launch and status
    'launch.starting': 'Starting…',
    'launch.running': 'Running',
    'launch.pending': ({ name }) => `${name} is starting.`,
    'launch.closeFirst': ({ name }) => `Close ${name} first.`,
    'launch.currentGame': 'the current game',
    'launch.notDetected': 'The game didn’t start within 3 minutes, so playtime wasn’t recorded.',
    'launch.played': ({ name, duration }) => `You played ${name} for ${duration}.`,
    'pad.handoff': 'Press PS to connect the controller to the PC’s Bluetooth.',
    'pad.playstation': 'PlayStation controller',
    'pad.xbox': 'Xbox controller',
    'battery.label': ({ percent, charging }) => `Battery ${percent}%${charging ? ', charging' : ''}`,
    'battery.padLabel': ({ name, percent, charging }) => `${name}: ${percent}%${charging ? ', charging' : ''}`,
    'user.initial': 'P',

    // Hints
    'hint.select': 'Select',
    'hint.back': 'Back',
    'hint.cancel': 'Cancel',
    'hint.menu': 'Menu',
    'hint.play': 'Play',
    'hint.options': 'Options',
    'hint.open': 'Open',
  },

  ro: {
    'tab.games': 'Jocuri',
    'tab.library': 'Bibliotecă',
    'tab.platforms': 'Platforme',
    'lib.title': 'Biblioteca ta',
    'aria.settings': 'Setări',

    'time.never': 'niciodată',
    'time.now': 'chiar acum',
    'time.minutes': ({ n }) => `acum ${roCount(n, 'un minut', 'minute')}`,
    'time.hours': ({ n }) => `acum ${roCount(n, 'o oră', 'ore')}`,
    'time.yesterday': 'ieri',
    'time.days': ({ n }) => `acum ${roCount(n, 'o zi', 'zile')}`,
    'duration.short': 'sub un minut',

    'source.local': 'Local',
    'filter.all': 'Toate',
    'filter.steam': 'Steam',
    'filter.xbox': 'Xbox',
    'filter.local': 'Locale',

    'game.play': 'Joacă',
    'game.back': 'Înapoi la joc',
    'game.starting': 'Se pornește',
    'game.moreOptions': 'Mai multe opțiuni',
    'fact.platform': 'Platformă',
    'fact.lastPlayed': 'Jucat ultima dată',
    'fact.timePlayed': 'Timp jucat',
    'fact.diskSpace': 'Spațiu ocupat',
    'detail.emptyTitle': 'Niciun joc găsit',
    'detail.emptyText': 'Lumen caută jocurile instalate prin Steam și prin aplicația Xbox. Instalează un joc, apoi reîmprospătează biblioteca.',
    'detail.refresh': 'Reîmprospătează biblioteca',

    'lib.count': ({ n }) => roCount(n, 'Un joc', 'jocuri'),
    'lib.emptyNoFolders': 'Nu ai adăugat încă jocuri locale. Deschide Setări → Adaugă un joc, sau Setări → Foldere cu jocuri.',
    'lib.emptyFilter': 'Niciun joc pentru acest filtru.',
    'lib.emptyAll': 'Biblioteca e goală. Instalează un joc din Steam sau din aplicația Xbox, apoi reîmprospătează biblioteca din meniul rapid.',
    'scan.start': 'Se caută jocurile instalate…',
    'scan.done': ({ n }) => `Biblioteca a fost actualizată: ${roCount(n, 'un joc', 'jocuri')}.`,

    'plat.title': 'Platformele tale',
    'plat.count': ({ n }) => roCount(n, 'O platformă', 'platforme'),
    'plat.empty': 'Nu am găsit nicio platformă de jocuri pe acest PC. Instalează Steam, Epic Games Store, GOG Galaxy sau alt launcher și va apărea aici.',
    'plat.opening': ({ name }) => `Se deschide ${name}…`,

    'settings.title': 'Setări',
    'settings.autostart': 'Pornește odată cu Windows',
    'settings.autostart.desc': 'Lumen se deschide în ecran complet după autentificare',
    'settings.minimize': 'Minimizează când pornește un joc',
    'settings.minimize.desc': 'Lumen revine singur după ce închizi jocul',
    'settings.pause': 'Pune jocul în pauză în meniu',
    'settings.pause.desc': 'Îngheață jocul cât Lumen e deschis peste el, ca butoanele să nu ajungă în joc',
    'settings.sounds': 'Sunete în meniu',
    'settings.music': 'Muzică de fundal',
    'settings.music.desc': 'Adaugă un fișier music.mp3 în folderul temei',
    'settings.onlineArt': 'Imagini online pentru Steam',
    'settings.onlineArt.desc': 'Descarcă coperțile care lipsesc din cache-ul Steam',
    'settings.reduceMotion': 'Reduce animațiile',
    'settings.language': 'Limbă',
    'set.general': 'General',
    'set.library': 'Bibliotecă',
    'set.audio': 'Audio',
    'set.appearance': 'Aspect',
    'settings.addGame': 'Adaugă un joc',
    'settings.addGame.desc': 'Alege executabilul (.exe) unui joc din afara Steam și Xbox',
    'settings.folders': 'Foldere cu jocuri',
    'settings.folders.desc': 'Adaugă folderele cu jocuri din afara Steam și Xbox',
    'settings.hidden': 'Jocuri ascunse',
    'settings.hidden.desc': 'Readu în bibliotecă jocurile eliminate',
    'settings.theme': 'Deschide folderul temei',
    'settings.theme.desc': 'Sunete, muzică, font și fundal personalizat',
    'language.title': 'Limbă',
    'language.note': 'Alege limba interfeței.',
    'language.changed': 'Limba a fost schimbată în română.',

    'add.done': ({ name }) => `${name} a fost adăugat. Se caută imaginile…`,
    'remove.title': ({ name }) => `Elimini ${name}?`,
    'remove.noteManual': 'Jocul dispare doar din Lumen; fișierele lui rămân pe disc.',
    'remove.noteOther': 'Jocul rămâne instalat. Îl poți readuce din Setări → Jocuri ascunse.',
    'remove.confirm': 'Elimină din bibliotecă',
    'remove.done': ({ name }) => `${name} a fost eliminat din bibliotecă.`,
    'uninstall.title': ({ name }) => `Dezinstalezi ${name}?`,
    'uninstall.note': 'Jocul și fișierele lui sunt șterse de pe disc.',
    'uninstall.confirm': 'Dezinstalează',
    'uninstall.progress': ({ name }) => `Se dezinstalează ${name}…`,
    'common.cancel': 'Anulează',
    'audio.title': 'Ieșire audio',
    'audio.note': 'Alege unde se aude sunetul de la Windows și de la jocuri.',
    'audio.none': 'Nu am găsit niciun dispozitiv audio.',
    'audio.done': ({ name }) => `Sunetul merge acum pe ${name}.`,
    'bt.title': 'Bluetooth',
    'bt.note': 'Alege un dispozitiv ca să-l elimini sau adaugă unul nou.',
    'bt.none': 'Niciun dispozitiv Bluetooth asociat.',
    'bt.connected': 'Conectat',
    'bt.paired': 'Asociat',
    'bt.add': 'Adaugă un dispozitiv',
    'bt.add.desc': 'Pune dispozitivul în modul de asociere, apoi caută',
    'bt.scanning': 'Se caută…',
    'bt.scanning.note': 'Se caută dispozitive în apropiere. Durează câteva secunde.',
    'bt.found.note': 'Alege un dispozitiv ca să te asociezi cu el.',
    'bt.found.none': 'Nu am găsit dispozitive noi. Verifică dacă dispozitivul este în modul de asociere.',
    'bt.rescan': 'Caută din nou',
    'bt.pairing': ({ name }) => `Se asociază cu ${name}…`,
    'bt.pairDone': ({ name }) => `${name} a fost asociat.`,
    'bt.remove': 'Elimină dispozitivul',
    'bt.removeNote': 'Va trebui asociat din nou ca să-l poți folosi.',
    'bt.removed': ({ name }) => `${name} a fost eliminat.`,
    'hidden.title': 'Jocuri ascunse',
    'hidden.note': 'Alege un joc ca să-l readuci în bibliotecă.',
    'hidden.none': 'Nu ai ascuns niciun joc.',
    'hidden.restore': 'Readu în bibliotecă',
    'hidden.done': ({ name }) => `${name} revine în bibliotecă.`,
    'folders.title': 'Foldere cu jocuri',
    'folders.note': 'Fiecare subfolder e considerat un joc. Jocurile Xbox din aceste foldere sunt deja importate.',
    'folders.remove': 'Elimină folderul',
    'folders.removeTitle': 'Elimini folderul?',
    'folders.removeConfirm': 'Elimină',
    'folders.add': 'Adaugă folder',
    'folders.add.desc': 'Se deschide fereastra Windows de alegere a folderului',

    'opt.close': 'Închide jocul',
    'opt.closeNamed': ({ name }) => `Închide ${name}`,
    'opt.runningNow': 'Rulează acum',
    'opt.openLocation': 'Deschide locația jocului',
    'opt.uninstall': 'Dezinstalează',
    'close.title': ({ name }) => `Închizi ${name}?`,
    'close.note': 'Progresul nesalvat se va pierde.',
    'close.done': ({ name }) => `${name} a fost închis.`,

    'power.title': 'Alimentare',
    'power.sleep': 'Intră în repaus',
    'power.restart': 'Repornește PC-ul',
    'power.restartTitle': 'Repornești PC-ul?',
    'power.restartConfirm': 'Repornește',
    'power.shutdown': 'Oprește PC-ul',
    'power.shutdownTitle': 'Oprești PC-ul?',
    'power.shutdownConfirm': 'Oprește',
    'power.note': 'Aplicațiile deschise se vor închide.',
    'power.exit': 'Ieși în Windows',
    'power.exit.desc': 'Închide Lumen',

    'quick.title': 'Meniu rapid',
    'quick.home': 'Acasă',
    'quick.running': 'Rulează acum',
    'quick.starting': 'Se pornește',

    'launch.starting': 'Se pornește…',
    'launch.running': 'Rulează',
    'launch.pending': ({ name }) => `${name} se pornește.`,
    'launch.closeFirst': ({ name }) => `Închide mai întâi ${name}.`,
    'launch.currentGame': 'jocul curent',
    'launch.notDetected': 'Jocul nu a pornit în 3 minute, așa că timpul de joc nu a fost înregistrat.',
    'launch.played': ({ name, duration }) => `Ai jucat ${name} ${duration}.`,
    'pad.handoff': 'Apasă PS ca să conectezi controlerul la Bluetooth-ul PC-ului.',
    'pad.playstation': 'Controler PlayStation',
    'pad.xbox': 'Controler Xbox',
    'battery.label': ({ percent, charging }) => `Baterie ${percent}%${charging ? ', se încarcă' : ''}`,
    'battery.padLabel': ({ name, percent, charging }) => `${name}: ${percent}%${charging ? ', se încarcă' : ''}`,
    'user.initial': 'J',

    'hint.select': 'Selectează',
    'hint.back': 'Înapoi',
    'hint.cancel': 'Anulează',
    'hint.menu': 'Meniu',
    'hint.play': 'Joacă',
    'hint.options': 'Opțiuni',
    'hint.open': 'Deschide',
  },
};

let lang = DEFAULT_LANG;

export const getLang = () => lang;
export const isSupported = (code) => LANGUAGES.some((l) => l.code === code);
export const locale = () => LANGUAGES.find((l) => l.code === lang)?.locale || 'en-US';

/** Switches the active language; unknown codes fall back to the default. */
export function setLang(code) {
  lang = isSupported(code) ? code : DEFAULT_LANG;
  document.documentElement.lang = lang;
}

/** Looks a key up in the active language, then in English, then returns the key itself. */
export function t(key, vars = {}) {
  const v = dict[lang]?.[key] ?? dict[DEFAULT_LANG][key] ?? key;
  if (typeof v === 'function') return v(vars);
  return v.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? vars[k] : m));
}

/** Fills the static page: `data-i18n` sets the text, `data-i18n-aria` sets aria-label. */
export function translateDocument(root = document) {
  root.querySelectorAll('[data-i18n]').forEach((n) => { n.textContent = t(n.dataset.i18n); });
  root.querySelectorAll('[data-i18n-aria]').forEach((n) => { n.setAttribute('aria-label', t(n.dataset.i18nAria)); });
}
