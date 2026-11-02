// Puntea dintre interfață și backend-ul Rust.
// În afara aplicației (deschis direct în browser) folosește date simulate,
// ca interfața să poată fi dezvoltată și testată fără Windows.
import * as mock from './mock.js';

const T = window.__TAURI__;
export const isTauri = !!(T && T.core);

export function call(cmd, args = {}) {
  return isTauri ? T.core.invoke(cmd, args) : mock.invoke(cmd, args);
}

export function on(event, cb) {
  return isTauri ? T.event.listen(event, (e) => cb(e.payload)) : mock.listen(event, cb);
}

/** Transformă o cale locală într-un URL pe care webview-ul îl poate încărca. */
export function fileUrl(p) {
  if (!p) return '';
  if (/^(https?:|data:|blob:)/i.test(p)) return p;
  return isTauri ? T.core.convertFileSrc(p) : p;
}
