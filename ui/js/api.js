// The bridge between the interface and the Rust backend.
// Outside the app (opened directly in a browser) it uses mock data,
// so the interface can be developed and tested without Windows.
import * as mock from './mock.js';

const T = window.__TAURI__;
export const isTauri = !!(T && T.core);

export function call(cmd, args = {}) {
  return isTauri ? T.core.invoke(cmd, args) : mock.invoke(cmd, args);
}

export function on(event, cb) {
  return isTauri ? T.event.listen(event, (e) => cb(e.payload)) : mock.listen(event, cb);
}

/** Turns a local path into a URL the webview can load. */
export function fileUrl(p) {
  if (!p) return '';
  if (/^(https?:|data:|blob:)/i.test(p)) return p;
  return isTauri ? T.core.convertFileSrc(p) : p;
}
