// Interface sounds. By default they are synthesized with Web Audio (no files),
// but any file in the theme folder (move, confirm, back, open, launch) replaces them.

let ctx = null;
let master = null;
let enabled = true;
const buffers = {};

function audio() {
  if (!ctx) {
    ctx = new (window.AudioContext || window.webkitAudioContext)();
    master = ctx.createGain();
    master.gain.value = 0.55;
    const lp = ctx.createBiquadFilter();
    lp.type = 'lowpass';
    lp.frequency.value = 5200;
    master.connect(lp).connect(ctx.destination);
  }
  if (ctx.state === 'suspended') ctx.resume();
  return ctx;
}

function tone(c, t, { type = 'sine', f0, f1 = f0, dur, gain }) {
  const o = c.createOscillator();
  const g = c.createGain();
  o.type = type;
  o.frequency.setValueAtTime(f0, t);
  if (f1 !== f0) o.frequency.exponentialRampToValueAtTime(f1, t + dur);
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(gain, t + 0.006);
  g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
  o.connect(g).connect(master);
  o.start(t);
  o.stop(t + dur + 0.03);
}

const synth = {
  move(c, t) {
    tone(c, t, { f0: 1480, f1: 1320, dur: 0.045, gain: 0.07 });
    tone(c, t, { type: 'triangle', f0: 740, dur: 0.035, gain: 0.03 });
  },
  confirm(c, t) {
    tone(c, t, { f0: 880, dur: 0.07, gain: 0.08 });
    tone(c, t + 0.055, { f0: 1320, dur: 0.11, gain: 0.07 });
  },
  back(c, t) {
    tone(c, t, { f0: 760, f1: 540, dur: 0.09, gain: 0.07 });
  },
  open(c, t) {
    [523.25, 659.25, 783.99].forEach((f, i) => tone(c, t + i * 0.035, { f0: f, dur: 0.22, gain: 0.045 }));
  },
  launch(c, t) {
    tone(c, t, { type: 'triangle', f0: 220, f1: 660, dur: 0.5, gain: 0.05 });
    [523.25, 783.99, 1046.5].forEach((f, i) => tone(c, t + 0.18 + i * 0.06, { f0: f, dur: 0.6, gain: 0.04 }));
  },
};

export function setEnabled(on) { enabled = !!on; }

export function play(name) {
  if (!enabled) return;
  try {
    const c = audio();
    const buf = buffers[name];
    if (buf) {
      const src = c.createBufferSource();
      src.buffer = buf;
      src.connect(master);
      src.start();
    } else {
      synth[name]?.(c, c.currentTime + 0.001);
    }
  } catch { /* the sound is not essential */ }
}

/** Loads a custom sound (ArrayBuffer from the theme folder). */
export async function load(name, arrayBuffer) {
  try {
    buffers[name] = await audio().decodeAudioData(arrayBuffer);
  } catch { /* invalid file: the synthesized sound stays */ }
}

// --- Background music
let music = null;
export function setMusic(url) {
  if (!url) return;
  music = new Audio(url);
  music.loop = true;
  music.volume = 0;
}
export function musicPlay(on) {
  if (!music) return;
  if (on) {
    music.play().then(() => fade(0.28)).catch(() => {});
  } else {
    fade(0, () => music.pause());
  }
}
let fadeTimer = null;
function fade(target, done) {
  clearInterval(fadeTimer);
  fadeTimer = setInterval(() => {
    const v = music.volume + (target > music.volume ? 0.02 : -0.03);
    if ((target > music.volume && v >= target) || (target <= music.volume && v <= target)) {
      music.volume = Math.max(0, Math.min(1, target));
      clearInterval(fadeTimer);
      done?.();
    } else {
      music.volume = Math.max(0, Math.min(1, v));
    }
  }, 50);
}
