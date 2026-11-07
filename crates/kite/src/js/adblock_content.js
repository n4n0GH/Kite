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

  const done = new WeakSet();
  function apply(parsed, el) {
    const action = parsed.action;
    if (!action) {
      if (!done.has(el)) { done.add(el); el.style.setProperty('display', 'none', 'important'); }
      return;
    }
    switch (action.type) {
      case 'remove': el.remove(); break;
      case 'style': if (!done.has(el)) { done.add(el); el.style.cssText += ';' + (action.arg || ''); } break;
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

  // ---- generic rules, by the classes and ids found ----
  const asked = new Set();
  const frame = Array.from(crypto.getRandomValues(new Uint8Array(8)), (b) => b.toString(16).padStart(2, '0')).join('');
  const newClasses = new Set();
  const newIds = new Set();
  function note(el) {
    const id = el.id;
    if (typeof id === 'string' && id && !asked.has('#' + id)) { asked.add('#' + id); newIds.add(id); }
    const list = el.classList;
    if (!list) return;
    for (let i = 0; i < list.length; i++) {
      const c = list[i];
      if (!asked.has('.' + c)) { asked.add('.' + c); newClasses.add(c); }
    }
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
    sheet.textContent += rule(selectors.filter((s) => CSS.supports('selector(' + s + ')')));
    keepSheet();
  };

  // ---- watching ----
  let lastProcedural = 0;
  let timer = 0;
  function runProcedural() {
    lastProcedural = performance.now();
    for (const p of procedural) { for (const el of select(p)) apply(p, el); }
    // (A page that changes all the time shouldn't take all the time.)
    lastCost = performance.now() - lastProcedural;
  }
  let lastCost = 0;
  function tick() {
    timer = 0;
    keepSheet();
    ask();
    if (procedural.length) runProcedural();
  }
  function soon() {
    if (timer) return;
    timer = setTimeout(tick, Math.max(100, Math.min(2000, lastCost * 20)));
  }
  function surveyAll() {
    if (!plan.generichide) for (const el of document.querySelectorAll('[id],[class]')) note(el);
    soon();
  }
  function watch() {
    if (!document.documentElement) return false;
    keepSheet();
    new MutationObserver((records) => {
      for (const r of records) {
        if (r.type === 'attributes') { if (!plan.generichide) note(r.target); continue; }
        for (const n of r.addedNodes) {
          if (n.nodeType !== 1) continue;
          if (!plan.generichide) {
            note(n);
            if (n.firstElementChild) for (const el of n.querySelectorAll('[id],[class]')) note(el);
          }
        }
      }
      soon();
    }).observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ['class', 'id'] });
    surveyAll();
    return true;
  }
  if (!watch()) {
    const wait = new MutationObserver(() => { if (watch()) wait.disconnect(); });
    wait.observe(document, { childList: true });
  }
  document.addEventListener('DOMContentLoaded', surveyAll, { once: true });

  if (KITE_AB.test) KITE_AB.test({ parseProcedural, fromEngine, select });
})();
