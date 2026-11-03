// Screen Wake Lock for web pages (navigator.wakeLock.request("screen")).
//
// QtWebEngine has no permission type for wake locks and refuses every
// request without asking, so Kite implements the API: this main-world
// script asks Kite through DOM events relayed by the bridge world
// (bridge.js); Kite prompts the user and keeps the screen on while a lock
// is held. Top-level documents only (Kite's bridge lives there).
(function () {
  'use strict';
  if (window !== top || typeof WakeLock === 'undefined' || !WakeLock.prototype.request) return;
  const REQUEST = 'kite-wakelock-request', RELEASE = 'kite-wakelock-release', ANSWER = 'kite-wakelock-answer';
  const send = (name, detail) => document.dispatchEvent(new CustomEvent(name, { detail: String(detail) }));
  const notAllowed = (msg) => new DOMException(msg, 'NotAllowedError');
  let nextId = 1;
  const pending = new Map();
  const held = new Set();

  class WakeLockSentinel extends EventTarget {
    #released = false;
    #onrelease = null;
    get released() { return this.#released; }
    get type() { return 'screen'; }
    get onrelease() { return this.#onrelease; }
    set onrelease(fn) {
      if (this.#onrelease) this.removeEventListener('release', this.#onrelease);
      this.#onrelease = typeof fn === 'function' ? fn : null;
      if (this.#onrelease) this.addEventListener('release', this.#onrelease);
    }
    release() {
      if (!this.#released) {
        this.#released = true;
        held.delete(this);
        if (held.size === 0) send(RELEASE, '');
        this.dispatchEvent(new Event('release'));
      }
      return Promise.resolve();
    }
    get [Symbol.toStringTag]() { return 'WakeLockSentinel'; }
  }

  document.addEventListener(ANSWER, (e) => {
    let a; try { a = JSON.parse(e.detail); } catch (x) { return; }
    const p = pending.get(a.id);
    if (!p) return;
    pending.delete(a.id);
    if (!a.granted) { p.reject(notAllowed('Wake Lock permission request denied')); return; }
    if (document.hidden) {
      if (held.size === 0) send(RELEASE, '');
      p.reject(notAllowed('The requesting page is not visible'));
      return;
    }
    const s = new WakeLockSentinel();
    held.add(s);
    p.resolve(s);
  });

  Object.defineProperty(WakeLock.prototype, 'request', {
    configurable: true,
    writable: true,
    value: function request(type = 'screen') {
      if (type !== 'screen') return Promise.reject(new TypeError(`The provided value '${type}' is not a valid enum value of type WakeLockType.`));
      if (document.hidden) return Promise.reject(notAllowed('The requesting page is not visible'));
      return new Promise((resolve, reject) => {
        const id = nextId++;
        pending.set(id, { resolve, reject });
        send(REQUEST, id);
      });
    },
  });

  // Screen wake locks end when the page is hidden or goes away.
  const releaseAll = () => { for (const s of [...held]) s.release(); };
  document.addEventListener('visibilitychange', () => { if (document.hidden) releaseAll(); });
  addEventListener('pagehide', releaseAll);
})();
