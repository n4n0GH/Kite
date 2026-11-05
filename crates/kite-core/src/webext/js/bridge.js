// Kite bridge world (QWebEngineScript::ApplicationWorld). Owns the
// QWebChannel connection for a tab and relays calls from extension
// content-script worlds, and applies content-blocker cosmetic filters.
(function (tokens) {
  'use strict';
  if (window !== top || globalThis.__kiteBridge) return;
  Object.defineProperty(globalThis, '__kiteBridge', { value: true });
  const ctxToken = new Map();
  const queue = [];
  let kite = null;
  const forward = (args) => (kite ? kite.call(...args) : queue.push(args));
  for (const token of tokens) {
    document.addEventListener('kite-up-' + token, (e) => {
      let m; try { m = JSON.parse(e.detail); } catch (x) { return; }
      ctxToken.set(m.ctx, token);
      forward([token, m.ctx, m.method, JSON.stringify(m.args), m.id]);
    });
  }
  const down = (token, json) => document.dispatchEvent(new CustomEvent('kite-down-' + token, { detail: json }));
  function onDeliver(json) {
    let d; try { d = JSON.parse(json); } catch (x) { return; }
    if (d.kind === 'reply') { const t = ctxToken.get(d.ctx); if (t) down(t, json); }
    else if (d.kind === 'deliver') {
      const seen = new Set();
      for (const c of d.ctxs) { const t = ctxToken.get(c); if (t && !seen.has(t)) { seen.add(t); down(t, json); } }
    }
  }
  function applyCosmetic(res) {
    if (!res) return;
    const r = typeof res === 'string' ? JSON.parse(res) : res;
    if (r.css) {
      const put = () => { const s = document.createElement('style'); s.textContent = r.css; s.setAttribute('data-kite-adblock', ''); (document.head || document.documentElement).appendChild(s); };
      document.documentElement ? put() : document.addEventListener('readystatechange', put, { once: true });
    }
    if (r.script) {
      // Scriptlets must run in the page's world.
      const s = document.createElement('script');
      s.textContent = '(function(){try{' + r.script + '}catch(e){}})();';
      (document.head || document.documentElement).appendChild(s);
      s.remove();
    }
  }
  // Screen Wake Lock: relay the page's requests (crates/kite/src/js/wakelock.js)
  // to the browser and its answers back.
  const wakeQueue = [];
  const toKite = (fn) => (kite ? fn(kite) : wakeQueue.push(fn));
  document.addEventListener('kite-wakelock-request', (e) => { const id = +e.detail; if (id > 0) toKite((k) => k.requestWakeLock(id)); });
  document.addEventListener('kite-wakelock-release', () => toKite((k) => k.releaseWakeLock()));
  function init() {
    new QWebChannel(qt.webChannelTransport, (ch) => {
      kite = ch.objects.kite;
      kite.deliver.connect(onDeliver);
      if (kite.wakeLockAnswer) kite.wakeLockAnswer.connect((id, granted) => document.dispatchEvent(new CustomEvent('kite-wakelock-answer', { detail: JSON.stringify({ id, granted }) })));
      for (const fn of wakeQueue.splice(0)) fn(kite);
      for (const a of queue.splice(0)) kite.call(...a);
      kite.cosmetic(location.href, applyCosmetic);
      if (globalThis.__kiteFormsReady) globalThis.__kiteFormsReady(kite);
      if (globalThis.__kiteFillReady) globalThis.__kiteFillReady(kite);
      if (globalThis.__kitePipReady) globalThis.__kitePipReady(kite);
    });
  }
  (function wait(n) {
    if (globalThis.qt && qt.webChannelTransport) init();
    else if (n < 400) setTimeout(() => wait(n + 1), 5);
  })(0);
})(__KITE_TOKENS__);

// __KITE_FORMS__

// ---------------------------------------------------------------------------
// Picture-in-picture (Kite's own: QtWebEngine has none). The browser moves the
// page into a small window; here the chosen video is put in the top layer (as
// a popover, no user gesture needed) filling the viewport, with its native
// controls. The browser calls __kitePip.enter()/exit() in this world; the
// page is told about videos and about the video going away.
(function () {
  'use strict';
  if (window !== top || globalThis.__kitePip) return;
  let kite = null, current = null, saved = null, style = null, announced = false;
  const queue = [];
  const withKite = (fn) => (kite ? fn(kite) : queue.push(fn));
  globalThis.__kitePipReady = (k) => { kite = k; for (const fn of queue.splice(0)) fn(k); };

  const big = (v) => { const r = v.getBoundingClientRect(); return r.width >= 160 && r.height >= 90 && getComputedStyle(v).visibility !== 'hidden'; };
  // A video worth offering: playing (or able to), and not tiny.
  function announce() {
    if (announced) return;
    if (Array.from(document.querySelectorAll('video')).some((v) => big(v) && (v.readyState >= 2 || !v.paused))) {
      announced = true;
      withKite((k) => k.videoFound());
    }
  }
  document.addEventListener('play', announce, true);
  document.addEventListener('loadeddata', announce, true);
  const start = () => { announce(); setTimeout(announce, 2000); };
  document.readyState === 'loading' ? document.addEventListener('DOMContentLoaded', start, { once: true }) : start();

  // The video at (x, y) (CSS px), else the biggest one, playing ones first.
  function pick(x, y) {
    if (x >= 0 && y >= 0) {
      const at = document.elementsFromPoint(x, y).find((e) => e.tagName === 'VIDEO');
      if (at) return at;
    }
    const all = Array.from(document.querySelectorAll('video')).filter(big);
    const area = (v) => { const r = v.getBoundingClientRect(); return r.width * r.height; };
    all.sort((a, b) => (a.paused - b.paused) || area(b) - area(a));
    return all[0] || null;
  }

  const CSS = '[data-kite-pip]:popover-open{position:fixed!important;inset:0!important;width:100vw!important;height:100vh!important;'
    + 'max-width:none!important;max-height:none!important;min-width:0!important;min-height:0!important;margin:0!important;padding:0!important;'
    + 'border:0!important;background:#000!important;object-fit:contain!important;transform:none!important;opacity:1!important;visibility:visible!important}'
    + '[data-kite-pip]::backdrop{background:#000}html[data-kite-pip-on]{overflow:hidden!important}';

  let watcher = null;
  function enter(x, y) {
    if (current) exit();
    const v = pick(x, y);
    if (!v) return { ok: false };
    saved = { popover: v.getAttribute('popover'), controls: v.controls };
    style = document.createElement('style');
    style.textContent = CSS;
    (document.head || document.documentElement).appendChild(style);
    document.documentElement.setAttribute('data-kite-pip-on', '');
    v.setAttribute('data-kite-pip', '');
    v.setAttribute('popover', 'manual');
    v.controls = true;
    try { v.showPopover(); } catch (e) { exit(); return { ok: false }; }
    current = v;
    // The video left the page (another video, a new page in a web app).
    watcher = setInterval(() => { if (current && !current.isConnected) { exit(); withKite((k) => k.pipEnded()); } }, 1000);
    return { ok: true, width: v.videoWidth || 16, height: v.videoHeight || 9, paused: v.paused };
  }
  function exit(pause) {
    clearInterval(watcher);
    const v = current;
    current = null;
    if (v) {
      try { v.hidePopover(); } catch (e) {}
      v.removeAttribute('data-kite-pip');
      if (saved.popover === null) v.removeAttribute('popover'); else v.setAttribute('popover', saved.popover);
      v.controls = saved.controls;
      if (pause) v.pause();
    }
    if (style) style.remove();
    style = null;
    document.documentElement.removeAttribute('data-kite-pip-on');
    return true;
  }
  Object.defineProperty(globalThis, '__kitePip', { value: { enter, exit, active: () => !!current } });
})();
