// Storage for third-party frames that are refused it. Blocking third-party cookies
// also denies them storage: Chromium has one check for both, and Qt's cookie filter
// can't tell them apart. Most sites expect it to work, so video players embedded from
// elsewhere crash on reading localStorage and show nothing. Here such a frame gets
// storage that's only in its own memory: it works while the frame is open, and
// isn't saved or shared with anything, so it can't be used to follow anyone around.
(function (window) {
  'use strict';
  if (window === window.top) return;
  // Sandboxed frames are refused by the standard: that's meant.
  if (window.origin === 'null') return;

  function memoryStorage(StoragePrototype) {
    const data = new Map();
    const methods = {
      get length() { return data.size; },
      key: (i) => { const k = Array.from(data.keys())[i]; return k === undefined ? null : k; },
      getItem: (k) => { const v = data.get(String(k)); return v === undefined ? null : v; },
      setItem: (k, v) => { data.set(String(k), String(v)); },
      removeItem: (k) => { data.delete(String(k)); },
      clear: () => { data.clear(); },
    };
    return new Proxy(methods, {
      get: (t, k) => (typeof k === 'symbol' || k in methods ? t[k] : methods.getItem(k)),
      set: (t, k, v) => { if (typeof k !== 'symbol') methods.setItem(k, v); return true; },
      has: (t, k) => k in methods || data.has(String(k)),
      deleteProperty: (t, k) => { data.delete(String(k)); return true; },
      ownKeys: () => Array.from(data.keys()),
      getOwnPropertyDescriptor: (t, k) => (data.has(String(k)) ? { value: data.get(String(k)), writable: true, enumerable: true, configurable: true } : undefined),
      getPrototypeOf: () => StoragePrototype,
    });
  }

  for (const name of ['localStorage', 'sessionStorage']) {
    try {
      void window[name];
    } catch (e) {
      if (e.name !== 'SecurityError') continue;
      const store = memoryStorage(window.Storage ? window.Storage.prototype : Object.prototype);
      Object.defineProperty(window, name, { get: () => store, configurable: true, enumerable: true });
    }
  }
})(globalThis);
