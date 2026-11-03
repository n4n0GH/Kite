// Transport for content scripts: runs in the extension's own isolated world
// and talks to the Kite bridge world through DOM events whose names contain
// the extension's secret token (unknown to the page and to other extensions).
//
// The bridge only exists in the top frame. In sub-frames, content scripts
// call the runtime through the extension's own origin instead (POST
// moz-extension://<uuid>/__kite__/rpc and /poll, see crates/kite/src/rpc.rs),
// authenticated by the token in the request body.
(function (cfg) {
  'use strict';
  if (globalThis.__kiteAPI) return;
  const UP = 'kite-up-' + cfg.token, DOWN = 'kite-down-' + cfg.token;
  const ctx = 'c-' + Math.random().toString(36).slice(2) + Date.now().toString(36);
  const pending = new Map();
  let nextId = 1;
  const doc = document;
  const inFrame = window !== top;
  // Frame ids aren't known to Kite; a random one is unique enough per tab.
  const frameId = inFrame ? 1 + Math.floor(Math.random() * 0x3fffffff) : 0;
  const rpcBase = 'moz-extension://' + cfg.uuid + '/__kite__/';
  const rawFetch = globalThis.fetch.bind(globalThis);
  // The top frame's origin tells Kite which tab this frame is in.
  const topOrigin = inFrame && location.ancestorOrigins && location.ancestorOrigins.length ? location.ancestorOrigins[location.ancestorOrigins.length - 1] : '';
  const post = (path, body, keepalive) => rawFetch(rpcBase + path, { method: 'POST', cache: 'no-store', credentials: 'omit', keepalive: !!keepalive, body: JSON.stringify(Object.assign({ token: cfg.token, ctx }, body)) });
  const send = inFrame
    ? (method, args, id) => post('rpc', { method, args, callId: id, top: topOrigin }).catch((e) => {
      const p = pending.get(id); if (p) { pending.delete(id); p.reject(new Error('Extension API unavailable: ' + e.message)); }
    })
    : (method, args, id) => doc.dispatchEvent(new CustomEvent(UP, { detail: JSON.stringify({ ctx, method, args, id }) }));
  const transport = {
    call(method, args) {
      return new Promise((resolve, reject) => { const id = nextId++; pending.set(id, { resolve, reject }); send(method, args, id); });
    },
  };
  const api = __kiteMakeAPI(cfg, transport);
  function onDown(d) {
    if (d.kind === 'reply' && d.ctx === ctx) {
      const p = pending.get(d.callId); if (!p) return;
      pending.delete(d.callId);
      d.ok ? p.resolve(d.value) : p.reject(new Error(d.error));
    } else if (d.kind === 'deliver' && d.ctxs.includes(ctx)) api.dispatch(d.payload);
  }
  doc.addEventListener(DOWN, (e) => {
    let d; try { d = JSON.parse(e.detail); } catch (x) { return; }
    onDown(d);
  });
  let polling = false;
  async function pollLoop() {
    polling = true;
    while (polling) {
      try {
        const r = await post('poll', {});
        if (r.status === 403 || r.status === 404) return;
        for (const d of await r.json()) onDown(d);
      } catch (e) {
        await new Promise((res) => setTimeout(res, 1000));
      }
    }
  }
  // Remember the element a context menu was opened on (menus.getTargetElement).
  doc.addEventListener('contextmenu', (e) => { globalThis.__kiteLastContextTarget = e.target; }, true);
  Object.defineProperty(globalThis, '__kiteAPI', { value: api });
  Object.defineProperty(globalThis, 'browser', { value: api.browser, writable: true, configurable: true });
  Object.defineProperty(globalThis, 'chrome', { value: api.chrome, writable: true, configurable: true });
  if (inFrame) {
    // Start polling before registering so nothing is missed.
    pollLoop();
    send('_.register', [{ kind: 'content', url: location.href, frameId }], 0);
    addEventListener('pagehide', () => { polling = false; post('rpc', { method: '_.unregister', args: [], callId: 0 }, true).catch(() => {}); });
  } else {
    send('_.register', [{ kind: 'content', url: location.href, frameId: 0 }], 0);
    addEventListener('pagehide', () => send('_.unregister', [], 0));
  }
})(__KITE_CFG__);
