// Intrare unificată: tastatură, gamepad (Gamepad API), rotița mouse-ului.
// Emite acțiuni abstracte: up, down, left, right, confirm, back, menu, options,
// prevTab, nextTab, first, last. Direcțiile se repetă când sunt ținute apăsate.

const KEYS = {
  ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right',
  Enter: 'confirm', ' ': 'confirm',
  Escape: 'back', Backspace: 'back',
  Tab: 'menu', F1: 'menu',
  o: 'options', O: 'options',
  q: 'prevTab', Q: 'prevTab', e: 'nextTab', E: 'nextTab',
  Home: 'first', End: 'last',
};

// Maparea „standard” din Gamepad API (Xbox / DualSense în Chromium).
const BUTTONS = {
  // 8 = View / Share (stânga), 9 = Menu / Options (dreapta). Butonul 16 (Guide / PS) lipsește
  // intenționat: îl citește nativ `guide.rs` și emite evenimentul „guide”; mapat și aici,
  // o apăsare ar deschide meniul rapid și l-ar închide imediat.
  0: 'confirm', 1: 'back', 3: 'options', 8: 'menu', 9: 'options',
  4: 'prevTab', 5: 'nextTab', 6: 'first', 7: 'last', // LB / RB, LT / RT (L1 / R1, L2 / R2)
  12: 'up', 13: 'down', 14: 'left', 15: 'right',
};
const DIRS = new Set(['up', 'down', 'left', 'right']);
// Controlerele Sony (DualShock 4, DualSense) au vendor ID 054c; WebView2 îl pune în `id`.
const SONY = /054c|dualsense|dualshock|playstation|wireless controller/i;
const padKind = (p) => (SONY.test(p?.id || '') ? 'playstation' : 'xbox');
const REPEAT_DELAY = 360;
const REPEAT_RATE = 95;
const STICK = 0.55;

export function createInput({ onAction, onDevice }) {
  let device = 'keyboard';
  let pad = 'xbox'; // tipul ultimului gamepad folosit: 'xbox' sau 'playstation'
  const setDevice = (d, kind = pad) => {
    if (d === device && kind === pad) return;
    device = d;
    pad = kind;
    onDevice?.(d, pad);
  };

  // --- Tastatură
  window.addEventListener('keydown', (ev) => {
    // Blocăm scurtăturile de browser care ar strica experiența de consolă.
    if (ev.key === 'F5' || (ev.ctrlKey && /^[rpfsu+=\-0]$/i.test(ev.key))) { ev.preventDefault(); return; }
    const a = KEYS[ev.key];
    if (!a) return;
    ev.preventDefault();
    if (ev.repeat && !DIRS.has(a)) return;
    setDevice('keyboard');
    onAction(a);
  });

  // --- Rotița: un pas de focus pe „tick”
  let wheelAt = 0;
  window.addEventListener('wheel', (ev) => {
    const now = performance.now();
    if (now - wheelAt < 110) return;
    wheelAt = now;
    const vertical = Math.abs(ev.deltaY) >= Math.abs(ev.deltaX);
    const d = vertical ? ev.deltaY : ev.deltaX;
    if (!d) return;
    onAction(vertical ? (d > 0 ? 'down' : 'up') : (d > 0 ? 'right' : 'left'), { wheel: true });
  }, { passive: true });

  // --- Gamepad
  const held = new Map(); // acțiune -> { next: timestamp }
  let polling = false;

  function frame(now) {
    const pads = navigator.getGamepads ? [...navigator.getGamepads()].filter(Boolean) : [];
    if (!pads.length) { polling = false; held.clear(); return; }
    const active = new Map(); // acțiune -> gamepad-ul care a declanșat-o
    for (const p of pads) {
      p.buttons.forEach((b, i) => { if (b.pressed && BUTTONS[i]) active.set(BUTTONS[i], p); });
      const [x = 0, y = 0] = p.axes;
      if (x < -STICK) active.set('left', p); else if (x > STICK) active.set('right', p);
      if (y < -STICK) active.set('up', p); else if (y > STICK) active.set('down', p);
    }
    // Windows trimite starea controlerului și ferestrelor inactive: cât joci, apăsările
    // sunt ale jocului. Le marcăm ca ținute, ca la revenire să nu se declanșeze singure.
    if (!document.hasFocus()) {
      held.clear();
      for (const a of active.keys()) held.set(a, { next: Infinity });
      requestAnimationFrame(frame);
      return;
    }
    for (const [a, p] of active) {
      const h = held.get(a);
      if (!h) {
        held.set(a, { next: now + REPEAT_DELAY });
        setDevice('gamepad', padKind(p));
        onAction(a);
      } else if (DIRS.has(a) && now >= h.next) {
        h.next = now + REPEAT_RATE;
        onAction(a, { repeat: true });
      }
    }
    for (const a of [...held.keys()]) if (!active.has(a)) held.delete(a);
    requestAnimationFrame(frame);
  }
  function startPolling() {
    if (polling) return;
    polling = true;
    requestAnimationFrame(frame);
  }
  window.addEventListener('gamepadconnected', (e) => { setDevice('gamepad', padKind(e.gamepad)); startPolling(); });
  window.addEventListener('focus', startPolling);
  // Un controler deja conectat la pornire stabilește tipul indiciilor de la început.
  const first = navigator.getGamepads ? [...navigator.getGamepads()].find(Boolean) : null;
  if (first) pad = padKind(first);
  startPolling();

  return { get device() { return device; }, get pad() { return pad; }, setDevice };
}
