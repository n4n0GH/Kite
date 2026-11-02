// Transport for extension pages (moz-extension:// documents): talks to the
// Kite bridge object over QWebChannel in the main world.
(function (cfg) {
  'use strict';
  if (globalThis.browser && globalThis.browser.runtime && globalThis.browser.runtime.id === cfg.id) return;
  const ctx = 'p-' + Math.random().toString(36).slice(2) + Date.now().toString(36);
  const pending = new Map();
  let nextId = 1, kite = null, api = null;
  const queue = [];
  const send = (m) => (kite ? kite.call(cfg.token, ctx, m[0], m[1], m[2]) : queue.push(m));
  const transport = {
    call(method, args) {
      return new Promise((resolve, reject) => {
        const id = nextId++;
        pending.set(id, { resolve, reject });
        send([method, JSON.stringify(args), id]);
      });
    },
  };
  api = __kiteMakeAPI(cfg, transport);
  const def = (k, v) => Object.defineProperty(globalThis, k, { value: v, writable: true, configurable: true });
  def('browser', api.browser);
  def('chrome', api.chrome);
  __kiteInstallCorsShim((m, ...a) => transport.call(m, a).then((v) => { if (v && v.__kiteError) throw new Error(v.__kiteError); return v; }));

  function onDeliver(json) {
    let d; try { d = JSON.parse(json); } catch (e) { return; }
    if (d.kind === 'reply' && d.ctx === ctx) {
      const p = pending.get(d.callId); if (!p) return;
      pending.delete(d.callId);
      d.ok ? p.resolve(d.value) : p.reject(new Error(d.error));
    } else if (d.kind === 'deliver' && d.ctxs.includes(ctx)) {
      api.dispatch(d.payload);
    }
  }
  let ready = false, loaded = document.readyState === 'complete';
  const maybeReady = () => { if (ready && loaded && cfg.kind === 'background') transport.call('_.ready', []).catch(() => {}); };
  addEventListener('load', () => { loaded = true; maybeReady(); });
  function init() {
    new QWebChannel(qt.webChannelTransport, (ch) => {
      kite = ch.objects.kite;
      kite.deliver.connect(onDeliver);
      cfg.kind = kite.viewKind || cfg.kind;
      const reg = nextId++;
      pending.set(reg, { resolve: () => {}, reject: () => {} });
      kite.call(cfg.token, ctx, '_.register', JSON.stringify([{ kind: cfg.kind, url: location.href, frameId: window === top ? 0 : 1 }]), reg);
      for (const m of queue.splice(0)) send(m);
      ready = true; maybeReady();
    });
  }
  (function wait(n) {
    if (globalThis.qt && qt.webChannelTransport) init();
    else if (n < 400) setTimeout(() => wait(n + 1), 5);
    else console.error('[Kite] extension API transport unavailable');
  })(0);
  addEventListener('pagehide', () => { if (kite) kite.call(cfg.token, ctx, '_.unregister', '[]', 0); });
  // Extension pages in Kite close themselves via window.close(): map to tabs.remove for tab pages.
})(__KITE_CFG__);
