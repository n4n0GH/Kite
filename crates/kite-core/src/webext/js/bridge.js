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
  function init() {
    new QWebChannel(qt.webChannelTransport, (ch) => {
      kite = ch.objects.kite;
      kite.deliver.connect(onDeliver);
      for (const a of queue.splice(0)) kite.call(...a);
      kite.cosmetic(location.href, applyCosmetic);
    });
  }
  (function wait(n) {
    if (globalThis.qt && qt.webChannelTransport) init();
    else if (n < 400) setTimeout(() => wait(n + 1), 5);
  })(0);
})(__KITE_TOKENS__);
