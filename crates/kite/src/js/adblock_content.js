// The cosmetic side of Kite's content blocker, for one site, in every frame of it (isolated
// world, from document creation; QML puts `KITE_AB` before this: WebTab.prepareAdblock).
//
//  * hides what the lists name for this site;
//  * hides what generic rules name, by asking which apply to the classes and ids it finds;
//  * runs procedural filters (uBO's :has-text and the like), which CSS can't express.
//
// The rules come from the engine (kite_core::adblock), and scriptlets go separately: they
// have to be in the page's own world.
(function () {
  'use strict';
  const { host, plan, secret } = KITE_AB;
  if (location.hostname !== host || globalThis.__kiteAbFill) return;

  const SPLIT = /[()[\]"'\\]/;

  // ---- procedural selectors: "base:op(arg) more:op(arg)" ----
  const OPERATORS = new Set(['has-text', 'contains', '-abp-contains', 'upward', 'nth-ancestor', 'matches-css', 'matches-css-before',
    'matches-css-after', 'matches-attr', 'matches-path', 'matches-media', 'min-text-length', 'others', 'xpath', 'watch-attr',
    'has', '-abp-has', 'if', 'if-not', 'not', 'remove', 'remove-attr', 'remove-class', 'style']);
  const ACTIONS = new Set(['remove', 'remove-attr', 'remove-class', 'style']);
  // (:has and :not are CSS already, unless what's inside them isn't.)
  const HAS_PROCEDURAL = /:(?:-abp-|has-text|contains|upward|nth-ancestor|matches-|min-text-length|others|xpath|if\b|if-not)/;

  // The end of the parenthesis opening at `open`, minding quotes and escapes.
  function closing(text, open) {
    let depth = 0, quote = '';
    for (let i = open; i < text.length; i++) {
      const c = text[i];
      if (c === '\\') { i++; continue; }
      if (quote) { if (c === quote) quote = ''; continue; }
      if (c === '"' || c === "'") { quote = c; continue; }
      if (c === '(') depth++;
      else if (c === ')' && --depth === 0) return i;
    }
    return -1;
  }
  function unquote(arg) {
    const t = arg.trim();
    return t.length > 1 && (t[0] === '"' || t[0] === "'") && t[t.length - 1] === t[0] ? t.slice(1, -1) : t;
  }

  // {base, tasks: [{op, arg}], action}, or null if it can't be understood.
  function parseProcedural(selector) {
    const tasks = [];
    let action = null, plain = '', i = 0, bracket = 0, quote = '';
    const flush = () => {
      if (plain.trim() !== '') tasks.push({ op: 'css', arg: plain });
      plain = '';
    };
    while (i < selector.length) {
      const c = selector[i];
      if (c === '\\') { plain += c + (selector[i + 1] ?? ''); i += 2; continue; }
      if (quote) { if (c === quote) quote = ''; plain += c; i++; continue; }
      if (c === '"' || c === "'") { quote = c; plain += c; i++; continue; }
      if (c === '[') bracket++;
      if (c === ']') bracket--;
      if (c === ':' && bracket === 0) {
        const name = /^:(-?[a-z][a-z-]*)\(/.exec(selector.slice(i));
        if (name && OPERATORS.has(name[1])) {
          const open = i + name[0].length - 1;
          const end = closing(selector, open);
          if (end < 0) return null;
          const arg = selector.slice(open + 1, end);
          const op = name[1];
          // Plain CSS stays CSS, in the text around it.
          if ((op === 'has' || op === 'not') && !HAS_PROCEDURAL.test(arg)) {
            plain += selector.slice(i, end + 1);
          } else {
            flush();
            if (ACTIONS.has(op)) action = { type: op, arg: unquote(arg) };
            else tasks.push({ op, arg });
          }
          i = end + 1;
          continue;
        }
      }
      plain += c;
      i++;
    }
    flush();
    const first = tasks[0] && tasks[0].op === 'css' ? tasks.shift().arg.trim() : '';
    return { base: first, tasks, action };
  }

  // From the engine's own form: {selector: [{type, arg}], action: {type, arg}}.
  function fromEngine(json) {
    let parsed;
    try { parsed = JSON.parse(json); } catch (e) { return null; }
    if (!Array.isArray(parsed.selector)) return null;
    const steps = parsed.selector.map((s) => ({ op: s.type === 'css-selector' ? 'css' : s.type, arg: s.arg }));
    const base = steps[0] && steps[0].op === 'css' ? steps.shift().arg : '';
    return { base, tasks: steps, action: parsed.action || null };
  }

  // A text to look for: "/regex/flags", or the words themselves.
  function matcher(pattern) {
    const text = unquote(pattern);
    const m = /^\/(.+)\/([dgimsuvy]*)$/.exec(text);
    if (m) {
      try { const re = new RegExp(m[1], m[2].replace(/[gy]/g, '')); return (s) => re.test(s); } catch (e) { return () => false; }
    }
    return (s) => s.includes(text);
  }
  function globOrRegex(pattern) {
    const text = unquote(pattern);
    const m = /^\/(.+)\/([dgimsuvy]*)$/.exec(text);
    if (m) {
      try { const re = new RegExp(m[1], m[2].replace(/[gy]/g, '')); return (s) => re.test(s); } catch (e) { return () => false; }
    }
    const re = new RegExp('^' + text.replace(/[.+?^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*') + '$');
    return (s) => re.test(s);
  }

  // What each operator makes of the elements so far.
  const run = {
    css(els, arg) {
      const part = arg.trim();
      const relative = /^[>+~]/.test(part);
      const descendant = /^\s/.test(arg);
      const out = new Set();
      for (const el of els) {
        if (relative) { for (const e of el.querySelectorAll(':scope ' + part)) out.add(e); }
        else if (descendant) { for (const e of el.querySelectorAll(part)) out.add(e); }
        else if (el.matches(part)) out.add(el);
      }
      return [...out];
    },
    'has-text': (els, arg) => { const m = matcher(arg); return els.filter((el) => m(el.textContent)); },
    'min-text-length': (els, arg) => { const n = parseInt(arg, 10); return els.filter((el) => el.textContent.length >= n); },
    upward(els, arg) {
      const t = arg.trim();
      const out = new Set();
      if (/^\d+$/.test(t)) {
        for (const el of els) {
          let up = el;
          for (let n = parseInt(t, 10); up && n > 0; n--) up = up.parentElement;
          if (up) out.add(up);
        }
      } else {
        for (const el of els) { const up = el.parentElement && el.parentElement.closest(t); if (up) out.add(up); }
      }
      return [...out];
    },
    'matches-css': (els, arg) => cssFilter(els, arg, null),
    'matches-css-before': (els, arg) => cssFilter(els, arg, '::before'),
    'matches-css-after': (els, arg) => cssFilter(els, arg, '::after'),
    'matches-attr'(els, arg) {
      const m = /^\s*("?[^"=]+"?|\/.+?\/)\s*(?:=\s*(.+))?$/.exec(arg);
      if (!m) return [];
      const name = globOrRegex(m[1]);
      const value = m[2] === undefined ? () => true : globOrRegex(m[2]);
      return els.filter((el) => { for (const a of el.attributes) if (name(a.name) && value(a.value)) return true; return false; });
    },
    'matches-path'(els, arg) { return matcher(arg)(location.pathname + location.search) ? els : []; },
    'matches-media'(els, arg) { return matchMedia(unquote(arg)).matches ? els : []; },
    others(els) {
      const keep = new Set();
      for (const el of els) { for (let e = el; e; e = e.parentElement) keep.add(e); }
      const out = [];
      for (const el of els) {
        for (let e = el; e && e.parentElement; e = e.parentElement) {
          for (const sibling of e.parentElement.children) if (!keep.has(sibling)) out.push(sibling);
        }
      }
      return [...new Set(out)];
    },
    xpath(els, arg) {
      const out = new Set();
      for (const el of els) {
        const r = document.evaluate(unquote(arg), el, null, XPathResult.UNORDERED_NODE_SNAPSHOT_TYPE, null);
        for (let i = 0; i < r.snapshotLength; i++) { const n = r.snapshotItem(i); if (n.nodeType === 1) out.add(n); }
      }
      return [...out];
    },
    'watch-attr': (els) => els,
    has: (els, arg) => relation(els, arg, true),
    '-abp-has': (els, arg) => relation(els, arg, true),
    if: (els, arg) => relation(els, arg, true),
    not: (els, arg) => relation(els, arg, false),
    'if-not': (els, arg) => relation(els, arg, false),
  };
  run.contains = run['has-text'];
  run['-abp-contains'] = run['has-text'];
  run['nth-ancestor'] = run.upward;

  function cssFilter(els, arg, pseudo) {
    const at = arg.indexOf(':');
    if (at < 0) return [];
    const name = arg.slice(0, at).trim();
    const value = globOrRegex(arg.slice(at + 1));
    return els.filter((el) => value(getComputedStyle(el, pseudo).getPropertyValue(name)));
  }
  function relation(els, arg, wanted) {
    const inner = parseProcedural(arg.trim());
    if (!inner) return [];
    return els.filter((el) => {
      const found = select(inner, el);
      return (found.length > 0) === wanted;
    });
  }
  // What a parsed selector finds, under `root` if it's given.
  function select(parsed, root) {
    let els;
    const within = root || document;
    if (parsed.base) {
      const part = parsed.base;
      try {
        els = [...within.querySelectorAll(root && /^[>+~]/.test(part) ? ':scope ' + part : part)];
      } catch (e) { return []; }
    } else {
      els = root ? [root] : [...document.querySelectorAll('*')];
    }
    for (const { op, arg } of parsed.tasks) {
      if (els.length === 0) break;
      const step = run[op];
      if (!step) return [];
      try { els = step(els, arg); } catch (e) { return []; }
    }
    return els;
  }

  // ---- holding hidden elements hidden ----
  // The sheet below is of the page's own kind (QtWebEngine has no user stylesheets, which
  // uBO gets from Chromium's extension API), so a page wins against it with
  // `display: … !important` set on the element itself, or with an !important rule of its own
  // that's more specific or comes later. (kaa.lt's "adblock trap", a see-through link over
  // the whole page, does the first.) An element a rule names that is still shown gets
  // `display: none !important` on itself, and again whenever the page takes that back.
  // Elements and why they're held: 'css' (by the sheet's selectors, so only while one still
  // names them) or 'procedural' (for good).
  const held = new WeakMap();
  const inlineHidden = (el) => el.style.getPropertyValue('display') === 'none' && el.style.getPropertyPriority('display') === 'important';
  // How often the page took it back: one that keeps at it would have this script and the
  // page answering each other for good, so after a while the element goes.
  const takenBack = new WeakMap();
  // Whether anything was ever held: until then, and with no selectors, there's nothing to settle.
  let anyHeld = false;
  function hold(el, why) {
    anyHeld = true;
    if (!held.has(el)) held.set(el, why);
    if (inlineHidden(el)) return;
    const n = (takenBack.get(el) || 0) + 1;
    takenBack.set(el, n);
    if (n > 20) { el.remove(); return; }
    el.style.setProperty('display', 'none', 'important');
  }
  function release(el) {
    held.delete(el);
    if (inlineHidden(el)) el.style.removeProperty('display');
  }
  // Every selector of the sheet as one, for matching (each was checked to be readable).
  let hideSelector = '';
  const named = (el) => {
    if (!hideSelector) return false;
    try { return el.matches(hideSelector); } catch (e) { return false; }
  };
  // Showing itself the way the sheet can't overrule: by its own style attribute. (Asked for
  // first: `el.style` makes an object for every element it's read of.)
  const insists = (el) => el.hasAttribute('style') && el.style && el.style.getPropertyPriority('display') === 'important' && el.style.getPropertyValue('display') !== 'none';
  // An element added or restyled: settled at once (observers run before the page is drawn
  // again or takes a click), so what a rule names never shows.
  function recheck(el) {
    if (!anyHeld && !hideSelector) return;
    const why = held.get(el);
    if (why === 'procedural') hold(el, why);
    else if (why === 'css') { if (named(el)) hold(el, why); else release(el); }
    else if (insists(el) && named(el)) hold(el, 'css');
  }
  // The rest (a page's own !important rules) shows only in what an element comes to: asked
  // of those a rule names, new ones as they're looked at and all of them now and then.
  function holdShown(els) {
    for (let i = 0; i < els.length; i++) {
      const el = els[i];
      if (!held.has(el) && getComputedStyle(el).display !== 'none') hold(el, 'css');
    }
  }
  function holdAll() {
    if (!hideSelector) return;
    let els;
    try { els = document.querySelectorAll(hideSelector); } catch (e) { return; }
    holdShown(els);
  }

  const styled = new WeakSet();
  function apply(parsed, el) {
    const action = parsed.action;
    if (!action) { hold(el, 'procedural'); return; }
    switch (action.type) {
      case 'remove': el.remove(); break;
      case 'style': if (!styled.has(el)) { styled.add(el); el.style.cssText += ';' + (action.arg || ''); } break;
      case 'remove-attr': el.removeAttribute(action.arg); break;
      case 'remove-class': el.classList.remove(action.arg); break;
    }
  }

  // ---- hiding by CSS ----
  // A selector CSS can't read would take the others of a rule with it, so each is checked.
  const hide = [];
  const procedural = [];
  for (const selector of plan.hide) {
    if (CSS.supports('selector(' + selector + ')')) hide.push(selector);
    else { const p = parseProcedural(selector); if (p) procedural.push(p); }
  }
  for (const json of plan.procedural) { const p = fromEngine(json); if (p) procedural.push(p); }

  const sheet = document.createElement('style');
  const rule = (list) => list.join(',\n') + ' { display: none !important; }\n';
  function keepSheet() {
    if (sheet.isConnected || !document.documentElement) return;
    (document.head || document.documentElement).appendChild(sheet);
  }
  sheet.textContent = hide.length ? rule(hide) : '';
  hideSelector = hide.join(',');

  // ---- generic rules, by the classes and ids found ----
  const askedClasses = new Set();
  const askedIds = new Set();
  // Whole class attributes read already: most elements of a page repeat a few of them.
  const readClasses = new Set();
  const frame = Array.from(crypto.getRandomValues(new Uint8Array(8)), (b) => b.toString(16).padStart(2, '0')).join('');
  const newClasses = new Set();
  const newIds = new Set();
  function note(el) {
    const id = el.getAttribute('id');
    if (id && !askedIds.has(id)) { askedIds.add(id); newIds.add(id); }
    const classes = el.getAttribute('class');
    if (!classes || readClasses.has(classes)) return;
    if (readClasses.size >= 4096) readClasses.clear();
    readClasses.add(classes);
    for (const c of classes.split(/[ \t\n\f\r]+/)) {
      if (c && !askedClasses.has(c)) { askedClasses.add(c); newClasses.add(c); }
    }
  }
  function noteAll(els) {
    for (let i = 0; i < els.length; i++) note(els[i]);
  }
  // What the page added or renamed since the last look: read then, not as it happens (a page
  // building itself adds thousands of elements, and takes many away again before anyone
  // could ask about them). Past `LOOK_MOST` of them, the whole document is read instead.
  const LOOK_MOST = 2000;
  let added = [];
  let renamed = [];
  let whole = true;
  function seen(list, el) {
    if (whole) return;
    if (added.length + renamed.length >= LOOK_MOST) { whole = true; added = []; renamed = []; return; }
    list.push(el);
  }
  function look() {
    const noting = !plan.generichide;
    if (whole) {
      whole = false;
      if (noting) noteAll(document.querySelectorAll('[id],[class]'));
      // (Which of them to hold is for `holdWhole`, after every look.)
      return;
    }
    const wasRenamed = renamed, wasAdded = added;
    renamed = [];
    added = [];
    // (What's inside something renamed is for `holdWhole` to find.)
    for (const el of wasRenamed) {
      if (!el.isConnected) continue;
      if (noting) note(el);
      holdNew(el, false);
    }
    for (const el of wasAdded) {
      if (!el.isConnected) continue;
      const inside = !!el.firstElementChild;
      if (noting) { note(el); if (inside) noteAll(el.querySelectorAll('[id],[class]')); }
      holdNew(el, inside);
    }
  }
  // An element new to the page or newly named, and what's inside it: held if a rule names
  // it and it shows all the same.
  function holdNew(el, inside) {
    if (!hideSelector) return;
    try {
      if (el.matches(hideSelector)) holdShown([el]);
      if (inside) holdShown(el.querySelectorAll(hideSelector));
    } catch (e) { /* (selectors that can't be read: `holdAll` leaves them too) */ }
  }
  function ask() {
    if (plan.generichide || (!newClasses.size && !newIds.size)) return;
    const message = { f: frame, c: [...newClasses], i: [...newIds], x: plan.exceptions };
    newClasses.clear();
    newIds.clear();
    console.debug(secret + 'ab:' + JSON.stringify(message));
  }
  globalThis.__kiteAbFill = (to, selectors) => {
    if (to !== frame || !selectors.length) return;
    const readable = selectors.filter((s) => CSS.supports('selector(' + s + ')'));
    if (!readable.length) return;
    sheet.textContent += rule(readable);
    hideSelector += (hideSelector ? ',' : '') + readable.join(',');
    keepSheet();
    // (Only the new ones: the others' elements were looked at as they came.)
    try { holdShown(document.querySelectorAll(readable.join(','))); } catch (e) { /* (each was checked) */ }
  };

  // ---- watching ----
  // Going through the whole document with every selector takes long on a large page (some
  // microseconds an element), so it waits until the page has nothing to do (a second at
  // most), and gets a hundredth of the time. What's new was looked at already; this is for
  // what a change made of the elements around it.
  let wholeWaiting = false;
  let wholeNext = 0;
  function holdWhole() {
    wholeWaiting = false;
    const started = performance.now();
    holdAll();
    const now = performance.now();
    wholeNext = now + Math.max(250, (now - started) * 100);
  }
  function holdWholeSoon() {
    if (!hideSelector || wholeWaiting) return;
    wholeWaiting = true;
    setTimeout(() => requestIdleCallback(holdWhole, { timeout: 1000 }), Math.max(0, wholeNext - performance.now()));
  }

  let timer = 0;
  let lastCost = 0;
  function tick() {
    timer = 0;
    const started = performance.now();
    keepSheet();
    look();
    ask();
    for (const p of procedural) { for (const el of select(p)) apply(p, el); }
    // (A page that changes all the time shouldn't take all the time.)
    lastCost = performance.now() - started;
    holdWholeSoon();
  }
  function soon() {
    if (timer) return;
    timer = setTimeout(tick, Math.max(100, Math.min(2000, lastCost * 20)));
  }
  // The document as it stands: at the start, and once it's all there.
  function loaded() {
    whole = true;
    added = [];
    renamed = [];
    soon();
  }
  function watch() {
    if (!document.documentElement) return false;
    keepSheet();
    new MutationObserver((records) => {
      let changed = false;
      // (Nothing to settle while no rule names anything and nothing is held.)
      const settle = anyHeld || hideSelector !== '';
      for (const r of records) {
        if (r.type === 'attributes') {
          // (Style changes come often, from animations: only held or insisting elements count.)
          if (r.attributeName !== 'style') { changed = true; seen(renamed, r.target); }
          if (settle) recheck(r.target);
          continue;
        }
        changed = true;
        const nodes = r.addedNodes;
        for (let i = 0; i < nodes.length; i++) {
          const n = nodes[i];
          if (n.nodeType !== 1) continue;
          seen(added, n);
          if (!settle) continue;
          recheck(n);
          if (n.firstElementChild) { const inside = n.querySelectorAll('[style]'); for (let j = 0; j < inside.length; j++) recheck(inside[j]); }
        }
      }
      if (changed) soon();
    }).observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ['class', 'id', 'style'] });
    loaded();
    return true;
  }
  if (!watch()) {
    const wait = new MutationObserver(() => { if (watch()) wait.disconnect(); });
    wait.observe(document, { childList: true });
  }
  document.addEventListener('DOMContentLoaded', loaded, { once: true });

  if (KITE_AB.test) KITE_AB.test({ parseProcedural, fromEngine, select, recheck, held });
})();
