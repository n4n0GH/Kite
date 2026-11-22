// Run with: node --test crates/kite-core/src/webext/js/trusted_types.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/trusted_types.js', 'utf8');
const install = new Function(src + '\nreturn __kiteTrustedScriptUrls;')();
const BASE = 'moz-extension://u1/';

// A content-script world as the engine makes it. `enforced`: the page requires
// Trusted Types; `policies`: whether one may be made there (false: the page
// lists the ones there may be).
function world({ enforced = false, policies = true, trustedTypes = true } = {}) {
  const log = { refusals: 0, made: [] };
  class Trusted { constructor(s) { this.s = s; } toString() { return this.s; } }
  const check = (v) => {
    if (enforced && !(v instanceof Trusted)) { log.refusals++; throw new TypeError("This document requires 'TrustedScriptURL' assignment."); }
    return String(v);
  };
  class Element {
    constructor() { this.attrs = {}; }
    setAttribute(name, value) {
      if (arguments.length < 2) throw new TypeError('2 arguments required');
      this.attrs[name.toLowerCase()] = this instanceof HTMLScriptElement && name.toLowerCase() === 'src' ? check(value) : String(value);
    }
  }
  class HTMLScriptElement extends Element {
    get src() { return this.attrs.src || ''; }
    set src(v) { this.attrs.src = check(v); }
  }
  const g = { Element, HTMLScriptElement, URL };
  if (trustedTypes) {
    g.trustedTypes = {
      createPolicy(name, rules) {
        if (!policies) throw new TypeError("Policy '" + name + "' disallowed.");
        log.made.push(name);
        return { createScriptURL: (s) => new Trusted(rules.createScriptURL(s)) };
      },
    };
  }
  return { g, log, script: () => new HTMLScriptElement(), div: () => new Element() };
}

test('a page without Trusted Types: nothing is made, everything goes as it did', () => {
  const w = world();
  install(w.g, BASE);
  const s = w.script();
  s.src = BASE + 'js/page.js';
  assert.strictEqual(s.src, BASE + 'js/page.js');
  s.setAttribute('src', 'https://cdn.example/x.js');
  assert.strictEqual(s.src, 'https://cdn.example/x.js');
  const d = w.div();
  d.setAttribute('hide_feed', true);
  assert.strictEqual(d.attrs.hide_feed, 'true');
  assert.deepStrictEqual(w.log.made, []);
});

test('where the page requires them, the add-on\'s own script still goes in', () => {
  const w = world({ enforced: true });
  install(w.g, BASE);
  const s = w.script();
  s.src = BASE + 'js/unhook-yt.js';
  assert.strictEqual(s.src, BASE + 'js/unhook-yt.js');
  assert.deepStrictEqual(w.log.made, ['content-script']);
  assert.strictEqual(w.log.refusals, 1, 'the engine was asked first');
  // The next ones go through the policy at once: one policy, no more refusals.
  const t = w.script();
  t.setAttribute('SRC', BASE + 'other.js');
  assert.strictEqual(t.src, BASE + 'other.js');
  const u = w.script();
  u.src = new URL(BASE + 'third.js');
  assert.strictEqual(u.src, BASE + 'third.js');
  assert.deepStrictEqual(w.log.made, ['content-script']);
  assert.strictEqual(w.log.refusals, 1);
});

test('nothing but the add-on\'s own files gets through', () => {
  const w = world({ enforced: true });
  install(w.g, BASE);
  const s = w.script();
  for (const url of ['https://evil.example/x.js', 'moz-extension://other/x.js', 'data:text/javascript,alert(1)', BASE.slice(0, -1) + '.evil.example/x.js']) {
    assert.throws(() => { s.src = url; }, /TrustedScriptURL/, url);
    assert.throws(() => s.setAttribute('src', url), /TrustedScriptURL/, url);
  }
  assert.strictEqual(s.src, '');
  assert.deepStrictEqual(w.log.made, [], 'and no policy is made for them');
  // Other attributes and other elements are the engine's as ever.
  s.setAttribute('id', 'unhook-yt');
  assert.strictEqual(s.attrs.id, 'unhook-yt');
  const d = w.div();
  d.setAttribute('src', 'https://evil.example/x.js');
  assert.strictEqual(d.attrs.src, 'https://evil.example/x.js');
  assert.throws(() => d.setAttribute('src'), /2 arguments/);
});

test('a page that lists its policies: the engine\'s error, and no second try', () => {
  const w = world({ enforced: true, policies: false });
  install(w.g, BASE);
  const s = w.script();
  assert.throws(() => { s.src = BASE + 'js/page.js'; }, /TrustedScriptURL/);
  assert.throws(() => { s.src = BASE + 'js/page.js'; }, /TrustedScriptURL/);
  assert.deepStrictEqual(w.log.made, []);
});

test('an engine without Trusted Types is left alone', () => {
  const w = world({ trustedTypes: false });
  const before = Object.getOwnPropertyDescriptor(w.g.HTMLScriptElement.prototype, 'src').set;
  const attr = w.g.Element.prototype.setAttribute;
  install(w.g, BASE);
  assert.strictEqual(Object.getOwnPropertyDescriptor(w.g.HTMLScriptElement.prototype, 'src').set, before);
  assert.strictEqual(w.g.Element.prototype.setAttribute, attr);
});
