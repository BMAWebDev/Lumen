// Unified input: keyboard, gamepad (Gamepad API), mouse wheel.
// Emits abstract actions: up, down, left, right, confirm, back, menu, options,
// prevTab, nextTab, first, last. Directions repeat while held down.

const KEYS = {
  ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right',
  Enter: 'confirm', ' ': 'confirm',
  Escape: 'back', Backspace: 'back',
  Tab: 'menu', F1: 'menu',
  o: 'options', O: 'options',
  q: 'prevTab', Q: 'prevTab', e: 'nextTab', E: 'nextTab',
  Home: 'first', End: 'last',
};

// The "standard" mapping of the Gamepad API (Xbox / DualSense in Chromium).
const BUTTONS = {
  // 8 = View / Share (left), 9 = Menu / Options (right). Button 16 (Guide / PS) is missing
  // on purpose: `guide.rs` reads it natively and emits the "guide" event; if it were mapped
  // here too, one press would open the quick menu and close it right away.
  0: 'confirm', 1: 'back', 2: 'delete', 3: 'options', // 2 = square / X: deletes in the text popup only 8: 'menu', 9: 'options',
  4: 'prevTab', 5: 'nextTab', 6: 'first', 7: 'last', // LB / RB, LT / RT (L1 / R1, L2 / R2)
  12: 'up', 13: 'down', 14: 'left', 15: 'right',
  11: 'mode', // R3: changes the character set of the on-screen keyboard
};
const DIRS = new Set(['up', 'down', 'left', 'right']);
const REPEATING = new Set([...DIRS, 'delete']); // held buttons that keep firing
// Sony controllers (DualShock 4, DualSense) have vendor ID 054c; WebView2 puts it in `id`.
const SONY = /054c|dualsense|dualshock|playstation|wireless controller/i;
const padKind = (p) => (SONY.test(p?.id || '') ? 'playstation' : 'xbox');
const REPEAT_DELAY = 360;
const REPEAT_RATE = 95;
const STICK = 0.55;

export function createInput({ onAction, onDevice }) {
  let device = 'keyboard';
  let pad = 'xbox'; // type of the last gamepad used: 'xbox' or 'playstation'
  const setDevice = (d, kind = pad) => {
    if (d === device && kind === pad) return;
    device = d;
    pad = kind;
    onDevice?.(d, pad);
  };

  // --- Keyboard
  window.addEventListener('keydown', (ev) => {
    // Block browser shortcuts that would break the console experience.
    if (ev.key === 'F5' || (ev.ctrlKey && /^[rpfsu+=\-0]$/i.test(ev.key))) { ev.preventDefault(); return; }
    let a = KEYS[ev.key];
    // Typing in a text field: only Enter and Escape reach the interface; everything else is text.
    if (ev.target instanceof HTMLInputElement && ev.key !== 'Enter' && ev.key !== 'Escape') a = null;
    if (!a) return;
    ev.preventDefault();
    if (ev.repeat && !DIRS.has(a)) return;
    setDevice('keyboard');
    onAction(a);
  });

  // --- Wheel: one focus step per tick
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
  const held = new Map(); // action -> { next: timestamp }
  let polling = false;

  function frame(now) {
    const pads = navigator.getGamepads ? [...navigator.getGamepads()].filter(Boolean) : [];
    if (!pads.length) { polling = false; held.clear(); return; }
    const active = new Map(); // action -> the gamepad that triggered it
    for (const p of pads) {
      p.buttons.forEach((b, i) => { if (b.pressed && BUTTONS[i]) active.set(BUTTONS[i], p); });
      const [x = 0, y = 0] = p.axes;
      if (x < -STICK) active.set('left', p); else if (x > STICK) active.set('right', p);
      if (y < -STICK) active.set('up', p); else if (y > STICK) active.set('down', p);
    }
    // Windows sends the controller state to inactive windows too: while you play, the presses
    // belong to the game. We mark them as held so they don't fire on their own when we regain focus.
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
      } else if (REPEATING.has(a) && now >= h.next) {
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
  // A controller already connected at startup sets the type of the hints from the start.
  const first = navigator.getGamepads ? [...navigator.getGamepads()].find(Boolean) : null;
  if (first) pad = padKind(first);
  startPolling();

  return { get device() { return device; }, get pad() { return pad; }, setDevice };
}
