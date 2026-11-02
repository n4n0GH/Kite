// Transport for content scripts: runs in the extension's own isolated world
// and talks to the Kite bridge world through DOM events whose names contain
// the extension's secret token (unknown to the page and to other extensions).
(function (cfg) {
  'use strict';
  if (globalThis.__kiteAPI) return;
  const UP = 'kite-up-' + cfg.token, DOWN = 'kite-down-' + cfg.token;
  const ctx = 'c-' + Math.random().toString(36).slice(2) + Date.now().toString(36);
  const pending = new Map();
  let nextId = 1;
  const doc = document;
  const send = (method, args, id) => doc.dispatchEvent(new CustomEvent(UP, { detail: JSON.stringify({ ctx, method, args, id }) }));
  const transport = {
    call(method, args) {
      if (window !== top && !cfg.allFramesApi) {
        // Sub-frames have no channel to the browser yet; only synchronous APIs work there.
        return Promise.reject(new Error('This API is not available to content scripts in sub-frames in Kite yet'));
      }
      return new Promise((resolve, reject) => { const id = nextId++; pending.set(id, { resolve, reject }); send(method, args, id); });
    },
  };
  const api = __kiteMakeAPI(cfg, transport);
  doc.addEventListener(DOWN, (e) => {
    let d; try { d = JSON.parse(e.detail); } catch (x) { return; }
    if (d.kind === 'reply' && d.ctx === ctx) {
      const p = pending.get(d.callId); if (!p) return;
      pending.delete(d.callId);
      d.ok ? p.resolve(d.value) : p.reject(new Error(d.error));
    } else if (d.kind === 'deliver' && d.ctxs.includes(ctx)) api.dispatch(d.payload);
  });
  // Remember the element a context menu was opened on (menus.getTargetElement).
  doc.addEventListener('contextmenu', (e) => { globalThis.__kiteLastContextTarget = e.target; }, true);
  Object.defineProperty(globalThis, '__kiteAPI', { value: api });
  Object.defineProperty(globalThis, 'browser', { value: api.browser, writable: true, configurable: true });
  Object.defineProperty(globalThis, 'chrome', { value: api.chrome, writable: true, configurable: true });
  if (window === top) send('_.register', [{ kind: 'content', url: location.href, frameId: 0 }], 0);
  addEventListener('pagehide', () => { if (window === top) send('_.unregister', [], 0); });
})(__KITE_CFG__);
