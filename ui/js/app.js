import { call, on, fileUrl, isTauri } from './api.js';
import * as sound from './sound.js';
import { createInput } from './input.js';
import { icons } from './icons.js';

// ======================================================================
// Utilitare
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

// Româna cere „de” după numerele de la 20 în sus: „20 de zile”, dar „19 zile”.
function plural(n, one, many) {
  if (n === 1) return one;
  const r = n % 100;
  return `${n} ${n >= 20 && (r === 0 || r >= 20) ? 'de ' : ''}${many}`;
}

function relTime(ts) {
  if (!ts) return 'niciodată';
  const d = Math.max(0, Date.now() / 1000 - ts);
  if (d < 60) return 'chiar acum';
  if (d < 3600) return `acum ${plural(Math.floor(d / 60), 'un minut', 'minute')}`;
  if (d < 86400) return `acum ${plural(Math.floor(d / 3600), 'o oră', 'ore')}`;
  if (d < 2 * 86400) return 'ieri';
  if (d < 30 * 86400) return `acum ${plural(Math.floor(d / 86400), 'o zi', 'zile')}`;
  return new Date(ts * 1000).toLocaleDateString('ro-RO', { day: 'numeric', month: 'short', year: 'numeric' });
}

function duration(sec) {
  if (!sec || sec < 60) return 'sub un minut';
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  if (!h) return `${m} min`;
  return m ? `${h} h ${m} min` : `${h} h`;
}

function bytes(n) {
  if (!n) return '';
  const gb = n / 1024 ** 3;
  return gb >= 1 ? `${gb.toFixed(gb >= 100 ? 0 : 1).replace('.', ',')} GB` : `${Math.round(n / 1024 ** 2)} MB`;
}

const SOURCE = { steam: 'Steam', xbox: 'Xbox', local: 'Local' };

/** Încearcă pe rând fiecare imagine candidată până se încarcă una. */
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

/** Găsește primul URL care se încarcă (rezultatul se memorează). */
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
// Stare
// ======================================================================
const ROW_MAX = 12;
const S = {
  games: [],
  stats: {},
  settings: {},
  theme: {},
  themeDir: '',
  running: null,
  user: '',
  version: '',
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
  device: 'keyboard',
  pad: 'xbox',
  booting: true,
};
const stack = []; // suprapuneri deschise (meniu rapid, panouri, lansare)

const stat = (id) => S.stats[id] || { playtimeSec: 0, lastPlayed: 0, launches: 0 };
const gameById = (id) => S.games.find((g) => g.id === id);
const curGame = () => S.row[S.rowIndex] || null;

function recentGames() {
  return [...S.games]
    .sort((a, b) => (stat(b.id).lastPlayed - stat(a.id).lastPlayed) || a.name.localeCompare(b.name, 'ro'))
    .slice(0, ROW_MAX);
}

// ======================================================================
// Fundal cu tranziție încrucișată
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
  // O mică întârziere: la derulare rapidă nu încărcăm fiecare fundal intermediar.
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
      try { await img.decode(); } catch { /* afișăm oricum */ }
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
// Ecranul principal: rândul de jocuri
// ======================================================================
const rowTrack = $('#row-track');
const rowLabel = $('#row-label');

function tileArt(game) {
  const art = el('div', 'art');
  const ph = el('div', 'ph', esc(game.name));
  ph.style.setProperty('--h', hue(game.id));
  // Jocurile locale fără copertă primesc iconița executabilului, centrată.
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
  S.row = recentGames();
  rowTrack.textContent = '';
  S.rowEls = S.row.map((g, i) => {
    const t = el('div', 'tile');
    t.append(tileArt(g));
    t.style.setProperty('--d', `${i * 40}ms`);
    t.addEventListener('click', () => clickRow(i));
    rowTrack.appendChild(t);
    return t;
  });
  const lib = el('div', 'tile tile-library', `<div class="art">${icons.library}<span>Bibliotecă</span></div>`);
  lib.style.setProperty('--d', `${S.row.length * 40}ms`);
  lib.addEventListener('click', () => clickRow(S.row.length));
  rowTrack.appendChild(lib);
  S.rowEls.push(lib);

  const idx = keepId ? S.row.findIndex((g) => g.id === keepId) : -1;
  S.rowIndex = idx >= 0 ? idx : clamp(S.rowIndex, 0, S.rowEls.length - 1);
  layoutRow(true);
  onRowFocus(true);
}

function layoutRow(instant) {
  const r = remPx;
  const small = 12 * r, big = 18 * r, gap = 1.6 * r, pad = 7 * r;
  const n = S.rowEls.length;
  const f = S.rowIndex;
  const xs = [];
  let x = 0;
  for (let i = 0; i < n; i++) { xs.push(x); x += (i === f ? big : small) + gap; }
  // Păstrăm vizibilă o singură copertă înaintea celei selectate, ca pe consolă.
  const scroll = Math.max(0, xs[f] - (small + gap));
  if (instant) rowTrack.classList.add('instant');
  S.rowEls.forEach((t, i) => {
    const s = i === f ? 1 : small / big;
    t.style.transform = `translate3d(${pad + xs[i] - scroll}px,0,0) scale(${s})`;
    t.classList.toggle('past', i < f - 1);
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
  // Preîncărcăm fundalurile vecinilor, ca tranziția să fie instantanee.
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
// Detaliile jocului selectat
// ======================================================================
const detail = $('#detail');
let detailId = null;

function renderDetail(force) {
  const g = curGame();
  const id = g ? g.id : S.games.length ? '__library' : '__empty';
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
        label: isRunning ? 'Rulează' : 'Joacă',
        cls: 'primary' + (isRunning ? ' muted' : ''),
        icon: isRunning ? '' : icons.play,
        run: () => launch(g),
      });
      S.actions.push({ label: '', aria: 'Mai multe opțiuni', cls: 'round', icon: icons.more, run: () => openGameOptions(g) });

      const st = stat(g.id);
      const facts = [
        ['Platformă', `<span class="platform ${g.source}">${SOURCE[g.source] || g.source}</span>`],
        ['Jucat ultima dată', esc(relTime(st.lastPlayed))],
      ];
      if (st.playtimeSec) facts.push(['Timp jucat', esc(duration(st.playtimeSec))]);
      if (g.sizeBytes) facts.push(['Spațiu ocupat', esc(bytes(g.sizeBytes))]);
      renderActions();
      detail.append(el('dl', 'facts', facts.map(([k, v]) => `<div><dt>${k}</dt><dd>${v}</dd></div>`).join('')));
    } else if (S.games.length) {
      detail.append(el('h1', 'detail-title', 'Biblioteca de jocuri'));
      detail.append(el('p', 'detail-text', `${plural(S.games.length, 'Un joc instalat', 'jocuri instalate')}, din Steam și Xbox.`));
      S.actions.push({ label: 'Deschide biblioteca', cls: 'primary', run: () => showView('library') });
      renderActions();
    } else {
      detail.append(el('h1', 'detail-title', 'Niciun joc găsit'));
      detail.append(el('p', 'detail-text', 'Lumen caută jocurile instalate prin Steam și prin aplicația Xbox. Instalează un joc, apoi reîmprospătează biblioteca.'));
      S.actions.push({ label: 'Reîmprospătează biblioteca', cls: 'primary', run: () => rescan(true) });
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
// Biblioteca
// ======================================================================
const FILTERS = [['all', 'Toate'], ['steam', 'Steam'], ['xbox', 'Xbox'], ['local', 'Locale']];
const grid = $('#grid');
const filtersEl = $('#filters');

function renderFilters() {
  filtersEl.textContent = '';
  FILTERS.forEach(([key, label], i) => {
    const b = el('button', 'chip' + (S.filter === key ? ' active' : ''), label);
    b.addEventListener('click', () => { S.filterIndex = i; applyFilter(); S.zone = 'filters'; updateFocus(); });
    filtersEl.appendChild(b);
  });
}

function applyFilter() {
  const key = FILTERS[S.filterIndex][0];
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
    const t = el('div', 'gtile');
    const art = el('div', 'art');
    const clip = tileArt(g);
    clip.className = 'clip';
    clip.querySelector('img').loading = 'lazy';
    art.appendChild(clip);
    t.append(art, el('div', 'name', esc(g.name)));
    t.addEventListener('click', () => {
      if (stack.length) return;
      if (S.zone === 'grid' && S.libIndex === i) { sound.play('confirm'); launch(g); return; }
      S.zone = 'grid'; S.libIndex = i; sound.play('move'); updateFocus(); scrollGrid();
    });
    grid.appendChild(t);
    return t;
  });
  if (!S.libList.length) {
    const noFolders = S.filter === 'local' && !S.settings.gameFolders?.length;
    grid.appendChild(el('div', 'empty', noFolders
      ? 'Nu ai adăugat încă jocuri locale. Deschide Setări → Adaugă un joc, sau Setări → Foldere cu jocuri.'
      : S.games.length ? 'Niciun joc pentru acest filtru.' : 'Biblioteca e goală. Instalează un joc din Steam sau din aplicația Xbox, apoi reîmprospătează biblioteca din meniul rapid.'));
  }
  const n = S.libList.length;
  $('#lib-count').textContent = n ? plural(n, 'Un joc', 'jocuri') : '';
  const idx = keepId ? S.libList.findIndex((g) => g.id === keepId) : -1;
  S.libIndex = idx >= 0 ? idx : clamp(S.libIndex, 0, Math.max(0, n - 1));
  S.gridScroll = 0;
  grid.style.transform = '';
  if (S.zone === 'grid' && !n) S.zone = 'filters';
  updateFocus();
  scrollGrid();
}

function gridCols() {
  const cols = getComputedStyle(grid).gridTemplateColumns.split(' ').filter(Boolean).length;
  return Math.max(1, cols);
}

function scrollGrid() {
  const t = S.libEls[S.libIndex];
  if (!t) return;
  const wrapH = $('#grid-wrap').clientHeight;
  const top = t.offsetTop;
  const h = t.offsetHeight;
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
// Focus și vederi
// ======================================================================
const tabEls = () => [$('#tab-home'), $('#tab-library'), $('#btn-settings')];

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
  target?.classList.add('focused');
  updateHints();
}

function showView(view) {
  if (S.view === view) return;
  S.view = view;
  $('#home').classList.toggle('hidden', view !== 'home');
  $('#library').classList.toggle('hidden', view !== 'library');
  $('#tab-home').classList.toggle('active', view === 'home');
  $('#tab-library').classList.toggle('active', view === 'library');
  if (view === 'library') {
    S.zone = S.libList.length ? 'grid' : 'filters';
    scrollGrid();
  } else {
    S.zone = 'row';
    setBackground(curGame());
  }
  updateFocus();
}

// ======================================================================
// Gestionarea acțiunilor (tastatură / controller)
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
      if (g) launch(g); else if (S.games.length) showView('library'); else rescan(true);
    } else if (a === 'options') {
      const g = curGame();
      if (g) { sound.play('confirm'); openGameOptions(g); }
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
      const g = curGame(); if (g) openGameOptions(g);
    }
  }
}

function libraryAction(a) {
  if (S.zone === 'grid') {
    const cols = gridCols();
    const n = S.libList.length;
    let ni = S.libIndex;
    if (a === 'left') ni = S.libIndex % cols === 0 ? S.libIndex : S.libIndex - 1;
    else if (a === 'right') ni = Math.min(n - 1, S.libIndex + 1);
    else if (a === 'down') ni = S.libIndex + cols < n ? S.libIndex + cols : (Math.floor(S.libIndex / cols) < Math.floor((n - 1) / cols) ? n - 1 : S.libIndex);
    else if (a === 'up') {
      if (S.libIndex < cols) { S.zone = 'filters'; sound.play('move'); updateFocus(); return; }
      ni = S.libIndex - cols;
    } else if (a === 'confirm') { const g = S.libList[S.libIndex]; if (g) { sound.play('confirm'); launch(g); } return; }
    else if (a === 'options') { const g = S.libList[S.libIndex]; if (g) { sound.play('confirm'); openGameOptions(g); } return; }
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

// L2 / R2 (Home / End): salt la primul sau ultimul joc din rând ori din bibliotecă.
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
  } else {
    const n = S.libList.length;
    if (!n) return;
    const ni = last ? n - 1 : 0;
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
    const ni = clamp(S.tabIndex + (a === 'left' ? -1 : 1), 0, 2);
    if (ni !== S.tabIndex) { S.tabIndex = ni; sound.play('move'); updateFocus(); }
  } else if (a === 'confirm') {
    sound.play('confirm');
    if (S.tabIndex === 0) showView('home');
    else if (S.tabIndex === 1) showView('library');
    else openSettings();
  } else if (a === 'down' || a === 'back') {
    S.zone = S.view === 'home' ? 'row' : (S.libList.length ? 'grid' : 'filters');
    sound.play(a === 'back' ? 'back' : 'move');
    updateFocus();
  }
}

function onAction(a) {
  if (S.booting) return;
  const top = stack[stack.length - 1];
  if (top) { top.handle(a); updateHints(); return; }
  if (a === 'menu') { openQuick(); return; }
  if (a === 'prevTab' || a === 'nextTab') {
    const v = a === 'prevTab' ? 'home' : 'library';
    if (v !== S.view) { sound.play('move'); showView(v); }
    return;
  }
  if (a === 'first' || a === 'last') { jumpToEnd(a === 'last'); return; }
  if (S.zone === 'tabs') tabsAction(a);
  else if (S.view === 'home') homeAction(a);
  else libraryAction(a);
}

// ======================================================================
// Suprapuneri: panouri, meniu rapid, ecran de lansare
// ======================================================================
const scrim = $('#scrim');
scrim.addEventListener('click', () => { const t = stack[stack.length - 1]; if (t && t.type !== 'launch') closeTop(); });

function syncScrim() {
  scrim.classList.toggle('show', stack.some((o) => o.type !== 'launch'));
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

/** Panou lateral cu o listă verticală de opțiuni. */
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
        (it.toggle ? `<span class="switch ${it.toggle() ? 'on' : ''}"><i></i></span>` : '');
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
    if (it.toggle && panel.isConnected) render();
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
      { label: 'Anulează', action: () => closeTop() },
    ],
  }));
}

function openSettings() {
  const toggle = (key, label, desc) => ({
    label, desc,
    toggle: () => !!S.settings[key],
    action: () => setSetting(key, !S.settings[key]),
  });
  pushOverlay(listPanel({
    title: 'Setări',
    note: `Lumen ${S.version}`,
    items: [
      toggle('autostart', 'Pornește odată cu Windows', 'Lumen se deschide în ecran complet după autentificare'),
      toggle('minimizeOnLaunch', 'Minimizează când pornește un joc', 'Lumen revine singur după ce închizi jocul'),
      toggle('sounds', 'Sunete în meniu'),
      toggle('music', 'Muzică de fundal', S.theme.music ? '' : 'Adaugă un fișier music.mp3 în folderul temei'),
      toggle('onlineArt', 'Imagini online pentru Steam', 'Descarcă coperțile care lipsesc din cache-ul Steam'),
      toggle('reduceMotion', 'Reduce animațiile'),
      { label: 'Adaugă un joc', desc: 'Alege executabilul (.exe) unui joc din afara Steam și Xbox', action: addGame },
      {
        label: 'Foldere cu jocuri',
        desc: S.settings.gameFolders?.length ? S.settings.gameFolders.join(', ') : 'Adaugă folderele cu jocuri din afara Steam și Xbox',
        action: openGameFolders,
      },
      { label: 'Jocuri ascunse', desc: 'Readu în bibliotecă jocurile eliminate', action: openHiddenGames },
      { label: 'Deschide folderul temei', desc: 'Sunete, muzică, font și fundal personalizat', action: () => call('open_path', { path: S.themeDir }).catch((e) => toast(String(e), 'error')) },
      { label: 'Reîmprospătează biblioteca', action: () => { closeTop(true); rescan(true); } },
    ],
  }));
}

// Numele, iconița și (dacă jocul e și pe Steam) coperta se completează automat la reîmprospătare.
async function addGame() {
  try {
    const name = await call('add_game');
    if (!name) return;
    closeAll();
    toast(`${name} a fost adăugat. Se caută imaginile…`);
    rescan(false);
  } catch (e) {
    toast(String(e), 'error');
  }
}

// Jocurile adăugate manual se șterg de tot; celelalte doar se ascund și pot fi readuse din Setări.
function removeGame(g) {
  const manual = g.id.startsWith('manual:');
  const note = manual
    ? 'Jocul dispare doar din Lumen; fișierele lui rămân pe disc.'
    : 'Jocul rămâne instalat. Îl poți readuce din Setări → Jocuri ascunse.';
  confirmPanel(`Elimini ${g.name}?`, note, 'Elimină din bibliotecă', async () => {
    try {
      await call('remove_game', { id: g.id });
      closeAll();
      // Scoatem jocul pe loc, fără o scanare completă.
      S.games = S.games.filter((x) => x.id !== g.id);
      renderRow();
      renderLibrary();
      toast(`${g.name} a fost eliminat din bibliotecă.`);
    } catch (e) {
      toast(String(e), 'error');
    }
  });
}

async function openHiddenGames() {
  let hidden = [];
  try { hidden = (await call('hidden_games')) || []; } catch (e) { toast(String(e), 'error'); return; }
  pushOverlay(listPanel({
    title: 'Jocuri ascunse',
    note: hidden.length ? 'Alege un joc ca să-l readuci în bibliotecă.' : 'Nu ai ascuns niciun joc.',
    items: hidden.map((h) => ({
      label: h.name,
      desc: 'Readu în bibliotecă',
      action: async () => {
        try {
          await call('restore_game', { id: h.id });
          closeAll();
          toast(`${h.name} revine în bibliotecă.`);
          rescan(false);
        } catch (e) { toast(String(e), 'error'); }
      },
    })),
  }));
}

// Fiecare subfolder al unui folder de aici apare în bibliotecă ca joc „Local”.
function openGameFolders() {
  const reopen = () => { closeAll(); openSettings(); openGameFolders(); };
  const folders = S.settings.gameFolders || [];
  pushOverlay(listPanel({
    title: 'Foldere cu jocuri',
    note: 'Fiecare subfolder e considerat un joc. Jocurile Xbox din aceste foldere sunt deja importate.',
    items: [
      ...folders.map((path) => ({
        label: path,
        desc: 'Elimină folderul',
        action: () => confirmPanel('Elimini folderul?', path, 'Elimină', async () => {
          try {
            S.settings = await call('remove_game_folder', { path });
            reopen();
            rescan(true);
          } catch (e) { toast(String(e), 'error'); }
        }),
      })),
      {
        label: 'Adaugă folder',
        desc: 'Se deschide fereastra Windows de alegere a folderului',
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

function openGameOptions(g) {
  const running = S.running?.id === g.id;
  const items = [
    running
      ? { label: 'Închide jocul', danger: true, action: () => { closeTop(true); askCloseGame(); } }
      : { label: 'Joacă', action: () => { closeTop(true); launch(g); } },
    { label: 'Deschide locația jocului', desc: g.installDir, action: () => call('open_path', { path: g.installDir }).catch((e) => toast(String(e), 'error')) },
    { label: 'Reîmprospătează biblioteca', action: () => { closeTop(true); rescan(true); } },
  ];
  if (!running) items.push({ label: 'Elimină din bibliotecă', danger: true, action: () => removeGame(g) });
  pushOverlay(listPanel({ title: g.name, note: `${SOURCE[g.source]} · ${relTime(stat(g.id).lastPlayed)}`, items }));
}

function askCloseGame() {
  const g = gameById(S.running?.id);
  if (!g) return;
  confirmPanel(`Închizi ${g.name}?`, 'Progresul nesalvat se va pierde.', 'Închide jocul', async () => {
    try { await call('close_game'); } catch (e) { toast(String(e), 'error'); }
  });
}

function openPower() {
  pushOverlay(listPanel({
    title: 'Alimentare',
    items: [
      { label: 'Intră în repaus', action: () => { closeAll(); call('power', { action: 'sleep' }); } },
      { label: 'Repornește PC-ul', danger: true, action: () => confirmPanel('Repornești PC-ul?', 'Aplicațiile deschise se vor închide.', 'Repornește', () => call('power', { action: 'restart' })) },
      { label: 'Oprește PC-ul', danger: true, action: () => confirmPanel('Oprești PC-ul?', 'Aplicațiile deschise se vor închide.', 'Oprește', () => call('power', { action: 'shutdown' })) },
      { label: 'Ieși în Windows', desc: 'Închide Lumen', action: () => call('exit_app') },
    ],
  }));
}

/** Meniul rapid: bandă orizontală jos, deschisă cu Guide / Menu / Tab. */
function openQuick() {
  const running = gameById(S.running?.id);
  const cards = [
    { label: 'Acasă', icon: icons.home, run: () => { closeAll(); showView('home'); } },
    { label: 'Bibliotecă', icon: icons.library, run: () => { closeAll(); showView('library'); } },
  ];
  if (running) cards.push({ label: `Închide ${running.name}`, icon: icons.close, danger: true, run: () => askCloseGame() });
  cards.push(
    { label: 'Reîmprospătează biblioteca', icon: icons.refresh, run: () => { closeAll(); rescan(true); } },
    { label: 'Setări', icon: icons.gear, run: () => openSettings() },
    { label: 'Alimentare', icon: icons.power, run: () => openPower() },
  );

  const box = el('div', 'quick');
  const time = new Date().toLocaleTimeString('ro-RO', { hour: '2-digit', minute: '2-digit' });
  box.innerHTML = `<div class="quick-top"><strong>${running ? esc(running.name) : 'Meniu rapid'}</strong><span>${running ? (S.running.started ? 'Rulează acum' : 'Se pornește') : esc(time)}</span></div><div class="quick-cards"></div>`;
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

/** Ecranul afișat cât timp jocul pornește. */
let launchOverlay = null;
function showLaunch(g) {
  const box = el('div', 'launch');
  box.innerHTML = `<div class="cover"></div><h2>${esc(g.name)}</h2><p>Se pornește…</p><div class="bar"><i></i></div>`;
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
// Lansarea jocurilor
// ======================================================================
async function launch(g) {
  if (S.running) {
    if (S.running.id === g.id) toast(`${g.name} rulează deja.`);
    else toast(`Închide mai întâi ${gameById(S.running.id)?.name || 'jocul curent'}.`, 'error');
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
// Notificări, indicii, ceas
// ======================================================================
function toast(msg, kind = 'info') {
  const t = el('div', `toast ${kind}`, esc(msg));
  $('#toasts').appendChild(t);
  nextFrame().then(() => t.classList.add('in'));
  setTimeout(() => { t.classList.remove('in'); setTimeout(() => t.remove(), 420); }, 4800);
}

// Simbolurile PlayStation, desenate cu culorile lor clasice.
const psSvg = (color, body) =>
  `<svg viewBox="0 0 16 16" fill="none" stroke="${color}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
// Meniul rapid e pe butonul din stânga (View / Create), opțiunile jocului pe cel din dreapta (☰).
// Y / △ deschid în continuare opțiunile, dar indiciul arată butonul din dreapta.
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
  if (top?.type === 'launch') list = S.running?.started ? [['menu', 'Meniu']] : [['back', 'Anulează'], ['menu', 'Meniu']];
  else if (top) list = [['confirm', 'Selectează'], ['back', 'Înapoi']];
  else if (S.view === 'home' && S.zone === 'row' && curGame()) list = [['confirm', 'Joacă'], ['options', 'Opțiuni'], ['menu', 'Meniu']];
  else if (S.view === 'library' && S.zone === 'grid') list = [['confirm', 'Joacă'], ['options', 'Opțiuni'], ['back', 'Înapoi'], ['menu', 'Meniu']];
  else list = [['confirm', 'Selectează'], ['menu', 'Meniu']];
  $('#hints').innerHTML = list.map(([k, label]) => `<span class="hint"><span class="glyph">${g[k]}</span>${label}</span>`).join('');
}

function tickClock() {
  $('#clock').textContent = new Date().toLocaleTimeString('ro-RO', { hour: '2-digit', minute: '2-digit' });
}

// Bateria apare doar pe dispozitivele care au una (laptop, consolă portabilă).
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
  try { b = await call('battery'); } catch { /* fără informații despre baterie */ }
  const box = $('#battery');
  box.hidden = !b;
  if (!b) return;
  const charging = b.charging || b.plugged;
  box.classList.toggle('charging', charging);
  box.classList.toggle('low', !charging && b.percent <= 20);
  $('.batt-icon', box).innerHTML = batteryIcon(b.percent, charging);
  $('.batt-pct', box).textContent = `${b.percent}%`;
  box.setAttribute('aria-label', `Baterie ${b.percent}%${charging ? ', se încarcă' : ''}`);
}

// Bateria controlerelor: DualSense/DualShock prin HID, Xbox prin XInput.
async function tickPads() {
  let pads = [];
  try { pads = (await call('pad_batteries')) || []; } catch { /* fără controlere */ }
  $('#pad-batteries').innerHTML = pads.map((p) => {
    const cls = p.charging ? ' charging' : p.percent <= 20 ? ' low' : '';
    const name = p.kind === 'playstation' ? 'Controler PlayStation' : 'Controler Xbox';
    return `<div class="battery pad${cls}" aria-label="${name}: ${p.percent}%${p.charging ? ', se încarcă' : ''}">
      <span class="pad-icon">${icons.gamepad}</span><span class="batt-icon">${batteryIcon(p.percent, p.charging)}</span><span class="batt-pct">${p.percent}%</span>
    </div>`;
  }).join('');
}

// ======================================================================
// Biblioteca: încărcare și reîmprospătare
// ======================================================================
let manualScan = false;
function rescan(manual) {
  manualScan = !!manual;
  if (manual) toast('Se caută jocurile instalate…');
  call('scan_library');
}

function onLibrary({ games, warnings }) {
  const keepRow = curGame()?.id;
  const keepLib = S.libList[S.libIndex]?.id;
  S.games = games;
  probeCache.clear();
  renderRow(keepRow);
  renderLibrary(keepLib);
  if (manualScan) toast(`Biblioteca a fost actualizată: ${plural(games.length, 'un joc', 'jocuri')}.`);
  // Avertismentele (de ex. Steam lipsă) contează doar dacă nu s-a găsit nimic.
  if (warnings?.length && (manualScan || !games.length)) warnings.forEach((w) => toast(w, 'error'));
  manualScan = false;
}

// ======================================================================
// Temă personalizată
// ======================================================================
async function loadTheme() {
  const t = S.theme;
  if (!isTauri) return;
  if (t.font) {
    try {
      const buf = await call('theme_file', { name: 'font' });
      const face = new FontFace('LumenTheme', buf);
      await face.load();
      document.fonts.add(face);
      document.documentElement.style.setProperty('--font', '"LumenTheme", "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif');
      measureRem();
    } catch { /* fontul rămâne cel implicit */ }
  }
  for (const name of ['move', 'confirm', 'back', 'open', 'launch']) {
    if (t[name]) call('theme_file', { name }).then((buf) => sound.load(name, buf)).catch(() => {});
  }
  if (t.music) sound.setMusic(fileUrl(t.music));
}

// ======================================================================
// Pornire
// ======================================================================
async function boot() {
  measureRem();
  $('#btn-settings').innerHTML = icons.gear;
  tickClock();
  setInterval(tickClock, 10_000);
  tickBattery();
  setInterval(tickBattery, 30_000);
  tickPads();
  setInterval(tickPads, 30_000);
  // Controlerul abia conectat are nevoie de o clipă până răspunde prin HID.
  for (const ev of ['gamepadconnected', 'gamepaddisconnected']) window.addEventListener(ev, () => setTimeout(tickPads, 800));

  const input = createInput({
    onAction,
    onDevice: (d, pad) => { S.device = d; S.pad = pad; document.body.classList.toggle('hide-cursor', d === 'gamepad'); updateHints(); },
  });
  S.device = input.device;
  S.pad = input.pad;

  // Cursorul dispare după câteva secunde de inactivitate.
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
  $('#btn-settings').addEventListener('click', () => { if (!stack.length) { sound.play('confirm'); openSettings(); } });

  // Starea salvată se afișează imediat; scanarea completă vine în fundal.
  const snap = await call('get_state');
  Object.assign(S, {
    games: snap.games || [],
    stats: snap.stats || {},
    settings: snap.settings || {},
    theme: snap.theme || {},
    themeDir: snap.themeDir || '',
    running: snap.running || null,
    user: snap.user || '',
    version: snap.version || '',
  });
  $('#avatar').textContent = (S.user || 'J').trim().charAt(0).toUpperCase();
  await loadTheme();
  applySettings();
  renderFilters();
  renderRow();
  renderLibrary();
  updateRunning();

  on('library', onLibrary);
  on('guide', () => {
    const top = stack[stack.length - 1];
    if (top?.type === 'quick') closeTop(); else openQuick();
  });
  on('game-started', ({ id }) => {
    S.running = { id, started: true };
    launchOverlay?.box.classList.add('started');
    const p = launchOverlay && $('p', launchOverlay.box);
    if (p) p.textContent = 'Rulează';
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
    if (!detected) toast('Jocul nu a pornit în 3 minute, așa că timpul de joc nu a fost înregistrat.', 'error');
    else if (g) toast(`Ai jucat ${g.name} ${duration(seconds)}.`);
    applySettings();
  });
  document.addEventListener('visibilitychange', applySettings);

  if (isTauri) await call('app_ready');
  await nextFrame();
  S.booting = false;
  document.body.classList.remove('booting');
  updateHints();
  rescan(false);
}

boot();
