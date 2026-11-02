// Intrare unificată: tastatură, gamepad (Gamepad API), rotița mouse-ului.
// Emite acțiuni abstracte: up, down, left, right, confirm, back, menu, options,
// prevTab, nextTab. Direcțiile se repetă când sunt ținute apăsate.

const KEYS = {
  ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right',
  Enter: 'confirm', ' ': 'confirm',
  Escape: 'back', Backspace: 'back',
  Tab: 'menu', F1: 'menu',
  o: 'options', O: 'options',
  q: 'prevTab', Q: 'prevTab', e: 'nextTab', E: 'nextTab',
};

// Maparea „standard” din Gamepad API (Xbox / DualSense în Chromium).
const BUTTONS = {
  0: 'confirm', 1: 'back', 3: 'options', 8: 'options', 9: 'menu', 16: 'menu',
  4: 'prevTab', 5: 'nextTab',
  12: 'up', 13: 'down', 14: 'left', 15: 'right',
};
const DIRS = new Set(['up', 'down', 'left', 'right']);
const REPEAT_DELAY = 360;
const REPEAT_RATE = 95;
const STICK = 0.55;

export function createInput({ onAction, onDevice }) {
  let device = 'keyboard';
  const setDevice = (d) => { if (d !== device) { device = d; onDevice?.(d); } };

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
    const active = new Set();
    for (const p of pads) {
      p.buttons.forEach((b, i) => { if (b.pressed && BUTTONS[i]) active.add(BUTTONS[i]); });
      const [x = 0, y = 0] = p.axes;
      if (x < -STICK) active.add('left'); else if (x > STICK) active.add('right');
      if (y < -STICK) active.add('up'); else if (y > STICK) active.add('down');
    }
    for (const a of active) {
      const h = held.get(a);
      if (!h) {
        held.set(a, { next: now + REPEAT_DELAY });
        setDevice('gamepad');
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
  window.addEventListener('gamepadconnected', () => { setDevice('gamepad'); startPolling(); });
  window.addEventListener('focus', startPolling);
  startPolling();

  return { get device() { return device; }, setDevice };
}
