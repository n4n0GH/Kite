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
    });
  }
  (function wait(n) {
    if (globalThis.qt && qt.webChannelTransport) init();
    else if (n < 400) setTimeout(() => wait(n + 1), 5);
  })(0);
})(__KITE_TOKENS__);

// ---------------------------------------------------------------------------
// Password manager: login form detection and autofill (Falkon's AutoFill
// form observer). Runs in the bridge world, invisible to the page.
(function () {
  'use strict';
  if (window !== top || globalThis.__kiteForms) return;
  Object.defineProperty(globalThis, '__kiteForms', { value: true });
  let kite = null;
  const queue = [];
  const withKite = (fn) => (kite ? fn(kite) : queue.push(fn));
  globalThis.__kiteFormsReady = (k) => { kite = k; for (const fn of queue.splice(0)) fn(k); k.fillLogin.connect((json) => fill(JSON.parse(json))); };

  const textTypes = new Set(['text', 'email', 'tel', '']);
  const visible = (el) => !!(el.offsetWidth || el.offsetHeight || el.getClientRects().length) && getComputedStyle(el).visibility !== 'hidden';
  const inputsOf = (root) => Array.from(root.querySelectorAll('input')).filter((i) => !i.disabled && (textTypes.has(i.type.toLowerCase()) || i.type.toLowerCase() === 'password'));

  function findUsername(inputs, pw) {
    const before = inputs.slice(0, inputs.indexOf(pw)).filter((i) => i.type.toLowerCase() !== 'password');
    const byAc = inputs.find((i) => /username|email/.test(i.autocomplete || ''));
    if (byAc) return byAc;
    const named = before.reverse().find((i) => /user|name|login|mail|account|id/i.test(i.name + ' ' + i.id) && visible(i));
    return named || before.find((i) => visible(i)) || before[0] || null;
  }

  function scopeOf(pw) { return pw.form || pw.closest('[role=form], form, dialog') || document; }

  function collect(pw) {
    const inputs = inputsOf(scopeOf(pw));
    const user = findUsername(inputs, pw);
    // New-password forms with a confirmation field: take the first new password.
    let data = '';
    for (const i of inputs) if (i.name) data += encodeURIComponent(i.name) + '=' + encodeURIComponent(i.value) + '&';
    return { username: user ? user.value : '', password: pw.value, data };
  }

  let lastSent = '';
  function submitted(scope) {
    const pw = inputsOf(scope).find((i) => i.type.toLowerCase() === 'password' && i.value);
    if (!pw) return;
    const d = collect(pw);
    const key = d.username + '\u0000' + d.password;
    if (key === lastSent) return;
    lastSent = key;
    withKite((k) => k.formSubmitted(JSON.stringify(d)));
  }
  document.addEventListener('submit', (e) => submitted(e.target), true);
  // Script-driven logins: submit-like button clicks and Enter in a password field.
  document.addEventListener('click', (e) => {
    const b = e.target && e.target.closest && e.target.closest('button, input[type=submit], input[type=button], [role=button]');
    if (b) submitted(scopeOf(b));
  }, true);
  document.addEventListener('keydown', (e) => { if (e.key === 'Enter' && e.target && e.target.type === 'password') submitted(scopeOf(e.target)); }, true);

  // Setting values the way a user would, so frameworks (React, Vue) notice.
  function setValue(input, value) {
    const proto = Object.getPrototypeOf(input);
    const setter = Object.getOwnPropertyDescriptor(proto, 'value') || Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
    input.focus();
    setter.set.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
    input.blur();
  }

  function fill(cred) {
    if (!cred) return;
    const pws = inputsOf(document).filter((i) => i.type.toLowerCase() === 'password' && visible(i));
    const pw = pws[0] || inputsOf(document).find((i) => i.type.toLowerCase() === 'password');
    // First: restore named fields from the saved form data (Falkon behaviour).
    const byName = new Map();
    for (const pair of (cred.data || '').split('&')) {
      const [k, v] = pair.split('=');
      if (k && v !== undefined) try { byName.set(decodeURIComponent(k), decodeURIComponent(v)); } catch (e) {}
    }
    let filled = false;
    for (const i of inputsOf(document)) {
      if (i.name && byName.has(i.name) && i.type.toLowerCase() !== 'password') { setValue(i, byName.get(i.name)); filled = true; }
    }
    if (pw) {
      const user = findUsername(inputsOf(scopeOf(pw)), pw);
      if (user && cred.username && (!filled || !user.value)) setValue(user, cred.username);
      for (const p of pws.length ? pws.slice(0, 1) : [pw]) setValue(p, cred.password);
    } else {
      // Two-step logins (username first, password on the next screen).
      const u = inputsOf(document).find((i) => /username|email/.test(i.autocomplete || '') || i.type === 'email');
      if (u && cred.username) setValue(u, cred.username);
    }
  }

  // Tell the browser when a login form is present (to autofill and show the key icon).
  let announced = false, checks = 0;
  function scan() {
    if (announced) return;
    const pw = inputsOf(document).find((i) => i.type.toLowerCase() === 'password' && visible(i) && i.autocomplete !== 'new-password');
    const two = !pw && inputsOf(document).find((i) => /username|webauthn/.test(i.autocomplete || '') && visible(i));
    if (!pw && !two) return;
    announced = true;
    withKite((k) => k.loginFormFound(!!pw && !pw.value));
  }
  const observer = new MutationObserver(() => { if (++checks > 400) observer.disconnect(); else { clearTimeout(scan._t); scan._t = setTimeout(scan, 150); } });
  const start = () => { scan(); observer.observe(document.documentElement || document, { childList: true, subtree: true }); setTimeout(() => observer.disconnect(), 30000); };
  document.readyState === 'loading' ? document.addEventListener('DOMContentLoaded', start, { once: true }) : start();
})();
