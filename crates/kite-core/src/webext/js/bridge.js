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

// ---------------------------------------------------------------------------
// Form autofill (saved addresses) and the password generator: classifies the
// focused field, tells the browser where it is (the browser shows the
// suggestions), relays arrow keys while suggestions are shown, fills the
// form with the chosen entry and reports submitted addresses. Runs in the
// bridge world, invisible to the page.
(function () {
  'use strict';
  if (window !== top || globalThis.__kiteFill) return;
  Object.defineProperty(globalThis, '__kiteFill', { value: true });
  let kite = null;
  const queue = [];
  const withKite = (fn) => (kite ? fn(kite) : queue.push(fn));
  let menuOpen = false, current = null, filling = false;
  globalThis.__kiteFillReady = (k) => {
    kite = k;
    for (const fn of queue.splice(0)) fn(k);
    k.fillAddress.connect((json) => fillAddress(JSON.parse(json)));
    k.fillPassword.connect((pw) => fillPassword(pw));
    k.autofillMenu.connect((open) => { menuOpen = open; });
  };

  // HTML autocomplete tokens Kite fills (some mapped onto the ones it stores).
  const TOKENS = {
    'name': 'name', 'given-name': 'given-name', 'family-name': 'family-name', 'organization': 'organization',
    'street-address': 'street-address', 'address-line1': 'address-line1', 'address-line2': 'address-line2', 'address-line3': 'address-line3',
    'address-level2': 'address-level2', 'address-level1': 'address-level1', 'postal-code': 'postal-code',
    'country': 'country', 'country-name': 'country', 'email': 'email', 'tel': 'tel', 'tel-national': 'tel-national',
  };
  const NEVER = /^(cc-|current-password|new-password|one-time-code|username|webauthn|off$|bday|sex|url|photo|language|transaction)/;
  // Guesses from the field's name, id, placeholder and label (first match wins).
  const GUESSES = [
    ['email', /e-?mail/],
    ['tel', /phone|\btel\b|telefon|mobile|handy|cell/],
    ['organization', /company|organi[sz]ation|business|firma|employer/],
    ['postal-code', /zip|postal|post.?code|\bplz\b|postleitzahl/],
    ['address-level2', /city|town|locality|\bort\b|stadt|suburb/],
    ['address-level1', /\bstate\b|province|region|county|bundesland|prefecture/],
    ['country', /country|\bland\b|nation/],
    ['address-line2', /address.?(line)?.?2|addr.?2|apartment|\bapt\b|suite|\bunit\b|adresszusatz/],
    ['address-line1', /address.?(line)?.?1|addr.?1|street|stra(ss|ß)e|address|adresse/],
    ['given-name', /first.?name|given.?name|\bfname\b|vorname|forename/],
    ['family-name', /last.?name|family.?name|surname|\blname\b|nachname/],
    ['name', /full.?name|your.?name|^name$|\bname\b/],
  ];
  const SKIP = /search|suche|captcha|coupon|promo|voucher|gift|user.?name|login|nick|subject|message|comment|card|cvc|cvv|iban|password|query/;
  const textTypes = new Set(['text', 'email', 'tel', '']);

  const visible = (el) => !!(el.offsetWidth || el.offsetHeight || el.getClientRects().length) && getComputedStyle(el).visibility !== 'hidden';
  const isPassword = (el) => el instanceof HTMLInputElement && el.type.toLowerCase() === 'password';
  const scopeOf = (el) => el.form || el.closest('[role=form], form, dialog') || document;

  function describe(el) {
    let label = '';
    try { if (el.labels && el.labels[0]) label = el.labels[0].textContent; } catch (e) {}
    return [el.name, el.id, el.getAttribute('placeholder'), el.getAttribute('aria-label'), label].filter(Boolean).join(' ').toLowerCase();
  }

  // The kind of an address/contact field, with whether the page said so
  // itself (autocomplete) rather than Kite guessing.
  function kindOf(el) {
    if (!el || el.disabled || el.readOnly) return null;
    const tag = el.tagName;
    if (tag === 'INPUT') { if (!textTypes.has(el.type.toLowerCase())) return null; }
    else if (tag !== 'SELECT' && tag !== 'TEXTAREA') return null;
    const ac = (el.getAttribute('autocomplete') || '').toLowerCase().trim().split(/\s+/).filter((t) => !/^(section-|shipping$|billing$|home$|work$|mobile$|fax$|pager$)/.test(t));
    const token = ac[ac.length - 1] || '';
    if (TOKENS[token]) return { kind: TOKENS[token], sure: true };
    if (token && NEVER.test(token)) return null;
    if (tag === 'INPUT' && el.type.toLowerCase() === 'email') return { kind: 'email', sure: true };
    if (tag === 'INPUT' && el.type.toLowerCase() === 'tel') return { kind: 'tel', sure: true };
    const text = describe(el);
    if (!text || SKIP.test(text)) return null;
    for (const [kind, re] of GUESSES) if (re.test(text)) return { kind: tag === 'TEXTAREA' && kind === 'address-line1' ? 'street-address' : kind, sure: false };
    return null;
  }

  const fieldsOf = (scope) => Array.from(scope.querySelectorAll('input, select, textarea')).map((el) => ({ el, k: kindOf(el) })).filter((f) => f.k && visible(f.el));

  // Is this field part of an address form? A guessed field alone is not.
  function addressField(el) {
    const k = kindOf(el);
    if (!k) return null;
    if (k.sure) return k.kind;
    return fieldsOf(scopeOf(el)).length >= 2 ? k.kind : null;
  }

  // A password field for a new password (sign-up or change forms).
  function newPassword(el) {
    if (!isPassword(el) || el.disabled || el.readOnly) return false;
    const ac = (el.getAttribute('autocomplete') || '').toLowerCase();
    if (ac.includes('new-password')) return true;
    if (ac.includes('current-password')) return false;
    const pws = Array.from(scopeOf(el).querySelectorAll('input[type=password]')).filter(visible);
    if (pws.length >= 2) return pws.indexOf(el) > 0 || pws.length === 2;
    const scope = scopeOf(el);
    const hint = describe(el) + ' ' + (scope.id || '') + ' ' + (scope.getAttribute && (scope.getAttribute('action') || '') || '');
    return /new|confirm|regist|sign.?up|create|repeat|retype|join|enroll/i.test(hint);
  }

  function report(el) {
    const kind = newPassword(el) ? 'new-password' : addressField(el);
    if (!kind) { if (current) blur(); return; }
    current = el;
    const r = el.getBoundingClientRect();
    const d = { kind, value: el.tagName === 'SELECT' ? '' : el.value, x: r.left, y: r.top, w: r.width, h: r.height };
    withKite((k) => k.autofillFocus(JSON.stringify(d)));
  }
  function blur() { current = null; menuOpen = false; withKite((k) => k.autofillBlur()); }

  document.addEventListener('focusin', (e) => { if (!filling) report(e.target); }, true);
  document.addEventListener('focusout', (e) => { if (!filling && e.target === current) blur(); }, true);
  document.addEventListener('input', (e) => { if (!filling && e.target === current) report(current); }, true);
  // Clicking a field that already has focus brings the suggestions back.
  document.addEventListener('mousedown', (e) => { if (e.target === current && document.activeElement === current && !menuOpen) setTimeout(() => current && report(current), 0); }, true);
  let moved = 0;
  const follow = () => { if (current && !moved) moved = requestAnimationFrame(() => { moved = 0; if (current) report(current); }); };
  window.addEventListener('scroll', follow, true);
  window.addEventListener('resize', follow, true);
  document.addEventListener('keydown', (e) => {
    if (!menuOpen || e.target !== current) return;
    if (['ArrowDown', 'ArrowUp', 'Enter', 'Escape', 'PageDown', 'PageUp'].includes(e.key)) {
      e.preventDefault();
      e.stopImmediatePropagation();
      withKite((k) => k.autofillKey(e.key));
    } else if (e.key === 'Tab') {
      withKite((k) => k.autofillKey('Escape'));
    }
  }, true);

  // Setting values the way a user would, so frameworks (React, Vue) notice.
  function setValue(el, value) {
    const proto = Object.getPrototypeOf(el);
    const desc = Object.getOwnPropertyDescriptor(proto, 'value');
    if (desc && desc.set) desc.set.call(el, value); else el.value = value;
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }

  function choose(select, value) {
    const want = value.trim().toLowerCase();
    const opts = Array.from(select.options);
    const hit = opts.find((o) => o.value.toLowerCase() === want || o.text.trim().toLowerCase() === want)
      || opts.find((o) => want.length > 2 && o.text.trim().toLowerCase().startsWith(want))
      || opts.find((o) => o.text.trim().length > 2 && want.startsWith(o.text.trim().toLowerCase()));
    if (hit && select.value !== hit.value) setValue(select, hit.value);
  }

  function fillAddress(values) {
    const start = current;
    if (!values || !start) return;
    const fields = fieldsOf(scopeOf(start));
    const kinds = new Set(fields.map((f) => f.k.kind));
    filling = true;
    try {
      for (const { el, k } of fields) {
        let v = values[k.kind];
        // Forms without a second address line get the whole street on line 1.
        if (k.kind === 'address-line1' && !kinds.has('address-line2') && values['street-address']) v = values['street-address'].replace(/\n/g, ', ');
        if (k.kind === 'street-address' && el.tagName !== 'TEXTAREA' && v) v = v.replace(/\n/g, ', ');
        // Phone fields that start with a country code picker.
        if (k.kind === 'tel' && el.maxLength > 0 && v && v.length > el.maxLength && values['tel-national']) v = values['tel-national'];
        if (!v) continue;
        if (el !== start && el.value && el.tagName !== 'SELECT') continue;
        if (el.tagName === 'SELECT') choose(el, v); else setValue(el, v);
      }
    } finally {
      filling = false;
    }
    start.focus();
    menuOpen = false;
  }

  function fillPassword(pw) {
    const start = current;
    if (!pw || !isPassword(start)) return;
    const pws = Array.from(scopeOf(start).querySelectorAll('input[type=password]')).filter((p) => visible(p) && !p.disabled && !p.readOnly);
    const marked = pws.filter((p) => (p.getAttribute('autocomplete') || '').includes('new-password'));
    const targets = marked.length ? marked : pws.length >= 3 ? pws.slice(1) : pws;
    filling = true;
    try { for (const p of targets.includes(start) ? targets : [start]) setValue(p, pw); } finally { filling = false; }
    start.focus();
    menuOpen = false;
  }

  // Report addresses typed into submitted forms (the browser offers to save).
  let lastSent = '';
  function submitted(scope) {
    const values = {};
    for (const { el, k } of fieldsOf(scope)) {
      let v = el.tagName === 'SELECT' ? (el.selectedOptions[0] ? el.selectedOptions[0].text : '') : el.value;
      v = (v || '').trim();
      if (v && !values[k.kind]) values[k.kind] = v;
    }
    if (Object.keys(values).length < 3) return;
    const key = JSON.stringify(values);
    if (key === lastSent) return;
    lastSent = key;
    withKite((k) => k.autofillSubmit(key));
  }
  document.addEventListener('submit', (e) => submitted(e.target), true);
  // Typed, unsent text: the browser keeps such tabs loaded.
  let dirty = false;
  const setDirty = (d) => { if (d !== dirty) { dirty = d; withKite((k) => k.formDirty(d)); } };
  document.addEventListener('input', (e) => {
    const t = e.target;
    if (!e.isTrusted || !t) return;
    const editable = t.isContentEditable || t.tagName === 'TEXTAREA' || (t.tagName === 'INPUT' && !/^(checkbox|radio|range|color|file|button|submit|search)$/i.test(t.type));
    if (editable) setDirty(Array.from(document.querySelectorAll('input, textarea')).some((f) => f.value && f.value !== f.defaultValue && f.type !== 'search' && f.type !== 'hidden') || t.isContentEditable);
  }, true);
  document.addEventListener('submit', () => setDirty(false), true);
  document.addEventListener('click', (e) => {
    const b = e.target && e.target.closest && e.target.closest('button, input[type=submit], [role=button]');
    if (b && (b.type === 'submit' || /continue|next|save|submit|order|checkout|weiter|bestellen|speichern|register|sign.?up|pay/i.test(b.textContent + ' ' + (b.value || '')))) submitted(scopeOf(b));
  }, true);
})();
