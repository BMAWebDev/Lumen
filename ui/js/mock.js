// Mock data for developing the interface in a browser (no backend).
// The games and images are made up and generated as SVG.

const listeners = {};
export function listen(event, cb) {
  (listeners[event] ||= []).push(cb);
  return Promise.resolve(() => {});
}
function emit(event, payload) {
  (listeners[event] || []).forEach((cb) => cb(payload));
}

const x = (t) => t.replace(/&/g, '&amp;').replace(/</g, '&lt;');
const uri = (svg) => 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);

function shapes(v, h1, h2, w, hgt) {
  const k = w / 600;
  switch (v % 4) {
    case 0: // mountain ridges
      return `<path d="M0 ${hgt * 0.72} L${w * 0.22} ${hgt * 0.42} L${w * 0.4} ${hgt * 0.6} L${w * 0.62} ${hgt * 0.3} L${w} ${hgt * 0.68} V${hgt} H0Z" fill="hsl(${h2} 60% 10%)" opacity=".85"/>
              <path d="M0 ${hgt * 0.82} L${w * 0.3} ${hgt * 0.62} L${w * 0.55} ${hgt * 0.78} L${w * 0.8} ${hgt * 0.58} L${w} ${hgt * 0.74} V${hgt} H0Z" fill="hsl(${h2} 70% 6%)"/>`;
    case 1: // rings
      return [1, 2, 3, 4].map((i) => `<circle cx="${w * 0.66}" cy="${hgt * 0.4}" r="${70 * i * k}" stroke="hsl(${h1} 90% 80%)" stroke-opacity="${0.5 - i * 0.1}" stroke-width="${6 * k}" fill="none"/>`).join('');
    case 2: // diagonal stripes
      return [0, 1, 2, 3, 4, 5].map((i) => `<rect x="${(i * 130 - 200) * k}" y="${-100 * k}" width="${46 * k}" height="${hgt * 1.6}" fill="hsl(${h1} 85% 70%)" opacity="${0.08 + i * 0.03}" transform="rotate(28 ${w / 2} ${hgt / 2})"/>`).join('');
    default: // sun on the horizon
      return `<circle cx="${w * 0.5}" cy="${hgt * 0.62}" r="${150 * k}" fill="hsl(${h1} 95% 70%)" opacity=".9"/>
              <rect y="${hgt * 0.62}" width="${w}" height="${hgt * 0.38}" fill="hsl(${h2} 70% 8%)"/>
              ${[0, 1, 2, 3].map((i) => `<rect x="0" y="${hgt * (0.66 + i * 0.07)}" width="${w}" height="${3 * k}" fill="hsl(${h1} 90% 70%)" opacity="${0.5 - i * 0.1}"/>`).join('')}`;
  }
}

function cover(name, h1, h2, v) {
  const size = name.length > 14 ? 44 : 56;
  return uri(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 600">
    <defs>
      <linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${h1} 70% 46%)"/><stop offset="1" stop-color="hsl(${h2} 75% 14%)"/></linearGradient>
      <radialGradient id="r" cx=".7" cy=".25" r=".7"><stop offset="0" stop-color="hsl(${h1} 95% 78%)" stop-opacity=".75"/><stop offset="1" stop-color="hsl(${h1} 95% 78%)" stop-opacity="0"/></radialGradient>
      <linearGradient id="f" x1="0" y1="0" x2="0" y2="1"><stop offset=".55" stop-color="#000" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".55"/></linearGradient>
    </defs>
    <rect width="600" height="600" fill="url(#g)"/><rect width="600" height="600" fill="url(#r)"/>
    ${shapes(v, h1, h2, 600, 600)}
    <rect width="600" height="600" fill="url(#f)"/>
    <text x="44" y="548" font-family="Segoe UI, Arial, sans-serif" font-size="${size}" font-weight="800" fill="#fff" letter-spacing="1">${x(name.toUpperCase())}</text>
  </svg>`);
}

function hero(h1, h2, v) {
  return uri(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1920 1080">
    <defs>
      <linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${h2} 70% 14%)"/><stop offset="1" stop-color="hsl(${h1} 65% 38%)"/></linearGradient>
      <radialGradient id="r" cx=".72" cy=".3" r=".55"><stop offset="0" stop-color="hsl(${h1} 95% 75%)" stop-opacity=".8"/><stop offset="1" stop-color="hsl(${h1} 95% 75%)" stop-opacity="0"/></radialGradient>
    </defs>
    <rect width="1920" height="1080" fill="url(#g)"/><rect width="1920" height="1080" fill="url(#r)"/>
    ${shapes(v, h1, h2, 1920, 1080)}
  </svg>`);
}

function logo(name) {
  return uri(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 300">
    <text x="0" y="200" font-family="Segoe UI, Arial, sans-serif" font-size="${Math.min(140, Math.round(1250 / name.length))}" font-weight="900" fill="#fff" letter-spacing="4">${x(name.toUpperCase())}</text>
    <rect x="4" y="232" width="220" height="10" fill="#fff" opacity=".8"/>
  </svg>`);
}

const DAY = 86400;
const now = Math.floor(Date.now() / 1000);
const DATA = [
  ['Ashfall Requiem', 'steam', 18, 250, 0, 2 * 3600, 54 * 3600, true],
  ['Neon Meridian', 'steam', 300, 240, 3, DAY * 1, 12 * 3600, true],
  ['Tidebound', 'xbox', 190, 215, 0, DAY * 3, 31 * 3600, false],
  ['Ironvale Tactics', 'steam', 32, 8, 2, DAY * 6, 140 * 3600, true],
  ['Starlight Courier', 'xbox', 265, 230, 1, DAY * 9, 7 * 3600, false],
  ['Hollow Crown', 'steam', 350, 280, 0, DAY * 15, 22 * 3600, true],
  ['Velocity Run', 'steam', 48, 20, 2, DAY * 40, 3 * 3600, false],
  ['Frostline', 'xbox', 200, 230, 0, DAY * 80, 9 * 3600, true],
  ['Kintsugi', 'steam', 42, 350, 3, 0, 0, false],
  ['Echo Protocol', 'steam', 170, 210, 1, 0, 0, true],
  ['Saltmarsh', 'xbox', 150, 190, 0, 0, 0, false],
  ['Orbital Gardens', 'steam', 120, 260, 1, 0, 0, true],
  ['Last Lantern', 'steam', 28, 280, 3, 0, 0, false],
  ['Cinder & Sage', 'xbox', 15, 340, 2, 0, 0, false],
  ['Paper Skies', 'steam', 205, 245, 3, 0, 0, true],
  ['Moth & Mirror', 'steam', 290, 330, 1, 0, 0, false],
  ['Driftwood Atlas', 'local', 35, 200, 0, 0, 0, false],
];

const games = DATA.map(([name, source, h1, h2, v, ago, play, hasLogo], i) => ({
  id: `${source}:${i}`,
  source,
  name,
  installDir: `C:\\Games\\${name}`,
  launchUri: '',
  cover: [cover(name, h1, h2, v)],
  hero: i % 5 === 4 ? [] : [hero(h1, h2, v)],
  logo: hasLogo ? [logo(name)] : [],
  sizeBytes: (8 + i * 3.7) * 1e9,
}));
const stats = {};
DATA.forEach(([, , , , , ago, play], i) => {
  if (ago) stats[games[i].id] = { playtimeSec: play, lastPlayed: now - ago, launches: 3 + i };
});
const settings = { minimizeOnLaunch: true, pauseGame: true, sounds: true, music: true, onlineArt: true, reduceMotion: false, autostart: false, language: 'en', gameFolders: [] };
let running = null;
let audioOut = 'a';
const AUDIO = [['a', 'Speakers (Realtek Audio)'], ['b', 'Headphones (Razer Kraken)'], ['c', 'LG TV (NVIDIA High Definition Audio)']];
let btPaired = [{ id: 'bt1', name: 'DualSense Wireless Controller', connected: true, paired: true }, { id: 'bt2', name: 'Razer Kraken BT', connected: false, paired: true }];
const btNearby = [{ id: 'bt3', name: 'Xbox Wireless Controller', connected: false, paired: false }, { id: 'bt4', name: 'JBL Flip 6', connected: false, paired: false }];
let hidden = [];
const visible = () => games.filter((g) => !hidden.some((h) => h.id === g.id));

export async function invoke(cmd, args) {
  switch (cmd) {
    case 'get_state':
      return { games: visible(), stats, settings: { ...settings }, theme: {}, themeDir: 'C:\\Users\\you\\AppData\\Roaming\\dev.lumen.launcher\\theme', running, user: 'Andrei', version: '0.1.0' };
    case 'scan_library':
      setTimeout(() => emit('library', { games: visible(), warnings: [] }), 700);
      return;
    case 'launch_game':
      running = { id: args.id, started: false };
      setTimeout(() => { if (running) { running.started = true; emit('game-started', { id: args.id, seconds: 0, detected: true }); } }, 1800);
      setTimeout(() => { if (running?.started) { running = null; emit('game-exited', { id: args.id, seconds: 4380, detected: true }); } }, 9000);
      return;
    case 'cancel_launch':
      if (running && !running.started) running = null;
      return;
    case 'resume_game':
      if (!running?.started) throw new Error('No game is running.');
      return;
    case 'close_game':
      if (!running) return false;
      // Launch not detected yet: the interface clears its own state.
      if (!running.started) { running = null; return true; }
      { const id = running.id; running = null; setTimeout(() => emit('game-exited', { id, seconds: 1260, detected: true }), 400); }
      return false;
    case 'set_language':
      settings.language = args.language;
      return { ...settings };
    case 'set_setting':
      settings[args.key] = args.value;
      return { ...settings };
    case 'add_game_folder':
      if (!settings.gameFolders.includes('D:\\Games')) settings.gameFolders.push('D:\\Games');
      return { ...settings, gameFolders: [...settings.gameFolders] };
    case 'remove_game_folder':
      settings.gameFolders = settings.gameFolders.filter((f) => f !== args.path);
      return { ...settings, gameFolders: [...settings.gameFolders] };
    case 'add_game': {
      // A manually added game, without a cover, only with the executable's icon.
      if (games.some((g) => g.id === 'manual:lantern')) throw new Error('Paper Lantern is already in the library.');
      games.push({
        id: 'manual:lantern', source: 'local', name: 'Paper Lantern', installDir: 'D:\\Games\\Paper Lantern',
        launchUri: '', launchArgs: [], cover: [], hero: [], logo: [], sizeBytes: 3.2e9,
        icon: uri('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="8" y="8" width="48" height="48" rx="12" fill="#f5b84a"/><path d="M32 16c8 8 10 16 0 32-10-16-8-24 0-32z" fill="#7a3a00"/></svg>'),
      });
      return 'Paper Lantern';
    }
    case 'remove_game': {
      const i = games.findIndex((g) => g.id === args.id);
      if (i < 0) return;
      if (args.id.startsWith('manual:')) games.splice(i, 1);
      else hidden.push({ id: args.id, name: games[i].name });
      return;
    }
    case 'uninstall_game': {
      const g = games.find((x) => x.id === args.id);
      if (!g) throw new Error('The game is no longer in the library.');
      if (g.source === 'local') throw new Error('The game has no uninstaller. Open the game location and delete the folder manually.');
      if (g.source === 'steam') return { done: false, message: 'Confirm the uninstall in the Steam window, then refresh the library.' };
      games.splice(games.indexOf(g), 1);
      return { done: true, message: `${g.name} was uninstalled.` };
    }
    case 'detect_platforms':
      return [['steam', 'Steam'], ['epic', 'Epic Games Store'], ['gog', 'GOG Galaxy'], ['ea', 'EA app'], ['battlenet', 'Battle.net'], ['xbox', 'Xbox']].map(([id, name]) => ({ id, name, icon: id === 'gog' || id === 'ea' ? null : uri(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="6" y="6" width="52" height="52" rx="12" fill="#fff"/><text x="32" y="43" text-anchor="middle" font-size="32" font-weight="800" fill="#223">${name[0]}</text></svg>`) }));
    case 'open_platform':
      return;
    case 'hidden_games':
      return [...hidden];
    case 'restore_game':
      hidden = hidden.filter((h) => h.id !== args.id);
      return;
    case 'audio_outputs':
      return AUDIO.map(([id, name]) => ({ id, name, current: id === audioOut }));
    case 'set_audio_output':
      audioOut = args.id;
      return;
    case 'bluetooth_paired':
      return btPaired.map((d) => ({ ...d }));
    case 'bluetooth_scan':
      await new Promise((r) => setTimeout(r, 1500));
      return btNearby.filter((d) => !btPaired.some((p) => p.id === d.id));
    case 'bluetooth_pair':
      btPaired.push({ ...btNearby.find((d) => d.id === args.id), paired: true });
      return;
    case 'bluetooth_remove':
      btPaired = btPaired.filter((d) => d.id !== args.id);
      return;
    case 'theme_file':
      throw new Error('no theme');
    case 'clock_24h':
      return true;
    case 'battery':
      return { percent: 64, charging: false, plugged: false };
    case 'pad_batteries':
      return [{ kind: 'playstation', percent: 75, charging: false }];
    default:
      return null;
  }
}

// Lets you simulate the Guide button from the console: lumenGuide()
window.lumenGuide = () => emit('guide', null);
