import { call, on, fileUrl, isTauri } from './api.js';
import * as sound from './sound.js';
import { createInput } from './input.js';
import { icons } from './icons.js';
import { t, setLang, getLang, locale, translateDocument, LANGUAGES } from './i18n.js';

// ======================================================================
// Utilities
// ======================================================================
const $ = (s, r = document) => r.querySelector(s);
const el = (tag, cls, html) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (html != null) e.innerHTML = html;
  return e;
};
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const nextFrame = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
let remPx = 10;
const measureRem = () => { remPx = parseFloat(getComputedStyle(document.documentElement).fontSize) || 10; };

function hue(str) {
  let h = 0;
  for (const c of str) h = (h * 31 + c.charCodeAt(0)) % 360;
  return h;
}

function relTime(ts) {
  if (!ts) return t('time.never');
  const d = Math.max(0, Date.now() / 1000 - ts);
  if (d < 60) return t('time.now');
  if (d < 3600) return t('time.minutes', { n: Math.floor(d / 60) });
  if (d < 86400) return t('time.hours', { n: Math.floor(d / 3600) });
  if (d < 2 * 86400) return t('time.yesterday');
  if (d < 30 * 86400) return t('time.days', { n: Math.floor(d / 86400) });
  return new Date(ts * 1000).toLocaleDateString(locale(), { day: 'numeric', month: 'short', year: 'numeric' });
}

function duration(sec) {
  if (!sec || sec < 60) return t('duration.short');
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  if (!h) return `${m} min`;
  return m ? `${h} h ${m} min` : `${h} h`;
}

function bytes(n) {
  if (!n) return '';
  const gb = n / 1024 ** 3;
  if (gb < 1) return `${Math.round(n / 1024 ** 2)} MB`;
  const digits = gb >= 100 ? 0 : 1;
  return `${gb.toLocaleString(locale(), { minimumFractionDigits: digits, maximumFractionDigits: digits })} GB`;
}

const sourceName = (src) => (src === 'local' ? t('source.local') : { steam: 'Steam', xbox: 'Xbox' }[src] || src);
// The clock follows the Windows setting (24-hour or 12-hour); until it is known, the language's own default applies.
let clock24 = null;
const timeNow = () => new Date().toLocaleTimeString(locale(), {
  hour: '2-digit', minute: '2-digit', ...(clock24 == null ? {} : { hourCycle: clock24 ? 'h23' : 'h12' }),
});
async function syncClockFormat() {
  try { clock24 = await call('clock_24h'); } catch { clock24 = null; }
  tickClock();
}

/** Tries each candidate image in turn until one loads. */
function setSrc(img, list, onFail) {
  const urls = (list || []).map(fileUrl).filter(Boolean);
  let i = 0;
  img.onload = () => img.classList.add('ok');
  img.onerror = () => {
    i++;
    if (i < urls.length) img.src = urls[i];
    else { img.onerror = null; img.removeAttribute('src'); onFail?.(); }
  };
  if (urls.length) img.src = urls[0];
  else onFail?.();
}

/** Finds the first URL that loads (the result is cached). */
const probeCache = new Map();
function firstLoadable(list) {
  const key = (list || []).join('|');
  if (probeCache.has(key)) return probeCache.get(key);
  const p = (async () => {
    for (const raw of list || []) {
      const url = fileUrl(raw);
      if (!url) continue;
      const ok = await new Promise((res) => {
        const im = new Image();
        im.decoding = 'async';
        im.onload = () => res(true);
        im.onerror = () => res(false);
        im.src = url;
      });
      if (ok) return url;
    }
    return null;
  })();
  probeCache.set(key, p);
  return p;
}

// ======================================================================
// State
// ======================================================================
const S = {
  games: [],
  favorites: [], // ids of the games shown on the Favorites tab
  stats: {},
  settings: {},
  theme: {},
  themeDir: '',
  running: null,
  user: '',
  version: '',
  update: null, // { version, url } when a newer release exists
  updating: false,
  os: null, // { count, titles, rebootRequired } from Windows Update
  view: 'home',
  zone: 'row',
  row: [],
  rowEls: [],
  rowIndex: 0,
  actions: [],
  actionIndex: 0,
  tabIndex: 0,
  filter: 'all',
  filterIndex: 0,
  libList: [],
  libEls: [],
  libIndex: 0,
  gridScroll: 0,
  platforms: [],
  platIndex: 0,
  platEls: [],
  setCat: 0,
  setIndex: 0,
  setItems: [],
  setEls: [],
  device: 'keyboard',
  pad: 'xbox',
  booting: true,
};
const VIEWS = ['home', 'library', 'platforms']; // the views with a tab; Settings is a page of its own, opened from the gear
const ALL_VIEWS = [...VIEWS, 'settings'];
const stack = []; // open overlays (quick menu, panels, launch screen)

const stat = (id) => S.stats[id] || { playtimeSec: 0, lastPlayed: 0, launches: 0 };
const gameById = (id) => S.games.find((g) => g.id === id);
const curGame = () => S.row[S.rowIndex] || null;

const isFavorite = (id) => S.favorites.includes(id);

/** The games on the Favorites tab, the most recently played first. */
function favoriteGames() {
  return S.games
    .filter((g) => isFavorite(g.id))
    .sort((a, b) => (stat(b.id).lastPlayed - stat(a.id).lastPlayed) || a.name.localeCompare(b.name, locale()));
}

// ======================================================================
// Cross-fading background
// ======================================================================
const layers = [...document.querySelectorAll('.bg-layer')];
let front = 0;
let bgToken = 0;
let bgTimer = null;
let bgKey = null;

function bgSources(game) {
  if (!game) return { list: S.theme.wallpaper ? [S.theme.wallpaper] : [], mode: 'wallpaper' };
  if (game.hero?.length) return { list: [...game.hero], fallback: game.cover, mode: 'hero' };
  return { list: game.cover || [], mode: 'from-cover' };
}

function setBackground(game) {
  const key = game ? game.id : '__none';
  if (key === bgKey) return;
  bgKey = key;
  const token = ++bgToken;
  clearTimeout(bgTimer);
  // A short delay: when scrolling fast we don't load every intermediate background.
  bgTimer = setTimeout(async () => {
    const src = bgSources(game);
    let mode = src.mode;
    let url = await firstLoadable(src.list);
    if (!url && src.fallback) { url = await firstLoadable(src.fallback); mode = 'from-cover'; }
    if (token !== bgToken) return;
    const back = layers[1 - front];
    const img = back.querySelector('img');
    back.classList.remove('show');
    back.classList.toggle('from-cover', mode === 'from-cover');
    back.classList.toggle('wallpaper', mode === 'wallpaper');
    if (url) {
      img.src = url;
      try { await img.decode(); } catch { /* show it anyway */ }
    } else {
      img.removeAttribute('src');
    }
    if (token !== bgToken) return;
    back.classList.add('show');
    layers[front].classList.remove('show');
    front = 1 - front;
  }, 90);
}

// ======================================================================
// Home screen: the row of games
// ======================================================================
const rowTrack = $('#row-track');
const rowLabel = $('#row-label');

function tileArt(game) {
  const art = el('div', 'art');
  const ph = el('div', 'ph', esc(game.name));
  ph.style.setProperty('--h', hue(game.id));
  // Local games without a cover get the executable's icon, centered.
  if (game.icon) {
    const ic = el('i', 'ph-icon');
    ic.style.backgroundImage = `url("${fileUrl(game.icon).replace(/"/g, '%22')}")`;
    ph.append(ic);
  }
  const img = new Image();
  img.alt = '';
  img.decoding = 'async';
  img.draggable = false;
  art.append(ph, img);
  setSrc(img, game.cover);
  return art;
}

function renderRow(keepId) {
  S.row = favoriteGames();
  rowTrack.textContent = '';
  S.rowEls = S.row.map((g, i) => {
    const tile = el('div', 'tile');
    tile.append(tileArt(g));
    tile.style.setProperty('--d', `${i * 40}ms`);
    tile.addEventListener('click', () => clickRow(i));
    rowTrack.appendChild(tile);
    return tile;
  });

  const idx = keepId ? S.row.findIndex((g) => g.id === keepId) : -1;
  S.rowIndex = idx >= 0 ? idx : clamp(S.rowIndex, 0, S.rowEls.length - 1);
  layoutRow(true);
  onRowFocus(true);
}

function layoutRow(instant) {
  const r = remPx;
  const small = 12 * r, big = 18 * r, gap = 1.6 * r, pad = 7 * r;
  const n = S.rowEls.length;
  if (!n) { rowLabel.dataset.id = ''; rowLabel.textContent = ''; return; }
  const f = S.rowIndex;
  const xs = [];
  let x = 0;
  for (let i = 0; i < n; i++) { xs.push(x); x += (i === f ? big : small) + gap; }
  // Keep a single cover visible before the selected one, like on a console.
  const scroll = Math.max(0, xs[f] - (small + gap));
  if (instant) rowTrack.classList.add('instant');
  S.rowEls.forEach((tile, i) => {
    const s = i === f ? 1 : small / big;
    tile.style.transform = `translate3d(${pad + xs[i] - scroll}px,0,0) scale(${s})`;
    tile.classList.toggle('past', i < f - 1);
  });
  const g = curGame();
  rowLabel.style.transform = `translate3d(${pad + xs[f] - scroll + big + gap * 1.4}px, ${small + 1.6 * r}px, 0)`;
  if (rowLabel.dataset.id !== (g?.id || '')) {
    rowLabel.dataset.id = g?.id || '';
    rowLabel.classList.add('swap');
    requestAnimationFrame(() => {
      rowLabel.textContent = g ? g.name : '';
      rowLabel.classList.remove('swap');
    });
  }
  if (instant) { void rowTrack.offsetWidth; rowTrack.classList.remove('instant'); }
}

function onRowFocus(instant) {
  if (!instant) layoutRow(false);
  renderDetail();
  setBackground(curGame());
  // Preload the neighbors' backgrounds so the transition is instant.
  [S.rowIndex - 1, S.rowIndex + 1].forEach((i) => { const g = S.row[i]; if (g) firstLoadable(g.hero?.length ? g.hero : g.cover); });
  updateFocus();
}

function bump(node, d) {
  node?.querySelector('.art')?.animate(
    [{ transform: 'translateX(0)' }, { transform: `translateX(${d * 0.8}rem)` }, { transform: 'translateX(0)' }],
    { duration: 200, easing: 'ease-out' },
  );
}

function clickRow(i) {
  if (stack.length) return;
  if (S.view !== 'home') showView('home');
  if (S.zone === 'row' && S.rowIndex === i) { homeAction('confirm'); return; }
  S.zone = 'row';
  S.rowIndex = i;
  sound.play('move');
  onRowFocus();
}

// ======================================================================
// Details of the selected game
// ======================================================================
const detail = $('#detail');
let detailId = null;

function renderDetail(force) {
  const g = curGame();
  const id = g ? g.id : '__empty';
  if (id === detailId && !force) return;
  const changed = id !== detailId;
  detailId = id;

  const build = () => {
    detail.textContent = '';
    S.actions = [];
    if (g) {
      const head = el('div');
      if (g.logo?.length) {
        const img = new Image();
        img.className = 'detail-logo';
        img.alt = g.name;
        const title = el('h1', 'detail-title', esc(g.name));
        img.onerror = null;
        firstLoadable(g.logo).then((url) => {
          if (detailId !== g.id) return;
          if (url) { img.src = url; head.replaceChildren(img); } else head.replaceChildren(title);
        });
        head.append(title);
        title.style.visibility = 'hidden';
        setTimeout(() => { title.style.visibility = ''; }, 140);
      } else {
        head.append(el('h1', 'detail-title', esc(g.name)));
      }
      detail.append(head);

      const isRunning = S.running?.id === g.id;
      S.actions.push({
        label: isRunning ? (S.running.started ? t('game.back') : t('game.starting')) : t('game.play'),
        cls: 'primary' + (isRunning && !S.running.started ? ' muted' : ''),
        icon: isRunning && !S.running.started ? '' : icons.play,
        run: () => launch(g),
      });
      S.actions.push({ label: '', aria: t('game.moreOptions'), cls: 'round', icon: icons.more, run: () => openGameOptions(g, 'favorites') });

      const st = stat(g.id);
      const facts = [
        [t('fact.platform'), `<span class="platform ${g.source}">${esc(sourceName(g.source))}</span>`],
        [t('fact.lastPlayed'), esc(relTime(st.lastPlayed))],
      ];
      if (st.playtimeSec) facts.push([t('fact.timePlayed'), esc(duration(st.playtimeSec))]);
      if (g.sizeBytes) facts.push([t('fact.diskSpace'), esc(bytes(g.sizeBytes))]);
      renderActions();
      detail.append(el('dl', 'facts', facts.map(([k, v]) => `<div><dt>${esc(k)}</dt><dd>${v}</dd></div>`).join('')));
    } else {
      if (S.games.length) {
        // There are games, but none was added to Favorites yet.
        detail.append(el('h1', 'detail-title', esc(t('fav.emptyTitle'))));
        detail.append(el('p', 'detail-text', esc(t('fav.emptyText'))));
        S.actions.push({ label: t('fav.openLibrary'), cls: 'primary', run: () => showView('library') });
      } else {
        detail.append(el('h1', 'detail-title', esc(t('detail.emptyTitle'))));
        detail.append(el('p', 'detail-text', esc(t('detail.emptyText'))));
        S.actions.push({ label: t('detail.refresh'), cls: 'primary', run: () => rescan(true) });
      }
      renderActions();
    }
    S.actionIndex = clamp(S.actionIndex, 0, S.actions.length - 1);
    updateFocus();
  };

  if (changed && !S.booting) {
    detail.classList.add('swap');
    clearTimeout(renderDetail.t);
    renderDetail.t = setTimeout(() => { build(); detail.classList.remove('swap'); }, 120);
  } else {
    build();
  }
}

function renderActions() {
  const box = el('div', 'actions');
  S.actions.forEach((a, i) => {
    const b = el('button', `btn ${a.cls}`, `${a.icon || ''}${a.label ? `<span>${esc(a.label)}</span>` : ''}`);
    if (a.aria) b.setAttribute('aria-label', a.aria);
    b.addEventListener('click', () => {
      if (stack.length) return;
      S.zone = 'actions';
      S.actionIndex = i;
      updateFocus();
      sound.play('confirm');
      a.run();
    });
    a.el = b;
    box.appendChild(b);
  });
  detail.appendChild(box);
}

// ======================================================================
// Library
// ======================================================================
const FILTERS = ['all', 'steam', 'xbox', 'local'];
const grid = $('#grid');
const filtersEl = $('#filters');

function renderFilters() {
  filtersEl.textContent = '';
  FILTERS.forEach((key, i) => {
    const b = el('button', 'chip' + (S.filter === key ? ' active' : ''), esc(t(`filter.${key}`)));
    b.addEventListener('click', () => { S.filterIndex = i; applyFilter(); S.zone = 'filters'; updateFocus(); });
    filtersEl.appendChild(b);
  });
}

function applyFilter() {
  const key = FILTERS[S.filterIndex];
  if (S.filter === key) return;
  S.filter = key;
  sound.play('move');
  renderFilters();
  renderLibrary();
}

function renderLibrary(keepId) {
  S.libList = S.games.filter((g) => S.filter === 'all' || g.source === S.filter);
  grid.textContent = '';
  S.libEls = S.libList.map((g, i) => {
    const tile = el('div', 'gtile');
    const art = el('div', 'art');
    const clip = tileArt(g);
    clip.className = 'clip';
    clip.querySelector('img').loading = 'lazy';
    art.appendChild(clip);
    tile.append(art, el('div', 'name', esc(g.name)));
    tile.addEventListener('click', () => {
      if (stack.length) return;
      if (S.zone === 'grid' && S.libIndex === i) { sound.play('confirm'); launch(g); return; }
      S.zone = 'grid'; S.libIndex = i; sound.play('move'); updateFocus(); scrollGrid();
    });
    grid.appendChild(tile);
    return tile;
  });
  if (!S.libList.length) {
    const noFolders = S.filter === 'local' && !S.settings.gameFolders?.length;
    grid.appendChild(el('div', 'empty', esc(t(noFolders
      ? 'lib.emptyNoFolders'
      : S.games.length ? 'lib.emptyFilter' : 'lib.emptyAll'))));
  }
  const n = S.libList.length;
  $('#lib-count').textContent = n ? t('lib.count', { n }) : '';
  const idx = keepId ? S.libList.findIndex((g) => g.id === keepId) : -1;
  S.libIndex = idx >= 0 ? idx : clamp(S.libIndex, 0, Math.max(0, n - 1));
  S.gridScroll = 0;
  grid.style.transform = '';
  if (S.zone === 'grid' && !n) S.zone = 'filters';
  updateFocus();
  scrollGrid();
}

function gridCols(g = grid) {
  const cols = getComputedStyle(g).gridTemplateColumns.split(' ').filter(Boolean).length;
  return Math.max(1, cols);
}

function scrollGrid() {
  const tile = S.libEls[S.libIndex];
  if (!tile) return;
  const wrapH = $('#grid-wrap').clientHeight;
  const top = tile.offsetTop;
  const h = tile.offsetHeight;
  const margin = 3 * remPx;
  let s = S.gridScroll;
  if (top - s < margin) s = Math.max(0, top - margin);
  else if (top + h - s > wrapH - 9 * remPx) s = top + h - wrapH + 9 * remPx;
  if (S.libIndex < gridCols()) s = 0;
  if (s !== S.gridScroll) {
    S.gridScroll = s;
    grid.style.transform = `translate3d(0, ${-s}px, 0)`;
  }
}

// ======================================================================
// Platforms: the gaming launchers and stores found on this PC
// ======================================================================
const platGrid = $('#plat-grid');
let platLoaded = false;

function renderPlatforms() {
  platGrid.textContent = '';
  S.platEls = S.platforms.map((p, i) => {
    const tile = el('div', 'gtile');
    const art = el('div', 'art');
    const clip = el('div', 'clip');
    const face = el('div', 'ptile');
    face.style.setProperty('--h', hue(p.id));
    if (p.icon) {
      // The platform's real icon, centered like the icon of a local game.
      const ic = el('i', 'ph-icon');
      ic.style.backgroundImage = `url("${fileUrl(p.icon).replace(/"/g, '%22')}")`;
      face.appendChild(ic);
    } else {
      face.dataset.initial = p.name.charAt(0).toUpperCase();
    }
    clip.appendChild(face);
    art.appendChild(clip);
    tile.append(art, el('div', 'name', esc(p.name)));
    tile.addEventListener('click', () => {
      if (stack.length) return;
      if (S.zone === 'plat' && S.platIndex === i) { sound.play('confirm'); openPlatform(p); return; }
      S.zone = 'plat'; S.platIndex = i; sound.play('move'); updateFocus();
    });
    platGrid.appendChild(tile);
    return tile;
  });
  if (!S.platforms.length && platLoaded) platGrid.appendChild(el('div', 'empty', esc(t('plat.empty'))));
  const n = S.platforms.length;
  $('#plat-count').textContent = n ? t('plat.count', { n }) : '';
  S.platIndex = clamp(S.platIndex, 0, Math.max(0, n - 1));
  updateFocus();
}

async function loadPlatforms() {
  try { S.platforms = (await call('detect_platforms')) || []; } catch (e) { S.platforms = []; toast(String(e), 'error'); }
  platLoaded = true;
  renderPlatforms();
  if (S.view === 'platforms' && S.zone === 'tabs' && S.tabIndex === 2 && S.platforms.length) S.zone = 'plat';
  updateFocus();
}

async function openPlatform(p) {
  try {
    await call('open_platform', { id: p.id });
    toast(t('plat.opening', { name: p.name }));
  } catch (e) { toast(String(e), 'error'); }
}

function platformsAction(a) {
  const cols = gridCols(platGrid);
  const n = S.platforms.length;
  let ni = S.platIndex;
  if (a === 'left') ni = Math.max(0, S.platIndex - 1);
  else if (a === 'right') ni = Math.min(n - 1, S.platIndex + 1);
  else if (a === 'down') ni = S.platIndex + cols < n ? S.platIndex + cols : (Math.floor(S.platIndex / cols) < Math.floor((n - 1) / cols) ? n - 1 : S.platIndex);
  else if (a === 'up') {
    if (S.platIndex < cols) { goTabs(2); return; }
    ni = S.platIndex - cols;
  } else if (a === 'confirm') { const p = S.platforms[S.platIndex]; if (p) { sound.play('confirm'); openPlatform(p); } return; }
  else if (a === 'back') { sound.play('back'); showView('home'); return; }
  if (ni !== S.platIndex) { S.platIndex = ni; sound.play('move'); updateFocus(); }
  else if (a === 'left' || a === 'right') bump(S.platEls[S.platIndex], a === 'left' ? -1 : 1);
}

// ======================================================================
// Focus and views
// ======================================================================
const tabEls = () => [$('#tab-home'), $('#tab-library'), $('#tab-platforms'), $('#btn-settings')];

function updateFocus() {
  document.querySelectorAll('.focused').forEach((n) => n.classList.remove('focused'));
  document.body.dataset.zone = S.zone;
  document.body.dataset.view = S.view;
  let target = null;
  if (S.zone === 'tabs') target = tabEls()[S.tabIndex];
  else if (S.zone === 'row') target = S.rowEls[S.rowIndex];
  else if (S.zone === 'actions') target = S.actions[S.actionIndex]?.el;
  else if (S.zone === 'filters') target = filtersEl.children[S.filterIndex];
  else if (S.zone === 'grid') target = S.libEls[S.libIndex];
  else if (S.zone === 'plat') target = S.platEls[S.platIndex];
  else if (S.zone === 'scat') target = setCatsEl.children[S.setCat];
  else if (S.zone === 'sitems') target = S.setEls[S.setIndex];
  target?.classList.add('focused');
  if (S.zone === 'sitems') target?.scrollIntoView({ block: 'nearest' });
  updateHints();
}

function showView(view) {
  if (S.view === view) return;
  S.view = view;
  for (const v of ALL_VIEWS) {
    $(`#${v}`).classList.toggle('hidden', view !== v);
    $(`#tab-${v}`)?.classList.toggle('active', view === v);
  }
  if (view === 'settings') {
    S.zone = 'scat';
    S.tabIndex = 3;
    S.setIndex = 0;
    renderSettings();
  } else if (view === 'library') {
    S.zone = S.libList.length ? 'grid' : 'filters';
    scrollGrid();
  } else if (view === 'platforms') {
    S.zone = S.platforms.length ? 'plat' : 'tabs';
    S.tabIndex = 2;
    loadPlatforms();
  } else {
    S.zone = 'row';
    setBackground(curGame());
  }
  updateFocus();
}

// ======================================================================
// Handling actions (keyboard / controller)
// ======================================================================
function homeAction(a) {
  if (S.zone === 'row') {
    if (a === 'left' || a === 'right') {
      const d = a === 'left' ? -1 : 1;
      const ni = clamp(S.rowIndex + d, 0, S.rowEls.length - 1);
      if (ni !== S.rowIndex) { S.rowIndex = ni; sound.play('move'); onRowFocus(); } else bump(S.rowEls[S.rowIndex], d);
    } else if (a === 'down') {
      if (S.actions.length) { S.zone = 'actions'; S.actionIndex = 0; sound.play('move'); updateFocus(); }
    } else if (a === 'up') {
      goTabs(0);
    } else if (a === 'confirm') {
      const g = curGame();
      sound.play('confirm');
      if (g) launch(g); else rescan(true);
    } else if (a === 'options') {
      const g = curGame();
      if (g) { sound.play('confirm'); openGameOptions(g, 'favorites'); }
    } else if (a === 'back' && S.rowIndex !== 0) {
      S.rowIndex = 0; sound.play('back'); onRowFocus();
    }
  } else if (S.zone === 'actions') {
    if (a === 'left' || a === 'right') {
      const ni = clamp(S.actionIndex + (a === 'left' ? -1 : 1), 0, S.actions.length - 1);
      if (ni !== S.actionIndex) { S.actionIndex = ni; sound.play('move'); updateFocus(); }
    } else if (a === 'up' || a === 'back') {
      S.zone = 'row'; sound.play(a === 'back' ? 'back' : 'move'); updateFocus();
    } else if (a === 'confirm') {
      sound.play('confirm'); S.actions[S.actionIndex]?.run();
    } else if (a === 'options') {
      const g = curGame(); if (g) openGameOptions(g, 'favorites');
    }
  }
}

function libraryAction(a) {
  if (S.zone === 'grid') {
    const cols = gridCols();
    const n = S.libList.length;
    let ni = S.libIndex;
    if (a === 'left') ni = Math.max(0, S.libIndex - 1); // from the start of a row it continues at the end of the previous one
    else if (a === 'right') ni = Math.min(n - 1, S.libIndex + 1);
    else if (a === 'down') ni = S.libIndex + cols < n ? S.libIndex + cols : (Math.floor(S.libIndex / cols) < Math.floor((n - 1) / cols) ? n - 1 : S.libIndex);
    else if (a === 'up') {
      if (S.libIndex < cols) { S.zone = 'filters'; sound.play('move'); updateFocus(); return; }
      ni = S.libIndex - cols;
    } else if (a === 'confirm') { const g = S.libList[S.libIndex]; if (g) { sound.play('confirm'); launch(g); } return; }
    else if (a === 'options') { const g = S.libList[S.libIndex]; if (g) { sound.play('confirm'); openGameOptions(g, 'library'); } return; }
    else if (a === 'back') { sound.play('back'); showView('home'); return; }
    if (ni !== S.libIndex) { S.libIndex = ni; sound.play('move'); updateFocus(); scrollGrid(); }
    else if (a === 'left' || a === 'right') bump(S.libEls[S.libIndex], a === 'left' ? -1 : 1);
  } else if (S.zone === 'filters') {
    if (a === 'left' || a === 'right') {
      const ni = clamp(S.filterIndex + (a === 'left' ? -1 : 1), 0, FILTERS.length - 1);
      if (ni !== S.filterIndex) { S.filterIndex = ni; applyFilter(); updateFocus(); }
    } else if (a === 'down' || a === 'confirm') {
      if (S.libList.length) { S.zone = 'grid'; sound.play('move'); updateFocus(); }
    } else if (a === 'up') goTabs(1);
    else if (a === 'back') { sound.play('back'); showView('home'); }
  }
}

/** The first or last index of the grid row that holds `index`. */
const rowEdge = (index, n, cols, last) => {
  const start = Math.floor(index / cols) * cols;
  return last ? Math.min(n - 1, start + cols - 1) : start;
};

// L2 / R2 (Home / End): jump to the first or last item of the current row (in the grids, the row of the focused tile).
function jumpToEnd(last) {
  if (S.view === 'home') {
    const n = S.rowEls.length;
    if (!n) return;
    const ni = last ? n - 1 : 0;
    const moved = S.zone !== 'row' || ni !== S.rowIndex;
    S.zone = 'row';
    if (!moved) { bump(S.rowEls[ni], last ? 1 : -1); return; }
    S.rowIndex = ni;
    sound.play('move');
    onRowFocus();
  } else if (S.view === 'platforms') {
    const n = S.platforms.length;
    if (!n) return;
    const ni = rowEdge(S.platIndex, n, gridCols(platGrid), last);
    const moved = S.zone !== 'plat' || ni !== S.platIndex;
    S.zone = 'plat';
    if (!moved) { bump(S.platEls[ni], last ? 1 : -1); return; }
    S.platIndex = ni;
    sound.play('move');
    updateFocus();
  } else {
    const n = S.libList.length;
    if (!n) return;
    const ni = rowEdge(S.libIndex, n, gridCols(), last);
    const moved = S.zone !== 'grid' || ni !== S.libIndex;
    S.zone = 'grid';
    if (!moved) { bump(S.libEls[ni], last ? 1 : -1); return; }
    S.libIndex = ni;
    sound.play('move');
    updateFocus();
    scrollGrid();
  }
}

function goTabs(index) {
  S.zone = 'tabs';
  S.tabIndex = index;
  sound.play('move');
  updateFocus();
}

function tabsAction(a) {
  if (a === 'left' || a === 'right') {
    const ni = clamp(S.tabIndex + (a === 'left' ? -1 : 1), 0, 3);
    if (ni !== S.tabIndex) { S.tabIndex = ni; sound.play('move'); updateFocus(); }
  } else if (a === 'confirm') {
    sound.play('confirm');
    if (S.tabIndex < VIEWS.length) showView(VIEWS[S.tabIndex]);
    else openSettings();
  } else if (a === 'down' || a === 'back') {
    S.zone = S.view === 'home' ? 'row'
      : S.view === 'settings' ? 'scat'
      : S.view === 'platforms' ? (S.platforms.length ? 'plat' : 'tabs')
      : (S.libList.length ? 'grid' : 'filters');
    sound.play(a === 'back' ? 'back' : 'move');
    updateFocus();
  }
}

function onAction(a) {
  if (S.booting) return;
  const top = stack[stack.length - 1];
  if (top) { top.handle(a); updateHints(); return; }
  if (a === 'delete' || a === 'mode') return; // only the text popup uses these
  if (a === 'menu') { openQuick(); return; }
  if (a === 'prevTab' || a === 'nextTab') {
    // The shoulder buttons walk through the tabs and on to Settings, the page behind the gear.
    const v = ALL_VIEWS[clamp(ALL_VIEWS.indexOf(S.view) + (a === 'prevTab' ? -1 : 1), 0, ALL_VIEWS.length - 1)];
    if (v !== S.view) { sound.play('move'); showView(v); }
    return;
  }
  if (a === 'first' || a === 'last') { if (S.view !== 'settings') jumpToEnd(a === 'last'); return; }
  if (S.zone === 'tabs') tabsAction(a);
  else if (S.view === 'home') homeAction(a);
  else if (S.view === 'platforms') platformsAction(a);
  else if (S.view === 'settings') settingsAction(a);
  else libraryAction(a);
}

// ======================================================================
// Overlays: panels, quick menu, launch screen
// ======================================================================
const scrim = $('#scrim');
scrim.addEventListener('click', () => { const top = stack[stack.length - 1]; if (top && top.type !== 'launch') closeTop(); });

function syncScrim() {
  scrim.classList.toggle('show', stack.some((o) => o.type !== 'launch'));
  document.body.classList.toggle('panel-open', stack[stack.length - 1]?.type === 'panel');
  updateHints();
}

function pushOverlay(o, silent) {
  stack.push(o);
  o.open();
  if (!silent) sound.play('open');
  syncScrim();
}

function closeTop(silent) {
  const o = stack.pop();
  if (!o) return;
  o.close();
  if (!silent) sound.play('back');
  syncScrim();
}

function closeAll() {
  while (stack.length && stack[stack.length - 1].type !== 'launch') closeTop(true);
}

/** The control at the end of an option: a switch for on/off settings (`toggle`), a radio for one choice among several (`radio`). */
function control(it) {
  if (it.busy) return '<span class="spinner" aria-hidden="true"></span>';
  if (it.toggle) return `<span class="switch ${it.toggle() ? 'on' : ''}"><i></i></span>`;
  if (it.radio) return `<span class="radio ${it.radio() ? 'on' : ''}"><i></i></span>`;
  return '';
}

/** Side panel with a vertical list of options. */
function listPanel({ title, note, items }) {
  const panel = el('aside', 'panel');
  panel.innerHTML = `<header><h2>${esc(title)}</h2>${note ? `<p>${esc(note)}</p>` : ''}</header><ul></ul>`;
  const ul = $('ul', panel);
  let idx = 0;
  const focus = () => {
    [...ul.children].forEach((c, i) => c.classList.toggle('focused', i === idx));
    ul.children[idx]?.scrollIntoView({ block: 'nearest' });
  };
  const render = () => {
    ul.textContent = '';
    items.forEach((it, i) => {
      const li = el('li', 'item' + (it.danger ? ' danger' : ''));
      li.innerHTML = `<span>${esc(it.label)}${it.desc ? `<small>${esc(it.desc)}</small>` : ''}</span>` +
        control(it);
      li.addEventListener('click', () => { idx = i; focus(); activate(); });
      ul.appendChild(li);
    });
    focus();
  };
  const activate = async () => {
    const it = items[idx];
    if (!it) return;
    sound.play('confirm');
    await it.action?.();
    if ((it.toggle || it.radio) && panel.isConnected) render();
  };
  return {
    type: 'panel',
    open() { document.body.appendChild(panel); render(); nextFrame().then(() => panel.classList.add('in')); },
    close() { panel.classList.remove('in'); setTimeout(() => panel.remove(), 320); },
    handle(a) {
      if (a === 'up' || a === 'down') {
        const ni = clamp(idx + (a === 'up' ? -1 : 1), 0, items.length - 1);
        if (ni !== idx) { idx = ni; focus(); sound.play('move'); }
      } else if (a === 'confirm') activate();
      else if (a === 'back') closeTop();
      else if (a === 'menu') { closeAll(); sound.play('back'); }
    },
  };
}

function confirmPanel(title, note, label, action) {
  pushOverlay(listPanel({
    title, note,
    items: [
      { label, danger: true, action: () => { closeTop(true); action(); } },
      { label: t('common.cancel'), action: () => closeTop() },
    ],
  }));
}

const setCatsEl = $('#set-cats');
const setListEl = $('#set-list');
let setToken = 0;
let shownCat = null; // id of the category whose items are currently drawn

// Results of slow backend calls (PowerShell/COM), kept after the first load so menus open at once.
// A cached value is returned immediately while a fresh one is fetched; `onChange` runs if it differs.
const cacheStore = {};
function cachedCall(key, cmd, onChange) {
  const hit = cacheStore[key];
  const fresh = call(cmd).then((v) => {
    const changed = JSON.stringify(v) !== JSON.stringify(cacheStore[key]);
    cacheStore[key] = v;
    if (hit && changed) onChange?.();
    return v;
  });
  if (!hit) return fresh;
  fresh.catch(() => {}); // the stale value stays on screen if the refresh fails
  return Promise.resolve(hit);
}

/** Redraws the open Settings category when background data for it changed. */
const refreshCat = (id) => () => { if (S.view === 'settings' && settingsCats()[S.setCat]?.id === id) renderSettings(); };

/** The settings, grouped in categories. Each `load` returns `{ items, note }` (it may be async). */
function settingsCats() {
  const toggle = (key, label, desc) => ({
    label, desc,
    toggle: () => !!S.settings[key],
    action: () => setSetting(key, !S.settings[key]),
  });
  const langName = LANGUAGES.find((l) => l.code === getLang())?.name || '';
  return [
    {
      id: 'general', label: t('set.general'), icon: icons.gear,
      load: () => ({
        note: `Lumen ${S.version}`,
        items: [
          { label: t('settings.language'), desc: langName, action: openLanguage },
          toggle('autostart', t('settings.autostart'), t('settings.autostart.desc')),
          toggle('startOnPad', t('settings.startOnPad'), t('settings.startOnPad.desc')),
          toggle('minimizeOnLaunch', t('settings.minimize'), t('settings.minimize.desc')),
          toggle('pauseGame', t('settings.pause'), t('settings.pause.desc')),
          toggle('checkUpdates', t('settings.checkUpdates'), t('settings.checkUpdates.desc')),
          S.update
            ? { label: t('update.install', { version: S.update.version }), desc: t('update.install.desc'), action: askInstallUpdate }
            : { label: t('update.check'), desc: S.updateNote || t('update.check.desc'), action: () => checkUpdate(true) },
          osUpdateItem(),
        ],
      }),
    },
    {
      id: 'library', label: t('set.library'), icon: icons.library,
      load: () => ({
        items: [
          { label: t('settings.addGame'), desc: t('settings.addGame.desc'), action: addGame },
          {
            label: t('settings.folders'),
            desc: S.settings.gameFolders?.length ? S.settings.gameFolders.join(', ') : t('settings.folders.desc'),
            action: openGameFolders,
          },
          toggle('onlineArt', t('settings.onlineArt'), t('settings.onlineArt.desc')),
          { label: t('detail.refresh'), action: () => { closeAll(); rescan(true); } },
        ],
      }),
    },
    { id: 'audio', label: t('set.audio'), icon: icons.speaker, load: audioLoad },
    { id: 'video', label: t('set.video'), icon: icons.monitor, load: videoLoad },
    { id: 'bluetooth', label: t('bt.title'), icon: icons.bluetooth, load: bluetoothLoad },
    {
      id: 'appearance', label: t('set.appearance'), icon: icons.palette,
      load: () => ({
        items: [
          toggle('reduceMotion', t('settings.reduceMotion')),
          { label: t('settings.theme'), desc: t('settings.theme.desc'), action: () => call('open_path', { path: S.themeDir }).catch((e) => toast(String(e), 'error')) },
        ],
      }),
    },
  ];
}

/** Opens the Settings page, optionally on one category (`general`, `library`, `audio`, `bluetooth`, `appearance`). */
function openSettings(catId) {
  closeAll();
  const idx = settingsCats().findIndex((c) => c.id === catId);
  if (idx >= 0) S.setCat = idx;
  if (S.view !== 'settings') { showView('settings'); return; }
  S.zone = 'scat';
  S.setIndex = 0;
  renderSettings();
  updateFocus();
}

/** Draws the category list and the items of the selected category. */
async function renderSettings() {
  const cats = settingsCats();
  S.setCat = clamp(S.setCat, 0, cats.length - 1);
  setCatsEl.textContent = '';
  cats.forEach((c, i) => {
    const b = el('button', 'set-cat' + (i === S.setCat ? ' active' : ''), `${c.icon}<span>${esc(c.label)}</span>`);
    b.addEventListener('click', () => { if (stack.length) return; S.zone = 'scat'; selectCat(i); });
    setCatsEl.appendChild(b);
  });
  $('#set-title').textContent = cats[S.setCat].label;
  const me = ++setToken;
  // A slow category (Bluetooth's first load queries Windows) must not leave the previous category's items on screen.
  if (shownCat !== cats[S.setCat].id) {
    shownCat = cats[S.setCat].id;
    S.setItems = [];
    S.setEls = [];
    setListEl.textContent = '';
    $('#set-note').textContent = '';
  }
  let r;
  try { r = await cats[S.setCat].load(); } catch (e) { r = { items: [], note: String(e) }; }
  if (me !== setToken) return; // another category was picked while this one loaded
  S.setItems = r.items;
  $('#set-note').textContent = r.note || '';
  renderSettingsItems();
}

function renderSettingsItems() {
  setListEl.textContent = '';
  S.setEls = S.setItems.map((it, i) => {
    if (it.group) setListEl.appendChild(el('li', 'set-group', esc(it.group)));
    const li = el('li', 'item' + (it.danger ? ' danger' : '') + (it.inUse ? ' in-use' : ''));
    li.innerHTML = `<span>${esc(it.label)}${it.desc ? `<small>${esc(it.desc)}</small>` : ''}</span>` +
      control(it);
    li.addEventListener('click', () => {
      if (stack.length) return;
      S.zone = 'sitems'; S.setIndex = i; updateFocus(); activateSetting();
    });
    setListEl.appendChild(li);
    return li;
  });
  S.setIndex = clamp(S.setIndex, 0, Math.max(0, S.setItems.length - 1));
  if (S.zone === 'sitems' && !S.setItems.length) S.zone = 'scat';
  updateFocus();
}

function selectCat(i) {
  const n = settingsCats().length;
  const ni = clamp(i, 0, n - 1);
  if (ni === S.setCat) return;
  S.setCat = ni;
  S.setIndex = 0;
  sound.play('move');
  renderSettings();
  updateFocus();
}

async function activateSetting() {
  const it = S.setItems[S.setIndex];
  if (!it) return;
  sound.play('confirm');
  const cat = S.setCat;
  await it.action?.();
  if ((it.toggle || it.radio) && S.view === 'settings' && cat === S.setCat) renderSettingsItems();
}

function settingsAction(a) {
  if (S.zone === 'scat') {
    if (a === 'up') { if (S.setCat === 0) goTabs(3); else selectCat(S.setCat - 1); }
    else if (a === 'down') selectCat(S.setCat + 1);
    else if (a === 'right' || a === 'confirm') {
      if (S.setItems.length) { S.zone = 'sitems'; sound.play('move'); updateFocus(); }
    } else if (a === 'back') { sound.play('back'); showView('home'); }
    return;
  }
  if (a === 'up' || a === 'down') {
    const ni = clamp(S.setIndex + (a === 'up' ? -1 : 1), 0, S.setItems.length - 1);
    if (ni !== S.setIndex) { S.setIndex = ni; sound.play('move'); updateFocus(); }
  } else if (a === 'confirm') activateSetting();
  else if (a === 'left' || a === 'back') { S.zone = 'scat'; sound.play('back'); updateFocus(); }
}

/** Language picker. The choice is saved by the backend, which also uses it for its own messages. */
function openLanguage() {
  pushOverlay(listPanel({
    title: t('language.title'),
    note: t('language.note'),
    items: LANGUAGES.map((l) => ({
      label: l.name,
      radio: () => getLang() === l.code,
      action: () => setLanguage(l.code),
    })),
  }));
}

async function setLanguage(code) {
  if (code === getLang()) return;
  try {
    S.settings = await call('set_language', { language: code });
    setLang(S.settings.language || code);
    closeAll();
    refreshLanguage();
    toast(t('language.changed'));
  } catch (e) {
    toast(String(e), 'error');
  }
}

/** Redraws everything that shows text, after the language changed. */
function refreshLanguage() {
  translateDocument();
  $('#avatar').textContent = (S.user || t('user.initial')).trim().charAt(0).toUpperCase();
  tickClock();
  tickBattery();
  tickPads();
  renderFilters();
  renderRow(curGame()?.id);
  renderLibrary(S.libList[S.libIndex]?.id);
  renderPlatforms();
  if (S.view === 'settings') renderSettings();
  updateRunning();
  updateHints();
}

// The name, icon and (if the game is also on Steam) cover are filled in automatically on refresh.
async function addGame() {
  try {
    const name = await call('add_game');
    if (!name) return;
    closeAll();
    toast(t('add.done', { name }));
    rescan(false);
  } catch (e) {
    toast(String(e), 'error');
  }
}

// Games the user added by hand can be removed from Lumen (their files stay on disk).
function removeGame(g) {
  confirmPanel(t('remove.title', { name: g.name }), t('remove.noteManual'), t('remove.confirm'), async () => {
    try {
      await call('remove_game', { id: g.id });
      closeAll();
      // Remove the game right away, without a full scan.
      S.games = S.games.filter((x) => x.id !== g.id);
      S.favorites = S.favorites.filter((id) => id !== g.id);
      renderRow();
      renderLibrary();
      toast(t('remove.done', { name: g.name }));
    } catch (e) {
      toast(String(e), 'error');
    }
  });
}

/** Adds a game to Favorites (from the Library) or takes it off (from the Favorites tab). */
async function setFavorite(g, on) {
  try {
    S.favorites = (await call('set_favorite', { id: g.id, on })) || [];
    closeAll();
    renderRow();
    renderLibrary(g.id);
    toast(t(on ? 'fav.added' : 'fav.removed', { name: g.name }));
  } catch (e) {
    toast(String(e), 'error');
  }
}

// The audio category: sounds and music, plus the Windows default output device (it applies to games too).
async function audioLoad() {
  const toggle = (key, label, desc) => ({ label, desc, toggle: () => !!S.settings[key], action: () => setSetting(key, !S.settings[key]) });
  const items = [
    toggle('sounds', t('settings.sounds')),
    toggle('music', t('settings.music'), S.theme.music ? '' : t('settings.music.desc')),
  ];
  let devices = [];
  let note = t('audio.note');
  try { devices = (await cachedCall('audio', 'audio_outputs', refreshCat('audio'))) || []; } catch (e) { note = String(e); }
  if (!devices.length && note === t('audio.note')) note = t('audio.none');
  devices.forEach((d, i) => items.push({
    group: i === 0 ? t('audio.title') : '',
    label: d.name,
    radio: () => d.current,
    action: async () => {
      if (d.current) return;
      try {
        await call('set_audio_output', { id: d.id });
        devices.forEach((x) => { x.current = x.id === d.id; });
        toast(t('audio.done', { name: d.name }));
      } catch (e) { toast(String(e), 'error'); }
    },
  }));
  return { items, note };
}

// The video category: which screen is the main one (Lumen and the games open there) and, with several
// screens, whether they are extended or duplicated.
async function videoLoad() {
  let st;
  try { st = await cachedCall('video', 'video_state', refreshCat('video')); } catch (e) { return { items: [], note: String(e) }; }
  const items = [];
  // A duplicated pair of screens shows up as one display, so the mode choice also appears for `clone`.
  if (st.displays.length > 1 || st.mode === 'clone' || st.mode === 'external') {
    [['extend', 'video.extend'], ['clone', 'video.clone']].forEach(([mode, key], i) => items.push({
      group: i === 0 ? t('video.mode') : '',
      label: t(key),
      desc: t(`${key}.desc`),
      radio: () => st.mode === mode,
      action: () => { if (st.mode !== mode) return changeDisplayMode(mode); },
    }));
  }
  st.displays.forEach((d, i) => items.push({
    group: i === 0 ? t('video.main') : '',
    label: d.name,
    desc: `${d.width} × ${d.height}`,
    radio: () => d.primary,
    action: () => { if (!d.primary) return setMainDisplay(d); },
  }));
  return { items, note: items.length ? t('video.note') : t('video.none') };
}

/** Reloads the video category with fresh data after the screens changed. */
function reloadVideo() {
  delete cacheStore.video;
  if (S.view === 'settings') renderSettings();
}

async function setMainDisplay(d) {
  toast(t('video.applying'));
  try {
    await call('set_primary_display', { id: d.id });
    toast(t('video.mainDone', { name: d.name }));
  } catch (e) { toast(String(e), 'error'); }
  reloadVideo();
}

async function changeDisplayMode(mode) {
  toast(t('video.applying'));
  try {
    await call('set_display_mode', { mode });
    await new Promise((r) => setTimeout(r, 2500)); // Windows takes a moment to switch
    toast(t(`video.${mode}.done`));
  } catch (e) { toast(String(e), 'error'); }
  reloadVideo();
}

// Bluetooth: paired devices, adding a new one (pairing) and removing one (unpairing).
async function bluetoothLoad() {
  let devices = [];
  try { devices = (await cachedCall('bluetooth', 'bluetooth_paired', refreshCat('bluetooth'))) || []; } catch (e) { return { items: [], note: String(e) }; }
  const items = devices.map((d) => ({
    label: btName(d),
    desc: d.connected ? t('bt.connected') : t('bt.paired'),
    inUse: d.connected, // highlighted in green
    action: () => openBluetoothDevice(d),
  }));
  items.push({ label: t('bt.add'), desc: t('bt.add.desc'), action: scanBluetooth });
  return { items, note: devices.length ? t('bt.note') : t('bt.none') };
}

/** The name shown for a device: the one the user gave it, else the Windows one. */
const btName = (d) => S.settings.bluetoothNames?.[d.id] || d.name;

/** What can be done with a paired device: rename it or remove it. */
function openBluetoothDevice(d) {
  pushOverlay(listPanel({
    title: btName(d),
    items: [
      { label: t('bt.rename'), action: () => renameBluetooth(d) },
      { label: t('bt.remove'), danger: true, action: () => confirmPanel(btName(d), t('bt.removeNote'), t('bt.remove'), () => removeBluetooth(d)) },
      { label: t('common.cancel'), action: () => closeTop() },
    ],
  }));
}

function renameBluetooth(d) {
  pushOverlay(textPanel({
    title: t('bt.rename'),
    note: t('bt.rename.note', { name: d.name }),
    value: S.settings.bluetoothNames?.[d.id] || d.name,
    placeholder: d.name,
    onSubmit: async (value) => {
      try {
        S.settings = await call('set_bluetooth_name', { id: d.id, name: value });
        toast(t('bt.renamed'));
      } catch (e) { toast(String(e), 'error'); }
      reloadBluetooth();
    },
  }));
}

// Character sets of the on-screen keyboard, 11 keys per row: letters, symbols and accents.
const KB_MODES = {
  abc: ['1234567890@', 'qwertyuiop#', 'asdfghjkl"/', 'zxcvbnm-_?!'],
  sym: ['!@#$%^&*()_', '-+=[]{};:\'"', '<>,.?/\\|~`€', '£¥§°±×÷©®™…'],
  acc: ['ăâîșțĂÂÎȘȚç', 'àáäåæèéêëñø', 'ìíïòóôöùúüý', 'ÿœšžčřłğıþð'],
};
const KB_MODE_ORDER = ['abc', 'sym', 'acc'];
// What the buttons are called on each controller, shown in the corner of the keys they trigger.
const KB_TAGS = {
  playstation: { shift: 'L2', acc: 'R3', space: '△', del: '□', left: 'L1', right: 'R1', cancel: '○', done: 'R2' },
  xbox: { shift: 'LT', acc: 'RS', space: 'Y', del: 'X', left: 'LB', right: 'RB', cancel: 'B', done: 'RT' },
};

/**
 * Text popup with an on-screen keyboard docked at the bottom, laid out like the console ones.
 * Controller: D-pad moves, confirm types, square deletes (hold to repeat), triangle types a space,
 * L2 toggles shift, R3 changes the character set, L1 / R1 move the cursor, R2 is Done, back cancels.
 * Physical keyboard: type in the field, Enter saves, Escape cancels. The keys also work with the mouse.
 */
function textPanel({ title, note, value = '', placeholder = '', maxLength = 40, onSubmit }) {
  const panel = el('div', 'dialog');
  panel.setAttribute('role', 'dialog');
  panel.innerHTML = `<header><h2>${esc(title)}</h2>${note ? `<p>${esc(note)}</p>` : ''}</header>`;
  const input = el('input', 'name-field');
  input.type = 'text';
  input.maxLength = maxLength;
  input.value = value;
  input.placeholder = placeholder;
  panel.appendChild(input);
  const board = el('div', 'kb');
  panel.appendChild(board);

  const chars = (s) => [...s].map((ch) => ({ ch }));
  const fn = (act, label, span) => ({ act, label, span });
  const layout = () => [
    ...KB_MODES[mode].map(chars),
    [fn('shift', '⇧', 2), fn('abc', 'ABC', 1), fn('sym', '@#:', 1), fn('acc', 'àă', 1), { act: 'space', span: 4, svg: '<svg class="space-icon" viewBox="0 0 100 20" preserveAspectRatio="none" aria-hidden="true"><path d="M1 3v13h98V3" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" vector-effect="non-scaling-stroke"/></svg>' }, fn('del', '⌫', 2)],
    [fn('left', '◀', 2), fn('right', '▶', 2), fn('cancel', t('common.cancel'), 3), fn('done', t('kb.done'), 4)],
  ];
  let mode = 'abc', r = 1, c = 0, shift = false, submitted = false;
  const starts = (row) => { let x = 0; return row.map((k) => { const s = x; x += k.span || 1; return s; }); };
  const label = (k) => (k.ch ? (shift ? k.ch.toUpperCase() : k.ch) : k.label);

  const draw = () => {
    const rows = layout();
    r = clamp(r, 0, rows.length - 1);
    c = clamp(c, 0, rows[r].length - 1);
    const tags = S.device === 'gamepad' ? KB_TAGS[S.pad] || KB_TAGS.xbox : null;
    board.textContent = '';
    rows.forEach((row, ri) => {
      const line = el('div', 'kb-row');
      row.forEach((k, ci) => {
        const on = (k.act === 'shift' && shift) || k.act === mode;
        const b = el('button', 'kb-key' + (k.ch ? '' : ' fn') + (KB_MODES[k.act] ? ' mode' + (ci === 1 ? ' first' : ci === 3 ? ' last' : '') : '') + (ri === r && ci === c ? ' focused' : '') + (on ? ' on' : '') + (k.act === 'done' ? ' ok' : ''),
          `${k.svg || esc(label(k))}${tags && tags[k.act] ? `<i>${esc(tags[k.act])}</i>` : ''}`);
        b.tabIndex = -1;
        b.style.gridColumn = `span ${k.span || 1}`;
        b.addEventListener('mousedown', (e) => e.preventDefault()); // keep the focus in the field
        b.addEventListener('click', () => { r = ri; c = ci; press(k); draw(); });
        line.appendChild(b);
      });
      board.appendChild(line);
    });
  };

  // Editing happens at the cursor of the field, so L1 / R1 and the physical keyboard agree.
  const insert = (s) => {
    const a = input.selectionStart ?? input.value.length, b = input.selectionEnd ?? a;
    if (input.value.length - (b - a) + s.length > maxLength) return;
    input.setRangeText(s, a, b, 'end');
  };
  const deleteChar = () => {
    const a = input.selectionStart ?? input.value.length, b = input.selectionEnd ?? a;
    if (a !== b) input.setRangeText('', a, b, 'end');
    else if (a > 0) input.setRangeText('', a - 1, a, 'end');
  };
  const moveCaret = (d) => {
    const p = clamp((input.selectionStart ?? 0) + d, 0, input.value.length);
    input.setSelectionRange(p, p);
  };
  const submit = () => { if (submitted) return; submitted = true; onSubmit(input.value); };
  const setMode = (m) => { mode = m; shift = false; draw(); };
  const press = (k) => {
    sound.play('confirm');
    if (k.ch) { insert(shift ? k.ch.toUpperCase() : k.ch); shift = false; }
    else if (k.act === 'shift') shift = !shift;
    else if (KB_MODES[k.act]) setMode(k.act);
    else if (k.act === 'space') insert(' ');
    else if (k.act === 'del') deleteChar();
    else if (k.act === 'left') moveCaret(-1);
    else if (k.act === 'right') moveCaret(1);
    else if (k.act === 'cancel') closeTop();
    else if (k.act === 'done') submit();
  };
  const move = (a) => {
    const rows = layout();
    if (a === 'left') c = Math.max(0, c - 1);
    else if (a === 'right') c = Math.min(rows[r].length - 1, c + 1);
    else {
      const x = starts(rows[r])[c] + (rows[r][c].span || 1) / 2;
      r = clamp(r + (a === 'up' ? -1 : 1), 0, rows.length - 1);
      c = Math.max(0, starts(rows[r]).findLastIndex((s) => s <= Math.floor(x)));
    }
    sound.play('move');
    draw();
  };

  return {
    type: 'dialog',
    // The key tags follow the device in use; the global hint bar stays empty for this popup.
    hints: () => { draw(); return []; },
    open() {
      document.body.appendChild(panel);
      draw();
      nextFrame().then(() => {
        panel.classList.add('in');
        input.focus();
        if (S.device === 'gamepad') input.setSelectionRange(input.value.length, input.value.length); else input.select();
      });
    },
    close() { panel.classList.remove('in'); input.blur(); setTimeout(() => panel.remove(), 320); },
    handle(a) {
      if (a === 'menu') { closeAll(); sound.play('back'); return; }
      if (S.device !== 'gamepad') {
        if (a === 'confirm') { sound.play('confirm'); submit(); }
        else if (a === 'back') closeTop();
        return;
      }
      if (document.activeElement !== input) input.focus({ preventScroll: true });
      if (['up', 'down', 'left', 'right'].includes(a)) move(a);
      else if (a === 'confirm') { press(layout()[r][c]); draw(); }
      else if (a === 'back') closeTop();
      else if (a === 'delete') deleteChar();
      else if (a === 'options') insert(' '); // triangle
      else if (a === 'first') { shift = !shift; sound.play('move'); draw(); } // L2
      else if (a === 'last') { sound.play('confirm'); submit(); } // R2
      else if (a === 'prevTab') moveCaret(-1); // L1
      else if (a === 'nextTab') moveCaret(1); // R1
      else if (a === 'mode') setMode(KB_MODE_ORDER[(KB_MODE_ORDER.indexOf(mode) + 1) % KB_MODE_ORDER.length]); // R3
    },
  };
}

/** Closes the open panels and reloads the Bluetooth list. */
function reloadBluetooth() {
  closeAll();
  if (S.view === 'settings') renderSettings();
}

async function removeBluetooth(d) {
  try {
    await call('bluetooth_remove', { id: d.id });
    if (cacheStore.bluetooth) cacheStore.bluetooth = cacheStore.bluetooth.filter((x) => x.id !== d.id);
    toast(t('bt.removed', { name: btName(d) }));
  } catch (e) { toast(String(e), 'error'); }
  reloadBluetooth();
}

let btBusy = false;
async function scanBluetooth() {
  if (btBusy) return;
  btBusy = true;
  // The inquiry takes a while: a panel with only "Cancel" stays up, and the results replace it.
  let cancelled = false;
  const wait = listPanel({
    title: t('bt.scanning'), note: t('bt.scanning.note'),
    items: [{ label: t('common.cancel'), action: () => { cancelled = true; closeTop(); } }],
  });
  pushOverlay(wait);
  let found = [];
  try { found = (await call('bluetooth_scan')) || []; } catch (e) { if (!cancelled) toast(String(e), 'error'); }
  btBusy = false;
  if (cancelled) return;
  if (stack[stack.length - 1] !== wait) return;
  closeTop(true);
  const items = found.map((d) => ({ label: d.name, action: () => pairBluetooth(d) }));
  items.push({ label: t('bt.rescan'), action: () => { closeTop(true); scanBluetooth(); } });
  pushOverlay(listPanel({ title: t('bt.add'), note: found.length ? t('bt.found.note') : t('bt.found.none'), items }), true);
}

async function pairBluetooth(d) {
  toast(t('bt.pairing', { name: d.name }));
  try {
    await call('bluetooth_pair', { id: d.id });
    delete cacheStore.bluetooth; // the new device shows up with its real state
    toast(t('bt.pairDone', { name: d.name }));
    reloadBluetooth();
  } catch (e) { toast(String(e), 'error'); }
}

// Steam and games with an uninstaller ask for confirmation in their own window; Xbox games are uninstalled directly.
function uninstallGame(g) {
  const run = async () => {
    try {
      toast(t('uninstall.progress', { name: g.name }));
      const r = await call('uninstall_game', { id: g.id });
      closeAll();
      if (r.done) {
        S.games = S.games.filter((x) => x.id !== g.id);
        renderRow();
        renderLibrary();
      }
      toast(r.message);
    } catch (e) {
      toast(String(e), 'error');
    }
  };
  if (g.source !== 'xbox') { run(); return; }
  confirmPanel(t('uninstall.title', { name: g.name }), t('uninstall.note'), t('uninstall.confirm'), run);
}

// Each subfolder of a folder listed here shows up in the library as a "Local" game.
function openGameFolders() {
  const reopen = () => { closeAll(); renderSettings(); openGameFolders(); };
  const folders = S.settings.gameFolders || [];
  pushOverlay(listPanel({
    title: t('folders.title'),
    note: t('folders.note'),
    items: [
      ...folders.map((path) => ({
        label: path,
        desc: t('folders.remove'),
        action: () => confirmPanel(t('folders.removeTitle'), path, t('folders.removeConfirm'), async () => {
          try {
            S.settings = await call('remove_game_folder', { path });
            reopen();
            rescan(true);
          } catch (e) { toast(String(e), 'error'); }
        }),
      })),
      {
        label: t('folders.add'),
        desc: t('folders.add.desc'),
        action: async () => {
          try {
            const settings = await call('add_game_folder');
            if (!settings) return;
            S.settings = settings;
            reopen();
            rescan(true);
          } catch (e) { toast(String(e), 'error'); }
        },
      },
    ],
  }));
}

async function setSetting(key, value) {
  try {
    S.settings = await call('set_setting', { key, value });
    applySettings();
  } catch (e) {
    toast(String(e), 'error');
  }
}

function applySettings() {
  sound.setEnabled(S.settings.sounds);
  document.body.classList.toggle('reduce-motion', !!S.settings.reduceMotion);
  sound.musicPlay(!!S.settings.music && !S.running?.started && !document.hidden);
}

/** The options of a game. `from` is where they were opened: the Favorites tab or the Library. */
function openGameOptions(g, from) {
  const running = S.running?.id === g.id;
  const items = [
    ...(running && S.running.started ? [{ label: t('game.back'), action: () => resumeGame() }] : []),
    running
      ? { label: t('opt.close'), danger: true, action: () => { closeTop(true); askCloseGame(); } }
      : { label: t('game.play'), action: () => { closeTop(true); launch(g); } },
    ...(from === 'favorites' ? [{ label: t('fav.remove'), action: () => setFavorite(g, false) }] : []),
    ...(from === 'library' && !isFavorite(g.id) ? [{ label: t('fav.add'), action: () => setFavorite(g, true) }] : []),
    { label: t('opt.openLocation'), desc: g.installDir, action: () => call('open_path', { path: g.installDir }).catch((e) => toast(String(e), 'error')) },
  ];
  if (!running) items.push({ label: t('opt.uninstall'), danger: true, action: () => uninstallGame(g) });
  if (!running && g.id.startsWith('manual:')) items.push({ label: t('remove.confirm'), danger: true, action: () => removeGame(g) });
  // The running game can be closed from the options of any game in the library.
  const other = !running && gameById(S.running?.id);
  if (other) items.unshift({ label: t('opt.closeNamed', { name: other.name }), desc: t('opt.runningNow'), danger: true, action: () => { closeTop(true); askCloseGame(); } });
  pushOverlay(listPanel({ title: g.name, note: `${sourceName(g.source)} · ${relTime(stat(g.id).lastPlayed)}`, items }));
}

function askCloseGame() {
  const g = gameById(S.running?.id);
  if (!g) return;
  confirmPanel(t('close.title', { name: g.name }), t('close.note'), t('opt.close'), async () => {
    try {
      // `true`: the launch hadn't been detected yet, so no `game-exited` will arrive.
      if (await call('close_game')) {
        S.running = null;
        hideLaunch();
        closeAll();
        updateRunning();
        applySettings();
        toast(t('close.done', { name: g.name }));
      }
    } catch (e) { toast(String(e), 'error'); }
  });
}

function openPower() {
  pushOverlay(listPanel({
    title: t('power.title'),
    items: [
      { label: t('power.sleep'), action: () => { closeAll(); call('power', { action: 'sleep' }); } },
      { label: t('power.restart'), danger: true, action: () => confirmPanel(t('power.restartTitle'), t('power.note'), t('power.restartConfirm'), () => call('power', { action: 'restart' })) },
      { label: t('power.shutdown'), danger: true, action: () => confirmPanel(t('power.shutdownTitle'), t('power.note'), t('power.shutdownConfirm'), () => call('power', { action: 'shutdown' })) },
      { label: t('power.exit'), desc: t('power.exit.desc'), action: () => call('exit_app') },
    ],
  }));
}

/** Quick menu: a horizontal band at the bottom, opened with Guide / Menu / Tab. */
function openQuick() {
  const running = gameById(S.running?.id);
  const cards = [
    { label: t('quick.home'), icon: icons.home, run: () => { closeAll(); showView('home'); } },
  ];
  if (running && S.running.started) cards.unshift({ label: t('game.back'), icon: icons.play, run: () => resumeGame() });
  if (running) cards.push({ label: t('opt.closeNamed', { name: running.name }), icon: icons.close, danger: true, run: () => askCloseGame() });
  cards.push(
    { label: t('detail.refresh'), icon: icons.refresh, run: () => { closeAll(); rescan(true); } },
    { label: t('settings.title'), icon: icons.gear, run: () => openSettings() },
    { label: t('power.title'), icon: icons.power, run: () => openPower() },
  );

  const box = el('div', 'quick');
  box.innerHTML = `<div class="quick-top"><strong>${running ? esc(running.name) : esc(t('quick.title'))}</strong><span>${running ? esc(S.running.started ? t('quick.running') : t('quick.starting')) : esc(timeNow())}</span></div><div class="quick-cards"></div>`;
  const list = $('.quick-cards', box);
  let idx = 0;
  cards.forEach((c, i) => {
    const n = el('div', 'qcard' + (c.danger ? ' danger' : ''), `${c.icon}<span>${esc(c.label)}</span>`);
    n.addEventListener('click', () => { idx = i; focus(); sound.play('confirm'); c.run(); });
    list.appendChild(n);
  });
  const focus = () => [...list.children].forEach((c, i) => c.classList.toggle('focused', i === idx));
  focus();

  pushOverlay({
    type: 'quick',
    open() { document.body.appendChild(box); nextFrame().then(() => box.classList.add('in')); },
    close() { box.classList.remove('in'); setTimeout(() => box.remove(), 320); },
    handle(a) {
      if (a === 'left' || a === 'right') {
        const ni = clamp(idx + (a === 'left' ? -1 : 1), 0, cards.length - 1);
        if (ni !== idx) { idx = ni; focus(); sound.play('move'); }
      } else if (a === 'confirm') { sound.play('confirm'); cards[idx].run(); }
      else if (a === 'back' || a === 'menu' || a === 'up') closeTop();
    },
  });
}

/** Waits for the controller buttons to be released (2 s at most). Otherwise the press that picked
 *  "Back to game" stays held when the game gets focus, and the game takes it as its own. */
async function padsReleased() {
  const pressed = () => [...(navigator.getGamepads?.() || [])].some((p) => p?.buttons.some((b) => b.pressed));
  for (let i = 0; i < 40 && pressed(); i++) await new Promise((r) => setTimeout(r, 50));
}

/** Brings the running game back to the front; Lumen minimizes. */
async function resumeGame() {
  try {
    await padsReleased();
    await call('resume_game');
    closeAll();
  } catch (e) {
    toast(String(e), 'error');
  }
}

/** The screen shown while the game starts. */
let launchOverlay = null;
function showLaunch(g) {
  const box = el('div', 'launch');
  box.innerHTML = `<div class="cover"></div><h2>${esc(g.name)}</h2><p>${esc(t('launch.starting'))}</p><div class="bar"><i></i></div>`;
  const art = tileArt(g);
  art.style.position = 'absolute';
  $('.cover', box).appendChild(art);
  launchOverlay = {
    type: 'launch',
    box,
    open() { document.body.appendChild(box); nextFrame().then(() => box.classList.add('in')); },
    close() { box.classList.remove('in'); setTimeout(() => box.remove(), 380); },
    handle(a) {
      if (a === 'back' && !S.running?.started) {
        call('cancel_launch');
        S.running = null;
        hideLaunch();
        sound.play('back');
        renderDetail(true);
      } else if (a === 'menu') openQuick();
    },
  };
  pushOverlay(launchOverlay, true);
}

function hideLaunch() {
  if (!launchOverlay) return;
  const i = stack.indexOf(launchOverlay);
  if (i >= 0) stack.splice(i, 1);
  launchOverlay.close();
  launchOverlay = null;
  syncScrim();
}

// ======================================================================
// Launching games
// ======================================================================
async function launch(g) {
  if (S.running) {
    // "Play" on the game that is already running takes you back to it.
    if (S.running.id === g.id && S.running.started) resumeGame();
    else if (S.running.id === g.id) toast(t('launch.pending', { name: g.name }));
    else toast(t('launch.closeFirst', { name: gameById(S.running.id)?.name || t('launch.currentGame') }), 'error');
    return;
  }
  sound.play('launch');
  showLaunch(g);
  try {
    await call('launch_game', { id: g.id });
    S.running = { id: g.id, started: false };
    updateRunning();
  } catch (e) {
    hideLaunch();
    toast(String(e), 'error');
  }
}

function updateRunning() {
  const pill = $('#running-pill');
  const g = gameById(S.running?.id);
  pill.hidden = !g;
  if (g) $('span', pill).textContent = g.name;
  renderDetail(true);
}

// ======================================================================
// Updates
// ======================================================================
/** Looks for a newer release. A manual check also reports "up to date" and errors; the automatic one stays silent. */
async function checkUpdate(manual) {
  if (S.updating) return;
  try {
    if (manual) { S.updateNote = t('update.checking'); renderSettingsIfGeneral(); }
    const info = await call('check_update');
    const isNew = info && info.version !== S.update?.version;
    S.update = info || null;
    S.updateNote = info ? '' : t('update.upToDate');
    updateDot();
    if (info && (manual || isNew)) toast(t('update.available', { version: info.version }));
    else if (manual) toast(t('update.upToDate'));
  } catch (e) {
    S.updateNote = '';
    if (manual) toast(String(e), 'error');
  }
  renderSettingsIfGeneral();
}

const updateDot = () => $('#btn-settings').classList.toggle('has-update', !!S.update || !!S.os?.count || !!S.os?.rebootRequired);

/** Asks Windows Update what is waiting (nothing is installed by Lumen). The automatic check only speaks up for new findings. */
async function checkOsUpdates(manual) {
  if (S.osChecking) return;
  S.osChecking = true;
  if (manual) { S.osNote = t('update.checking'); renderSettingsIfGeneral(); }
  try {
    const os = await call('check_os_updates');
    const isNew = os.count > (S.os?.count || 0);
    S.os = os;
    S.osNote = '';
    if (os.count && (manual || isNew)) toast(t('os.available', { n: os.count }));
    else if (manual) toast(os.rebootRequired ? t('os.reboot') : t('os.upToDate'));
  } catch (e) {
    S.osNote = '';
    if (manual) toast(String(e), 'error');
  }
  S.osChecking = false;
  updateDot();
  renderSettingsIfGeneral();
}

/** Installs the waiting Windows updates (Windows asks for administrator approval first). */
async function installOsUpdates() {
  if (S.osInstalling) return;
  S.osInstalling = true;
  S.osProgress = null;
  renderSettingsIfGeneral();
  toast(t('os.installing'));
  try {
    const r = await call('install_os_updates');
    S.os = { count: 0, titles: [], rebootRequired: r.rebootRequired };
    toast(r.failed ? t('os.partial', { done: r.installed, failed: r.failed }) : t('os.installed', { n: r.installed }), r.failed ? 'error' : 'info');
  } catch (e) {
    toast(String(e), 'error');
  }
  S.osInstalling = false;
  S.osProgress = null;
  updateDot();
  renderSettingsIfGeneral();
}

function osUpdateItem() {
  if (S.osInstalling) {
    const p = S.osProgress;
    return {
      label: p ? t('os.progress', { i: p.i, n: p.n }) : t('os.installing'),
      desc: p ? t(p.stage === 'download' ? 'os.downloading' : 'os.installingOne', { title: p.title }) : t('os.installing.desc'),
      busy: true,
      action: () => {},
    };
  }
  if (S.os?.count) {
    return {
      label: t('os.pending', { n: S.os.count }),
      desc: S.os.titles.join(' · '),
      action: () => confirmPanel(t('os.installTitle', { n: S.os.count }), t('os.installNote'), t('os.installConfirm'), installOsUpdates),
    };
  }
  if (S.os?.rebootRequired) {
    return { label: t('os.reboot'), desc: t('os.reboot.desc'), action: () => confirmPanel(t('power.restartTitle'), t('power.note'), t('power.restartConfirm'), () => call('power', { action: 'restart' })) };
  }
  return { label: t('os.check'), desc: S.osNote || (S.os ? t('os.upToDate') : t('os.check.desc')), action: () => checkOsUpdates(true) };
}

const renderSettingsIfGeneral = () => { if (S.view === 'settings' && settingsCats()[S.setCat]?.id === 'general') renderSettings(); };

function askInstallUpdate() {
  if (!S.update) return;
  confirmPanel(t('update.title', { version: S.update.version }), t('update.note'), t('update.confirm'), installUpdate);
}

async function installUpdate() {
  if (S.updating) return;
  S.updating = true;
  toast(t('update.downloading'));
  try {
    await call('install_update'); // Lumen closes itself and the installer starts the new version
  } catch (e) {
    toast(String(e), 'error');
  }
  S.updating = false;
}

// ======================================================================
// Notifications, hints, clock
// ======================================================================
function toast(msg, kind = 'info') {
  const node = el('div', `toast ${kind}`, esc(msg));
  $('#toasts').appendChild(node);
  nextFrame().then(() => node.classList.add('in'));
  setTimeout(() => { node.classList.remove('in'); setTimeout(() => node.remove(), 420); }, 4800);
}

// The PlayStation symbols, drawn in their classic colors.
const psSvg = (color, body) =>
  `<svg viewBox="0 0 16 16" fill="none" stroke="${color}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
// The quick menu is on the left button (View / Create), game options on the right one (☰).
// Y / △ still open the options, but the hint shows the right button.
const GLYPHS = {
  xbox: {
    confirm: 'A', back: 'B', options: '☰',
    menu: psSvg('currentColor', '<rect x="2.5" y="2.5" width="8" height="8" rx="1.5"/><path d="M6 13.5h6a1.5 1.5 0 0 0 1.5-1.5V6"/>'),
  },
  playstation: {
    confirm: psSvg('#7cb2ff', '<path d="m4 4 8 8M12 4l-8 8"/>'),
    back: psSvg('#ff6b81', '<circle cx="8" cy="8" r="4.6"/>'),
    options: '☰',
    menu: psSvg('currentColor', '<path d="M8 13V5M4.5 12 3 6M11.5 12 13 6"/>'),
  },
  keyboard: { confirm: 'Enter', back: 'Esc', menu: 'Tab', options: 'O' },
};
function updateHints() {
  const g = GLYPHS[S.device === 'gamepad' ? S.pad : 'keyboard'];
  const top = stack[stack.length - 1];
  let list;
  if (top?.type === 'launch') list = S.running?.started ? [['menu', 'hint.menu']] : [['back', 'hint.cancel'], ['menu', 'hint.menu']];
  else if (top?.hints) list = top.hints();
  else if (top) list = [['confirm', 'hint.select'], ['back', 'hint.back']];
  else if (S.view === 'home' && S.zone === 'row' && curGame()) list = [['confirm', 'hint.play'], ['options', 'hint.options'], ['menu', 'hint.menu']];
  else if (S.view === 'settings' && S.zone === 'scat') list = [['confirm', 'hint.select'], ['back', 'hint.back'], ['menu', 'hint.menu']];
  else if (S.view === 'settings' && S.zone === 'sitems') list = [['confirm', 'hint.select'], ['back', 'hint.back'], ['menu', 'hint.menu']];
  else if (S.view === 'platforms' && S.zone === 'plat') list = [['confirm', 'hint.open'], ['back', 'hint.back'], ['menu', 'hint.menu']];
  else if (S.view === 'library' && S.zone === 'grid') list = [['confirm', 'hint.play'], ['options', 'hint.options'], ['back', 'hint.back'], ['menu', 'hint.menu']];
  else list = [['confirm', 'hint.select'], ['menu', 'hint.menu']];
  $('#hints').innerHTML = list.map(([k, label]) => `<span class="hint"><span class="glyph">${g[k]}</span>${esc(t(label))}</span>`).join('');
}

function tickClock() {
  $('#clock').textContent = timeNow();
}

// The battery only shows on devices that have one (laptop, handheld).
function batteryIcon(percent, charging) {
  const w = Math.round((clamp(percent, 0, 100) / 100) * 22 * 10) / 10;
  const bolt = charging ? '<path d="M15 2.5 11 9h3.5L12 15.5" fill="none" stroke="var(--void)" stroke-width="3.2" stroke-linejoin="round"/><path d="M15 2.5 11 9h3.5L12 15.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/>' : '';
  return `<svg viewBox="0 0 34 18" aria-hidden="true">
    <rect x="1" y="1" width="28" height="16" rx="3.5" fill="none" stroke="currentColor" stroke-width="1.6" opacity=".85"/>
    <rect x="30.5" y="6" width="2.5" height="6" rx="1" fill="currentColor" opacity=".85"/>
    <rect x="4" y="4" width="${w}" height="10" rx="1.6" fill="currentColor"/>${bolt}
  </svg>`;
}

async function tickBattery() {
  let b = null;
  try { b = await call('battery'); } catch { /* no battery information */ }
  const box = $('#battery');
  box.hidden = !b;
  if (!b) return;
  const charging = b.charging || b.plugged;
  box.classList.toggle('charging', charging);
  box.classList.toggle('low', !charging && b.percent <= 20);
  $('.batt-icon', box).innerHTML = batteryIcon(b.percent, charging);
  $('.batt-pct', box).textContent = `${b.percent}%`;
  box.setAttribute('aria-label', t('battery.label', { percent: b.percent, charging }));
}

// Controller battery: DualSense/DualShock over HID, Xbox over XInput.
async function tickPads() {
  let pads = [];
  try { pads = (await call('pad_batteries')) || []; } catch { /* no controllers */ }
  $('#pad-batteries').innerHTML = pads.map((p) => {
    const cls = p.charging ? ' charging' : p.percent <= 20 ? ' low' : '';
    const name = t(p.kind === 'playstation' ? 'pad.playstation' : 'pad.xbox');
    return `<div class="battery pad${cls}" aria-label="${esc(t('battery.padLabel', { name, percent: p.percent, charging: p.charging }))}">
      <span class="pad-icon">${icons.gamepad}</span><span class="batt-icon">${batteryIcon(p.percent, p.charging)}</span><span class="batt-pct">${p.percent}%</span>
    </div>`;
  }).join('');
}

// ======================================================================
// Library: loading and refreshing
// ======================================================================
let manualScan = false;
function rescan(manual) {
  manualScan = !!manual;
  if (manual) toast(t('scan.start'));
  call('scan_library');
}

function onLibrary({ games, warnings }) {
  const keepRow = curGame()?.id;
  const keepLib = S.libList[S.libIndex]?.id;
  S.games = games;
  probeCache.clear();
  renderRow(keepRow);
  renderLibrary(keepLib);
  if (manualScan) toast(t('scan.done', { n: games.length }));
  // Warnings (e.g. Steam missing) only matter if nothing was found.
  if (warnings?.length && (manualScan || !games.length)) warnings.forEach((w) => toast(w, 'error'));
  manualScan = false;
}

// ======================================================================
// Custom theme
// ======================================================================
async function loadTheme() {
  const theme = S.theme;
  if (!isTauri) return;
  if (theme.font) {
    try {
      const buf = await call('theme_file', { name: 'font' });
      const face = new FontFace('LumenTheme', buf);
      await face.load();
      document.fonts.add(face);
      document.documentElement.style.setProperty('--font', '"LumenTheme", "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif');
      measureRem();
    } catch { /* the font stays the default one */ }
  }
  for (const name of ['move', 'confirm', 'back', 'open', 'launch']) {
    if (theme[name]) call('theme_file', { name }).then((buf) => sound.load(name, buf)).catch(() => {});
  }
  if (theme.music) sound.setMusic(fileUrl(theme.music));
}

// ======================================================================
// Startup
// ======================================================================
async function boot() {
  measureRem();
  $('#btn-settings').innerHTML = icons.gear;
  syncClockFormat();
  setInterval(tickClock, 10_000);
  window.addEventListener('focus', syncClockFormat);
  tickBattery();
  setInterval(tickBattery, 30_000);
  tickPads();
  setInterval(tickPads, 30_000);
  // A freshly connected controller needs a moment before it answers over HID.
  for (const ev of ['gamepadconnected', 'gamepaddisconnected']) window.addEventListener(ev, () => setTimeout(tickPads, 800));

  const input = createInput({
    onAction,
    onDevice: (d, pad) => { S.device = d; S.pad = pad; document.body.classList.toggle('hide-cursor', d === 'gamepad'); updateHints(); },
  });
  S.device = input.device;
  S.pad = input.pad;

  // The cursor disappears after a few seconds of inactivity.
  let cursorTimer = null;
  window.addEventListener('mousemove', () => {
    document.body.classList.remove('hide-cursor');
    clearTimeout(cursorTimer);
    cursorTimer = setTimeout(() => document.body.classList.add('hide-cursor'), 2500);
  });
  window.addEventListener('contextmenu', (e) => e.preventDefault());
  window.addEventListener('resize', () => { measureRem(); layoutRow(true); scrollGrid(); });

  $('#tab-home').addEventListener('click', () => { if (!stack.length) { sound.play('move'); showView('home'); } });
  $('#tab-library').addEventListener('click', () => { if (!stack.length) { sound.play('move'); showView('library'); } });
  $('#tab-platforms').addEventListener('click', () => { if (!stack.length) { sound.play('move'); showView('platforms'); } });
  $('#btn-settings').addEventListener('click', () => { if (!stack.length) { sound.play('confirm'); openSettings(); } });

  // The saved state is shown right away; the full scan comes in the background.
  const snap = await call('get_state');
  Object.assign(S, {
    games: snap.games || [],
    favorites: snap.favorites || [],
    stats: snap.stats || {},
    settings: snap.settings || {},
    theme: snap.theme || {},
    themeDir: snap.themeDir || '',
    running: snap.running || null,
    user: snap.user || '',
    version: snap.version || '',
  });
  setLang(S.settings.language);
  translateDocument();
  $('#avatar').textContent = (S.user || t('user.initial')).trim().charAt(0).toUpperCase();
  tickClock();
  tickBattery();
  tickPads();
  await loadTheme();
  applySettings();
  renderFilters();
  renderRow();
  renderLibrary();
  updateRunning();

  on('library', onLibrary);
  on('os-progress', (p) => { S.osProgress = p; renderSettingsIfGeneral(); });
  on('guide', () => {
    const top = stack[stack.length - 1];
    // Like on a console: a second press on PS / Guide takes you back to the game.
    if (top?.type === 'quick') { if (S.running?.started) resumeGame(); else closeTop(); }
    else openQuick();
  });
  // The DS5 Bridge dongle gave up the controller: the next PS press connects it to the PC.
  on('pad-handoff', () => toast(t('pad.handoff')));
  on('game-started', ({ id }) => {
    S.running = { id, started: true };
    launchOverlay?.box.classList.add('started');
    const p = launchOverlay && $('p', launchOverlay.box);
    if (p) p.textContent = t('launch.running');
    sound.musicPlay(false);
    updateRunning();
    setTimeout(hideLaunch, 1500);
  });
  on('game-exited', ({ id, seconds, detected }) => {
    S.running = null;
    hideLaunch();
    closeAll();
    const st = (S.stats[id] ||= { playtimeSec: 0, lastPlayed: 0, launches: 0 });
    st.lastPlayed = Math.floor(Date.now() / 1000);
    st.playtimeSec += seconds || 0;
    showView('home');
    S.zone = 'row';
    renderRow(id);
    updateRunning();
    const g = gameById(id);
    if (!detected) toast(t('launch.notDetected'), 'error');
    else if (g) toast(t('launch.played', { name: g.name, duration: duration(seconds) }));
    applySettings();
  });
  document.addEventListener('visibilitychange', applySettings);

  if (isTauri) await call('app_ready');
  await nextFrame();
  S.booting = false;
  document.body.classList.remove('booting');
  updateHints();
  rescan(false);
  // Warm the slow menus once Lumen is up, so they open instantly later (staggered to keep the start light).
  setTimeout(() => { cachedCall('bluetooth', 'bluetooth_paired').catch(() => {}); loadPlatforms(); }, 1500);
  setTimeout(() => cachedCall('audio', 'audio_outputs').catch(() => {}), 3500);
  setTimeout(() => cachedCall('video', 'video_state').catch(() => {}), 5500);
  // Once a while after start, and then every few hours if Lumen stays open (it is meant to run for days).
  const autoCheck = () => {
    if (!S.settings.checkUpdates || S.running?.started) return;
    checkUpdate(false);
    checkOsUpdates(false);
  };
  setTimeout(autoCheck, 12000);
  setInterval(autoCheck, 6 * 3600 * 1000);
}

boot();
