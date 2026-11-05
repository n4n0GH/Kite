// Picture-in-picture (Kite's own: QtWebEngine has none). The browser moves the
// page into a small window; here the chosen video is put in the top layer (as
// a popover, no user gesture needed) filling the viewport, with its native
// controls. The browser calls __kitePip.enter()/exit() in this world; the
// page is told about videos and about the video going away.
(function () {
  'use strict';
  if (window !== top || globalThis.__kitePip) return;
  let current = null, saved = null, style = null, announced = false;

  const big = (v) => { const r = v.getBoundingClientRect(); return r.width >= 160 && r.height >= 90 && getComputedStyle(v).visibility !== 'hidden'; };
  // A video worth offering: playing (or able to), and not tiny.
  function announce() {
    if (announced) return;
    if (Array.from(document.querySelectorAll('video')).some((v) => big(v) && (v.readyState >= 2 || !v.paused))) {
      announced = true;
      kitePluginSend({ cmd: 'videoFound' });
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
    watcher = setInterval(() => { if (current && !current.isConnected) { exit(); kitePluginSend({ cmd: 'pipEnded' }); } }, 1000);
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
