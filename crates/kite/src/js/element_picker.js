// Picking an element to block, from the page's context menu (ElementPicker.qml). In the
// isolated world, so the page sees no more than a host element with nothing to read.
(function () {
  'use strict';
  if (globalThis.__kitePicker) { globalThis.__kitePicker.stop(); }

  // ---- what to call an element ----
  // Names that change between visits, or as the element is used, make rules that don't last.
  const VOLATILE_CLASS = /\d{3,}|^(?:hover|active|focus|focused|selected|open|opened|visible|hidden|show|shown|loaded|loading|is-|has-)/i;
  const VOLATILE_ID = /\d{3,}|[0-9a-f]{8,}|^:/i;
  const ATTRIBUTES = ['data-testid', 'data-ad', 'name', 'aria-label', 'alt', 'title', 'rel', 'role'];

  function stableClasses(el) {
    const list = el.getAttribute('class');
    if (!list) return [];
    return list.trim().split(/\s+/).filter((c) => c && !VOLATILE_CLASS.test(c));
  }
  function byNetworkAddress(el) {
    for (const name of ['src', 'href']) {
      const value = el.getAttribute(name);
      if (!value || value.startsWith('data:') || value.startsWith('#') || value.startsWith('javascript:')) continue;
      // Only what stays the same: not a tracking query.
      const path = value.split(/[?#]/)[0];
      if (path.length > 3) return `${el.localName}[${name}^="${CSS.escape(path).replace(/\\([:/.-])/g, '$1')}"]`;
    }
    return '';
  }
  // A selector for this element, as short as tells it apart: an id, its classes, an attribute,
  // the address it loads, or else its place under its parent.
  function selectorOf(el, depth = 0) {
    const tag = el.localName;
    const id = el.getAttribute('id');
    if (id && !VOLATILE_ID.test(id)) return '#' + CSS.escape(id);
    const classes = stableClasses(el);
    if (classes.length) return tag + classes.map((c) => '.' + CSS.escape(c)).join('');
    if (tag === 'img' || tag === 'iframe' || tag === 'a' || tag === 'video' || tag === 'embed') {
      const address = byNetworkAddress(el);
      if (address) return address;
    }
    for (const name of ATTRIBUTES) {
      const value = el.getAttribute(name);
      if (value && value.length < 80) return `${tag}[${name}="${value.replace(/["\\]/g, '\\$&')}"]`;
    }
    const parent = el.parentElement;
    if (!parent || parent === document.documentElement || depth > 3) return tag;
    let nth = 1;
    for (let s = el.previousElementSibling; s; s = s.previousElementSibling) if (s.localName === tag) nth++;
    return `${selectorOf(parent, depth + 1)} > ${tag}:nth-of-type(${nth})`;
  }

  // ---- what's being shown ----
  const host = document.createElement('div');
  host.style.cssText = 'all:initial;position:fixed;inset:0;z-index:2147483647;pointer-events:none';
  const shadow = host.attachShadow({ mode: 'closed' });
  const style = document.createElement('style');
  style.textContent = ':host{all:initial}div{position:fixed;box-sizing:border-box;background:rgba(218,68,83,.3);outline:2px solid rgb(218,68,83);outline-offset:-1px}';
  shadow.append(style);
  (document.documentElement).append(host);

  const MAX_MARKS = 200;
  let chain = [];
  let shown = [];
  let previewing = false;
  let previewed = [];
  function find(selector) {
    try { return [...document.querySelectorAll(selector)].filter((e) => e !== host); } catch (e) { return null; }
  }
  function mark() {
    for (const old of shadow.querySelectorAll('div')) old.remove();
    if (previewing) return;
    for (const el of shown.slice(0, MAX_MARKS)) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue;
      const d = document.createElement('div');
      d.style.cssText = `left:${r.left}px;top:${r.top}px;width:${r.width}px;height:${r.height}px`;
      shadow.append(d);
    }
  }
  let frame = 0;
  const later = () => { if (!frame) frame = requestAnimationFrame(() => { frame = 0; mark(); }); };
  addEventListener('scroll', later, { capture: true, passive: true });
  addEventListener('resize', later, { passive: true });

  function unpreview() {
    for (const { el, display, priority } of previewed) el.style.setProperty('display', display, priority);
    previewed = [];
  }
  function applyPreview() {
    unpreview();
    if (!previewing) return;
    previewed = shown.map((el) => ({ el, display: el.style.getPropertyValue('display'), priority: el.style.getPropertyPriority('display') }));
    for (const { el } of previewed) el.style.setProperty('display', 'none', 'important');
  }
  function show(list) {
    unpreview();
    shown = list;
    applyPreview();
    mark();
  }

  // The part of a rule that can be tried here, if it is one.
  const PROCEDURAL = /:(?:has-text|contains|-abp-|upward|nth-ancestor|matches-|min-text-length|others|xpath|if\b|if-not|remove|style)/;
  function cosmeticPart(rule) {
    const at = rule.indexOf('##');
    if (at < 0 || rule[at + 2] === '+' || rule[at + 2] === '^') return null;
    return rule.slice(at + 2).trim();
  }

  globalThis.__kitePicker = {
    // At a point in the page's coordinates: the element there and what's around it, inmost first.
    start(x, y) {
      let el = document.elementFromPoint(x, y);
      if (!el) return JSON.stringify({ candidates: [] });
      chain = [];
      for (; el && el !== document.documentElement && el !== document.body; el = el.parentElement) chain.push(el);
      const candidates = chain.map((e) => {
        const selector = selectorOf(e);
        const found = find(selector);
        return { selector, label: e.localName, count: found ? found.length : 0 };
      });
      if (chain.length) show([chain[0]]);
      const r = chain.length ? chain[0].getBoundingClientRect() : null;
      return JSON.stringify({
        host: location.hostname.replace(/^www\./, ''),
        address: location.origin + location.pathname,
        candidates,
        right: r ? r.left + r.width / 2 > innerWidth / 2 : false,
      });
    },
    // What a rule as written would do here: {state: "matches" | "other" | "invalid", count}.
    test(rule) {
      const text = rule.trim();
      if (!text) return JSON.stringify({ state: 'invalid', count: 0 });
      const selector = cosmeticPart(text);
      if (selector === null) { show([]); return JSON.stringify({ state: 'other', count: 0 }); }
      if (!selector) return JSON.stringify({ state: 'invalid', count: 0 });
      const found = find(selector);
      if (found) { show(found); return JSON.stringify({ state: 'matches', count: found.length }); }
      show([]);
      return JSON.stringify({ state: PROCEDURAL.test(selector) ? 'other' : 'invalid', count: 0 });
    },
    preview(on) {
      previewing = on;
      applyPreview();
      mark();
    },
    // The rule was added: it applies from the next load, and right now here.
    block(rule) {
      const selector = cosmeticPart(rule);
      const found = selector === null ? [] : (find(selector) || []);
      previewing = false;
      unpreview();
      for (const el of found) el.style.setProperty('display', 'none', 'important');
      this.stop();
      return found.length;
    },
    stop() {
      previewing = false;
      unpreview();
      removeEventListener('scroll', later, { capture: true });
      removeEventListener('resize', later);
      cancelAnimationFrame(frame);
      host.remove();
      delete globalThis.__kitePicker;
    },
  };
  if (globalThis.__kitePickerTest) globalThis.__kitePickerTest({ selectorOf, stableClasses, cosmeticPart });
})();
